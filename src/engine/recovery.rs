use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    engine::compaction::{CompactionConfig, CompactionManager, RecoveryStrategy, SnapshotInfo},
    AofLog, ShardedIndex, StoreError, StoreResult,
};

/// Менеджер восстановления с расширенными возможностями по работе с AOF
pub struct RecoveryManager {
    /// Путь к AOF файлу
    aof_path: PathBuf,
    /// Опциональный менеджер компакции
    compaction_manager: Option<CompactionManager>,
    /// Стратегия восстановления
    recovery_strategy: RecoveryStrategy,
    /// Время последнего восстановления (Unix-время в секундах)
    last_recovery_time: AtomicU64,
    /// Количество ключей, восстановленных при последнем запуске
    keys_recovered: AtomicUsize,
    /// Продолжительность последнего восстановления в наносекундах
    recovery_duration_ns: AtomicU64,
}

/// Структура, содержащая статистику восстановления
#[derive(Debug, Default, Clone)]
pub struct RecoveryStats {
    /// Использованная стратегия восстановления (например, "aof_only",
    /// "snapshot_plus_aof")
    pub strategy_used: String,
    /// Был ли использован снимок
    pub snapshot_used: bool,
    /// Размер файла снимка в байтах
    pub snapshot_size_bytes: u64,
    /// Время создания снимка (Unix-время)
    pub snapshot_timestamp: u64,
    /// Размер AOF файла в байтах
    pub aof_size_bytes: u64,
    /// Всего времени восстановления в миллисекундах
    pub total_duration_ms: u64,
    /// Количество воспроизведённых операций из AOF
    pub operations_replayed: usize,
    /// Итоговое количество загруженных ключей
    pub keys_loaded: usize,
    /// Количество добавленных ключей в процессе восстановления
    pub keys_added: usize,
    /// Количество обновленных ключей
    pub keys_updated: usize,
    /// Количество удалённых ключей
    pub keys_deleted: usize,
}

/// Метрики производительности восстановления
#[derive(Debug, Clone)]
pub struct RecoveryMetrics {
    /// Время последнего восстановления (Unix-время)
    pub last_recovery_time: u64,
    /// Количество ключей, восстановленных за последнее восстановление
    pub keys_recovered: usize,
    /// Продолжительность последнего восстановления в наносекундах
    pub recovery_duration_ns: u64,
}

impl RecoveryManager {
    /// Создаёт новый менеджер восстановления.
    pub fn new(
        aof_path: PathBuf,
        _compaction_config: Option<CompactionConfig>,
        recovery_strategy: RecoveryStrategy,
    ) -> Self {
        Self {
            aof_path,
            compaction_manager: None,
            recovery_strategy,
            last_recovery_time: AtomicU64::new(0),
            keys_recovered: AtomicUsize::new(0),
            recovery_duration_ns: AtomicU64::new(0),
        }
    }

    /// Инициализация менеджера восстановления с шардингом и конфигурацией
    /// компакции.
    pub fn initialize(
        &mut self,
        index: Arc<ShardedIndex<Vec<u8>>>,
        compaction_config: CompactionConfig,
    ) -> StoreResult<()> {
        let mut manager = CompactionManager::new(self.aof_path.clone(), compaction_config, index)?;

        manager.start()?;
        self.compaction_manager = Some(manager);

        Ok(())
    }

