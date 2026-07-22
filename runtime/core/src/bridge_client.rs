use crate::transport::Event;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct BridgeClient {
    url: String,
    ws: Option<Arc<Mutex<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
}

impl BridgeClient {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            ws: None,
        }
    }

    pub async fn connect(&mut self) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        self.ws = Some(Arc::new(Mutex::new(ws_stream)));
        Ok(())
    }

    pub async fn send_event(&self, event: &Event) -> Result<()> {
        if let Some(ws) = &self.ws {
            let mut ws = ws.lock().await;
            let json = serde_json::to_string(event)?;
            ws.send(Message::Text(json)).await?;
        }
        Ok(())
    }

    pub async fn receive_event(&self) -> Result<Option<Event>> {
        if let Some(ws) = &self.ws {
            let mut ws = ws.lock().await;
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
        if let Some(ws) = &mut self.ws {
            let mut ws = ws.lock().await;
            ws.close(None).await?;
        }
        self.ws = None;
        Ok(())
    }
}
