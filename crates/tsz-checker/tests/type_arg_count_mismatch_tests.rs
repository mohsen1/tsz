//! Tests for TS2558 type argument count mismatch behavior.
//!
//! When a function is called with the wrong number of type arguments,
//! tsc emits TS2558 and does NOT proceed to check argument types against
//! the incorrectly-instantiated signature. This prevents spurious TS2345
//! (argument not assignable) errors.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostic_codes(source: &str) -> Vec<u32> {
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

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

/// When calling a generic function with too few type arguments,
/// only TS2558 should be emitted — no spurious TS2345.
/// Reproduces the conformance failure for mismatchedExplicitTypeParameterAndArgumentType.ts.
#[test]
fn test_too_few_type_args_no_spurious_ts2345() {
    let source = r#"
function map<T, U>(xs: T[], f: (x: T) => U) {
    var ys: U[] = [];
    xs.forEach(x => ys.push(f(x)));
    return ys;
}

var r7b = map<number>([1, ""], (x) => x.toString());
"#;
    let codes = diagnostic_codes(source);

    assert!(
        codes.contains(&2558),
        "Expected TS2558 for wrong type argument count, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "Should not emit TS2345 when type arg count is wrong, got: {codes:?}"
    );
}

/// When calling a generic function with too many type arguments,
/// only TS2558 should be emitted — no spurious argument errors.
#[test]
fn test_too_many_type_args_no_spurious_ts2345() {
    let source = r#"
function identity<T>(x: T): T { return x; }
var r = identity<number, string>(42);
"#;
    let codes = diagnostic_codes(source);

    assert!(
        codes.contains(&2558),
        "Expected TS2558 for too many type arguments, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "Should not emit TS2345 when type arg count is wrong, got: {codes:?}"
    );
}

/// When calling with correct type arg count but wrong argument types,
/// TS2322 should still be emitted (the fix must not suppress valid errors).
#[test]
fn test_correct_type_arg_count_still_emits_ts2322() {
    let source = r#"
function map<T, U>(xs: T[], f: (x: T) => U) {
    var ys: U[] = [];
    xs.forEach(x => ys.push(f(x)));
    return ys;
}

var r7 = map<number, string>([1, ""], (x) => x.toString());
"#;
    let codes = diagnostic_codes(source);

    // Should emit TS2322 for the string element not assignable to number
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for type mismatch with correct type arg count, got: {codes:?}"
    );
    // Should NOT emit TS2558
    assert!(
        !codes.contains(&2558),
        "Should not emit TS2558 when type arg count is correct, got: {codes:?}"
    );
}

/// Type arguments with defaults: providing fewer than required should emit TS2558.
#[test]
fn test_type_args_with_defaults_too_few() {
    let source = r#"
function create<T, U, V = string>(x: T): T { return x; }
var r = create<number>(1);
"#;
    let codes = diagnostic_codes(source);

    // U has no default, so providing 1 of 2-3 is wrong
    assert!(
        codes.contains(&2558),
        "Expected TS2558 when providing too few type args (below min), got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "Should not emit TS2345 when type arg count is wrong, got: {codes:?}"
    );
}

/// Providing within the valid range (with defaults) should NOT emit TS2558.
#[test]
fn test_type_args_within_default_range_no_ts2558() {
    let source = r#"
function create<T, U = string>(x: T, y: U): [T, U] { return [x, y]; }
var r1 = create<number>(1, "hello");
"#;
    let codes = diagnostic_codes(source);

    assert!(
        !codes.contains(&2558),
        "Should not emit TS2558 when type arg count is within valid range, got: {codes:?}"
    );
}

/// Calling a non-generic function with type arguments should emit TS2558
/// but NOT emit spurious argument-related errors.
#[test]
fn test_non_generic_with_type_args_no_spurious_errors() {
    let source = r#"
function add(a: number, b: number): number { return a + b; }
var r = add<number>(1, 2);
"#;
    let codes = diagnostic_codes(source);

    assert!(
        codes.contains(&2558),
        "Expected TS2558 for type args on non-generic function, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "Should not emit TS2345 when type args given to non-generic function, got: {codes:?}"
    );
}

