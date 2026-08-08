mod api;
mod stitcher;
mod tools;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "chatto",
    about = "OpenAI-compatible HTTP API frontend for webaichat\n\nExposes your browser-connected AI providers as a local OpenAI API.\nPoint opencode, Cursor, or any OpenAI client at http://127.0.0.1:<port>",
    version
)]
struct Args {
    /// Port to listen on
    #[arg(long, short, default_value = "11434")]
    port: u16,

    /// WebSocket URL of the browser-bridge server
    #[arg(long, default_value = "ws://127.0.0.1:9527")]
    bridge_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
        .init();

    let args = Args::parse();

    let state = api::AppState {
        bridge_url: args.bridge_url.clone(),
    };

    let app = api::router(state);
    let addr = format!("127.0.0.1:{}", args.port);

    tracing::info!("╔══════════════════════════════════════════╗");
    tracing::info!("║  chatto — OpenAI-compatible API          ║");
    tracing::info!("╚══════════════════════════════════════════╝");
    tracing::info!("Listening on http://{}", addr);
    tracing::info!("Bridge URL:  {}", args.bridge_url);
    tracing::info!("");
    tracing::info!("Configure opencode:");
    tracing::info!("  OPENAI_BASE_URL=http://{}", addr);
    tracing::info!("  OPENAI_API_KEY=chatto");
    tracing::info!("");
    tracing::info!("Available models: chatgpt, gemini");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
