use crate::errors::JobError;
use crate::state::State;
use protocol::MessageReport;
use rand::Rng;

pub struct Watcher {
    watch_id: i64,
    interest: String,
}

impl Watcher {
    pub(crate) fn new(watch_id: i64, interest: &str) -> Self {
        Watcher {
            watch_id,
            interest: interest.into(),
        }
    }
}

impl Watcher {
    pub(crate) async fn run(
        &self,
        tenant: &str,
        executor_id: i64,
        state: &State,
    ) -> Result<MessageReport, JobError> {
        let lag = state.rng.write().await.random_range(0..11000);
        let execution_time = state.rng.write().await.random_range(0.0..1500.0);

        let report = MessageReport {
            tenant: tenant.into(),
            executor_id,
            watch_id: self.watch_id,
            lag,
            execution_time,
            interest: self.interest.clone(),
        };

        Ok(report)
    }
}
