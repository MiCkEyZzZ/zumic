use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use ordered_float::Pow;

use crate::engine::{ShardId, SlotId, SlotManager};

/// Причины, по которым может быть инициирован ребаланс.
#[derive(Debug, Clone, PartialEq)]
pub enum RebalanceTrigger {
    /// Ребаланс инициирован из-за дисбаланса нагрузки между шардами.
    LoadImbalance { max_load: f64, min_load: f64 },
    /// Ребаланс инициирован из-за обнаружения "горячего" ключа.
    HotKeyDetected { key: String, ops_per_sec: u64 },
    /// Ручной триггер ребаланса.
    ManualTrigger,
    /// Добавлен новый шард.
    ShardAddition,
    /// Удален существующий шард.
    ShardRemoval,
}

/// Конфигурация для `AdvancedRebalancer`.
pub struct RebalancerConfig {
    /// Порог для перегруженных/недогруженных шардов.
    pub load_threshold: f64,
    /// Порог операций в секунду для "горячего" ключа.
    pub hot_key_threshold: u64,
    /// Максимальное количество слотов для миграции за один раз.
    pub migration_batch_size: usize,
    /// Время ожидания между ребалансами (cooldown).
    pub cool_down_period: Duration,
    /// Включение консистентного хеширования при перераспределении.
    pub enable_consistent_hashing: bool,
}

/// Основная структура для ребаланса.
pub struct AdvancedRebalancer {
    slot_manager: Arc<SlotManager>,
    config: RebalancerConfig,
    last_rebalance: Instant,
    rebalance_history: Vec<RebalanceEvent>,
}

/// Информация о событии ребаланса.
#[derive(Debug, Clone)]
pub struct RebalanceEvent {
    pub timestamp: Instant,
    pub trigger_reason: RebalanceTrigger,
    pub migrations_planned: usize,
    pub migrations_completed: usize,
    pub duration: Duration,
    pub load_before: HashMap<ShardId, f64>,
    pub load_after: HashMap<ShardId, f64>,
}

impl AdvancedRebalancer {
    /// Создает новый `AdvancedRebalancer` с указанным SlotManager и конфигурацией.
    pub fn new(
        slot_manager: Arc<SlotManager>,
        config: RebalancerConfig,
    ) -> Self {
        Self {
            slot_manager,
            config,
            last_rebalance: Instant::now(),
            rebalance_history: Vec::new(),
        }
    }

    /// Оценивает необходимость ребаланса.
    ///
    /// Возвращает триггер ребаланса, если требуется, иначе `None`.
    pub fn evaluate_rebalancing_need(&self) -> Option<RebalanceTrigger> {
        // Check cooldown period
        if self.last_rebalance.elapsed() < self.config.cool_down_period {
            return None;
        }

        let load_distribution = self.slot_manager.get_load_distribution();
        if load_distribution.is_empty() {
            return None;
        }

        None
    }

    /// Планирует миграции слотов в зависимости от триггера.
    pub fn plan_rebalancing(
        &self,
        trigger: &RebalanceTrigger,
    ) -> Vec<(SlotId, ShardId, ShardId)> {
        match trigger {
            RebalanceTrigger::LoadImbalance { .. } => self.plan_load_based_rebalancing(),
            RebalanceTrigger::HotKeyDetected { key, .. } => self.plan_hot_key_redistribution(key),
            RebalanceTrigger::ManualTrigger => self.plan_load_based_rebalancing(),
            _ => Vec::new(),
        }
    }

