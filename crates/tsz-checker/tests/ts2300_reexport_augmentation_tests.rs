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

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn check_for_dup(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|d| matches!(d.code, 2300 | 2451))
    .map(|d| (d.code, d.message_text))
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
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.is_empty(),
        "`export {{ User as MyUser }}` + interface augmentation must not produce TS2300/2451; got: {errs:?}"
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
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.is_empty(),
        "`export {{ Config as PublicConfig }}` + interface augmentation must not produce TS2300/2451; got: {errs:?}"
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
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.is_empty(),
        "`export {{ Widget }}` + interface augmentation must not produce TS2300/2451; got: {errs:?}"
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
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.is_empty(),
        "`export {{ Request }}` + interface augmentation must not produce TS2300/2451; got: {errs:?}"
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
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.is_empty(),
        "`export type {{ Item }}` + interface augmentation must not produce TS2300/2451; got: {errs:?}"
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
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.is_empty(),
        "`export type {{ Entity as PublicEntity }}` + interface augmentation must not produce TS2300/2451; got: {errs:?}"
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
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.is_empty(),
        "`import {{ User }}` + interface augmentation must not produce TS2300/2451; got: {errs:?}"
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
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.is_empty(),
        "`import {{ Response }}` + interface augmentation must not produce TS2300/2451; got: {errs:?}"
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
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.is_empty(),
        "`import type {{ Product }}` + interface augmentation must not produce TS2300/2451; got: {errs:?}"
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
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.is_empty(),
        "`import type {{ Service }}` + interface augmentation must not produce TS2300/2451; got: {errs:?}"
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
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.is_empty(),
        "Multiple re-exports + augmentations must not produce TS2300/2451; got: {errs:?}"
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
    let errs = check_for_dup(&[("source.ts", source), ("test.ts", test)], "test.ts");
    assert!(
        errs.is_empty(),
        "Multiple `import type` + augmentations must not produce TS2300/2451; got: {errs:?}"
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
        errs.iter().any(|(code, _)| *code == 2451 || *code == 2300),
        "const-vs-const augmentation conflict must still produce an error; got: {errs:?}"
    );
}
