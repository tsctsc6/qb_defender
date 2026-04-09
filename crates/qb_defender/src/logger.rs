use std::sync::OnceLock;

use thiserror::Error;
use time::OffsetDateTime;
use time::macros::format_description;
use tracing::Event;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

#[derive(Error, Debug)]
pub enum Error {
    #[error("Rolling file error:\n{0}")]
    RollingFileError(#[from] tracing_appender::rolling::InitError),
    #[error("Logger initialization error:\n{0}")]
    LoggerInitializationError(String),
}

/// Initializes the logger with both console and file outputs, using a rolling file appender.
pub fn init_logger(verbose: u8) -> Result<(), Error> {
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_suffix("log")
        .max_log_files(7)
        .build("logs")?;

    let (non_blocking_appender, guard) = tracing_appender::non_blocking(file_appender);

    let console_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .event_format(MyCustomFormatter {
            display_colors: true,
        });

    let file_layer = fmt::layer()
        .with_writer(non_blocking_appender)
        .event_format(MyCustomFormatter {
            display_colors: false,
        });

    let env_filter = EnvFilter::new(get_log_level(verbose));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    LOG_GUARD
        .set(guard)
        .map_err(|_| Error::LoggerInitializationError("Failed to set log guard".to_string()))?;
    Ok(())
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

struct MyCustomFormatter {
    display_colors: bool,
}

impl<S, N> FormatEvent<S, N> for MyCustomFormatter
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> Result<(), std::fmt::Error> {
        // time
        let now = OffsetDateTime::now_utc();
        let format = format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]Z"
        );
        let time_str = now
            .format(&format)
            .unwrap_or_else(|_| "unknown time".to_string());

        // metadata
        let meta = event.metadata();
        let level = meta.level();
        let target = meta.target();

        // header with optional colors
        let reset_code = "\x1b[0m";
        let gray_code = "\x1b[90m";
        if self.display_colors {
            let color_code = match *level {
                tracing::Level::ERROR => "\x1b[31m", // red
                tracing::Level::WARN => "\x1b[33m",  // yellow
                tracing::Level::INFO => "\x1b[32m",  // green
                tracing::Level::DEBUG => "\x1b[34m", // blue
                tracing::Level::TRACE => "\x1b[90m", // gray
            };
            write!(
                writer,
                "{gray_code}[{time_str} {color_code}{level:>5} {gray_code}{target}]{reset_code} "
            )?;
        } else {
            write!(writer, "[{time_str} {level:>5} {target}] ")?;
        }

        // message and fields
        ctx.field_format().format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}
