//! Tests for TS2322 assignability errors
//!
//! These tests verify that TS2322 "Type 'X' is not assignable to type 'Y'" errors
//! are properly emitted in various contexts.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::{Diagnostic, diagnostic_codes};
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::{
    HasDiagnosticCode, check_source_with_libs, diagnostic_codes as project_diagnostic_codes,
    load_lib_files,
};
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    load_lib_files(&[
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.promise.d.ts",
        "es2015.proxy.d.ts",
        "es2015.reflect.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "es2019.array.d.ts",
        "dom.d.ts",
        "dom.generated.d.ts",
        "dom.iterable.d.ts",
        "esnext.iterator.d.ts",
        "esnext.d.ts",
    ])
}

fn with_lib_contexts(source: &str, file_name: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let is_js_file = matches!(
        file_name,
        s if s.ends_with(".js")
            || s.ends_with(".jsx")
            || s.ends_with(".mjs")
            || s.ends_with(".cjs")
    );
    let lib_files = if is_js_file {
        load_lib_files_for_test()
    } else {
        Vec::new()
    };

    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn with_lib_contexts_and_positions(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
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
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect()
}

/// Helper function to check if a diagnostic with a specific code was emitted
fn has_error_with_code(source: &str, code: u32) -> bool {
    with_lib_contexts(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .any(|(d, _)| d == code)
}

/// Helper to count errors with a specific code
fn count_errors_with_code(source: &str, code: u32) -> usize {
    with_lib_contexts(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .filter(|(d, _)| *d == code)
        .count()
}

/// Helper that returns all diagnostics for inspection
fn get_all_diagnostics(source: &str) -> Vec<(u32, String)> {
    with_lib_contexts(source, "test.ts", CheckerOptions::default())
}

fn ts2322_messages(source: &str) -> Vec<String> {
    get_all_diagnostics(source)
        .into_iter()
        .filter_map(|(code, message)| (code == 2322).then_some(message))
        .collect()
}

fn ts2820_messages(source: &str) -> Vec<String> {
    get_all_diagnostics(source)
        .into_iter()
        .filter_map(|(code, message)| (code == 2820).then_some(message))
        .collect()
}

fn diagnostic_count<T: HasDiagnosticCode>(diagnostics: &[T], code: u32) -> usize {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.diagnostic_code() == code)
        .count()
}

fn diagnostics_with_code<T: HasDiagnosticCode>(diagnostics: &[T], code: u32) -> Vec<&T> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.diagnostic_code() == code)
        .collect()
}

fn has_diagnostic_code<T: HasDiagnosticCode>(diagnostics: &[T], code: u32) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.diagnostic_code() == code)
}

fn assert_no_missing_property_diagnostics(diagnostics: &[Diagnostic]) {
    let missing_property_codes = [
        diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
        diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
        diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
    ];
    let actual: Vec<u32> = diagnostics
        .iter()
        .map(|d| d.code)
        .chain(
            diagnostics
                .iter()
                .flat_map(|d| d.related_information.iter().map(|related| related.code)),
        )
        .filter(|code| missing_property_codes.contains(code))
        .collect();

    assert!(
        actual.is_empty(),
        "Expected no missing-property diagnostics, got codes {actual:?}. Diagnostics: {diagnostics:#?}"
    );
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of ts2322_tests tests.
include!("ts2322_tests_parts/part_00.rs");
include!("ts2322_tests_parts/part_01.rs");
include!("ts2322_tests_parts/part_02.rs");
include!("ts2322_tests_parts/part_03.rs");
include!("ts2322_tests_parts/part_04.rs");
