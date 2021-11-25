use futures_util::StreamExt;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};

use tokio_tungstenite::tungstenite;
use tokio_tungstenite::{accept_async, connect_async, tungstenite::protocol::Message};
type Error = Box<dyn std::error::Error>;
use crate::tokio::io::AsyncWriteExt;
use futures_util::SinkExt;
use log::{debug, error, info, trace, warn};

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
    let (mut dest_read, mut dest_write) = src_stream.into_split();
    let (mut write, mut read) = ws.split();

    // Even though this looks the exact same as

    // Consume from the websocket, if this loop quits, we return both things we took ownership of.
    let task_ws_to_tcp = tokio::spawn(async move {
        loop {
            let message = read.next().await;
            if message.is_none() {
                debug!("Got none, end of stream.");
                break;
            }
            let message = message.unwrap();
            let data = match message {
                Ok(v) => v,
                Err(p) => {
                    debug!("Err reading data {:?}", p);
                    // dest_write.shutdown().await?;
                    break;
                }
            };
            trace!("from ws {:?}, {:?}", data, dest_write);
            match data {
                Message::Binary(ref x) => {
                    if dest_write.write(x).await.is_err() {
                        break;
                    };
                }
                Message::Close(m) => {
                    trace!("Encountered close message {:?}", m);
                    // dest_write.shutdown().await?;
                    // need to somehow shut down the tcp socket here as well.
                }
                other => {
                    error!("Something unhandled on the websocket: {:?}", other);
                    // dest_write.shutdown().await?;
                }
            }
        }
        debug!("Reached end of consume from websocket.");
        (dest_write, read)
    });

    // Consume from the tcp socket and write on the websocket.
    let task_tcp_to_ws = tokio::spawn(async move {
        loop {
            let mut buf = vec![0; 1024];

            match dest_read.read(&mut buf).await {
                // Return value of `Ok(0)` signifies that the remote has closed, if this happens
                // we want to initiate shutting down the websocket.
                Ok(0) => {
                    debug!("Remote tcp socket has closed, sending close message on websocket.");
                    break;
                }
                Ok(n) => {
                    let res = buf[..n].to_vec();
                    let resr = buf[..n].to_vec();
                    trace!("tcp -> ws: {:?}", resr);
                    match write.send(Message::Binary(res)).await {
                        Ok(_) => {
                            continue;
                        }
                        Err(v) => {
                            debug!("Failed to send binary data on ws: {:?}", v);
                            break;
                        }
                    }
                }
                Err(e) => {
                    // Unexpected socket error. There isn't much we can do here so just stop
                    // processing, and close the tcp socket.
                    error!(
                        "Something unexpected happened reading from the tcp socket: {:?}",
                        e
                    );
                    break;
                }
            }
        }

        // Send the websocket close message.
        if let Err(v) = write.send(Message::Close(None)).await {
            error!("Failed to send close message to websocket.");
        }
        debug!("Reached end of consume from tcp.");
        (dest_read, write)
    });

    // Finally, the cleanup task, all it does is close down the tcp connections.
    tokio::spawn(async move {
        // Wait for both tasks to complete.
        let (r_ws_to_tcp, r_tcp_to_ws) = tokio::join!(task_ws_to_tcp, task_tcp_to_ws);
        if let Err(ref v) = r_ws_to_tcp {
            error!(
                "Error joining: {:?}, dropping connection without proper close.",
                v
            );
            return;
        }
        let (dest_write, read) = r_ws_to_tcp.unwrap();

        if let Err(ref v) = r_tcp_to_ws {
            error!(
                "Error joining: {:?}, dropping connection without proper close.",
                v
            );
            return;
        }
        let (dest_read, write) = r_tcp_to_ws.unwrap();

        // Reunite the streams, this is guaranteed to succeed as we always use the correct parts.
        let mut tcp_stream = dest_write.reunite(dest_read).unwrap();
        if let Err(ref v) = tcp_stream.shutdown().await {
            error!(
                "Error properly closing the tcp from {:?}: {:?}",
                tcp_stream.peer_addr(),
                v
            );
            return;
        }

        let mut ws_stream = write.reunite(read).unwrap();
        if let Err(ref v) = ws_stream.get_mut().shutdown().await {
            error!(
                "Error properly closing the ws from {:?}: {:?}",
                "something", v
            );
            return;
        }
        debug!("Properly closed connections.");
    });
    Ok(())
}
