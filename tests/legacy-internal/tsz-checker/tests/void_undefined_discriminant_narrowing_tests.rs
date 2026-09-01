//! Narrowing a union containing `void` by an undefined discriminator must
//! remove the `void` member, matching tsc's `NEUndefined`/`EQUndefined` type
//! facts: `void`'s sole inhabitant is `undefined`, so `x !== undefined`
//! (and `typeof x !== "undefined"`, plus the symmetric `=== undefined` false
//! branch) discards it.
//!
//! Regression for es-toolkit (`isEqualWith.ts`): a `() => boolean | void`
//! callback result narrowed by `if (result !== undefined)` wrongly kept
//! `void`, yielding a spurious TS2322 against a `boolean` return target.
//! Binder names are varied across cases per the anti-hardcoding gate.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics.iter().map(|d| d.code).collect()
}

/// Strict `!== undefined` removes `void` from `boolean | void`.
#[test]
fn strict_ne_undefined_removes_void() {
    let diagnostics = check_source_diagnostics(
        r#"
declare const probe: () => boolean | void;
function decide(): boolean {
    const outcome = probe();
    if (outcome !== undefined) {
        return outcome;
    }
    return false;
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "`outcome` should narrow to `boolean`, got {:?}",
        codes(&diagnostics)
    );
}

/// Loose `!= undefined` (which also strips `null`) removes `void`.
#[test]
fn loose_ne_undefined_removes_void() {
    let diagnostics = check_source_diagnostics(
        r#"
declare const fetchFlag: () => boolean | void;
function resolve(): boolean {
    const flag = fetchFlag();
    if (flag != undefined) {
        return flag;
    }
    return true;
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "`flag` should narrow to `boolean`, got {:?}",
        codes(&diagnostics)
    );
}

/// `typeof x !== "undefined"` removes `void`.
#[test]
fn typeof_ne_undefined_removes_void() {
    let diagnostics = check_source_diagnostics(
        r#"
declare const compute: () => boolean | void;
function pick(): boolean {
    const value = compute();
    if (typeof value !== "undefined") {
        return value;
    }
    return false;
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "`value` should narrow to `boolean`, got {:?}",
        codes(&diagnostics)
    );
}

/// The symmetric `=== undefined` false branch removes `void`.
#[test]
fn eq_undefined_false_branch_removes_void() {
    let diagnostics = check_source_diagnostics(
        r#"
declare const gather: () => boolean | void;
function evaluate(): boolean {
    const collected = gather();
    if (collected === undefined) {
        return false;
    }
    return collected;
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "`collected` should narrow to `boolean`, got {:?}",
        codes(&diagnostics)
    );
}

/// Yoda form `undefined !== x` removes `void` too.
#[test]
fn yoda_ne_undefined_removes_void() {
    let diagnostics = check_source_diagnostics(
        r#"
declare const sample: () => boolean | void;
function judge(): boolean {
    const item = sample();
    if (undefined !== item) {
        return item;
    }
    return false;
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "`item` should narrow to `boolean`, got {:?}",
        codes(&diagnostics)
    );
}

/// The positive `=== undefined` branch keeps `void` (dual of the exclusion).
#[test]
fn eq_undefined_true_branch_keeps_void() {
    let diagnostics = check_source_diagnostics(
        r#"
declare const observe: () => boolean | void;
function inspect(): void {
    const reading = observe();
    if (reading === undefined) {
        const captured: void = reading;
        return captured;
    }
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "positive branch should keep `void`, got {:?}",
        codes(&diagnostics)
    );
}

/// Without a narrowing guard, the `void` member still triggers TS2322 — the
/// fix must not introduce a false negative.
#[test]
fn unnarrowed_void_still_reports_ts2322() {
    let diagnostics = check_source_diagnostics(
        r#"
declare const yield_: () => boolean | void;
function passthrough(): boolean {
    const carried = yield_();
    return carried;
}
"#,
    );
    assert_eq!(
        codes(&diagnostics),
        vec![2322],
        "unnarrowed `boolean | void` must still fail against `boolean`"
    );
}

/// Excluding a non-`undefined` type must not drop `void`.
#[test]
fn non_undefined_exclusion_keeps_void() {
    let diagnostics = check_source_diagnostics(
        r#"
declare const produce: () => boolean | void;
function route(): boolean | void {
    const slot = produce();
    if (typeof slot === "boolean") {
        return slot;
    }
    return slot;
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "non-undefined exclusion should keep `void`, got {:?}",
        codes(&diagnostics)
    );
}
