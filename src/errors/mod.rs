use thiserror::Error;

#[derive(Error, Debug)]
pub enum InitLoggerError {
    #[error("Failed to initialize logger: {0}")]
    SetLoggerError(#[from] log::SetLoggerError),

    #[error("Rolling file error: {0}")]
    RollingFileError(#[from] tracing_appender::rolling::InitError),

    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
}
