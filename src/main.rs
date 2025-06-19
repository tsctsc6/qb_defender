mod qb_sdk;
mod command;
mod log;

use clap::Parser;

#[tokio::main]
async fn main() {
    match run().await{
        Ok(_) => return,
        Err(e) => {
            log::log(e.as_str());
        }
    }
}

async fn run() -> Result<(), String> {
    let cli = command::Cli::parse();
    let mut qb_client = qb_sdk::QbClient::new(cli);
    qb_client.ensure_api_version().await?;
    qb_client.reset_banned_IPs().await?;
    loop {
        qb_client.try_reset_banned_IPs().await?;
        qb_client.record_peers().await?;
        qb_client.wait().await;
    }
}