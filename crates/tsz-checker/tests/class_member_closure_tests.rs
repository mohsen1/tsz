//! Regression tests for the `ClassMemberClosure` / `OwnMemberSummary` boundary.
//!
//! These tests verify that class/member summary extraction and routing
//! through the boundary correctly handles:
//!   - Strict property initialization (TS2564)
//!   - Parameter properties
//!   - Override visibility/type checks (TS4112-TS4115, TS2416)
//!   - Base/member closure consistency

use crate::context::CheckerOptions;
use crate::test_utils::check_with_options;
use tsz_common::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    check_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            strict_property_initialization: true,
            ..CheckerOptions::default()
        },
    )
}

fn check_with_no_implicit_override(source: &str) -> Vec<Diagnostic> {
    check_with_options(
        source,
        CheckerOptions {
            no_implicit_override: true,
            ..CheckerOptions::default()
        },
    )
}

fn check_default(source: &str) -> Vec<Diagnostic> {
    check_with_options(source, CheckerOptions::default())
}
