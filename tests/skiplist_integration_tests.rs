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
    let mut list = SkipList::new();

    list.insert(42, "answer");

    assert_eq!(list.len(), 1);
    assert_eq!(list.first(), Some((42, "answer")));
    assert_eq!(list.last(), Some((42, "answer")));
    assert_eq!(list.search(&42), Some("answer"));

    list.remove(&42);

    assert!(list.is_empty());
}

#[test]
fn test_two_elements() {
    let mut list = SkipList::new();

    list.insert(1, "one");
    list.insert(2, "two");

    assert_eq!(list.len(), 2);
    assert_eq!(list.first(), Some((1, "one")));
    assert_eq!(list.last(), Some((2, "two")));
}

#[test]
fn test_remove_only_element() {
    let mut list = SkipList::new();

    list.insert(1, "one");

    assert_eq!(list.remove(&1), Some("one"));
    assert!(list.is_empty());
    assert_eq!(list.first(), None);
}

#[test]
fn test_remove_first_of_many() {
    let mut list = SkipList::new();
    list.insert(1, "one");
    list.insert(2, "two");
    list.insert(3, "three");

    list.remove(&1);
    assert_eq!(list.first(), Some((2, "two")));
    assert_eq!(list.len(), 2);
}

#[test]
fn test_remove_last_of_many() {
    let mut list = SkipList::new();

    list.insert(1, "one");
    list.insert(2, "two");
    list.insert(3, "three");

    list.remove(&3);

    assert_eq!(list.last(), Some((2, "two")));
    assert_eq!(list.len(), 2);
}

#[test]
fn test_remove_middle_element() {
    let mut list = SkipList::new();
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
    let mut list = SkipList::new();

    for i in 0..100 {
        list.insert(i, format!("value_{i}"));
    }

    assert_eq!(list.len(), 100);
    assert_eq!(list.first(), Some((0, "value_0".to_string())));
    assert_eq!(list.last(), Some((99, "value_99".to_string())));
}

#[test]
fn test_insert_descending_order() {
    let mut list = SkipList::new();

    for i in (0..100).rev() {
        list.insert(i, format!("value_{i}"));
    }

    assert_eq!(list.len(), 100);
    assert_eq!(list.first(), Some((0, "value_0".to_string())));
    assert_eq!(list.last(), Some((99, "value_99".to_string())));
}

#[test]
fn test_insert_random_then_remove_all() {
    let mut list = SkipList::new();
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
    let mut list = SkipList::new();
    list.insert(1, "first");
    list.insert(1, "second");
    list.insert(1, "third");

    assert_eq!(list.len(), 1);
    assert_eq!(list.search(&1), Some("third"));
}

#[test]
fn test_alternating_insert_remove() {
    let mut list = SkipList::new();

    for i in 0..100 {
        list.insert(i, format!("v{i}"));

        if i % 2 == 0 {
            list.remove(&(i / 2));
        }
    }

    // Должно остаться примерно половина элементов
    assert!(!list.is_empty());
    assert!(list.len() < 100);
}

#[test]
fn test_large_dataset_sequential() {
    let mut list = SkipList::new();
    let n = 10_000;

    // Вставка
    for i in 0..n {
        list.insert(i, i * 2);
    }

    assert_eq!(list.len(), n);

    // Поиск всех
    for i in 0..n {
        assert_eq!(list.search(&i), Some(i * 2));
    }

    // Удаление чётных
    for i in (0..n).step_by(2) {
        assert!(list.remove(&i).is_some());
    }
    assert_eq!(list.len(), n / 2);

    // Проверка оставшихся
    for i in (1..n).step_by(2) {
        assert!(list.contains(&i));
    }
}

#[test]
fn test_many_duplicates() {
    let mut list = SkipList::new();

    for _ in 0..1000 {
        list.insert(1, "value");
    }

    assert_eq!(list.len(), 1);
    assert_eq!(list.search(&1), Some("value"));
}

#[test]
fn test_no_memory_leak_after_clear() {
    let mut list = SkipList::new();

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
    let mut list = SkipList::new();

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
