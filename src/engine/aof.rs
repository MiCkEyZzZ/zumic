use std::{
    fs::{File, OpenOptions},
    io::{self, BufWriter, Read, Seek, Write},
    path::Path,
    thread,
    time::Duration,
};

use tempfile::NamedTempFile;

/// 4-байтовый магический заголовок для формата AOF (идентификатор версии).
const MAGIC: &[u8; 4] = b"AOF1";

/// Коды операций, используемые в журнале AOF.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AofOp {
    Set = 1,
    Del = 2,
}

/// Политика синхронизации — как часто сбрасывать буфер AOF на диск.
#[derive(Debug, Clone, Copy)]
pub enum SyncPolicy {
    /// fsync/flush после каждой команды (максимальная надёжность, минимальная производительность).
    Always,
    /// сброс раз в секунду в фоне.
    EverySec,
    /// не сбрасывать явно (полагаемся на ОС).
    No,
}

/// Журнал AOF (Append-Only File) с буферизованной записью и безопасным воспроизведением.
pub struct AofLog {
    writer: BufWriter<File>,
    reader: File,
    policy: SyncPolicy,
}

impl AofLog {
    /// Открывает (или создаёт) файл AOF с заданной политикой синхронизации,
    /// проверяет или записывает магический заголовок.
    pub fn open<P: AsRef<Path>>(path: P, policy: SyncPolicy) -> io::Result<Self> {
        // Открываем файл для чтения и дозаписи.
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)?;

