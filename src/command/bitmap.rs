use std::ops::Not;

use super::CommandExecute;
use crate::{database::Bitmap, Sds, StorageEngine, StoreError, Value};

/// Команда SETBIT — устанавливает значение бита по смещению.
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

    fn command_name(&self) -> &'static str {
        "SETBIT"
    }
}

/// Команда GETBIT — получает значение бита по смещению.
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

    fn command_name(&self) -> &'static str {
        "GETBIT"
    }
}

/// Команда BITCOUNT — считает количество установленных битов в диапазоне.
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

    fn command_name(&self) -> &'static str {
        "BITCOUNT"
    }
}

/// Команда BITOP — выполняет побитовые операции (AND, OR, XOR, NOT) над bitmap.
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

    fn command_name(&self) -> &'static str {
        "BITOP"
    }
}

/// Команда BITPOS — находит позицию первого бита со значением 0 или 1 в bitmap.
#[derive(Debug)]
pub struct BitPosCommand {
    pub key: String,
    pub bit: bool,
    pub start: Option<usize>,
    pub end: Option<usize>,
}

impl CommandExecute for BitPosCommand {
    fn execute(
        &self,
        _store: &mut StorageEngine,
    ) -> Result<Value, StoreError> {
        unimplemented!("BITPOS is not implemented yet")
    }

    fn command_name(&self) -> &'static str {
        "BITPOS"
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStore;

    /// Тест проверяет корректность установки и получения значения бита.
    /// - Устанавливает бит и ожидает старое значение.
    /// - Получает текущие значения установленных и неустановленных битов.
    #[test]
    fn test_setbit_and_getbit() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());
        let key = "foo".to_string();

        // изначально бита нет => old = 0
        let set0 = SetBitCommand {
            key: key.clone(),
            offset: 5,
            value: true,
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(set0, Value::Int(0));

        // теперь бит установлен => old = 1
        let set1 = SetBitCommand {
            key: key.clone(),
            offset: 5,
            value: false,
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(set1, Value::Int(1));

        // GETBIT должен вернуть текущее значение (0)
        let get = GetBitCommand {
            key: key.clone(),
            offset: 5,
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(get, Value::Int(0));

        // GETBIT вне диапазона всегда 0
        let get2 = GetBitCommand {
            key: key.clone(),
            offset: 100,
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(get2, Value::Int(0));
    }

    /// Тест проверяет подсчёт количества установленных битов в заданном
    /// диапазоне.
    /// - Устанавливает биты в определённых позициях.
    /// - Проверяет bitcount для полного и частичного диапазона.
    #[test]
    fn test_bitcount_range() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());
        let key = "bar".to_string();

        // установим биты 0, 3, 7
        for &off in &[0, 3, 7] {
            SetBitCommand {
                key: key.clone(),
                offset: off,
                value: true,
            }
            .execute(&mut store)
            .unwrap();
        }

        // считаем с 0 до 8 -> должно быть 3
        let cnt_all = BitCountCommand {
            key: key.clone(),
            start: 0,
            end: 8,
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(cnt_all, Value::Int(3));

        // с 1 до 3 -> только бит 3 -> 1
        let cnt_sub = BitCountCommand {
            key: key.clone(),
            start: 1,
            end: 4,
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(cnt_sub, Value::Int(1));
    }

    /// Тест проверяет побитовые операции: NOT, AND, OR, XOR.
    /// - Устанавливает два битмапа и выполняет операции.
    /// - Проверяет результат через подсчёт установленных битов.
    /// - Также проверяет обработку некорректной операции.
    #[test]
    fn test_bitop_commands() {
        let mut store = StorageEngine::Memory(InMemoryStore::new());

        // A = 1010 (bits 1,3), B = 0110 (bits 1,2)
        let key_a = "A".to_string();
        let key_b = "B".to_string();
        for &off in &[1, 3] {
            SetBitCommand {
                key: key_a.clone(),
                offset: off,
                value: true,
            }
            .execute(&mut store)
            .unwrap();
        }
        for &off in &[1, 2] {
            SetBitCommand {
                key: key_b.clone(),
                offset: off,
                value: true,
            }
            .execute(&mut store)
            .unwrap();
        }

        // NOT A => 0101… length = 4
        let not_len = BitOpCommand {
            op: "NOT".into(),
            dest: "X".into(),
            keys: vec![key_a.clone()],
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(not_len, Value::Int(8)); // битовая длина байта = 8

        // AND => bits {1}
        BitOpCommand {
            op: "AND".into(),
            dest: "AND".into(),
            keys: vec![key_a.clone(), key_b.clone()],
        }
        .execute(&mut store)
        .unwrap();
        let and_cnt = BitCountCommand {
            key: "AND".into(),
            start: 0,
            end: 8,
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(and_cnt, Value::Int(1));

        // OR => bits {1,2,3} => 3
        BitOpCommand {
            op: "OR".into(),
            dest: "OR".into(),
            keys: vec![key_a.clone(), key_b.clone()],
        }
        .execute(&mut store)
        .unwrap();
        let or_cnt = BitCountCommand {
            key: "OR".into(),
            start: 0,
            end: 8,
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(or_cnt, Value::Int(3));

        // XOR => bits {2,3} => 2
        BitOpCommand {
            op: "XOR".into(),
            dest: "XOR".into(),
            keys: vec![key_a.clone(), key_b.clone()],
        }
        .execute(&mut store)
        .unwrap();
        let xor_cnt = BitCountCommand {
            key: "XOR".into(),
            start: 0,
            end: 8,
        }
        .execute(&mut store)
        .unwrap();
        assert_eq!(xor_cnt, Value::Int(2));

        // неизвестная операция
        let err = BitOpCommand {
            op: "FOO".into(),
            dest: "Z".into(),
            keys: vec![key_a.clone()],
        }
        .execute(&mut store);
        assert!(err.is_err());
    }
}
