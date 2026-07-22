use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    // Browser → Bridge
    Connect {
        provider: String,
        version: String,
    },
    
    // Bridge → Browser
    Ready {
        version: String,
    },
    
    // Runtime → Bridge → Browser
    SendMessage {
        provider: String,
        message: String,
    },
    
    // Browser → Bridge → Runtime
    MessageStart {
        provider: String,
        message_id: String,
    },
    
    MessageChunk {
        provider: String,
        message_id: String,
        index: u32,
        content: String,
    },
    
    MessageEnd {
        provider: String,
        message_id: String,
    },
    
    // Runtime → Bridge → Browser
    Cancel {
        provider: String,
        message_id: String,
    },
    
    // Browser → Bridge → Runtime
    Cancelled {
        provider: String,
        message_id: String,
    },
    
    // Any → Any
    Error {
        provider: String,
        message: String,
    },
    
    Ping {
        timestamp: u64,
    },
    
    Pong {
        timestamp: u64,
    },
}
