use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier,
    },
    thread,
    time::Duration,
};

use zumic::ConcurrentSkipList;

/// Кол-во итераций для стресс теста
const STRESS_ITERATIONS: usize = 10_000;

/// Кол-во потоков для стресс тестов
const STRESS_THREADS: usize = 16;

#[test]
fn test_concurrent_insert_search_basic() {
    let list = ConcurrentSkipList::new();
    let list_w = list.clone();
    let list_r = list.clone();

    let writer = thread::spawn(move || {
        for i in 0..1000 {
            list_w.insert(i, i * 2);
        }
    });

    let reader = thread::spawn(move || {
        for i in 0..1000 {
            let _ = list_r.search(&i);
        }
    });

    writer.join().unwrap();
    reader.join().unwrap();

    assert_eq!(list.len(), 1000);
}

#[test]
fn test_multiple_concurrent_readers() {
    let list = ConcurrentSkipList::new();
    let mut handles = vec![];

    for i in 0..1000 {
        list.insert(i, i);
    }

    for _ in 0..10 {
        let list_c = list.clone();
        handles.push(thread::spawn(move || {
            for i in 0..1000 {
                assert_eq!(list_c.search(&i), Some(i));
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_concurrent_insert_no_duplicates() {
    let list = ConcurrentSkipList::new();
    let mut handles = vec![];

    for thread_id in 0..8 {
        let list_c = list.clone();
        handles.push(thread::spawn(move || {
            for i in 0..1000 {
                list_c.insert(thread_id * 1000 + i, thread_id);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(list.len(), 8000);
}

#[test]
fn test_concurrent_updates_same_keys() {
    let list = ConcurrentSkipList::new();
    let mut handles = vec![];

    // Предварительно вставляем ключи
    for i in 0..100 {
        list.insert(i, 0);
    }

    // 10 потоков обновляют те же ключи
    for thread_id in 0..10 {
        let list_c = list.clone();
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                list_c.insert(i, thread_id);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Длина не должна измениться
    assert_eq!(list.len(), 100);

    // Каждый ключ должен иметь валидное значение
    for i in 0..100 {
        let val = list.search(&i).unwrap();
        assert!(val < 10);
    }
}

#[test]
fn test_concurrent_insert_remove() {
    let list = ConcurrentSkipList::new();
    let mut handles = vec![];

    let insert_count = Arc::new(AtomicUsize::new(0));
    let remove_count = Arc::new(AtomicUsize::new(0));

    // Писатели
    for i in 0..4 {
        let list_c = list.clone();
        let ins_count_c = Arc::clone(&insert_count);
        handles.push(thread::spawn(move || {
            for j in 0..1000 {
                list_c.insert(i * 1000 + j, j);
                ins_count_c.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    // Удалители (начинают после небольшой задержки)
    for i in 0..2 {
        let list_c = list.clone();
        let r_count_c = Arc::clone(&remove_count);
        handles.push(thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            for j in 0..500 {
                if list_c.remove(&(i * 1000 + j)).is_some() {
                    r_count_c.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let inserted = insert_count.load(Ordering::Relaxed);
    let removed = remove_count.load(Ordering::Relaxed);

    // Проверка корректности
    assert_eq!(list.len(), inserted - removed);
}

#[test]
fn test_stress_concurrent_mixed_ops() {
    let list = ConcurrentSkipList::new();
    let mut handles = vec![];
    let barrier = Arc::new(Barrier::new(STRESS_THREADS));

    for thread_id in 0..STRESS_THREADS {
        let list_c = list.clone();
        let barrier_c = Arc::clone(&barrier);

        handles.push(thread::spawn(move || {
            // Синхронизируем старт всех потоков
            barrier_c.wait();

            let base = thread_id * STRESS_ITERATIONS;

            // Вставляем
            for i in 0..STRESS_ITERATIONS / 2 {
                list_c.insert(base + i, i);
            }

            // Поиск
            for i in 0..STRESS_ITERATIONS / 4 {
                let _ = list_c.search(&(base + i));
            }

            // Удаление
            for i in 0..STRESS_ITERATIONS / 4 {
                let _ = list_c.remove(&(base + i));
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Проверяем, что структура валидна
    assert!(list.validate_invariants().is_ok());
}

#[test]
fn test_no_deadlock_clear_with_active_readers() {
    let list = ConcurrentSkipList::new();

    for i in 0..1000 {
        list.insert(i, i);
    }

    let list_reader = list.clone();
    let reader = thread::spawn(move || {
        for _ in 0..100 {
            for i in 0..1000 {
                let _ = list_reader.search(&i);
            }
        }
    });

    // Очистка во время чтения
    thread::sleep(Duration::from_millis(10));
    list.clear();

    reader.join().unwrap();
    assert_eq!(list.len(), 0);
}

#[test]
fn test_concurrent_first_last() {
    let list = ConcurrentSkipList::new();
    let mut handles = vec![];

    // Писатели
    for i in 0..8 {
        let list = list.clone();
        handles.push(thread::spawn(move || {
            for j in 0..100 {
                list.insert(i * 100 + j, j);
            }
        }));
    }

    // Читатели для первого/последнего
    for _ in 0..4 {
        let list = list.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                let _ = list.first();
                let _ = list.last();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(list.len(), 800);
}
