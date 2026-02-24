use zumic::Sds;

/// Строка ровно `INLINE_CAP` байт.
fn inline_max() -> String {
    "x".repeat(Sds::INLINE_CAP)
}

/// Строка `INLINE_CAP + 1` байт — минимальная heap-строка.
fn heap_min() -> String {
    "x".repeat(Sds::INLINE_CAP + 1)
}

#[test]
fn inline_cap_matches_architecture() {
    // На каждой архитектуре INLINE_CAP = 3 * usize - 1.
    // 64-bit: 23, 32-bit: 11, 16-bit: 5.
    let expected = std::mem::size_of::<usize>() * 3 - 1;
    assert_eq!(Sds::INLINE_CAP, expected);
}

#[test]
fn empty_string_is_inline() {
    let s = Sds::from_str("");
    assert!(s.is_inline());
    assert_eq!(s.len(), 0);
    s.debug_assert_invariants();
}

#[test]
fn single_byte_is_inline() {
    let s = Sds::from_str("a");
    assert!(s.is_inline());
    assert_eq!(s.len(), 1);
    s.debug_assert_invariants();
}

#[test]
fn exactly_inline_cap_is_inline() {
    let s = Sds::from_str(&inline_max());
    assert!(s.is_inline());
    assert_eq!(s.len(), Sds::INLINE_CAP);
    s.debug_assert_invariants();
}

#[test]
fn one_over_inline_cap_is_heap() {
    let s = Sds::from_str(&heap_min());
    assert!(!s.is_inline());
    assert_eq!(s.len(), Sds::INLINE_CAP + 1);
    s.debug_assert_invariants();
}

#[test]
fn from_bytes_empty_is_inline() {
    let s = Sds::from_bytes(b"");
    assert!(s.is_inline());
    assert_eq!(s.len(), 0);
    s.debug_assert_invariants();
}

#[test]
fn from_bytes_exact_cap_is_inline() {
    let data: Vec<u8> = (0..Sds::INLINE_CAP as u8).collect();
    let s = Sds::from_bytes(&data);
    assert!(s.is_inline());
    assert_eq!(s.as_slice(), data.as_slice());
    s.debug_assert_invariants();
}

#[test]
fn from_vec_empty_is_inline() {
    let s = Sds::from_vec(vec![]);
    assert!(s.is_inline());
    s.debug_assert_invariants();
}

#[test]
fn from_vec_inline_cap_is_inline() {
    let v = vec![42u8; Sds::INLINE_CAP];
    let s = Sds::from_vec(v.clone());
    assert!(s.is_inline());
    assert_eq!(s.as_slice(), v.as_slice());
    s.debug_assert_invariants();
}

#[test]
fn from_vec_over_cap_is_heap() {
    let v = vec![42u8; Sds::INLINE_CAP + 1];
    let s = Sds::from_vec(v.clone());
    assert!(!s.is_inline());
    assert_eq!(s.as_slice(), v.as_slice());
    s.debug_assert_invariants();
}

#[test]
fn push_into_empty_inline_stays_inline() {
    let mut s = Sds::default();
    s.push(b'a');
    assert!(s.is_inline());
    assert_eq!(s.as_slice(), b"a");
    s.debug_assert_invariants();
}

#[test]
fn push_fills_inline_to_cap() {
    let mut s = Sds::default();
    for b in 0u8..Sds::INLINE_CAP as u8 {
        s.push(b);
        assert!(s.is_inline());
        s.debug_assert_invariants();
    }
    assert_eq!(s.len(), Sds::INLINE_CAP);
}

#[test]
fn push_over_inline_cap_promotes_to_heap() {
    let mut s = Sds::from_str(&inline_max());
    assert!(s.is_inline());
    s.push(b'!');
    assert!(!s.is_inline());
    assert_eq!(s.len(), Sds::INLINE_CAP + 1);
    assert_eq!(s.as_slice()[Sds::INLINE_CAP], b'!');
    s.debug_assert_invariants();
}

