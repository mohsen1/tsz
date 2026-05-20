//! Tests for generator function return type extraction from union and type alias types.
//!
//! When a generator function has a return type like `Generator<Y,R,N> | AsyncGenerator<Y,R,N>`,
//! or a type alias that expands to such a union, the checker must extract `TReturn` from each
//! union member to check `return expr` against `TReturn`, not the full Generator union type.
//! Without this, the checker would produce false positive TS2322 errors on return statements.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

const GENERATOR_STUBS: &str = r#"
interface Generator<T = unknown, TReturn = any, TNext = unknown> {
    next(value: TNext): IteratorResult<T, TReturn>;
    return(value: TReturn): IteratorResult<T, TReturn>;
    throw(e: any): IteratorResult<T, TReturn>;
    [Symbol.iterator](): Generator<T, TReturn, TNext>;
}
interface AsyncGenerator<T = unknown, TReturn = any, TNext = unknown> {
    next(value: TNext): Promise<IteratorResult<T, TReturn>>;
    return(value: TReturn): Promise<IteratorResult<T, TReturn>>;
    throw(e: any): Promise<IteratorResult<T, TReturn>>;
    [Symbol.asyncIterator](): AsyncGenerator<T, TReturn, TNext>;
}
interface IteratorResult<T, TReturn = any> {
    done?: boolean;
    value: T;
}
interface IterableIterator<T> {}
interface Promise<T> {}
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

fn check_with_no_implicit_returns(source: &str) -> Vec<(u32, String)> {
    let full_source = format!("{GENERATOR_STUBS}\n{source}");
    let options = CheckerOptions {
        strict_null_checks: true,
        no_implicit_any: true,
        no_implicit_returns: true,
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

fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags
        .iter()
        .filter(|(diag_code, _)| *diag_code == code)
        .count()
}

// =========================================================================
// Generator TReturn return-completeness diagnostics
// =========================================================================

#[test]
fn generator_treturn_drives_return_completeness_diagnostics() {
    let source = r#"
declare const cond: boolean;

function* declPartial(): Generator<number, number, unknown> {
    if (cond) {
        return 1;
    }
}

function* declEmpty(): Generator<number, number, unknown> {
}

const exprPartial = function* (): Generator<number, number, unknown> {
    if (cond) {
        return 1;
    }
};

async function* asyncPartial(): AsyncGenerator<number, number, unknown> {
    if (cond) {
        return 1;
    }
}

function* voidReturn(): Generator<number, void, unknown> {
    if (cond) {
        return;
    }
}
"#;
    let diags = check_with_no_implicit_returns(source);
    assert_eq!(
        count_code(&diags, 2355),
        1,
        "empty generator with non-void TReturn should emit TS2355, got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 2366),
        3,
        "partial declaration, expression, and async-generator returns should emit TS2366, got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 7030),
        0,
        "explicit generator TReturn diagnostics should not fall back to TS7030, got: {diags:?}"
    );
}

#[test]
fn generator_treturn_unknown_and_undefined_union_keep_ts2355_rules() {
    let source = r#"
declare const cond: boolean;

function* unknownEmpty(): Generator<number, unknown, unknown> {
}

function* unknownPartial(): Generator<number, unknown, unknown> {
    if (cond) {
        return 1;
    }
}

function* undefinedUnionEmpty(): Generator<number, number | undefined, unknown> {
}

function* undefinedUnionPartial(): Generator<number, number | undefined, unknown> {
    if (cond) {
        return 1;
    }
}
"#;
    let diags = check_with_no_implicit_returns(source);
    assert_eq!(
        count_code(&diags, 2355),
        2,
        "empty generator bodies with unknown or undefined-union TReturn should emit TS2355, got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 7030),
        2,
        "partial unknown/undefined-union TReturn generators should still emit TS7030, got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 2366),
        0,
        "unknown and undefined-union TReturn should not emit TS2366, got: {diags:?}"
    );
}

// =========================================================================
// Type alias expanding to union of Generator | AsyncGenerator
// =========================================================================

#[test]
fn generator_return_type_alias_union_no_false_positive() {
    // A type alias like StepResultGenerator<T> = Generator<...> | AsyncGenerator<...>
    // should not produce false positive TS2322 on return statements in a function*.
    let source = r#"
type MyGenResult<T> = Generator<number, T, undefined> | AsyncGenerator<number, T, undefined>;

function* f(): MyGenResult<string> {
    yield 1;
    return "hello";
}
"#;
    let diags = codes_with_strict(source);
    let ts2322_count = diags.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Should not emit TS2322 when return type matches TReturn from type alias union. Got: {diags:?}"
    );
}