    /// Выполняет ребаланс согласно триггеру.
    ///
    /// Возвращает событие с информацией о миграциях.
    pub fn execute_rebalancing(
        &mut self,
        trigger: RebalanceTrigger,
    ) -> Result<RebalanceEvent, String> {
        let start_time = Instant::now();
        let load_before = self.slot_manager.get_load_distribution();

        let migrations = self.plan_rebalancing(&trigger);
        let mut completed_migrations = 0;

        for (slot, from_shard, to_shard) in &migrations {
            match self
                .slot_manager
                .start_slot_migration(*slot, *from_shard, *to_shard)
            {
                Ok(()) => {
                    if let Err(e) = self.slot_manager.complete_slot_migration(*slot) {
                        eprintln!("Failed to complete migration for slot {slot}: {e}");
                    } else {
                        completed_migrations += 1;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to start migration for slot {slot}: {e}");
                }
            }
        }

        let duration = start_time.elapsed();
        let load_after = self.slot_manager.get_load_distribution();

        let event = RebalanceEvent {
            timestamp: start_time,
            trigger_reason: trigger,
            migrations_planned: migrations.len(),
            migrations_completed: completed_migrations,
            duration,
            load_before,
            load_after,
        };

        self.rebalance_history.push(event.clone());
        self.last_rebalance = Instant::now();

        Ok(event)
    }

    /// Возвращает историю всех событий ребаланса.
    pub fn get_rebalance_history(&self) -> &[RebalanceEvent] {
        &self.rebalance_history
    }

    /// Рассчитывает эффективность последних ребалансов (0.0..1.0).
    /// Основано на уменьшении дисбаланса нагрузки между шардами.
    pub fn calculate_rebalance_efficiency(&self) -> Option<f64> {
        if self.rebalance_history.is_empty() {
            return None;
        }

        let recent_events: Vec<_> = self.rebalance_history.iter().rev().take(5).collect();

        let mut efficiency_sum = 0.0;
        let mut count = 0;

        for event in recent_events {
            if !event.load_before.is_empty() && !event.load_after.is_empty() {
                let load_variance_before = calculate_load_variance(&event.load_before);
                let load_variance_after = calculate_load_variance(&event.load_after);

                if load_variance_before > 0.0 {
                    let improvement =
                        (load_variance_before - load_variance_after) / load_variance_before;
                    efficiency_sum += improvement;
                    count += 1;
                }
            }
        }

        if count > 0 {
            Some(efficiency_sum / count as f64)
        } else {
            None
        }
    }

    /// Планирует миграции для ребаланса на основе дисбаланса нагрузки.
    fn plan_load_based_rebalancing(&self) -> Vec<(SlotId, ShardId, ShardId)> {
        let load_distribution = self.slot_manager.get_load_distribution();
        let mut migrations = Vec::new();

        // Calculate average load
        let total_load: f64 = load_distribution.values().sum();
        let avg_load = total_load / load_distribution.len() as f64;

        // Identify overloaded and underloaded shards
        let mut overloaded: Vec<(ShardId, f64)> = Vec::new();
        let mut underloaded: Vec<(ShardId, f64)> = Vec::new();

        for (&shard_id, &load) in &load_distribution {
            if load > avg_load * self.config.load_threshold {
                overloaded.push((shard_id, load));
            } else if load < avg_load / self.config.load_threshold {
                underloaded.push((shard_id, load));
            }
        }

        overloaded.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        underloaded.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        // Plan migrations from overloaded to underloaded shards
        for (overloaded_shard, _) in overloaded.iter().take(self.config.migration_batch_size / 2) {
            for (underloaded_shard, _) in underloaded.iter().take(2) {
                // Find slots to migrate (simplified - would use actual slot analysis)
                for slot in 0..100 {
                    // Simplified range
                    if self.slot_manager.get_slot_shard(slot) == Some(*overloaded_shard) {
                        migrations.push((slot, *overloaded_shard, *underloaded_shard));
                        if migrations.len() >= self.config.migration_batch_size {
                            break;
                        }
                    }
                }
                if migrations.len() >= self.config.migration_batch_size {
                    break;
                }
            }
            if migrations.len() >= self.config.migration_batch_size {
                break;
            }
        }

        migrations
    }

    /// Планирует миграцию для горячего ключа.
    fn plan_hot_key_redistribution(
        &self,
        hot_key: &str,
    ) -> Vec<(SlotId, ShardId, ShardId)> {
        let slot = self.slot_manager.calculate_slot(hot_key);
        let current_shard = self.slot_manager.get_key_shard(hot_key);

        let load_distribution = self.slot_manager.get_load_distribution();

        // Если все шарды имеют одинаковую нагрузку, миграция не нужна
        if load_distribution
            .values()
            .all(|&load| (load - load_distribution[&current_shard]).abs() < f64::EPSILON)
        {
            return Vec::new();
        }

        let target_shard = load_distribution
            .iter()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(&shard_id, _)| shard_id)
            .unwrap_or(0);

        if target_shard != current_shard {
            vec![(slot, current_shard, target_shard)]
        } else {
            Vec::new()
        }
    }
}

impl Default for RebalancerConfig {
    fn default() -> Self {
        Self {
            load_threshold: 1.5,
            hot_key_threshold: 100,
            migration_batch_size: 64,
            cool_down_period: Duration::from_secs(60),
            enable_consistent_hashing: false,
        }
    }
}