#[test]
fn push_preserves_existing_data_on_promotion() {
    let base = "a".repeat(Sds::INLINE_CAP);
    let mut s = Sds::from_str(&base);
    s.push(b'Z');
    assert_eq!(&s.as_slice()[..Sds::INLINE_CAP], base.as_bytes());
    assert_eq!(s.as_slice()[Sds::INLINE_CAP], b'Z');
    s.debug_assert_invariants();
}

#[test]
fn push_many_into_heap_stays_coherent() {
    let mut s = Sds::default();
    for i in 0u8..=255 {
        s.push(i);
    }
    assert_eq!(s.len(), 256);
    for (i, &b) in s.as_slice().iter().enumerate() {
        assert_eq!(b, i as u8);
    }
    s.debug_assert_invariants();
}

#[test]
fn append_empty_to_empty_stays_empty() {
    let mut s = Sds::default();
    s.append(b"");
    assert!(s.is_empty());
    assert!(s.is_inline());
    s.debug_assert_invariants();
}

#[test]
fn append_empty_to_nonempty_noop() {
    let mut s = Sds::from_str("hello");
    s.append(b"");
    assert_eq!(s.as_slice(), b"hello");
    s.debug_assert_invariants();
}

#[test]
fn append_within_inline_stays_inline() {
    let mut s = Sds::from_str("abc");
    s.append(b"def");
    assert!(s.is_inline());
    assert_eq!(s.as_slice(), b"abcdef");
    s.debug_assert_invariants();
}

#[test]
fn append_to_exact_cap_stays_inline() {
    // Итог ровно INLINE_CAP байт.
    let half = Sds::INLINE_CAP / 2;
    let mut s = Sds::from_str(&"a".repeat(half));
    let rest = "b".repeat(Sds::INLINE_CAP - half);
    s.append(rest.as_bytes());
    assert!(s.is_inline());
    assert_eq!(s.len(), Sds::INLINE_CAP);
    s.debug_assert_invariants();
}

#[test]
fn append_exceeds_inline_promotes_to_heap() {
    let mut s = Sds::from_str(&inline_max());
    s.append(b"X");
    assert!(!s.is_inline());
    assert_eq!(s.len(), Sds::INLINE_CAP + 1);
    s.debug_assert_invariants();
}

#[test]
fn append_preserves_existing_data_on_promotion() {
    let base = "hello";
    let suffix = "this_suffix_pushes_us_into_heap_territory_for_sure";
    let mut s = Sds::from_str(base);
    s.append(suffix.as_bytes());
    assert!(!s.is_inline());
    assert_eq!(&s.as_slice()[..base.len()], base.as_bytes());
    assert_eq!(&s.as_slice()[base.len()..], suffix.as_bytes());
    s.debug_assert_invariants();
}

#[test]
fn append_to_heap_extends_correctly() {
    let mut s = Sds::from_str(&heap_min());
    let extra = "extra";
    let expected_len = Sds::INLINE_CAP + 1 + extra.len();
    s.append(extra.as_bytes());
    assert_eq!(s.len(), expected_len);
    assert_eq!(&s.as_slice()[Sds::INLINE_CAP + 1..], extra.as_bytes());
    s.debug_assert_invariants();
}

#[test]
fn clear_inline_sets_len_zero() {
    let mut s = Sds::from_str("hello");
    s.clear();
    assert_eq!(s.len(), 0);
    assert!(s.is_empty());
    s.debug_assert_invariants();
}

#[test]
fn clear_heap_sets_len_zero_keeps_capacity() {
    let mut s = Sds::from_str(&heap_min());
    let cap = s.capacity();
    s.clear();
    assert_eq!(s.len(), 0);
    assert!(s.is_empty());
    // Heap capacity должна сохраниться.
    assert!(s.capacity() >= cap);
    s.debug_assert_invariants();
}

#[test]
fn clear_then_push_works_correctly() {
    let mut s = Sds::from_str("hello");
    s.clear();
    s.push(b'Z');
    assert_eq!(s.as_slice(), b"Z");
    s.debug_assert_invariants();
}

#[test]
fn clear_heap_then_push_works_correctly() {
    let mut s = Sds::from_str(&heap_min());
    s.clear();
    s.push(b'A');
    assert_eq!(s.as_slice(), b"A");
    s.debug_assert_invariants();
}

