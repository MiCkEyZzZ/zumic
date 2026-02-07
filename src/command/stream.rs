use super::CommandExecute;

/// Команда XADD добавляет запись в поток.
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
        unimplemented!()
    }

    fn command_name(&self) -> &'static str {
        "XADD"
    }
}

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
        unimplemented!()
    }

    fn command_name(&self) -> &'static str {
        "XREAD"
    }
}

/// Команда XRANGE возвращает записи из потока в диапазоне ID.
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
        unimplemented!()
    }

    fn command_name(&self) -> &'static str {
        "XRANGE"
    }
}

/// Команда XREVRANGE возвращает записи из потока в обратном порядке.
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
        unimplemented!()
    }

    fn command_name(&self) -> &'static str {
        "XREVRANGE"
    }
}

/// Команда XLEN возвращает кол-во записей в потоке.
#[derive(Debug)]
pub struct XLenCommand {
    pub key: String,
}

impl CommandExecute for XLenCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!()
    }

    fn command_name(&self) -> &'static str {
        "XLEN"
    }
}

/// Команда XDEL удаляет записи по их ID.
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
        unimplemented!()
    }

    fn command_name(&self) -> &'static str {
        "XDEL"
    }
}

/// Команда XTRIM обрезает поток, оставляя только последние N записей.
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
        unimplemented!()
    }

    fn command_name(&self) -> &'static str {
        "XTRIM"
    }
}

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
        unimplemented!()
    }

    fn command_name(&self) -> &'static str {
        "XGROUP CREATE"
    }
}

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
        unimplemented!()
    }

    fn command_name(&self) -> &'static str {
        "XACK"
    }
}
