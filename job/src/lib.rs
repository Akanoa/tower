use crate::errors::JobError;
use state::State;
use std::io::{Read, Write};
use std::ops::Deref;
use std::sync::Arc;
use tokio::net::UnixStream;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

mod commands;
mod errors;
mod executor;
mod state;
mod watcher;

trait Action {
    async fn execute(self, state: &State, socket: &RwLock<UnixStream>) -> Result<(), JobError>;
}

const SOCKET_PATH: &str = "server.sock";

pub async fn run() {
    tracing_subscriber::fmt::init();

    let socket = UnixStream::connect(SOCKET_PATH)
        .await
        .expect("Failed to connect to socket");

    let socket = Arc::new(RwLock::new(socket));

    let mut buffer = vec![0; 1024];
    let state = Arc::new(State::default());

    tokio::spawn(async_job(state.clone(), socket.clone()));

    println!("Please enter command");
    loop {
        print!("> ");
        std::io::stdout().flush().expect("failed to flush stdout");
        let read_size = std::io::stdin()
            .lock()
            .read(&mut buffer)
            .expect("Unable to read input");
        let data = &buffer[..read_size].trim_ascii_end();
        let Ok(command) = commands::Command::from_slice(data) else {
            warn!(input=?String::from_utf8_lossy(data), "Invalid command");
            continue;
        };
        if let Err(err) = command.execute(&state, &socket).await {
            error!(error = %err, "Error executing command")
        }
    }
}

async fn async_job(state: Arc<State>, socket: Arc<RwLock<UnixStream>>) {
    info!("Starting async job worker");
    loop {
        for executor in state.executors.read().await.values() {
            if let Err(err) = executor.run(&state, socket.deref()).await {
                error!(error = %err, "Error running executor");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}
