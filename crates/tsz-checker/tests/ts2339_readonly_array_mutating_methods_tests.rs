//! Tests for TS2339 on readonly array mutating method access.
//!
//! Structural rule: when a `ReadonlyType(Array(T))` or `ReadonlyType(Tuple(...))`
//! is the target of a property access, tsz must report TS2339 for any mutating
//! method (push, pop, sort, splice, reverse, fill, copyWithin, shift, unshift).
//! Read-only methods (filter, map, slice, forEach, etc.) must continue to succeed.
//!
//! This is a false-negative fix for issue #6897: tsz was stripping the
//! `ReadonlyType` wrapper before property lookup, allowing mutating methods
//! through silently.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::{check_source_code_messages, load_lib_files};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

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
        "esnext.d.ts",
    ])
}

fn compile_with_libs(source: &str) -> Vec<(u32, String)> {
    let file_name = "test.ts";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_lib_files_for_test();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions::default(),
    );

    let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_code(diags: &[(u32, String)], code: u32) -> bool {
    diags.iter().any(|(c, _)| *c == code)
}

fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

// =============================================================================
// Core repro: readonly number[] blocks mutating methods (no-lib fast check)
// =============================================================================

#[test]
fn test_readonly_number_array_push_errors() {
    let diags = check_source_code_messages(
        r"
declare const arr: readonly number[];
arr.push(1);
",
    );
    assert!(
        has_code(&diags, 2339),
        "push on readonly number[] must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_number_array_pop_errors() {
    let diags = check_source_code_messages(
        r"
declare const arr: readonly number[];
arr.pop();
",
    );
    assert!(
        has_code(&diags, 2339),
        "pop on readonly number[] must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_number_array_sort_errors() {
    let diags = check_source_code_messages(
        r"
declare const arr: readonly number[];
arr.sort();
",
    );
    assert!(
        has_code(&diags, 2339),
        "sort on readonly number[] must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_number_array_splice_errors() {
    let diags = check_source_code_messages(
        r"
declare const arr: readonly number[];
arr.splice(0, 1);
",
    );
    assert!(
        has_code(&diags, 2339),
        "splice on readonly number[] must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_number_array_reverse_errors() {
    let diags = check_source_code_messages(
        r"
declare const arr: readonly number[];
arr.reverse();
",
    );
    assert!(
        has_code(&diags, 2339),
        "reverse on readonly number[] must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_number_array_shift_errors() {
    let diags = check_source_code_messages(
        r"
declare const arr: readonly number[];
arr.shift();
",
    );
    assert!(
        has_code(&diags, 2339),
        "shift on readonly number[] must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_number_array_unshift_errors() {
    let diags = check_source_code_messages(
        r"
declare const arr: readonly number[];
arr.unshift(1);
",
    );
    assert!(
        has_code(&diags, 2339),
        "unshift on readonly number[] must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_number_array_fill_errors() {
    let diags = check_source_code_messages(
        r"
declare const arr: readonly number[];
arr.fill(0);
",
    );
    assert!(
        has_code(&diags, 2339),
        "fill on readonly number[] must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_number_array_copywithin_errors() {
    let diags = check_source_code_messages(
        r"
declare const arr: readonly number[];
arr.copyWithin(0, 1);
",
    );
    assert!(
        has_code(&diags, 2339),
        "copyWithin on readonly number[] must emit TS2339, got: {diags:?}"
    );
}

// =============================================================================
// Rule is not hardcoded to number[] — different element types must behave the same
// =============================================================================

#[test]
fn test_readonly_string_array_push_errors() {
    let diags = check_source_code_messages(
        r#"
declare const arr: readonly string[];
arr.push("hello");
"#,
    );
    assert!(
        has_code(&diags, 2339),
        "push on readonly string[] must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_boolean_array_pop_errors() {
    let diags = check_source_code_messages(
        r"
declare const arr: readonly boolean[];
arr.pop();
",
    );
    assert!(
        has_code(&diags, 2339),
        "pop on readonly boolean[] must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_object_array_sort_errors() {
    let diags = check_source_code_messages(
        r"
interface Item { value: number }
declare const arr: readonly Item[];
arr.sort();
",
    );
    assert!(
        has_code(&diags, 2339),
        "sort on readonly Item[] must emit TS2339, got: {diags:?}"
    );
}

// =============================================================================
// All 9 mutating methods blocked on a single array (batch check)
// =============================================================================

#[test]
fn test_readonly_array_all_mutating_methods_blocked() {
    let diags = check_source_code_messages(
        r"
declare const arr: readonly number[];
arr.push(1);
arr.pop();
arr.sort();
arr.splice(0, 1);
arr.reverse();
arr.shift();
arr.unshift(1);
arr.fill(0);
arr.copyWithin(0, 1);
",
    );
    let ts2339_count = count_code(&diags, 2339);
    assert_eq!(
        ts2339_count, 9,
        "all 9 mutating methods must emit TS2339, got {ts2339_count}: {diags:?}"
    );
}

// =============================================================================
// Mutable array still allows mutating methods (no false positives, with lib)
// =============================================================================

#[test]
fn test_mutable_array_push_no_error_with_lib() {
    let diags = compile_with_libs(
        r"
declare const arr: number[];
arr.push(1);
",
    );
    assert!(
        !has_code(&diags, 2339),
        "push on mutable number[] must NOT emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_mutable_array_all_mutating_methods_no_error_with_lib() {
    let diags = compile_with_libs(
        r"
declare const arr: string[];
arr.push('a');
arr.pop();
arr.sort();
arr.reverse();
arr.shift();
arr.unshift('b');
",
    );
    let ts2339_count = count_code(&diags, 2339);
    assert_eq!(
        ts2339_count, 0,
        "mutable string[] must NOT block any mutating methods, got {ts2339_count}: {diags:?}"
    );
}

// =============================================================================
// Readonly tuple blocks mutating methods
// =============================================================================

#[test]
fn test_readonly_tuple_push_errors() {
    let diags = check_source_code_messages(
        r"
declare const tup: readonly [number, string];
tup.push(1);
",
    );
    assert!(
        has_code(&diags, 2339),
        "push on readonly [number, string] must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_tuple_sort_errors() {
    let diags = check_source_code_messages(
        r"
declare const tup: readonly [boolean, number, string];
tup.sort();
",
    );
    assert!(
        has_code(&diags, 2339),
        "sort on readonly [boolean, number, string] must emit TS2339, got: {diags:?}"
    );
}

// =============================================================================
// With libs: ReadonlyArray<T> access works for non-mutating methods
// =============================================================================

#[test]
fn test_readonly_array_slice_no_error_with_lib() {
    let diags = compile_with_libs(
        r"
declare const arr: readonly number[];
const sliced = arr.slice(0, 2);
",
    );
    assert!(
        !has_code(&diags, 2339),
        "slice on readonly number[] must NOT emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_array_filter_no_error_with_lib() {
    let diags = compile_with_libs(
        r"
declare const arr: readonly number[];
const filtered = arr.filter(x => x > 0);
",
    );
    assert!(
        !has_code(&diags, 2339),
        "filter on readonly number[] must NOT emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_array_map_no_error_with_lib() {
    let diags = compile_with_libs(
        r"
declare const arr: readonly string[];
const mapped = arr.map(s => s.length);
",
    );
    assert!(
        !has_code(&diags, 2339),
        "map on readonly string[] must NOT emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_array_length_no_error_with_lib() {
    let diags = compile_with_libs(
        r"
declare const arr: readonly boolean[];
const n = arr.length;
",
    );
    assert!(
        !has_code(&diags, 2339),
        "length on readonly boolean[] must NOT emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_array_push_still_errors_with_lib() {
    let diags = compile_with_libs(
        r"
declare const arr: readonly number[];
arr.push(1);
",
    );
    assert!(
        has_code(&diags, 2339),
        "push on readonly number[] must emit TS2339 even with lib, got: {diags:?}"
    );
}

#[test]
fn test_readonly_array_pop_still_errors_with_lib() {
    let diags = compile_with_libs(
        r"
declare const arr: readonly string[];
arr.pop();
",
    );
    assert!(
        has_code(&diags, 2339),
        "pop on readonly string[] must emit TS2339 even with lib, got: {diags:?}"
    );
}

// =============================================================================
// ReadonlyArray<T> (explicit generic form) also blocks mutating methods with lib
// =============================================================================

#[test]
fn test_readonly_array_generic_push_errors_with_lib() {
    let diags = compile_with_libs(
        r"
declare const arr: ReadonlyArray<number>;
arr.push(1);
",
    );
    assert!(
        has_code(&diags, 2339),
        "push on ReadonlyArray<number> must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_array_generic_sort_errors_with_lib() {
    let diags = compile_with_libs(
        r"
declare const arr: ReadonlyArray<string>;
arr.sort();
",
    );
    assert!(
        has_code(&diags, 2339),
        "sort on ReadonlyArray<string> must emit TS2339, got: {diags:?}"
    );
}

#[test]
fn test_readonly_array_generic_map_no_error_with_lib() {
    let diags = compile_with_libs(
        r"
declare const arr: ReadonlyArray<number>;
const mapped = arr.map(x => x * 2);
",
    );
    assert!(
        !has_code(&diags, 2339),
        "map on ReadonlyArray<number> must NOT emit TS2339, got: {diags:?}"
    );
}

// =============================================================================
// Union containing readonly array: mutating methods blocked on the union
// =============================================================================

#[test]
fn test_union_readonly_array_push_errors() {
    let diags = check_source_code_messages(
        r"
declare const arr: readonly number[] | readonly string[];
arr.push(1 as any);
",
    );
    assert!(
        has_code(&diags, 2339),
        "push on (readonly number[] | readonly string[]) must emit TS2339, got: {diags:?}"
    );
}