        // Читаем или инициализируем магический заголовок.
        {
            let mut header = [0u8; 4];
            let n = file.read(&mut header)?;
            if n == 4 {
                if &header != MAGIC {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid AOF magic header",
                    ));
                }
            } else {
                // Пустой файл — записываем заголовок.
                file.seek(io::SeekFrom::Start(0))?;
                file.write_all(MAGIC)?;
                file.flush()?;
            }
        }

        // Сброс курсора в начало для чтения.
        file.seek(io::SeekFrom::Start(0))?;
        // Отдельный файл для записи.
        let writer_file = OpenOptions::new().create(true).append(true).open(path)?;

        let log = AofLog {
            writer: BufWriter::new(writer_file),
            reader: file,
            policy,
        };

        // Если выбрана EverySec, запускаем фоновый flusher.
        if let SyncPolicy::EverySec = log.policy {
            let writer_clone = log.writer.get_ref().try_clone()?;
            thread::spawn(move || {
                let mut bufm = BufWriter::new(writer_clone);
                loop {
                    thread::sleep(Duration::from_secs(1));
                    let _ = bufm.flush();
                }
            });
        }

        Ok(log)
    }

    /// Добавляет SET-операцию в журнал AOF.
    pub fn append_set(&mut self, key: &[u8], value: &[u8]) -> io::Result<()> {
        self.writer.write_all(&[AofOp::Set as u8])?;
        Self::write_32(&mut self.writer, key.len() as u32)?;
        self.writer.write_all(key)?;
        Self::write_32(&mut self.writer, value.len() as u32)?;
        self.writer.write_all(value)?;
        self.maybe_flush()?;
        Ok(())
    }

    /// Добавляет DEL-операцию в журнал AOF.
    pub fn append_del(&mut self, key: &[u8]) -> io::Result<()> {
        self.writer.write_all(&[AofOp::Del as u8])?;
        Self::write_32(&mut self.writer, key.len() as u32)?;
        self.writer.write_all(key)?;
        self.maybe_flush()?;
        Ok(())
    }

    /// Воспроизводит все операции из журнала, вызывая заданный callback для каждой записи.
    pub fn replay<F>(&mut self, mut f: F) -> io::Result<()>
    where
        F: FnMut(AofOp, Vec<u8>, Option<Vec<u8>>),
    {
        // Сбросываем считыватель в начало.
        self.reader.seek(io::SeekFrom::Start(0))?;

        // Читаем и проверяем магический заголовок.
        let mut header = [0u8; 4];
        self.reader.read_exact(&mut header)?;
        if &header != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad AOF header"));
        }

        // Считываем полное содержимое журнала в память.
        let mut buf = Vec::new();
        self.reader.read_to_end(&mut buf)?;
        let mut pos = 0;

        while pos < buf.len() {
            // Читаем код операции.
            let op = AofOp::try_from(buf[pos])?;
            pos += 1;

            // Читаем ключ.
            let key_len = Self::read_u32(&buf, &mut pos)? as usize;
            if pos + key_len > buf.len() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Key truncated",
                ));
            }
            let key = buf[pos..pos + key_len].to_vec();
            pos += key_len;

            // Если SET, читаем значение.
            let val = if op == AofOp::Set {
                let vlen = Self::read_u32(&buf, &mut pos)? as usize;
                if pos + vlen > buf.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Value truncated",
                    ));
                }
                let v = buf[pos..pos + vlen].to_vec();
                pos += vlen;
                Some(v)
            } else {
                None
            };

            f(op, key, val);
        }

        Ok(())
    }

    /// Сжимает (перезаписывает) AOF: оставляет только последние SET-операции из `live`,
    /// записывает их в новый временный файл и атомарно заменяет текущий.
    pub fn rewrite<I, P>(&mut self, path: P, live: I) -> io::Result<()>
    where
        I: IntoIterator<Item = (Vec<u8>, Vec<u8>)>,
        P: AsRef<Path>,
    {
        let path = path.as_ref();

        // 1. Создаём временный файл, в той же директории.
        let mut tmp = NamedTempFile::new_in(path.parent().unwrap_or_else(|| Path::new(".")))?;

        // 2. Записываем MAGIC
        tmp.write_all(MAGIC)?;
        tmp.flush()?;

        // 3. Записываем только SET-операции для каждого живого key/value.
        for (key, value) in live {
            tmp.write_all(&[AofOp::Set as u8])?;
            Self::write_32(&mut tmp, key.len() as u32)?;
            tmp.write_all(&key)?;
            Self::write_32(&mut tmp, value.len() as u32)?;
            tmp.write_all(&value)?;
        }
        tmp.flush()?;

        // Атомарно заменяем старый файл.
        tmp.persist(path)?;

        // После замены - нужно обновить writer и reader в AofLog:
        // Закрываем текущие дескрипторы, открываем новый файл.
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)?;
        file.seek(io::SeekFrom::Start(0))?;
        self.reader = file;
        let writer_file = OpenOptions::new().create(true).append(true).open(path)?;
        self.writer = BufWriter::new(writer_file);

        Ok(())
    }

    /// Утилита для записи `u32` в формате big-endian.
    #[inline]
    fn write_32<W: Write>(w: &mut W, v: u32) -> io::Result<()> {
        w.write_all(&v.to_be_bytes())
    }

    /// Безопасно читает `u32` из буфера в формате big-endian, с проверкой границ.
    #[inline]
    fn read_u32(buf: &[u8], pos: &mut usize) -> io::Result<u32> {
        if *pos + 4 > buf.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Unexpected EOF while reading u32",
            ));
        }
        let mut arr = [0u8; 4];
        arr.copy_from_slice(&buf[*pos..*pos + 4]);
        *pos += 4;
        Ok(u32::from_be_bytes(arr))
    }

    /// Выполняет flush согласно текущей политике синхронизации.
    fn maybe_flush(&mut self) -> io::Result<()> {
        match self.policy {
            SyncPolicy::Always => self.writer.flush(),
            SyncPolicy::EverySec => Ok(()),
            SyncPolicy::No => Ok(()), // будет сброшено фоном
        }
    }
}

