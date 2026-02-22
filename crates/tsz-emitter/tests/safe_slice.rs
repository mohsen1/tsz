use super::*;

#[test]
fn test_safe_slice_basic() {
    let s = "hello world";
    assert_eq!(slice(s, 0, 5), "hello");
    assert_eq!(slice(s, 6, 11), "world");
}

#[test]
fn test_safe_slice_empty() {
    let s = "hello";
    assert_eq!(slice(s, 10, 20), "");
    assert_eq!(slice(s, 5, 3), "");
}

#[test]
fn test_safe_slice_unicode() {
    let s = "hello 🦀 world";
    // The crab emoji is 4 bytes
    let crab_start = 6;
    let crab_end = 10;

    // Safe slice should work with valid boundaries
    assert_eq!(slice(s, 0, crab_start), "hello ");
    assert_eq!(slice(s, crab_end + 1, s.len()), "world");

    // Invalid boundary should return empty
    assert_eq!(slice(s, 7, 9), ""); // Mid-emoji
}

#[test]
fn test_safe_slice_from_to() {
    let s = "hello";
    assert_eq!(slice_from(s, 2), "llo");
    assert_eq!(slice_to(s, 3), "hel");
    assert_eq!(slice_from(s, 10), "");
}

#[test]
fn test_char_at() {
    let s = "hello 🦀";
    assert_eq!(char_at(s, 0), Some('h'));
    assert_eq!(char_at(s, 6), Some('🦀'));
    assert_eq!(char_at(s, 100), None);
}

#[test]
fn test_byte_at() {
    let s = "hello";
    assert_eq!(byte_at(s, 0), Some(b'h'));
    assert_eq!(byte_at(s, 4), Some(b'o'));
    assert_eq!(byte_at(s, 10), None);
}
