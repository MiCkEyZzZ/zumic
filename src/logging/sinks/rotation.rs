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
        *seq += 1;

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
                    .with_file_name(format!("{}-{}.{}", base_name, date, extension))
            }
            FileNaming::Sequential => self
                .base_path
                .with_file_name(format!("{}-{:03}.{}", base_name, *seq, extension)),
            FileNaming::Full => {
                let date = chrono::Local::now().format("%Y-%m-%d");
                self.base_path
                    .with_file_name(format!("{}-{}-{:03}.{}", base_name, date, *seq, extension))
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
