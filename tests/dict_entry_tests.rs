use zumic::{database::entry::Entry, Dict};

#[test]
fn test_vacant_or_insert_adds_element() {
    let mut d: Dict<&str, u32> = Dict::new();
    let v = d.entry("foo").or_insert(99);

    assert_eq!(*v, 99);
    assert_eq!(d.len(), 1);
    assert_eq!(d.get(&"foo"), Some(&99));
}

#[test]
fn test_occupied_or_insert_keeps_existing() {
    let mut d: Dict<&str, u32> = Dict::new();

    d.insert("foo", 1);

    let v = d.entry("foo").or_insert(999);

    assert_eq!(*v, 1); // старое значение
    assert_eq!(d.len(), 1);
}

#[test]
fn test_or_insert_returns_mut_ref_can_be_modified() {
    let mut d: Dict<&str, u32> = Dict::new();

    *d.entry("foo").or_insert(0) += 5;
    *d.entry("foo").or_insert(0) += 3;

    assert_eq!(d.get(&"foo"), Some(&8));
}

#[test]
fn test_or_insert_with_not_called_if_occupied() {
    let mut d: Dict<u32, Vec<u32>> = Dict::new();

    d.insert(1, vec![10]);

    let mut calls = 0usize;

    d.entry(1).or_insert_with(|| {
        calls += 1;
        vec![20]
    });

    assert_eq!(calls, 0);
    assert_eq!(d.get(&1), Some(&vec![10]));
}

#[test]
fn test_or_insert_with_called_once_if_vacant() {
    let mut d: Dict<u32, Vec<u32>> = Dict::new();
    let mut calls = 0usize;

    d.entry(1).or_insert_with(|| {
        calls += 1;
        vec![1, 2, 3]
    });
    d.entry(1).or_insert_with(|| {
        calls += 1;
        vec![4, 5, 6]
    });

    assert_eq!(calls, 1);
    assert_eq!(d.get(&1), Some(&vec![1, 2, 3]));
}

#[test]
fn test_or_insert_with_key_computes_from_key() {
    let mut d: Dict<u32, u32> = Dict::new();

    d.entry(6).or_insert_with_key(|&k| k * k);

    assert_eq!(d.get(&6), Some(&36));
}

#[test]
fn test_or_insert_with_key_not_called_if_occupied() {
    let mut d: Dict<u32, u32> = Dict::new();

    d.insert(3, 100);

    let mut calls = 0usize;

    d.entry(3).or_insert_with_key(|_| {
        calls += 1;
        999
    });

    assert_eq!(calls, 0);
    assert_eq!(d.get(&3), Some(&100));
}

#[test]
fn test_or_default_inserts_default_value() {
    let mut d: Dict<u32, String> = Dict::new();
    let v = d.entry(1).or_default();

    assert!(v.is_empty());
    assert_eq!(d.get(&1), Some(&String::new()));
}

#[test]
fn test_or_default_returns_existing_if_occupied() {
    let mut d: Dict<u32, u32> = Dict::new();

    d.insert(1, 42);

    let v = d.entry(1).or_default();

    assert_eq!(*v, 42);
}

#[test]
fn test_and_modify_called_if_occupied() {
    let mut d: Dict<&str, i32> = Dict::new();

    d.insert("foo", 10);
    d.entry("foo").and_modify(|v| *v *= 3).or_insert(0);

    assert_eq!(d.get(&"foo"), Some(&30));
}

#[test]
fn test_and_modify_not_called_if_vacant() {
    let mut d: Dict<&str, i32> = Dict::new();
    let mut called = false;

    d.entry("foo").and_modify(|_| called = true).or_insert(1);

    assert!(!called);
    assert_eq!(d.get(&"foo"), Some(&1));
}

#[test]
fn test_and_modify_chained_pattern() {
    let mut d: Dict<&str, u32> = Dict::new();

    for _ in 0..5 {
        d.entry("x").and_modify(|v| *v += 1).or_insert(1);
    }

    assert_eq!(d.get(&"x"), Some(&5));
}

#[test]
fn occupied_key_returns_correct_key() {
    let mut d: Dict<String, u32> = Dict::new();
    d.insert("hello".to_string(), 1);
    if let Entry::Occupied(e) = d.entry("hello".to_string()) {
        assert_eq!(e.key(), "hello");
    } else {
        panic!("expected Occupied");
    }
}

#[test]
fn occupied_get_returns_current_value() {
    let mut d: Dict<u32, u32> = Dict::new();
    d.insert(7, 77);
    if let Entry::Occupied(e) = d.entry(7) {
        assert_eq!(*e.get(), 77);
    } else {
        panic!("expected Occupied");
    }
}

#[test]
fn occupied_get_mut_modifies_in_place() {
    let mut d: Dict<u32, u32> = Dict::new();
    d.insert(1, 10);
    if let Entry::Occupied(mut e) = d.entry(1) {
        *e.get_mut() += 90;
    } else {
        panic!("expected Occupied");
    }
    assert_eq!(d.get(&1), Some(&100));
}

#[test]
fn occupied_into_mut_lifetime_extends_to_dict() {
    let mut d: Dict<u32, u32> = Dict::new();
    d.insert(1, 1);
    let r: &mut u32 = match d.entry(1) {
        Entry::Occupied(e) => e.into_mut(),
        Entry::Vacant(_) => panic!("expected Occupied"),
    };
    *r = 42;
    // После освобождения r — проверяем через shared borrow.
    assert_eq!(d.get(&1), Some(&42));
}

