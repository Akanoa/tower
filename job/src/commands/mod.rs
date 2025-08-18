mod parser;
use crate::errors::JobError;
use crate::state::State;
use crate::Action;
pub use parser::*;

impl Action for CommandRegister<'_> {
    async fn execute(self, state: &State) -> Result<(), JobError> {
        let CommandRegister {
            tenant,
            executor_id,
            interest,
        } = self;
        state.register(tenant, executor_id, interest).await
    }
}

impl Action for CommandUnregister<'_> {
    async fn execute(self, state: &State) -> Result<(), JobError> {
        let CommandUnregister {
            tenant,
            executor_id,
            watch_id,
        } = self;
        state.unregister(tenant, executor_id, watch_id).await
    }
}

impl Action for Command<'_> {
    async fn execute(self, state: &State) -> Result<(), JobError> {
        match self {
            Command::Register(x) => x.execute(state).await,
            Command::Unregister(x) => x.execute(state).await,
        }
    }
}
