use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    time::{Duration, Instant},
};

/// `ExpireMap` хранит ключи с временем жизни (TTL) и автоматически
/// очищает просроченные записи.
///
/// Internally uses:
/// - `deadlines` для быстрого поиска активных ключей.
/// - `queue` (минимальная куча по времени) для эффективной очистки.
pub struct ExpireMap {
    deadlines: HashMap<Vec<u8>, Instant>,
    queue: BinaryHeap<Reverse<(Instant, Vec<u8>)>>,
}

impl ExpireMap {
    /// Создаёт новый пустой `ExpireMap`.
    pub fn new() -> Self {
        Self {
            deadlines: HashMap::new(),
            queue: BinaryHeap::new(),
        }
    }

    /// Устанавливает для `key` время жизни `ttl`.
    ///
    /// # Параметры
    /// - `key`: двоичный вектор-ключ.
    /// - `ttl`: длительность, через которую ключ станет просроченным.
    ///
    /// Сохраняет момент `deadline = now + ttl` в `deadlines` и
    /// добавляет в очередь для последующей очистки.
    pub fn set(
        &mut self,
        key: Vec<u8>,
        ttl: Duration,
    ) {
        let deadline = Instant::now() + ttl;
        self.deadlines.insert(key.clone(), deadline);
        self.queue.push(Reverse((deadline, key)));
    }

    /// Проверяет наличие непрошедшего по времени `key`.
    ///
    /// # Параметры
    /// - `key`: ссылка на вектор-ключ.
    ///
    /// # Возвращает
    /// - `true`, если ключ есть и не истёк; иначе `false`.
    ///
    /// При каждом вызове выполняется автоматическая очистка просроченных
    /// записей.
    pub fn get(
        &mut self,
        key: &[u8],
    ) -> bool {
        self.purge();
        self.deadlines.contains_key(key)
    }

    /// Удаляет `key`, если он есть, не дожидаясь его TTL.
    ///
    /// # Параметры
    /// - `key`: ссылка на вектор-ключ.
    ///
    /// Замечание: из-за особенностей `BinaryHeap` физическое удаление из неё
    /// не происходит, но при `purge` просроченные записи будут игнорироваться.
    pub fn remove(
        &mut self,
        key: &[u8],
    ) {
        self.deadlines.remove(key);
        // BinaryHeap не поддерживает удаление по ключу, но это не критично:
        // просроченные ключи будут проигнорированы при purge.
    }

    /// Очищает все записи, срок жизни которых истёк.
    ///
    /// # Возвращает
    /// Список ключей, которые были удалены как просроченные.
    pub fn purge(&mut self) -> Vec<Vec<u8>> {
        let now = Instant::now();
        let mut expired = Vec::new();
        while let Some(Reverse((deadline, ref key))) = self.queue.peek() {
            if *deadline > now {
                break;
            }
            let key = key.clone();
            self.queue.pop();
            // Если в deadlines тот же крайний срок и он уже в прошлом, удаляем.
            if let Some(sorted_deadline) = self.deadlines.get(&key) {
                if *sorted_deadline <= now {
                    self.deadlines.remove(&key);
                    expired.push(key);
                }
            }
        }
        expired
    }
}

impl Default for ExpireMap {
    /// То же, что и `ExpireMap::new()`.
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::thread::sleep;

    use super::*;

    fn key(s: &str) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    #[test]
    fn test_set_and_get() {
        let mut map = ExpireMap::new();
        map.set(key("foo"), Duration::from_secs(1));

        assert!(map.get(b"foo"));
        assert!(!map.get(b"bar"));
    }

    #[test]
    fn test_expire_after_duration() {
        let mut map = ExpireMap::new();
        map.set(key("expiring"), Duration::from_millis(100));

        assert!(map.get(b"expiring"));
        sleep(Duration::from_millis(120));
        assert!(!map.get(b"expiring"));
    }

    #[test]
    fn test_remove() {
        let mut map = ExpireMap::new();
        map.set(key("delete_me"), Duration::from_secs(10));
        assert!(map.get(b"delete_me"));

        map.remove(b"delete_me");
        assert!(!map.get(b"delete_me"));
    }

    #[test]
    fn test_purge_returns_expired_keys() {
        let mut map = ExpireMap::new();
        map.set(key("a"), Duration::from_millis(50));
        map.set(key("b"), Duration::from_secs(1));

        sleep(Duration::from_millis(70));
        let expired = map.purge();

        assert!(expired.contains(&key("a")));
        assert!(!expired.contains(&key("b")));
        assert!(!map.get(b"a"));
        assert!(map.get(b"b"));
    }

    #[test]
    fn test_reinsert_key_updates_deadline() {
        let mut map = ExpireMap::new();
        map.set(key("foo"), Duration::from_millis(50));
        sleep(Duration::from_millis(30));
        map.set(key("foo"), Duration::from_secs(1));

        sleep(Duration::from_millis(40));
        assert!(map.get(b"foo"));
    }

    #[test]
    fn test_default_impl() {
        let mut map: ExpireMap = Default::default();
        assert!(!map.get(b"nope"));
    }
}
