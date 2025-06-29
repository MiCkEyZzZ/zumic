use std::{
    fs::{File, OpenOptions},
    io::{self, BufWriter, Read, Seek, Write},
    path::Path,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tempfile::NamedTempFile;

/// 4-байтовый магический заголовок для формата AOF (идентификатор версии).
const MAGIC: &[u8; 4] = b"AOF1";

/// Коды операций в AOF-логе.
/// Используются для сериализации и восстановления команд.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AofOp {
    /// Установка пары ключ-значение (аналог команды SET)
    Set = 1,
    /// Удаление ключа (аналог команды DEL)
    Del = 2,
}

/// Политика синхронизации AOF.
/// Определяет, когда именно сбрасывать данные из буфера на диск.
#[derive(Debug, Clone, Copy)]
pub enum SyncPolicy {
    /// Сбрасывать буфер после каждой команды (максимальная надёжность, но медленно)
    Always,
    /// Сбрасывать в фоновом потоке каждую секунду
    EverySec,
    /// Никогда не сбрасывать явно (надеемся на ОС и кэш)
    No,
}

/// Статистика работы AOF-журнала.
#[derive(Debug, Default)]
pub struct AofMetrics {
    /// Общее количество операций (SET + DEL)
    pub ops_total: usize,
    /// Количество операций SET
    pub ops_set: usize,
    /// Количество операций DEL
    pub ops_del: usize,
    /// Количество вызовов flush
    pub flush_count: usize,
    /// Количество ошибок при flush
    pub flush_errors: usize,
    /// Общее время всех flush-операций в наносекундах
    pub flush_total_ns: u64,
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
    pending_ops: AtomicUsize,
    /// Текущий порог батча (количество операций до flush)
    batch_size: AtomicUsize,
    /// Время последней операции (секунды с UNIX epoch)
    last_activity: AtomicU64,
    metrics_ops_total: AtomicUsize,
    metrics_ops_set: AtomicUsize,
    metrics_ops_del: AtomicUsize,
    metrics_flush_count: AtomicUsize,
    metrics_flush_errors: AtomicUsize,
    metrics_flush_total_ns: AtomicU64,
}

impl AofLog {
    /// Изначальный размер батча перед flush в режиме Always.
    const INITIAL_BATCH: usize = 32;

    /// Открывает (или создаёт) файл AOF по заданному пути.
    /// Проверяет или записывает магический заголовок.
    /// Настраивает буферизацию и политику синхронизации.
    ///
    /// Если политика `EverySec`, запускает фоновый поток сброса.
    ///
    /// # Аргументы
    /// - `path` — путь к AOF-файлу.
    /// - `policy` — политика синхронизации (Always, EverySec, No).
    ///
    /// # Ошибки
    /// Возвращает ошибку при некорректном заголовке файла или ошибке открытия.
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

