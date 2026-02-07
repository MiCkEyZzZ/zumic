use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Condvar, Mutex, RwLock,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use tempfile::NamedTempFile;

use crate::{engine::AofOp, ShardedIndex, StoreError, StoreResult};

/// Стратегия восстановления из AOF файла
#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    /// Воспроизводить только AOF с самого начала
    AofOnly,
    /// Загрузить последний снимок + воспроизвести инкрементальный AOF
    SnapshotPlusIncremental,
    /// Автоматический выбор стратегии на основании доступных данных
    Auto,
}

/// Конфигурация для управления компакцией AOF и созданием снимков
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Включить автоматическую фоновую компакцию
    pub auto_compaction_enabled: bool,
    /// Интервал между проверками на необходимость компакции (в секундах)
    pub compaction_check_interval: u64,
    /// Минимальный размер AOF-файла для запуска компакции (в байтах)
    pub min_file_size_threshold: u64,
    /// Максимальный размер AOF-файла, после которого компакция запускается
    /// принудительно (в байтах)
    pub max_file_size_threshold: u64,
    /// Минимальное количество операций для запуска компакции
    pub min_ops_threshold: usize,
    /// Максимальное время с момента последней компакции (в секундах)
    pub max_time_threshold: u64,
    /// Включить создание снимков во время компакции
    pub enable_snapshots: bool,
    /// Каталог для хранения снимков
    pub snapshot_dir: PathBuf,
    /// Количество снимков, которые нужно хранить
    pub snapshot_retention_count: usize,
    /// Уровень сжатия снимков (0–9, 0 = без сжатия)
    pub snapshot_compression: u8,
}

/// Метрики состояния компакции и создания снимков
#[derive(Debug, Default, Clone)]
pub struct CompactionMetrics {
    /// Общее количество выполненных компакций
    pub compactions_total: usize,
    /// Количество неудачных компакций
    pub compactions_failed: usize,
    /// Общее время, затраченное на компакцию (нс)
    pub compaction_total_ns: u64,
    /// Время последней компакции (секунды Unix-эпохи)
    pub last_compaction_time: u64,
    /// Количество созданных снимков
    pub snapshots_created: usize,
    /// Общее время создания снимков (нс)
    pub snapshot_total_ns: u64,
    /// Средний коэффициент уменьшения размера файла (0.0–1.0)
    pub avg_size_reduction: f64,
    /// Текущий размер AOF-файла
    pub current_aof_size: u64,
    /// Размер последнего снимка
    pub last_snapshot_size: u64,
}

/// Информация о созданном снимке
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    /// Путь к файлу снимка
    pub path: PathBuf,
    /// Время создания (Unix-время в секундах)
    pub timestamp: u64,
    /// Кол-во ключей в снимке
    pub key_count: usize,
    /// Размер файла в байтах
    pub file_size: u64,
    /// Контрольная сумма данных снимка
    pub checksum: u64,
}

