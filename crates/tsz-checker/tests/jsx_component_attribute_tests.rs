//! Tests for JSX component attribute type checking.
//!
//! Verifies that TS2322 (type mismatch) and TS2741 (missing required property)
//! are correctly emitted for JSX component attributes.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::CheckerState;
use tsz_checker::test_utils::load_compiled_lib_files;
use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::{Diagnostic, diagnostic_codes};
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Compile JSX source with inline JSX namespace and return diagnostics.
fn jsx_diagnostics(source: &str) -> Vec<(u32, String)> {
    jsx_diagnostics_with_mode(source, JsxMode::Preserve)
}

fn jsx_diagnostics_with_mode(source: &str, jsx_mode: JsxMode) -> Vec<(u32, String)> {
    jsx_diagnostics_with_options(
        source,
        CheckerOptions {
            jsx_mode,
            ..CheckerOptions::default()
        },
    )
}

fn jsx_diagnostics_with_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn jsx_full_diagnostics_with_mode(source: &str, jsx_mode: JsxMode) -> Vec<Diagnostic> {
    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let options = CheckerOptions {
        jsx_mode,
        ..CheckerOptions::default()
    };

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn has_code(diags: &[(u32, String)], code: u32) -> bool {
    diags.iter().any(|(c, _)| *c == code)
}

// =============================================================================
// Diagnostic-assertion helpers
//
// Most assertions in this file boil down to a handful of shapes over the
// diagnostic lists produced by `jsx_diagnostics` / `jsx_diagnostics_with_pos`:
//
//   * "code C is present" / "code C is absent"
//   * "code C is present with a message fragment F"
//   * "list of messages for code C" / "count of diagnostics for code C"
//
// The helpers below express those shapes once, so individual tests don't have
// to repeat the `iter().any(...) / iter().filter(...).map(...).collect()`
// boilerplate. They are intentionally tiny adapters — they do not change any
// assertion's meaning, only its spelling.
// =============================================================================

/// Returns `true` if any diagnostic with `code` carries a message containing
/// `fragment`. The companion of [`has_code`] when callers also want to match a
/// substring of the rendered message.
fn has_code_with_message(diags: &[(u32, String)], code: u32, fragment: &str) -> bool {
    diags
        .iter()
        .any(|(c, message)| *c == code && message.contains(fragment))
}

/// Returns the messages for every diagnostic with the given `code`.
fn messages_for_code(diags: &[(u32, String)], code: u32) -> Vec<&str> {
    diags
        .iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, m)| m.as_str())
        .collect()
}

/// Returns the number of diagnostics with the given `code`.
fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// Position-aware variant of [`has_code_with_message`] for diagnostics
/// carrying `(code, start, message)`.
fn has_code_with_message_pos(diags: &[(u32, u32, String)], code: u32, fragment: &str) -> bool {
    diags
        .iter()
        .any(|(c, _, message)| *c == code && message.contains(fragment))
}

/// Return diagnostics with position info (code, start, message).
fn jsx_diagnostics_with_pos(source: &str) -> Vec<(u32, u32, String)> {
    jsx_diagnostics_with_pos_mode(source, JsxMode::Preserve)
}

fn jsx_diagnostics_with_pos_mode(source: &str, jsx_mode: JsxMode) -> Vec<(u32, u32, String)> {
    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let options = CheckerOptions {
        jsx_mode,
        ..CheckerOptions::default()
    };

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect()
}

/// Inline JSX namespace preamble for tests (with `ElementAttributesProperty` { props: {} }).
/// This mimics react16.d.ts's structure where props are accessed via instance.props.
const JSX_PREAMBLE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
        span: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
"#;

// =============================================================================
// SFC attribute type checking
// =============================================================================

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of jsx_component_attribute_tests tests.
include!("jsx_component_attribute_tests_parts/part_00.rs");
include!("jsx_component_attribute_tests_parts/part_01.rs");
include!("jsx_component_attribute_tests_parts/part_02.rs");
include!("jsx_component_attribute_tests_parts/part_03.rs");
include!("jsx_component_attribute_tests_parts/part_04.rs");
