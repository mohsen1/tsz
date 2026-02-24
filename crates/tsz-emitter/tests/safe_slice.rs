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
