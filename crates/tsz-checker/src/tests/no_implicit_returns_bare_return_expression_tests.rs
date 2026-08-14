//! TS7030 from a reachable (or unreachable) bare `return;`.
//!
//! Under `noImplicitReturns`, tsc reports TS7030 from two independent sources
//! (`checker.ts`): the fall-off-the-end check anchored at the return-type
//! annotation, and — separately, in `checkReturnStatement` — a diagnostic
//! anchored at each bare `return;` (no expression) inside a function whose
//! unwrapped return type requires a value. The second source only applies
//! without `strictNullChecks` (with it, a bare `return;` is checked as
//! `undefined` against the return type by ordinary assignability). tsc does not
//! gate the second source on reachability: it runs for every return statement,
//! including one after a `return`/`throw` or inside an `if (false)` branch.
//!
//! Every expectation below is verified against `tsc` 6.0.2
//! (`--strict false --noImplicitReturns`, and `--strict true` where noted).
//! Binder names are varied so no expectation depends on an identifier.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn codes(source: &str, no_implicit_returns: bool, strict_null_checks: bool) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_returns,
            strict_null_checks,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|diag| diag.code)
    .collect()
}

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn sole_bare_return_reports_ts7030() {
    // The function returns on every path (no fall-off-the-end), yet the bare
    // `return;` alone still reports.
    let out = codes("function alpha(): number { return; }", true, false);
    assert_eq!(count(&out, 7030), 1, "expected one TS7030, got {out:?}");
    assert_eq!(
        count(&out, 2355),
        0,
        "no fall-off-the-end TS2355 expected: {out:?}"
    );
}

#[test]
fn bare_return_in_one_arm_reports_ts7030() {
    let out = codes(
        r#"
function beta(flag: boolean): number {
    if (flag) {
        return 1;
    }
    return;
}
"#,
        true,
        false,
    );
    assert_eq!(count(&out, 7030), 1, "expected one TS7030, got {out:?}");
}

#[test]
fn two_sibling_bare_returns_report_two_ts7030() {
    let out = codes(
        r#"
function gamma(n: number): number {
    if (n === 1) {
        return;
    } else if (n === 2) {
        return;
    } else {
        return 3;
    }
}
"#,
        true,
        false,
    );
    assert_eq!(count(&out, 7030), 2, "expected two TS7030, got {out:?}");
}

#[test]
fn bare_return_and_fall_off_end_report_both() {
    // A bare `return;` in one arm (return-anchored TS7030) plus a fall-off-the-
    // end path (annotation-anchored TS7030) are not mutually exclusive.
    let out = codes(
        r#"
function delta(n: number): number {
    if (n === 1) {
        return;
    } else if (n === 2) {
        return 2;
    }
}
"#,
        true,
        false,
    );
    assert_eq!(
        count(&out, 7030),
        2,
        "expected two TS7030 (bare + fall-off), got {out:?}"
    );
}

#[test]
fn unreachable_bare_return_after_value_return_still_reports() {
    // tsc checks every return statement regardless of reachability.
    let out = codes(
        r#"
function epsilon(): number {
    return 1;
    return;
}
"#,
        true,
        false,
    );
    assert_eq!(
        count(&out, 7030),
        1,
        "unreachable bare return still reports: {out:?}"
    );
}

#[test]
fn bare_return_in_dead_if_false_still_reports() {
    let out = codes(
        r#"
function zeta(): number {
    if (false) {
        return;
    }
    return 1;
}
"#,
        true,
        false,
    );
    assert_eq!(
        count(&out, 7030),
        1,
        "bare return in dead branch still reports: {out:?}"
    );
}

#[test]
fn bare_return_in_try_and_catch_report_each() {
    let out = codes(
        r#"
function eta(n: number): number {
    try {
        if (n > 0) return 1;
        return;
    } catch {
        return;
    }
}
"#,
        true,
        false,
    );
    assert_eq!(
        count(&out, 7030),
        2,
        "expected one per bare return in try/catch: {out:?}"
    );
}

#[test]
fn bare_return_in_loop_reports() {
    let out = codes(
        r#"
function theta(items: string[]): string {
    for (const item of items) {
        if (item) return item;
        return;
    }
    return "";
}
"#,
        true,
        false,
    );
    assert_eq!(
        count(&out, 7030),
        1,
        "bare return inside loop reports: {out:?}"
    );
}

