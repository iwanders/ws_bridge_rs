use clap::{App, Arg, SubCommand};

use tokio;

use env_logger::{Builder, WriteStyle};
use log::LevelFilter;

type Error = Box<dyn std::error::Error>;

mod ws_to_tcp;
mod tcp_to_ws;

use ws_to_tcp::ws_to_tcp;
use tcp_to_ws::tcp_to_ws;

// helpful example; https://github.com/snapview/tokio-tungstenite/issues/137

fn main() -> Result<(), Error> {
    let mut app = App::new("Websocket Bridge")
        .setting(clap::AppSettings::SubcommandRequiredElseHelp)
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
                ),
        )
        .subcommand(
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
                ),
        );

    let matches = app.clone().get_matches();

    // Abort with the help if no subcommand is given.
    match matches.subcommand() {
        (_something, Some(_subcmd)) => {}
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

    let rt = tokio::runtime::Runtime::new().unwrap();

    if let Some(sub_matches) = matches.subcommand_matches("ws_to_tcp") {
        let rt = tokio::runtime::Runtime::new().unwrap();
        println!("Running ws to tcp.");
        let bind_value = sub_matches
            .value_of("bind")
            .ok_or(clap::Error::with_description(
                "Couldn't find bind value.",
                clap::ErrorKind::EmptyValue,
            ))?;
        let dest_value = sub_matches
            .value_of("dest")
            .ok_or(clap::Error::with_description(
                "Couldn't find dest value.",
                clap::ErrorKind::EmptyValue,
            ))?;

        rt.block_on(async {
            let res = ws_to_tcp(bind_value, dest_value).await;
            println!("{:?}", res);
        })
    }

    if let Some(sub_matches) = matches.subcommand_matches("tcp_to_ws") {
        let bind_value = sub_matches
            .value_of("bind")
            .ok_or(clap::Error::with_description(
                "Couldn't find bind value.",
                clap::ErrorKind::EmptyValue,
            ))?;
        let dest_value = sub_matches
            .value_of("dest")
            .ok_or(clap::Error::with_description(
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