        {
            let mut header = [0u8; 4];
            let n = file.read(&mut header)?;
            if n == MAGIC.len() {
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

        let write_file = OpenOptions::new().create(true).append(true).open(&path)?;
        let writer = Arc::new(Mutex::new(BufWriter::new(write_file)));

        let mut log = AofLog {
            writer: Arc::clone(&writer),
            reader,
            policy,
            flusher_stop_tx: None,
            flusher_handle: None,
            pending_ops: AtomicUsize::new(0),
            batch_size: AtomicUsize::new(Self::INITIAL_BATCH),
            last_activity: AtomicU64::new(Instant::now().elapsed().as_secs()),
            metrics_ops_total: AtomicUsize::new(0),
            metrics_ops_set: AtomicUsize::new(0),
            metrics_ops_del: AtomicUsize::new(0),
            metrics_flush_count: AtomicUsize::new(0),
            metrics_flush_errors: AtomicUsize::new(0),
            metrics_flush_total_ns: AtomicU64::new(0),
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

    /// Добавляет в журнал команду `SET` с ключом и значением.
    /// Формат: [AofOp::Set][len(key)][key][len(val)][val].
    ///
    /// В зависимости от политики синхронизации вызывает flush немедленно или отложено.
    ///
    /// # Аргументы
    /// - `key` — ключ.
    /// - `value` — значение.
    ///
    /// # Ошибки
    /// Возвращает ошибку записи или сброса буфера.
    pub fn append_set(
        &mut self,
        key: &[u8],
        value: &[u8],
    ) -> io::Result<()> {
        self.metrics_ops_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.metrics_ops_set
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        {
            let mut buf = self.writer.lock().unwrap();
            buf.write_all(&[AofOp::Set as u8])?;
            Self::write_32(&mut *buf, key.len() as u32)?;
            buf.write_all(key)?;
            Self::write_32(&mut *buf, value.len() as u32)?;
            buf.write_all(value)?;
        }
        let now_s = Instant::now().elapsed().as_secs();
        self.last_activity.store(now_s, Ordering::Relaxed);
        if let SyncPolicy::Always = self.policy {
            let count = self.pending_ops.fetch_add(1, Ordering::Relaxed) + 1;
            let threshold = self.batch_size.load(Ordering::Relaxed);
            if count >= threshold {
                self.flush_immediate()?;
                self.pending_ops.store(0, Ordering::Relaxed);
            }
            self.adjust_batch_size();
            Ok(())
        } else {
            self.maybe_flush()
        }
    }

    /// Добавляет в журнал команду `DEL` с ключом.
    /// Формат: [AofOp::Del][len(key)][key]
    ///
    /// В зависимости от политики синхронизации вызывает flush немедленно или отложено.
    ///
    /// # Аргументы
    /// - `key` — ключ, который нужно удалить.
    ///
    /// # Ошибки
    /// Возвращает ошибку записи или сброса буфера.
    pub fn append_del(
        &mut self,
        key: &[u8],
    ) -> io::Result<()> {
        self.metrics_ops_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.metrics_ops_del
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        {
            let mut buf = self.writer.lock().unwrap();
            buf.write_all(&[AofOp::Del as u8])?;
            Self::write_32(&mut *buf, key.len() as u32)?;
            buf.write_all(key)?;
        }
        let now_s = Instant::now().elapsed().as_secs();
        self.last_activity.store(now_s, Ordering::Relaxed);
        if let SyncPolicy::Always = self.policy {
            let count = self.pending_ops.fetch_add(1, Ordering::Relaxed) + 1;
            let threshold = self.batch_size.load(Ordering::Relaxed);
            if count >= threshold {
                self.flush_immediate()?;
                self.pending_ops.store(0, Ordering::Relaxed);
            }
            self.adjust_batch_size();
            Ok(())
        } else {
            self.maybe_flush()
        }
    }

    /// Воспроизводит все операции из AOF-журнала с начала файла.
    ///
    /// Вызывает переданный callback `f(op, key, val)`
    /// для каждой операции SET/DEL. Для DEL значение `None`.
    ///
    /// # Аргументы
    /// - `f` — функция, принимающая тип операции, ключ и значение (или None).
    ///
    /// # Ошибки
    /// Возвращает ошибку при чтении файла или некорректных данных.
    pub fn replay<F>(
        &mut self,
        mut f: F,
    ) -> io::Result<()>
    where
        F: FnMut(AofOp, Vec<u8>, Option<Vec<u8>>),
    {
        self.reader.seek(io::SeekFrom::Start(0))?;
        let mut header = [0u8; 4];
        self.reader.read_exact(&mut header)?;
        if &header != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad AOF header"));
        }
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

    /// Компактирует журнал AOF, записывая только "живые" ключи.
    /// Использует временный файл и затем атомарно заменяет оригинал.
    ///
    /// # Аргументы
    /// - `path` — путь к AOF-файлу.
    /// - `live` — итерируемое множество пар (ключ, значение), представляющее актуальное состояние.
    ///
    /// # Ошибки
    /// Возвращает ошибку при записи или замене файла.
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

    /// Возвращает снимок текущих метрик AOF-журнала.
    ///
    /// Полезно для мониторинга и отладки.
    pub fn metrics(&self) -> AofMetrics {
        AofMetrics {
            ops_total: self.metrics_ops_total.load(Ordering::Relaxed),
            ops_set: self.metrics_ops_set.load(Ordering::Relaxed),
            ops_del: self.metrics_ops_del.load(Ordering::Relaxed),
            flush_count: self.metrics_flush_count.load(Ordering::Relaxed),
            flush_errors: self.metrics_flush_errors.load(Ordering::Relaxed),
            flush_total_ns: self.metrics_flush_total_ns.load(Ordering::Relaxed),
        }
    }

    /// Утилита: записывает `u32` в формате big-endian.
    ///
    /// # Аргументы
    /// - `w` — writer (например, BufWriter или File).
    /// - `v` — значение u32.
    ///
    /// # Ошибки
    /// Возвращает ошибку записи.
    #[inline]
    fn write_32<W: Write>(
        w: &mut W,
        v: u32,
    ) -> io::Result<()> {
        w.write_all(&v.to_be_bytes())
    }

    /// Утилита: безопасно читает `u32` в формате big-endian из буфера.
    ///
    /// # Аргументы
    /// - `buf` — байтовый буфер.
    /// - `pos` — текущая позиция чтения (модифицируется).
    ///
    /// # Ошибки
    /// Возвращает ошибку при недостатке байт.
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
    fn maybe_flush(&self) -> io::Result<()> {
        match self.policy {
            SyncPolicy::Always => {
                let start = Instant::now();
                let mut buf = self.writer.lock().unwrap();
                let res = buf.flush();
                let dur = start.elapsed().as_nanos() as u64;
                self.metrics_flush_count.fetch_add(1, Ordering::Relaxed);
                self.metrics_flush_total_ns
                    .fetch_add(dur, Ordering::Relaxed);
                if res.is_err() {
                    self.metrics_flush_errors.fetch_add(1, Ordering::Relaxed);
                }
                res
            }
            SyncPolicy::EverySec => Ok(()),
            SyncPolicy::No => Ok(()),
        }
    }

    /// Немедленно сбрасывает буфер на диск.
    /// Используется при политике `Always`, когда достигнут порог batch.
    /// Обновляет метрики (время, количество, ошибки).
    ///
    /// # Ошибки
    /// Возвращает ошибку flush.
    fn flush_immediate(&self) -> io::Result<()> {
        let start = Instant::now();
        let mut buf = self.writer.lock().unwrap();
        let res = buf.flush();
        let dur = start.elapsed().as_nanos() as u64;
        self.metrics_flush_count.fetch_add(1, Ordering::Relaxed);
        self.metrics_flush_total_ns
            .fetch_add(dur, Ordering::Relaxed);
        if res.is_err() {
            self.metrics_flush_errors.fetch_add(1, Ordering::Relaxed);
        }
        res
    }

    /// Адаптивно подстраивает размер батча для flush:
    ///
    /// - Если активности не было > 5 секунд, уменьшает batch в 2 раза (не ниже 4).
    /// - При активности < 1 секунды назад может увеличить (не реализовано).
    ///
    /// Используется только при политике `Always`.
    fn adjust_batch_size(&self) {
        let now = Instant::now().elapsed().as_secs();
        let last = self.last_activity.load(Ordering::Relaxed);
        let mut cur = self.batch_size.load(Ordering::Relaxed);
        if now.saturating_sub(last) > 5 && cur > 4 {
            cur = (cur / 2).max(4);
            self.batch_size.store(cur, Ordering::Relaxed);
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
        // при drop отсылаем сигнал остановки и ждём потока
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

    /// Проверяет, что метрики операций и flush корректно считаются при SyncPolicy::Always.
    #[test]
    fn test_metrics_always_policy() -> io::Result<()> {
        let temp = NamedTempFile::new()?;
        let path = temp.path();
        let mut log = AofLog::open(path, SyncPolicy::Always)?;

        // Изначально все счётчики нулевые
        let m0 = log.metrics();
        assert_eq!(m0.ops_total, 0);
        assert_eq!(m0.ops_set, 0);
        assert_eq!(m0.ops_del, 0);
        assert_eq!(m0.flush_count, 0);
        assert_eq!(m0.flush_errors, 0);
        assert_eq!(m0.flush_total_ns, 0);

        // Делаем меньше BATCH_SIZE операций: flush не должен вызываться
        log.append_set(b"k1", b"v1")?;
        log.append_del(b"k1")?;

        // Проверяем, что flush_count по-прежнему 0
        let m1 = log.metrics();
        assert_eq!(m1.ops_total, 2, "должно быть 2 операций");
        assert_eq!(m1.ops_set, 1, "одна SET");
        assert_eq!(m1.ops_del, 1, "один DEL");
        assert_eq!(m1.flush_count, 0, "flush не должен быть вызван");
        assert_eq!(m1.flush_errors, 0, "ошибок flush быть не должно");
        assert_eq!(m1.flush_total_ns, 0, "время flush должно быть 0");

        for _i in 0..AofLog::INITIAL_BATCH {
            log.append_set(b"k", b"v")?;
        }

        let m2 = log.metrics();
        assert!(
            m2.flush_count >= 1,
            "должен произойти хотя бы один flush после BATCH_SIZE операций"
        );

        Ok(())
    }

    /// Проверяет, что при SyncPolicy::No flush не происходит автоматически.
    #[test]
    fn test_metrics_no_policy() -> io::Result<()> {
        let temp = NamedTempFile::new()?;
        let path = temp.path();
        let mut log = AofLog::open(path, SyncPolicy::No)?;

        log.append_set(b"k", b"v")?;
        log.append_del(b"k")?;

        let m = log.metrics();
        assert_eq!(m.ops_total, 2);
        assert_eq!(m.ops_set, 1);
        assert_eq!(m.ops_del, 1);
        // При No flush_count остаётся 0
        assert_eq!(m.flush_count, 0, "ни один flush не должен быть вызван");
        assert_eq!(m.flush_errors, 0);
        assert_eq!(m.flush_total_ns, 0);

        Ok(())
    }
}
