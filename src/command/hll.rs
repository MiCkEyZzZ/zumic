use super::CommandExecute;

/// Команда PFADD добавляет элементы в HLL.
#[derive(Debug)]
pub struct PfAddCommand {
    pub key: String,
    pub elements: Vec<String>,
}

impl CommandExecute for PfAddCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("PFADD is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "PFADD"
    }
}

/// Команда PFCOUNT возвращает приблизительное кол-во уникальных элементов.
#[derive(Debug)]
pub struct PfCountCommand {
    pub key: String,
}

impl CommandExecute for PfCountCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("PFCOUNT is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "PFCOUNT"
    }
}

/// Команда PFMERGE объежиняет несколько HLL в один.
#[derive(Debug)]
pub struct PfMergeCommand {
    pub data: String,
    pub sources: Vec<String>,
}

impl CommandExecute for PfMergeCommand {
    fn execute(
        &self,
        _store: &mut crate::StorageEngine,
    ) -> Result<crate::Value, crate::StoreError> {
        unimplemented!("PFMERGE is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "PFMERGE"
    }
}
