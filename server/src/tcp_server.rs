use crate::error::ServerError;
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
    backends: Vec<TcpStream>,
    tx: UnboundedSender<Message>,
) -> Result<(), ServerError> {
    let mut handles = vec![];
    for mut backend in backends.into_iter() {
        let tx_clone = tx.clone();
        // Spawn a task per backend to continuously read and forward messages
        let handle = tokio::spawn(async move {
            let _ = poll_backend(&mut backend, tx_clone).await;
        });
        handles.push(handle);
    }
    for handle in handles {
        let _ = handle.await;
    }
    Ok(())
}

async fn poll_backend(
    backend: &mut TcpStream,
    tx: UnboundedSender<Message>,
) -> Result<(), ServerError> {
    let mut read_buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 4096];

    info!("Polling backend");

    loop {
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
