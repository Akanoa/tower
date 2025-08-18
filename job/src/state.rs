use crate::errors::JobError;
use crate::executor::Executor;
use crate::watcher::Watcher;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use std::collections::BTreeMap;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

pub struct State {
    pub host: String,
    pub executors: RwLock<BTreeMap<(String, i64), Executor>>,
    pub(crate) rng: RwLock<StdRng>,
}

impl State {
    pub(crate) fn new(host: String) -> Self {
        State {
            host: host.to_string(),
            executors: Default::default(),
            rng: RwLock::new(StdRng::from_os_rng()),
        }
    }
}

impl State {
    pub async fn register(
        &self,
        tenant: &str,
        executor_id: i64,
        interest: &str,
    ) -> Result<i64, JobError> {
        let watcher_id = self.rng.write().await.next_u32() as i64;

        self.executors
            .write()
            .await
            .entry((tenant.to_string(), executor_id))
            .or_insert(Executor::new(tenant, executor_id))
            .watchers
            .insert(watcher_id, Watcher::new(watcher_id, interest));

        info!(
            tenant,
            executor_id, watcher_id, interest, "Registering executor"
        );

        Ok(watcher_id)
    }

    pub async fn unregister(
        &self,
        tenant: &str,
        executor_id: i64,
        watch_id: i64,
    ) -> Result<(), JobError> {
        let mut executor_guard = self.executors.write().await;

        let key = (tenant.to_string(), executor_id);
        let Some(executor) = executor_guard.get_mut(&key) else {
            error!(tenant, executor_id, "Executor not found");
            return Err(JobError::NotExistingExecutor(executor_id));
        };

        match executor.watchers.remove(&watch_id) {
            None => {
                warn!(tenant, executor_id, watch_id, "Unable to remove watcher")
            }
            Some(_) => {
                info!(tenant, executor_id, watch_id, "Removing watcher")
            }
        }

        Ok(())
    }
}
