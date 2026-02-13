use std::collections::BTreeMap;

use proptest::prelude::*;
use zumic::SkipList;

proptest! {
    #[test]
    fn prop_behaves_like_btreemap(ops in prop::collection::vec(
        (0u8..3, -100i32..100, -1000i32..1000), 0..200
    )) {
        let mut sl = SkipList::<i32, i32>::new();
        let mut map = BTreeMap::new();

        for (op, key, value) in ops {
            match op {
                0 => { // insert
                    sl.insert(key, value);
                    map.insert(key, value);
                }
                1 => { // remove
                    let r1 = sl.remove(&key);
                    let r2 = map.remove(&key);
                    prop_assert_eq!(r1, r2);
                }
                2 => { // search
                    let r1 = sl.search(&key);
                    let r2 = map.get(&key).cloned();
                    prop_assert_eq!(r1, r2);
                }
                _ => unreachable!(),
            }

            prop_assert_eq!(sl.len(), map.len());
            prop_assert!(sl.validate_invariants().is_ok());
        }

        // финальная проверка порядка
        let sl_items: Vec<_> = sl.iter().collect();
        let map_items: Vec<_> = map.into_iter().collect();
        prop_assert_eq!(sl_items, map_items);
    }
}

proptest! {
    #[test]
    fn prop_iteration_is_sorted(keys in prop::collection::vec(-1000i32..1000, 0..200)) {
        let mut sl = SkipList::<i32, i32>::new();

        for k in &keys {
            sl.insert(*k, *k);
        }

        let mut prev: Option<i32> = None;

        for (k, _) in sl.iter() {
            if let Some(p) = prev {
                prop_assert!(k > p);
            }
            prev = Some(k);
        }

        prop_assert!(sl.validate_invariants().is_ok());
    }
}

proptest! {
    #[test]
    fn prop_clone_equivalent(keys in prop::collection::vec(-1000i32..1000, 0..200)) {
        let mut sl = SkipList::<i32, i32>::new();

        for k in &keys {
            sl.insert(*k, *k * 2);
        }

        let cloned = sl.clone();

        prop_assert_eq!(&sl, &cloned);
        prop_assert!(cloned.validate_invariants().is_ok());
    }
}

proptest! {
    #[test]
    fn prop_serde_roundtrip(keys in prop::collection::vec(-1000i32..1000, 0..200)) {
        let mut sl = SkipList::<i32, i32>::new();

        for k in &keys {
            sl.insert(*k, *k + 1);
        }

        let json = serde_json::to_string(&sl).unwrap();
        let restored: SkipList<i32, i32> = serde_json::from_str(&json).unwrap();

        prop_assert_eq!(&sl, &restored);
        prop_assert!(restored.validate_invariants().is_ok());
    }
}
