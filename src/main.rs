extern crate clap;

use clap::{App, Arg, SubCommand};

use std::io;
use tokio::io::Interest;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::io::{AsyncRead, AsyncWrite};


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
        tokio::io::copy(&mut dest_read, &mut src_write).await;
    });

    let src_to_dest = tokio::spawn(async move {
        tokio::io::copy(&mut src_read, &mut dest_write).await;
        
    });

    dest_to_src.await.unwrap();
    src_to_dest.await.unwrap();

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
