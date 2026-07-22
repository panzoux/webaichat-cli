use crate::bridge_client::BridgeClient;
use crate::provider::{Provider, ProviderError};
use crate::transport::Event;
use async_trait::async_trait;

pub struct ChatGptProvider;

impl ChatGptProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Provider for ChatGptProvider {
    fn name(&self) -> &str {
        "chatgpt"
    }

    async fn send(
        &self,
        bridge: &mut BridgeClient,
        message: &str,
        mut on_chunk: Box<dyn FnMut(String) + Send>,
    ) -> Result<(), ProviderError> {
        bridge.send_event(&Event::SendMessage {
            provider: self.name().to_string(),
            message: message.to_string(),
        }).await.map_err(|e| ProviderError::Connection(e.to_string()))?;

        while let Some(event) = bridge.receive_event().await
            .map_err(|e| ProviderError::Connection(e.to_string()))? 
        {
            match event {
                Event::MessageStart { provider, .. } if provider == self.name() => {
                    // Message started
                }
                Event::MessageChunk { provider, content, .. } if provider == self.name() => {
                    on_chunk(content);
                }
                Event::MessageEnd { provider, .. } if provider == self.name() => {
                    break;
                }
                Event::Error { provider, message } if provider == self.name() => {
                    return Err(ProviderError::Message(message));
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn cancel(
        &self,
        bridge: &mut BridgeClient,
        message_id: &str,
    ) -> Result<(), ProviderError> {
        bridge.send_event(&Event::Cancel {
            provider: self.name().to_string(),
            message_id: message_id.to_string(),
        }).await.map_err(|e| ProviderError::Connection(e.to_string()))?;

        Ok(())
    }
}
