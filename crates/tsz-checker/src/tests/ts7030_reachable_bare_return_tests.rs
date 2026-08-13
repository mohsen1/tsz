//! Regression coverage for #17425: `tsc` reports TS7030 (`noImplicitReturns`)
//! not only for fall-off-the-end, but also for each bare `return;` inside a
//! function whose effective return type requires a value — anchored at the
//! `return` keyword itself, and co-occurring with the existing
//! fall-off-the-end diagnostic rather than replacing it.
//!
//! `statement_falls_through`/`block_falls_through` (`crates/tsz-checker/src/
//! flow/reachability_checker.rs`) already treat any `return` — bare or not —
//! as an unconditional terminator, which is correct for "does control reach
//! the end of the block" but made a bare return invisible to both TS7030
//! call sites (`function_type_helpers.rs`, `function_declaration_checks.rs`).
//!
//! The bare-return half of TS7030 turned out **not** to be gated by flow
//! reachability at all — the issue's own suggested design was oracle-checked
//! against `if (false) { return; }`, a bare return dead after an
//! unconditional `return`/`throw`, and one after `while (true) { return 1;
//! }`; `tsc` 6.0.2 reports TS7030 in every one of those dead-code cases. So
//! `collect_bare_returns` is a plain structural descent through every
//! statement kind that can contain a nested statement, with no
//! `statement_falls_through`/`is_true_condition` pruning, stopping only at
//! nested function-like boundaries (their own independent return-type
//! context).

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn ts7030_codes(source: &str) -> Vec<u32> {
    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_returns: true,
            ..CheckerOptions::default()
        },
    );
    diagnostics.iter().map(|diag| diag.code).collect()
}

fn ts7030_spans(source: &str) -> Vec<(u32, u32)> {
    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_returns: true,
            ..CheckerOptions::default()
        },
    );
    diagnostics
        .iter()
        .filter(|diag| diag.code == 7030)
        .map(|diag| (diag.start, diag.length))
        .collect()
}

/// Sole-statement bare return, no other control flow — the
/// `isMissingReturnExpression` shape from `noImplicitReturnsWithoutReturnExpression.ts`.
#[test]
fn sole_bare_return_reports_ts7030() {
    let codes = ts7030_codes(
        r#"
function f(): number {
    return;
}
"#,
    );
    assert!(
        codes.contains(&7030),
        "a lone bare `return;` in a value-returning function must report TS7030, got {codes:?}"
    );
}

/// tsc anchors TS7030-for-bare-return at the `return` keyword itself: a
/// fixed 6-column span, not the full `return;` statement (which includes the
/// semicolon in tsz's parser) and not the type annotation used by the
/// fall-off-the-end variant of TS7030.
#[test]
fn bare_return_ts7030_anchors_at_return_keyword() {
    let source = "function f(): number {\n    return;\n}\n";
    let spans = ts7030_spans(source);
    assert_eq!(spans.len(), 1, "expected exactly one TS7030, got {spans:?}");
    let (start, length) = spans[0];
    let return_kw_pos = source.find("return").unwrap() as u32;
    assert_eq!(
        (start, length),
        (return_kw_pos, 6),
        "TS7030 for a bare return must anchor at the `return` keyword (6 columns), got start={start} length={length}"
    );
}

/// Bare return in one `if`/`else` arm, value return in the other — no
/// fall-off-the-end, so only the bare-return TS7030 fires.
#[test]
fn bare_return_in_if_else_arm_reports_ts7030_without_fall_off() {
    let codes = ts7030_codes(
        r#"
function f(x: boolean): number {
    if (x) {
        return 1;
    }
    return;
}
"#,
    );
    assert!(
        codes.contains(&7030),
        "the trailing bare `return;` is the function's last statement (no fall-off-the-end) \
         but still reachable, so TS7030 must fire, got {codes:?}"
    );
}

/// Two bare returns in sibling `else if` arms report two independent
/// diagnostics, each anchored at its own `return`.
#[test]
fn two_sibling_bare_returns_report_two_ts7030_diagnostics() {
    let source = r#"
function f(x: number): number {
    if (x === 1) {
        return;
    } else if (x === 2) {
        return;
    } else {
        return 3;
    }
}
"#;
    let spans = ts7030_spans(source);
    assert_eq!(
        spans.len(),
        2,
        "each independently-reachable bare return gets its own TS7030, got {spans:?}"
    );
}

