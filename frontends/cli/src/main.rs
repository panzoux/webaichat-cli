use web_llm_runtime::cli::{Cli, Commands};
use web_llm_runtime::bridge_client::BridgeClient;
use web_llm_runtime::providers;
use clap::Parser;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Send { provider, message } => {
            let bridge_url = cli.bridge_url.as_deref().unwrap_or("ws://127.0.0.1:9527");
            let mut bridge_client = BridgeClient::new(bridge_url);
            bridge_client.connect().await?;
            
            use std::io::{self, Write};
            let provider_impl = providers::create_provider(&provider)?;
            provider_impl.send(&mut bridge_client, &message, Box::new(|chunk| {
                print!("{}", chunk);
                let _ = io::stdout().flush();
            })).await?;
            
            println!();
        }
        Commands::ListProviders => {
            let providers = providers::list_providers();
            println!("Available providers:");
            for p in providers {
                println!("  - {}", p);
            }
        }
    }

    Ok(())
}
