use clap::Parser;

use log::error;
use qb_sdk;

mod command;
mod logger;

#[tokio::main]
async fn main() -> Result<(), i32> {
    let cli = command::Cli::parse();
    let _guard = match logger::init_logger(cli.verbose) {
        Ok(guard) => guard,
        Err(e) => {
            error!("{}", e);
            return Err(1);
        }
    };
    match run().await {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("Error: {}", e);
            Err(1)
        }
    }
}

async fn run() -> Result<(), String> {
    let cli = command::Cli::parse();
    let mut qb_client = qb_sdk::QbClient::new(cli.port, cli.interval);
    qb_client.ensure_api_version().await?;
    loop {
        qb_client.try_reset_banned_IPs().await?;
        qb_client.record_and_ban_peers().await?;
        qb_client.wait().await;
    }
}
