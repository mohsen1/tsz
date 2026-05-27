//! Regression tests for call expression resolution, overload resolution,
//! and property-call patterns.
//!
//! These exercise `call.rs` through the query boundary layer:
//! - Basic call expression type checking (TS2349, TS2554, TS2345)
//! - Overload resolution with multiple signatures
//! - Property/method call patterns (TS2339, TS2349)
//! - Optional chaining calls
//! - Spread arguments in calls (TS2556)
//! - Super calls and construct signatures
//! - Union callee types
//! - Generic call inference with overloads

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_with_options_code_messages;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    get_diagnostics_with_options(source, &CheckerOptions::default())
}

fn get_diagnostics_with_options(source: &str, options: &CheckerOptions) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, options.clone())
        .into_iter()
        .filter(|(code, _)| *code != 2318) // Filter "Cannot find global type"
        .collect()
}

fn get_codes(source: &str) -> Vec<u32> {
    get_diagnostics(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

fn has_error(source: &str, code: u32) -> bool {
    get_codes(source).contains(&code)
}

fn has_error_with_options(source: &str, options: &CheckerOptions, code: u32) -> bool {
    get_diagnostics_with_options(source, options)
        .into_iter()
        .any(|(diag_code, _)| diag_code == code)
}

fn no_errors(source: &str) -> bool {
    get_codes(source).is_empty()
}

fn no_errors_with_options(source: &str, options: &CheckerOptions) -> bool {
    get_diagnostics_with_options(source, options).is_empty()
}

// ============================================================================
// Basic call expression checks
// ============================================================================

include!("call_resolution_regression_tests_parts/part_00.rs");
include!("call_resolution_regression_tests_parts/part_01.rs");
