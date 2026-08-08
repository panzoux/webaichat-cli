mod cli;
mod bridge_client;
mod provider;
mod session;
mod providers;
mod transport;

use clap::Parser;
use cli::{Cli, Commands};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Send { provider, message } => {
            let bridge_url = cli.bridge_url.as_deref().unwrap_or("ws://127.0.0.1:9527");
            let mut bridge_client = bridge_client::BridgeClient::new(bridge_url);
            bridge_client.connect().await?;
            
            let provider_impl = providers::create_provider(&provider)?;
            provider_impl.send(&mut bridge_client, &message, Box::new(|chunk| {
                print!("{}", chunk);
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
