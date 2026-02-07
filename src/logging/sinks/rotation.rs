use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::SystemTime,
};

use flate2::{write::GzEncoder, Compression};

/// File naming strategy.
#[derive(Debug, Clone, Default)]
pub enum FileNaming {
    /// Simple: zumic.log
    Simple,
    /// Dated: zumic-2025-10-06.log
    #[default]
    Dated,
    /// Sequential: zumic-001.log, zumic-002.log
    Sequential,
    /// Full: zumic-2025-10-06-001.log
    Full,
}

/// Метрики ротации файлов.
#[derive(Debug, Default)]
pub struct RotationMetrics {
    /// Кол-во событий ротаций
    pub rotation_count: AtomicU64,
    /// Кол-во сжатых файлов
    pub compressed_count: AtomicU64,
    /// Кол-во удалённых файлов (retention)
    pub deleted_count: AtomicU64,
    /// Общий размер удалённых файлов (байты)
    pub deleted_bytes: AtomicU64,
}

#[derive(Debug, Clone, Copy)]
pub struct RotationStats {
    pub rotation_count: u64,
    pub compressed_count: u64,
    pub deleted_count: u64,
    pub deleted_bytes: u64,
}

/// Retention policy для автоматического удаления старых файлов.
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// Максимальный возраст файлов (дни)
    pub max_age_days: Option<u32>,
    /// Максимальное количество файлов
    pub max_files: Option<usize>,
    /// Максимальный общий размер (байты)
    pub max_total_size_bytes: Option<u64>,
}

/// Size-based rotation writer.
pub struct SizeRotatingWriter {
    base_path: PathBuf,
    max_size: u64,
    current_size: Arc<Mutex<u64>>,
    current_file: Arc<Mutex<Option<File>>>,
    metrics: Arc<RotationMetrics>,
    naming: FileNaming,
    sequence: Arc<Mutex<usize>>,
}

