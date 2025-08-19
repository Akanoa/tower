use protocol::Message;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;
use tracing::debug;

const BUFFER_SIZE: usize = 200;

type CircularBuffer = Arc<RwLock<circular_buffer::CircularBuffer<BUFFER_SIZE, Message>>>;

pub async fn start(addr: &str, port: u16, rx: Receiver<Message>) {
    let buffer = circular_buffer::CircularBuffer::<BUFFER_SIZE, Message>::new();
    let buffer = Arc::new(RwLock::new(buffer));

    tokio::spawn(handle_message(rx, buffer.clone()));

    let listener = tokio::net::TcpListener::bind((addr, port)).await.unwrap();
    debug!("Listening on port {addr}{port}");

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(handle_connection(stream, buffer.clone()));
    }
}

async fn handle_message(mut rx: Receiver<Message>, buffer: CircularBuffer) {
    while let Some(msg) = rx.recv().await {
        buffer.write().await.push_back(msg);
    }
}

async fn handle_connection(mut stream: TcpStream, buffer: CircularBuffer) {
    let mut i = 0;
    loop {
        for message in buffer.read().await.iter() {
            i += 1;
            let buffer = message.encode().expect("Failed to encode message");
            if let Err(e) = stream.write_all(&buffer).await {
                debug!("Closed stream");
                break;
            }
            if let Err(e) = stream.flush().await {
                debug!("Closed stream");
                break;
            }
        }
    }
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
