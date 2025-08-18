use crate::errors::JobError;
use state::State;
use std::io::{Read, Write};
use tracing::{error, info, warn};

mod commands;
mod errors;
mod executor;
mod state;
mod watcher;

trait Action {
    async fn execute(self, state: &State) -> Result<(), JobError>;
}

pub async fn run() {
    tracing_subscriber::fmt::init();

    let mut buffer = vec![0; 1024];
    let state = State::default();
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
        if let Err(err) = command.execute(&state).await {
            error!(error = %err, "Error executing command")
        }
    }
}
