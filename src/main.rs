use clap::{App, Arg, SubCommand};


use tokio::net::{TcpListener, TcpStream};
use tokio;
use tokio::io::AsyncReadExt;

use tokio_tungstenite::{connect_async, accept_async, tungstenite::protocol::Message};

use futures_util::StreamExt;

use log::{info, trace, warn, debug, error};

use env_logger::{Builder, WriteStyle};
use log::{LevelFilter};

type Error = Box<dyn std::error::Error>;

// helpful example; https://github.com/snapview/tokio-tungstenite/issues/137

async fn tcp_to_ws(bind_location: &str, dest_location: &str)  -> Result<(), Error>{
    let listener = TcpListener::bind(bind_location).await.unwrap();

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        handle_tcp_to_ws(socket, dest_location).await?;
    }
}

async fn handle_tcp_to_ws(src_stream: TcpStream, dest_location: &str) -> Result<(), Error> {

    let (mut tcp_src_read, tcp_src_write) = src_stream.into_split();
    let  (ws, _) = connect_async(dest_location).await.expect("Failed to connect");

    // Split the websocket.
    let (mut write, read) = ws.split();

    // Consume from the websocket.
    tokio::spawn(async move {
        // Need fold here to pass in the output destination; https://stackoverflow.com/a/62563511
        read.fold(tcp_src_write, |mut tcp_src_write, message| async {
                let msg = match message
                {
                    Ok(v) => v,
                    Err(p) =>
                    {
                        debug!("Err {:?}", p);
                        return tcp_src_write;
                    }
                };
                // let data = message.unwrap().into_data();
                let data = match msg
                {
                    Message::Binary(v) => v,
                    other => {
                        error!("Encountered unhandled data on websocket {:?}", other);
                        return tcp_src_write;
                    }
                };
                trace!("from ws: {:?}", data);

                // Write to the tcp socket.
                tcp_src_write.write(&data).await;
                tcp_src_write
            }).await;
    });
    

    // Consume from the tcp socket and write to the websocket.
    tokio::spawn(async move {
        let mut buf = vec![0; 1024];
        loop {
            match tcp_src_read.read(&mut buf).await {
                // Return value of `Ok(0)` signifies that the remote has
                // closed
                Ok(0) => return,
                Ok(n) => {
                    trace!("from tcp: {:?}", buf[..n].to_vec());
                    // send to the websocket.
                    write.send(Message::Binary(buf[..n].to_vec())).await.unwrap();
                }
                Err(_) => {
                    // Unexpected socket error. There isn't much we can do
                    // here so just stop processing.
                    return;
                }
            }
        }
    });

    Ok(())
}

// And the reverse direction.
async fn ws_to_tcp(bind_location: &str, dest_location: &str)  -> Result<(), Error>{
    let listener = TcpListener::bind(bind_location).await.unwrap();

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        handle_ws_to_tcp(socket, dest_location).await?;
    }
}

use futures_util::SinkExt;
use crate::tokio::io::AsyncWriteExt;
async fn handle_ws_to_tcp(src_stream: TcpStream, dest_location: &str) -> Result<(), Error> {
    
    let ws = accept_async(src_stream).await.unwrap();
    let dest_stream = TcpStream::connect(dest_location).await?;
    let (mut dest_read, dest_write) = dest_stream.into_split();

    let (mut write, read) = ws.split();


    // Consume from the websocket.
    tokio::spawn(async move {
        read.fold(dest_write, |mut dest_write, message| async {
                let data = match message
                {
                    Ok(v) => v,
                    Err(p) => 
                    {
                        debug!("Err reading data {:?}", p);
                        return dest_write;
                    }
                };
                // let data = message.unwrap();
                trace!("from ws {:?}, {:?}", data, dest_write);
                match data{
                    Message::Binary(ref x) =>
                    {
                        if dest_write.write(x).await.is_err()
                        {
                            return dest_write;
                        };
                    },
                    other =>
                    {
                        error!("Something unhandled on the websocket: {:?}", other);
                    }
                }
                dest_write
            }).await;
    });
    

    // Consume from the tcp socket and write on the websocket.
    tokio::spawn(async move {
        loop {
            let mut buf = vec![0; 1024];
            match dest_read.read(&mut buf).await {
                // Return value of `Ok(0)` signifies that the remote has
                // closed
                Ok(0) => {
                    debug!("Remote tcp socket has closed.");
                    return
                },
                Ok(n) => {
                    let res = buf[..n].to_vec();
                    let resr = buf[..n].to_vec();
                    trace!("from tcp {:?}", resr);
                    if write.send(Message::Binary(res)).await.is_err()
                    {
                        debug!("Failed to send binary on websocket.");
                        return;
                    }
                }
                Err(_) => {
                    // Unexpected socket error. There isn't much we can do
                    // here so just stop processing.
                    error!("bad");
                    return;
                }
            }
        }
    });

    Ok(())
}



fn main()  -> Result<(), Error> {
    let mut app = App::new("Websocket Bridge").setting(clap::AppSettings::SubcommandRequiredElseHelp)
        .about("Allows bridging a TCP connection over a websocket..")
        .arg(
            Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Verbosity, add more for more verbose mode."),
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
                .arg(
                    Arg::with_name("dest")
                        .takes_value(true)
                        .required(true)
                        .help("ip:port to send to."),
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

    let verbosity = matches.occurrences_of("v");
    let level = match verbosity
    {
        0 => LevelFilter::Error,
        1 => LevelFilter::Warn,
        2 => LevelFilter::Info,
        3 => LevelFilter::Debug,
        4 => LevelFilter::Trace,
        _ => {
                return Err(Box::new(clap::Error::with_description(
                    "Couldn't find dest value.",
                    clap::ErrorKind::EmptyValue,
                )));
        },
    };

    let _stylish_logger = Builder::new()
        .filter(None, level)
        .write_style(WriteStyle::Always)
        .init();

    

    let rt = tokio::runtime::Runtime::new().unwrap();

    if let Some(sub_matches) = matches.subcommand_matches("ws_to_tcp") {
        let rt = tokio::runtime::Runtime::new().unwrap();
        println!("Running ws to tcp.");
        let bind_value = sub_matches.value_of("bind").ok_or(clap::Error::with_description(
            "Couldn't find bind value.",
            clap::ErrorKind::EmptyValue,
        ))?;
        let dest_value = sub_matches.value_of("dest").ok_or(clap::Error::with_description(
            "Couldn't find dest value.",
            clap::ErrorKind::EmptyValue,
        ))?;


        rt.block_on(async {
            let res = ws_to_tcp(bind_value, dest_value).await;
            println!("{:?}", res);
        })
    }

    if let Some(sub_matches) = matches.subcommand_matches("tcp_to_ws") {

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
