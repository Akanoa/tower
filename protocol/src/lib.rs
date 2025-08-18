pub use bincode;
pub use bincode::{Decode, Encode};

#[derive(Encode, Decode, Debug)]
pub struct Message {
    pub host: String,
    pub body: MessageBody,
}

impl Message {
    pub fn new(host: String, body: MessageBody) -> Message {
        Message { host, body }
    }
}

#[derive(Encode, Decode, Debug)]
pub enum MessageBody {
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

#[derive(Encode, Decode, Debug)]
pub struct MessageRegister {
    pub tenant: String,
    pub executor_id: i64,
    pub watch_id: i64,
}

#[derive(Encode, Decode, Debug)]
pub struct MessageUnregister {
    pub tenant: String,
    pub executor_id: i64,
    pub watch_id: i64,
}

#[derive(Encode, Decode, Debug)]
pub struct MessageReport {
    pub tenant: String,
    pub executor_id: i64,
    pub watch_id: i64,
    pub lag: u64,
    pub execution_time: f64,
    pub interest: String,
}
