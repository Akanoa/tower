use bincode::{Decode, Encode};

#[derive(Encode, Decode)]
pub enum Message {
    Register(MessageRegister),
    Unregister(MessageUnregister),
    Report(MessageReport),
}

impl Message {
    pub fn encode(&self) -> Result<Vec<u8>, bincode::error::EncodeError> {
        bincode::encode_to_vec(self, bincode::config::standard())
    }
    pub fn decode(bytes: &[u8]) -> Result<Message, bincode::error::DecodeError> {
        let (data, _) = bincode::decode_from_slice(bytes, bincode::config::standard())?;
        Ok(data)
    }
}

#[derive(Encode, Decode)]
pub struct MessageRegister {
    tenant: String,
    executor_id: i64,
    watch_id: i64,
}

#[derive(Encode, Decode)]
pub struct MessageUnregister {
    tenant: String,
    executor_id: i64,
    watch_id: i64,
}

#[derive(Encode, Decode)]
pub struct MessageReport {
    tenant: String,
    executor_id: i64,
    watch_id: i64,
    lag: u64,
    execution_time: f64,
}
