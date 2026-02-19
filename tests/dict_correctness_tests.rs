use zumic::Dict;

#[test]
fn test_get_immutable_multiple_borrows() {
    let mut d = Dict::new();

    d.insert("x", 10u32);
    d.insert("y", 20u32);
    d.insert("z", 30u32);

    // Все три ссылки живут одновременно - невозможно с &mut self
    let vx = d.get(&"x").unwrap();
    let vy = d.get(&"y").unwrap();
    let vz = d.get(&"z").unwrap();

    assert_eq!(*vx + *vy + *vz, 60);
}

#[test]
fn test_via_shared_reference() {
    let mut d = Dict::new();

    d.insert("hello", "world");

    // Превращаем в иммутабильную ссылку явно.
    let shared: &Dict<&str, &str> = &d;

    assert_eq!(shared.get(&"hello"), Some(&"world"));
    assert_eq!(shared.get(&"nope"), None);
}

#[test]
fn test_get_mut_increment() {
    let mut d = Dict::new();

    d.insert("counter", 0u64);

    for _ in 0..100 {
        *d.get_mut(&"counter").unwrap() += 1;
    }

    assert_eq!(d.get(&"counter"), Some(&100u64));
}

#[test]
fn test_get_mut_absent_key_returns_none() {
    let mut d: Dict<u32, u32> = Dict::new();

    assert!(d.get_mut(&0).is_none());

    d.insert(1, 100);

    assert!(d.get_mut(&0).is_none()); // 0 всё ещё отсутствует
    assert!(d.get_mut(&1).is_some()); // 1 есть
}

#[test]
fn test_get_mut_different_keys_sequential() {
    let mut d = Dict::new();

    d.insert("a", 1i32);
    d.insert("b", 2i32);

    *d.get_mut(&"a").unwrap() *= 10;
    *d.get_mut(&"b").unwrap() *= 10;

    assert_eq!(d.get(&"a"), Some(&10));
    assert_eq!(d.get(&"b"), Some(&20));
}

#[test]
fn test_first_insert_initializes_storage() {
    let mut d: Dict<u64, u64> = Dict::new();

    // Вставляем ровно один элемент
    assert!(d.insert(42, 99));
    assert_eq!(d.len(), 1);
    assert_eq!(d.get(&42), Some(&99));
}

#[test]
fn test_insert_after_clear_reinitializes() {
    let mut d = Dict::new();

    d.insert("before", 1);
    d.clear();

    // clear оставляет таблицу в состоянии size == 0
    assert!(d.is_empty());

    // Первая вставка после clear должна работать
    assert!(d.insert("after", 2));
    assert_eq!(d.get(&"after"), Some(&2));
    assert_eq!(d.len(), 1);
}

#[test]
fn test_multiple_inserts_after_clear() {
    let mut d = Dict::new();

    for i in 0..50u32 {
        d.insert(i, i * 2);
    }

    d.clear();

    for i in 100..150u32 {
        d.insert(i, i * 3);
    }

    assert_eq!(d.len(), 50);

    for i in 100..150u32 {
        assert_eq!(d.get(&i), Some(&(i * 3)));
    }
}

#[test]
fn test_no_stack_overflow_on_deep_chains() {
    let mut d = Dict::new();
    const N: u64 = 50_000;

    for i in 0..N {
        d.insert(i, i);
    }

    assert_eq!(d.len() as u64, N);

    for i in 0..N {
        assert!(d.remove(&i), "key {i} not found while deleting");
    }

    assert!(d.is_empty());
}

#[test]
fn test_remove_in_reverse_order() {
    let mut d = Dict::new();

    for i in 0..1_000u32 {
        d.insert(i, i);
    }

    for i in (0..1_000u32).rev() {
        assert!(d.remove(&i));
    }

    assert!(d.is_empty());
}

#[test]
fn test_dict_all_ops() {
    let mut d: Dict<i32, i32> = Dict::new();

    assert_eq!(d.len(), 0);
    assert!(d.is_empty());
    assert_eq!(d.get(&0), None);
    assert_eq!(d.get_mut(&0), None);
    assert!(!d.remove(&0));
    assert_eq!(d.iter().next(), None);

    d.clear(); // ояистка пустого словаря не паникует

    assert!(d.is_empty());
}

#[test]
fn test_single_element_lifecycle() {
    let mut d = Dict::new();

    assert!(d.insert("foo", 1u32));
    assert_eq!(d.len(), 1);
    assert!(!d.is_empty());

    assert_eq!(d.get(&"foo"), Some(&1));
    assert_eq!(d.get(&"bar"), None);

    *d.get_mut(&"foo").unwrap() = 2;

    assert_eq!(d.get(&"foo"), Some(&2));

    assert!(d.remove(&"foo"));
    assert_eq!(d.len(), 0);
    assert!(d.is_empty());
    assert_eq!(d.get(&"foo"), None);
    assert!(!d.remove(&"foo")); // повторное удаление
}

#[test]
fn test_overwrite_same_key_many_times() {
    let mut d = Dict::new();

    d.insert("k", 0u32);

    for v in 1..=1_000u32 {
        let is_new = d.insert("k", v);
        assert!(!is_new, "reinsert should return false");
    }

    assert_eq!(d.len(), 1);
    assert_eq!(d.get(&"k"), Some(&1_000));
}

#[test]
fn test_iter_count_matches_len() {
    let mut d = Dict::new();

    for i in 0..37u32 {
        d.insert(i, i);
    }

    let count = d.iter().count();

    assert_eq!(count, d.len());
}

#[test]
fn test_iter_covers_both_tables_during_rehash() {
    let mut d = Dict::new();

    // Вставляем достаточно элементов для запуска рехеширования
    for i in 0..32u32 {
        d.insert(i, i);
    }

    // Итерируем не завершая рехеширование - итератор обязан обойти обе таблицы
    let collected: Vec<u32> = d.iter().map(|(_, v)| *v).collect();
    assert_eq!(collected.len(), 32, "the iterator skipped elements");

    // Повторяем уникальность - дубликатов быть не должно.
    let mut sorted = collected.clone();

    sorted.sort();
    sorted.dedup();

    assert_eq!(sorted.len(), 32, "the iterator returned duplicates");
}

#[test]
fn test_get_finds_keys_during_rehash() {
    let mut d = Dict::new();

    for i in 0..20u32 {
        d.insert(i, i * 10);
    }

    // Вызываем insert для старта рехеширования и частичного продвижения.
    // После нескольких вставок ht[0] и ht[1] могут оба содержать данные.
    for i in 20..30u32 {
        d.insert(i, i * 10);
    }

    // Все ключи должны быть найдены независимо от стадии рехеширования.
    for i in 0..30u32 {
        assert_eq!(d.get(&i), Some(&(i * 10)), "key {i} not found");
    }
}

#[test]
fn test_remove_during_rehash_both_tables() {
    let mut d = Dict::new();

    for i in 0..20u32 {
        d.insert(i, i);
    }

    // Удаляем половину - часть может находиться в ht[0], часть в ht[1]
    for i in (0..20u32).step_by(2) {
        assert!(d.remove(&i), "key {i} not found while deleting");
    }

    // Чётные удалены, нечётные на месте.
    for i in (0..20u32).step_by(2) {
        assert_eq!(d.get(&i), None, "even key {i} must not exist");
    }

    for i in (1..20u32).step_by(2) {
        assert_eq!(d.get(&i), Some(&i), "odd key {i} must exist");
    }
}