#[test]
fn truncate_zero_clears_inline() {
    let mut s = Sds::from_str("hello");
    s.truncate(0);
    assert_eq!(s.len(), 0);
    s.debug_assert_invariants();
}

#[test]
fn truncate_zero_clears_heap() {
    let mut s = Sds::from_str(&heap_min());
    s.truncate(0);
    assert_eq!(s.len(), 0);
    // После truncate(0) на heap → переходит в inline (inline_downgrade).
    assert!(s.is_inline());
    s.debug_assert_invariants();
}

#[test]
fn truncate_noop_when_new_len_equals_current() {
    let mut s = Sds::from_str("hello");
    s.truncate(5);
    assert_eq!(s.as_slice(), b"hello");
    s.debug_assert_invariants();
}

#[test]
fn truncate_noop_when_new_len_exceeds_current() {
    let mut s = Sds::from_str("hello");
    s.truncate(999);
    assert_eq!(s.as_slice(), b"hello");
    s.debug_assert_invariants();
}

#[test]
fn truncate_heap_to_inline_cap_promotes_inline() {
    let long = "a".repeat(Sds::INLINE_CAP * 2);
    let mut s = Sds::from_str(&long);
    assert!(!s.is_inline());
    s.truncate(Sds::INLINE_CAP);
    assert!(s.is_inline());
    assert_eq!(s.len(), Sds::INLINE_CAP);
    s.debug_assert_invariants();
}

#[test]
fn truncate_heap_one_over_cap_stays_heap() {
    let long = "b".repeat(Sds::INLINE_CAP * 2);
    let mut s = Sds::from_str(&long);
    s.truncate(Sds::INLINE_CAP + 1);
    // INLINE_CAP + 1 не умещается в inline.
    assert!(!s.is_inline());
    assert_eq!(s.len(), Sds::INLINE_CAP + 1);
    s.debug_assert_invariants();
}

#[test]
fn truncate_preserves_content() {
    let mut s = Sds::from_str("hello world");
    s.truncate(5);
    assert_eq!(s.as_slice(), b"hello");
    s.debug_assert_invariants();
}

#[test]
fn slice_range_empty_range() {
    let s = Sds::from_str("hello");
    let r = s.slice_range(2, 2);
    assert!(r.is_empty());
    assert!(r.is_inline());
    r.debug_assert_invariants();
}

#[test]
fn slice_range_full() {
    let s = Sds::from_str("hello");
    let r = s.slice_range(0, s.len());
    assert_eq!(r.as_slice(), b"hello");
    r.debug_assert_invariants();
}

#[test]
fn slice_range_from_heap_short_result_is_inline() {
    let long = "a".repeat(Sds::INLINE_CAP + 10);
    let s = Sds::from_str(&long);
    assert!(!s.is_inline());
    let r = s.slice_range(0, 5);
    assert!(r.is_inline());
    assert_eq!(r.len(), 5);
    r.debug_assert_invariants();
}

#[test]
fn slice_range_from_heap_long_result_is_heap() {
    let data: Vec<u8> = (0u8..255).collect();
    let s = Sds::from_bytes(&data);
    let r = s.slice_range(0, Sds::INLINE_CAP + 1);
    assert!(!r.is_inline());
    assert_eq!(r.as_slice(), &data[..Sds::INLINE_CAP + 1]);
    r.debug_assert_invariants();
}

#[test]
#[should_panic]
fn slice_range_invalid_start_gt_end_panics() {
    let s = Sds::from_str("hello");
    let _ = s.slice_range(3, 2);
}

#[test]
#[should_panic]
fn slice_range_end_out_of_bounds_panics() {
    let s = Sds::from_str("hello");
    let _ = s.slice_range(0, 10);
}

#[test]
fn reserve_within_inline_noop() {
    let mut s = Sds::from_str("hi");
    s.reserve(1);
    // Помещается в inline — остаёмся inline.
    assert!(s.is_inline());
    s.debug_assert_invariants();
}

