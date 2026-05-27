//! Focused tests for generic call inference and contextual instantiation.
//!
//! These exercise the `call_inference.rs` module:
//! - Round-2 contextual typing for callback parameters
//! - Return-context substitution collection
//! - Generic function argument refinement against targets
//! - Widening/literal-preservation in type parameter substitutions
//! - Binding-pattern sanitization during inference
//! - Contextual constraint with self-referential type parameters
//! - Application shape preservation through union/intersection
//! - Anyish inference detection across composite types
//! - Return context substitution through tuples, arrays, and generics

use tsz_checker::context::CheckerOptions;

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_compiled_lib_files};

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn compile_and_get_raw_diagnostics(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
}

fn load_es5_lib_files_for_test() -> Vec<Arc<LibFile>> {
    load_compiled_lib_files(&["lib.es5.d.ts"])
}

fn load_es2015_lib_files_for_test() -> Vec<Arc<LibFile>> {
    load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.es2015.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.generator.d.ts",
        "lib.es2015.promise.d.ts",
        "lib.es2015.proxy.d.ts",
        "lib.es2015.reflect.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
    ])
}

fn compile_with_es5_lib_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let lib_files = load_es5_lib_files_for_test();
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &lib_files)
}

fn compile_with_es2015_lib_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let lib_files = load_es2015_lib_files_for_test();
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &lib_files)
}

fn relevant_lib_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_with_es5_lib_and_get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect()
}

fn compile_strict_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn compile_js_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn relevant_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318) // Filter out "Cannot find global type"
        .collect()
}

fn relevant_js_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_js_and_get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318) // Filter out "Cannot find global type"
        .collect()
}

fn relevant_strict_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_strict_and_get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318) // Filter out "Cannot find global type"
        .collect()
}

fn relevant_default_lib_diagnostics(source: &str) -> Vec<(u32, String)> {
    let lib_files = tsz_checker::test_utils::load_default_lib_files();
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &lib_files)
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect()
}

fn relevant_strict_default_lib_diagnostics(source: &str) -> Vec<(u32, String)> {
    let lib_files = tsz_checker::test_utils::load_default_lib_files();
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
    .into_iter()
    .filter(|(code, _)| *code != 2318)
    .collect()
}

// -----------------------------------------------------------------------------
// Diagnostic assertion helpers
//
// These collapse the repeated `.iter().filter(...).count()` /
// `.iter().any(...)` / `.iter().find(...)` patterns that show up across the
// many generic call inference tests. They are intentionally tiny and
// behavior-preserving: each call expands to the same closure-over-`code` /
// `(code, message)` pattern the tests already use.
// -----------------------------------------------------------------------------

/// Number of diagnostics with the given TS code.
fn diagnostic_count(diagnostics: &[(u32, String)], code: u32) -> usize {
    diagnostics
        .iter()
        .filter(|(actual, _)| *actual == code)
        .count()
}

/// Borrowed messages for diagnostics with the given TS code, in order.
fn diagnostics_with_code(diagnostics: &[(u32, String)], code: u32) -> Vec<&str> {
    diagnostics
        .iter()
        .filter_map(|(actual, message)| (*actual == code).then_some(message.as_str()))
        .collect()
}

/// True if at least one diagnostic has the given TS code.
fn has_diagnostic_code(diagnostics: &[(u32, String)], code: u32) -> bool {
    diagnostics.iter().any(|(actual, _)| *actual == code)
}

/// True if no diagnostic has the given TS code.
fn lacks_diagnostic_code(diagnostics: &[(u32, String)], code: u32) -> bool {
    !has_diagnostic_code(diagnostics, code)
}

/// True if at least one diagnostic's code is in the given list.
fn has_any_diagnostic_code(diagnostics: &[(u32, String)], codes: &[u32]) -> bool {
    diagnostics.iter().any(|(actual, _)| codes.contains(actual))
}

/// True if no diagnostic's code is in the given list.
fn lacks_any_diagnostic_code(diagnostics: &[(u32, String)], codes: &[u32]) -> bool {
    !has_any_diagnostic_code(diagnostics, codes)
}

/// True if at least one diagnostic with the given code has a message
/// containing the given fragment.
fn has_diagnostic_message_containing(
    diagnostics: &[(u32, String)],
    code: u32,
    fragment: &str,
) -> bool {
    diagnostics_with_code(diagnostics, code)
        .iter()
        .any(|message| message.contains(fragment))
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of generic_call_inference_tests tests.
include!("generic_call_inference_tests_parts/part_00.rs");
include!("generic_call_inference_tests_parts/part_01.rs");
include!("generic_call_inference_tests_parts/part_02.rs");
