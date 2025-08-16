#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("The socket {0} already exist")]
    SocketAlreadyExist(String),
}
