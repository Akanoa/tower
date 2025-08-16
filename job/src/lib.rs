use crate::commands::{Command, CommandRegister, CommandUnregister};
use crate::errors::JobError;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::RwLock;

mod commands;
mod errors;

#[derive(Default)]
struct State {
    executors: Arc<RwLock<BTreeMap<i64, Executor>>>,
}

impl State {
    async fn register(
        &self,
        executor_id: i64,
        watch_id: i64,
        interest: &str,
    ) -> Result<(), JobError> {
        self.executors
            .write()
            .await
            .entry(executor_id)
            .or_insert(Executor::new(executor_id))
            .watchers
            .insert(watch_id, interest.into());

        Ok(())
    }

    async fn unregister(&self, executor_id: i64, watch_id: i64) -> Result<(), JobError> {
        Ok(())
    }
}

struct Executor {
    executor_id: i64,
    watchers: BTreeMap<i64, Watcher>,
}

impl Executor {
    fn new(executor_id: i64) -> Self {
        Executor {
            executor_id,
            watchers: BTreeMap::new(),
        }
    }
}

struct Watcher {
    watch_id: i64,
    interest: String,
}

pub trait Action {
    fn execute(self, state: &mut State) -> Result<(), JobError>;
}

impl Action for CommandRegister<'_> {
    fn execute(&mut self) -> Result<(), JobError> {
        let CommandRegister {
            tenant,
            executor_id,
            interest,
        } = self;
        println!(
            "Executing register for tenant '{tenant}' using executor #{executor_id} with interest '{interest}'"
        );
        Ok(())
    }
}

impl Action for CommandUnregister<'_> {
    fn execute(&mut self) -> Result<(), JobError> {
        let CommandUnregister {
            tenant,
            executor_id,
            watch_id,
        } = self;
        println!(
            "Executing unregister for tenant '{tenant}' using executor #{executor_id} for watcher #{watch_id}'"
        );
        Ok(())
    }
}

impl Action for Command<'_> {
    fn execute(&mut self) -> Result<(), JobError> {
        match self {
            Command::Register(x) => x.execute(),
            Command::Unregister(x) => x.execute(),
        }
    }
}

pub async fn run() {
    let mut buffer = vec![0; 1024];
    println!("Please enter command");
    loop {
        print!("> ");
        std::io::stdout().flush().expect("failed to flush stdout");
        let read_size = std::io::stdin()
            .lock()
            .read(&mut buffer)
            .expect("Unable to read input");
        let data = &buffer[..read_size].trim_ascii_end();
        let mut command = commands::Command::from_slice(data).expect("Unable to parse command");
        command.execute().expect("Unable to execute command");
    }
}