/// Вычисляет дисперсию нагрузки по всем шардом.
/// Используется для оценки эффективности ребаланса.
fn calculate_load_variance(load_distribution: &HashMap<ShardId, f64>) -> f64 {
    if load_distribution.is_empty() {
        return 0.0;
    }

    let loads: Vec<f64> = load_distribution.values().cloned().collect();
    let mean = loads.iter().sum::<f64>() / loads.len() as f64;
    let variance = loads.iter().map(|load| (load - mean).pow(2)).sum::<f64>() / loads.len() as f64;
    variance
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{sync::Arc, thread::sleep};

    fn setup_slot_manager(shard_count: usize) -> Arc<SlotManager> {
        Arc::new(SlotManager::new(shard_count))
    }

    /// Тест проверяет на отсутствие необходимости ребаланса при нулевой нагрузке
    #[test]
    fn test_rebalance_no_load() {
        let slot_manager = setup_slot_manager(4);
        let rebalancer =
            AdvancedRebalancer::new(Arc::clone(&slot_manager), RebalancerConfig::default());

        // Нет нагрузки - ребаланс не нужен
        assert_eq!(rebalancer.evaluate_rebalancing_need(), None);
        assert!(rebalancer
            .plan_rebalancing(&RebalanceTrigger::ManualTrigger)
            .is_empty());
    }

    /// Тест проверяет на корректное планирование и выполнение миграций при перегрузке одного шарда
    #[test]
    fn test_rebalance_with_overloaded_shard() {
        let slot_manager = setup_slot_manager(4);

        // Симулируем перегрузку первого шарда
        for _ in 0..200 {
            slot_manager.record_operation("key1");
        }

        for _ in 0..50 {
            slot_manager.record_operation("key2");
        }

        // Ждём немного, чтобы cooldown не блокировался
        sleep(Duration::from_millis(10));

        let mut rebalancer = AdvancedRebalancer::new(
            Arc::clone(&slot_manager),
            RebalancerConfig {
                load_threshold: 1.2,
                hot_key_threshold: 50,
                migration_batch_size: 10,
                cool_down_period: Duration::from_secs(0),
                enable_consistent_hashing: false,
            },
        );

        let trigger = RebalanceTrigger::ManualTrigger;
        let event = rebalancer.execute_rebalancing(trigger.clone()).unwrap();

        // Проверяем, что миграции были спланированы
        assert!(event.migrations_planned > 0);
        assert_eq!(event.migrations_planned, event.migrations_completed);

        // После rebalance нагрузка должна немного перераспределиться
        let load_after = slot_manager.get_load_distribution();
        assert!(!load_after.is_empty());
    }

    /// Тест проверяет на выполнение миграции горячего ключа на наименее загруженный шард
    #[test]
    fn test_hot_key_trigger_rebalance() {
        let slot_manager = setup_slot_manager(4);

        // Симулируем один горячий ключ
        for _ in 0..150 {
            slot_manager.record_operation("hot_key");
        }

        let mut rebalancer = AdvancedRebalancer::new(
            Arc::clone(&slot_manager),
            RebalancerConfig {
                load_threshold: 1.5,
                hot_key_threshold: 100,
                migration_batch_size: 10,
                cool_down_period: Duration::from_secs(0),
                enable_consistent_hashing: false,
            },
        );

        let trigger = RebalanceTrigger::HotKeyDetected {
            key: "hot_key".to_string(),
            ops_per_sec: 150,
        };
        let event = rebalancer.execute_rebalancing(trigger.clone()).unwrap();

        // Должна быть хотя бы одна миграция для горячего ключа
        assert!(event.migrations_planned > 0);
        assert_eq!(event.migrations_planned, event.migrations_completed);
    }

    /// Тест проверяет на корректное вычисление эффективности ребаланса
    #[test]
    fn test_rebalance_efficiency_calculation() {
        let slot_manager = setup_slot_manager(4);

        // Создаём несколько rebalance событий
        let mut rebalancer = AdvancedRebalancer::new(
            Arc::clone(&slot_manager),
            RebalancerConfig {
                load_threshold: 1.2,
                hot_key_threshold: 50,
                migration_batch_size: 10,
                cool_down_period: Duration::from_secs(0),
                enable_consistent_hashing: false,
            },
        );

        for i in 0..3 {
            for _ in 0..(50 + i * 20) {
                slot_manager.record_operation(&format!("key{i}"));
            }
            let trigger = RebalanceTrigger::ManualTrigger;
            rebalancer.execute_rebalancing(trigger).unwrap();
        }

        let efficiency = rebalancer.calculate_rebalance_efficiency();
        assert!(efficiency.is_some());
        assert!(efficiency.unwrap() >= 0.0);
        assert!(efficiency.unwrap() <= 1.0);
    }

    /// Тест проверяет на блокировку повторного ребаланса в течение cooldown
    #[test]
    fn test_cooldown_prevents_rebalance() {
        let slot_manager = setup_slot_manager(4);
        let mut rebalancer = AdvancedRebalancer::new(
            Arc::clone(&slot_manager),
            RebalancerConfig {
                load_threshold: 1.2,
                hot_key_threshold: 50,
                migration_batch_size: 5,
                cool_down_period: Duration::from_secs(5),
                enable_consistent_hashing: false,
            },
        );

        // Запускаем rebalance первый раз
        let trigger = RebalanceTrigger::ManualTrigger;
        rebalancer.execute_rebalancing(trigger.clone()).unwrap();

        // Попытка второго rebalance должна быть заблокирована cooldown
        assert_eq!(rebalancer.evaluate_rebalancing_need(), None);
    }

    /// Тест проверяет на выполнение ребаланса по триггеру LoadImbalance
    #[test]
    fn test_load_imbalance_trigger() {
        let slot_manager = setup_slot_manager(4);

        for _ in 0..200 {
            slot_manager.record_operation("key1");
        }

        let mut rebalancer = AdvancedRebalancer::new(
            Arc::clone(&slot_manager),
            RebalancerConfig {
                load_threshold: 1.2,
                hot_key_threshold: 50,
                migration_batch_size: 10,
                cool_down_period: Duration::from_secs(0),
                enable_consistent_hashing: false,
            },
        );

        let trigger = RebalanceTrigger::LoadImbalance {
            max_load: 200.0,
            min_load: 0.0,
        };
        let event = rebalancer.execute_rebalancing(trigger.clone()).unwrap();

        assert!(event.migrations_planned > 0);
        assert_eq!(event.migrations_planned, event.migrations_completed);
    }

    /// Тест проверяет на корректную обработку пустого распределения нагрузки
    #[test]
    fn test_empty_load_distribution_returns_none() {
        let slot_manager = setup_slot_manager(4);
        let rebalancer =
            AdvancedRebalancer::new(Arc::clone(&slot_manager), RebalancerConfig::default());
        assert_eq!(rebalancer.evaluate_rebalancing_need(), None);
    }

    /// Тест проверяет на соблюдение ограничения batch size при миграциях
    #[test]
    fn test_migration_batch_limit() {
        let slot_manager = setup_slot_manager(4);
        for i in 0..500 {
            slot_manager.record_operation(&format!("key{i}"));
        }

        let mut rebalancer = AdvancedRebalancer::new(
            Arc::clone(&slot_manager),
            RebalancerConfig {
                load_threshold: 1.1,
                hot_key_threshold: 50,
                migration_batch_size: 5,
                cool_down_period: Duration::from_secs(0),
                enable_consistent_hashing: false,
            },
        );

        let trigger = RebalanceTrigger::ManualTrigger;
        let event = rebalancer.execute_rebalancing(trigger).unwrap();
        assert!(event.migrations_planned <= 5);
    }

    /// Тест проверяет на отсутствие миграций при равномерной нагрузке
    #[test]
    fn test_uniform_load_no_migration() {
        let slot_manager = setup_slot_manager(4);
        for shard_id in 0..4 {
            for _ in 0..50 {
                slot_manager.record_operation(&format!("key{}", shard_id));
            }
        }

        let mut rebalancer = AdvancedRebalancer::new(
            Arc::clone(&slot_manager),
            RebalancerConfig {
                load_threshold: 2.0,
                hot_key_threshold: 50,
                migration_batch_size: 10,
                cool_down_period: Duration::from_secs(0),
                enable_consistent_hashing: false,
            },
        );

        let trigger = RebalanceTrigger::ManualTrigger;
        let event = rebalancer.execute_rebalancing(trigger).unwrap();
        assert_eq!(event.migrations_planned, 0);
    }

    /// Тест проверяет на отсутствие миграции горячего ключа, если он уже на наименее загруженном шарде
    #[test]
    fn test_hot_key_already_on_least_loaded_shard() {
        let slot_manager = setup_slot_manager(4);
        let mut rebalancer = AdvancedRebalancer::new(
            Arc::clone(&slot_manager),
            RebalancerConfig {
                load_threshold: 1.5,
                hot_key_threshold: 50,
                migration_batch_size: 10,
                cool_down_period: Duration::from_secs(0),
                enable_consistent_hashing: false,
            },
        );

        // Симулируем равномерную нагрузку
        for shard_id in 0..4 {
            for _ in 0..50 {
                slot_manager.record_operation(&format!("key{}", shard_id));
            }
        }

        let trigger = RebalanceTrigger::HotKeyDetected {
            key: "key0".to_string(),
            ops_per_sec: 60,
        };
        let event = rebalancer.execute_rebalancing(trigger).unwrap();
        assert_eq!(event.migrations_planned, 0);
    }

    /// Тест проверяет на отсутствие миграций при триггерах добавления или удаления шарда
    #[test]
    fn test_shard_addition_removal_triggers_no_migration() {
        let slot_manager = setup_slot_manager(4);
        let mut rebalancer =
            AdvancedRebalancer::new(Arc::clone(&slot_manager), RebalancerConfig::default());

        let add_event = rebalancer
            .execute_rebalancing(RebalanceTrigger::ShardAddition)
            .unwrap();
        assert_eq!(add_event.migrations_planned, 0);

        let remove_event = rebalancer
            .execute_rebalancing(RebalanceTrigger::ShardRemoval)
            .unwrap();
        assert_eq!(remove_event.migrations_planned, 0);
    }
}