    /// Выполняет восстановление данных в соответствии со стратегией.
    pub fn recover(
        &self,
        index: &ShardedIndex<Vec<u8>>,
    ) -> StoreResult<RecoveryStats> {
        let start_time = Instant::now();

        // Получаем stats как результат выбора стратегии
        let stats = match self.recovery_strategy {
            RecoveryStrategy::AofOnly => self.recover_from_aof_only(index)?,
            RecoveryStrategy::SnapshotPlusIncremental => {
                self.recover_from_snapshot_plus_aof(index)?
            }
            RecoveryStrategy::Auto => {
                // Try snapshot + AOF first, fallback to AOF only
                self.recover_from_snapshot_plus_aof(index)
                    .or_else(|_| self.recover_from_aof_only(index))?
            }
        };

        let duration = start_time.elapsed();
        let mut stats = stats;
        stats.total_duration_ms = duration.as_millis() as u64;

        // Обновляем метрики
        self.last_recovery_time.store(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            Ordering::Relaxed,
        );
        self.keys_recovered
            .store(stats.keys_loaded, Ordering::Relaxed);
        self.recovery_duration_ns
            .store(duration.as_nanos() as u64, Ordering::Relaxed);

        Ok(stats)
    }

    /// Восстановление только из AOF файла.
    fn recover_from_aof_only(
        &self,
        index: &ShardedIndex<Vec<u8>>,
    ) -> StoreResult<RecoveryStats> {
        let mut stats = RecoveryStats {
            strategy_used: "aof_only".to_string(),
            ..Default::default()
        };

        if !self.aof_path.exists() {
            return Ok(stats);
        }

        let file_size = std::fs::metadata(&self.aof_path)
            .map(|m| m.len())
            .unwrap_or(0);
        stats.aof_size_bytes = file_size;

        // Создаём временный AOF лог для повтора
        let mut aof_log = AofLog::open(
            &self.aof_path,
            super::SyncPolicy::No,
            super::CorruptionPolicy::Log,
        )?;

        aof_log
            .replay(|op, key, val| {
                let shard = index.get_shard(&key);

                shard.write(|data| match op {
                    super::AofOp::Set => {
                        if let Some(value) = val {
                            let was_new = !data.contains_key(&key);
                            data.insert(key.clone(), value);

                            stats.keys_loaded += 1;
                            if was_new {
                                stats.keys_added += 1;
                                if let Some(ref metrics) = shard.metrics {
                                    metrics.increment_key_count();
                                }
                            } else {
                                stats.keys_updated += 1;
                            }
                        }
                    }
                    super::AofOp::Del => {
                        if data.remove(&key).is_some() {
                            stats.keys_deleted += 1;
                            if let Some(ref metrics) = shard.metrics {
                                metrics.decrement_key_count();
                            }
                        }
                    }
                });

                stats.operations_replayed += 1;
            })
            .map_err(StoreError::Io)?;

        Ok(stats)
    }

    /// Восстановление с использованием снимка состояния и инкрементального AOF.
    fn recover_from_snapshot_plus_aof(
        &self,
        index: &ShardedIndex<Vec<u8>>,
    ) -> StoreResult<RecoveryStats> {
        let mut stats = RecoveryStats {
            strategy_used: "snapshot_plus_aof".to_string(),
            ..Default::default()
        };

        // Поиск и загрузка последнего моментального снимка
        if let Some(ref manager) = self.compaction_manager {
            if let Some(snapshot_info) = manager.find_latest_snapshot()? {
                stats.snapshot_used = true;
                stats.snapshot_size_bytes = snapshot_info.file_size;
                stats.snapshot_timestamp = snapshot_info.timestamp;

                let loaded_keys = manager.load_snapshot(&snapshot_info.path)?;
                stats.keys_loaded = loaded_keys;
                stats.keys_added = loaded_keys;
            }
        }

        // Воспроизведение AOF для любых операций после моментального снимка
        if self.aof_path.exists() {
            let aof_stats = self.recover_from_aof_only(index)?;

            // Статистика слияния (повтор AOF может перезаписать некоторые данные
            // моментального снимка)
            stats.aof_size_bytes = aof_stats.aof_size_bytes;
            stats.operations_replayed = aof_stats.operations_replayed;

            // При поэтапном подсчете мы больше заботимся об окончательных подсчетах
            stats.keys_loaded = aof_stats.keys_loaded;
            stats.keys_added = aof_stats.keys_added;
            stats.keys_updated = aof_stats.keys_updated;
            stats.keys_deleted = aof_stats.keys_deleted;
        }

        Ok(stats)
    }

