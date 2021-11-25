
use futures_util::StreamExt;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};

use tokio_tungstenite::tungstenite;
use tokio_tungstenite::{accept_async, connect_async, tungstenite::protocol::Message};
type Error = Box<dyn std::error::Error>;
use log::{debug, error, info, trace, warn};
use crate::tokio::io::AsyncWriteExt;
use futures_util::SinkExt;

pub async fn tcp_to_ws(bind_location: &str, dest_location: &str) -> Result<(), Error> {
    let listener = TcpListener::bind(bind_location).await.unwrap();

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        match handle_tcp_to_ws(socket, dest_location).await {
            Ok(v) => {
                info!("Succesfully setup connection; {:?}", v);
            }
            Err(e) => {
                error!("{:?} (dest: {})", e, dest_location);
            }
        }
    }
}

async fn handle_tcp_to_ws(mut src_stream: TcpStream, dest_location: &str) -> Result<(), Error> {
    let (ws, _) = match connect_async(dest_location).await {
        Ok(v) => v,
        Err(e) => {
            warn!("Something went wrong connecting {:?}", e);
            src_stream.shutdown().await?;
            return Err(Box::new(e));
        }
    };
    let (mut tcp_src_read, mut tcp_src_write) = src_stream.into_split();

    // Split the websocket.
    let (mut write, read) = ws.split();

    let (ws_tx, mut ws_rx) = tokio::sync::broadcast::channel::<Message>(16);
    let (tcp_tx, mut tcp_rx) = tokio::sync::broadcast::channel::<Message>(16);

    // Consume from the websocket.
    tokio::spawn(async move {
        read.for_each(|message| async {
            match message
            {
                Ok(msg) => {ws_tx.send(msg);},
                Err(v) => {},
            };
        }).await;
        ws_tx.send(Message::Close(None));
        Ok::<_, tokio_tungstenite::tungstenite::Error>(())
    });

    // Worker to consume from the ws rx and put into tcp tx.
    tokio::spawn(async move {
        // let &mut z = tcp_src_write;
        loop {
            tokio::select! {
                m = ws_rx.recv() =>  {
                    match m {
                        Ok(m) => {
                            match m
                            {
                                Message::Binary(b) => {
                                    tcp_src_write.write(&b).await;
                                },
                                Message::Close(_) => {
                                    // Ah, a closing event, shut down the tcp socket.
                                    tcp_src_write.shutdown().await;
                                },
                                _ => {},
                            }
                        },
                        Err(v) => {},
                    }
                }
                m2 = tcp_rx.recv() => {
                    match m2 {
                        Ok(m2) => {
                            match m2
                            {
                                Message::Binary(b) => {
                                    match write.send(Message::Binary(b)).await
                                    {
                                        Ok(v) => {},
                                        Err(e) => {
                                            error!("Error {:?}, trying to shut down the tcp connection.", e);
                                            tcp_src_write.shutdown().await;
                                        },
                                    }
                                },
                                Message::Close(_) => {
                                    // write.send(Message::Close(None)).await;
                                    tcp_src_write.shutdown().await;
                                    return;
                                },
                                _ => {},
                            }
                        },
                        Err(v) => {},
                    }
                }
            }
        }
    });

    // Consume from the tcp socket and write to the websocket.
    tokio::spawn(async move {
        let mut buf = vec![0; 1024];
        loop {
            match tcp_src_read.read(&mut buf).await {
                // Return value of `Ok(0)` signifies that the remote has
                // closed
                Ok(0) => {
                    debug!("Tcp read returned 0, remote has closed.");
                    // Close the websocket.
                    tcp_tx.send(Message::Close(None));
                    break;
                }
                Ok(n) => {
                    trace!("from tcp: {:?}", buf[..n].to_vec());
                    tcp_tx.send(Message::Binary(buf[..n].to_vec()));
                }
                Err(e) => {
                    tcp_tx.send(Message::Close(None));
                }
            }
        }
        Ok::<_, tokio_tungstenite::tungstenite::Error>(())
    });

    Ok(())
}

