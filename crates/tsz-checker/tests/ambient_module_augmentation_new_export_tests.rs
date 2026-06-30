//! Regression coverage for #14853: a module augmentation that adds a NEW export
//! to an ambient `declare module "x"` (declared in a .d.ts) must type that new
//! export from its declaration, not collapse it to `any`.
//!
//! Structural rule: when an augmentation in file B adds an exported value symbol
//! (`const`/`function`/`class`/`enum`) to an ambient module declared in file A,
//! importing that symbol in file C must resolve its declared type even though the
//! augmentation declaration lives in a foreign arena relative to the file being
//! checked. Previously the cross-arena merge bailed to `TypeId::ANY`, silently
//! dropping every assignability error against the new export.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;

fn diagnostics(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn has_code(diags: &[(u32, String)], expected: u32) -> bool {
    diags.iter().any(|(code, _)| *code == expected)
}

const BASE: &str = r#"declare module "lib" { export const original: number; }"#;

#[test]
fn augmentation_added_const_keeps_declared_type() {
    let diags = diagnostics(
        &[
            ("base.d.ts", BASE),
            (
                "aug.ts",
                "declare module \"lib\" { export const addedConst: string; }\nexport {};\n",
            ),
            (
                "main.ts",
                "import { addedConst } from \"lib\";\nconst c: { z: 1 } = addedConst;\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        has_code(&diags, 2322),
        "expected TS2322 for string-typed augmentation const; got {diags:#?}"
    );
}

#[test]
fn augmentation_added_function_keeps_declared_type() {
    let diags = diagnostics(
        &[
            ("base.d.ts", BASE),
            (
                "aug.ts",
                "declare module \"lib\" { export function addedFn(): boolean; }\nexport {};\n",
            ),
            (
                "main.ts",
                "import { addedFn } from \"lib\";\nconst c: string = addedFn();\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        has_code(&diags, 2322),
        "expected TS2322 for boolean return of augmentation function; got {diags:#?}"
    );
}

#[test]
fn augmentation_added_class_resolves_constructor_type() {
    // Same-file augmentation: the fix resolves the class through its declared
    // symbol, yielding the constructor type so `new C()` is constructable and the
    // instance member keeps its declared type. (The cross-file class-constructor
    // case is verified end-to-end via the CLI; the entry-only unit harness lacks
    // the global symbol index needed to resolve a foreign class constructor.)
    let diags = diagnostics(
        &[
            ("base.d.ts", BASE),
            (
                "main.ts",
                "declare module \"lib\" { export class AddedCls { x: number; } }\nimport { AddedCls } from \"lib\";\nconst inst = new AddedCls();\nconst c: string = inst.x;\n",
            ),
        ],
        "main.ts",
    );
    // Constructable (no TS2351) and the instance member keeps its declared type.
    assert!(
        !has_code(&diags, 2351),
        "augmentation class must be constructable; got {diags:#?}"
    );
    assert!(
        has_code(&diags, 2322),
        "expected TS2322 for number instance member assigned to string; got {diags:#?}"
    );
}

#[test]
fn augmentation_added_enum_resolves_object_type() {
    let diags = diagnostics(
        &[
            ("base.d.ts", BASE),
            (
                "aug.ts",
                "declare module \"lib\" { export enum AddedEnum { A, B } }\nexport {};\n",
            ),
            (
                "main.ts",
                "import { AddedEnum } from \"lib\";\nconst c: string = AddedEnum;\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        has_code(&diags, 2322),
        "expected TS2322 assigning enum object to string; got {diags:#?}"
    );
}

#[test]
fn augmentation_added_const_clean_assignment_is_not_flagged() {
    // The declared type must be preserved precisely: a correct assignment must
    // not spuriously error (guards against over-widening to error/never).
    let diags = diagnostics(
        &[
            ("base.d.ts", BASE),
            (
                "aug.ts",
                "declare module \"lib\" { export const addedConst: string; }\nexport {};\n",
            ),
            (
                "main.ts",
                "import { addedConst } from \"lib\";\nconst ok: string = addedConst;\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        !has_code(&diags, 2322),
        "correct string-to-string assignment should be clean; got {diags:#?}"
    );
}

#[test]
fn augmentation_added_export_is_not_name_specific() {
    // Anti-hardcoding: the fix must be structural, not keyed on a fixture name.
    let diags = diagnostics(
        &[
            (
                "base.d.ts",
                r#"declare module "widgets" { export const seed: number; }"#,
            ),
            (
                "extra.ts",
                "declare module \"widgets\" { export const quux: string; }\nexport {};\n",
            ),
            (
                "use.ts",
                "import { quux } from \"widgets\";\nconst c: { nope: 1 } = quux;\n",
            ),
        ],
        "use.ts",
    );
    assert!(
        has_code(&diags, 2322),
        "expected TS2322 regardless of binder/module names; got {diags:#?}"
    );
}

#[test]
fn augmentation_added_const_same_file_keeps_declared_type() {
    // Same-arena augmentation (augmentation + usage in one file) must also keep
    // the declared type rather than collapse to `any`.
    let diags = diagnostics(
        &[
            ("base.d.ts", BASE),
            (
                "main.ts",
                "declare module \"lib\" { export const localAdded: string; }\nimport { localAdded } from \"lib\";\nconst c: { z: 1 } = localAdded;\n",
            ),
        ],
        "main.ts",
    );
    assert!(
        has_code(&diags, 2322),
        "expected TS2322 for same-file augmentation const; got {diags:#?}"
    );
}

#[test]
fn augmentation_merging_existing_interface_member_still_works() {
    // Regression guard: augmenting an EXISTING exported interface with a new
    // member must keep working (this never went through the new-export path).
    let diags = diagnostics(
        &[
            (
                "base.d.ts",
                r#"declare module "lib" { export interface Opts { a: number; } }"#,
            ),
            (
                "aug.ts",
                "declare module \"lib\" { interface Opts { b: string; } }\nexport {};\n",
            ),
            (
                "main.ts",
                "import { Opts } from \"lib\";\nconst o: Opts = { a: 1, b: 2 };\n",
            ),
        ],
        "main.ts",
    );
    // `b` is `string`, assigning `2` must error; merge must not regress to clean.
    assert!(
        has_code(&diags, 2322) || has_code(&diags, 2353) || has_code(&diags, 2739),
        "expected a member-type error from merged interface augmentation; got {diags:#?}"
    );
}
