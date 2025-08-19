use crate::cli::{Action, Aggregator, Cli, Local, Proxy};
use crate::error::ServerError;
use clap::Parser;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::select;
use tracing::{debug, error, info, warn};

mod cli;
mod error;
mod interface;
mod tcp_server;

use protocol::Message;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;

pub async fn run() -> Result<(), ServerError> {
    let _ = tracing_subscriber::fmt::init();
    let args = Cli::parse();

    match args.command {
        Action::Local(local) => handle_local(local).await?,
        Action::Aggregator(aggregator) => handle_aggregator(aggregator).await?,
        Action::Proxy(proxy) => handle_proxy(proxy).await?,
    }

    Ok(())
}

async fn handle_local(args: Local) -> Result<(), ServerError> {
    // channel to forward all messages to TUI
    let (tx, rx) = mpsc::unbounded_channel::<Message>();
    // spawn TUI
    tokio::spawn(async move {
        if let Err(err) = interface::run_tui(rx, None).await {
            error!(?err, "TUI exited with error");
        }
    });

    server_unix_socket(&args.socket, args.force, tx).await?;

    Ok(())
}

async fn handle_proxy(args: Proxy) -> Result<(), ServerError> {
    // channel to forward all messages to tcp server
    let (tx, rx) = mpsc::unbounded_channel::<Message>();
    // spawn tcp_server
    tokio::spawn(async move {
        if let Err(err) = tcp_server::start(&args.address, args.port, rx).await {
            error!(?err, "TUI exited with error");
        }
    });

    server_unix_socket(&args.socket, args.force, tx).await?;

    Ok(())
}

async fn handle_aggregator(args: Aggregator) -> Result<(), ServerError> {
    // channel to forward all messages to TUI
    let (tx, rx) = mpsc::unbounded_channel::<Message>();
    // control channel for backend polling management
    let (ctrl_tx, ctrl_rx) = mpsc::unbounded_channel::<tcp_server::PollControl>();

    // Prepare aggregator tab config for TUI
    let backends_copy = args.backends.clone();
    let tui_ctrl_tx = ctrl_tx.clone();

    // spawn TUI
    tokio::spawn(async move {
        let cfg = interface::AggregatorTabConfig { backends: backends_copy, control_tx: tui_ctrl_tx };
        if let Err(err) = interface::run_tui(rx, Some(cfg)).await {
            error!(?err, "TUI exited with error");
        }
    });

    // Start restartable polling jobs for each backend address
    tcp_server::poll_backends(args.backends, tx, ctrl_rx).await?;
    Ok(())
}

async fn server_unix_socket(
    socket: &str,
    force: bool,
    tx: UnboundedSender<Message>,
) -> Result<(), ServerError> {
    if std::fs::exists(socket)? {
        if force {
            debug!("Removing previous socket file");
            std::fs::remove_file(&socket)?;
        } else {
            error!("Server already running, please use --force to remove previous socket file");
            return Err(ServerError::SocketAlreadyExist(socket.to_string()));
        }
    }

    let server = UnixListener::bind(&socket)?;

    loop {
        select! {
            result = server.accept() => {
                match result {
                    Ok((socket, _addr)) => {
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            let _ = handle_connection_remote_connection(socket, tx).await;
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

async fn handle_connection_remote_connection<S: AsyncRead + Unpin>(
    mut stream: S,
    tx: UnboundedSender<Message>,
) -> Result<(), ServerError> {
    let mut read_buf = vec![];
    let mut tmp = [0u8; 4096];
    loop {
        match stream.read(&mut tmp).await {
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
