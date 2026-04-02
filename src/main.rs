use clap::Parser;
use qb_sdk;
use thiserror::Error;
use tracing::error;

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

    let mut application = match setup(cli.port, cli.interval).await {
        Ok(app) => app,
        Err(e) => {
            error!("{}", e);
            return Err(1);
        }
    };

    run_loop(&mut application).await;

    return Ok(());
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Application error:\n{0}")]
    ApplicationError(#[from] application::Error),
}

async fn setup(port: u16, interval: u64) -> Result<Application, Error> {
    let qb_client = qb_sdk::QbClient::new("127.0.0.1".into(), port);
    let application = Application::new(qb_client, interval);
    application
        .ensure_api_version(application.try_get_api_version().await)
        .await?;
    Ok(application)
}

async fn run(application: &mut Application) -> Result<(), Error> {
    application.try_reset_banned_IPs().await?;
    application.record_and_ban_peers().await?;
    Ok(())
}

async fn run_loop(application: &mut Application) -> () {
    loop {
        if let Err(e) = run(application).await {
            error!("{}", e);
        }
        application.wait().await;
    }
}