/// Менеджер для обработки компакции AOF и управления снимками
pub struct CompactionManager {
    /// Путь к AOF-файлу
    aof_path: PathBuf,
    /// Конфигурация компакции
    #[allow(dead_code)]
    config: CompactionConfig,
    /// Ссылка на шардинг-индекс для доступа к данным
    index: Arc<ShardedIndex<Vec<u8>>>,
    /// Хэндл потока компакции
    compaction_handle: Option<JoinHandle<()>>,
    /// Канал для остановки потока компакции
    stop_tx: Option<Sender<()>>,
    /// Метрики
    metrics: Arc<RwLock<CompactionMetrics>>,
    /// Ручной триггер компакции
    manual_trigger: Arc<(Mutex<bool>, Condvar)>,
    /// Флаг завершения работы
    shutdown: Arc<AtomicBool>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl CompactionManager {
    /// Создание нового менеджера компакции
    pub fn new(
        aof_path: PathBuf,
        config: CompactionConfig,
        index: Arc<ShardedIndex<Vec<u8>>>,
    ) -> StoreResult<Self> {
        if config.enable_snapshots {
            fs::create_dir_all(&config.snapshot_dir).map_err(StoreError::Io)?;
        }

        let manager = CompactionManager {
            aof_path,
            config,
            index,
            compaction_handle: None,
            stop_tx: None,
            metrics: Arc::new(RwLock::new(CompactionMetrics::default())),
            manual_trigger: Arc::new((Mutex::new(false), Condvar::new())),
            shutdown: Arc::new(AtomicBool::new(false)),
        };

        Ok(manager)
    }

    /// Запускает фоновый поток компакции, если автокомпакция включена.
    pub fn start(&mut self) -> StoreResult<()> {
        if !self.config.auto_compaction_enabled {
            return Ok(());
        }

        let (tx, rx) = mpsc::channel();
        self.stop_tx = Some(tx);

        let aof_path = self.aof_path.clone();
        let config = self.config.clone();
        let index = Arc::clone(&self.index);
        let metrics = Arc::clone(&self.metrics);
        let manual_trigger = Arc::clone(&self.manual_trigger);
        let shutdown = Arc::clone(&self.shutdown);

        let handle = thread::spawn(move || {
            Self::compaction_worker(
                aof_path,
                config,
                index,
                metrics,
                manual_trigger,
                shutdown,
                rx,
            );
        });

        self.compaction_handle = Some(handle);
        Ok(())
    }

    /// Ручной триггер для немедленного запуска компакции.
    pub fn trigger_compaction(&self) {
        let (lock, cvar) = &*self.manual_trigger;
        let mut triggered = lock.lock().unwrap();
        *triggered = true;
        cvar.notify_one();
    }

    /// Возвращает текущее состояние метрик компакции.
    pub fn metrics(&self) -> CompactionMetrics {
        self.metrics.read().unwrap().clone()
    }

    /// Создает снимок текущего состояния базы данных.
    pub fn create_snapshot(&self) -> StoreResult<SnapshotInfo> {
        let start_time = Instant::now();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let snapshot_name = format!("snapshot_{timestamp}.db");
        let snapshot_path = self.config.snapshot_dir.join(snapshot_name);

        // Collect all data from shards (сбор в память — OK пока без streaming)
        let mut all_data = HashMap::new();
        let mut total_keys = 0usize;

        for shard in self.index.all_shards() {
            shard.read(|data| {
                for (key, value) in data.iter() {
                    all_data.insert(key.clone(), value.clone());
                    total_keys += 1;
                }
            });
        }

        // Запись файла моментального снимка
        let mut temp_file =
            NamedTempFile::new_in(&self.config.snapshot_dir).map_err(StoreError::Io)?;

        // Запись волшебного заголовка для моментальных снимков
        temp_file.write_all(b"SNAP")?;

        // Запись метаданных: timestamp + key_count
        Self::write_u64(&mut temp_file, timestamp)?;
        Self::write_u64(&mut temp_file, total_keys as u64)?;

        // Запись данных
        let mut checksum = 0u64;
        for (key, value) in &all_data {
            Self::write_u32(&mut temp_file, key.len() as u32)?;
            temp_file.write_all(key)?;
            Self::write_u32(&mut temp_file, value.len() as u32)?;
            temp_file.write_all(value)?;

            // Простая контрольная сумма
            checksum = checksum.wrapping_add(key.len() as u64);
            checksum = checksum.wrapping_add(value.len() as u64);
        }

        temp_file.flush()?;
        // синхронизируем данные на диск перед persist
        temp_file.as_file_mut().sync_all().map_err(StoreError::Io)?;
        let file_size = temp_file.as_file().metadata()?.len();

        // Атомарное перемещение в конечное местоположение
        temp_file
            .persist(&snapshot_path)
            .map_err(|e| StoreError::Io(e.error))?;

        let snapshot_info = SnapshotInfo {
            path: snapshot_path,
            timestamp,
            key_count: total_keys,
            file_size,
            checksum,
        };

        // Обновить показатели
        {
            let mut metrics = self.metrics.write().unwrap();
            metrics.snapshots_created += 1;
            metrics.snapshot_total_ns += start_time.elapsed().as_nanos() as u64;
            metrics.last_snapshot_size = file_size;
        }

        // Очистка старых снимков
        self.cleanup_old_snapshots()?;

        Ok(snapshot_info)
    }

    /// Загружает состояние из указанного снимка.
    pub fn load_snapshot(
        &self,
        snapshot_path: &Path,
    ) -> StoreResult<usize> {
        let mut file = File::open(snapshot_path).map_err(StoreError::Io)?;

        // Прочитать и проверить магическую подпись
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic).map_err(StoreError::Io)?;
        if &magic != b"SNAP" {
            return Err(StoreError::InvalidData(
                "Invalid snapshot magic".to_string(),
            ));
        }

        // Прочитать метаданные
        let _timestamp = Self::read_u64(&mut file)?;
        let key_count = Self::read_u64(&mut file)? as usize;

        let mut loaded_count = 0usize;

        // Прочитать записи
        for _ in 0..key_count {
            let key_len = Self::read_u32(&mut file)? as usize;
            let mut key = vec![0u8; key_len];
            file.read_exact(&mut key).map_err(StoreError::Io)?;

            let value_len = Self::read_u32(&mut file)? as usize;
            let mut value = vec![0u8; value_len];
            file.read_exact(&mut value).map_err(StoreError::Io)?;

            // Вставить в соответствующий шард
            let shard = self.index.get_shard(&key);
            shard.write(|data| {
                let was_new = !data.contains_key(&key);
                data.insert(key, value);

                if was_new {
                    if let Some(ref metrics) = shard.metrics {
                        metrics.increment_key_count();
                    }
                }
            });
            loaded_count += 1;
        }

        Ok(loaded_count)
    }

