use bytes::Bytes;

#[derive(Debug, Clone)]
pub struct Message {
    pub channel: String,
    pub payload: Bytes,
}

impl Message {
    pub fn new(channel: impl Into<String>, payload: impl Into<Bytes>) -> Self {
        Self {
            channel: channel.into(),
            payload: payload.into(),
        }
    }
}
