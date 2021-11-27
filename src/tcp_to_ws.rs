use tokio::net::TcpListener;
type Error = Box<dyn std::error::Error>;
use log::{error, info};

use crate::common::{communicate, TcpOrDestination};

pub async fn tcp_to_ws(bind_location: &str, dest_location: &str) -> Result<(), Error> {
    let listener = TcpListener::bind(bind_location).await.unwrap();

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        match communicate(
            TcpOrDestination::Dest(dest_location.to_owned()),
            TcpOrDestination::Tcp(socket),
        )
        .await
        {
            Ok(v) => {
                info!("Succesfully setup connection; {:?}", v);
            }
            Err(e) => {
                error!("{:?} (dest: {})", e, dest_location);
            }
        }
    }
}
