//! String-literal module export names — TS18057 and TS1003.
//!
//! tsc centralises both codes in `checkModuleExportName`, which runs over every
//! *module export name* position: an import specifier's property name, both
//! halves of an export specifier, and the `export * as <name>` namespace name.
//! It reports via `grammarErrorOnNode`, so a file with parse diagnostics
//! suppresses it entirely.
//!
//! The function has two mutually exclusive branches, chosen **per position**:
//!
//! * `!allowStringLiteral` answers **TS1003** and is module-target independent.
//!   It means the position binds a local, which no string can name. Exactly one
//!   position takes it: an export specifier's property name when the export
//!   declaration has no module specifier.
//! * otherwise ECMAScript arbitrary module namespace names postdate the `es2015`
//!   and `es2020` output formats, so tsc answers **TS18057** on exactly those
//!   two targets and accepts them on every other.
//!
//! Because the choice is per position, one specifier can draw one of each.
//!
//! Every expectation here is pinned against the oracle (`typescript@7.0.2`,
//! `--noEmit --strict --target es2022`), including the negatives.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, check_source_with_parse_health};
use tsz_common::common::{ModuleKind, ScriptTarget};

/// TS18057.
const STRING_LITERAL_MODULE_EXPORT_NAME: u32 = 18057;

/// TS1003 — `checkModuleExportName`'s *other* branch, taken when the position
/// binds a local rather than naming a module export.
const IDENTIFIER_EXPECTED: u32 = 1003;

