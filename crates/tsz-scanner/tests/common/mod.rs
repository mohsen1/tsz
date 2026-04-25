//! Shared scanner test fixtures.
//!
//! The three scanner integration test files (`scanner_impl_tests.rs`,
//! `regex_unicode_tests.rs`, `scanner_comprehensive_tests.rs`) each
//! repeated the same one-line constructor pattern dozens of times:
//!
//! ```ignore
//! let source = "...".to_string();
//! let mut scanner = ScannerState::new(source, true);
//! ```
//!
//! This module gives those tests a single fixture they can call with a
//! `&str` literal, so the constructor convention (`skip_trivia = true`)
//! stays single-source.
//!
//! Rationale: workstream 8 item 9 in `docs/plan/ROADMAP.md`
//! ("Create parser/scanner/binder/lowering fixtures").

use tsz_scanner::scanner_impl::ScannerState;

/// Construct a `ScannerState` from a `&str` source with the default
/// scanner options used across all current scanner tests
/// (`skip_trivia = true`).
///
/// Tests that need a different `skip_trivia` value should construct the
/// scanner inline — those cases are rare and worth surfacing explicitly.
#[allow(dead_code)] // Each integration-test binary uses a subset.
pub fn make_scanner(source: &str) -> ScannerState {
    ScannerState::new(source.to_string(), true)
}
