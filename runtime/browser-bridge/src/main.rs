mod server;
mod protocol;

use clap::Parser;
use anyhow::Result;

#[derive(Parser)]
#[command(name = "browser-bridge")]
#[command(about = "WebSocket bridge between Runtime and Browser")]
struct Cli {
    /// Port to listen on
    #[arg(short, long, default_value_t = 9527)]
    port: u16,
    
    /// Config file path
    #[arg(short, long, default_value = "bridge.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    
    tracing::info!("Starting Browser Bridge on port {}", cli.port);
    
    server::run(cli.port).await?;

    Ok(())
}
