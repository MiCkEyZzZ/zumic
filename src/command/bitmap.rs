use std::ops::Not;

use super::CommandExecute;
use crate::{database::Bitmap, Sds, StorageEngine, StoreError, Value};

#[derive(Debug)]
pub struct SetBitCommand {
    pub key: String,
    pub offset: usize,
    pub value: bool,
}

impl CommandExecute for SetBitCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let mut bmp = match store.get(&Sds::from_str(&self.key))? {
            Some(Value::Bitmap(b)) => b,
            _ => Bitmap::new(),
        };
        let old = bmp.set_bit(self.offset, self.value);
        store.set(&Sds::from_str(&self.key), Value::Bitmap(bmp))?;
        Ok(Value::Int(old as i64))
    }
}

#[derive(Debug)]
pub struct GetBitCommand {
    pub key: String,
    pub offset: usize,
}

impl CommandExecute for GetBitCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let bit = if let Some(Value::Bitmap(b)) = store.get(&Sds::from_str(&self.key))? {
            b.get_bit(self.offset)
        } else {
            false
        };
        Ok(Value::Int(bit as i64))
    }
}

#[derive(Debug)]
pub struct BitCountCommand {
    pub key: String,
    pub start: usize,
    pub end: usize,
}

impl CommandExecute for BitCountCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        let cnt = if let Some(Value::Bitmap(b)) = store.get(&Sds::from_str(&self.key))? {
            b.bitcount(self.start, self.end)
        } else {
            0
        };
        Ok(Value::Int(cnt as i64))
    }
}

#[derive(Debug)]
pub struct BitOpCommand {
    pub op: String,
    pub dest: String,
    pub keys: Vec<String>,
}

impl CommandExecute for BitOpCommand {
    fn execute(
        &self,
        store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        // собираем все битмапы
        let mut bitmaps: Vec<Bitmap> = Vec::with_capacity(self.keys.len());
        for k in &self.keys {
            if let Some(Value::Bitmap(b)) = store.get(&Sds::from_str(k))? {
                bitmaps.push(b);
            }
        }

        // вычисляе результат
        let result = match self.op.as_str() {
            "NOT" => {
                let first = bitmaps.first().cloned().unwrap_or_default();
                first.not()
            }
            "AND" | "OR" | "XOR" => {
                let mut iter = bitmaps.into_iter();
                let first = iter.next().unwrap_or_default();
                iter.fold(first, |acc, bmp| match self.op.as_str() {
                    "AND" => &acc & &bmp,
                    "OR" => &acc | &bmp,
                    "XOR" => &acc ^ &bmp,
                    _ => acc,
                })
            }
            _ => return Err(StoreError::Syntax(format!("Unknown BITOP `{}`", self.op))),
        };

        // сохраняем и возвращаем длину в битах
        store.set(&Sds::from_str(&self.dest), Value::Bitmap(result.clone()))?;
        Ok(Value::Int(result.bit_len() as i64))
    }
}
