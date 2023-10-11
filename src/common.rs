use futures_util::StreamExt;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};

use tokio_tungstenite::tungstenite;
use tokio_tungstenite::{accept_async, connect_async, tungstenite::protocol::Message};
type Error = Box<dyn std::error::Error>;
use crate::tokio::io::AsyncWriteExt;
use futures_util::SinkExt;
use log::{debug, error, info, trace, warn};

pub enum TcpOrDestination {
    Tcp(TcpStream),
    Dest(String),
}

pub async fn communicate(tcp_in: TcpOrDestination, ws_in: TcpOrDestination) -> Result<(), Error> {
    let mut ws;
    let tcp;

    match tcp_in {
        TcpOrDestination::Tcp(src_stream) => {
            // Convert the source stream into a websocket connection.
            ws = accept_async(tokio_tungstenite::MaybeTlsStream::Plain(src_stream)).await?;
        }
        TcpOrDestination::Dest(dest_location) => {
            let (wsz, _) = match connect_async(dest_location).await {
                Ok(v) => v,
                Err(e) => {
                    warn!("Something went wrong connecting {:?}", e);
                    if let TcpOrDestination::Tcp(mut v) = ws_in {
                        v.shutdown().await?;
                    }
                    return Err(Box::new(e));
                }
            };
            ws = wsz;
        }
    }

    match ws_in {
        TcpOrDestination::Tcp(v) => {
            tcp = v;
        }
        TcpOrDestination::Dest(dest_location) => {
            // Try to setup the tcp stream we'll be communicating to, if this fails, we close the websocket.
            tcp = match TcpStream::connect(dest_location).await {
                Ok(e) => e,
                Err(v) => {
                    let msg = tungstenite::protocol::frame::CloseFrame {
                        reason: std::borrow::Cow::Borrowed("Could not connect to destination."),
                        code: tungstenite::protocol::frame::coding::CloseCode::Error,
                    };

                    // Send the websocket close message.
                    if let Err(e) = ws.send(Message::Close(Some(msg))).await {
                        warn!("Tried to send close message, but this failed {:?}", e);
                    }

                    // Ensure we collect messages until the shutdown is actually performed.
                    let (mut _write, read) = ws.split();
                    read.for_each(|_message| async {}).await;
                    return Err(Box::new(v));
                }
            }
        }
    }

    // We got the tcp connection setup, split both streams in their read and write parts
    let (mut dest_read, mut dest_write) = tcp.into_split();
    let (mut write, mut read) = ws.split();
    let (shutdown_from_ws_tx, mut shutdown_from_ws_rx) = tokio::sync::oneshot::channel::<bool>();
    let (shutdown_from_tcp_tx, mut shutdown_from_tcp_rx) = tokio::sync::oneshot::channel::<bool>();

    // Consume from the websocket, if this loop quits, we return both things we took ownership of.
    let task_ws_to_tcp = tokio::spawn(async move {
        loop {
            tokio::select! {
                message = read.next() => {
                    if message.is_none() {
                        debug!("Got none, end of websocket stream.");
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
                    match data {
                        Message::Binary(ref x) => {
                            if dest_write.write(x).await.is_err() {
                                break;
                            };
                        }
                        Message::Text(ref x) => {
                            if dest_write.write(x.as_bytes()).await.is_err() {
                                break;
                            }
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
                },
                _shutdown_received = (&mut shutdown_from_tcp_rx ) =>
                {
                    break;
                }
            }
        }
        debug!("Reached end of consume from websocket.");
        if let Err(_) = shutdown_from_ws_tx.send(true) {
            // This happens if the shutdown happens from the other side.
            // error!("Could not send shutdown signal: {:?}", v);
        }
        (dest_write, read)
    });

    // Consume from the tcp socket and write on the websocket.
    let task_tcp_to_ws = tokio::spawn(async move {
        let mut need_close = true;
        let mut buf = vec![0; 1024 * 1024];
        loop {
            tokio::select! {
                res = dest_read.read(&mut buf) => {
                    // Return value of `Ok(0)` signifies that the remote has closed, if this happens
                    // we want to initiate shutting down the websocket.
                    match res {
                        Ok(0) => {
                            debug!("Remote tcp socket has closed, sending close message on websocket.");
                            break;
                        }
                        Ok(n) => {
                            let res = buf[..n].to_vec();
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
                },
                _shutdown_received = (&mut shutdown_from_ws_rx ) =>
                {
                    need_close = false;
                    break;
                }
            }
        }

        // Send the websocket close message.
        if need_close {
            if let Err(v) = write.send(Message::Close(None)).await {
                error!("Failed to send close message to websocket: {:?}", v);
            }
        }
        if let Err(_) = shutdown_from_tcp_tx.send(true) {
            // This happens if the shutdown happens from the other side.
            // error!("Could not send shutdown signal: {:?}", v);
        }
        debug!("Reached end of consume from tcp.");
        (dest_read, write, need_close)
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
        let (dest_read, write, ws_need_close) = r_tcp_to_ws.unwrap();

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

        if ws_need_close {
            let mut ws_stream = write.reunite(read).unwrap();
            if let Err(ref v) = ws_stream.get_mut().shutdown().await {
                error!(
                    "Error properly closing the ws from {:?}: {:?}",
                    "something", v
                );
                return;
            }
        }
        debug!("Properly closed connections.");
    });

    Ok(())
}

pub enum Direction {
    WsToTcp,
    TcpToWs,
}

pub async fn serve(bind_location: &str, dest_location: &str, dir: Direction) -> Result<(), Error> {
    let listener = TcpListener::bind(bind_location)
        .await
        .expect("Could not bind to port");
    info!("Succesfully bound to {:?}", bind_location);

    loop {
        let in1 = match dir {
            Direction::WsToTcp => {
                let (socket, _) = listener
                    .accept()
                    .await
                    .expect("Could not accept connection?");

                info!(
                    "Accepting ws connection from {:?}",
                    socket.peer_addr().unwrap()
                );
                TcpOrDestination::Tcp(socket)
            }
            Direction::TcpToWs => {
                let proto_addition = if &dest_location[..2] != "ws" {
                    "ws://"
                } else {
                    ""
                };
                TcpOrDestination::Dest(proto_addition.to_owned() + dest_location)
            }
        };
        let in2 = match dir {
            Direction::WsToTcp => TcpOrDestination::Dest(dest_location.to_owned()),
            Direction::TcpToWs => {
                let (socket, _) = listener
                    .accept()
                    .await
                    .expect("Could not accept connection?");
                info!(
                    "Accepting tcp connection from {:?}",
                    socket.peer_addr().unwrap()
                );
                TcpOrDestination::Tcp(socket)
            }
        };
        match communicate(in1, in2).await {
            Ok(_v) => {
                info!("Succesfully setup communication.");
            }
            Err(e) => {
                error!(
                    "Failed to connect to server {:?} (dest: {})",
                    e, dest_location
                );
            }
        }
    }
}
