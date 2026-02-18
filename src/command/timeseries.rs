use crate::CommandExecute;

#[derive(Debug)]
pub struct TsCreateCommand {
    pub key: String,
    pub retention_ms: Option<u64>,
    pub labels: Vec<(String, String)>,
}

impl CommandExecute for TsCreateCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("TS.CREATE is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "TS.CREATE"
    }
}

#[derive(Debug)]
pub struct TsAddCommand {
    pub key: String,
    pub timestamp: u64,
    pub value: f64,
}

impl CommandExecute for TsAddCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("TS.ADD is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "TS.ADD"
    }
}

#[derive(Debug)]
pub struct TsGetCommand {
    pub key: String,
}

impl CommandExecute for TsGetCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("TS.GET is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "TS.GET"
    }
}

#[derive(Debug)]
pub struct TsRangeCommand {
    pub key: String,
    pub from: u64,
    pub to: u64,
    pub count: Option<usize>,
}

impl CommandExecute for TsRangeCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("TS.RANGE is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "TS.RANGE"
    }
}

#[derive(Debug)]
pub struct TsDelCommand {
    pub key: String,
    pub from: u64,
    pub to: u64,
}

impl CommandExecute for TsDelCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("TS.DEL is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "TS.DEL"
    }
}
