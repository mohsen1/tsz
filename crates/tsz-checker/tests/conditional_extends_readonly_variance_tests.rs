//! Regression tests for issue #9743: in a conditional `S extends U`, a readonly
//! source paired with a mutable array/tuple target must take the false branch.
//!
//! Structural rule: when the target is a mutable array shape (`T[]` /
//! `Array<T>`) and the source is `readonly T[]` / `readonly [..]` /
//! `ReadonlyArray<T>`, the conditional `extends` relation rejects the source
//! for the same reason direct assignment errors with TS4104. Before this fix
//! the array fast path in conditional evaluation silently stripped the
//! `ReadonlyType` wrapper from the source, causing it to take the true branch.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::{Diagnostic, DiagnosticCategory};
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};
use tsz_common::common::{ModuleKind, ScriptTarget};

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ESNext,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
}

fn error_codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::Error)
        .map(|d| d.code)
        .collect()
}

const COND: &str = "type R<S, T> = S extends T ? \"Y\" : \"N\";\n";

#[test]
fn readonly_array_source_vs_mutable_array_target_is_false() {
    let source = format!("{COND}\nconst r: R<readonly number[], number[]> = \"N\";\n");
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "readonly number[] extends number[] should be false; got: {diags:#?}"
    );
}

#[test]
fn readonly_tuple_source_vs_mutable_array_target_is_false() {
    let source = format!("{COND}\nconst r: R<readonly [1, 2], number[]> = \"N\";\n");
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "readonly [1,2] extends number[] should be false; got: {diags:#?}"
    );
}

#[test]
fn readonly_tuple_source_via_named_alias_vs_mutable_array_target_is_false() {
    let source = r#"
type R<S, T> = S extends T ? "Y" : "N";
type Pair = readonly [1, 2];
const r: R<Pair, number[]> = "N";
"#;
    let diags = check(source);
    assert!(
        error_codes(&diags).is_empty(),
        "alias for readonly tuple extends number[] should be false; got: {diags:#?}"
    );
}

#[test]
fn renamed_conditional_and_type_params_still_apply_the_rule() {
    // Same rule, different surface names; proves the fix is structural.
    let source = r#"
type Cond<X, Y, Hit, Miss> = X extends Y ? Hit : Miss;
const r: Cond<readonly string[], string[], "Y", "N"> = "N";
"#;
    let diags = check(source);
    assert!(
        error_codes(&diags).is_empty(),
        "renamed conditional readonly string[] extends string[] should be false; got: {diags:#?}"
    );
}

#[test]
fn rule_applies_with_string_element_type_too() {
    let source = format!("{COND}\nconst r: R<readonly string[], string[]> = \"N\";\n");
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "readonly string[] extends string[] should be false; got: {diags:#?}"
    );
}

#[test]
fn readonly_array_interface_source_vs_mutable_array_interface_target_is_false() {
    let libs = load_lib_files(&["es5.d.ts"]);
    let source = format!("{COND}\nconst r: R<ReadonlyArray<number>, Array<number>> = \"N\";\n");
    let diags = check_source_with_libs(&source, "test.ts", CheckerOptions::default(), &libs);
    assert!(
        error_codes(&diags).is_empty(),
        "ReadonlyArray<number> extends Array<number> should be false; got: {diags:#?}"
    );
}

#[test]
fn control_readonly_extends_readonly_is_true() {
    let source = format!("{COND}\nconst r: R<readonly number[], readonly number[]> = \"Y\";\n");
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "readonly number[] extends readonly number[] should be true; got: {diags:#?}"
    );
}

#[test]
fn control_mutable_extends_readonly_is_true() {
    let source = format!("{COND}\nconst r: R<number[], readonly number[]> = \"Y\";\n");
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "number[] extends readonly number[] should be true; got: {diags:#?}"
    );
}

#[test]
fn control_mutable_extends_mutable_is_true() {
    let source = format!("{COND}\nconst r: R<number[], number[]> = \"Y\";\n");
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "number[] extends number[] should be true; got: {diags:#?}"
    );
}

#[test]
fn control_readonly_tuple_extends_mutable_tuple_is_false_unchanged() {
    let source = format!("{COND}\nconst r: R<readonly [1, 2], [1, 2]> = \"N\";\n");
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "readonly [1,2] extends [1,2] should be false; got: {diags:#?}"
    );
}

#[test]
fn infer_pattern_rejects_readonly_source_against_mutable_array_target() {
    let source = r#"
type Elem<T> = T extends (infer U)[] ? U : never;
type ROElem<T> = T extends readonly (infer U)[] ? U : never;

const a: Elem<number[]> = 0;
const b: Elem<readonly number[]> = (null as never);
const c: ROElem<number[]> = 0;
const d: ROElem<readonly number[]> = 0;
"#;
    let diags = check(source);
    assert!(
        error_codes(&diags).is_empty(),
        "infer pattern variance for readonly source must match tsc; got: {diags:#?}"
    );
}

#[test]
fn distributive_infer_pattern_filters_readonly_union_member() {
    let source = r#"
type Elem<T> = T extends (infer U)[] ? U : never;
const ok: Elem<readonly number[] | string[]> = "s";
const bad: Elem<readonly number[] | string[]> = 1;
"#;
    let codes = error_codes(&check(source));
    assert_eq!(
        codes,
        vec![2322],
        "readonly union member should fall to never while mutable string[] contributes string"
    );
}

#[test]
fn non_distributive_infer_pattern_rejects_readonly_union_member() {
    let source = r#"
type Elem<T> = [T] extends [(infer U)[]] ? U : never;
const r: Elem<readonly number[] | string[]> = (null as never);
"#;
    let diags = check(source);
    assert!(
        error_codes(&diags).is_empty(),
        "non-distributive union with readonly member should reject the mutable array target; got: {diags:#?}"
    );
}

#[test]
fn conditional_branch_matches_direct_assignment_verdict() {
    let source = r#"
type R<S, T> = S extends T ? "Y" : "N";
declare const ro: readonly number[];
const _direct: number[] = ro;
const conditional: R<readonly number[], number[]> = "N";
"#;
    let codes = error_codes(&check(source));
    assert_eq!(
        codes,
        vec![4104],
        "expected only TS4104 from direct assignment; the conditional branch must agree"
    );
}
