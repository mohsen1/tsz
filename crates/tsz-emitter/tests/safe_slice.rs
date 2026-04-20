use super::*;

#[test]
fn valid_non_empty_slice() {
    let s = "hello world";
    assert_eq!(slice(s, 0, 5), Ok("hello"));
    assert_eq!(slice(s, 6, 11), Ok("world"));
    assert_eq!(slice(s, 0, 11), Ok("hello world"));
}

#[test]
fn valid_empty_slice() {
    let s = "hello";
    assert_eq!(slice(s, 0, 0), Ok(""));
    assert_eq!(slice(s, 3, 3), Ok(""));
    assert_eq!(slice(s, 5, 5), Ok(""));
}

#[test]
fn start_out_of_bounds() {
    let s = "hello"; // len 5
    assert_eq!(
        slice(s, 6, 6),
        Err(SliceError::StartOutOfBounds { start: 6, len: 5 }),
    );
    assert_eq!(
        slice(s, 100, 200),
        Err(SliceError::StartOutOfBounds { start: 100, len: 5 }),
    );
}

#[test]
fn end_out_of_bounds() {
    let s = "hello"; // len 5
    assert_eq!(
        slice(s, 0, 6),
        Err(SliceError::EndOutOfBounds { end: 6, len: 5 }),
    );
    assert_eq!(
        slice(s, 2, 100),
        Err(SliceError::EndOutOfBounds { end: 100, len: 5 }),
    );
}

#[test]
fn reversed_range() {
    let s = "hello";
    assert_eq!(
        slice(s, 4, 2),
        Err(SliceError::ReversedRange { start: 4, end: 2 }),
    );
}

#[test]
fn invalid_utf8_boundary() {
    let s = "hello 🦀 world";
    // The crab emoji starts at byte 6 and occupies 4 bytes (6..10).
    // Byte 7 lands in the middle of the emoji — not a char boundary.
    assert_eq!(
        slice(s, 7, 10),
        Err(SliceError::InvalidUtf8Boundary { index: 7 }),
    );
    // Valid start, invalid end (mid-emoji).
    assert_eq!(
        slice(s, 6, 9),
        Err(SliceError::InvalidUtf8Boundary { index: 9 }),
    );
    // Valid boundaries around the emoji should still work.
    assert_eq!(slice(s, 0, 6), Ok("hello "));
    assert_eq!(slice(s, 6, 10), Ok("🦀"));
}

#[test]
fn error_order_start_before_end_before_reversed() {
    // When both start and end are out of bounds, StartOutOfBounds wins.
    let s = "ab"; // len 2
    assert_eq!(
        slice(s, 10, 5),
        Err(SliceError::StartOutOfBounds { start: 10, len: 2 }),
    );
    // Start in-bounds, end out-of-bounds: EndOutOfBounds wins.
    assert_eq!(
        slice(s, 1, 9),
        Err(SliceError::EndOutOfBounds { end: 9, len: 2 }),
    );
    // Both in-bounds but reversed: ReversedRange wins (not boundary error).
    let u = "🦀🦀"; // len 8, crab boundaries at 0/4/8
    assert_eq!(
        slice(u, 4, 1),
        Err(SliceError::ReversedRange { start: 4, end: 1 }),
    );
}

#[test]
fn slice_error_display_is_informative() {
    let s = "ab";
    let err = slice(s, 10, 5).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("10"), "msg={msg}");
    assert!(msg.contains("out of bounds"), "msg={msg}");
}

#[test]
fn explicit_empty_fallback_still_available_via_unwrap_or() {
    // The old `slice_or_empty` shim was removed. When a caller genuinely
    // wants empty-on-failure, the intent must be written at the call site
    // so it is visible to reviewers.
    let s = "hello";
    assert_eq!(slice(s, 0, 5).unwrap_or(""), "hello");
    assert_eq!(slice(s, 100, 200).unwrap_or(""), "");
    assert_eq!(slice(s, 5, 3).unwrap_or(""), "");
}