/// Bare return in one arm plus fall-off-the-end elsewhere: both diagnostics
/// fire, at different nodes. `strictNullChecks` is off so the fall-off site
/// reports TS7030 rather than TS2366 (tsz's existing, unrelated strict-mode
/// diagnostic-selection rule for the fall-off case).
#[test]
fn bare_return_and_fall_off_the_end_both_report_ts7030() {
    let diagnostics = check_source(
        r#"
function f(x: number): number {
    if (x === 1) {
        return;
    } else if (x === 2) {
        return 2;
    }
}
"#,
        "test.ts",
        CheckerOptions {
            no_implicit_returns: true,
            strict_null_checks: false,
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let count = codes.iter().filter(|&&code| code == 7030).count();
    assert_eq!(
        count, 2,
        "expected one TS7030 for the bare return and one for falling off the end, got {codes:?}"
    );
}

/// Negative: return type `void` never triggers TS7030, bare return or not.
#[test]
fn bare_return_with_void_return_type_is_not_reported() {
    let codes = ts7030_codes(
        r#"
function f(x: boolean): void {
    if (x) {
        return;
    }
}
"#,
    );
    assert!(
        !codes.contains(&7030),
        "a `void`-returning function must never report TS7030, got {codes:?}"
    );
}

/// Negative: return type `any` never triggers TS7030.
#[test]
fn bare_return_with_any_return_type_is_not_reported() {
    let codes = ts7030_codes(
        r#"
function f(x: boolean): any {
    if (x) {
        return;
    }
}
"#,
    );
    assert!(
        !codes.contains(&7030),
        "an `any`-returning function must never report TS7030, got {codes:?}"
    );
}

/// Negative: `noImplicitReturns` off skips the check entirely.
#[test]
fn bare_return_without_no_implicit_returns_flag_is_not_reported() {
    let diagnostics = check_source(
        r#"
function f(x: boolean): number {
    if (x) {
        return;
    }
    return 1;
}
"#,
        "test.ts",
        CheckerOptions::default(),
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&7030),
        "TS7030 must not fire when `noImplicitReturns` is off, got {codes:?}"
    );
}

/// The bare-return half of TS7030 is **not** gated by flow reachability —
/// oracle-verified (`typescript` 6.0.2, `--noImplicitReturns`): `tsc` still
/// reports TS7030 at a bare return that is dead code after an unconditional
/// `return`. (Confirmed directly against the oracle before writing this
/// assertion; a flow-reachability-gated design would wrongly stay silent
/// here.)
#[test]
fn dead_bare_return_after_return_still_reports_ts7030() {
    let codes = ts7030_codes(
        r#"
function f(): number {
    return 1;
    return;
}
"#,
    );
    assert!(
        codes.contains(&7030),
        "tsc reports TS7030 at a bare return even when it is dead code after \
         an earlier `return`, got {codes:?}"
    );
}

/// Same as above for dead code after an unconditional `throw`.
#[test]
fn dead_bare_return_after_throw_still_reports_ts7030() {
    let codes = ts7030_codes(
        r#"
function f(): number {
    throw new Error("x");
    return;
}
"#,
    );
    assert!(
        codes.contains(&7030),
        "tsc reports TS7030 at a bare return even when it is dead code after \
         an unconditional `throw`, got {codes:?}"
    );
}

/// Same as above for a bare return inside a compile-time-`false` branch —
/// oracle-verified: `tsc` does not special-case a literal-`false` `if` for
/// this diagnostic either.
#[test]
fn dead_bare_return_inside_if_false_still_reports_ts7030() {
    let codes = ts7030_codes(
        r#"
function f(): number {
    if (false) {
        return;
    }
    return 1;
}
"#,
    );
    assert!(
        codes.contains(&7030),
        "tsc reports TS7030 at a bare return inside `if (false)`, got {codes:?}"
    );
}

/// Negative: a bare return inside a nested function declaration does not
/// leak into the outer function's TS7030 check — the inner function has its
/// own independent return-type context (here `void`, exempt from TS7030).
#[test]
fn bare_return_in_nested_function_does_not_affect_outer() {
    let codes = ts7030_codes(
        r#"
function outer(): number {
    function inner(): void {
        return;
    }
    inner();
    return 1;
}
"#,
    );
    assert!(
        !codes.contains(&7030),
        "a bare return inside a nested function must not be attributed to the \
         enclosing function, got {codes:?}"
    );
}

/// Renamed-binder / concrete adjacent case (Anti-Hardcoding Gate): same
/// shape as the sibling-arms test with different identifiers throughout,
/// plus a `switch` instead of `if`/`else if` to cover the switch-clause
/// collection arm.
#[test]
fn bare_return_inside_switch_clause_reports_ts7030() {
    let codes = ts7030_codes(
        r#"
function classify(status: number): string {
    switch (status) {
        case 1:
            return "one";
        case 2:
            return;
        default:
            return "other";
    }
}
"#,
    );
    assert!(
        codes.contains(&7030),
        "a bare `return;` inside a reachable switch clause must report TS7030, got {codes:?}"
    );
}

/// Bare return inside a `for-of` loop body is reachable (the loop may
/// execute at least once) even though the loop itself always "falls
/// through" for fall-off purposes.
#[test]
fn bare_return_inside_for_of_body_reports_ts7030() {
    let codes = ts7030_codes(
        r#"
function firstPositive(values: number[]): number {
    for (const v of values) {
        if (v > 0) {
            return;
        }
    }
    return 0;
}
"#,
    );
    assert!(
        codes.contains(&7030),
        "a bare return reachable inside a for-of body must report TS7030, got {codes:?}"
    );
}

/// Bare return inside a function expression assigned to a variable —
/// exercises the `check_function_return_completeness` call site rather than
/// the function-declaration one.
#[test]
fn bare_return_in_function_expression_reports_ts7030() {
    let codes = ts7030_codes(
        r#"
const f = function (x: boolean): number {
    if (x) {
        return;
    }
    return 1;
};
"#,
    );
    assert!(
        codes.contains(&7030),
        "a bare return inside a function expression must report TS7030, got {codes:?}"
    );
}

/// Bare return inside an arrow function with a block body.
#[test]
fn bare_return_in_arrow_function_reports_ts7030() {
    let codes = ts7030_codes(
        r#"
const f = (x: boolean): number => {
    if (x) {
        return;
    }
    return 1;
};
"#,
    );
    assert!(
        codes.contains(&7030),
        "a bare return inside an arrow function must report TS7030, got {codes:?}"
    );
}