#[test]
fn non_generic_explicit_type_args_suppress_argument_mismatch_but_plain_calls_report() {
    let source = r#"
function foo<T, U>(f: (v: T) => U) {
   var r1 = f<number>(1);
   var r2 = f(1);
   var r3 = f<any>(null);
   var r4 = f(null);
}
"#;
    let diagnostics = check_diagnostics(source);
    let mut ts2558: Vec<_> = diagnostics.iter().filter(|d| d.code == 2558).collect();
    let mut ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    ts2558.sort_by_key(|d| d.start);
    ts2345.sort_by_key(|d| d.start);

    let explicit_number = source.find("number").expect("number type argument") as u32;
    let explicit_any = source.find("any").expect("any type argument") as u32;
    let plain_number_arg = (source.find("f(1)").expect("plain number call") + 2) as u32;
    let plain_null_arg = (source.find("f(null)").expect("plain null call") + 2) as u32;

    assert_eq!(
        ts2558.len(),
        2,
        "Expected two TS2558 diagnostics, got: {diagnostics:?}"
    );
    assert_eq!(
        ts2558[0].start, explicit_number,
        "TS2558 from f<number>(1) should anchor at the explicit type argument"
    );
    assert_eq!(
        ts2558[1].start, explicit_any,
        "TS2558 from f<any>(null) should anchor at the explicit type argument"
    );
    assert_eq!(
        ts2345.len(),
        2,
        "Expected two TS2345 diagnostics, got: {diagnostics:?}"
    );
    assert_eq!(
        ts2345[0].start, plain_number_arg,
        "TS2345 should come from the plain f(1) call, not f<number>(1)"
    );
    assert_eq!(
        ts2345[1].start, plain_null_arg,
        "TS2345 should come from the plain f(null) call, not f<any>(null)"
    );
}

/// When type arg count is wrong but argument count is also wrong,
/// both TS2558 and TS2554 should be suppressed (only TS2558 for the
/// type arg count). TSC does not emit argument count errors in this case.
#[test]
fn test_type_arg_mismatch_suppresses_argument_count_errors() {
    let source = r#"
function pair<T, U>(a: T, b: U): [T, U] { return [a, b]; }
var r = pair<number>(1);
"#;
    let codes = diagnostic_codes(source);

    assert!(
        codes.contains(&2558),
        "Expected TS2558 for wrong type argument count, got: {codes:?}"
    );
    // Should not emit TS2554 (wrong argument count) because the type arg
    // count was already wrong — tsc doesn't check arguments in this case
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 when type arg count is wrong, got: {codes:?}"
    );
}

// =============================================================================
// Type-argument walking on unresolved heritage and qualified-name type refs
// =============================================================================
//
// Regression tests for `parserGenericsInTypeContexts1.ts` conformance failure:
// tsc visits the `<T>` type arguments of a type reference / heritage clause
// even when the base name is unresolved, so identifiers inside the type args
// surface their own diagnostics (e.g., TS2304 "Cannot find name 'T'").
// Previously tsz silently dropped those type args along several paths:
//   - unresolved heritage expression: `class C extends A<T> implements B<T>`
//   - unresolved qualified-name type ref: `var v3: E.F<T>`
//   - value-only qualified-name type ref
//
// The fix walks every type argument via `get_type_from_type_node` before
// returning the error type, matching the simple-identifier path that already
// handled `var v2: D<T>` correctly.

fn check_diagnostics(source: &str) -> Vec<Diagnostic> {
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
    checker.ctx.diagnostics.clone()
}

fn count_ts2304_for_name(diags: &[Diagnostic], name: &str) -> usize {
    let needle = format!("'{name}'");
    diags
        .iter()
        .filter(|d| d.code == 2304 && d.message_text.contains(&needle))
        .count()
}