    /// Находит последний созданный снимок.
    pub fn find_latest_snapshot(&self) -> StoreResult<Option<SnapshotInfo>> {
        if !self.config.enable_snapshots {
            return Ok(None);
        }

        let snapshot_dir = &self.config.snapshot_dir;
        if !snapshot_dir.exists() {
            return Ok(None);
        }

        let mut snapshots = Vec::new();

        for entry in fs::read_dir(snapshot_dir).map_err(StoreError::Io)? {
            let entry = entry.map_err(StoreError::Io)?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "db") {
                if let Some(info) = self.parse_snapshot_info(&path)? {
                    snapshots.push(info);
                }
            }
        }

        snapshots.sort_by_key(|s| s.timestamp);
        Ok(snapshots.into_iter().last())
    }

    /// Плавное завершение работы менеджера компакции.
    pub fn shutdown(&mut self) -> StoreResult<()> {
        self.shutdown.store(true, Ordering::Relaxed);

        // Останавливаем поток компакции
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }

        if let Some(handle) = self.compaction_handle.take() {
            if let Err(e) = handle.join() {
                eprintln!("Failed to join compaction thread: {e:?}");
            }
        }

        Ok(())
    }

    /// Прочитать метаданные снимка из файла (по имени/metadata)
    fn parse_snapshot_info(
        &self,
        path: &Path,
    ) -> StoreResult<Option<SnapshotInfo>> {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };

        let metadata = file.metadata().map_err(StoreError::Io)?;
        let file_size = metadata.len();

        // Извлечь timestamp из имени файла
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        let timestamp = stem
            .strip_prefix("snapshot_")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        Ok(Some(SnapshotInfo {
            path: path.to_path_buf(),
            timestamp,
            key_count: 0, // Для точного значения нужно читать файл; можно доработать
            file_size,
            checksum: 0,
        }))
    }

    /// Фоновый воркер, который периодически проверяет необходимость компакции
    /// и запускает её при необходимости.
    fn compaction_worker(
        aof_path: PathBuf,
        config: CompactionConfig,
        index: Arc<ShardedIndex<Vec<u8>>>,
        metrics: Arc<RwLock<CompactionMetrics>>,
        manual_trigger: Arc<(Mutex<bool>, Condvar)>,
        shutdown: Arc<AtomicBool>,
        stop_rx: Receiver<()>,
    ) {
        let (lock, cvar) = &*manual_trigger;

        loop {
            // Проверяем флаг завершения
            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            // Ждём триггер или таймаут
            let mut triggered = lock.lock().unwrap();
            let wait_result = cvar
                .wait_timeout(
                    triggered,
                    Duration::from_secs(config.compaction_check_interval),
                )
                .unwrap();

            triggered = wait_result.0;
            let timed_out = wait_result.1.timed_out();

            // Проверяем сигнал на стоп
            if stop_rx.try_recv().is_ok() {
                break;
            }

            let should_compact =
                *triggered || (timed_out && Self::should_compact(&aof_path, &config, &metrics));

            if should_compact {
                *triggered = false;
                drop(triggered);

                if let Err(e) = Self::perform_compaction(&aof_path, &config, &index, &metrics) {
                    eprintln!("Compaction failed: {e:?}");
                    let mut m = metrics.write().unwrap();
                    m.compactions_failed += 1;
                }
            }
        }
    }

    /// Проверяет, нужно ли запускать компакцию на основе текущих условий.
    fn should_compact(
        aof_path: &Path,
        config: &CompactionConfig,
        metrics: &RwLock<CompactionMetrics>,
    ) -> bool {
        let file_size = fs::metadata(aof_path).map(|m| m.len()).unwrap_or(0);

        let metrics_guard = metrics.read().unwrap();
        let last_compaction = metrics_guard.last_compaction_time;
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        drop(metrics_guard);

        // Триггеры по размеру
        if file_size >= config.max_file_size_threshold {
            return true;
        }

        if file_size >= config.min_file_size_threshold {
            // Триггер по времени
            if current_time.saturating_sub(last_compaction) >= config.max_time_threshold {
                return true;
            }
        }

        false
    }

    /// Выполняет компакцию — создает новый компактный AOF-файл с актуальными
    /// данными.
    fn perform_compaction(
        aof_path: &Path,
        _config: &CompactionConfig,
        index: &ShardedIndex<Vec<u8>>,
        metrics: &RwLock<CompactionMetrics>,
    ) -> StoreResult<()> {
        let start_time = Instant::now();

        // Размер файла до компакции
        let old_size = fs::metadata(aof_path).map(|m| m.len()).unwrap_or(0);

        // Сбор всех текущих данных из фрагментов (в память)
        let mut live_data = HashMap::new();
        for shard in index.all_shards() {
            shard.read(|data| {
                for (key, value) in data.iter() {
                    live_data.insert(key.clone(), value.clone());
                }
            });
        }

        // Создание новых AOF только с текущими данными
        let mut temp_aof =
            NamedTempFile::new_in(aof_path.parent().unwrap_or_else(|| Path::new(".")))
                .map_err(StoreError::Io)?;

        // Записываем магическую подпись
        temp_aof.write_all(b"AOF1")?;

        // Запись всех текущих записей как SET операции
        for (key, value) in &live_data {
            temp_aof.write_all(&[AofOp::Set as u8])?;
            Self::write_u32(&mut temp_aof, key.len() as u32)?;
            temp_aof.write_all(key)?;
            Self::write_u32(&mut temp_aof, value.len() as u32)?;
            temp_aof.write_all(value)?;
        }

        temp_aof.flush()?;
        // синхронизируем перед заменой
        temp_aof.as_file_mut().sync_all().map_err(StoreError::Io)?;

        // Атомарно заменяем AOF-файл
        temp_aof
            .persist(aof_path)
            .map_err(|e| StoreError::Io(e.error))?;

        // Новый размер файла
        let new_size = fs::metadata(aof_path).map(|m| m.len()).unwrap_or(0);

        // Обновляем метрики
        {
            let mut m = metrics.write().unwrap();
            m.compactions_total += 1;
            m.compaction_total_ns += start_time.elapsed().as_nanos() as u64;
            m.last_compaction_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            m.current_aof_size = new_size;

            // Обновляем средний коэффициент уменьшения
            if old_size > 0 {
                let reduction = 1.0 - (new_size as f64 / old_size as f64);
                let total_compactions = m.compactions_total as f64;
                m.avg_size_reduction = (m.avg_size_reduction * (total_compactions - 1.0)
                    + reduction)
                    / total_compactions;
            }
        }

        Ok(())
    }

    /// Удаляет старые снимки в соответствии с политикой retention.
    fn cleanup_old_snapshots(&self) -> StoreResult<()> {
        if !self.config.enable_snapshots {
            return Ok(());
        }

        let snapshot_dir = &self.config.snapshot_dir;
        let mut snapshots = Vec::new();

        for entry in fs::read_dir(snapshot_dir).map_err(StoreError::Io)? {
            let entry = entry.map_err(StoreError::Io)?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "db") {
                if let Some(info) = self.parse_snapshot_info(&path)? {
                    snapshots.push(info);
                }
            }
        }

        // Сортируем по timestamp, сначала самые новые
        snapshots.sort_by_key(|s| std::cmp::Reverse(s.timestamp));

        // Удаляем старые снимки сверх retention
        for snapshot in snapshots
            .into_iter()
            .skip(self.config.snapshot_retention_count)
        {
            if let Err(e) = fs::remove_file(&snapshot.path) {
                eprintln!("Failed to remove old snapshot {:?}: {}", snapshot.path, e);
            }
        }

        Ok(())
    }

    /// Записывает 32-битное беззнаковое целое в переданный писатель в формате
    /// big-endian.
    fn write_u32<W: Write>(
        w: &mut W,
        v: u32,
    ) -> io::Result<()> {
        w.write_all(&v.to_be_bytes())
    }

    /// Записывает 64-битное беззнаковое целое в переданный писатель в формате
    /// big-endian.
    fn write_u64<W: Write>(
        w: &mut W,
        v: u64,
    ) -> io::Result<()> {
        w.write_all(&v.to_be_bytes())
    }

    /// Читает 32-битное беззнаковое целое из переданного читателя в формате
    /// big-endian.
    fn read_u32<R: Read>(r: &mut R) -> StoreResult<u32> {
        let mut buf = [0u8; 4];
        r.read_exact(&mut buf).map_err(StoreError::Io)?;
        Ok(u32::from_be_bytes(buf))
    }

    /// Читает 64-битное беззнаковое целое из переданного читателя в формате
    /// big-endian.
    fn read_u64<R: Read>(r: &mut R) -> StoreResult<u64> {
        let mut buf = [0u8; 8];
        r.read_exact(&mut buf).map_err(StoreError::Io)?;
        Ok(u64::from_be_bytes(buf))
    }
}

