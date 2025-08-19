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
    // Setup tracing to send logs into a channel consumed by the TUI
    let (log_tx, log_rx) = mpsc::unbounded_channel::<String>();

    // Custom writer that forwards logs to the channel (line-buffered)
    struct LogWriter {
        tx: mpsc::UnboundedSender<String>,
        buf: Vec<u8>,
    }
    impl Clone for LogWriter {
        fn clone(&self) -> Self {
            LogWriter { tx: self.tx.clone(), buf: Vec::new() }
        }
    }
    impl std::io::Write for LogWriter {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.buf.extend_from_slice(data);
            let mut last_idx = 0usize;
            for i in 0..self.buf.len() {
                if self.buf[i] == b'\n' {
                    let line = String::from_utf8_lossy(&self.buf[last_idx..i]).to_string();
                    let _ = self.tx.send(line);
                    last_idx = i + 1;
                }
            }
            if last_idx > 0 {
                self.buf.drain(0..last_idx);
            }
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            if !self.buf.is_empty() {
                let line = String::from_utf8_lossy(&self.buf).to_string();
                let _ = self.tx.send(line);
                self.buf.clear();
            }
            Ok(())
        }
    }

    // Initialize tracing subscriber with our writer
    let make_writer = move || LogWriter { tx: log_tx.clone(), buf: Vec::new() };
    let _ = tracing_subscriber::fmt()
        .with_writer(make_writer)
        .with_ansi(false)
        .try_init();

    let args = Cli::parse();

    match args.command {
        Action::Local(local) => handle_local(local, log_rx).await?,
        Action::Aggregator(aggregator) => handle_aggregator(aggregator, log_rx).await?,
        Action::Proxy(proxy) => handle_proxy(proxy).await?,
    }

    Ok(())
}

async fn handle_local(args: Local, mut log_rx: mpsc::UnboundedReceiver<String>) -> Result<(), ServerError> {
    // channel to forward all messages to TUI
    let (tx, rx) = mpsc::unbounded_channel::<Message>();
    // spawn server socket in background
    let socket_path = args.socket.clone();
    let force = args.force;
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let _ = server_unix_socket(&socket_path, force, tx_clone).await;
    });

    // run TUI in foreground; pressing 'q' will return
    if let Err(err) = interface::run_tui(rx, None, Some(log_rx)).await {
        error!(?err, "TUI exited with error");
    }

    // When TUI exits, end the application (background task will be aborted as runtime shuts down)
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

async fn handle_aggregator(args: Aggregator, mut log_rx: mpsc::UnboundedReceiver<String>) -> Result<(), ServerError> {
    // channel to forward all messages to TUI
    let (tx, rx) = mpsc::unbounded_channel::<Message>();
    // control channel for backend polling management
    let (ctrl_tx, ctrl_rx) = mpsc::unbounded_channel::<tcp_server::PollControl>();

    // Spawn backend polling in background
    let backends_for_poll = args.backends.clone();
    let tx_for_poll = tx.clone();
    tokio::spawn(async move {
        let _ = tcp_server::poll_backends(backends_for_poll, tx_for_poll, ctrl_rx).await;
    });

    // Prepare aggregator tab config for TUI (pass the only sender to TUI)
    let tui_cfg = interface::AggregatorTabConfig { backends: args.backends.clone(), control_tx: ctrl_tx };

    // Run TUI in foreground
    if let Err(err) = interface::run_tui(rx, Some(tui_cfg), Some(log_rx)).await {
        error!(?err, "TUI exited with error");
    }

    // When TUI exits, sender is dropped, control channel closes, and background task will stop; end application
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
