mod qb_sdk;
mod command;
mod log;

use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), String> {
    let cli = command::Cli::parse();
    let qb_client = qb_sdk::QbClient::new(cli);
    qb_client.ensure_api_version().await?;
    Ok(())
}