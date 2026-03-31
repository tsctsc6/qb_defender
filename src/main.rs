use clap::Parser;
use log::error;
use qb_sdk;
use thiserror::Error;

use crate::application::Application;

mod application;
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
    match run(cli.port, cli.interval).await {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("{}", e);
            Err(1)
        }
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Application error:\n{0}")]
    ApplicationError(#[from] application::Error),
}

async fn run(port: u16, interval: u64) -> Result<(), Error> {
    let qb_client = qb_sdk::QbClient::new(port);
    let mut application = Application::new(qb_client, interval);
    application.ensure_api_version().await?;
    loop {
        application.try_reset_banned_IPs().await?;
        application.record_and_ban_peers().await?;
        application.wait().await;
    }
}