impl RotationMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_rotation(&self) {
        self.rotation_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_compression(&self) {
        self.compressed_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_deletion(
        &self,
        size: u64,
    ) {
        self.deleted_count.fetch_add(1, Ordering::Relaxed);
        self.deleted_bytes.fetch_add(size, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> RotationStats {
        RotationStats {
            rotation_count: self.rotation_count.load(Ordering::Relaxed),
            compressed_count: self.compressed_count.load(Ordering::Relaxed),
            deleted_count: self.deleted_count.load(Ordering::Relaxed),
            deleted_bytes: self.deleted_bytes.load(Ordering::Relaxed),
        }
    }
}

impl RetentionPolicy {
    pub fn should_delete(
        &self,
        file_path: &Path,
    ) -> bool {
        // Проверяем возраста файла.
        if let Some(max_age) = self.max_age_days {
            if let Ok(metadata) = fs::metadata(file_path) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(duration) = SystemTime::now().duration_since(modified) {
                        let days = duration.as_secs() / 86400;
                        if days > max_age as u64 {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

impl SizeRotatingWriter {
    pub fn new(
        base_path: PathBuf,
        max_size_mb: u64,
        naming: FileNaming,
        metrics: Arc<RotationMetrics>,
    ) -> io::Result<Self> {
        let max_size = max_size_mb * 1024 * 1024;

        let writer = Self {
            base_path: base_path.clone(),
            max_size,
            current_size: Arc::new(Mutex::new(0)),
            current_file: Arc::new(Mutex::new(None)),
            metrics,
            naming,
            sequence: Arc::new(Mutex::new(0)),
        };

        // Создаём начальный файл
        writer.rotate()?;

        Ok(writer)
    }

    pub fn rotate(&self) -> io::Result<()> {
        let new_path = self.get_next_filename();
        let new_file = File::create(&new_path)?;

        // Заменяем текущий файл
        let mut current = self.current_file.lock().unwrap();
        *current = Some(new_file);

        // Сбрасываем размер
        let mut size = self.current_size.lock().unwrap();
        *size = 0;

        // Записываем метрику
        self.metrics.record_rotation();
        tracing::debug!(
            path = %new_path.display(),
            "Log file rotated"
        );
        Ok(())
    }

    pub fn write_all(
        &self,
        buf: &[u8],
    ) -> io::Result<()> {
        let buf_len = buf.len() as u64;

        // Проверяем, нужна ли ротация
        {
            let size = self.current_size.lock().unwrap();
            if *size + buf_len > self.max_size {
                drop(size);
                self.rotate()?;
            }
        }

        // Записываем в файл
        let mut file_guard = self.current_file.lock().unwrap();
        if let Some(ref mut file) = *file_guard {
            file.write_all(buf)?;
            file.flush()?;

            // Обновляем размер
            let mut size = self.current_size.lock().unwrap();
            *size += buf_len;
        }

        Ok(())
    }

    fn get_next_filename(&self) -> PathBuf {
        let mut seq = self.sequence.lock().unwrap();
        let current_seq = *seq; // берем текущее значение
        *seq += 1; // инкрементируем после использования

        let base_name = self
            .base_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("zumic");

        let extension = self
            .base_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("log");

        match self.naming {
            FileNaming::Simple => self.base_path.clone(),
            FileNaming::Dated => {
                let date = chrono::Local::now().format("%Y-%m-%d");
                self.base_path
                    .with_file_name(format!("{base_name}-{date}.{extension}"))
            }
            FileNaming::Sequential => self.base_path.with_file_name(format!(
                "{}-{:03}.{}",
                base_name,
                current_seq + 1,
                extension
            )),
            FileNaming::Full => {
                let date = chrono::Local::now().format("%Y-%m-%d");
                self.base_path.with_file_name(format!(
                    "{}-{}-{:03}.{}",
                    base_name,
                    date,
                    current_seq + 1,
                    extension
                ))
            }
        }
    }
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_age_days: Some(30),
            max_files: None,
            max_total_size_bytes: None,
        }
    }
}

/// Сжатие старых log файлов в gzip.
pub fn compress_log_file(
    path: &Path,
    metrics: &Arc<RotationMetrics>,
) -> io::Result<PathBuf> {
    let gz_path = path.with_extension("log.gz");

    // Читаем исходный файл
    let input = fs::read(path)?;

    // Сжимаем
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&input)?;
    let compressed = encoder.finish()?;

    // Записываем сжатые файлы
    fs::write(&gz_path, &compressed)?;

    // Удаляем исходный
    fs::remove_file(path)?;

    // Метрика
    metrics.record_compression();

    tracing::info!(
        original = %path.display(),
        compressed = %gz_path.display(),
        original_size = input.len(),
        compressed_size = compressed.len(),
        ratio = format!("{:.1}%", (compressed.len() as f64 / input.len() as f64) * 100.0),
        "Log file compressed"
    );

    Ok(gz_path)
}

/// Применение retention policy к директории с логами.
pub fn apply_retention_policy(
    log_dir: &Path,
    policy: &RetentionPolicy,
    metrics: &Arc<RotationMetrics>,
) -> io::Result<()> {
    let entries = fs::read_dir(log_dir)?;
    let mut files: Vec<(PathBuf, SystemTime, u64)> = Vec::new();

    // Собираем информацию о файлах
    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Ok(metadata) = fs::metadata(&path) {
                if let Ok(modified) = metadata.modified() {
                    files.push((path, modified, metadata.len()));
                }
            }
        }
    }

    // Сортируем по времени (старые первые)
    files.sort_by_key(|(_, modified, _)| *modified);

    // Удаление по возрасту
    if let Some(max_age) = policy.max_age_days {
        let now = SystemTime::now();
        let max_duration = std::time::Duration::from_secs(max_age as u64 * 86400);

        for (path, modified, size) in &files {
            if let Ok(duration) = now.duration_since(*modified) {
                if duration > max_duration {
                    fs::remove_file(path)?;
                    metrics.record_deletion(*size);

                    tracing::info!(
                        path = %path.display(),
                        age_days = duration.as_secs() / 86400,
                        "Old log file deleted"
                    );
                }
            }
        }
    }

    // Удаление по кол-ву файлов
    if let Some(max_files) = policy.max_files {
        if files.len() > max_files {
            let to_delete = files.len() - max_files;
            for (path, _, size) in files.iter().take(to_delete) {
                fs::remove_file(path)?;
                metrics.record_deletion(*size);

                tracing::info!(
                    path = %path.display(),
                    "Excess log file deleted (max_files exceeded)"
                );
            }
        }
    }

    // Удаление по общему размеру
    if let Some(max_total) = policy.max_total_size_bytes {
        let total_size: u64 = files.iter().map(|(_, _, size)| size).sum();

        if total_size > max_total {
            let mut deleted_size = 0u64;
            for (path, _, size) in &files {
                if total_size - deleted_size <= max_total {
                    break;
                }

                fs::remove_file(path)?;
                deleted_size += *size;
                metrics.record_deletion(*size);

                tracing::info!(
                    path = %path.display(),
                    "Log file deleted (max_total_size exceeded)"
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{Duration, SystemTime},
    };

    use filetime::{set_file_mtime, FileTime};
    use tempfile::tempdir;

    use super::*;

    /// Тест проверят, что compress_log_file создаёт .log.gz, удаляет исходный
    /// файл, и увеличивает счётчик compressed_count.
    #[test]
    fn test_compress_log_file_creates_gz_and_updates_metrics() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.log");
        fs::write(&path, b"some content to compress").unwrap();

        let metrics = Arc::new(RotationMetrics::new());
        let res = compress_log_file(&path, &metrics);
        assert!(res.is_ok());
        let gz_path = res.unwrap();
        assert!(gz_path.exists(), "gz file should exist");
        assert!(!path.exists(), "original file should be removed");
        let stats = metrics.get_stats();
        assert_eq!(stats.compressed_count, 1);
    }

    /// Тест проверят удаление по возрасту (max_age_days).
    #[test]
    fn test_apply_retention_policy_deletes_old_files_by_age() {
        let dir = tempdir().unwrap();

        // Создаем два файла: старый и новый
        let old = dir.path().join("old.log");
        let new = dir.path().join("new.log");
        fs::write(&old, b"old").unwrap();
        fs::write(&new, b"new").unwrap();

        // Установим mtime старого файла на 10 дней назад
        let old_time =
            FileTime::from_system_time(SystemTime::now() - Duration::from_secs(10 * 86400));
        set_file_mtime(&old, old_time).unwrap();

        let metrics = Arc::new(RotationMetrics::new());
        let policy = RetentionPolicy {
            max_age_days: Some(7),
            ..Default::default()
        };

        apply_retention_policy(dir.path(), &policy, &metrics).unwrap();

        assert!(!old.exists(), "old file should be deleted by age");
        assert!(new.exists(), "new file should remain");
        let s = metrics.get_stats();
        assert_eq!(s.deleted_count, 1);
    }

    /// Тест проверят удаление по количеству файлов (max_files).
    #[test]
    fn test_apply_retention_policy_deletes_excess_files_by_count() {
        let dir = tempdir().unwrap();

        // Создаём 5 файлов с разным временем
        for i in 0..5 {
            let p = dir.path().join(format!("file-{i}.log"));
            fs::write(&p, vec![0u8; 10]).unwrap();
            // выставим разные времена: чем меньше i — тем старее
            let t = SystemTime::now() - Duration::from_secs((5 - i) as u64 * 86400);
            set_file_mtime(&p, FileTime::from_system_time(t)).unwrap();
        }

        let metrics = Arc::new(RotationMetrics::new());
        let policy = RetentionPolicy {
            max_files: Some(3),
            ..Default::default()
        };

        apply_retention_policy(dir.path(), &policy, &metrics).unwrap();

        // Должно остаться 3 файла
        let remaining: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(remaining.len(), 3);
        let s = metrics.get_stats();
        assert_eq!(s.deleted_count, 2);
    }

    /// Тест проверят удаление по общему размеру (max_total_size_bytes).
    #[test]
    fn test_apply_retention_policy_deletes_excess_files_by_total_size() {
        let dir = tempdir().unwrap();

        // Создадим три файла: 1KB, 1KB, 1KB => total 3KB
        for i in 0..3 {
            let p = dir.path().join(format!("size-{i}.log"));
            fs::write(&p, vec![0u8; 1024]).unwrap();
            // стареее проще не трогать
        }

        let metrics = Arc::new(RotationMetrics::new());
        let policy = RetentionPolicy {
            max_total_size_bytes: Some(1024 * 2), // 2KB
            ..Default::default()
        };

        apply_retention_policy(dir.path(), &policy, &metrics).unwrap();

        // После удаления общий размер <= 2KB -> удалено хотя бы 1 файл
        let s = metrics.get_stats();
        assert!(s.deleted_count >= 1);
        // Убедимся, что суммарный размер файлов <= limit
        let total: u64 = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().len())
            .sum();
        assert!(total <= 1024 * 2);
    }

    /// Тест проверят, что SizeRotatingWriter ротается при достижении лимита
    /// и метрика rotation_count увеличивается.
    #[test]
    fn test_size_rotating_writer_rotates_on_size() {
        let dir = tempdir().unwrap();
        let base = dir.path().join("zumic.log");
        let metrics = Arc::new(RotationMetrics::new());

        // max_size_mb = 1 (1 MiB)
        let writer =
            SizeRotatingWriter::new(base.clone(), 1, FileNaming::Sequential, metrics.clone())
                .expect("failed to create SizeRotatingWriter");

        // Запишем чуть больше 1 MiB чтобы вызвать ротацию
        let buf = vec![b'x'; 1024 * 1024 + 100]; // 1 MiB + 100 bytes
        writer.write_all(&buf).expect("write should succeed");

        // После записи должна произойти по крайней мере одна ротация
        let stats = metrics.get_stats();
        assert!(stats.rotation_count >= 1, "expected rotation_count >= 1");
    }

    /// Тест проверят, что get_next_filename использует стратегию
    /// Dated/Sequential/Full.
    #[test]
    fn test_file_naming_strategies_generate_expected_patterns() {
        let dir = tempdir().unwrap();
        let base = dir.path().join("zumic.log");
        let metrics = Arc::new(RotationMetrics::new());

        // For Dated strategy
        let w_dated =
            SizeRotatingWriter::new(base.clone(), 1, FileNaming::Dated, metrics.clone()).unwrap();
        let p1 = w_dated.get_next_filename();
        assert!(p1
            .to_string_lossy()
            .contains(&chrono::Local::now().format("%Y-%m-%d").to_string()));

        // For Sequential strategy - note that constructor already called rotate() once,
        // so the next filename will be -002
        let w_seq =
            SizeRotatingWriter::new(base.clone(), 1, FileNaming::Sequential, metrics.clone())
                .unwrap();
        let p2 = w_seq.get_next_filename();
        assert!(
            p2.to_string_lossy().contains("-002."),
            "Expected -002 because constructor already created -001, got: {}",
            p2.display()
        );

        // For Full strategy - same applies
        let w_full =
            SizeRotatingWriter::new(base.clone(), 1, FileNaming::Full, metrics.clone()).unwrap();
        let p3 = w_full.get_next_filename();
        assert!(p3
            .to_string_lossy()
            .contains(&chrono::Local::now().format("%Y-%m-%d").to_string()));
        assert!(
            p3.to_string_lossy().contains("-002."),
            "Expected -002 because constructor already created -001, got: {}",
            p3.display()
        );
    }
}
