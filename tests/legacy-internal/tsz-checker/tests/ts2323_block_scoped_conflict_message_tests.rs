//! TS2323 ("Cannot redeclare exported variable") only applies when *every*
//! declaration taking part in the conflict is a function-scoped `var`.
//!
//! A `let`/`const` binding carries the `VARIABLE` symbol flag alongside
//! `BLOCK_SCOPED_VARIABLE`, so "every conflicting declaration is a variable"
//! does not imply "every conflicting declaration is a `var`". Once a
//! block-scoped binding participates, tsc falls back to its binder-level
//! redeclaration message and chooses between TS2451 and TS2300 by the
//! block-scopedness of the *first* conflicting declaration in source order.
//!
//! Every expectation below is pinned against `tsc` 7.0.2 run with
//! `--noEmit --strict --pretty false --lib es2015 --target es2015`.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    let mut out: Vec<u32> = diags.iter().map(|d| d.code).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// `export var` then `export let`: the first conflicting declaration is
/// function-scoped, so tsc reports TS2300 on both declarations.
#[test]
fn exported_var_then_let_is_ts2300() {
    let diags = check_source_diagnostics(
        r#"
export var alpha = 1;
export let alpha = 2;
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2300],
        "`export var` + `export let` must report TS2300, not TS2323; got: {diags:?}"
    );
}

/// `export let` then `export var`: the first conflicting declaration is
/// block-scoped, so the same pair flips to TS2451. The order dependence is the
/// whole point — a predicate that only asks "is any declaration block-scoped"
/// cannot tell these two rows apart.
#[test]
fn exported_let_then_var_is_ts2451() {
    let diags = check_source_diagnostics(
        r#"
export let alpha = 1;
export var alpha = 2;
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2451],
        "`export let` + `export var` must report TS2451, not TS2323; got: {diags:?}"
    );
}

/// `const` behaves exactly like `let` here: block-scoped first means TS2451.
#[test]
fn exported_const_then_var_is_ts2451() {
    let diags = check_source_diagnostics(
        r#"
export const alpha = 1;
export var alpha = 2;
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2451],
        "`export const` + `export var` must report TS2451, not TS2323; got: {diags:?}"
    );
}

/// ...and `var` first means TS2300, whichever block-scoped keyword follows.
#[test]
fn exported_var_then_const_is_ts2300() {
    let diags = check_source_diagnostics(
        r#"
export var alpha = 1;
export const alpha = 2;
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2300],
        "`export var` + `export const` must report TS2300, not TS2323; got: {diags:?}"
    );
}

/// The positive control: an all-`var` exported conflict is the one shape TS2323
/// is actually for, and it must keep reporting TS2323.
#[test]
fn exported_var_then_var_stays_ts2323() {
    let diags = check_source_diagnostics(
        r#"
export var alpha = 1;
export var alpha = 2;
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2323],
        "all-`var` exported redeclaration must stay TS2323; got: {diags:?}"
    );
}

/// The other side of the same control: an all-block-scoped exported conflict
/// never reached the TS2323 arm and must stay TS2451.
#[test]
fn exported_let_then_let_stays_ts2451() {
    let diags = check_source_diagnostics(
        r#"
export let alpha = 1;
export let alpha = 2;
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2451],
        "all-`let` exported redeclaration must stay TS2451; got: {diags:?}"
    );
}

/// Three declarations, block-scoped first: still TS2451 across the whole group.
#[test]
fn exported_let_var_let_is_ts2451() {
    let diags = check_source_diagnostics(
        r#"
export let alpha = 1;
export var alpha = 2;
export let alpha = 3;
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2451],
        "`let`/`var`/`let` exported conflict must report TS2451; got: {diags:?}"
    );
}

/// Renamed binder: the decision is a symbol-flag test, not a name test.
#[test]
fn renamed_binder_exported_var_then_let_is_ts2300() {
    let diags = check_source_diagnostics(
        r#"
export var zetaBinding = 1;
export let zetaBinding = 2;
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2300],
        "renamed binder must behave identically; got: {diags:?}"
    );
}

/// Explicit type annotations do not change the message selection.
#[test]
fn annotated_exported_var_then_const_is_ts2300() {
    let diags = check_source_diagnostics(
        r#"
export var alpha: number = 1;
export const alpha: string = "x";
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2300],
        "annotated declarations must behave identically; got: {diags:?}"
    );
}

/// Inside a namespace body, `export var` + `export let` is a genuine conflict
/// and reports TS2300 — the namespace `export var` merge exemption covers
/// var/var merges only, and must not extend to a block-scoped sibling.
#[test]
fn namespace_exported_var_then_let_is_ts2300() {
    let diags = check_source_diagnostics(
        r#"
export namespace Holder {
  export var alpha = 1;
  export let alpha = 2;
}
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2300],
        "namespace-internal `export var` + `export let` must report TS2300; got: {diags:?}"
    );
}

/// Negative control: a plain, unexported `var`/`var` pair is legal and must
/// stay clean. TS2323 is gated on export-ness, and that gate is untouched.
#[test]
fn unexported_var_then_var_is_clean() {
    let diags = check_source_diagnostics(
        r#"
var alpha = 1;
var alpha = 2;
export {};
"#,
    );
    assert_eq!(
        codes(&diags),
        Vec::<u32>::new(),
        "unexported `var` redeclaration is legal; got: {diags:?}"
    );
}

/// Negative control: an unexported `var`/`let` pair already took the fallback
/// arm before this change and must keep reporting TS2300.
#[test]
fn unexported_var_then_let_stays_ts2300() {
    let diags = check_source_diagnostics(
        r#"
var alpha = 1;
let alpha = 2;
export {};
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2300],
        "unexported `var` + `let` must stay TS2300; got: {diags:?}"
    );
}

/// Negative control: a non-variable participant (`function`) already forced the
/// fallback arm through `has_non_variable_conflict`, and still reports TS2300.
#[test]
fn exported_var_then_function_stays_ts2300() {
    let diags = check_source_diagnostics(
        r#"
export var alpha = 1;
export function alpha() {}
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2300],
        "`export var` + `export function` must stay TS2300; got: {diags:?}"
    );
}

/// Negative control: a `class` participant likewise stays TS2300.
#[test]
fn exported_var_then_class_stays_ts2300() {
    let diags = check_source_diagnostics(
        r#"
export var alpha = 1;
export class alpha {}
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2300],
        "`export var` + `export class` must stay TS2300; got: {diags:?}"
    );
}

/// Negative control: mixed export-ness reports TS2395 and never reaches the
/// redeclaration-message selection at all.
#[test]
fn mixed_exportedness_var_pair_is_ts2395() {
    let diags = check_source_diagnostics(
        r#"
export var alpha = 1;
var alpha = 2;
"#,
    );
    assert_eq!(
        codes(&diags),
        vec![2395],
        "mixed exported/local merged declarations report TS2395; got: {diags:?}"
    );
}
