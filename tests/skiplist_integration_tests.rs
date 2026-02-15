use zumic::SkipList;

#[test]
fn test_empty_list() {
    let list: SkipList<i32, String> = SkipList::new();

    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
    assert_eq!(list.first(), None);
    assert_eq!(list.last(), None);
    assert_eq!(list.search(&1), None);
}

#[test]
fn test_single_element() {
    let mut list: SkipList<i32, &str> = SkipList::new();

    list.insert(42, "answer");

    // first/last возвращают Option<(&K, &V)> — приводим к owned кортежу через map +
    // deref
    assert_eq!(list.len(), 1);
    assert_eq!(list.first().map(|(k, v)| (*k, *v)), Some((42, "answer")));
    assert_eq!(list.last().map(|(k, v)| (*k, *v)), Some((42, "answer")));
    // search возвращает Option<&V>, можно сравнить через cloned() или deref
    assert_eq!(list.search(&42).copied(), Some("answer"));

    assert_eq!(list.remove(&42), Some("answer"));
    assert!(list.is_empty());
}

#[test]
fn test_two_elements() {
    let mut list: SkipList<i32, &str> = SkipList::new();

    list.insert(1, "one");
    list.insert(2, "two");

    assert_eq!(list.len(), 2);
    assert_eq!(list.first().map(|(k, v)| (*k, *v)), Some((1, "one")));
    assert_eq!(list.last().map(|(k, v)| (*k, *v)), Some((2, "two")));
}

#[test]
fn test_remove_only_element() {
    let mut list: SkipList<i32, &str> = SkipList::new();

    list.insert(1, "one");

    assert_eq!(list.remove(&1), Some("one"));
    assert!(list.is_empty());
    assert_eq!(list.first(), None);
}

#[test]
fn test_remove_first_of_many() {
    let mut list: SkipList<i32, &str> = SkipList::new();
    list.insert(1, "one");
    list.insert(2, "two");
    list.insert(3, "three");

    list.remove(&1);
    assert_eq!(list.first().map(|(k, v)| (*k, *v)), Some((2, "two")));
    assert_eq!(list.len(), 2);
}

#[test]
fn test_remove_last_of_many() {
    let mut list: SkipList<i32, &str> = SkipList::new();

    list.insert(1, "one");
    list.insert(2, "two");
    list.insert(3, "three");

    list.remove(&3);

    assert_eq!(list.last().map(|(k, v)| (*k, *v)), Some((2, "two")));
    assert_eq!(list.len(), 2);
}

#[test]
fn test_remove_middle_element() {
    let mut list: SkipList<i32, &str> = SkipList::new();
    list.insert(1, "one");
    list.insert(2, "two");
    list.insert(3, "three");

    list.remove(&2);
    assert_eq!(list.len(), 2);
    assert!(list.contains(&1));
    assert!(!list.contains(&2));
    assert!(list.contains(&3));
}

#[test]
fn test_insert_ascending_order() {
    let mut list: SkipList<i32, String> = SkipList::new();

    for i in 0..100 {
        list.insert(i, format!("value_{i}"));
    }

    assert_eq!(list.len(), 100);
    // first().map(|(k,v)| (k.clone(), v.clone())) -> owned tuple для сравнения
    assert_eq!(
        list.first().map(|(k, v)| (*k, v.clone())),
        Some((0, "value_0".to_string()))
    );
    assert_eq!(
        list.last().map(|(k, v)| (*k, v.clone())),
        Some((99, "value_99".to_string()))
    );
}

#[test]
fn test_insert_descending_order() {
    let mut list: SkipList<i32, String> = SkipList::new();

    for i in (0..100).rev() {
        list.insert(i, format!("value_{i}"));
    }

    assert_eq!(list.len(), 100);
    assert_eq!(
        list.first().map(|(k, v)| (*k, v.clone())),
        Some((0, "value_0".to_string()))
    );
    assert_eq!(
        list.last().map(|(k, v)| (*k, v.clone())),
        Some((99, "value_99".to_string()))
    );
}

#[test]
fn test_insert_random_then_remove_all() {
    let mut list: SkipList<i32, String> = SkipList::new();
    let keys: Vec<i32> = (0..50).collect();

    for &k in &keys {
        list.insert(k, format!("v{k}"));
    }

    for &k in &keys {
        assert!(list.remove(&k).is_some());
    }

    assert!(list.is_empty());
}

#[test]
fn test_duplicate_inserts() {
    let mut list: SkipList<i32, &str> = SkipList::new();

    list.insert(1, "first");
    list.insert(1, "second");
    list.insert(1, "third");

    assert_eq!(list.len(), 1);
    // search возвращает Option<&V>, поэтому .copied()
    assert_eq!(list.search(&1).copied(), Some("third"));
}

#[test]
fn test_alternating_insert_remove() {
    let mut list: SkipList<i32, String> = SkipList::new();

    for i in 0..100 {
        list.insert(i, format!("v{i}"));

        if i % 2 == 0 {
            // удаляем половину вставленных ранее ключей
            let _ = list.remove(&(i / 2));
        }
    }

    // Должно остаться не пусто и меньше 100 элементов
    assert!(!list.is_empty());
    assert!(list.len() < 100);
}

#[test]
fn test_large_dataset_sequential() {
    let mut list: SkipList<i32, i32> = SkipList::new();
    let n = 10_000;

    // Вставка
    for i in 0..n {
        list.insert(i, i * 2);
    }

    assert_eq!(list.len() as i32, n);

    // Поиск всех — search возвращает Option<&V>, используем cloned()
    for i in 0..n {
        assert_eq!(list.search(&i).cloned(), Some(i * 2));
    }

    // Удаление чётных
    for i in (0..n).step_by(2) {
        assert!(list.remove(&i).is_some());
    }
    assert_eq!(list.len() as i32, n / 2);

    // Проверка оставшихся
    for i in (1..n).step_by(2) {
        assert!(list.contains(&i));
    }
}

#[test]
fn test_many_duplicates() {
    let mut list: SkipList<i32, &str> = SkipList::new();

    for _ in 0..1000 {
        list.insert(1, "value");
    }

    assert_eq!(list.len(), 1);
    assert_eq!(list.search(&1).copied(), Some("value"));
}

#[test]
fn test_no_memory_leak_after_clear() {
    let mut list: SkipList<i32, String> = SkipList::new();

    for i in 0..1000 {
        list.insert(i, format!("value_{i}"));
    }

    list.clear();
    assert!(list.is_empty());

    // Повторная вставка должна работать нормально
    for i in 0..1000 {
        list.insert(i, format!("new_value_{i}"));
    }
    assert_eq!(list.len(), 1000);
}

#[test]
fn test_no_memory_leak_after_remove_all() {
    let mut list: SkipList<i32, String> = SkipList::new();

    for i in 0..1000 {
        list.insert(i, format!("value_{i}"));
    }

    for i in 0..1000 {
        list.remove(&i);
    }

    assert!(list.is_empty());

    // Повторная вставка
    for i in 0..1000 {
        list.insert(i, format!("new_{i}"));
    }
    assert_eq!(list.len(), 1000);
}
