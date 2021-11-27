use futures_util::StreamExt;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};

use tokio_tungstenite::tungstenite;
use tokio_tungstenite::{accept_async, connect_async, tungstenite::protocol::Message};
type Error = Box<dyn std::error::Error>;
use crate::tokio::io::AsyncWriteExt;
use futures_util::SinkExt;
use log::{debug, error, info, trace, warn};

use crate::common::{communicate, TcpOrDestination};

pub async fn ws_to_tcp(bind_location: &str, dest_location: &str) -> Result<(), Error> {
    let listener = TcpListener::bind(bind_location).await.unwrap();

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        match communicate(TcpOrDestination::Tcp(socket), TcpOrDestination::Dest(dest_location.to_owned())).await {
            Ok(v) => {
                info!("Succesfully setup connection; {:?}", v);
            }
            Err(e) => {
                error!("{:?} (dest: {})", e, dest_location);
            }
        }
    }
}

