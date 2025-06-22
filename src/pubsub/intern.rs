use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::Lazy;

/// Пул для повторного использования `Arc<str>` для одинаковых имён каналов.
/// Видимость: внутри текущего крейта (crate-private), недоступен снаружи.
static CHANNEL_INTERN: Lazy<DashMap<String, Arc<str>>> = Lazy::new(DashMap::new);

/// Возвращает интернированный `Arc<str>` для заданного имени канала.
/// При первом вызове с новым именем создаёт `Arc<str>` и сохраняет его в пуле.
#[inline(always)]
pub(crate) fn intern_channel<S: AsRef<str>>(chan: S) -> Arc<str> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Проверяет, что первый вызов создаёт `Arc<str>` с правильным содержимым,
    /// а повторный вызов возвращает тот же самый объект (без копирования).
    #[test]
    fn intern_new_and_repeats() {
        // Первый вызов создаёт Arc<str> с ожидаемым текстом.
        let a1 = intern_channel("kin");
        assert_eq!(&*a1, "kin");

        // Второй вызов должен вернуть идентичный Arc (по указателю).
        let a2 = intern_channel("kin");
        assert!(Arc::ptr_eq(&a1, &a2), "Should return the same Arc instance");
    }

    /// Проверяет, что разные имена каналов дают разные `Arc<str>`.
    #[test]
    fn intern_different_keys() {
        // Два разных имени → два разных Arc
        let a1 = intern_channel("dzadza");
        let a2 = intern_channel("maz");
        assert_eq!(&*a1, "dzadza");
        assert_eq!(&*a2, "maz");
        assert!(
            !Arc::ptr_eq(&a1, &a2),
            "Different keys should yield different Arcs"
        );
    }

    /// Проверяет, что `String` и строковый литерал с одинаковым содержимым
    /// интернируются в один и тот же `Arc<str>`.
    #[test]
    fn intern_mixed_static_and_string() {
        // Строка и литерал с одинаковым текстом
        let s = String::from("hello");
        let a1 = intern_channel(&s as &str);
        let a2 = intern_channel("hello");
        assert!(Arc::ptr_eq(&a1, &a2), "Arc instances should be identical");
    }

    /// Проверяет, что параллельные вызовы `intern_channel`
    /// с одним и тем же ключом из разных потоков возвращают один и тот же `Arc<str>`.
    #[test]
    fn intern_concurrent() {
        let keys = ["a", "b", "a", "c", "b", "a"];
        let handles: Vec<_> = keys
            .iter()
            .map(|&k| std::thread::spawn(move || intern_channel(k)))
            .collect();

        let arcs: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Все `"a"` должны ссылаться на один и тот же Arc.
        let a1 = arcs[0].clone();

        for arc in arcs.iter().filter(|arc| (*arc).as_ref() == "a") {
            assert!(
                Arc::ptr_eq(&a1, arc),
                "All interned Arcs for \"a\" should refer to the same instance"
            );
        }
    }
}
