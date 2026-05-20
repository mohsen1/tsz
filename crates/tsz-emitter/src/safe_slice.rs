//! Safe string slice utilities that never panic.
//!
//! The single entry point is the fallible [`slice`], which returns a
//! structured [`SliceError`] when the range is invalid. Callers must decide
//! how to handle an invalid slice request — there is no silent empty-string
//! fallback in the public API.
//!
//! Historical context: an earlier version exposed `slice_or_empty` that
//! swallowed every invalid request into `""`. That hid emitter span bugs, so
//! it was removed once every call site was audited and migrated. If a caller
//! genuinely needs the old semantics, the intent must be visible at the call
//! site:
//!
//! ```ignore
//! // Make the empty fallback explicit at the call site.
//! let text = safe_slice::slice(src, start, end).unwrap_or("");
//! ```

use tracing::debug;

/// Error returned by [`slice`] when the requested range is not valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SliceError {
    /// `start` points past the end of the string.
    StartOutOfBounds { start: usize, len: usize },
    /// `end` points past the end of the string.
    EndOutOfBounds { end: usize, len: usize },
    /// `start > end` — the range is reversed.
    ReversedRange { start: usize, end: usize },
    /// `start` or `end` does not fall on a UTF-8 character boundary.
    InvalidUtf8Boundary { index: usize },
}

impl core::fmt::Display for SliceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SliceError::StartOutOfBounds { start, len } => {
                write!(f, "slice start {start} is out of bounds (len {len})")
            }
            SliceError::EndOutOfBounds { end, len } => {
                write!(f, "slice end {end} is out of bounds (len {len})")
            }
            SliceError::ReversedRange { start, end } => {
                write!(f, "slice range is reversed (start {start} > end {end})")
            }
            SliceError::InvalidUtf8Boundary { index } => {
                write!(f, "slice index {index} is not on a UTF-8 boundary")
            }
        }
    }
}

impl std::error::Error for SliceError {}

/// Fallibly borrow `&s[start..end]` without panicking.
///
/// Returns the most specific [`SliceError`] possible when the range is not
/// valid. Checks happen in a fixed order: start bound → end bound → reversed
/// range → UTF-8 boundary, so the error always describes the first problem.
///
/// Invalid requests emit a `tracing::debug!` so bad span math is visible in
/// development builds without crashing release.
pub fn slice(s: &str, start: usize, end: usize) -> Result<&str, SliceError> {
    let len = s.len();

    if start > len {
        let err = SliceError::StartOutOfBounds { start, len };
        debug!(target: "tsz_emitter::safe_slice", "{err}");
        return Err(err);
    }
    if end > len {
        let err = SliceError::EndOutOfBounds { end, len };
        debug!(target: "tsz_emitter::safe_slice", "{err}");
        return Err(err);
    }
    if start > end {
        let err = SliceError::ReversedRange { start, end };
        debug!(target: "tsz_emitter::safe_slice", "{err}");
        return Err(err);
    }
    if !s.is_char_boundary(start) {
        let err = SliceError::InvalidUtf8Boundary { index: start };
        debug!(target: "tsz_emitter::safe_slice", "{err}");
        return Err(err);
    }
    if !s.is_char_boundary(end) {
        let err = SliceError::InvalidUtf8Boundary { index: end };
        debug!(target: "tsz_emitter::safe_slice", "{err}");
        return Err(err);
    }

    Ok(&s[start..end])
}

#[cfg(test)]
#[path = "../tests/safe_slice.rs"]
mod tests;
