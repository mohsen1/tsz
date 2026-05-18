//! TS2300 false positive: re-export or import alias + module augmentation.
//!
//! Structural rule: When a file has `export { X [as Y] } from "M"` (re-export) or
//! `import { X } from "M"` (import alias), and also `declare module "M" { interface X {} }`
//! (augmentation of M), the re-export/import is NOT a local declaration of X — it
//! references M's export. When M already exports `interface X`, the augmentation just adds
//! members. tsc accepts all these patterns without TS2300.
//!
//! Adjacent cases covered:
//! - `export { X as Y }` re-export + interface augmentation (exact issue #6052 repro)
//! - `export { X }` same-name re-export + interface augmentation
//! - `export type { X }` type-only re-export + interface augmentation
//! - `import { X }` import alias + interface augmentation
//! - `import type { X }` type-only import alias + interface augmentation
//! - multiple imports/re-exports + multiple augmentations
//! - renamed bindings (two names per case, proving the fix is structural not name-keyed)
//! - negative: genuine const-vs-const conflict still produces TS2451
//! - negative: cross-module mismatch (export from ./a + augment ./b) still errors
//! - negative: non-mergeable augmentation (const) still errors

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn check_diags(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn check_for_dup(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
    check_diags(files, entry_file)
        .into_iter()
        .filter(|(code, _)| matches!(code, 2300 | 2451))
        .collect()
}

// ─── re-export with rename ────────────────────────────────────────────────────

/// Exact issue #6052 repro: `export { User as MyUser } from "./source"` + interface augmentation.
#[test]
fn reexport_rename_plus_interface_augmentation_no_ts2300() {
    let source = "export interface User { id: number; }\n";
    let test = r#"export { User as MyUser } from "./source";
declare module "./source" {
    interface User { email?: string; }
}
"#;
    let diags = check_diags(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        diags.is_empty(),
        "`export {{ User as MyUser }}` + interface augmentation must produce no diagnostics; got: {diags:?}"
    );
}

/// Same fix, different binding name — proves the rule is structural, not name-keyed.
#[test]
fn reexport_rename_alternate_binding_plus_interface_augmentation_no_ts2300() {
    let source = "export interface Config { port: number; }\n";
    let test = r#"export { Config as PublicConfig } from "./source";
declare module "./source" {
    interface Config { host?: string; }
}
"#;
    let diags = check_diags(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        diags.is_empty(),
        "`export {{ Config as PublicConfig }}` + interface augmentation must produce no diagnostics; got: {diags:?}"
    );
}

// ─── same-name re-export ──────────────────────────────────────────────────────

#[test]
fn reexport_same_name_plus_interface_augmentation_no_ts2300() {
    let source = "export interface Widget { id: number; }\n";
    let test = r#"export { Widget } from "./source";
declare module "./source" {
    interface Widget { label?: string; }
}
"#;
    let diags = check_diags(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        diags.is_empty(),
        "`export {{ Widget }}` + interface augmentation must produce no diagnostics; got: {diags:?}"
    );
}

#[test]
fn reexport_same_name_alternate_binding_plus_interface_augmentation_no_ts2300() {
    let source = "export interface Request { url: string; }\n";
    let test = r#"export { Request } from "./source";
declare module "./source" {
    interface Request { headers?: string; }
}
"#;
    let diags = check_diags(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        diags.is_empty(),
        "`export {{ Request }}` + interface augmentation must produce no diagnostics; got: {diags:?}"
    );
}

// ─── type-only re-export ──────────────────────────────────────────────────────

#[test]
fn type_only_reexport_plus_interface_augmentation_no_ts2300() {
    let source = "export interface Item { id: number; }\n";
    let test = r#"export type { Item } from "./source";
declare module "./source" {
    interface Item { name?: string; }
}
"#;
    let diags = check_diags(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        diags.is_empty(),
        "`export type {{ Item }}` + interface augmentation must produce no diagnostics; got: {diags:?}"
    );
}

#[test]
fn type_only_reexport_with_rename_plus_interface_augmentation_no_ts2300() {
    let source = "export interface Entity { id: number; }\n";
    let test = r#"export type { Entity as PublicEntity } from "./source";
declare module "./source" {
    interface Entity { name?: string; }
}
"#;
    let diags = check_diags(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        diags.is_empty(),
        "`export type {{ Entity as PublicEntity }}` + interface augmentation must produce no diagnostics; got: {diags:?}"
    );
}

// ─── import alias ─────────────────────────────────────────────────────────────

#[test]
fn import_alias_plus_interface_augmentation_no_ts2300() {
    let source = "export interface User { id: number; }\n";
    let test = r#"import { User } from "./source";
declare module "./source" {
    interface User { email?: string; }
}
"#;
    let diags = check_diags(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        diags.is_empty(),
        "`import {{ User }}` + interface augmentation must produce no diagnostics; got: {diags:?}"
    );
}

/// Same fix, different binding name.
#[test]
fn import_alias_alternate_binding_plus_interface_augmentation_no_ts2300() {
    let source = "export interface Response { status: number; }\n";
    let test = r#"import { Response } from "./source";
declare module "./source" {
    interface Response { body?: string; }
}
"#;
    let diags = check_diags(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        diags.is_empty(),
        "`import {{ Response }}` + interface augmentation must produce no diagnostics; got: {diags:?}"
    );
}

// ─── type-only import alias ───────────────────────────────────────────────────

#[test]
fn type_only_import_alias_plus_interface_augmentation_no_ts2300() {
    let source = "export interface Product { sku: string; }\n";
    let test = r#"import type { Product } from "./source";
declare module "./source" {
    interface Product { price?: number; }
}
"#;
    let diags = check_diags(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        diags.is_empty(),
        "`import type {{ Product }}` + interface augmentation must produce no diagnostics; got: {diags:?}"
    );
}

#[test]
fn type_only_import_alias_alternate_binding_plus_interface_augmentation_no_ts2300() {
    let source = "export interface Service { name: string; }\n";
    let test = r#"import type { Service } from "./source";
declare module "./source" {
    interface Service { version?: string; }
}
"#;
    let diags = check_diags(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        diags.is_empty(),
        "`import type {{ Service }}` + interface augmentation must produce no diagnostics; got: {diags:?}"
    );
}

// ─── multiple bindings + multiple augmentations ───────────────────────────────

#[test]
fn multiple_reexports_and_augmentations_no_ts2300() {
    let source = r#"export interface Alpha { x: number; }
export interface Beta { y: string; }
"#;
    let test = r#"export { Alpha, Beta } from "./source";
declare module "./source" {
    interface Alpha { z?: boolean; }
    interface Beta { w?: boolean; }
}
"#;
    let diags = check_diags(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        diags.is_empty(),
        "Multiple re-exports + augmentations must produce no diagnostics; got: {diags:?}"
    );
}

#[test]
fn multiple_imports_and_augmentations_no_ts2300() {
    let source = r#"export interface Alpha { x: number; }
export interface Beta { y: string; }
"#;
    let test = r#"import type { Alpha, Beta } from "./source";
declare module "./source" {
    interface Alpha { z?: boolean; }
    interface Beta { w?: boolean; }
}
"#;
    let diags = check_diags(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        diags.is_empty(),
        "Multiple `import type` + augmentations must produce no diagnostics; got: {diags:?}"
    );
}

// ─── negative: genuine const-vs-const conflict still emits ───────────────────

/// A `const` declaration in the augmenting file that conflicts with a `const` in the
/// target still produces TS2451 (block-scoped redeclaration).  The interface-merge
/// suppression must not silence this.
#[test]
fn const_vs_const_augmentation_still_errors() {
    let source = "export const value = 0;\n";
    let test = r#"export {}; declare module "./source" { export const value: number; }
"#;
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.iter().any(|(code, _)| *code == 2451),
        "const-vs-const augmentation conflict must produce TS2451; got: {errs:?}"
    );
}