/// TS1110, used only as the parse error in the suppression row.
const TYPE_EXPECTED: u32 = 1110;

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
    assert!(
        codes.contains(&IDENTIFIER_EXPECTED),
        "the same position must answer TS1003 instead, got {codes:?}"
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

// ---------------------------------------------------------------------------
// `checkModuleExportName`'s first branch: `!allowStringLiteral` -> TS1003
//
// `allowStringLiteral` is false at exactly one position — an export specifier's
// property name when the export declaration has no module specifier. There the
// name binds a local, which no string can name, so the answer is TS1003 on
// EVERY module target rather than the target-gated TS18057. Every expectation
// below is pinned against `typescript@7.0.2`.
// ---------------------------------------------------------------------------

fn count_1003(module: ModuleKind, source: &str) -> usize {
    check_codes_with(module, source)
        .into_iter()
        .filter(|code| *code == IDENTIFIER_EXPECTED)
        .count()
}

#[test]
fn a_local_export_property_name_draws_ts1003_on_every_module_target() {
    // The branch is chosen by position, not by module target, so this fires
    // identically on targets that reject string export names and on those that
    // accept them.
    for module in [
        ModuleKind::ES2015,
        ModuleKind::ES2020,
        ModuleKind::ES2022,
        ModuleKind::ESNext,
        ModuleKind::CommonJS,
        ModuleKind::Preserve,
        ModuleKind::Node16,
        ModuleKind::NodeNext,
        ModuleKind::AMD,
        ModuleKind::UMD,
        ModuleKind::System,
    ] {
        assert_eq!(
            count_1003(module, r#"const q = 1; export { "q" as y };"#),
            1,
            "expected exactly one TS1003 under {module:?}"
        );
    }
}

#[test]
fn one_specifier_can_draw_both_codes_at_different_positions() {
    // The load-bearing row. Under es2015 `export { "q" as "y" }` with no module
    // specifier answers TS1003 on the property name (binds a local) and TS18057
    // on the exported name (a real module export name on a rejecting target).
    // A per-declaration implementation of either branch fails this.
    let codes = check_codes_with(ModuleKind::ES2015, r#"const q = 1; export { "q" as "y" };"#);
    assert!(
        codes.contains(&IDENTIFIER_EXPECTED) && codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "expected TS1003 and TS18057 together, got {codes:?}"
    );
}

#[test]
fn the_exported_name_half_stays_silent_on_an_accepting_target() {
    // Same source as above on esnext: only the local-binding half is wrong.
    let codes = check_codes_with(ModuleKind::ESNext, r#"const q = 1; export { "q" as "y" };"#);
    assert_eq!(
        codes.iter().filter(|c| **c == IDENTIFIER_EXPECTED).count(),
        1,
        "only the property name binds a local, got {codes:?}"
    );
    assert!(
        !codes.contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "esnext accepts a string-literal exported name, got {codes:?}"
    );
}

#[test]
fn a_module_specifier_restores_the_module_export_name_reading() {
    // With a `from` clause the property name IS a module export name, so
    // `allowStringLiteral` is true and TS1003 must not fire on any target.
    for module in [ModuleKind::ES2015, ModuleKind::ESNext, ModuleKind::CommonJS] {
        assert_eq!(
            count_1003(module, r#"export { "x" as y } from "target";"#),
            0,
            "a re-export's property name is a module export name under {module:?}"
        );
    }
}

#[test]
fn a_bare_string_specifier_without_a_module_specifier_is_not_a_property_name() {
    // `export { "q" }` parses the string as the specifier's NAME, leaving the
    // property name absent — so the false-`allowStringLiteral` position does
    // not exist and tsc reports nothing on an accepting target. Distinguishing
    // this from `export { "q" as y }` is the point.
    assert_eq!(
        count_1003(ModuleKind::ESNext, r#"const q = 1; export { "q" };"#),
        0,
        "the name half always allows a string literal"
    );
    assert_eq!(
        count_1003(ModuleKind::ES2015, r#"const q = 1; export { "q" };"#),
        0,
        "and it answers TS18057, not TS1003, on a rejecting target"
    );
    assert!(
        check_codes_with(ModuleKind::ES2015, r#"const q = 1; export { "q" };"#)
            .contains(&STRING_LITERAL_MODULE_EXPORT_NAME),
        "es2015 still rejects the exported name itself"
    );
}

#[test]
fn a_local_exported_name_written_as_a_string_never_draws_ts1003() {
    // The mirror image: `export { q as "a" }` puts the string on the name half,
    // which always allows it.
    assert_eq!(
        count_1003(ModuleKind::ESNext, r#"const q = 1; export { q as "a" };"#),
        0,
        "the exported name may be a string literal with no `from` clause"
    );
}

#[test]
fn the_type_modifier_does_not_exempt_the_property_name() {
    assert_eq!(
        count_1003(
            ModuleKind::ESNext,
            r#"type Q = 1; export { type "Q" as y };"#
        ),
        1,
        "a type-only specifier's property name binds a local just the same"
    );
}

#[test]
fn every_specifier_in_the_clause_is_checked() {
    assert_eq!(
        count_1003(
            ModuleKind::ESNext,
            r#"const a = 1, b = 2; export { "a" as x, "b" as y };"#
        ),
        2,
        "the walk covers all specifiers, not just the first"
    );
}

#[test]
fn ts1003_does_not_depend_on_the_names_involved() {
    // Anti-hardcoding: the rule is structural. Vary both the binder and the
    // string, including a string that does not name any local at all — tsc
    // skips its local-resolution check for exactly this shape, so TS1003 is
    // the only diagnostic and no resolution error joins it.
    for (local, quoted) in [
        ("q", "q"),
        ("someOtherBinder", "someOtherBinder"),
        ("Zzz", "Zzz"),
        ("q", "nothingNamedThis"),
    ] {
        let source = format!(r#"const {local} = 1; export {{ "{quoted}" as renamed }};"#);
        assert_eq!(
            count_1003(ModuleKind::ESNext, &source),
            1,
            "expected exactly one TS1003 for {source:?}"
        );
    }
}

#[test]
fn a_parse_error_suppresses_ts1003_too() {
    // Both branches report through `grammarErrorOnNode`, so a file with parse
    // diagnostics suppresses the whole check. Oracle answers TS1110 alone for
    // this source.
    //
    // This needs `check_source_with_parse_health`: the plain helpers leave
    // `has_parse_errors` at its `false` default, so a test written on them
    // cannot observe the suppression at all.
    let (parser_codes, checker_codes) =
        check_source_with_parse_health(r#"const q = 1; export { "q" as y }; let z: = 1;"#);
    assert!(
        parser_codes.contains(&TYPE_EXPECTED),
        "the source must actually produce the parse error, got {parser_codes:?}"
    );
    assert!(
        !checker_codes.contains(&IDENTIFIER_EXPECTED),
        "a parse error suppresses this grammar check, got {checker_codes:?}"
    );
}

#[test]
fn without_the_parse_error_the_same_source_reports_ts1003() {
    // The control for the row above: same file, parse error removed.
    let (parser_codes, checker_codes) =
        check_source_with_parse_health(r#"const q = 1; export { "q" as y }; let z = 1;"#);
    assert!(
        parser_codes.is_empty(),
        "control must parse cleanly, got {parser_codes:?}"
    );
    assert!(
        checker_codes.contains(&IDENTIFIER_EXPECTED),
        "expected TS1003 once nothing suppresses it, got {checker_codes:?}"
    );
}

#[test]
fn plain_identifier_specifiers_never_draw_ts1003() {
    for source in [
        r#"const q = 1; export { q };"#,
        r#"const q = 1; export { q as r };"#,
        r#"export { x as y } from "target";"#,
        r#"export * as ns from "target";"#,
    ] {
        for module in [ModuleKind::ES2015, ModuleKind::ESNext] {
            assert_eq!(
                count_1003(module, source),
                0,
                "TS1003 must not fire for {source:?} under {module:?}"
            );
        }
    }
}
