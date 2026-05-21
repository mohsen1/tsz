//! Cross-module unqualified name resolution.
//!
//! Structural rule: a top-level declaration of an external module that is not a
//! value-bearing export, module/namespace declaration, UMD export, or global
//! augmentation (i.e. a pure type-only `export type`/`export interface`) is not
//! visible in a sibling module's global scope. An unqualified reference to it
//! from a file that never imported the name is `TS2304`. Script files (no
//! top-level import/export) still contribute their top-level declarations to the
//! ambient global scope and remain cross-file visible.
//!
//! These cases vary identifier and file-name spellings so the fix is keyed on
//! module/declaration structure, not on any particular name (CLAUDE.md §25/§26).

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

const TS2304_CANNOT_FIND_NAME: u32 = 2304;

fn check_module_files(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn has_cannot_find_name(diagnostics: &[(u32, String)], name: &str) -> bool {
    diagnostics
        .iter()
        .any(|(code, msg)| *code == TS2304_CANNOT_FIND_NAME && msg.contains(name))
}

#[test]
fn unimported_type_alias_from_sibling_module_reports_ts2304() {
    let helpers = "export type Internal = { a: number; b: string };\n";
    let consumer = "export type UseInternal = Internal;\n";

    let diags = check_module_files(
        &[("Helpers.ts", helpers), ("Consumer.ts", consumer)],
        "Consumer.ts",
    );

    assert!(
        has_cannot_find_name(&diags, "Internal"),
        "type-only export must not leak into a sibling module's global scope; \
         expected TS2304 for 'Internal'. Got: {diags:?}"
    );
}

#[test]
fn unimported_interface_from_sibling_module_reports_ts2304() {
    let helpers = "export interface Shape { width: number }\n";
    let consumer = "export type UseShape = Shape;\n";

    let diags = check_module_files(
        &[("shapes.ts", helpers), ("uses-shape.ts", consumer)],
        "uses-shape.ts",
    );

    assert!(
        has_cannot_find_name(&diags, "Shape"),
        "interface export is type-only and must not leak across modules; \
         expected TS2304 for 'Shape'. Got: {diags:?}"
    );
}

#[test]
fn unimported_type_renamed_identifiers_still_reports_ts2304() {
    // Same shape as the first case but with different identifier and file-name
    // spellings — proves the rule is structural, not keyed on a name.
    let lib = "export type Registry<K> = { [P in keyof K]: P };\n";
    let app = "export type Project<C> = Registry<C>;\n";

    let diags = check_module_files(&[("reg.ts", lib), ("app.ts", app)], "app.ts");

    assert!(
        has_cannot_find_name(&diags, "Registry"),
        "renamed type-only export must still be unresolved across modules; \
         expected TS2304 for 'Registry'. Got: {diags:?}"
    );
}

#[test]
fn imported_type_alias_from_sibling_module_resolves() {
    // Negative case: with an explicit import the name resolves and there is no
    // TS2304 — proves the fix only restricts the *unimported* path.
    let helpers = "export type Internal = { a: number };\n";
    let consumer = "import { Internal } from \"./Helpers\";\nexport type UseInternal = Internal;\n";

    let diags = check_module_files(
        &[("Helpers.ts", helpers), ("Consumer.ts", consumer)],
        "Consumer.ts",
    );

    assert!(
        !has_cannot_find_name(&diags, "Internal"),
        "an imported type alias must resolve; expected no TS2304 for 'Internal'. \
         Got: {diags:?}"
    );
}

#[test]
fn script_file_top_level_type_remains_globally_visible() {
    // Negative case: files with no top-level import/export are scripts, so their
    // top-level declarations populate the ambient global scope and stay
    // cross-file visible. The fix must not over-filter these.
    let globals = "type AmbientShape = { id: number };\n";
    let consumer = "type UseAmbient = AmbientShape;\n";

    let diags = check_module_files(
        &[("globals.ts", globals), ("consumer.ts", consumer)],
        "consumer.ts",
    );

    assert!(
        !has_cannot_find_name(&diags, "AmbientShape"),
        "script-file top-level types are global and must resolve cross-file; \
         expected no TS2304 for 'AmbientShape'. Got: {diags:?}"
    );
}

#[test]
fn module_augmentation_body_resolves_target_reexport_for_self_reference() {
    // Regression guard for closing the global file-locals leak: names inside a
    // relative module augmentation body resolve against the augmented module's
    // export surface, not a sibling module's accidental globals.
    let base = "export interface Widget { value: string }\n";
    let barrel = "export * from \"./base\";\n";
    let augment = r#"
export {};
declare module "./barrel" {
    interface Widget {
        child: Widget;
    }
}
"#;

    let diags = check_module_files(
        &[
            ("base.ts", base),
            ("barrel.ts", barrel),
            ("augment.ts", augment),
        ],
        "augment.ts",
    );

    assert!(
        !has_cannot_find_name(&diags, "Widget"),
        "module augmentation body should resolve `Widget` through the augmented \
         module export surface; got: {diags:?}"
    );
}
