use std::any::Any;

use crate::{ErrorExt, StatusCode};

/// Ошибка класетра.
#[derive(Debug, Clone)]
pub enum ClusterError {
    /// Кластер недоступен
    ClusterDown { reason: String },
    /// Слот перемещён на другой узел
    MovedSlot { slot: u16, target_node: String },
    /// Операция затрагивает несколько слотов
    CrossSlot { keys: Vec<String> },
    /// Миграция уже активна для слота
    MigrationActive { slot: u16 },
    /// Нет активной миграции для слота
    NoActiveMigration { slot: u16 },
    /// Слот уже в очереди на миграцию
    SlotAlreadyQueued { slot: u16 },
    /// Невалидный ID шарда
    InvalidShard { shard_id: usize },
    /// Невалидный слот
    InvalidSlot { slot: u16 },
    /// Ошибка ребалансировки
    RebalanceFailed { reason: String },
    /// Блокировка отравлена
    PoisonedLock { resource: String },
    /// Узел недоступен
    NodeUnavailable { node_id: String, reason: String },
    /// Таймаут операции кластера
    ClusterTimeout { operation: String },
}

impl std::fmt::Display for ClusterError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::ClusterDown { reason } => write!(f, "Cluster is down: {reason}"),
            Self::MovedSlot { slot, target_node } => {
                write!(f, "Slot {slot} moved to node {target_node}")
            }
            Self::CrossSlot { keys } => {
                write!(f, "Cross-slot operation on keys: {}", keys.join(", "))
            }
            Self::MigrationActive { slot } => {
                write!(f, "Migration already active for slot {slot}")
            }
            Self::NoActiveMigration { slot } => {
                write!(f, "No active migration for slot {slot}")
            }
            Self::SlotAlreadyQueued { slot } => {
                write!(f, "Slot {slot} already queued for migration")
            }
            Self::InvalidShard { shard_id } => write!(f, "Invalid shard ID: {shard_id}"),
            Self::InvalidSlot { slot } => write!(f, "Invalid slot: {slot}"),
            Self::RebalanceFailed { reason } => write!(f, "Rebalance failed: {reason}"),
            Self::PoisonedLock { resource } => write!(f, "Lock poisoned for resource: {resource}"),
            Self::NodeUnavailable { node_id, reason } => {
                write!(f, "Node {node_id} unavailable: {reason}")
            }
            Self::ClusterTimeout { operation } => {
                write!(f, "Cluster operation timeout: {operation}")
            }
        }
    }
}

impl std::error::Error for ClusterError {}

impl ErrorExt for ClusterError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::ClusterDown { .. } => StatusCode::ClusterDown,
            Self::MovedSlot { .. } => StatusCode::MovedSlot,
            Self::CrossSlot { .. } => StatusCode::CrossSlot,
            Self::MigrationActive { .. }
            | Self::NoActiveMigration { .. }
            | Self::SlotAlreadyQueued { .. } => StatusCode::MigrationActive,
            Self::InvalidShard { .. } => StatusCode::InvalidShard,
            Self::InvalidSlot { .. } => StatusCode::InvalidSlot,
            Self::RebalanceFailed { .. } => StatusCode::RebalanceFailed,
            Self::PoisonedLock { .. } => StatusCode::LockError,
            Self::NodeUnavailable { .. } => StatusCode::ClusterDown,
            Self::ClusterTimeout { .. } => StatusCode::Timeout,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_message(&self) -> String {
        match self {
            Self::ClusterDown { .. } => "Cluster temporarily unavailable".to_string(),
            Self::MovedSlot { slot, target_node } => {
                format!("MOVED {slot} {target_node}")
            }
            Self::CrossSlot { .. } => "Operation spans multiple slots".to_string(),
            Self::MigrationActive { slot } => format!("Slot {slot} is being migrated"),
            Self::NoActiveMigration { .. }
            | Self::SlotAlreadyQueued { .. }
            | Self::RebalanceFailed { .. }
            | Self::PoisonedLock { .. } => "Internal server error".to_string(),
            Self::InvalidShard { .. } | Self::InvalidSlot { .. } => "Invalid slot".to_string(),
            Self::NodeUnavailable { .. } => "Cluster node unavailable".to_string(),
            Self::ClusterTimeout { .. } => "Cluster operation timeout".to_string(),
        }
    }

    fn metrics_tags(&self) -> Vec<(&'static str, String)> {
        let mut tags = vec![
            ("error_type", "cluster".to_string()),
            ("status_code", self.status_code().to_string()),
        ];

        match self {
            Self::MovedSlot { slot, target_node } => {
                tags.push(("slot", slot.to_string()));
                tags.push(("target_node", target_node.clone()));
            }
            Self::MigrationActive { slot }
            | Self::NoActiveMigration { slot }
            | Self::SlotAlreadyQueued { slot }
            | Self::InvalidSlot { slot } => {
                tags.push(("slot", slot.to_string()));
            }
            Self::InvalidShard { shard_id } => {
                tags.push(("shard_id", shard_id.to_string()));
            }
            Self::NodeUnavailable { node_id, .. } => {
                tags.push(("node_id", node_id.clone()));
            }
            Self::ClusterTimeout { operation } => {
                tags.push(("operation", operation.clone()));
            }
            _ => {}
        }

        tags
    }
}

/// Конвертация из std::sync::PoisonError
impl<T> From<std::sync::PoisonError<T>> for crate::StackError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        crate::StackError::new(ClusterError::PoisonedLock {
            resource: "mutex".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moved_slot() {
        let err = ClusterError::MovedSlot {
            slot: 1234,
            target_node: "127.0.0.1:6380".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::MovedSlot);
        assert!(err.client_message().contains("MOVED"));

        let tags = err.metrics_tags();
        assert!(tags.iter().any(|(k, v)| k == &"slot" && v == "1234"));
        assert!(tags
            .iter()
            .any(|(k, v)| k == &"target_node" && v == "127.0.0.1:6380"));
    }

    #[test]
    fn test_cross_slot() {
        let err = ClusterError::CrossSlot {
            keys: vec!["key1".to_string(), "key2".to_string()],
        };
        assert_eq!(err.status_code(), StatusCode::CrossSlot);
    }

    #[test]
    fn test_cluster_down() {
        let err = ClusterError::ClusterDown {
            reason: "majority of nodes unreachable".to_string(),
        };
        assert_eq!(err.status_code(), StatusCode::ClusterDown);
        assert!(err.status_code().is_critical());
    }
}