impl TryFrom<u8> for AofOp {
    type Error = io::Error;
    fn try_from(v: u8) -> io::Result<Self> {
        match v {
            1 => Ok(AofOp::Set),
            2 => Ok(AofOp::Del),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown AOF op: {v}"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::NamedTempFile;

    use super::*;

    /// Вспомогательная функция для проверки append_set и append_del с последующим воспроизведением
    /// в соответствии с заданной политикой синхронизации.
    fn run_append_replay(policy: SyncPolicy) -> io::Result<()> {
        let temp_file = NamedTempFile::new()?;
        let path = temp_file.path();

        {
            let mut log = AofLog::open(path, policy)?;
            log.append_set(b"kin", b"dzadza")?;
            log.append_del(b"kin")?;
        }

        {
            let mut log = AofLog::open(path, policy)?;
            let mut seq = Vec::new();
            log.replay(|op, key, val| seq.push((op, key, val)))?;

            assert_eq!(seq.len(), 2);
            assert_eq!(seq[0].0, AofOp::Set);
            assert_eq!(&seq[0].1, b"kin");
            assert_eq!(seq[0].2.as_deref(), Some(b"dzadza".as_ref()));
            assert_eq!(seq[1].0, AofOp::Del);
            assert_eq!(&seq[1].1, b"kin");
            assert!(seq[1].2.is_none());
        }

        Ok(())
    }

    /// Проверяет поведение добавления и воспроизведения с помощью SyncPolicy::Always
    #[test]
    fn test_always_policy() {
        run_append_replay(SyncPolicy::Always).unwrap();
    }

    /// Проверяет поведение добавления и воспроизведения с помощью SyncPolicy::EverySec
    #[test]
    fn test_everysec_policy() {
        run_append_replay(SyncPolicy::EverySec).unwrap();
    }

    /// Проверяет поведение добавления и воспроизведения с помощью SyncPolicy::No
    #[test]
    fn test_no_policy() {
        run_append_replay(SyncPolicy::No).unwrap();
    }

    /// Тестирует несколько операций SET при всех политиках синхронизации и проверяет порядок воспроизведения.
    #[test]
    fn test_append_multiple_set_under_all_policies() {
        for policy in &[SyncPolicy::Always, SyncPolicy::EverySec, SyncPolicy::No] {
            let temp = NamedTempFile::new().unwrap();
            let path = temp.path();
            {
                let mut log = AofLog::open(path, *policy).unwrap();
                log.append_set(b"k1", b"v1").unwrap();
                log.append_set(b"k2", b"v2").unwrap();
                log.append_set(b"k3", b"v3").unwrap();
            }
            {
                let mut log = AofLog::open(path, *policy).unwrap();
                let mut seq = Vec::new();
                log.replay(|op, key, val| seq.push((op, key, val))).unwrap();
                assert_eq!(seq.len(), 3);
                for (i, (k, v)) in [("k1", "v1"), ("k2", "v2"), ("k3", "v3")]
                    .iter()
                    .enumerate()
                {
                    assert_eq!(seq[i].0, AofOp::Set);
                    assert_eq!(&seq[i].1, k.as_bytes());
                    assert_eq!(seq[i].2.as_deref(), Some(v.as_bytes()));
                }
            }
        }
    }

    /// Проверяет, что `rewrite()` сжимает AOF, сохраняя только последние операции SET и
    /// удаляя перезаписанные или удаленные ключи.
    #[test]
    fn test_rewrite_compacts_log() -> io::Result<()> {
        // Create AOF with duplicate keys and deletions
        let temp = NamedTempFile::new()?;
        let path = temp.path().to_path_buf();
        let mut log = AofLog::open(&path, SyncPolicy::Always)?;
        log.append_set(b"k1", b"v1")?;
        log.append_set(b"k2", b"v2")?;
        log.append_set(b"k1", b"v1_new")?;
        log.append_del(b"k2")?;
        log.append_set(b"k3", b"v3")?;
        drop(log);

        // Собираем в памяти «живые» пары, как в Storage::new
        let mut live_map = std::collections::HashMap::new();
        {
            let mut rlog = AofLog::open(&path, SyncPolicy::Always)?;
            rlog.replay(|op, key, val| match op {
                AofOp::Set => {
                    live_map.insert(key, val.unwrap());
                }
                AofOp::Del => {
                    live_map.remove(&key);
                }
            })?;
        }

        // Перезаписываем вызовы
        let mut clog = AofLog::open(&path, SyncPolicy::Always)?;
        clog.rewrite(&path, live_map.clone().into_iter())?;

        // После перезаписи журнал должен содержать только фактический SET для каждого ключа
        let mut seq = Vec::new();
        let mut rlog2 = AofLog::open(&path, SyncPolicy::Always)?;
        rlog2.replay(|op, key, val| seq.push((op, key, val)))?;

        // Проверьте, что порядок может быть любым, но значения должны совпадать
        let mut seen = std::collections::HashMap::new();
        for (op, key, val) in seq {
            assert_eq!(op, AofOp::Set);
            seen.insert(key, val.unwrap());
        }
        assert_eq!(seen, live_map);

        Ok(())
    }
}
