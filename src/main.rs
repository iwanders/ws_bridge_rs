extern crate clap;
use clap::{App, Arg, SubCommand};

use std::io;
use tokio::io::Interest;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::AsyncWriteExt;

use crate::tokio::io::AsyncReadExt;
extern crate tokio;

type Error = Box<dyn std::error::Error>;



async fn tcp_to_ws(bind_location: &str, dest_location: &str)  -> Result<(), Error>{
    let listener = TcpListener::bind(bind_location).await.unwrap();

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        handle_tcp_connection(socket, dest_location).await;
    }
}


async fn handle_tcp_connection(mut src_stream: TcpStream, dest_location: &str) -> Result<(), Error> {
    
    let mut dest_stream = TcpStream::connect(dest_location).await?;
    let (mut dest_read, mut dest_write) = dest_stream.into_split();
    let (mut src_read, mut src_write) = src_stream.into_split();

    // Now, we make two tasks.
    // dest_read -> src_write
    // src_read ->  dest_write


    // Spawn two tasks, one gets a key, the other sets a key
    let dest_to_src = tokio::spawn(async move {
        loop
        {
            let mut buffer = [0; 4096];
            let ready = dest_read.ready(Interest::READABLE).await;
            let unwrap_ready;
            match ready {
                Ok(ready) => {
                    unwrap_ready = ready;
                },
                _ => {
                    break;
                }
            }

            let write_ready = src_write.ready(Interest::WRITABLE).await;
            let unwrap_write_ready;
            match write_ready {
                Ok(ready) => {
                    unwrap_write_ready = ready;
                },
                _ => {
                    break;
                }
            }

            if unwrap_ready.is_readable() && unwrap_write_ready.is_writable() {
                let n = dest_read.try_read(&mut buffer[..]);
                        println!("The bytes: {:?}",n);
                match n {
                    Ok(0) => {break;},
                    Ok(n) => {
                        println!("The bytes: {:?}", &buffer[..n]);
                        src_write.write_all(&buffer[..n]).await;
                    },
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(_) => {
                        println!("Something is wrong: {:?}", n);
                        break;
                    }
                }
            }
        }
    });

    let src_to_dest = tokio::spawn(async move {
        loop
        {
            let mut buffer = [0; 4096];
            let ready = src_read.ready(Interest::READABLE).await;
            let unwrap_ready;
            match ready {
                Ok(ready) => {
                    unwrap_ready = ready;
                },
                _ => {
                    break;
                }
            }

            let write_ready = dest_write.ready(Interest::WRITABLE).await;
            let unwrap_write_ready;
            match write_ready {
                Ok(ready) => {
                    unwrap_write_ready = ready;
                },
                _ => {
                    break;
                }
            }

            if unwrap_ready.is_readable() && unwrap_write_ready.is_writable() {
                let n = src_read.try_read(&mut buffer[..]);
                        println!("The bytes: {:?}",n);
                match n {
                    Ok(0) => {break;},
                    Ok(n) => {
                        println!("The bytes: {:?}", &buffer[..n]);
                        dest_write.write_all(&buffer[..n]).await;
                    },
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(_) => {
                        println!("Something is wrong: {:?}", n);
                        break;
                    }
                }
            }
        }
    });

    dest_to_src.await.unwrap();
    src_to_dest.await.unwrap();

    /*
    loop {
        let ready = stream.ready(Interest::READABLE | Interest::WRITABLE).await?;
        // let dest_ready = dest_stream.ready(Interest::READABLE | Interest::WRITABLE).await?;

        // From stream to destination stream.
        if ready.is_readable() {
            let mut data = vec![0; 1024];
            // Try to read data, this may still fail with `WouldBlock`
            // if the readiness event is a false positive.
            match stream.try_read(&mut data) {
                Ok(0) => {
                    dest_stream.shutdown().await?;
                    break
                },
                Ok(n) => {
                    println!("read {} bytes", n);        
                    dest_stream.write_all(&data).await?;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }

        // From destination stream to stream.
        if ready.is_writable() {
            // Try to write data, this may still fail with `WouldBlock`
            // if the readiness event is a false positive.
            if (dest_ready.is_readable())
            {
                let mut data = vec![0; 1024];
                match dest_stream.try_read(&mut data) {
                    Ok(0) => {
                        stream.shutdown().await?;
                        break
                    },
                    Ok(n) => {
                        println!("read {} bytes", n);        
                        stream.write_all(&data).await?;
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        return Err(e.into());
                    }
                }
            }

        }
    }
    */
    Ok(())
}


fn main()  -> Result<(), Error> {
    let mut app = App::new("Websocket Bridge").setting(clap::AppSettings::SubcommandRequiredElseHelp)
        .about("Allows bridging a TCP connection over a websocket..")
        .arg(
            Arg::with_name("d")
                .short("d")
                .help("Debug mode."),
        )
        .subcommand(
            SubCommand::with_name("ws_to_tcp")
                .about("Sets up a websocket server redirects binary data to a tcp connection.")
                .arg(
                    Arg::with_name("bind")
                        .takes_value(true)
                        .required(true)
                        .help("ip:port to bind to."),
                )
        ).subcommand(
            SubCommand::with_name("tcp_to_ws")
                .about("Sets up a tcp server that redirects data to a websocket connection.")
                .arg(
                    Arg::with_name("bind")
                        .takes_value(true)
                        .required(true)
                        .help("ip:port to bind to."),
                )
                .arg(
                    Arg::with_name("dest")
                        .takes_value(true)
                        .required(true)
                        .help("ip:port to send to."),
                )
        );

    let matches = app.clone().get_matches();

    // Abort with the help if no subcommand is given.
    match matches.subcommand() {
        (_something, Some(_subcmd)) => {},
        _ => {
            app.print_help()?;
            println!();
            return Err(Box::new(clap::Error::with_description(
                "No subcommand given",
                clap::ErrorKind::MissingSubcommand,
            )));
        }
    };


    if let Some(_matches) = matches.subcommand_matches("ws_to_tcp") {
        println!("Running ws to tcp.");
    }

    if let Some(sub_matches) = matches.subcommand_matches("tcp_to_ws") {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let bind_value = sub_matches.value_of("bind").ok_or(clap::Error::with_description(
            "Couldn't find bind value.",
            clap::ErrorKind::EmptyValue,
        ))?;
        let dest_value = sub_matches.value_of("dest").ok_or(clap::Error::with_description(
            "Couldn't find dest value.",
            clap::ErrorKind::EmptyValue,
        ))?;
        println!("Running tcp to ws on {:?}", bind_value);

        rt.block_on(async {
            let res = tcp_to_ws(bind_value, dest_value).await;
            println!("{:?}", res);
        })
    }

    Ok(())
}
