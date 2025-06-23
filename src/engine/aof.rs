use std::{
    fs::{File, OpenOptions},
    io::{self, BufWriter, Read, Seek, Write},
    path::Path,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use tempfile::NamedTempFile;

/// 4-байтовый магический заголовок для формата AOF (идентификатор версии).
const MAGIC: &[u8; 4] = b"AOF1";

/// Коды операций, используемые в журнале AOF.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AofOp {
    /// Установка значения (SET)
    Set = 1,
    /// Удаление ключа (DEL)
    Del = 2,
}

/// Политика синхронизации — как часто сбрасывать буфер AOF на диск.
#[derive(Debug, Clone, Copy)]
pub enum SyncPolicy {
    /// fsync/flush после каждой команды (максимальная надёжность).
    Always,
    /// сброс раз в секунду в фоновом потоке.
    EverySec,
    /// не сбрасывать явно (полагаемся на ОС).
    No,
}

/// Основная структура журнала AOF (Append-Only File).
/// Поддерживает буферизованную запись, восстановление и компактацию.
pub struct AofLog {
    /// Буферизованный writer, защищённый мьютексом для потокобезопасности.
    writer: Arc<Mutex<BufWriter<File>>>,
    /// Reader для воспроизведения операций из начала файла.
    reader: File,
    /// Выбранная политика синхронизации.
    policy: SyncPolicy,
    /// Канал для остановки фонового флешера (для EverySec).
    flusher_stop_tx: Option<Sender<()>>,
    /// Хэндл фонового потока флешера, чтобы ждать его завершения.
    flusher_handle: Option<JoinHandle<()>>,
}

impl AofLog {
    /// Открывает (или создаёт) файл AOF по пути `path`.
    /// Проверяет или инициализирует магический заголовок.
    /// Запускает фоновый flusher, если политика EverySec.
    pub fn open<P: AsRef<Path>>(
        path: P,
        policy: SyncPolicy,
    ) -> io::Result<Self> {
        // 1) Читаем или создаём файл для проверки заголовка и для replay.
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)?;

        // Читаем или инициализируем MAGIC
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
                file.seek(io::SeekFrom::Start(0))?;
                file.write_all(MAGIC)?;
                file.flush()?;
            }
        }
        file.seek(io::SeekFrom::Start(0))?;
        let reader = file;

        // 2) Открываем отдельный дескриптор для записи и буферизуем его
        let write_file = OpenOptions::new().create(true).append(true).open(&path)?;
        // Оборачиваем BufWriter<File> в Arc<Mutex<_>>
        let writer = Arc::new(Mutex::new(BufWriter::new(write_file)));

        // 3) Собираем структуру без фонового потока
        let mut log = AofLog {
            writer: Arc::clone(&writer),
            reader,
            policy,
            flusher_stop_tx: None,
            flusher_handle: None,
        };

        // 4) Если политика EverySec — запускаем фоновый флешер
        if let SyncPolicy::EverySec = policy {
            let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();
            log.flusher_stop_tx = Some(tx);

            let writer_clone = Arc::clone(&writer);
            let handle = thread::spawn(move || {
                loop {
                    // ждем секунду или сигнал остановки
                    if rx.recv_timeout(Duration::from_secs(1)).is_ok() {
                        break;
                    }
                    if let Ok(mut guard) = writer_clone.lock() {
                        let _ = guard.flush();
                    }
                }
            });
            log.flusher_handle = Some(handle);
        }

        Ok(log)
    }

    /// Добавляет команду SET в журнал: [AofOp::Set][len_key][key][len_val][value]
    /// Затем вызывает `maybe_flush` в зависимости от политики.
    pub fn append_set(
        &mut self,
        key: &[u8],
        value: &[u8],
    ) -> io::Result<()> {
        let mut buf = self.writer.lock().unwrap();
        buf.write_all(&[AofOp::Set as u8])?;
        Self::write_32(&mut *buf, key.len() as u32)?;
        buf.write_all(key)?;
        Self::write_32(&mut *buf, value.len() as u32)?;
        buf.write_all(value)?;
        drop(buf);
        self.maybe_flush()
    }

    /// Добавляет команду DEL в журнал: [AofOp::Del][len_key][key]
    /// Затем вызывает `maybe_flush` в зависимости от политики.
    pub fn append_del(
        &mut self,
        key: &[u8],
    ) -> io::Result<()> {
        let mut buf = self.writer.lock().unwrap();
        buf.write_all(&[AofOp::Del as u8])?;
        Self::write_32(&mut *buf, key.len() as u32)?;
        buf.write_all(key)?;
        drop(buf);
        self.maybe_flush()
    }

    /// Воспроизводит все операции из начала файла, вызывая `f(op, key, value)`.
    /// Для DEL значение будет `None`.
    pub fn replay<F>(
        &mut self,
        mut f: F,
    ) -> io::Result<()>
    where
        F: FnMut(AofOp, Vec<u8>, Option<Vec<u8>>),
    {
        // возвращаем reader в начало
        self.reader.seek(io::SeekFrom::Start(0))?;
        // проверяем MAGIC
        let mut header = [0u8; 4];
        self.reader.read_exact(&mut header)?;
        if &header != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad AOF header"));
        }
        // читаем всё в память
        let mut buf = Vec::new();
        self.reader.read_to_end(&mut buf)?;
        let mut pos = 0;

        while pos < buf.len() {
            let op = AofOp::try_from(buf[pos])?;
            pos += 1;
            let key_len = Self::read_u32(&buf, &mut pos)? as usize;
            let key = buf[pos..pos + key_len].to_vec();
            pos += key_len;
            let val = if op == AofOp::Set {
                let vlen = Self::read_u32(&buf, &mut pos)? as usize;
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

    /// Компактация AOF: на основе итератора `live` создаём новый временный файл,
    /// записываем в него только актуальные SET, и атомарно заменяем старый.
    pub fn rewrite<I, P>(
        &mut self,
        path: P,
        live: I,
    ) -> io::Result<()>
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

        // Обновляем writer внутри Arc<Mutex<...>>
        let writer_file = OpenOptions::new().create(true).append(true).open(path)?;

        let mut guard = self.writer.lock().unwrap();
        *guard = BufWriter::new(writer_file);

        Ok(())
    }

    /// Утилита для записи `u32` в формате big-endian.
    #[inline]
    fn write_32<W: Write>(
        w: &mut W,
        v: u32,
    ) -> io::Result<()> {
        w.write_all(&v.to_be_bytes())
    }

    /// Безопасно читает `u32` из буфера в формате big-endian, с проверкой границ.
    #[inline]
    fn read_u32(
        buf: &[u8],
        pos: &mut usize,
    ) -> io::Result<u32> {
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
            SyncPolicy::Always => {
                // Блокируем мьютекс и делегируем flush внутреннему BufWriter<File>
                let mut w = self.writer.lock().unwrap();
                w.flush()
            }
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

impl Drop for AofLog {
    fn drop(&mut self) {
        // при дропе отсылаем сигнал остановки и ждём потока
        if let Some(tx) = self.flusher_stop_tx.take() {
            let _ = tx.send(()); // сигнал на выход.
        }
        if let Some(handle) = self.flusher_handle.take() {
            let _ = handle.join();
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