/// `class C extends A<T> {}` — when `A` doesn't resolve, tsc still reports
/// TS2304 for `T` inside the type arguments.
#[test]
fn unresolved_heritage_extends_walks_type_arguments() {
    let diags = check_diagnostics("class C extends A<T> {}\n");

    assert_eq!(
        count_ts2304_for_name(&diags, "A"),
        1,
        "expected TS2304 for unresolved base name 'A', got: {diags:?}",
    );
    assert_eq!(
        count_ts2304_for_name(&diags, "T"),
        1,
        "expected TS2304 for unresolved type argument 'T' inside heritage extends clause, got: {diags:?}",
    );
}

/// `class C implements B<T> {}` — `B` unresolved still reports `T`.
#[test]
fn unresolved_heritage_implements_walks_type_arguments() {
    let diags = check_diagnostics("class C implements B<T> {}\n");

    assert_eq!(
        count_ts2304_for_name(&diags, "B"),
        1,
        "expected TS2304 for unresolved base name 'B', got: {diags:?}",
    );
    assert_eq!(
        count_ts2304_for_name(&diags, "T"),
        1,
        "expected TS2304 for unresolved type argument 'T' inside heritage implements clause, got: {diags:?}",
    );
}

/// Combined heritage: both extends and implements have unresolved type args.
/// From `parserGenericsInTypeContexts1.ts`.
#[test]
fn unresolved_heritage_extends_and_implements_walk_type_arguments() {
    let diags = check_diagnostics("class C extends A<T> implements B<T> {}\n");

    assert_eq!(
        count_ts2304_for_name(&diags, "A"),
        1,
        "missing TS2304 for 'A'"
    );
    assert_eq!(
        count_ts2304_for_name(&diags, "B"),
        1,
        "missing TS2304 for 'B'"
    );
    assert_eq!(
        count_ts2304_for_name(&diags, "T"),
        2,
        "expected two TS2304 diagnostics for 'T' (one per heritage clause), got: {diags:?}",
    );
}

/// `var v: E.F<T>` — when namespace `E` doesn't resolve, tsc emits TS2503 for
/// `E` AND TS2304 for `T`. Previously tsz only emitted TS2503 and dropped `T`.
#[test]
fn unresolved_qualified_name_type_reference_walks_type_arguments() {
    let diags = check_diagnostics("var v: E.F<T>;\n");

    let ts2503_count = diags
        .iter()
        .filter(|d| d.code == 2503 && d.message_text.contains("'E'"))
        .count();
    assert_eq!(
        ts2503_count, 1,
        "expected TS2503 for unresolved namespace 'E', got: {diags:?}",
    );
    assert_eq!(
        count_ts2304_for_name(&diags, "T"),
        1,
        "expected TS2304 for unresolved type argument 'T' inside qualified-name type ref, got: {diags:?}",
    );
}

/// Deeper qualified names are also covered (e.g., `G.H.I<T>`).
#[test]
fn unresolved_deeply_qualified_name_type_reference_walks_type_arguments() {
    let diags = check_diagnostics("var v: G.H.I<T>;\n");

    assert_eq!(
        count_ts2304_for_name(&diags, "T"),
        1,
        "expected TS2304 for 'T' inside deeply qualified unresolved type ref, got: {diags:?}",
    );
}

/// Sanity: the simple-identifier path (`var v: D<T>`) already worked before
/// this fix. This test locks it in so the newly-added paths stay consistent
/// with the established behaviour.
#[test]
fn unresolved_simple_type_reference_walks_type_arguments() {
    let diags = check_diagnostics("var v: D<T>;\n");

    assert_eq!(
        count_ts2304_for_name(&diags, "D"),
        1,
        "expected TS2304 for unresolved simple type name 'D', got: {diags:?}",
    );
    assert_eq!(
        count_ts2304_for_name(&diags, "T"),
        1,
        "expected TS2304 for 'T' inside simple unresolved type ref, got: {diags:?}",
    );
}
