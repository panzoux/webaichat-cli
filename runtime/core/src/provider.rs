use async_trait::async_trait;
use crate::bridge_client::BridgeClient;

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Provider not available: {0}")]
    NotAvailable(String),
    
    #[error("Message error: {0}")]
    Message(String),
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    
    async fn send(
        &self,
        bridge: &mut BridgeClient,
        message: &str,
        on_chunk: Box<dyn FnMut(String) + Send>,
    ) -> Result<(), ProviderError>;
    
    async fn cancel(
        &self,
        bridge: &mut BridgeClient,
        message_id: &str,
    ) -> Result<(), ProviderError>;
}
