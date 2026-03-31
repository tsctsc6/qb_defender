use thiserror::Error;
use time::macros::format_description;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Rolling file error: {0}")]
    RollingFileError(#[from] tracing_appender::rolling::InitError),
}

/// Initializes the logger with both console and file outputs, using a rolling file appender.
/// WorkerGuard must keep alive in main thread to ensure logs are flushed properly.
pub fn init_logger(verbose: u8) -> Result<WorkerGuard, Error> {
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_suffix("log")
        .max_log_files(7)
        .build("logs")?;

    let (non_blocking_appender, guard) = tracing_appender::non_blocking(file_appender);

    let time_format =
        format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]Z");
    let time_format = fmt::time::UtcTime::new(time_format);

    let console_layer = fmt::layer()
        .with_timer(time_format.clone())
        .with_target(true)
        .with_ansi(true) // color enabled for console
        .with_level(true);

    let file_layer = fmt::layer()
        .with_timer(time_format)
        .with_writer(non_blocking_appender)
        .with_target(true)
        .with_ansi(false) // color disabled for file
        .with_level(true);

    let env_filter = EnvFilter::new(get_log_level(verbose));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    Ok(guard)
}

fn get_log_level(verbose: u8) -> &'static str {
    match verbose {
        0 => "off",
        1 => "error",
        2 => "warn",
        3 => "info",
        4 => "debug",
        _ => "trace",
    }
}
