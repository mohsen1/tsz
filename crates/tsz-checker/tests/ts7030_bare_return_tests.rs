//! Tests for TS7030 ("Not all code paths return a value") emitted at a bare
//! `return;` statement under `noImplicitReturns`.
//!
//! `tsc` reports TS7030 from two independent sources: the fall-off-the-end of a
//! function body (already implemented in tsz), and — separately — at every bare
//! `return;` (a `return` with no expression) inside a function whose effective
//! return type requires a value. This second source is emitted per return
//! statement by `tsc`'s `checkReturnStatement`, so it fires regardless of
//! reachability and independently of the fall-off-the-end diagnostic (both can
//! fire in one function).
//!
//! The bare-return source is gated on `!strictNullChecks`: under
//! `strictNullChecks` a bare `return;` flows through the ordinary assignability
//! path instead (`undefined` is not assignable → TS2322), so no TS7030 there.
//!
//! Oracle: `typescript@7.x` (`tsc --strict false --noImplicitReturns`).
//!
//! Related issue: #17425.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source, diagnostic_count};

/// `// @strict: false` + `noImplicitReturns` — the configuration under which a
/// bare `return;` reports TS7030 (the strictNullChecks-off path).
fn no_implicit_returns_nonstrict() -> CheckerOptions {
    CheckerOptions {
        strict: false,
        strict_null_checks: false,
        no_implicit_any: false,
        no_implicit_returns: true,
        ..CheckerOptions::default()
    }
}

fn check(source: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    check_source(source, "test.ts", options)
}

/// Byte offsets of every literal `return;` (bare return) keyword in `source`.
fn bare_return_offsets(source: &str) -> Vec<u32> {
    source
        .match_indices("return;")
        .map(|(i, _)| i as u32)
        .collect()
}

/// Byte offsets each TS7030 diagnostic is anchored at.
fn ts7030_offsets(diags: &[Diagnostic]) -> Vec<u32> {
    let mut out: Vec<u32> = diags
        .iter()
        .filter(|d| d.code == 7030)
        .map(|d| d.start)
        .collect();
    out.sort_unstable();
    out
}

/// Assert TS7030 is anchored at exactly the set of bare `return;` keywords in
/// `source` (no more, no fewer) — the position-exact, indentation-independent
/// contract for functions with no fall-off-the-end diagnostic.
fn assert_ts7030_exactly_at_bare_returns(source: &str, diags: &[Diagnostic]) {
    let mut expected = bare_return_offsets(source);
    expected.sort_unstable();
    assert_eq!(
        ts7030_offsets(diags),
        expected,
        "TS7030 must anchor at exactly the bare `return;` keywords; diags: {:?}",
        diags.iter().map(|d| (d.code, d.start)).collect::<Vec<_>>()
    );
}

