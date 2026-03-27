//! Tests for yield* expression return type inference.
//!
//! The result type of `yield* expr` is the `TReturn` of the delegated iterator,
//! NOT the `TNext` of the containing generator (which is what regular `yield` returns).
//! For example, `const x = yield* gen` where `gen: Generator<Y, R, N>` gives `x: R`.
//!
//! These tests verify:
//! - yield* returns the delegated iterator's `TReturn`
//! - TS2322 fires when yield* return type doesn't match the variable annotation
//! - yield* with no captured return does not trigger spurious TS7057

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

/// Minimal stubs so Generator<T, `TReturn`, `TNext`> resolves in the test environment
/// (no lib files are loaded). The `[Symbol.iterator]()` protocol isn't needed here
/// because `get_generator_return_type_argument` uses direct Application arg extraction
/// via `is_generator_like_name`.
const GENERATOR_STUBS: &str = r#"
interface SymbolConstructor {
    readonly iterator: symbol;
}
declare var Symbol: SymbolConstructor;
interface ReadonlyArray<T> {
    readonly length: number;
    [n: number]: T;
}
interface Generator<T = unknown, TReturn = any, TNext = unknown> {
    next(value: TNext): IteratorResult<T, TReturn>;
    return(value: TReturn): IteratorResult<T, TReturn>;
    throw(e: any): IteratorResult<T, TReturn>;
}
interface Iterator<T = unknown, TReturn = any, TNext = unknown> {
    next(value: TNext): IteratorResult<T, TReturn>;
    return?(value: TReturn): IteratorResult<T, TReturn>;
    throw?(e: any): IteratorResult<T, TReturn>;
}
interface Iterable<T> {
    [Symbol.iterator](): Iterator<T>;
}
interface IteratorResult<T, TReturn = any> {
    done?: boolean;
    value: T;
}
interface IterableIterator<T> {}
"#;

