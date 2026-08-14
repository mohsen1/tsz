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

// -------------------------------------------------------------------------
// Generators — regression coverage for #17444 (a spurious TS7030 the
// bare-return collector introduced on generator bodies, net -1 conformance).
//
// A generator's bare `return;` only requires a value when a Generator-shaped
// annotation actually supplies a non-void/any/undefined completion (`R`)
// type — the same rule `check_function_return_paths` already applies to the
// fall-off-the-end source. Without that extraction (unannotated, or an
// annotation that isn't Generator-shaped), the effective completion type
// falls back to `UNKNOWN`, which is never void/any/undefined on its own, so
// whether a value is actually required instead follows whether the body has
// some other value-returning `return <expr>;` — exactly like a non-generator
// function's ordinary "has a return statement" gate.
//
// Oracle: `tsc` 6.0.2 (`--strict false --noImplicitReturns --target esnext`).
// -------------------------------------------------------------------------

/// An unannotated generator whose only return is a bare `return;` (no value
/// anywhere in the body) never reports TS7030 — its completion type can't be
/// extracted, and there is no value-returning return to require one.
#[test]
fn ts7030_silent_for_unannotated_generator_bare_return_only() {
    let source = "function* g(x: boolean) {\n    if (x) {\n        yield 1;\n        return;\n    }\n    yield 2;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(diagnostic_count(&diags, 7030), 0, "diags: {diags:?}");
}

/// A generator annotated `Generator<Y, void, N>` is exempt from the
/// bare-return TS7030 (the ordinary void-return skip), independent of the
/// new generator completion-type gate.
#[test]
fn ts7030_silent_for_generator_void_return_type_bare_return() {
    let source = "function* g(x: boolean): Generator<number, void, unknown> {\n    if (x) {\n        yield 1;\n        return;\n    }\n    yield 2;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(diagnostic_count(&diags, 7030), 0, "diags: {diags:?}");
}

/// An *annotated* generator whose declared completion type genuinely
/// requires a value (`Generator<Y, number, N>`) still reports TS7030 at a
/// bare `return;`, even though a sibling branch returns a value.
#[test]
fn ts7030_fires_for_annotated_generator_bare_return_with_value_return() {
    let source = "function* g(x: boolean): Generator<number, number, unknown> {\n    if (x) {\n        yield 1;\n        return;\n    }\n    yield 2;\n    return 6;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(diagnostic_count(&diags, 7030), 1, "diags: {diags:?}");
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

/// An unannotated generator that DOES have a value-returning `return <expr>;`
/// somewhere still reports the fall-off-the-end TS7030 (unaffected by the
/// bare-return gate — this fixture has no bare return at all).
#[test]
fn ts7030_fires_for_unannotated_generator_fall_off_with_value_return() {
    let source = "function* g(x: boolean) {\n    if (x) {\n        yield 1;\n        return 5;\n    }\n    yield 2;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(diagnostic_count(&diags, 7030), 1, "diags: {diags:?}");
}

/// An unannotated generator with both a bare `return;` and a sibling
/// value-returning `return <expr>;` reports TS7030 at the bare return — the
/// presence of a value-returning return elsewhere is what makes a value
/// required, not the (unextractable) completion type alone.
#[test]
fn ts7030_fires_for_unannotated_generator_bare_return_with_value_return() {
    let source = "function* g(x: boolean) {\n    if (x) {\n        yield 1;\n        return;\n    }\n    yield 2;\n    return 6;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(diagnostic_count(&diags, 7030), 1, "diags: {diags:?}");
    assert_ts7030_exactly_at_bare_returns(source, &diags);
}

/// Anti-hardcoding: renaming the generator and its parameter must not change
/// the outcome for the silent unannotated case.
#[test]
fn ts7030_generator_gate_is_binder_name_independent() {
    let source = "function* widget(flag: boolean) {\n    if (flag) {\n        yield 99;\n        return;\n    }\n    yield 1;\n}\n";
    let diags = check(source, no_implicit_returns_nonstrict());
    assert_eq!(diagnostic_count(&diags, 7030), 0, "diags: {diags:?}");
}
