use clap::Parser;
use tracing::info;

mod cli;
mod config;
mod core;
mod extractors;
mod utils;

use cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Parse command line arguments
    let cli = Cli::parse();
    
    info!("Starting yt-dlp-ng v{}", env!("CARGO_PKG_VERSION"));
    
    // Handle the command
    cli.run().await?;
    
    Ok(())
}