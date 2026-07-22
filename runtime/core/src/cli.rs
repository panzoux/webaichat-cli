use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "web-llm-runtime")]
#[command(about = "Browser-backed multi-provider LLM runtime")]
pub struct Cli {
    /// Browser Bridge WebSocket URL
    #[arg(long, default_value = "ws://127.0.0.1:9527")]
    pub bridge_url: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Send a message to an AI provider
    Send {
        /// Provider name (chatgpt, gemini, etc.)
        #[arg(short, long)]
        provider: String,
        
        /// Message to send
        #[arg(short, long)]
        message: String,
    },
    
    /// List available providers
    ListProviders,
}
