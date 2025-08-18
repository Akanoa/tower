use elyze::errors::ParseError;

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("ParsingError: {0}")]
    Parse(#[from] ParseError),
    #[error("RuntimeError: {0}")]
    Execute(String),
    #[error("Executor not found: {0}")]
    NotExistingExecutor(i64),
    #[error("ProtocolError: {0}")]
    Encode(#[from] protocol::bincode::error::EncodeError),
    #[error("IOError: {0}")]
    Io(#[from] std::io::Error),
}
