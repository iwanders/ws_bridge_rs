use clap::{App, Arg};

use tokio;

use env_logger::{Builder, WriteStyle};
use log::LevelFilter;

type Error = Box<dyn std::error::Error>;

mod common;


// helpful example; https://github.com/snapview/tokio-tungstenite/issues/137

fn main() -> Result<(), Error> {
    let app = App::new("Websocket Bridge")
        .about("Allows bridging a TCP connection over a websocket.")
        .arg(
            Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Verbosity, add more for more verbose mode."),
        )
        .arg(
            Arg::with_name("mode")
            .possible_value("ws_to_tcp")
            .possible_value("tcp_to_ws")
            .takes_value(true)
            .required(true)
            .help("The direction of transfer."),
        )
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
        );

    let matches = app.clone().get_matches();

    let verbosity = matches.occurrences_of("v");
    let level = match verbosity {
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
        }
    };

    let _stylish_logger = Builder::new()
        .filter(None, level)
        .write_style(WriteStyle::Always)
        .init();

    let bind_value = matches
        .value_of("bind")
        .ok_or(clap::Error::with_description(
            "Couldn't find bind value.",
            clap::ErrorKind::EmptyValue,
        ))?;

    let dest_value = matches
        .value_of("dest")
        .ok_or(clap::Error::with_description(
            "Couldn't find dest value.",
            clap::ErrorKind::EmptyValue,
        ))?;


    let rt = tokio::runtime::Runtime::new().unwrap();

    let direction = match matches.value_of("mode").ok_or(clap::Error::with_description(
            "Couldn't find mode value.",
            clap::ErrorKind::EmptyValue,
        ))?
    {
        "ws_to_tcp" => common::Direction::WsToTcp,
        "tcp_to_ws" => common::Direction::TcpToWs,
        &_ => {panic!("Can't happen");}
    };

    rt.block_on(async {
        let res = common::serve(bind_value, dest_value, direction).await;
        println!("{:?}", res);
    });


    Ok(())
}
