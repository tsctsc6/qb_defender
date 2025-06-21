use command;
use log;
use qb_sdk;

#[tokio::main]
async fn main() -> Result<(), i32> {
    match run().await{
        Ok(_) => Ok(()),
        Err(e) => {
            log::log(e.as_str());
            Err(1)
        }
    }
}

async fn run() -> Result<(), String> {
    let cli = command::Cli::pub_prase();
    let mut qb_client = qb_sdk::QbClient::new(cli);
    qb_client.ensure_api_version().await?;
    qb_client.reset_banned_IPs().await?;
    loop {
        qb_client.try_reset_banned_IPs().await?;
        qb_client.record_and_ban_peers().await?;
        qb_client.wait().await;
    }
}