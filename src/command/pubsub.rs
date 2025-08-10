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
        // Тут вызывается логика движка по подписке на каналы
        // Например: store.pubsub_subscribe(&self.channels)
        Ok(Value::Null)
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
        // Логика отписки
        Ok(Value::Null)
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
        // Логика публикации сообщения в канал
        Ok(Value::Int(1)) // например, кол-во получателей
    }
}