#[test]
fn generator_yield_type_alias_union_no_false_positive() {
    // Yield expressions should also be checked against TYield from the alias union.
    let source = r#"
type MyGenResult<T> = Generator<T, void, undefined> | AsyncGenerator<T, void, undefined>;

function* f(): MyGenResult<number> {
    yield 1;
    yield 2;
}
"#;
    let diags = codes_with_strict(source);
    let ts2322_count = diags.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Should not emit TS2322 when yield type matches TYield from type alias union. Got: {diags:?}"
    );
}

#[test]
fn generator_return_type_direct_union_no_false_positive() {
    // Direct union return type (not through type alias) should also work.
    let source = r#"
function* f(): Generator<number, string, undefined> | AsyncGenerator<number, string, undefined> {
    yield 1;
    return "hello";
}
"#;
    let diags = codes_with_strict(source);
    let ts2322_count = diags.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Should not emit TS2322 for direct union generator return type. Got: {diags:?}"
    );
}

#[test]
fn generator_return_type_alias_union_error_on_mismatch() {
    // When the actual return type doesn't match TReturn, TS2322 SHOULD fire.
    let source = r#"
type MyGenResult<T> = Generator<number, T, undefined> | AsyncGenerator<number, T, undefined>;

function* f(): MyGenResult<string> {
    yield 1;
    return 42;
}
"#;
    let diags = codes_with_strict(source);
    let ts2322_count = diags.iter().filter(|&&c| c == 2322).count();
    assert!(
        ts2322_count > 0,
        "Should emit TS2322 when return value doesn't match TReturn. Got: {diags:?}"
    );
}

// =========================================================================
// TS2505: A generator cannot have a 'void' type annotation
// =========================================================================

#[test]
fn generator_void_return_type_emits_ts2505() {
    // TS2505 should fire when a generator function has 'void' as return type.
    let source = r#"
function* f(): void { }
"#;
    let diags = codes_with_strict(source);
    let ts2505_count = diags.iter().filter(|&&c| c == 2505).count();
    assert!(
        ts2505_count > 0,
        "Should emit TS2505 for generator with void return type. Got: {diags:?}"
    );
    // Should NOT also emit TS2322 (cascading error suppressed)
    let ts2322_count = diags.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Should not emit cascading TS2322 when TS2505 is emitted. Got: {diags:?}"
    );
}

// =========================================================================
// yield* contextual type must not leak into inferred generator yield type
// =========================================================================

#[test]
fn yield_star_does_not_widen_to_contextual_supertype() {
    // When a generator has contextual yield type Foo (from `Iterable<Foo>`) and
    // yield* delegates to [new Bar] where Bar extends Foo, the inferred generator
    // yield type must be Bar (the actual type), not Foo (the contextual supertype).
    // Previously, the array literal [new Bar] was contextually typed as Foo[] instead
    // of Bar[], which leaked Foo into the inferred generator yield type.
    let source = r#"
class Foo { x: number = 0 }
class Bar extends Foo { y: string = "" }
class Baz { z: number = 0 }

interface Iterable<T> {
    [Symbol.iterator](): Iterator<T>;
}
interface Iterator<T> {
    next(): IteratorResult<T, void>;
}

var g: () => Iterable<Foo> = function* () {
    yield;
    yield new Bar;
    yield new Baz;
    yield *[new Bar];
    yield *[new Baz];
}
"#;
    let diags = check_with_strict(source);
    // The error should mention Bar | Baz | undefined, NOT Foo | Bar | Baz | undefined.
    // If Foo is in the yield type, the contextual supertype leaked in.
    for (code, msg) in &diags {
        if *code == 2322 && msg.contains("Generator<") {
            assert!(
                !msg.contains("Foo | Bar"),
                "Generator yield type should not include contextual supertype Foo. Got: {msg}"
            );
            assert!(
                !msg.contains("Foo | Baz"),
                "Generator yield type should not include contextual supertype Foo. Got: {msg}"
            );
        }
    }
}
