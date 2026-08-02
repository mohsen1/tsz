//! TS2323 ("Cannot redeclare exported variable") only covers a pure
//! `var`-vs-`var` exported redeclaration at module top level. tsc's binder
//! picks the redeclaration message off `symbol.flags & BlockScopedVariable`
//! at the moment the conflicting declaration binds, independent of `export`;
//! a `var`/`let`/`const` mix among the conflicting declarations must fall
//! back to the ordinary TS2300/TS2451 order-dependent selection instead.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

fn count(diags: &[Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

#[test]
fn exported_var_then_let_reports_ts2300_not_ts2323() {
    let diags = check_source_diagnostics(
        r#"
export var x = 1;
export let x = 2;
"#,
    );
    assert_eq!(count(&diags, 2300), 2, "expected TS2300 x2; got: {diags:?}");
    assert_eq!(
        count(&diags, 2323),
        0,
        "must not report TS2323; got: {diags:?}"
    );
    assert_eq!(
        count(&diags, 2451),
        0,
        "must not report TS2451; got: {diags:?}"
    );
}

#[test]
fn exported_let_then_var_reports_ts2451_not_ts2323() {
    let diags = check_source_diagnostics(
        r#"
export let x = 1;
export var x = 2;
"#,
    );
    assert_eq!(count(&diags, 2451), 2, "expected TS2451 x2; got: {diags:?}");
    assert_eq!(
        count(&diags, 2323),
        0,
        "must not report TS2323; got: {diags:?}"
    );
    assert_eq!(
        count(&diags, 2300),
        0,
        "must not report TS2300; got: {diags:?}"
    );
}

#[test]
fn exported_const_then_var_renamed_binder_reports_ts2451() {
    let diags = check_source_diagnostics(
        r#"
export const probe = 1;
export var probe = 2;
"#,
    );
    assert_eq!(count(&diags, 2451), 2, "expected TS2451 x2; got: {diags:?}");
    assert_eq!(
        count(&diags, 2323),
        0,
        "must not report TS2323; got: {diags:?}"
    );
}

#[test]
fn exported_var_then_const_renamed_binder_reports_ts2300() {
    let diags = check_source_diagnostics(
        r#"
export var probe = 1;
export const probe = 2;
"#,
    );
    assert_eq!(count(&diags, 2300), 2, "expected TS2300 x2; got: {diags:?}");
    assert_eq!(
        count(&diags, 2323),
        0,
        "must not report TS2323; got: {diags:?}"
    );
}

#[test]
fn exported_var_then_var_still_reports_ts2323() {
    let diags = check_source_diagnostics(
        r#"
export var y = 1;
export var y = 2;
"#,
    );
    assert_eq!(count(&diags, 2323), 2, "expected TS2323 x2; got: {diags:?}");
}

#[test]
fn non_exported_var_then_let_still_reports_ts2300() {
    let diags = check_source_diagnostics(
        r#"
var x = 1;
let x = 2;
"#,
    );
    assert_eq!(count(&diags, 2300), 2, "expected TS2300 x2; got: {diags:?}");
    assert_eq!(
        count(&diags, 2323),
        0,
        "must not report TS2323; got: {diags:?}"
    );
}

#[test]
fn non_exported_let_then_var_still_reports_ts2451() {
    let diags = check_source_diagnostics(
        r#"
let x = 1;
var x = 2;
"#,
    );
    assert_eq!(count(&diags, 2451), 2, "expected TS2451 x2; got: {diags:?}");
    assert_eq!(
        count(&diags, 2323),
        0,
        "must not report TS2323; got: {diags:?}"
    );
}
