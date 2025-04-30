use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::Lazy;

/// Пул для повторного использования Arc<str> по одинаковым именам каналов.
/// Crate-private: другие модули внутри этого крейта видят, а внешние — нет.
static CHANNEL_INTERN: Lazy<DashMap<String, Arc<str>>> = Lazy::new(DashMap::new);

/// Возвращает interned Arc<str> для данного канала.
/// При первом вызове для нового имени создаёт Arc<str> и сохраняет его в пуле.
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

    #[test]
    fn intern_new_and_repeats() {
        // первый раз создаётся Arc<str> с нужным текстом.
        let a1 = intern_channel("kin");
        assert_eq!(&*a1, "kin");

        // второй раз pointer должен совпадать
        let a2 = intern_channel("kin");
        assert!(
            Arc::ptr_eq(&a1, &a2),
            "Должен вернуть тот же Arc по указателю"
        );
    }

    #[test]
    fn intern_different_keys() {
        // дава разных имени - разные Arc
        let a1 = intern_channel("dzadza");
        let a2 = intern_channel("maz");
        assert_eq!(&*a1, "dzadza");
        assert_eq!(&*a2, "maz");
        assert!(!Arc::ptr_eq(&a1, &a2), "Разные ключи - разные Arc");
    }

    #[test]
    fn intern_mixed_static_and_string() {
        // строковый и статичный вариант при одинаковом тексте.
        let s = String::from("hello");
        let a1 = intern_channel(&s as &str);
        let a2 = intern_channel("hello");
        assert!(Arc::ptr_eq(&a1, &a2), "Arc должен выдаваться единообразно");
    }
}