#[test]
fn method_bare_return_reports() {
    let out = codes(
        r#"
class Container {
    pick(flag: boolean): number {
        if (flag) {
            return 1;
        }
        return;
    }
}
"#,
        true,
        false,
    );
    assert_eq!(count(&out, 7030), 1, "method bare return reports: {out:?}");
}

#[test]
fn getter_bare_return_reports() {
    let out = codes(
        r#"
class Box {
    get size(): number {
        if (this.dirty) {
            return 1;
        }
        return;
    }
    dirty = true;
}
"#,
        true,
        false,
    );
    assert_eq!(count(&out, 7030), 1, "getter bare return reports: {out:?}");
}

#[test]
fn function_expression_bare_return_reports() {
    let out = codes(
        "const pickValue = function (flag: boolean): number { if (flag) { return 1; } return; };",
        true,
        false,
    );
    assert_eq!(
        count(&out, 7030),
        1,
        "function-expression bare return reports: {out:?}"
    );
}

#[test]
fn arrow_bare_return_reports() {
    let out = codes(
        "const compute = (flag: boolean): number => { if (flag) { return 1; } return; };",
        true,
        false,
    );
    assert_eq!(count(&out, 7030), 1, "arrow bare return reports: {out:?}");
}

#[test]
fn nested_function_bare_return_belongs_to_inner_only() {
    // The inner function's bare return reports once; the outer function (which
    // returns `inner()`) does not add a second.
    let out = codes(
        r#"
function outer(): number {
    function inner(): number {
        return;
    }
    return inner();
}
"#,
        true,
        false,
    );
    assert_eq!(
        count(&out, 7030),
        1,
        "only the inner bare return reports: {out:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls — must stay silent.
// ---------------------------------------------------------------------------

#[test]
fn no_implicit_returns_disabled_stays_silent() {
    let out = codes(
        r#"
function iota(flag: boolean): number {
    if (flag) {
        return 1;
    }
    return;
}
"#,
        false,
        false,
    );
    assert_eq!(
        count(&out, 7030),
        0,
        "TS7030 needs noImplicitReturns: {out:?}"
    );
}

#[test]
fn void_return_type_stays_silent() {
    let out = codes(
        r#"
function kappa(flag: boolean): void {
    if (flag) {
        return;
    }
    return;
}
"#,
        true,
        false,
    );
    assert_eq!(
        count(&out, 7030),
        0,
        "void return requires no value: {out:?}"
    );
}

#[test]
fn any_return_type_stays_silent() {
    let out = codes(
        r#"
function lambda(flag: boolean): any {
    if (flag) {
        return 1;
    }
    return;
}
"#,
        true,
        false,
    );
    assert_eq!(
        count(&out, 7030),
        0,
        "any return requires no value: {out:?}"
    );
}

#[test]
fn setter_bare_return_stays_silent() {
    let out = codes(
        r#"
class Cell {
    set value(v: number) {
        if (v) {
            return;
        }
        return;
    }
}
"#,
        true,
        false,
    );
    assert_eq!(
        count(&out, 7030),
        0,
        "setter returns void, no TS7030: {out:?}"
    );
}

#[test]
fn constructor_bare_return_stays_silent() {
    let out = codes(
        r#"
class Widget {
    constructor(flag: boolean) {
        if (flag) {
            return;
        }
        return;
    }
}
"#,
        true,
        false,
    );
    assert_eq!(
        count(&out, 7030),
        0,
        "constructors are excluded from TS7030: {out:?}"
    );
}

#[test]
fn strict_null_checks_uses_assignability_not_ts7030() {
    // With strictNullChecks a bare `return;` is checked as `undefined` against
    // `number`, producing TS2322 (not TS7030).
    let out = codes(
        r#"
function mu(flag: boolean): number {
    if (flag) {
        return 1;
    }
    return;
}
"#,
        true,
        true,
    );
    assert_eq!(
        count(&out, 7030),
        0,
        "strict mode routes through assignability: {out:?}"
    );
    assert_eq!(
        count(&out, 2322),
        1,
        "expected TS2322 for undefined vs number: {out:?}"
    );
}

#[test]
fn strict_null_checks_union_with_undefined_stays_silent() {
    // `number | undefined` accepts `undefined`, so neither TS2322 nor TS7030.
    let out = codes(
        r#"
function nu(flag: boolean): number | undefined {
    if (flag) {
        return 1;
    }
    return;
}
"#,
        true,
        true,
    );
    assert_eq!(count(&out, 7030), 0, "no TS7030 under strict: {out:?}");
    assert_eq!(
        count(&out, 2322),
        0,
        "undefined is assignable to the union: {out:?}"
    );
}
