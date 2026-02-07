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

#[derive(Debug)]
pub struct PSubscribeCommand {
    pub patterns: Vec<String>,
}

impl CommandExecute for PSubscribeCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<Value, crate::StoreError> {
        unimplemented!("PSUBSCRIBE is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "PSUBSCRIBE"
    }
}

#[derive(Debug)]
pub struct PUnsubscribeCommand {
    pub patterns: Vec<String>,
}

impl CommandExecute for PUnsubscribeCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<Value, crate::StoreError> {
        unimplemented!("PUNSUBSCRIBE is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "PUNSUBSCRIBE"
    }
}

#[derive(Debug)]
pub struct PubSubCommand {
    pub subcommand: String,
    pub args: Vec<String>,
}

impl CommandExecute for PubSubCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<Value, crate::StoreError> {
        unimplemented!("PUBSUB command is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "PUBSUB"
    }
}
