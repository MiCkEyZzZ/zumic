use super::CommandExecute;

/// Команда XADD — добавляет запись в поток.
#[derive(Debug)]
pub struct XAddCommand {
    pub key: String,
    pub fileds: Vec<(String, String)>,
}

impl CommandExecute for XAddCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("XADD is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "XADD"
    }
}

/// Команда XREAD — читает записи из одного или нескольких потоков.
#[derive(Debug)]
pub struct XReadCommand {
    pub streams: Vec<(String, String)>,
    pub count: Option<usize>,
}

impl CommandExecute for XReadCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("XREAD is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "XREAD"
    }
}

/// Команда XRANGE — возвращает записи из потока в диапазоне ID.
#[derive(Debug)]
pub struct XRangeCommand {
    pub key: String,
    pub start: String,
    pub end: String,
}

impl CommandExecute for XRangeCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("XRANGE is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "XRANGE"
    }
}

/// Команда XREVRANGE — возвращает записи из потока в обратном порядке.
#[derive(Debug)]
pub struct XRevRangeCommand {
    pub key: String,
    pub start: String,
    pub end: String,
}

impl CommandExecute for XRevRangeCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("XREVRANGE is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "XREVRANGE"
    }
}

/// Команда XLEN — возвращает количество записей в потоке.
#[derive(Debug)]
pub struct XLenCommand {
    pub key: String,
}

impl CommandExecute for XLenCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("XLEN is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "XLEN"
    }
}

/// Команда XDEL — удаляет записи по их ID.
#[derive(Debug)]
pub struct XDelCommand {
    pub key: String,
    pub ids: Vec<String>,
}

impl CommandExecute for XDelCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("XDEL is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "XDEL"
    }
}

/// Команда XTRIM — обрезает поток, оставляя только последние `max_len`
/// записей.
#[derive(Debug)]
pub struct XTrimCommand {
    pub key: String,
    pub max_len: usize,
}

impl CommandExecute for XTrimCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("XTRIM is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "XTRIM"
    }
}

/// Команда XGROUP CREATE — создаёт группу потребителей для потока.
#[derive(Debug)]
pub struct XGroupCreateCommand {
    pub key: String,
    pub group: String,
    pub id: String,
}

impl CommandExecute for XGroupCreateCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("XGROUP CREATE is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "XGROUP CREATE"
    }
}

/// Команда XACK — подтверждает получение записей группой потребителей.
#[derive(Debug)]
pub struct XAckCommand {
    pub key: String,
    pub group: String,
    pub ids: Vec<String>,
}

impl CommandExecute for XAckCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("XACK is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "XACK"
    }
}
