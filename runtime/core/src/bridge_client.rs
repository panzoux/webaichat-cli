use crate::transport::Event;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

pub struct BridgeClient {
    ws: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl BridgeClient {
    pub fn new(_url: &str) -> Self {
        Self { ws: None }
    }

    pub async fn connect(&mut self, url: &str) -> Result<()> {
        let (ws_stream, _) = connect_async(url).await?;
        self.ws = Some(ws_stream);
        Ok(())
    }

    pub async fn send_event(&mut self, event: &Event) -> Result<()> {
        if let Some(ws) = &mut self.ws {
            let json = serde_json::to_string(event)?;
            ws.send(Message::Text(json)).await?;
        }
        Ok(())
    }

    pub async fn receive_event(&mut self) -> Result<Option<Event>> {
        if let Some(ws) = &mut self.ws {
            while let Some(msg) = ws.next().await {
                match msg? {
                    Message::Text(text) => {
                        let event: Event = serde_json::from_str(&text)?;
                        return Ok(Some(event));
                    }
                    Message::Close(_) => return Ok(None),
                    _ => {}
                }
            }
        }
        Ok(None)
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(mut ws) = self.ws.take() {
            // Send close frame
            let _ = ws.send(Message::Close(None)).await;
            let _ = ws.flush().await;

            // Wait for close response or timeout
            let _ = tokio::time::timeout(
                tokio::time::Duration::from_millis(500),
                async {
                    while let Some(msg) = ws.next().await {
                        if matches!(msg, Ok(Message::Close(_)) | Err(_)) {
                            break;
                        }
                    }
                }
            ).await;

            // Properly close the underlying TCP connection
            let _ = ws.close(None).await;
        }
        Ok(())
    }
}