    /// Получить ссылку на менеджер компакции, если он инициализирован.
    pub fn compaction_manager(&self) -> Option<&CompactionManager> {
        self.compaction_manager.as_ref()
    }

    /// Получить изменяемую ссылку на менеджер компакции, если он
    /// инициализирован.
    pub fn compaction_manager_mut(&mut self) -> Option<&mut CompactionManager> {
        self.compaction_manager.as_mut()
    }

    /// Запустить ручную компакцию через менеджер.
    pub fn trigger_compaction(&self) -> StoreResult<()> {
        if let Some(ref manager) = self.compaction_manager {
            manager.trigger_compaction();
            Ok(())
        } else {
            Err(StoreError::InvalidOperation(
                "Compaction manager not initialized".to_string(),
            ))
        }
    }

    /// Создать снимок вручную.
    pub fn create_snapshot(&self) -> StoreResult<SnapshotInfo> {
        if let Some(ref manager) = self.compaction_manager {
            manager.create_snapshot()
        } else {
            Err(StoreError::InvalidOperation(
                "Compaction manager not initialized".to_string(),
            ))
        }
    }

    /// Получить метрики восстановления.
    pub fn recovery_metrics(&self) -> RecoveryMetrics {
        RecoveryMetrics {
            last_recovery_time: self.last_recovery_time.load(Ordering::Relaxed),
            keys_recovered: self.keys_recovered.load(Ordering::Relaxed),
            recovery_duration_ns: self.recovery_duration_ns.load(Ordering::Relaxed),
        }
    }

    /// Корректно завершить работу менеджера восстановления.
    pub fn shutdown(&mut self) -> StoreResult<()> {
        if let Some(ref mut manager) = self.compaction_manager {
            manager.shutdown()?;
        }
        Ok(())
    }
}

impl RecoveryStats {
    /// Рассчитать скорость восстановления ключей (ключей в секунду)
    pub fn recovery_rate_keys_per_sec(&self) -> f64 {
        if self.total_duration_ms == 0 {
            0.0
        } else {
            (self.keys_loaded as f64 * 1000.0) / self.total_duration_ms as f64
        }
    }

    /// Рассчитать скорость воспроизведения операций (операций в секунду)
    pub fn operation_rate_ops_per_sec(&self) -> f64 {
        if self.total_duration_ms == 0 {
            0.0
        } else {
            (self.operations_replayed as f64 * 1000.0) / self.total_duration_ms as f64
        }
    }

    /// Получить общее количество обработанных данных в байтах
    pub fn total_bytes_processed(&self) -> u64 {
        self.snapshot_size_bytes + self.aof_size_bytes
    }

    /// Рассчитать скорость обработки данных в мегабайтах в секунду (MB/s)
    pub fn data_rate_mbps(&self) -> f64 {
        if self.total_duration_ms == 0 {
            0.0
        } else {
            let total_mb = self.total_bytes_processed() as f64 / (1024.0 + 1024.0);
            (total_mb * 1000.0) / self.total_duration_ms as f64
        }
    }
}

impl Drop for RecoveryManager {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tempfile::tempdir;

    use super::*;
    use crate::{
        engine::{compaction::RecoveryStrategy, recovery::RecoveryManager, CorruptionPolicy},
        AofLog, ShardedIndex, ShardingConfig, SyncPolicy,
    };

    fn make_index() -> Arc<ShardedIndex<Vec<u8>>> {
        let cfg = ShardingConfig {
            num_shards: 4,
            enable_metrics: true,
            slow_operation_threshold_us: 1000,
        };
        Arc::new(ShardedIndex::new(cfg))
    }

