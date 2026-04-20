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

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .filter(|d| d.code != 2318) // Filter "Cannot find global type"
        .map(|d| (d.code, d.message_text))
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

fn no_errors(source: &str) -> bool {
    get_codes(source).is_empty()
}

// ============================================================================
// Basic call expression checks
// ============================================================================
include!("call_resolution_regression_tests_parts/part_00.rs");
include!("call_resolution_regression_tests_parts/part_01.rs");
include!("call_resolution_regression_tests_parts/part_02.rs");
include!("call_resolution_regression_tests_parts/part_03.rs");
include!("call_resolution_regression_tests_parts/part_04.rs");
include!("call_resolution_regression_tests_parts/part_05.rs");
include!("call_resolution_regression_tests_parts/part_06.rs");
include!("call_resolution_regression_tests_parts/part_07.rs");
include!("call_resolution_regression_tests_parts/part_08.rs");
include!("call_resolution_regression_tests_parts/part_09.rs");
include!("call_resolution_regression_tests_parts/part_10.rs");
include!("call_resolution_regression_tests_parts/part_11.rs");
include!("call_resolution_regression_tests_parts/part_12.rs");
