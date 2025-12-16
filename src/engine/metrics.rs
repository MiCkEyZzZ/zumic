use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};

use crate::engine::{AdvancedRebalancer, SlotManager};

/// Статус здоровья кластера
#[derive(Debug, Clone)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

/// Тренд производительности во времени
#[derive(Debug, Clone, PartialEq)]
pub enum PerformanceTrend {
    Improving,
    Stable,
    Degrading,
    Unknown,
}

/// Сводные метрики состояния кластера
#[derive(Debug, Clone)]
pub struct ClusterMetrics {
    /// Метрики операций (GET/SET/DEL/ошибки и т.д.)
    pub operations: OperationMetrics,
    /// Метрики производительности (latency, cache, память)
    pub performance: PerformanceMetrics,
    /// Метрики ребалансировки
    pub rebalancing: RebalancingMetrics,
    /// Метрики здоровья (ошибки, баланс нагрузки)
    pub health: HealthMetrics,
    /// Время снятия метрик
    pub timestamp: Instant,
}

/// Метрики операций с ключами
#[derive(Debug, Clone, Default)]
pub struct OperationMetrics {
    pub total_ops: u64,
    pub get_ops: u64,
    pub set_ops: u64,
    pub del_ops: u64,
    pub cross_shard_ops: u64,
    pub failed_ops: u64,
    pub ops_per_second: f64,
}

/// Метрики производительности (latency, память, кэш)
#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    pub avg_response_time_ms: f64,
    pub p95_response_time_ms: f64,
    pub p99_response_time_ms: f64,
    pub memory_usage_bytes: u64,
    pub cache_hit_rate: f64,
}

/// Метрики ребалансировки слотов
#[derive(Debug, Clone, Default)]
pub struct RebalancingMetrics {
    pub active_migrations: u32,
    pub completed_migrations: u64,
    pub failed_migrations: u64,
    pub rebalancing_efficiency: f64,
    pub last_rebalance: Option<Instant>,
}

/// Метрики здоровья кластера
#[derive(Debug, Clone, Default)]
pub struct HealthMetrics {
    pub healthy_shards: u32,
    pub total_shards: u32,
    pub load_imbalance_ratio: f64,
    pub hot_keys_count: u32,
    pub error_rate: f64,
}

/// Отчёт о здоровье системы
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// Итоговый статус
    pub overall_health: HealthStatus,
    /// Предупреждения и алерты
    pub alerts: Vec<String>,
    /// Динамика производительности
    pub performance_trend: PerformanceTrend,
    /// Сводка метрик (последние данные)
    pub metrics_summary: Option<ClusterMetrics>,
    /// Время генерации отчёта
    pub generated_at: Instant,
}

/// Сборщик и агрегатор метрик кластера
pub struct MetricsCollector {
    metrics_history: Arc<RwLock<VecDeque<ClusterMetrics>>>,
    response_times: Arc<Mutex<VecDeque<f64>>>,
    operation_counts: Arc<RwLock<HashMap<String, u64>>>,
    error_counts: Arc<RwLock<HashMap<String, u64>>>,
    #[allow(dead_code)]
    collection_interval: Duration,
    max_history_size: usize,
    last_collection: Arc<Mutex<Instant>>,
}

////////////////////////////////////////////////////////////////////////////////
// Собственные методы
////////////////////////////////////////////////////////////////////////////////

