//! Tests for TS2558 type argument count mismatch behavior.
//!
//! When a function is called with the wrong number of type arguments,
//! tsc emits TS2558 and does NOT proceed to check argument types against
//! the incorrectly-instantiated signature. This prevents spurious TS2345
//! (argument not assignable) errors.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
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
        "Expected TS2558 for wrong type argument count, got: {:?}",
        codes
    );
    assert!(
        !codes.contains(&2345),
        "Should not emit TS2345 when type arg count is wrong, got: {:?}",
        codes
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
        "Expected TS2558 for too many type arguments, got: {:?}",
        codes
    );
    assert!(
        !codes.contains(&2345),
        "Should not emit TS2345 when type arg count is wrong, got: {:?}",
        codes
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
        "Expected TS2322 for type mismatch with correct type arg count, got: {:?}",
        codes
    );
    // Should NOT emit TS2558
    assert!(
        !codes.contains(&2558),
        "Should not emit TS2558 when type arg count is correct, got: {:?}",
        codes
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
        "Expected TS2558 when providing too few type args (below min), got: {:?}",
        codes
    );
    assert!(
        !codes.contains(&2345),
        "Should not emit TS2345 when type arg count is wrong, got: {:?}",
        codes
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
        "Should not emit TS2558 when type arg count is within valid range, got: {:?}",
        codes
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
        "Expected TS2558 for type args on non-generic function, got: {:?}",
        codes
    );
    assert!(
        !codes.contains(&2345),
        "Should not emit TS2345 when type args given to non-generic function, got: {:?}",
        codes
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
        "Expected TS2558 for wrong type argument count, got: {:?}",
        codes
    );
    // Should not emit TS2554 (wrong argument count) because the type arg
    // count was already wrong — tsc doesn't check arguments in this case
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 when type arg count is wrong, got: {:?}",
        codes
    );
}
