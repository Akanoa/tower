use crate::cli::Cli;
use crate::error::ServerError;
use clap::Parser;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::select;
use tracing::{debug, error, info, warn};

mod cli;
mod error;

pub async fn run() -> Result<(), error::ServerError> {
    let _ = tracing_subscriber::fmt::init();
    let args = Cli::parse();

    if std::fs::exists(&args.socket)? {
        if args.force {
            debug!("Removing previous socket file");
            std::fs::remove_file(&args.socket)?;
        } else {
            error!("Server already running, please use --force to remove previous socket file");
            return Err(ServerError::SocketAlreadyExist(args.socket));
        }
    }

    let server = UnixListener::bind(&args.socket)?;

    loop {
        select! {
            result = server.accept() => {
                match result {
                    Ok((socket, _addr)) => {
                        info!("new connection from {:?}", socket);
                        tokio::spawn(async move {
                            let _ = handle_connection(socket).await;
                        });
                    },
                    Err(error) => {
                        warn!("Error accepting socket: {}", error);
                        continue
                    }
                }
            },
            _ = tokio::signal::ctrl_c() => {
                break
            }
        }
    }

    Ok(())
}

async fn handle_connection(stream: UnixStream) -> Result<(), error::ServerError> {
    info!(peer=?stream.peer_addr()?,"New connection");
    let (reader, writer) = stream.into_split();

    let mut buffer = [0; 1024];
    let mut reader = tokio::io::BufReader::new(reader);
    let mut writer = tokio::io::BufWriter::new(writer);
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => {
                debug!("Connection closed");
                break;
            }
            Ok(n) => {
                debug!(len=n, data=?String::from_utf8_lossy(&buffer[..n]));
                let data = &buffer[..n];

                writer.write_all(b"Server: ").await?;
                writer.write_all(data).await?;
                writer.flush().await?;
            }
            Err(error) => {
                warn!(?error, "Error reading from socket");
                if error.kind() == std::io::ErrorKind::WouldBlock {
                    continue;
                }
                if error.kind() == std::io::ErrorKind::UnexpectedEof {
                    break;
                }
                return Err(error.into());
            }
        }
    }

    Ok(())
}
