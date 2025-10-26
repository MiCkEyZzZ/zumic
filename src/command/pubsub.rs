use crate::{CommandExecute, Value};

#[derive(Debug)]
pub struct SubscribeCommand {
    pub channels: Vec<String>,
}

impl CommandExecute for SubscribeCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        Ok(Value::Null)
    }

    fn command_name(&self) -> &'static str {
        "SUBSCRIBE"
    }
}

#[derive(Debug)]
pub struct UnsubscribeCommand {
    pub channels: Vec<String>,
}

impl CommandExecute for UnsubscribeCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<Value, crate::StoreError> {
        Ok(Value::Null)
    }

    fn command_name(&self) -> &'static str {
        "UNSUBSCRIBE"
    }
}

#[derive(Debug)]
pub struct PublishCommand {
    pub channel: String,
    pub message: Value,
}

impl CommandExecute for PublishCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<Value, crate::StoreError> {
        Ok(Value::Int(1))
    }

    fn command_name(&self) -> &'static str {
        "PUBLISH"
    }
}
