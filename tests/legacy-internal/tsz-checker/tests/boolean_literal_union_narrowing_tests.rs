//! Narrowing a union member of type `boolean` by a boolean-literal equality
//! guard must decompose the member into `true | false` and drop the excluded
//! literal, matching tsc's `narrowTypeByEquality` over the implicit
//! `true | false` representation of `boolean`.
//!
//! Regression for the mobx `make_(annotation: Annotation | boolean)` family,
//! where `if (a === true)` / `if (a === false)` must leave the object facet
//! (false TS2339 `Property ... does not exist on type 'boolean | Annotation'`).
//! Binder names are varied across cases per the anti-hardcoding gate.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics.iter().map(|d| d.code).collect()
}

/// `=== true` false-branch drops `true` from a `boolean` union member,
/// leaving the object facet plus `false`.
#[test]
fn eq_true_false_branch_keeps_object_and_false() {
    let diagnostics = check_source_diagnostics(
        r#"
type Marker = { tag_: string };
function classify(entry: Marker | boolean): void {
    if (entry === true) {
        return;
    }
    const rest: Marker | false = entry;
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "expected `entry` to narrow to `Marker | false`, got {:?}",
        codes(&diagnostics)
    );
}

/// Excluding both `true` and `false` removes `boolean` entirely, leaving the
/// object facet so member access on it is valid (the mobx `make_` shape).
#[test]
fn eq_true_and_eq_false_leave_only_object_facet() {
    let diagnostics = check_source_diagnostics(
        r#"
type Setting = { kind_: string; apply_(): number };
function handle(opt: Setting | boolean): void {
    if (opt === true) {
        return;
    }
    if (opt === false) {
        return;
    }
    const name = opt.kind_;
    opt.apply_();
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "expected `opt` to narrow to `Setting` after excluding both boolean literals, got {:?}",
        codes(&diagnostics)
    );
}

/// Assignment-narrowing in the `=== true` branch followed by `=== false`
/// exclusion, exactly mirroring mobx's `make_` (reassign `true` to a default
/// object, then drop `false`).
#[test]
fn reassign_true_branch_then_exclude_false() {
    let diagnostics = check_source_diagnostics(
        r#"
type Rule = { code_: string; run_(): number };
declare const fallback: Rule;
function evaluate(rule: Rule | boolean): void {
    if (rule === true) {
        rule = fallback;
    }
    if (rule === false) {
        return;
    }
    const id = rule.code_;
    rule.run_();
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "expected `rule` to narrow to `Rule`, got {:?}",
        codes(&diagnostics)
    );
}

/// The same decomposition applies when the boolean facet is spelled as an
/// explicit `true | false` union rather than the `boolean` keyword.
#[test]
fn explicit_true_false_union_member_decomposes() {
    let diagnostics = check_source_diagnostics(
        r#"
type Token = { text_: string };
function read(piece: Token | true | false): void {
    if (piece === false) {
        return;
    }
    if (piece === true) {
        return;
    }
    const value = piece.text_;
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "expected `piece` to narrow to `Token`, got {:?}",
        codes(&diagnostics)
    );
}

/// Negative control: the positive branch of `=== true` still narrows to the
/// `true` literal (no regression to the equality side of the guard).
#[test]
fn eq_true_positive_branch_narrows_to_true() {
    let diagnostics = check_source_diagnostics(
        r#"
type Flagged = { note_: string };
function inspect(flag: Flagged | boolean): void {
    if (flag === true) {
        const exact: true = flag;
    }
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "expected positive branch to narrow `flag` to `true`, got {:?}",
        codes(&diagnostics)
    );
}

/// Negative control: a non-boolean literal guard over the same union must not
/// be affected by the boolean-decomposition path.
#[test]
fn non_boolean_member_exclusion_unaffected() {
    let diagnostics = check_source_diagnostics(
        r#"
type Holder = { item_: string };
function pick(slot: Holder | 0 | 1): void {
    if (slot === 0) {
        return;
    }
    const rest: Holder | 1 = slot;
}
"#,
    );
    assert_eq!(
        diagnostics.len(),
        0,
        "expected numeric-literal exclusion to leave `Holder | 1`, got {:?}",
        codes(&diagnostics)
    );
}
