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

    // Increase buffer size to prevent message drops
    let (broadcast_tx, _) = broadcast::channel::<(String, Event)>(1000);

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

    // Default to "runtime" - clients that don't send Connect are treated as runtime
    let mut client_type = Some("runtime".to_string());
    let mut rx = rx;

    tracing::info!("Connection handler started for {}", addr);

    loop {
        tokio::select! {
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                        tracing::debug!("Received raw text from {}: {}", addr, text);
                        let event: Event = match serde_json::from_str(&text) {
                            Ok(ev) => ev,
                            Err(e) => {
                                tracing::error!("Error parsing JSON event from {}: {} | raw payload: {}", addr, e, text);
                                continue;
                            }
                        };
                        tracing::info!("Received event from {}: {:?}", addr, event);

                        match &event {
                            Event::Connect { provider, .. } => {
                                client_type = Some("browser".to_string());
                                tracing::info!("Browser connected for provider: {} from {}", provider, addr);
                                let ready = Event::Ready {
                                    version: "0.1.0".to_string(),
                                };
                                ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(
                                    serde_json::to_string(&ready)?,
                                )).await?;
                                tracing::info!("Sent Ready event to {}", addr);
                            }
                            Event::SendMessage { provider, message } => {
                                tracing::info!("Broadcasting SendMessage from {} for provider: {}", addr, provider);
                                let _ = tx.send(("browser".to_string(), event.clone()));
                                tracing::info!("SendMessage broadcast sent");
                            }
                            Event::Cancel { .. } => {
                                let _ = tx.send(("browser".to_string(), event.clone()));
                            }
                            Event::MessageStart { .. } | Event::MessageChunk { .. }
                            | Event::MessageEnd { .. } | Event::Cancelled { .. } => {
                                tracing::info!("Broadcasting response event from {}", addr);
                                let _ = tx.send(("runtime".to_string(), event.clone()));
                            }
                            Event::Ping { timestamp } => {
                                let pong = Event::Pong { timestamp: *timestamp };
                                ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(
                                    serde_json::to_string(&pong)?,
                                )).await?;
                            }
                            _ => {
                                tracing::debug!("Unhandled event from {}: {:?}", addr, event);
                            }
                        }
                    }
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                        tracing::info!("Connection closed from {}", addr);
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::error!("WebSocket error from {}: {}", addr, e);
                        break;
                    }
                    None => {
                        tracing::info!("Connection ended from {}", addr);
                        break;
                    }
                    _ => {}
                }
            }
            msg = rx.recv() => {
                match msg {
                    Ok((target, event)) => {
                        let client = client_type.as_deref().unwrap_or("");
                        tracing::debug!("Broadcast received: target={}, client={}", target, client);
                        if target == client {
                            tracing::info!("Sending event to {}: {:?}", addr, event);
                            let json = match serde_json::to_string(&event) {
                                Ok(j) => j,
                                Err(e) => {
                                    tracing::error!("Error serializing event to JSON: {}", e);
                                    continue;
                                }
                            };
                            if let Err(e) = ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(json)).await {
                                tracing::info!("Client {} disconnected during event send: {}", addr, e);
                                break;
                            }
                            tracing::info!("Event sent to {}", addr);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Connection {} lagged by {} messages", addr, n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("Broadcast channel closed for {}", addr);
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
