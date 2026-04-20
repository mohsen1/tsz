//! Tests for JSX component attribute type checking.
//!
//! Verifies that TS2322 (type mismatch) and TS2741 (missing required property)
//! are correctly emitted for JSX component attributes.

use std::path::Path;
use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::CheckerState;
use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::{Diagnostic, diagnostic_codes};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Compile JSX source with inline JSX namespace and return diagnostics.
fn jsx_diagnostics(source: &str) -> Vec<(u32, String)> {
    jsx_diagnostics_with_mode(source, JsxMode::Preserve)
}

fn jsx_diagnostics_with_mode(source: &str, jsx_mode: JsxMode) -> Vec<(u32, String)> {
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
include!("jsx_component_attribute_tests_parts/part_00.rs");
include!("jsx_component_attribute_tests_parts/part_01.rs");
include!("jsx_component_attribute_tests_parts/part_02.rs");
include!("jsx_component_attribute_tests_parts/part_03.rs");
include!("jsx_component_attribute_tests_parts/part_04.rs");
include!("jsx_component_attribute_tests_parts/part_05.rs");
include!("jsx_component_attribute_tests_parts/part_06.rs");
include!("jsx_component_attribute_tests_parts/part_07.rs");
include!("jsx_component_attribute_tests_parts/part_08.rs");
include!("jsx_component_attribute_tests_parts/part_09.rs");
include!("jsx_component_attribute_tests_parts/part_10.rs");
include!("jsx_component_attribute_tests_parts/part_11.rs");
include!("jsx_component_attribute_tests_parts/part_12.rs");
include!("jsx_component_attribute_tests_parts/part_13.rs");
include!("jsx_component_attribute_tests_parts/part_14.rs");
