mod parser;

use crate::errors::JobError;
use crate::state::State;
use crate::Action;
pub use parser::*;
use protocol::{Message, MessageRegister, MessageUnregister};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::sync::RwLock;

impl Action for CommandRegister<'_> {
    async fn execute(self, state: &State, socket: &RwLock<UnixStream>) -> Result<(), JobError> {
        let CommandRegister {
            tenant,
            executor_id,
            interest,
        } = self;
        let watch_id = state.register(tenant, executor_id, interest).await?;
        let register_message = Message::Register(MessageRegister {
            tenant: tenant.into(),
            watch_id,
            executor_id,
        });
        let buffer = register_message.encode()?;

        let mut socket = socket.write().await;
        socket.write_all(&buffer).await?;
        socket.flush().await?;
        Ok(())
    }
}

impl Action for CommandUnregister<'_> {
    async fn execute(self, state: &State, socket: &RwLock<UnixStream>) -> Result<(), JobError> {
        let CommandUnregister {
            tenant,
            executor_id,
            watch_id,
        } = self;
        state.unregister(tenant, executor_id, watch_id).await?;

        let unregister_message = Message::Unregister(MessageUnregister {
            tenant: tenant.into(),
            executor_id,
            watch_id,
        });

        let buffer = unregister_message.encode()?;
        let mut socket = socket.write().await;
        socket.write_all(&buffer).await?;
        socket.flush().await?;

        Ok(())
    }
}

impl Action for Command<'_> {
    async fn execute(self, state: &State, socket: &RwLock<UnixStream>) -> Result<(), JobError> {
        match self {
            Command::Register(x) => x.execute(state, socket).await,
            Command::Unregister(x) => x.execute(state, socket).await,
        }
    }
}
