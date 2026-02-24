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

#[cfg(test)]
#[path = "../tests/safe_slice.rs"]
mod tests;