    /// Тест проверяет, что при восстановлении из пустого AOF файл не приводит к
    /// ошибкам и не загружает ключи
    #[test]
    fn test_recover_from_aof_only_empty_file() {
        let dir = tempdir().unwrap();
        let aof_path = dir.path().join("appendonly.aof");

        // пустого файла нет
        let mgr = RecoveryManager::new(aof_path.clone(), None, RecoveryStrategy::AofOnly);
        let index = make_index();
        let stats = mgr.recover(&index).unwrap();

        assert_eq!(stats.strategy_used, "aof_only");
        assert_eq!(stats.keys_loaded, 0);
        assert_eq!(stats.operations_replayed, 0);
    }

    /// Тест проверяет, что восстановление из AOF с операциями корректно
    /// воспроизводит set и del
    #[test]
    fn test_recover_from_aof_only_with_ops() {
        let dir = tempdir().unwrap();
        let aof_path = dir.path().join("appendonly.aof");

        // создаём AOF через настоящий AofLog
        {
            let mut log =
                AofLog::open(&aof_path, SyncPolicy::Always, CorruptionPolicy::Log).unwrap();
            log.append_set(b"key", b"value").unwrap();
            log.append_del(b"key").unwrap();
            drop(log); // закрываем файл, чтобы всё сбросилось
        }

        let mgr = RecoveryManager::new(aof_path.clone(), None, RecoveryStrategy::AofOnly);
        let index = make_index();
        let stats = mgr.recover(&index).unwrap();

        assert_eq!(stats.strategy_used, "aof_only");
        assert!(stats.operations_replayed >= 2);
        assert!(stats.keys_added >= 1);
        assert!(stats.keys_deleted >= 1);
    }

    /// Тест проверяет, что метрики восстановления корректно обновляются после
    /// replay AOF
    #[test]
    fn test_recovery_metrics_updates() {
        let dir = tempdir().unwrap();
        let aof_path = dir.path().join("appendonly.aof");

        // создаём валидный бинарный AOF через AofLog
        {
            let mut log =
                AofLog::open(&aof_path, SyncPolicy::Always, CorruptionPolicy::Log).unwrap();
            log.append_set(b"key", b"value").unwrap();
            // при необходимости можно добавить и append_del
            drop(log); // гарантируем flush/закрытие
        }

        let mgr = RecoveryManager::new(aof_path.clone(), None, RecoveryStrategy::AofOnly);
        let index = make_index();
        let stats = mgr.recover(&index).unwrap();

        let metrics = mgr.recovery_metrics();
        assert!(metrics.last_recovery_time > 0);
        assert_eq!(metrics.keys_recovered, stats.keys_loaded);
        assert!(metrics.recovery_duration_ns > 0);
    }

    /// Тест проверяет, что вызов trigger_compaction без инициализации
    /// RecoveryManager возвращает ошибку
    #[test]
    fn test_trigger_compaction_without_init() {
        let dir = tempdir().unwrap();
        let aof_path = dir.path().join("appendonly.aof");

        let mgr = RecoveryManager::new(aof_path, None, RecoveryStrategy::AofOnly);
        let res = mgr.trigger_compaction();
        assert!(matches!(res, Err(StoreError::InvalidOperation(_))));
    }

    /// Тест проверяет, что вызов create_snapshot без инициализации
    /// RecoveryManager возвращает ошибку
    #[test]
    fn test_create_snapshot_without_init() {
        let dir = tempdir().unwrap();
        let aof_path = dir.path().join("appendonly.aof");

        let mgr = RecoveryManager::new(aof_path, None, RecoveryStrategy::AofOnly);
        let res = mgr.create_snapshot();
        assert!(matches!(res, Err(StoreError::InvalidOperation(_))));
    }

    /// Тест проверяет, что метод recovery_rate_keys_per_sec правильно
    /// рассчитывает скорость восстановления
    #[test]
    fn test_recovery_rate_keys_per_sec() {
        let stats = RecoveryStats {
            keys_loaded: 100,
            total_duration_ms: 2000, // если нужно установить другое поле
            ..Default::default()
        };
        let rate = stats.recovery_rate_keys_per_sec();
        assert!((rate - 50.0).abs() < f64::EPSILON);
    }
}