#[test]
fn reserve_exceeding_inline_promotes_to_heap() {
    let mut s = Sds::from_str("hi");
    s.reserve(Sds::INLINE_CAP + 1);
    assert!(!s.is_inline());
    assert!(s.capacity() > 2 + Sds::INLINE_CAP);
    assert_eq!(s.as_slice(), b"hi");
    s.debug_assert_invariants();
}

#[test]
fn reserve_on_heap_increases_capacity() {
    let mut s = Sds::from_str(&heap_min());
    let cap_before = s.capacity();
    s.reserve(1000);
    assert!(s.capacity() >= cap_before + 1000);
    s.debug_assert_invariants();
}

#[test]
fn from_str_trait_short() {
    let s: Sds = "hello".into();
    assert_eq!(s.as_slice(), b"hello");
    s.debug_assert_invariants();
}

#[test]
fn from_string_short_is_inline() {
    let s: Sds = String::from("hi").into();
    assert!(s.is_inline());
    assert_eq!(s.as_str().unwrap(), "hi");
    s.debug_assert_invariants();
}

#[test]
fn from_string_long_is_heap() {
    let original = "a".repeat(Sds::INLINE_CAP + 5);
    let s: Sds = original.clone().into();
    assert!(!s.is_inline());
    assert_eq!(s.as_str().unwrap(), original.as_str());
    s.debug_assert_invariants();
}

#[test]
fn into_vec_from_inline() {
    let s = Sds::from_str("hello");
    let v: Vec<u8> = s.into();
    assert_eq!(v, b"hello");
}

#[test]
fn into_vec_from_heap() {
    let data = "a".repeat(Sds::INLINE_CAP + 1);
    let s = Sds::from_str(&data);
    let v: Vec<u8> = s.into();
    assert_eq!(v.as_slice(), data.as_bytes());
}

#[test]
fn try_from_valid_utf8() {
    let s = Sds::from_str("hello");
    let st: String = s.try_into().unwrap();
    assert_eq!(st, "hello");
}

#[test]
fn try_from_invalid_utf8_errors() {
    let s = Sds::from_vec(vec![0xFF, 0xFE]);
    let result: Result<String, _> = s.try_into();
    assert!(result.is_err());
}

#[test]
fn roundtrip_inline_push_then_truncate() {
    let mut s = Sds::from_str("abc");
    s.push(b'd');
    s.push(b'e');
    s.truncate(3);
    assert_eq!(s.as_slice(), b"abc");
    assert!(s.is_inline());
    s.debug_assert_invariants();
}

#[test]
fn roundtrip_heap_clear_then_refill() {
    let mut s = Sds::from_str(&heap_min());
    s.clear();
    s.append(b"new content");
    assert_eq!(s.as_slice(), b"new content");
    s.debug_assert_invariants();
}

#[test]
fn repeated_inline_to_heap_to_inline_transitions() {
    let mut s = Sds::default();
    for _ in 0..5 {
        // Расти до heap.
        for _ in 0..Sds::INLINE_CAP + 1 {
            s.push(b'x');
        }
        assert!(!s.is_inline());
        s.debug_assert_invariants();

        // Усекаем до inline.
        s.truncate(1);
        assert!(s.is_inline());
        s.debug_assert_invariants();

        // Очищаем.
        s.clear();
        assert_eq!(s.len(), 0);
    }
}

#[test]
fn hash_equals_for_same_content_different_repr() {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    // Строка равна INLINE_CAP — inline.
    let inline = Sds::from_str(&inline_max());
    // Та же строка, попавшая через heap, потом усечённая до inline.
    let mut heap_then_inline = Sds::from_str(&heap_min());
    heap_then_inline.truncate(Sds::INLINE_CAP);
    assert!(inline.is_inline());
    assert!(heap_then_inline.is_inline());
    assert_eq!(inline, heap_then_inline);

    let hash = |s: &Sds| {
        let mut h = DefaultHasher::new();
        s.hash(&mut h);
        h.finish()
    };
    assert_eq!(hash(&inline), hash(&heap_then_inline));
}

#[test]
fn ordering_consistent_with_byte_ordering() {
    let a = Sds::from_str("apple");
    let b = Sds::from_str("banana");
    let c = Sds::from_str("cherry");
    assert!(a < b);
    assert!(b < c);
    assert!(a < c);
}