// ─── negative: cross-module mismatch must still emit ─────────────────────────

/// Re-export from ./a while augmenting ./b (a different module) with an interface of
/// the same name.  The augmentation does NOT cover the from-clause module, so the
/// suppression must NOT fire.
#[test]
fn reexport_from_a_augment_b_interface_still_errors() {
    let source_a = "export interface User { id: number; }\n";
    let source_b = "export interface User { name: string; }\n";
    // Exports from ./a but augments ./b — mismatched from-clause.
    let test = r#"export { User } from "./source_a";
declare module "./source_b" {
    interface User { email?: string; }
}
"#;
    let errs = check_for_dup(
        &[
            ("source_a.ts", source_a),
            ("source_b.ts", source_b),
            ("test.ts", test),
        ],
        "test.ts",
    );
    assert!(
        errs.iter().any(|(code, _)| *code == 2300 || *code == 2451),
        "re-export from ./a + augment ./b must still produce TS2300/2451; got: {errs:?}"
    );
}

/// Re-export from ./a while augmenting ./a with a `const` (non-mergeable).
/// The from-clause matches, but the declaration is not an interface or function, so
/// the suppression must NOT fire.
#[test]
fn reexport_from_a_augment_a_with_const_still_errors() {
    let source_a = "export const counter = 0;\n";
    let test = r#"export { counter } from "./source_a";
declare module "./source_a" {
    const counter: number;
}
"#;
    let errs = check_for_dup(&[("source_a.ts", source_a), ("test.ts", test)], "test.ts");
    assert!(
        errs.iter().any(|(code, _)| *code == 2451 || *code == 2300),
        "re-export from ./a + augment ./a with const must still produce an error; got: {errs:?}"
    );
}
