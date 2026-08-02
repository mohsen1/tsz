//! `var` merges across `export var` redeclarations of the same name inside a
//! namespace: tsc treats `namespace A { export var x = 1; export var x = 2; }`
//! as one variable, whether the two declarations sit in one body or several.
//! TS2323 ("Cannot redeclare exported variable") only applies to a module's
//! own top-level exports, not to namespace-internal `export var` merges.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;

fn ts2323_count(diags: &[Diagnostic]) -> usize {
    diags.iter().filter(|d| d.code == 2323).count()
}

#[test]
fn namespace_single_body_export_var_merge_is_clean() {
    let diags = check_source_diagnostics(
        r#"
namespace A { export var x = 1; export var x = 2; }
export {};
"#,
    );
    assert_eq!(
        ts2323_count(&diags),
        0,
        "namespace-internal `export var` merge must not report TS2323; got: {diags:?}"
    );
}

#[test]
fn namespace_two_bodies_export_var_merge_is_clean() {
    let diags = check_source_diagnostics(
        r#"
namespace A { export var x = 1; }
namespace A { export var x = 2; }
export {};
"#,
    );
    assert_eq!(
        ts2323_count(&diags),
        0,
        "cross-body namespace `export var` merge must not report TS2323; got: {diags:?}"
    );
}

#[test]
fn namespace_three_bodies_export_var_merge_is_clean() {
    let diags = check_source_diagnostics(
        r#"
namespace A { export var x = 1; }
namespace A { export var x = 2; }
namespace A { export var x = 3; }
export {};
"#,
    );
    assert_eq!(
        ts2323_count(&diags),
        0,
        "three-body namespace `export var` merge must not report TS2323; got: {diags:?}"
    );
}

#[test]
fn namespace_export_var_merge_is_clean_in_a_script_file_too() {
    let diags = check_source_diagnostics(
        r#"
namespace A { export var x = 1; export var x = 2; }
"#,
    );
    assert_eq!(
        ts2323_count(&diags),
        0,
        "namespace `export var` merge must be clean in a script file; got: {diags:?}"
    );
}

#[test]
fn module_top_level_export_var_redeclaration_still_reports_ts2323() {
    let diags = check_source_diagnostics(
        r#"
export var x = 1;
export var x = 2;
"#,
    );
    assert_eq!(
        ts2323_count(&diags),
        2,
        "top-level `export var` redeclaration in a module must still report TS2323 on both \
         declarations; got: {diags:?}"
    );
}

#[test]
fn module_top_level_export_var_redeclaration_with_sibling_namespace_still_reports_ts2323() {
    let diags = check_source_diagnostics(
        r#"
namespace A { export var x = 1; }
export var x2 = 1;
export var x2 = 2;
export {};
"#,
    );
    assert_eq!(
        ts2323_count(&diags),
        2,
        "a namespace-scoped sibling must not suppress a genuine module-level `export var` \
         redeclaration; got: {diags:?}"
    );
}

#[test]
fn namespace_export_let_redeclaration_still_reports_ts2451() {
    let diags = check_source_diagnostics(
        r#"
namespace A { export let x = 1; export let x = 2; }
export {};
"#,
    );
    let ts2451 = diags.iter().filter(|d| d.code == 2451).count();
    assert_eq!(
        ts2451, 2,
        "`let` must still refuse to merge inside a namespace (sibling arm, unaffected by the \
         `var` fix); got: {diags:?}"
    );
    assert_eq!(
        ts2323_count(&diags),
        0,
        "must not report TS2323; got: {diags:?}"
    );
}

#[test]
fn namespace_export_function_redeclaration_still_reports_ts2393() {
    let diags = check_source_diagnostics(
        r#"
namespace A { export function f() {} export function f() {} }
export {};
"#,
    );
    let ts2393 = diags.iter().filter(|d| d.code == 2393).count();
    assert_eq!(
        ts2393, 2,
        "duplicate function implementations must still be rejected inside a namespace \
         (sibling arm, unaffected by the `var` fix); got: {diags:?}"
    );
    assert_eq!(
        ts2323_count(&diags),
        0,
        "must not report TS2323; got: {diags:?}"
    );
}

#[test]
fn namespace_export_var_merge_is_clean_with_renamed_binder() {
    let diags = check_source_diagnostics(
        r#"
namespace Zeta { export var probe = 1; export var probe = 2; }
export {};
"#,
    );
    assert_eq!(
        ts2323_count(&diags),
        0,
        "the fix must not key on the identifier name; got: {diags:?}"
    );
}

#[test]
fn namespace_export_var_merge_is_clean_with_type_annotation() {
    let diags = check_source_diagnostics(
        r#"
namespace A { export var x: number; export var x: number = 2; }
export {};
"#,
    );
    assert_eq!(
        ts2323_count(&diags),
        0,
        "annotated namespace `export var` merge must not report TS2323; got: {diags:?}"
    );
}