#[test]
fn occupied_insert_replaces_and_returns_old() {
    let mut d: Dict<&str, String> = Dict::new();
    d.insert("k", "old".into());
    if let Entry::Occupied(mut e) = d.entry("k") {
        let old = e.insert("new".into());
        assert_eq!(old, "old");
        assert_eq!(e.get(), "new");
    } else {
        panic!("expected Occupied");
    }
    assert_eq!(d.get(&"k").map(|s| s.as_str()), Some("new"));
}

#[test]
fn occupied_remove_deletes_entry() {
    let mut d: Dict<u32, String> = Dict::new();
    d.insert(42, "bye".into());
    assert_eq!(d.len(), 1);
    let val = match d.entry(42) {
        Entry::Occupied(e) => e.remove(),
        Entry::Vacant(_) => panic!("expected Occupied"),
    };
    assert_eq!(val, "bye");
    assert_eq!(d.len(), 0);
    assert_eq!(d.get(&42), None);
}

#[test]
fn occupied_remove_head_of_chain() {
    let mut d: Dict<u32, u32> = Dict::new();
    // Принудительно вставляем несколько элементов.
    for i in 0..16u32 {
        d.insert(i, i);
    }
    // Удаляем первый и проверяем, что остальные целы.
    if let Entry::Occupied(e) = d.entry(0) {
        e.remove();
    }
    assert_eq!(d.get(&0), None);
    for i in 1..16u32 {
        assert_eq!(d.get(&i), Some(&i));
    }
    assert_eq!(d.len(), 15);
}

#[test]
fn occupied_remove_inside_chain() {
    let mut d: Dict<u32, u32> = Dict::new();
    for i in 0..20u32 {
        d.insert(i, i * 10);
    }
    // Удаляем каждый второй.
    for i in (0..20u32).step_by(2) {
        if let Entry::Occupied(e) = d.entry(i) {
            e.remove();
        }
    }
    assert_eq!(d.len(), 10);
    for i in (1..20u32).step_by(2) {
        assert_eq!(d.get(&i), Some(&(i * 10)));
    }
    for i in (0..20u32).step_by(2) {
        assert_eq!(d.get(&i), None);
    }
}

#[test]
fn vacant_key_does_not_insert() {
    let mut d: Dict<String, u32> = Dict::new();
    if let Entry::Vacant(e) = d.entry("ghost".to_string()) {
        assert_eq!(e.key(), "ghost");
    } else {
        panic!("expected Vacant");
    }
    assert!(d.is_empty());
}

#[test]
fn vacant_into_key_returns_key_without_insert() {
    let mut d: Dict<String, u32> = Dict::new();
    let key = match d.entry("ghost".to_string()) {
        Entry::Vacant(e) => e.into_key(),
        Entry::Occupied(_) => panic!("expected Vacant"),
    };
    assert_eq!(key, "ghost");
    assert!(d.is_empty());
}

#[test]
fn vacant_insert_allows_chained_modification() {
    let mut d: Dict<u32, Vec<u32>> = Dict::new();
    if let Entry::Vacant(e) = d.entry(1) {
        let v = e.insert(vec![1, 2, 3]);
        v.push(4);
        v.push(5);
    } else {
        panic!("expected Vacant");
    }
    assert_eq!(d.get(&1), Some(&vec![1, 2, 3, 4, 5]));
}

#[test]
fn entry_works_during_active_rehash() {
    let mut d: Dict<u64, u64> = Dict::new();
    // Провоцируем рехеш.
    for i in 0..30u64 {
        d.insert(i, i);
    }

    // Модифицируем существующий ключ.
    *d.entry(10).or_insert(0) += 100;
    assert_eq!(d.get(&10), Some(&110));

    // Вставляем новый ключ.
    d.entry(99999).or_insert(42);
    assert_eq!(d.get(&99999), Some(&42));

    // Все оригинальные ключи, кроме 10, не изменились.
    for i in 0..30u64 {
        if i == 10 {
            assert_eq!(d.get(&i), Some(&110));
        } else {
            assert_eq!(d.get(&i), Some(&i));
        }
    }
}

#[test]
fn entry_does_not_create_duplicates() {
    let mut d: Dict<u32, u32> = Dict::new();
    d.insert(1, 10);
    for _ in 0..10 {
        d.entry(1).or_insert(999);
    }
    assert_eq!(d.len(), 1);
    assert_eq!(d.get(&1), Some(&10));
}

#[test]
fn entry_remove_then_reinsert_via_entry() {
    let mut d: Dict<u32, u32> = Dict::new();
    d.insert(1, 10);
    if let Entry::Occupied(e) = d.entry(1) {
        e.remove();
    }
    assert_eq!(d.get(&1), None);
    d.entry(1).or_insert(20);
    assert_eq!(d.get(&1), Some(&20));
}

#[test]
fn entry_large_scale_correctness() {
    let mut d: Dict<u64, u64> = Dict::new();
    const N: u64 = 2_000;
    for i in 0..N {
        d.entry(i).or_insert(0);
        *d.entry(i).or_insert(0) += 1;
    }
    assert_eq!(d.len() as u64, N);
    for i in 0..N {
        assert_eq!(d.get(&i), Some(&1));
    }
}