////////////////////////////////////////////////////////////////////////////////
// Общие реализации трейтов для CompactionConfig
////////////////////////////////////////////////////////////////////////////////

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            auto_compaction_enabled: true,
            compaction_check_interval: 60,              // 1 мин
            min_file_size_threshold: 64 * 1024 * 1024,  // 64 МБ
            max_file_size_threshold: 256 * 1024 * 1024, // 256 МБ
            min_ops_threshold: 100_000,
            max_time_threshold: 3600, // 1 ч
            enable_snapshots: true,
            snapshot_dir: PathBuf::from("snapshots"),
            snapshot_retention_count: 3,
            snapshot_compression: 6,
        }
    }
}

impl Drop for CompactionManager {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        sync::Arc,
        thread,
        time::{Duration, Instant},
    };

    use tempfile::tempdir;

    use super::*;

    /// Тест проверяет создание snapshot из одного индекса, затем загружает его
    /// в новый индекс.
    #[test]
    fn test_create_and_load_snapshot() -> Result<(), StoreError> {
        let tmp = tempdir().unwrap();
        let snapshot_dir = tmp.path().join("snaps");
        fs::create_dir_all(&snapshot_dir).unwrap();

        let config = CompactionConfig {
            enable_snapshots: true,
            snapshot_dir: snapshot_dir.clone(),
            ..Default::default()
        };

        let aof_path = tmp.path().join("aof.bin");
        // создаём пустой файл AOF (compaction использует путь, но не требует
        // содержимого для snapshot)
        File::create(&aof_path).unwrap();

        let index1 = Arc::new(ShardedIndex::new(crate::ShardingConfig::default()));

        // Вставляем пару ключей
        index1.insert(b"key1", b"v1".to_vec());
        index1.insert(b"key2", b"v2".to_vec());

        // Создаём менеджера и делаем snapshot
        let mgr1 = CompactionManager::new(aof_path.clone(), config.clone(), Arc::clone(&index1))?;
        let snapshot = mgr1.create_snapshot()?;
        assert_eq!(snapshot.key_count, 2);

        // Новый индекс + менеджер для загрузки snapshot
        let index2 = Arc::new(ShardedIndex::new(crate::ShardingConfig::default()));
        let mgr2 = CompactionManager::new(aof_path.clone(), config.clone(), Arc::clone(&index2))?;
        let latest = mgr2.find_latest_snapshot()?.expect("snapshot must exist");
        let loaded = mgr2.load_snapshot(&latest.path)?;
        assert_eq!(loaded, 2);

        // Проверяем значения в index2
        let got1 = index2.get(b"key1").expect("key1 present");
        assert_eq!(got1, b"v1".to_vec());
        let got2 = index2.get(b"key2").expect("key2 present");
        assert_eq!(got2, b"v2".to_vec());

        Ok(())
    }

    /// Тест проверяет стартуем ли фоновый воркер, триггерит компакцию, ждёт
    /// изменения метрик и файла aof.
    #[test]
    fn test_compaction_thread_trigger_writes_aof_and_metrics() -> Result<(), StoreError> {
        let tmp = tempdir().unwrap();
        let snapshot_dir = tmp.path().join("snaps");
        fs::create_dir_all(&snapshot_dir).unwrap();

        let config = CompactionConfig {
            auto_compaction_enabled: true,
            compaction_check_interval: 1,
            enable_snapshots: false,
            snapshot_dir: snapshot_dir.clone(),
            ..Default::default()
        };

        let aof_path = tmp.path().join("aof_test.aof");
        File::create(&aof_path).unwrap();

        let index = Arc::new(ShardedIndex::new(crate::ShardingConfig::default()));
        index.insert(b"k1", b"val1".to_vec());
        index.insert(b"k2", b"val2".to_vec());

        let mut mgr = CompactionManager::new(aof_path.clone(), config.clone(), Arc::clone(&index))?;
        mgr.start()?;
        // Триггерим компакцию вручную
        mgr.trigger_compaction();

        // Ждём до 5 секунд, проверяя метрики
        let start = Instant::now();
        let mut ok = false;
        while start.elapsed() < Duration::from_secs(5) {
            let metrics = mgr.metrics();
            if metrics.compactions_total > 0 {
                ok = true;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        // Останавливаем менеджера
        mgr.shutdown()?;
        assert!(ok, "compaction did not run within timeout");

        // Убедимся, что AOF файл был записан и не пуст
        let meta = fs::metadata(&aof_path).unwrap();
        assert!(meta.len() > 0);

        Ok(())
    }
}
