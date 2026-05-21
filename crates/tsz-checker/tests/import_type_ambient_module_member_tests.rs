//! Regression tests for #9768: `import("mod").TypeName` import-type member
//! projection resolves to `any` for ambient modules.
//!
//! Structural rule: when an `import("mod").Member` qualifier projects a
//! member from a module, the member symbol must resolve through the same
//! effective-module-exports lookup that `typeof import("mod")` and named
//! imports use — regardless of whether `"mod"` is backed by a source file
//! or by an ambient `declare module "mod"` declaration in the current
//! file (or any other file).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, check_source_strict_messages, diagnostic_code_messages,
    load_lib_files,
};

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    check_source_strict_messages(source)
}

#[test]
fn ambient_module_member_projection_interface_emits_ts2322() {
    let diags = diagnostics(
        r#"
declare module "wm" {
  export const value: { a: number };
  export interface Thing { v: string; }
}
type Member = import("wm").Thing;
const m: Member = { v: 99 };
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 from `const m: Member = {{ v: 99 }}` where \
         `Member = import(\"wm\").Thing` from an ambient module. Got: {diags:#?}"
    );
}

#[test]
fn ambient_module_typeof_whole_module_still_checked() {
    // Control: the `typeof import("mod")` whole-module form must keep
    // reporting the same diagnostic as before, so the fix did not regress
    // the working path.
    let diags = diagnostics(
        r#"
declare module "wm" {
  export const value: { a: number };
  export interface Thing { v: string; }
}
type Whole = typeof import("wm");
const wbad: Whole = { value: { a: "x" } };
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 for `typeof import(\"wm\")` whole-module assignment \
         (control case from #9768 must remain green). Got: {diags:#?}"
    );
}

#[test]
fn ambient_module_member_projection_clean_assignment_no_error() {
    let diags = diagnostics(
        r#"
declare module "wm" {
  export interface Thing { v: string; }
}
type Member = import("wm").Thing;
const m: Member = { v: "ok" };
"#,
    );
    assert!(
        diags.is_empty(),
        "Expected zero diagnostics for a correctly-typed ambient import-type \
         member assignment. Got: {diags:#?}"
    );
}

#[test]
fn ambient_module_member_property_access_emits_ts2339() {
    // Property-access TS2339 on an ambient-module import-type member must
    // fire — tsz previously silently returned `any` for the binding type,
    // hiding the missing property diagnostic.
    let diags = diagnostics(
        r#"
declare module "wm" {
  export interface Thing { v: string; }
}
declare const m: import("wm").Thing;
m.notAProperty;
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2339),
        "Expected TS2339 for `m.notAProperty` where `m: import(\"wm\").Thing`. \
         Got: {diags:#?}"
    );
}

#[test]
fn ambient_module_member_generic_instantiation() {
    let diags = diagnostics(
        r#"
declare module "wm" {
  export interface Box<T> { value: T }
}
type StrBox = import("wm").Box<string>;
const bad: StrBox = { value: 99 };
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 from a generic `import(\"wm\").Box<string>` member \
         projection on an ambient module. Got: {diags:#?}"
    );
}

#[test]
fn ambient_module_member_class_projection() {
    // Classes are values + types: the import-type qualifier should resolve
    // their type, so a wrong-shape literal assigned at the type position
    // must still error.
    let diags = diagnostics(
        r#"
declare module "wm" {
  export class Box {
    value: number;
  }
}
type B = import("wm").Box;
const b: B = { value: "x" };
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 for a wrong-typed assignment to `import(\"wm\").Box` \
         (class member projection on an ambient module). Got: {diags:#?}"
    );
}

#[test]
fn ambient_module_member_rename_axis_not_hardcoded() {
    // The rule must be structural, not tied to specific identifier names.
    // Rename module and member; behavior must be identical.
    let diags = diagnostics(
        r#"
declare module "another-module" {
  export interface Renamed { kept: number; }
}
type R = import("another-module").Renamed;
const r: R = { kept: "wrong" };
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 with renamed module/member (`Renamed` / `kept`). \
         The fix must not depend on identifier names. Got: {diags:#?}"
    );
}

#[test]
fn ambient_module_type_alias_member_projection() {
    let diags = diagnostics(
        r#"
declare module "wm" {
  export type StrOrNum = string | number;
}
type S = import("wm").StrOrNum;
const s: S = true;
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 for `boolean` assigned to a `string | number` type \
         alias from an ambient module. Got: {diags:#?}"
    );
}

#[test]
fn cross_file_import_type_member_still_works() {
    // Control: the cross-file (file-backed module) path must keep working
    // exactly as before.
    let libs = load_lib_files(&["es5.d.ts"]);
    let diags = diagnostic_code_messages(check_multi_file_with_libs(
        &[
            (
                "module.ts",
                r#"
export interface Thing { v: string; }
"#,
            ),
            (
                "main.ts",
                r#"
type Member = import("./module").Thing;
const m: Member = { v: 99 };
"#,
            ),
        ],
        "main.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    ));
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 for cross-file import-type member assignment \
         (regression guard). Got: {diags:#?}"
    );
}

#[test]
fn ambient_module_typeof_member_value_kept() {
    // typeof `import("mod").value` for a value member of an ambient module
    // must continue to resolve to the value's type. (Distinct from the
    // type-position `import("mod").Thing` projection that this PR fixes,
    // but the typeof path is the canonical reference the fix aligns
    // against, so its behavior must remain stable.)
    let diags = diagnostics(
        r#"
declare module "wm" {
  export const value: { a: number };
  export interface Thing { v: string; }
}
type V = typeof import("wm").value;
const v: V = { a: "x" };
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "Expected TS2322 for `typeof import(\"wm\").value` value-type \
         projection (control case). Got: {diags:#?}"
    );
}

#[test]
fn ambient_module_member_missing_still_reports_ts2694() {
    // Negative: a genuinely missing member must still produce TS2694
    // (not silently resolve to `any`). The fix must not paper over real
    // missing-member errors.
    let diags = diagnostics(
        r#"
declare module "wm" {
  export interface Thing { v: string; }
}
type Missing = import("wm").DoesNotExist;
declare const m: Missing;
m;
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2694),
        "Expected TS2694 for a genuinely missing member on an ambient \
         module. Got: {diags:#?}"
    );
}
