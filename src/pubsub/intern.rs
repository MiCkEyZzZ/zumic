use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::Lazy;

/// Пул для повторного использования Arc<str> по одинаковым именам каналов.
/// Crate-private: другие модули внутри этого крейта видят, а внешние — нет.
static CHANNEL_INTERN: Lazy<DashMap<String, Arc<str>>> = Lazy::new(DashMap::new);

/// Возвращает interned Arc<str> для данного канала.
/// При первом вызове для нового имени создаёт Arc<str> и сохраняет его в пуле.
pub fn intern_channel<S: AsRef<str>>(chan: S) -> Arc<str> {
    let key = chan.as_ref();
    if let Some(existing) = CHANNEL_INTERN.get(key) {
        existing.clone()
    } else {
        let s = key.to_string();
        let arc: Arc<str> = Arc::from(s.clone());
        CHANNEL_INTERN.insert(s, arc.clone());
        arc
    }
}