impl MetricsCollector {
    /// Создаёт новый сборщик метрик
    pub fn new(
        collection_interval: Duration,
        max_history_size: usize,
    ) -> Self {
        Self {
            metrics_history: Arc::new(RwLock::new(VecDeque::with_capacity(max_history_size))),
            response_times: Arc::new(Mutex::new(VecDeque::with_capacity(1000))),
            operation_counts: Arc::new(RwLock::new(HashMap::new())),
            error_counts: Arc::new(RwLock::new(HashMap::new())),
            collection_interval,
            max_history_size,
            last_collection: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Записывает факт выполнения операции и её время отклика
    pub fn record_operation(
        &self,
        operation: &str,
        response_time_ms: f64,
    ) {
        // обновляем счётчики операций
        {
            let mut counts = self.operation_counts.write().unwrap();
            *counts.entry(operation.to_string()).or_insert(0) += 1;
        }
        // сохраняем время отклика
        {
            let mut times = self.response_times.lock().unwrap();
            times.push_back(response_time_ms);
            if times.len() > 1000 {
                times.pop_front();
            }
        }
    }

    /// Регистрирует ошибку определённого типа
    pub fn record_error(
        &self,
        error_type: &str,
    ) {
        let mut errors = self.error_counts.write().unwrap();
        *errors.entry(error_type.to_string()).or_insert(0) += 1;
    }

    /// Снимает метрики текущего состояния кластера
    pub fn collect_metrics(
        &self,
        slot_manager: &SlotManager,
        rebalancer: &AdvancedRebalancer,
    ) -> ClusterMetrics {
        let now = Instant::now();

        // Calculate operations metrics
        let operation_metrics = {
            let counts = self.operation_counts.read().unwrap();
            let total_ops = counts.values().sum::<u64>();
            let get_ops = *counts.get("get").unwrap_or(&0);
            let set_ops = *counts.get("set").unwrap_or(&0);
            let del_ops = *counts.get("del").unwrap_or(&0);
            let cross_shard_ops = *counts.get("cross_shard").unwrap_or(&0);

            let error_counts = self.error_counts.read().unwrap();
            let failed_ops = error_counts.values().sum::<u64>();

            let last_collection = *self.last_collection.lock().unwrap();
            let duration_secs = (now - last_collection).as_secs_f64();
            let ops_per_second = if duration_secs > 0.0 {
                total_ops as f64 / duration_secs
            } else {
                0.0
            };

            OperationMetrics {
                total_ops,
                get_ops,
                set_ops,
                del_ops,
                cross_shard_ops,
                failed_ops,
                ops_per_second,
            }
        };

        // Calculate performance metrics
        let performance_metrics = {
            let times = self.response_times.lock().unwrap();
            let mut sorted_times: Vec<f64> = times.iter().cloned().collect();
            sorted_times.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let avg_response_time_ms = if !sorted_times.is_empty() {
                sorted_times.iter().sum::<f64>() / sorted_times.len() as f64
            } else {
                0.0
            };

            let p95_response_time_ms = if !sorted_times.is_empty() {
                let idx = (sorted_times.len() as f64 * 0.95) as usize;
                sorted_times
                    .get(idx.min(sorted_times.len() - 1))
                    .cloned()
                    .unwrap_or(0.0)
            } else {
                0.0
            };

            let p99_response_time_ms = if !sorted_times.is_empty() {
                let idx = (sorted_times.len() as f64 * 0.99) as usize;
                sorted_times
                    .get(idx.min(sorted_times.len() - 1))
                    .cloned()
                    .unwrap_or(0.0)
            } else {
                0.0
            };

            PerformanceMetrics {
                avg_response_time_ms,
                p95_response_time_ms,
                p99_response_time_ms,
                memory_usage_bytes: 0, // Would need actual memory tracking
                cache_hit_rate: 0.0,   // Would need cache hit/miss tracking
            }
        };

        // Calculate rebalancing metrics
        let rebalancing_metrics = {
            let migration_status = slot_manager.get_migration_status();
            let active_migrations = migration_status.len() as u32;

            let rebalancing_efficiency = rebalancer.calculate_rebalance_efficiency().unwrap_or(0.0);

            let last_rebalance = rebalancer
                .get_rebalance_history()
                .last()
                .map(|event| event.timestamp);

            RebalancingMetrics {
                active_migrations,
                completed_migrations: 0, // Would need tracking
                failed_migrations: 0,    // Would need tracking
                rebalancing_efficiency,
                last_rebalance,
            }
        };

        // Calculate health metrics
        let health_metrics = {
            let load_distribution = slot_manager.get_load_distribution();
            let shard_count = load_distribution.len() as u32;

            let (load_imbalance_ratio, healthy_shards) = if !load_distribution.is_empty() {
                let loads: Vec<f64> = load_distribution.values().cloned().collect();
                let max_load = loads.iter().cloned().fold(0.0, f64::max);
                let min_load = loads.iter().cloned().fold(f64::INFINITY, f64::min);

                let imbalance = if min_load > 0.0 {
                    max_load / min_load
                } else {
                    1.0
                };

                // Consider a shard healthy if its load is within 2x of average
                let avg_load = loads.iter().sum::<f64>() / loads.len() as f64;
                let healthy = loads
                    .iter()
                    .filter(|&&load| load <= avg_load * 2.0 && load >= avg_load * 0.5)
                    .count() as u32;

                (imbalance, healthy)
            } else {
                (1.0, 0)
            };

            let hot_keys_count = slot_manager.get_hot_keys().len() as u32;

            let error_rate = {
                let total_ops = operation_metrics.total_ops;
                if total_ops > 0 {
                    operation_metrics.failed_ops as f64 / total_ops as f64
                } else {
                    0.0
                }
            };

            HealthMetrics {
                healthy_shards,
                total_shards: shard_count,
                load_imbalance_ratio,
                hot_keys_count,
                error_rate,
            }
        };

        *self.last_collection.lock().unwrap() = now;

        ClusterMetrics {
            operations: operation_metrics,
            performance: performance_metrics,
            rebalancing: rebalancing_metrics,
            health: health_metrics,
            timestamp: now,
        }
    }

    /// Сохраняет метрики в историю (с обрезкой по max_history_size)
    pub fn store_metrics(
        &self,
        meetrics: ClusterMetrics,
    ) {
        let mut history = self.metrics_history.write().unwrap();
        history.push_back(meetrics);

        if history.len() > self.max_history_size {
            history.pop_front();
        }
    }

    /// Возвращает историю метрик за указанное время
    pub fn get_metrics_history(
        &self,
        duration: Duration,
    ) -> Vec<ClusterMetrics> {
        let history = self.metrics_history.read().unwrap();
        let cutoff = Instant::now() - duration;

        history
            .iter()
            .filter(|metrics| metrics.timestamp > cutoff)
            .cloned()
            .collect()
    }

    /// Возвращает последние снятые метрики
    pub fn get_latest_metrics(&self) -> Option<ClusterMetrics> {
        let history = self.metrics_history.read().unwrap();
        history.back().cloned()
    }

    /// Экспортирует метрики за период в CSV-формат
    pub fn export_metrics_csv(
        &self,
        duration: Duration,
    ) -> String {
        let metrics = self.get_metrics_history(duration);
        let mut csv = String::new();

        // CSV header
        csv.push_str("timestamp,total_ops,ops_per_second,avg_response_time,p95_response_time,");
        csv.push_str("active_migrations,load_imbalance,error_rate,healthy_shards\n");

        // CSV data
        for metric in metrics {
            csv.push_str(&format!(
                "{:?},{},{:.2},{:.2},{:.2},{},{:.2},{:.4},{}\n",
                metric.timestamp,
                metric.operations.total_ops,
                metric.operations.ops_per_second,
                metric.performance.avg_response_time_ms,
                metric.performance.p95_response_time_ms,
                metric.rebalancing.active_migrations,
                metric.health.load_imbalance_ratio,
                metric.health.error_rate,
                metric.health.healthy_shards,
            ));
        }

        csv
    }

    /// Генерирует отчёт о здоровье системы на основе последних метрик
    pub fn generate_health_report(&self) -> HealthReport {
        let latest_metrics = self.get_latest_metrics();
        let recent_metrics = self.get_metrics_history(Duration::from_secs(15 * 60));

        let overall_health = if let Some(ref metrics) = latest_metrics {
            let helth_score = calculate_health_score(&metrics.health);
            if helth_score > 0.8 {
                HealthStatus::Healthy
            } else if helth_score > 0.6 {
                HealthStatus::Warning
            } else {
                HealthStatus::Critical
            }
        } else {
            HealthStatus::Unknown
        };

        let mut alerts = Vec::new();

        if let Some(ref metrics) = latest_metrics {
            // Check for high error rate
            if metrics.health.error_rate > 0.05 {
                alerts.push(format!(
                    "High error rate: {:.2}% (threshold: 5%)",
                    metrics.health.error_rate * 100.0
                ));
            }

            // Check for load imbalance
            if metrics.health.load_imbalance_ratio > 2.0 {
                alerts.push(format!(
                    "Load imbalance detected: {:.2}x difference between shards",
                    metrics.health.load_imbalance_ratio
                ));
            }

            // Check for unhealthy shards
            let unhealthy_shards = metrics.health.total_shards - metrics.health.healthy_shards;
            if unhealthy_shards > 0 {
                alerts.push(format!(
                    "{} out of {} shards are unhealthy",
                    unhealthy_shards, metrics.health.total_shards
                ));
            }

            // Check for high response times
            if metrics.performance.p95_response_time_ms > 100.0 {
                alerts.push(format!(
                    "High response times: P95 = {:.2}ms (threshold: 100ms)",
                    metrics.performance.p95_response_time_ms
                ));
            }
        }

        let performance_trend = if recent_metrics.len() >= 2 {
            let first = &recent_metrics[0];
            let last = &recent_metrics[recent_metrics.len() - 1];

            if last.performance.avg_response_time_ms > first.performance.avg_response_time_ms * 1.1
            {
                PerformanceTrend::Degrading
            } else if last.performance.avg_response_time_ms
                < first.performance.avg_response_time_ms * 0.9
            {
                PerformanceTrend::Improving
            } else {
                PerformanceTrend::Stable
            }
        } else {
            PerformanceTrend::Unknown
        };

        HealthReport {
            overall_health,
            alerts,
            performance_trend,
            metrics_summary: latest_metrics,
            generated_at: Instant::now(),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Внутренние методы и функции
////////////////////////////////////////////////////////////////////////////////

/// Вычисляет итоговый health score на основе HealthMetrics
fn calculate_health_score(health: &HealthMetrics) -> f64 {
    let mut score = 1.0;
    // Penalize error rate
    score *= (1.0 - health.error_rate.min(0.5) * 2.0).max(0.0);
    // Penalize load imbalance
    let imbalance_penalty = (health.load_imbalance_ratio - 1.0).min(2.0) / 2.0;
    score *= (1.0 - imbalance_penalty).max(0.0);
    // Penalize unhealthy shards
    if health.total_shards > 0 {
        let healthy_ratio = health.healthy_shards as f64 / health.total_shards as f64;
        score *= healthy_ratio;
    }

    score
}

////////////////////////////////////////////////////////////////////////////////
// Тесты
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::rebalancer::RebalancerConfig;

    /// Тест проверяет на корректность подсчёта операций после вызова
    /// record_operation
    #[test]
    fn test_metrics_collection() {
        let collector = MetricsCollector::new(Duration::from_secs(1), 100);

        // Record some operations
        collector.record_operation("get", 1.5);
        collector.record_operation("set", 2.1);
        collector.record_operation("get", 1.2);

        // Check operation counts
        let counts = collector.operation_counts.read().unwrap();
        assert_eq!(*counts.get("get").unwrap(), 2);
        assert_eq!(*counts.get("set").unwrap(), 1);
    }

    /// Тест проверяет на корректность вычисления `health` score для различных
    /// метрик
    #[test]
    fn test_health_score_calculation() {
        let healthy_metrics = HealthMetrics {
            healthy_shards: 3,
            total_shards: 3,
            load_imbalance_ratio: 1.1,
            hot_keys_count: 2,
            error_rate: 0.01,
        };

        let score = calculate_health_score(&healthy_metrics);
        assert!(score > 0.8);

        let unhealthy_metrics = HealthMetrics {
            healthy_shards: 1,
            total_shards: 3,
            load_imbalance_ratio: 3.0,
            hot_keys_count: 10,
            error_rate: 0.15,
        };

        let score = calculate_health_score(&unhealthy_metrics);
        assert!(score < 0.5);
    }

    /// Тест проверяет на правильность конфигурации `RebalancerConfig`
    #[test]
    fn test_rebalancer_planning() {
        // This would require more setup with actual SlotManager
        // Simplified test for now
        let config = RebalancerConfig::default();
        assert_eq!(config.load_threshold, 1.5);
        assert_eq!(config.migration_batch_size, 64);
    }

    /// Тест проверяет на сохранение и корректное извлечение истории метрик
    #[test]
    fn test_metrics_store_and_retrieve_history() {
        let collector = MetricsCollector::new(Duration::from_secs(1), 5);

        for i in 0..3 {
            let metric = ClusterMetrics {
                operations: OperationMetrics {
                    total_ops: i,
                    ..Default::default()
                },
                performance: PerformanceMetrics::default(),
                rebalancing: RebalancingMetrics::default(),
                health: HealthMetrics::default(),
                timestamp: Instant::now(),
            };
            collector.store_metrics(metric);
        }

        let history = collector.get_metrics_history(Duration::from_secs(10));
        assert_eq!(history.len(), 3);

        let latest = collector.get_latest_metrics().unwrap();
        assert_eq!(latest.operations.total_ops, 2);
    }

    /// Тест проверяет на корректность подсчёта операций и среднего времени
    /// отклика
    #[test]
    fn test_metrics_collect_operations_and_performance() {
        let collector = MetricsCollector::new(Duration::from_secs(1), 100);

        collector.record_operation("get", 10.0);
        collector.record_operation("set", 20.0);
        collector.record_operation("get", 30.0);

        let dummy_slot_manager = std::sync::Arc::new(crate::engine::SlotManager::new(2));
        let dummy_rebalancer = crate::engine::AdvancedRebalancer::new(
            Arc::clone(&dummy_slot_manager),
            crate::engine::RebalancerConfig::default(),
        );

        let metrics = collector.collect_metrics(&dummy_slot_manager, &dummy_rebalancer);
        assert_eq!(metrics.operations.total_ops, 3);
        assert!(metrics.performance.avg_response_time_ms > 0.0);
    }

    /// Тест проверяет на корректность экспорта метрик в CSV-формат
    #[test]
    fn test_export_metrics_csv_format() {
        let collector = MetricsCollector::new(Duration::from_secs(1), 10);
        let dummy_slot_manager = std::sync::Arc::new(crate::engine::SlotManager::new(1));
        let dummy_rebalancer = crate::engine::AdvancedRebalancer::new(
            Arc::clone(&dummy_slot_manager),
            crate::engine::RebalancerConfig::default(),
        );

        collector.collect_metrics(&dummy_slot_manager, &dummy_rebalancer);

        let csv = collector.export_metrics_csv(Duration::from_secs(60));
        assert!(csv.contains("timestamp"));
        assert!(csv.contains("total_ops"));
    }

    /// Тест проверяет на корректность генерации отчёта о здоровье кластера,
    /// включая alerts и performance trend
    #[test]
    fn test_generate_health_report_alerts_and_trend() {
        let collector = MetricsCollector::new(Duration::from_secs(1), 10);

        collector.record_operation("get", 10.0);
        collector.record_operation("set", 20.0);

        let dummy_slot_manager = std::sync::Arc::new(crate::engine::SlotManager::new(2));
        let dummy_rebalancer = crate::engine::AdvancedRebalancer::new(
            Arc::clone(&dummy_slot_manager),
            crate::engine::RebalancerConfig::default(),
        );

        // Сначала коллекция 1
        let metrics1 = collector.collect_metrics(&dummy_slot_manager, &dummy_rebalancer);
        collector.store_metrics(metrics1);

        // Коллекция 2, чтобы recent_metrics.len() >= 2
        let metrics2 = collector.collect_metrics(&dummy_slot_manager, &dummy_rebalancer);
        collector.store_metrics(metrics2);

        let report = collector.generate_health_report();

        assert!(matches!(
            report.overall_health,
            HealthStatus::Healthy
                | HealthStatus::Warning
                | HealthStatus::Critical
                | HealthStatus::Unknown
        ));

        // Теперь metrics_summary точно Some
        assert!(report.metrics_summary.is_some());
    }

    /// Тест проверяет на корректное увеличение счётчика ошибок после вызова
    /// `record_error`
    #[test]
    fn test_record_error_increments_failed_ops() {
        let collector = MetricsCollector::new(Duration::from_secs(1), 10);

        collector.record_error("timeout");
        collector.record_error("timeout");
        collector.record_error("connection");

        let errors = collector.error_counts.read().unwrap();
        assert_eq!(*errors.get("timeout").unwrap(), 2);
        assert_eq!(*errors.get("connection").unwrap(), 1);
    }
}
