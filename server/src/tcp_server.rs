use crate::error::ServerError;
use crate::interface::BackendIdentifier;
use protocol::Message;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::join;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, UnboundedReceiver, UnboundedSender};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

const BUFFER_SIZE: usize = 200;

type CircularBuffer = Arc<RwLock<circular_buffer::CircularBuffer<BUFFER_SIZE, Message>>>;

pub async fn start(
    addr: &str,
    port: u16,
    rx: UnboundedReceiver<Message>,
) -> Result<(), ServerError> {
    let buffer = circular_buffer::CircularBuffer::<BUFFER_SIZE, Message>::new();
    let buffer = Arc::new(RwLock::new(buffer));

    tokio::spawn(handle_message(rx, buffer.clone()));

    let listener = tokio::net::TcpListener::bind((addr, port)).await?;
    debug!("Listening on port {addr}{port}");

    loop {
        let socket = listener.accept().await;
        match socket {
            Ok((socket, _)) => {
                tokio::spawn(handle_connection(socket, buffer.clone()));
            }
            Err(err) => {
                error!("Error accepting socket: {}", err);
            }
        }
    }
}

async fn handle_message(mut rx: UnboundedReceiver<Message>, buffer: CircularBuffer) {
    while let Some(msg) = rx.recv().await {
        buffer.write().await.push_back(msg);
    }
}

async fn handle_connection(mut stream: TcpStream, buffer: CircularBuffer) {
    let mut i = 0;
    loop {
        let buffer = buffer.read().await;
        let message = buffer.get(i);

        i += 1;

        if i >= BUFFER_SIZE {
            i = 0;
        }

        if let Some(message) = message {
            let buffer = message.encode().expect("Failed to encode message");
            if let Err(e) = stream.write_all(&buffer).await {
                debug!("Closed stream unable to write");
                break;
            }
            if let Err(e) = stream.flush().await {
                debug!("Closed stream unable to flush");
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    warn!("Closed stream");
}

pub async fn poll_backends(
    backends: Vec<(String, u16)>,
    tx: UnboundedSender<Message>,
    mut control_rx: UnboundedReceiver<PollControl>,
) -> Result<(), ServerError> {
    use std::collections::HashMap;
    let paused = std::sync::Arc::new(tokio::sync::RwLock::new(false));

    // Map of backend -> (handle, shutdown_flag)
    let mut tasks: HashMap<BackendIdentifier, (tokio::task::JoinHandle<()>, Arc<RwLock<bool>>)> =
        HashMap::new();

    // helper to spawn a backend task
    let mut make_backend_task = |addr: String, port: u16| {
        let tx_clone = tx.clone();
        let paused_flag = paused.clone();
        let shutdown = std::sync::Arc::new(tokio::sync::RwLock::new(false));
        let shutdown_clone = shutdown.clone();
        let addr_clone = addr.clone();
        let handle = tokio::spawn(async move {
            let mut backoff_secs: u64 = 1;
            let max_backoff: u64 = 30;
            loop {
                if *shutdown_clone.read().await {
                    break;
                }
                match tokio::net::TcpStream::connect(format!("{}:{}", addr_clone, port)).await {
                    Ok(mut stream) => {
                        info!(backend = %format!("{}:{}", addr_clone, port), "Connected to backend");
                        backoff_secs = 1;
                        if let Err(err) =
                            poll_backend(&mut stream, tx_clone.clone(), paused_flag.clone()).await
                        {
                            warn!(?err, backend = %format!("{}:{}", addr_clone, port), "Polling error, will reconnect");
                        } else {
                            warn!(backend = %format!("{}:{}", addr_clone, port), "Backend disconnected, will reconnect");
                        }
                    }
                    Err(err) => {
                        warn!(?err, backend = %format!("{}:{}", addr_clone, port), "Failed to connect to backend");
                    }
                }
                if *shutdown_clone.read().await {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                backoff_secs = std::cmp::min(backoff_secs.saturating_mul(2), max_backoff);
            }
            info!(backend = %format!("{}:{}", addr_clone, port), "Backend task stopped");
        });
        ((addr, port), (handle, shutdown))
    };

    // spawn initial backends
    for (addr, port) in backends.into_iter() {
        let (k, v) = make_backend_task(addr, port);
        tasks.insert(k, v);
    }

    // control loop
    while let Some(cmd) = control_rx.recv().await {
        match cmd {
            PollControl::Pause => {
                *paused.write().await = true;
                info!("Polling paused by user");
            }
            PollControl::Resume => {
                *paused.write().await = false;
                info!("Polling resumed by user");
            }
            PollControl::AddBackend((addr, port)) => {
                if !tasks.contains_key(&(addr.clone(), port)) {
                    info!(backend = %format!("{}:{}", addr, port), "Adding backend");
                    let (k, v) = make_backend_task(addr, port);
                    tasks.insert(k, v);
                } else {
                    debug!("Backend already exists");
                }
            }
            PollControl::RemoveBackend((addr, port)) => {
                if let Some((handle, shutdown)) = tasks.remove(&(addr.clone(), port)) {
                    info!(backend = %format!("{}:{}", addr, port), "Removing backend");
                    *shutdown.write().await = true;
                    // Give the task a chance to stop gracefully
                    let _ = handle.await;
                } else {
                    debug!("Backend not found to remove");
                }
            }
        }
    }

    // channel closed, stop all tasks gracefully
    for ((_addr, _port), (handle, shutdown)) in tasks.into_iter() {
        *shutdown.write().await = true;
        let _ = handle.await;
    }

    Ok(())
}

async fn poll_backend(
    backend: &mut TcpStream,
    tx: UnboundedSender<Message>,
    paused: std::sync::Arc<tokio::sync::RwLock<bool>>,
) -> Result<(), ServerError> {
    let mut read_buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 4096];

    info!("Polling backend");

    loop {
        // If paused, wait and continue without reading
        if *paused.read().await {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            continue;
        }
        match backend.read(&mut tmp).await {
            Ok(0) => {
                debug!("Backend connection closed");
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
                            warn!(?err, "Decode error from backend, clearing buffer to resync");
                            read_buf.clear();
                            break;
                        }
                    }
                }
            }
            Err(error) => {
                warn!(?error, "Error reading from backend socket");
                if error.kind() == std::io::ErrorKind::WouldBlock {
                    continue;
                }
                if error.kind() == std::io::ErrorKind::UnexpectedEof {
                    break;
                }
                return Err(error.into());
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub enum PollControl {
    Pause,
    Resume,
    AddBackend(BackendIdentifier),
    RemoveBackend(BackendIdentifier),
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_buffer() {
        let mut buffer = circular_buffer::CircularBuffer::<2, i32>::new();

        buffer.push_back(1);
        buffer.push_back(2);
        buffer.push_back(3);

        dbg!(buffer.get(0));
        dbg!(buffer.get(1));
        dbg!(buffer.get(2));
        dbg!(buffer.get(3));
    }
}
