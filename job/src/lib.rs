use crate::commands::Command;
use crate::errors::JobError;
use clap::Parser;
use state::State;
use std::fs;
use std::io::{BufRead, Read, Write};
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

#[derive(clap::Parser)]
struct Cli {
    #[clap(long)]
    file: Option<String>,
    #[clap(long)]
    host: Option<String>,
    #[clap(long)]
    socket: Option<String>,
}

const SOCKET_PATH: &str = "server.sock";

pub async fn run() {
    tracing_subscriber::fmt::init();

    let args = Cli::parse();

    let socket = UnixStream::connect(args.socket.unwrap_or(SOCKET_PATH.to_string()))
        .await
        .expect("Failed to connect to socket");

    let socket = Arc::new(RwLock::new(socket));

    let mut buffer = vec![0; 1024];
    let state = Arc::new(State::new(
        args.host.unwrap_or("localhost:8080".to_string()),
    ));

    if let Some(file) = args.file {
        handle_file(state.clone(), socket.clone(), file).await;
    }

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

async fn handle_file(state: Arc<State>, socket: Arc<RwLock<UnixStream>>, file: String) {
    for line in fs::read(file).expect("Unable to open file").lines() {
        let line = line.expect("Unable to read line");
        let command = Command::from_slice(&line.as_bytes().trim_ascii_end())
            .expect("Unable to parse command");
        command
            .execute(&state, &socket)
            .await
            .expect("Unable to execute command");
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
