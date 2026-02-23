//! Safe string slice utilities that never panic.
//!
//! These functions handle edge cases like:
//! - Out-of-bounds indices
//! - Non-UTF8 boundary slicing

/// Safely slice a string, returning an empty string if bounds are invalid.
///
/// Unlike `&s[start..end]`, this never panics.
pub fn slice(s: &str, start: usize, end: usize) -> &str {
    if start >= s.len() || end > s.len() || start > end {
        return "";
    }

    // Check if indices are valid UTF-8 boundaries
    if !s.is_char_boundary(start) || !s.is_char_boundary(end) {
        return "";
    }

    &s[start..end]
}

/// Safely slice a string from a start position to the end.
pub fn slice_from(s: &str, start: usize) -> &str {
    if start >= s.len() {
        return "";
    }

    if !s.is_char_boundary(start) {
        return "";
    }

    &s[start..]
}

/// Safely slice a string from the beginning to an end position.
pub fn slice_to(s: &str, end: usize) -> &str {
    if end > s.len() {
        return s;
    }

    if !s.is_char_boundary(end) {
        return "";
    }

    &s[..end]
}

/// Get a character at a byte position, if valid.
pub fn char_at(s: &str, pos: usize) -> Option<char> {
    if pos >= s.len() {
        return None;
    }

    if !s.is_char_boundary(pos) {
        return None;
    }

    s[pos..].chars().next()
}

/// Get a byte at a position, returning None if out of bounds.
pub fn byte_at(s: &str, pos: usize) -> Option<u8> {
    s.as_bytes().get(pos).copied()
}

#[cfg(test)]
#[path = "../tests/safe_slice.rs"]
mod tests;
