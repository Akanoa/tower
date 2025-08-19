use crate::cli::{Action, Cli, Local};
use crate::error::ServerError;
use clap::Parser;
use tokio::io::AsyncReadExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::select;
use tracing::{debug, error, info, warn};

mod cli;
mod error;
mod interface;
mod tcp_server;

use tokio::sync::mpsc;

pub async fn run() -> Result<(), error::ServerError> {
    let _ = tracing_subscriber::fmt::init();
    let args = Cli::parse();

    match args.command {
        Action::Local(local) => handle_local(local).await?,
        Action::Aggregator(_) => {}
        Action::Proxy(_) => {}
    }

    Ok(())
}

async fn handle_local(args: Local) -> Result<(), error::ServerError> {
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

    // channel to forward all messages to TUI
    let (tx, rx) = mpsc::unbounded_channel::<protocol::Message>();
    // spawn TUI
    tokio::spawn(async move {
        if let Err(err) = interface::run_tui(rx).await {
            error!(?err, "TUI exited with error");
        }
    });

    loop {
        select! {
            result = server.accept() => {
                match result {
                    Ok((socket, _addr)) => {
                        // info!("new connection from {:?}", socket);
                        let tx2 = tx.clone();
                        tokio::spawn(async move {
                            let _ = handle_connection(socket, tx2).await;
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

async fn handle_connection(
    stream: UnixStream,
    tx: mpsc::UnboundedSender<protocol::Message>,
) -> Result<(), error::ServerError> {
    // info!(peer=?stream.peer_addr()?,"New connection");
    let (reader, writer) = stream.into_split();

    let mut read_buf = vec![];
    let mut tmp = [0u8; 4096];
    let mut reader = tokio::io::BufReader::new(reader);
    let mut writer = tokio::io::BufWriter::new(writer);
    loop {
        match reader.read(&mut tmp).await {
            Ok(0) => {
                debug!("Connection closed");
                break;
            }
            Ok(n) => {
                read_buf.extend_from_slice(&tmp[..n]);

                // Try to decode as many messages as available in the buffer
                loop {
                    match protocol::bincode::decode_from_slice::<protocol::Message, _>(
                        &read_buf,
                        protocol::bincode::config::standard(),
                    ) {
                        Ok((message, consumed)) => {
                            // Remove the consumed bytes
                            read_buf.drain(0..consumed);

                            // Forward all messages to the TUI task
                            let _ = tx.send(message);

                            // Continue the loop to decode next message (if any)
                            continue;
                        }
                        Err(err) => {
                            // If not enough bytes to decode a full message, wait for more data
                            if matches!(
                                err,
                                protocol::bincode::error::DecodeError::UnexpectedEnd { .. }
                            ) {
                                break;
                            }
                            // On other decode errors, log and clear buffer to resync
                            warn!(?err, "Decode error, clearing buffer to resync");
                            read_buf.clear();
                            break;
                        }
                    }
                }
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
