use crate::errors::JobError;
use crate::state::State;
use crate::watcher::Watcher;
use protocol;
use protocol::Message;
use std::collections::BTreeMap;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::sync::RwLock;

pub struct Executor {
    executor_id: i64,
    pub(crate) watchers: BTreeMap<i64, Watcher>,
    pub tenant: String,
}

impl Executor {
    pub(crate) fn new(tenant: &str, executor_id: i64) -> Self {
        Executor {
            executor_id,
            watchers: BTreeMap::new(),
            tenant: tenant.to_string(),
        }
    }
}

impl Executor {
    pub async fn run(&self, state: &State, socket: &RwLock<UnixStream>) -> Result<(), JobError> {
        let mut reports = vec![];

        for watcher in self.watchers.values() {
            let report = watcher.run(&self.tenant, self.executor_id, state).await?;
            reports.push(Message::Report(report));
        }

        let mut buffer = vec![0_u8; 1024];
        for report in reports {
            let write_size = protocol::bincode::encode_into_slice(
                report,
                &mut buffer,
                protocol::bincode::config::standard(),
            )?;

            let mut socket = socket.write().await;

            socket.write_all(&buffer[..write_size]).await?;
            socket.flush().await?;
        }

        Ok(())
    }
}
