use crate::errors::InitLoggerError;
use clap::Parser;

use log::error;
use qb_sdk;

mod command;
mod errors;

#[tokio::main]
async fn main() -> Result<(), i32> {
    match init_logger() {
        Ok(_) => {}
        Err(e) => {
            error!("{}", e);
            return Err(1);
        }
    }
    match run().await {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("Error: {}", e);
            Err(1)
        }
    }
}

fn init_logger() -> Result<(), InitLoggerError> {
    use fern::colors::Color;
    use fern::colors::ColoredLevelConfig;
    use tracing_appender::rolling::{RollingFileAppender, Rotation};

    let colors = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::Green)
        .debug(Color::Blue)
        .trace(Color::BrightBlack);

    let console_dispatch = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{timestamp} {level} {target}] {message}",
                timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
                level = colors.color(record.level()),
                target = record.target(),
                message = message
            ))
        })
        .chain(std::io::stdout());

    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("app")
        .filename_suffix("log")
        .max_log_files(7)
        .build("logs")?;

    let file_dispatch = fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{timestamp} {level} {target}] {message}",
                timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
                level = record.level(),
                target = record.target(),
                message = message
            ))
        })
        .chain(Box::new(file_appender) as Box<dyn std::io::Write + Send>);

    fern::Dispatch::new()
        .level(log::LevelFilter::Debug)
        .chain(console_dispatch)
        .chain(file_dispatch)
        .apply()?;

    Ok(())
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
