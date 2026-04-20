//! Safe string slice utilities that never panic.
//!
//! The public API is fallible: callers must decide how to handle an invalid
//! slice request (out-of-bounds index, reversed range, or non-UTF-8 boundary).
//! Returning an empty string silently hides emitter span bugs, so the main
//! entry point surfaces a structured [`SliceError`] instead.
//!
//! A [`slice_or_empty`] compatibility shim is kept temporarily for call sites
//! where the historical "empty on invalid" behavior is still deliberate. It
//! should be removed once every call site has been audited.
//!
//! # Burn-down plan for [`slice_or_empty`]
//!
//! Migration is operational, not another API redesign:
//!
//! 1. **Classify every call site** as one of:
//!    - **A — real-empty intent**: comment text extraction; empty means
//!      "skip this comment". Bad span here is benign and the empty fallback
//!      is the correct behavior. Mark with a `// safe_slice: A` comment.
//!    - **B — gap inspection**: source text scanned for trivia (newlines,
//!      whitespace) where empty produces a sensible default. Mark `// safe_slice: B`.
//!    - **C — decision hiding**: empty silently flips a control-flow choice
//!      (e.g. `text == "this"` skip checks). These hide real bugs and must
//!      migrate to the fallible [`slice`] API. Should be at zero.
//! 2. **Track fallback frequency** via [`fallback_count`] (incremented on
//!    every silent swallow, in both debug and release).
//! 3. **Make the silent path loud in dev** via a `tracing::warn!` inside the
//!    shim under `debug_assertions`.
//! 4. **Mark `#[deprecated]`** once the remaining call sites are short
//!    (≤ 5 Category-A sites and zero Category-C sites). Deprecation is the
//!    enforcement mechanism — the compiler then maintains the burn-down list
//!    for us.

use std::sync::atomic::{AtomicU64, Ordering};
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

/// Total number of times [`slice_or_empty`] has silently swallowed a
/// [`SliceError`] this process. Used by tests and burn-down telemetry to
/// monitor whether the shim is still doing meaningful work.
static SHIM_FALLBACK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Number of times [`slice_or_empty`] has silently returned `""` because the
/// underlying [`slice`] call failed. Includes every emitter call site.
///
/// A passing test run should ideally show this as `0`. Any non-zero value
/// indicates either a real span bug (Category C) or expected best-effort
/// fallback (Category A/B); see the module-level burn-down plan.
pub fn fallback_count() -> u64 {
    SHIM_FALLBACK_COUNT.load(Ordering::Relaxed)
}

/// Reset the fallback counter. Test-only: used to scope assertions to a
/// single test rather than process-global state.
#[cfg(any(test, debug_assertions))]
pub fn reset_fallback_count() {
    SHIM_FALLBACK_COUNT.store(0, Ordering::Relaxed);
}

/// Compatibility shim that returns `""` when [`slice`] would fail.
///
/// This preserves the historical "silent empty on invalid" behavior for call
/// sites where an empty fallback is intentional (e.g., best-effort comment
/// text extraction where a bad span should simply skip the comment).
///
/// Every silent fallback bumps [`fallback_count`] so the burn-down can be
/// monitored, and emits a `tracing::warn!` under `debug_assertions` so the
/// silent path is loud during development and tests.
///
/// New code should call [`slice`] directly and handle [`SliceError`].
///
/// TODO(`safe_slice`): mark `#[deprecated]` once all Category-C call sites are
/// migrated and the remaining list is ≤ 5 Category-A sites. See the burn-down
/// plan in the module docs.
pub fn slice_or_empty(s: &str, start: usize, end: usize) -> &str {
    match slice(s, start, end) {
        Ok(v) => v,
        Err(_err) => {
            SHIM_FALLBACK_COUNT.fetch_add(1, Ordering::Relaxed);
            #[cfg(debug_assertions)]
            tracing::warn!(
                target: "tsz_emitter::safe_slice",
                "slice_or_empty silent fallback: {_err} (call site should be audited)",
            );
            ""
        }
    }
}

#[cfg(test)]
#[path = "../tests/safe_slice.rs"]
mod tests;
