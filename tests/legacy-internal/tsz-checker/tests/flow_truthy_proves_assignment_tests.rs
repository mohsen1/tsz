//! Definite-assignment (TS2454) parity for bare truthiness conditions.
//!
//! Structural rule: when a possibly-unassigned reference is read in a
//! truthiness condition (`if (ref)` / `if (!ref)`), tsc reports TS2454 once at
//! the condition read, then narrows the falsy "unassigned" marker away on the
//! positive (truthy) branch. Subsequent uses on that branch are treated as
//! definitely assigned, so no further TS2454 (and no cascading TS2339) fires.
//! The negative (falsy) branch keeps the marker and still reports.

use crate::test_utils::check_source_strict_codes as check_strict;

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn truthy_branch_of_negated_condition_is_assigned() {
    // `if (!ec) {} else { ec.foo() }` — the else branch is the truthy sense.
    // tsc reports TS2454 once (at the condition read) and treats `ec` as
    // assigned with its narrowed type in the else branch, so `ec.foo()` is OK.
    let codes = check_strict(
        r#"
declare const enabled: boolean;
let ec: { foo(): void } | false;
try { ec = enabled && (window as any).__X__; } catch {}
if (!ec) { } else { ec.foo(); }
"#,
    );

    assert_eq!(
        count(&codes, 2454),
        1,
        "expected exactly one TS2454 at the condition read, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "truthy-branch use must not cascade into TS2339, got codes: {codes:?}"
    );
}

#[test]
fn truthy_branch_of_plain_condition_is_assigned() {
    // `if (ec) { ec.foo() }` — the then branch is the truthy sense.
    let codes = check_strict(
        r#"
declare const enabled: boolean;
let ec: { foo(): void } | false;
try { ec = enabled && (window as any).__X__; } catch {}
if (ec) { ec.foo(); } else { }
"#,
    );

    assert_eq!(
        count(&codes, 2454),
        1,
        "expected exactly one TS2454 at the condition read, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "truthy-branch use must not cascade into TS2339, got codes: {codes:?}"
    );
}

#[test]
fn truthy_branch_suppresses_even_when_never_assigned() {
    // Negative control matching tsc: a variable that is genuinely never
    // assigned still reports TS2454 only at the condition read; the truthy
    // branch use is suppressed (tsc behaviour, verified against tsc 6.0.3).
    let codes = check_strict(
        r#"
let y: number;
if (y) { y.toFixed(); }
"#,
    );

    assert_eq!(
        count(&codes, 2454),
        1,
        "expected exactly one TS2454 at the condition read, got codes: {codes:?}"
    );
}

#[test]
fn falsy_branch_still_reports_use_before_assignment() {
    // Negative control: the falsy (else of `if (ec)`) branch keeps the
    // unassigned marker, so a use there must STILL report TS2454. This guards
    // against over-suppression.
    let codes = check_strict(
        r#"
let y: number;
if (y) { } else { y.toFixed(); }
"#,
    );

    assert_eq!(
        count(&codes, 2454),
        2,
        "expected TS2454 at both the condition read and the falsy-branch use, got codes: {codes:?}"
    );
}

#[test]
fn short_circuit_operand_does_not_falsely_prove_assignment() {
    // Regression guard for the narrowed bare-ref rule: a bare reference reached
    // through a short-circuit operand position (here `!x` as the right operand
    // of `&&`, evaluated in the `&&`'s false sense because it sits under the
    // `||`'s true branch) is NOT guaranteed evaluated/truthy, so the unassigned
    // marker survives and tsc still reports the truthy-branch read.
    //
    // `(typeof x === 'boolean' && !x) || typeof x === 'string'`: tsc reports
    // TS2454 at the first `typeof x` (col 13) and the `||`-right `typeof x`
    // (col 62), but NOT at the `&&`-right `!x`. Two condition reads in total.
    let codes = check_strict(
        r#"
let strOrBool: string | boolean;
if ((typeof strOrBool === 'boolean' && !strOrBool) || typeof strOrBool === 'string') {
}
"#,
    );

    assert_eq!(
        count(&codes, 2454),
        2,
        "both guaranteed condition reads must report; the short-circuit `!x` operand must not falsely prove assignment, got codes: {codes:?}"
    );
}

#[test]
fn or_true_branch_does_not_prove_assignment() {
    // `if (x || foo()) { x }` — the truthy branch of `||` does not narrow the
    // unassigned marker away from `x` (it could be the `foo()` path), so the
    // body read still reports. Two TS2454 total (condition read + body read).
    let codes = check_strict(
        r#"
declare function foo(): boolean;
let x: number;
if (x || foo()) { x.toFixed(); }
"#,
    );

    assert_eq!(
        count(&codes, 2454),
        2,
        "the `||` true branch must not prove assignment for a bare reference, got codes: {codes:?}"
    );
}

#[test]
fn and_true_branch_still_proves_assignment() {
    // `if (x && foo()) { x }` — the truthy branch of `&&` DOES narrow `x` to
    // truthy (x is always evaluated and must be truthy), so the body read is
    // suppressed. Exactly one TS2454 at the condition read. This keeps the
    // zustand-style suppression intact for guaranteed-truthy operands.
    let codes = check_strict(
        r#"
declare function foo(): boolean;
let x: number;
if (x && foo()) { x.toFixed(); }
"#,
    );

    assert_eq!(
        count(&codes, 2454),
        1,
        "the `&&` true branch must still prove assignment for the guaranteed-truthy left operand, got codes: {codes:?}"
    );
}

#[test]
fn plain_reads_without_condition_still_report_each_use() {
    // Negative control: without an intervening condition, every read of a
    // possibly-unassigned variable reports TS2454 (tsc does not dedup here).
    let codes = check_strict(
        r#"
declare const enabled: boolean;
let ec: number;
try { ec = enabled ? 1 : 2; } catch {}
ec;
let z: number = ec;
"#,
    );

    assert_eq!(
        count(&codes, 2454),
        2,
        "expected TS2454 at each plain read, got codes: {codes:?}"
    );
}
