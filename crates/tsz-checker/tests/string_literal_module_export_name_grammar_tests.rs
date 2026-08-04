//! String-literal module export names — TS18057.
//!
//! ECMAScript arbitrary module namespace names (`export { x as "str name" }`)
//! postdate the `es2015` and `es2020` module output formats, so tsc rejects
//! them when `module` is exactly one of those two and accepts them on every
//! other module target.
//!
//! tsc centralises this in `checkModuleExportName`, which runs over every
//! *module export name* position: an import specifier's property name, both
//! halves of an export specifier, and the `export * as <name>` namespace name.
//! It reports via `grammarErrorOnNode`, so a file with parse diagnostics
//! suppresses it entirely.
//!
//! Every expectation here is pinned against the oracle (`typescript@7.0.2`,
//! `--noEmit --strict --target es2022`), including the negatives.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;
use tsz_common::common::{ModuleKind, ScriptTarget};

/// TS18057.
const STRING_LITERAL_MODULE_EXPORT_NAME: u32 = 18057;

fn options_for(module: ModuleKind) -> CheckerOptions {
    CheckerOptions {
        module,
        target: ScriptTarget::ES2022,
        ..CheckerOptions::default()
    }
}

fn check_codes_with(module: ModuleKind, source: &str) -> Vec<u32> {
    check_source(source, "test.ts", options_for(module))
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn count_18057(module: ModuleKind, source: &str) -> usize {
    check_codes_with(module, source)
        .into_iter()
        .filter(|code| *code == STRING_LITERAL_MODULE_EXPORT_NAME)
        .count()
}

/// A resolvable target for the import-side rows. tsc only walks an import
/// declaration's specifiers when its module specifier resolves, so these tests
/// need a module that actually exists.
const AMBIENT_TARGET: &str = r#"
declare module "target" {
    export const x: number;
}
"#;

fn with_target(source: &str) -> String {
    format!("{AMBIENT_TARGET}{source}")
}

// ---------------------------------------------------------------------------
// The module-target matrix — the whole point of the code
// ---------------------------------------------------------------------------

#[test]
fn es2015_rejects_a_string_literal_export_name() {
    let codes = check_codes_with(
        ModuleKind::ES2015,
        r#"export { x as "str name" } from "target";"#,
    );
    assert!(
        codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "expected TS18057 under --module es2015, got {codes:?}"
    );
}

#[test]
fn es2020_rejects_a_string_literal_export_name() {
    let codes = check_codes_with(
        ModuleKind::ES2020,
        r#"export { x as "str name" } from "target";"#,
    );
    assert!(
        codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "expected TS18057 under --module es2020, got {codes:?}"
    );
}

#[test]
fn newer_and_older_module_targets_accept_a_string_literal_export_name() {
    // es2022 and everything after it postdate the restriction; commonjs,
    // preserve and the node family were never subject to it.
    for module in [
        ModuleKind::ES2022,
        ModuleKind::ESNext,
        ModuleKind::CommonJS,
        ModuleKind::Preserve,
        ModuleKind::Node16,
        ModuleKind::Node18,
        ModuleKind::NodeNext,
        ModuleKind::AMD,
        ModuleKind::UMD,
        ModuleKind::System,
    ] {
        let codes = check_codes_with(module, r#"export { x as "str name" } from "target";"#);
        assert!(
            !codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
            "TS18057 must not fire under {module:?}, got {codes:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Every module export name position
// ---------------------------------------------------------------------------

#[test]
fn export_specifier_property_name_is_a_module_export_name() {
    let codes = check_codes_with(ModuleKind::ES2015, r#"export { "x" as y } from "target";"#);
    assert!(
        codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "expected TS18057 on the re-exported source name, got {codes:?}"
    );
}

#[test]
fn export_specifier_without_a_rename_is_a_module_export_name() {
    assert_eq!(
        count_18057(ModuleKind::ES2015, r#"export { "x" } from "target";"#),
        1,
        "`export {{ \"x\" }}` names one position, so it draws exactly one TS18057"
    );
}

#[test]
fn both_halves_of_an_export_specifier_are_checked() {
    assert_eq!(
        count_18057(
            ModuleKind::ES2015,
            r#"export { "x" as "y" } from "target";"#
        ),
        2,
        "property name and name are both module export names"
    );
}

#[test]
fn a_local_export_without_a_module_specifier_still_checks_the_exported_name() {
    // The exported side may be a string literal even with no `from` clause;
    // only the local side must be an identifier.
    let codes = check_codes_with(
        ModuleKind::ES2015,
        r#"const q = 1; export { q as "str name" };"#,
    );
    assert!(
        codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "expected TS18057 on a local export's string-literal name, got {codes:?}"
    );
}

#[test]
fn namespace_export_name_is_a_module_export_name() {
    let codes = check_codes_with(ModuleKind::ES2015, r#"export * as "ns" from "target";"#);
    assert!(
        codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "expected TS18057 on an `export * as \"ns\"` name, got {codes:?}"
    );
}

#[test]
fn an_import_from_an_unresolvable_augmentation_target_is_suppressed() {
    // A single file that both declares `module "target"` and imports from it
    // makes the declaration an *augmentation* of a module that does not exist,
    // so the specifier never resolves and the import-side walk is skipped.
    // Oracle (`--module es2015`) answers TS2664 + the module-not-found code and
    // no TS18057; this pins that tsz agrees rather than over-reporting.
    let codes = check_codes_with(
        ModuleKind::ES2015,
        &with_target(r#"import { "x" as y } from "target"; y;"#),
    );
    assert!(
        !codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "an augmentation of a missing module does not resolve, so TS18057 must not fire, got {codes:?}"
    );
}

#[test]
fn a_type_only_specifier_is_still_a_module_export_name() {
    assert_eq!(
        count_18057(
            ModuleKind::ES2015,
            r#"export { x as "a", type T as "b" } from "target";"#
        ),
        2,
        "the `type` modifier does not exempt a specifier from the check"
    );
}

// ---------------------------------------------------------------------------
// Negatives — identifier names are always fine
// ---------------------------------------------------------------------------

#[test]
fn identifier_names_never_draw_the_diagnostic() {
    for source in [
        r#"export { x as y } from "target";"#,
        r#"export { x } from "target";"#,
        r#"export * as ns from "target";"#,
        r#"export * from "target";"#,
        r#"const q = 1; export { q as r };"#,
    ] {
        let codes = check_codes_with(ModuleKind::ES2015, source);
        assert!(
            !codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
            "TS18057 must not fire for {source:?}, got {codes:?}"
        );
    }
}

#[test]
fn a_local_export_property_name_is_not_a_module_export_name() {
    // Without a `from` clause the property name names a *local* binding, which
    // no string can name. tsc's `allowStringLiteral` is false there, so it
    // answers TS1003 and never TS18057.
    let codes = check_codes_with(ModuleKind::ES2015, r#"const q = 1; export { "q" as y };"#);
    assert!(
        !codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "a local export's property name is not a module export name, got {codes:?}"
    );
}

#[test]
fn a_module_declaration_name_is_not_a_module_export_name() {
    let codes = check_codes_with(ModuleKind::ES2015, r#"declare module "m2" { }"#);
    assert!(
        !codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "a module declaration's own name is not a module export name, got {codes:?}"
    );
}

#[test]
fn a_module_specifier_is_not_a_module_export_name() {
    // The `from "target"` string is a module specifier, not an export name.
    let codes = check_codes_with(ModuleKind::ES2015, r#"export { x as y } from "target";"#);
    assert!(
        !codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "the module specifier must not be treated as an export name, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// The import/export asymmetry on an unresolved module — oracle-confirmed
// ---------------------------------------------------------------------------

#[test]
fn an_unresolved_module_suppresses_the_import_side() {
    // tsc's `checkImportDeclaration` only walks its specifiers when the module
    // specifier resolves, so this answers the module-not-found code alone.
    let codes = check_codes_with(
        ModuleKind::ES2015,
        r#"import { "x" as y } from "./nope"; y;"#,
    );
    assert!(
        !codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "TS18057 must not fire on an import from an unresolved module, got {codes:?}"
    );
}

#[test]
fn an_unresolved_module_does_not_suppress_the_export_side() {
    // The export paths have no such gate.
    let codes = check_codes_with(ModuleKind::ES2015, r#"export { x as "a" } from "./nope";"#);
    assert!(
        codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "TS18057 must still fire on an export from an unresolved module, got {codes:?}"
    );
}

#[test]
fn an_unresolved_module_does_not_suppress_a_namespace_export() {
    let codes = check_codes_with(ModuleKind::ES2015, r#"export * as "ns" from "./nope";"#);
    assert!(
        codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "TS18057 must still fire on `export * as \"ns\"` from an unresolved module, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Grammar-diagnostic suppression — tsc's `grammarErrorOnNode` gate
// ---------------------------------------------------------------------------

#[test]
fn a_parse_error_suppresses_the_diagnostic() {
    // `import { "x" as "y" }` names a local binding with a string literal,
    // which is a parse error (TS1003). tsc reports that alone.
    let codes = check_codes_with(
        ModuleKind::ES2015,
        &with_target(r#"import { "x" as "y" } from "target";"#),
    );
    assert!(
        !codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "a file with parse diagnostics suppresses this grammar check, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Binder-name independence — the rule is structural, not name-driven
// ---------------------------------------------------------------------------

#[test]
fn the_check_does_not_depend_on_the_names_involved() {
    for (local, exported) in [
        ("x", "str name"),
        ("someOtherBinder", "a-b-c"),
        ("Zzz", "default"),
        ("q", ""),
    ] {
        let source = format!(r#"export {{ {local} as "{exported}" }} from "target";"#);
        let codes = check_codes_with(ModuleKind::ES2015, &source);
        assert!(
            codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
            "TS18057 must fire regardless of the names in {source:?}, got {codes:?}"
        );
    }
}
