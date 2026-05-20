//! Read-only summary of the source file's parse-health state.
//!
//! Bundles the dense cluster of parse-error booleans on `CheckerContext`
//! (`has_parse_errors`, `has_syntax_parse_errors`, `has_real_syntax_errors`,
//! `has_structural_parse_errors`) plus the four position vectors into a
//! single value passed through diagnostic-emission code paths.
//!
//! Robustness audit (PR #I, item 9 in
//! `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md`).

use super::CheckerContext;

/// Read-only summary of the source file's parse-health state.
///
/// Diagnostic-emission paths can take a `ParseHealth` instead of reading
/// individual mutable `CheckerContext` flags, reducing the risk of state
/// leakage and nested-call confusion.
#[derive(Debug, Clone, Copy)]
pub struct ParseHealth<'b> {
    pub has_parse_errors: bool,
    pub has_syntax_parse_errors: bool,
    pub has_real_syntax_errors: bool,
    pub has_structural_parse_errors: bool,
    pub syntax_parse_error_positions: &'b [u32],
    pub real_syntax_error_positions: &'b [u32],
    pub all_parse_error_positions: &'b [u32],
    pub nullable_type_parse_error_positions: &'b [u32],
}

impl<'b> ParseHealth<'b> {
    /// True when the file has any parse-related issue (broadest test). Use
    /// this in suppression paths that should also fire for grammar-only
    /// violations (e.g. TS2695 for malformed JSON).
    #[inline]
    #[must_use]
    pub const fn has_any_parse_issue(&self) -> bool {
        self.has_parse_errors
            || self.has_syntax_parse_errors
            || self.has_real_syntax_errors
            || self.has_structural_parse_errors
    }

    /// True when at least one position vector contains the given start
    /// position. Cheaper than letting callers walk all four vectors when
    /// they only need a "near a parse error" check.
    #[inline]
    #[must_use]
    pub fn any_position_contains(&self, pos: u32) -> bool {
        self.syntax_parse_error_positions.contains(&pos)
            || self.real_syntax_error_positions.contains(&pos)
            || self.all_parse_error_positions.contains(&pos)
            || self.nullable_type_parse_error_positions.contains(&pos)
    }
}

impl<'a> CheckerContext<'a> {
    /// Borrow the parse-health flags as a single read-only value.
    ///
    /// Intended replacement for direct field reads of `has_parse_errors`,
    /// `has_syntax_parse_errors`, etc. New diagnostic-emission paths
    /// should accept a `ParseHealth<'_>` argument instead of reaching
    /// into `&CheckerContext` for the individual flags. Existing fields
    /// stay public until callers migrate.
    ///
    /// Robustness audit (PR #I, item 9 in
    /// `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md`).
    #[inline]
    #[must_use]
    pub fn parse_health(&self) -> ParseHealth<'_> {
        ParseHealth {
            has_parse_errors: self.has_parse_errors,
            has_syntax_parse_errors: self.has_syntax_parse_errors,
            has_real_syntax_errors: self.has_real_syntax_errors,
            has_structural_parse_errors: self.has_structural_parse_errors,
            syntax_parse_error_positions: &self.syntax_parse_error_positions,
            real_syntax_error_positions: &self.real_syntax_error_positions,
            all_parse_error_positions: &self.all_parse_error_positions,
            nullable_type_parse_error_positions: &self.nullable_type_parse_error_positions,
        }
    }
}
