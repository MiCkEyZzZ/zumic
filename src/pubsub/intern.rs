use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::Lazy;

/// A pool for reusing `Arc<str>` for identical channel names.
/// Crate-private: visible to modules within this crate, but not externally.
static CHANNEL_INTERN: Lazy<DashMap<String, Arc<str>>> = Lazy::new(DashMap::new);

/// Returns an interned `Arc<str>` for the given channel name.
/// On first invocation for a new name, creates an `Arc<str>` and stores it in the pool.
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

    /// Verifies that the first call creates an `Arc<str>` with the correct contents,
    /// and a subsequent call returns the same object (zero-copy).
    #[test]
    fn intern_new_and_repeats() {
        // First call creates an Arc<str> with the expected text.
        let a1 = intern_channel("kin");
        assert_eq!(&*a1, "kin");

        // Second call should return the identical Arc by pointer.
        let a2 = intern_channel("kin");
        assert!(Arc::ptr_eq(&a1, &a2), "Should return the same Arc instance");
    }

    /// Verifies that different channel names produce distinct `Arc<str>`.
    #[test]
    fn intern_different_keys() {
        // Two different names → two different Arcs
        let a1 = intern_channel("dzadza");
        let a2 = intern_channel("maz");
        assert_eq!(&*a1, "dzadza");
        assert_eq!(&*a2, "maz");
        assert!(
            !Arc::ptr_eq(&a1, &a2),
            "Different keys should yield different Arcs"
        );
    }

    /// Verifies that both a `String` and a string literal with the same contents
    /// are interned to the same `Arc<str>`.
    #[test]
    fn intern_mixed_static_and_string() {
        // String and literal variants with identical text.
        let s = String::from("hello");
        let a1 = intern_channel(&s as &str);
        let a2 = intern_channel("hello");
        assert!(Arc::ptr_eq(&a1, &a2), "Arc instances should be identical");
    }

    /// Verifies that concurrent calls to `intern_channel`
    /// for the same string across threads return the same `Arc<str>`.
    #[test]
    fn intern_concurrent() {
        let keys = ["a", "b", "a", "c", "b", "a"];
        let handles: Vec<_> = keys
            .iter()
            .map(|&k| std::thread::spawn(move || intern_channel(k)))
            .collect();

        let arcs: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All `"a"` channels should point to the same Arc.
        let a1 = arcs[0].clone();

        for arc in arcs.iter().filter(|arc| (*arc).as_ref() == "a") {
            assert!(
                Arc::ptr_eq(&a1, arc),
                "All interned Arcs for \"a\" should refer to the same instance"
            );
        }
    }
}