#[test]
fn ts7030_fires_for_reachable_bare_return() {
    let source = "function f(x: boolean): number {\n    if (x) {\n        return 1;\n    }\n    return;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

/// The key correctness point vs the report's suggested "reachability collector":
/// `tsc` fires TS7030 at a bare `return;` even when it is unreachable (here,
/// after an unconditional `return 1;`). A reachability-gated fix would miss it.
#[test]
fn ts7030_fires_for_unreachable_bare_return_after_return() {
    let source = "function g(x: boolean): number {\n    return 1;\n    return;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

/// Same: a bare `return;` after an unconditional `throw` (unreachable) still
/// reports.
#[test]
fn ts7030_fires_for_unreachable_bare_return_after_throw() {
    let source = "function k(x: boolean): number {\n    if (x) {\n        return 1;\n    }\n    throw new Error();\n    return;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

/// An unannotated function whose inferred return type is not void/undefined/any
/// (it has a value-returning path) still reports at its bare returns.
#[test]
fn ts7030_fires_for_unannotated_function_with_bare_return() {
    let source =
        "function m(x: number) {\n    if (x === 1) {\n        return 1;\n    }\n    return;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

/// Two sibling bare returns → two independent TS7030 diagnostics.
#[test]
fn ts7030_fires_once_per_bare_return() {
    let source = "function f(x: number): number {\n    if (x === 1) {\n        return;\n    } else if (x === 2) {\n        return;\n    } else {\n        return 3;\n    }\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(diagnostic_count(&diags, 7030), 2);
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

/// A bare `return;` inside a switch `default` clause is reached by the walk.
#[test]
fn ts7030_fires_for_bare_return_in_switch_default() {
    let source = "function sw(x: number): number {\n    switch (x) {\n        case 1: return 1;\n        default: return;\n    }\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

/// A bare `return;` inside a `try` block is reached by the walk.
#[test]
fn ts7030_fires_for_bare_return_in_try_block() {
    let source = "function t(x: boolean): number {\n    try {\n        if (x) return 1;\n        return;\n    } finally {\n    }\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

/// Fall-off-the-end and a bare return co-occur as two separate diagnostics.
/// The fall-off diagnostic anchors at the return-type annotation (not a bare
/// `return;`), so exactly one of the two TS7030s sits on the bare return.
#[test]
fn ts7030_fall_off_and_bare_return_both_fire() {
    let source = "function f(x: number): number {\n    if (x === 1) {\n        return;\n    } else if (x === 2) {\n        return 2;\n    }\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(
        diagnostic_count(&diags, 7030),
        2,
        "expected fall-off-the-end + bare-return TS7030; diags: {:?}",
        diags.iter().map(|d| (d.code, d.start)).collect::<Vec<_>>()
    );
    let bare = bare_return_offsets(source);
    let on_bare = diags
        .iter()
        .filter(|d| d.code == 7030 && bare.contains(&d.start))
        .count();
    assert_eq!(
        on_bare, 1,
        "exactly one TS7030 must anchor at the bare return"
    );
}

/// A method body reports the bare-return TS7030 like a free function.
#[test]
fn ts7030_fires_for_method_bare_return() {
    let source = "class C {\n    m(x: boolean): number {\n        if (x) {\n            return 1;\n        }\n        return;\n    }\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

/// A getter body reports the bare-return TS7030 like a free function.
#[test]
fn ts7030_fires_for_getter_bare_return() {
    let source = "class C {\n    get g(): number {\n        if (true) {\n            return 1;\n        }\n        return;\n    }\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    // A getter with a definitely-true guard still has a reachable bare return.
    assert_eq!(diagnostic_count(&diags, 7030), 1);
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

/// A nested function's bare return answers to its OWN return type, not the
/// enclosing one: the walk stops at the nested function boundary, and the
/// nested function is checked separately. Each function reports its own bare
/// return exactly once (no double-count of the inner one).
#[test]
fn ts7030_nested_function_bare_returns_are_isolated() {
    let source = "function n(x: boolean): number {\n    function inner(): string {\n        return;\n    }\n    if (x) return 1;\n    return;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    // Two bare returns total (inner's `string`, outer's `number`), each once.
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

/// Anti-hardcoding: renaming the function, parameters, and the value-returning
/// literal must not change the outcome — the rule is structural.
#[test]
fn ts7030_is_binder_name_independent() {
    let source = "function widget(flag: boolean): number {\n    if (flag) {\n        return 99;\n    }\n    return;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

// -------------------------------------------------------------------------
// Negative controls — TS7030 must NOT fire.
// -------------------------------------------------------------------------

/// A `void` return type exempts the bare return.
#[test]
fn ts7030_silent_for_void_return() {
    let source =
        "function v(x: boolean): void {\n    if (x) {\n        return;\n    }\n    return;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(diagnostic_count(&diags, 7030), 0);
}

/// An `any` return type exempts the bare return.
#[test]
fn ts7030_silent_for_any_return() {
    let source =
        "function a(x: boolean): any {\n    if (x) {\n        return 1;\n    }\n    return;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(diagnostic_count(&diags, 7030), 0);
}

/// A union containing `void` exempts the bare return.
#[test]
fn ts7030_silent_for_union_with_void_return() {
    let source = "function w(x: boolean): number | void {\n    if (x) {\n        return 1;\n    }\n    return;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(diagnostic_count(&diags, 7030), 0);
}

/// With `noImplicitReturns` off, no bare-return TS7030 at all.
#[test]
fn ts7030_silent_when_no_implicit_returns_off() {
    let source = "function f(x: boolean): number {\n    if (x) {\n        return 1;\n    }\n    return;\n}\n";
    let diags = check(
        source,
        CheckerOptions {
            strict: false,
            strict_null_checks: false,
            no_implicit_any: false,
            no_implicit_returns: false,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(diagnostic_count(&diags, 7030), 0);
}

/// Under `strictNullChecks`, a bare `return;` is a TS2322 (`undefined` is not
/// assignable), NOT a TS7030.
#[test]
fn ts7030_silent_under_strict_null_checks_where_ts2322_fires() {
    let source = "function s(x: boolean): number {\n    if (x) {\n        return 1;\n    }\n    return;\n}\n";
    let diags = check(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            no_implicit_returns: true,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(diagnostic_count(&diags, 7030), 0);
    assert_eq!(
        diagnostic_count(&diags, 2322),
        1,
        "a strict bare return is TS2322; diags: {:?}",
        diags.iter().map(|d| (d.code, d.start)).collect::<Vec<_>>()
    );
}

/// A constructor's bare `return;` never reports TS7030 (tsc excludes
/// constructors), even though the instance return type is not void.
#[test]
fn ts7030_silent_for_constructor_bare_return() {
    let source = "class C {\n    constructor(x: boolean) {\n        if (x) {\n            return;\n        }\n        return;\n    }\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(diagnostic_count(&diags, 7030), 0);
}

/// A setter's bare `return;` never reports TS7030 (its return type is void).
#[test]
fn ts7030_silent_for_setter_bare_return() {
    let source = "class S {\n    set v(x: number) {\n        if (x) {\n            return;\n        }\n        return;\n    }\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(diagnostic_count(&diags, 7030), 0);
}

/// A bare `return;` under an `unknown` return type is silent: `tsc`'s
/// per-return gate is `isUnwrappedReturnTypeVoidOrAny` (`TypeFlags.Void |
/// TypeFlags.AnyOrUnknown`), and `AnyOrUnknown` covers `unknown`. Here every
/// path returns, so there is no fall-off diagnostic either (#17444).
#[test]
fn ts7030_silent_for_unknown_return_bare() {
    let source = "function u(x: boolean): unknown {\n    if (x) {\n        return 1;\n    }\n    return;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(
        diagnostic_count(&diags, 7030),
        0,
        "a bare return under unknown must suppress TS7030; diags: {:?}",
        diags.iter().map(|d| (d.code, d.start)).collect::<Vec<_>>()
    );
}

/// The two-gate distinction: the *fall-off-the-end* gate is NOT
/// `isUnwrappedReturnTypeVoidOrAny` — it excludes only any/void/undefined, so a
/// non-generator `unknown` return that falls off the end still reports TS7030
/// (`tsc`'s `noImplicitReturnsExclusions.ts` f6/f13). This is the case an
/// over-broad "unknown always suppresses" rule wrongly silenced.
#[test]
fn ts7030_fires_for_unknown_return_fall_off() {
    // `return 1;` has an expression (not a bare return) and the else path
    // falls off, so this exercises only the fall-off-the-end source.
    let source = "function u(x: boolean): unknown {\n    if (x) {\n        return 1;\n    }\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(
        diagnostic_count(&diags, 7030),
        1,
        "a non-generator unknown return that falls off must report TS7030; diags: {:?}",
        diags.iter().map(|d| (d.code, d.start)).collect::<Vec<_>>()
    );
}

// -------------------------------------------------------------------------
// Generator negative controls (#17444) — a bare `return;` in a generator whose
// unwrapped `TReturn` is void/unknown must NOT report TS7030. `tsc` unwraps the
// generator return type to `TReturn` and applies the same
// `isUnwrappedReturnTypeVoidOrAny` gate; an inferred generator that cannot yield
// a concrete `TReturn` collapses to `unknown`, which suppresses like `void`.
//
// The default checker lib does not ship the `Generator`/`IterableIterator`
// interfaces, so these inject a minimal stub matching the pinned lib's shape
// (`TReturn = any`), mirroring generator_union_return_type_tests.rs.
// -------------------------------------------------------------------------

/// A minimal generator lib stub (`TReturn` defaulted, like the real lib).
const GENERATOR_STUB: &str = "interface IteratorResult<T, TReturn = any> { done?: boolean; value: T; }\ninterface Generator<T = unknown, TReturn = void, TNext = unknown> {\n    next(value: TNext): IteratorResult<T, TReturn>;\n    return(value: TReturn): IteratorResult<T, TReturn>;\n    throw(e: any): IteratorResult<T, TReturn>;\n    [Symbol.iterator](): Generator<T, TReturn, TNext>;\n}\ninterface AsyncGenerator<T = unknown, TReturn = void, TNext = unknown> {\n    next(value: TNext): Promise<IteratorResult<T, TReturn>>;\n    [Symbol.asyncIterator](): AsyncGenerator<T, TReturn, TNext>;\n}\ninterface IterableIterator<T> {}\ninterface Promise<T> {}\ninterface SymbolConstructor { readonly iterator: symbol; readonly asyncIterator: symbol; }\ndeclare var Symbol: SymbolConstructor;\n";

fn ts7030_count_with_generator_stub(body: &str) -> usize {
    let source = format!("{GENERATOR_STUB}\n{body}");
    let diags = check(&source, no_implicit_returns_nonstrict());
    diagnostic_count(&diags, 7030)
}

/// The `generatorNoImplicitReturns.ts` witness: an inferred generator with a
/// bare `return;` on one branch and a `yield` on another. `tsc` is clean.
#[test]
fn ts7030_silent_for_inferred_generator_bare_return() {
    let count = ts7030_count_with_generator_stub(
        "function* testGenerator() {\n    if (1 > 0.5) {\n        return;\n    }\n    yield 'hello';\n}\n",
    );
    assert_eq!(
        count, 0,
        "an inferred generator's bare return must not report TS7030"
    );
}

/// The `generatorReturnTypeInferenceNonStrict.ts` `g302` witness: `yield` then a
/// bare `return;`. Inferred `TReturn` is void; `tsc` is clean.
#[test]
fn ts7030_silent_for_inferred_generator_yield_then_bare_return() {
    let count =
        ts7030_count_with_generator_stub("function* g302() {\n    yield 1;\n    return;\n}\n");
    assert_eq!(
        count, 0,
        "yield-then-bare-return generator must not report TS7030"
    );
}

/// An `async function*` with a bare `return;` is exempt on the same rule.
#[test]
fn ts7030_silent_for_inferred_async_generator_bare_return() {
    let count = ts7030_count_with_generator_stub(
        "async function* ag() {\n    if (1 > 0.5) {\n        return;\n    }\n    yield 1;\n}\n",
    );
    assert_eq!(
        count, 0,
        "an inferred async generator's bare return must not report TS7030"
    );
}
