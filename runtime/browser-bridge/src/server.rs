use crate::protocol::Event;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::accept_async;

pub async fn run(port: u16) -> Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Listening on {}", addr);

    let (broadcast_tx, _) = broadcast::channel::<(String, Event)>(100);

    loop {
        let (stream, addr) = listener.accept().await?;
        tracing::info!("New connection from {}", addr);

        let tx = broadcast_tx.clone();
        let rx = tx.subscribe();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, addr, tx, rx).await {
                tracing::error!("Error handling connection {}: {}", addr, e);
            }
        });
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    addr: SocketAddr,
    tx: broadcast::Sender<(String, Event)>,
    rx: broadcast::Receiver<(String, Event)>,
) -> Result<()> {
    let ws = accept_async(stream).await?;
    let (mut ws_sender, mut ws_receiver) = ws.split();

    let mut client_type: Option<String> = None;
    let mut rx = rx;

    loop {
        tokio::select! {
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                        let event: Event = serde_json::from_str(&text)?;
                        tracing::debug!("Received event: {:?}", event);

                        match &event {
                            Event::Connect { provider, .. } => {
                                client_type = Some("browser".to_string());
                                tracing::info!("Browser connected for provider: {}", provider);
                                let ready = Event::Ready {
                                    version: "0.1.0".to_string(),
                                };
                                ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(
                                    serde_json::to_string(&ready)?,
                                )).await?;
                            }
                            Event::SendMessage { .. } | Event::Cancel { .. } => {
                                let _ = tx.send(("browser".to_string(), event));
                            }
                            Event::MessageStart { .. } | Event::MessageChunk { .. } 
                            | Event::MessageEnd { .. } | Event::Cancelled { .. } => {
                                let _ = tx.send(("runtime".to_string(), event));
                            }
                            Event::Ping { timestamp } => {
                                let pong = Event::Pong { timestamp: *timestamp };
                                ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(
                                    serde_json::to_string(&pong)?,
                                )).await?;
                            }
                            _ => {}
                        }
                    }
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                        tracing::info!("Connection closed from {}", addr);
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::error!("WebSocket error: {}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
            msg = rx.recv() => {
                if let Ok((target, event)) = msg {
                    if target == client_type.as_deref().unwrap_or("") {
                        ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(
                            serde_json::to_string(&event)?,
                        )).await?;
                    }
                }
            }
        }
    }

    Ok(())
}