fn check_with_strict(source: &str) -> Vec<(u32, String)> {
    let full_source = format!("{GENERATOR_STUBS}\n{source}");
    let options = CheckerOptions {
        strict_null_checks: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    check_source(&full_source, "test.ts", options)
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn codes_with_strict(source: &str) -> Vec<u32> {
    check_with_strict(source)
        .iter()
        .map(|(code, _)| *code)
        .collect()
}

// =========================================================================
// yield* should NOT trigger TS7057 (implicit any) at the yield* line itself
// =========================================================================

#[test]
fn yield_star_no_ts7057_when_delegated_generator_is_typed() {
    // yield* gen should not fire TS7057 — the result type is the delegated
    // generator's TReturn, which is known (symbol), not implicit any.
    let source = r#"
declare const gen: Generator<number, symbol, string>;
function* f(): Generator<number, boolean, string> {
    const x = yield* gen;
}
"#;
    let diags = codes_with_strict(source);
    let ts7057_count = diags.iter().filter(|&&c| c == 7057).count();
    assert_eq!(
        ts7057_count, 0,
        "yield* should not emit TS7057 when delegated generator has known return type"
    );
}

#[test]
fn yield_star_in_annotated_generator_returns_delegated_return_type() {
    // yield* gen should return symbol (TReturn of gen), so assigning to
    // `const y: number = x` where x = yield* gen should produce TS2322.
    let source = r#"
declare const gen: Generator<number, symbol, string>;
function* f(): Generator<number, boolean, string> {
    const x = yield* gen;
    const y: number = x;
}
"#;
    let diags = codes_with_strict(source);
    let ts2322_count = diags.iter().filter(|&&c| c == 2322).count();
    assert!(
        ts2322_count > 0,
        "assigning yield* result (symbol) to number should produce TS2322, got: {diags:?}"
    );
}

// =========================================================================
// yield* in unannotated generators
// =========================================================================

#[test]
fn yield_star_unannotated_generator_no_ts7057_at_yield_star() {
    // Even without a return type annotation on the containing generator,
    // yield* should infer the delegated generator's TReturn — no TS7057.
    let source = r#"
declare const gen: Generator<number, symbol, string>;
function* f() {
    const x = yield* gen;
}
"#;
    let diags = codes_with_strict(source);
    let ts7057_count = diags.iter().filter(|&&c| c == 7057).count();
    assert_eq!(
        ts7057_count, 0,
        "yield* in unannotated generator should not emit TS7057 when delegated type is known"
    );
}

// =========================================================================
// Regular yield (non-star) should still work correctly
// =========================================================================

#[test]
fn regular_yield_still_returns_tnext() {
    // Regular yield in annotated generator returns TNext (string).
    // `const x = yield 1` should give x: string when generator is
    // Generator<number, boolean, string>. Assigning x to number should error.
    let source = r#"
function* f(): Generator<number, boolean, string> {
    const x = yield 1;
    const y: number = x;
}
"#;
    let diags = codes_with_strict(source);
    let ts2322_count = diags.iter().filter(|&&c| c == 2322).count();
    assert!(
        ts2322_count > 0,
        "assigning yield result (string TNext) to number should produce TS2322, got: {diags:?}"
    );
}

#[test]
fn regular_yield_in_unannotated_generator_emits_ts7057() {
    // Without return type annotation + noImplicitAny, consumed yield should fire TS7057.
    let source = r#"
function* g() {
    const value = yield 1;
}
"#;
    let diags = codes_with_strict(source);
    let ts7057_count = diags.iter().filter(|&&c| c == 7057).count();
    assert_eq!(
        ts7057_count, 1,
        "regular yield in unannotated generator should emit TS7057"
    );
}

// =========================================================================
// yield* with no consumed result should not trigger issues
// =========================================================================

#[test]
fn yield_star_discarded_result_no_errors() {
    // yield* gen; in expression statement — result is not consumed, no errors expected.
    let source = r#"
declare const gen: Generator<number, symbol, string>;
function* f(): Generator<number, boolean, string> {
    yield* gen;
}
"#;
    let diags = codes_with_strict(source);
    // No TS7057, no TS2322 expected
    let problem_codes: Vec<_> = diags.iter().filter(|&&c| c == 7057 || c == 2322).collect();
    assert!(
        problem_codes.is_empty(),
        "discarded yield* should not produce TS7057 or TS2322, got: {diags:?}"
    );
}

#[test]
fn yield_star_generator_iife_contextually_types_nested_callback() {
    let source = r#"
function* g(): IterableIterator<(x: string) => number> {
    yield * function* () {
        yield x => x.length;
    }();
}
"#;
    let diags = codes_with_strict(source);
    let ts7006_count = diags.iter().filter(|&&c| c == 7006).count();
    assert_eq!(
        ts7006_count, 0,
        "yield* generator IIFE should contextually type nested callback params, got: {diags:?}"
    );
}

#[test]
fn yield_operand_generator_iife_contextually_types_nested_callback() {
    let source = r#"
function* g(): Iterator<Iterable<(x: string) => number>> {
    yield (function* () {
        yield (x) => x.length;
    })();
}
"#;
    let diags = codes_with_strict(source);
    let ts7006_count = diags.iter().filter(|&&c| c == 7006).count();
    assert_eq!(
        ts7006_count, 0,
        "yield operand generator IIFE should contextually type nested callback params, got: {diags:?}"
    );
}

#[test]
fn yield_star_generator_callback_mismatch_reports_outer_ts2345() {
    let source = r#"
declare const inner3: {
  <A>(value: A): {
    (): A;
    [Symbol.iterator](): {
      next(...args: ReadonlyArray<any>): IteratorResult<number, A>;
    };
  };
};

declare function outer3<A>(
  body: (value: A) => Generator<never, unknown, unknown>,
): void;

outer3(function* <T>(value: T) {
  yield* inner3(value);
});

outer3(function* <T>(value: T) {
  const x = inner3(value);
  yield* x;
});
"#;
    let diags = check_with_strict(source);
    let ts2345_count = diags.iter().filter(|(code, _)| *code == 2345).count();
    let ts2488_count = diags.iter().filter(|(code, _)| *code == 2488).count();
    assert_eq!(
        ts2345_count, 2,
        "outer generator callback should reject delegated number yield type against Generator<never, unknown, unknown>, got: {diags:?}"
    );
    assert_eq!(
        ts2488_count, 0,
        "iterable generator shape should not emit TS2488 in the valid inner3 cases, got: {diags:?}"
    );
}
