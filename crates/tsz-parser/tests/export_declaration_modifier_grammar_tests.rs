//! TS1193 (`An export declaration cannot have modifiers.`) — a plain export
//! declaration (`export { }`, `export * from`, type-only `export type { }` /
//! `export type *`) preceded by a `declare` modifier.
//!
//! Every expectation below is pinned against the vendored `typescript@7.0.2`
//! oracle (`--noEmit --strict --target es2022 --module esnext --lib es2022`).
//! TS1193 was previously unwired: `state_declarations.rs`'s `declare export`
//! arm had no match for `OpenBraceToken` / `AsteriskToken` / a type-only
//! `TypeKeyword`, so all four shapes fell through to a generic
//! `error_declaration_expected` (TS1146) instead.

use crate::parser::test_fixture::parse_source;

fn codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

fn first_diag_start(source: &str, code: u32) -> u32 {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("expected TS{code} for {source:?}"))
        .start
}

#[test]
fn declare_export_named_emits_ts1193_at_declare() {
    // oracle: TS1193 at (1,1), TS2304 for `x` — no TS1029.
    let source = "declare export { x };";
    let cs = codes(source);
    assert!(
        cs.contains(&1193),
        "expected TS1193 for {source:?}, got {cs:?}"
    );
    assert!(
        !cs.contains(&1029),
        "TS1029 must not accompany TS1193 for a plain export declaration, got {cs:?}"
    );
    assert_eq!(first_diag_start(source, 1193), 0);
}

#[test]
fn declare_export_named_renamed_binder() {
    // Same shape, different exported name — the rule is positional, not
    // keyed to the binder's spelling.
    let source = "declare export { qux$_0 };";
    let cs = codes(source);
    assert!(
        cs.contains(&1193),
        "expected TS1193 for {source:?}, got {cs:?}"
    );
}

#[test]
fn declare_export_star_emits_ts1193() {
    let source = r#"declare export * from "mod";"#;
    let cs = codes(source);
    assert!(
        cs.contains(&1193),
        "expected TS1193 for {source:?}, got {cs:?}"
    );
    assert!(!cs.contains(&1029), "got {cs:?}");
    assert_eq!(first_diag_start(source, 1193), 0);
}

#[test]
fn declare_export_named_with_module_specifier_emits_ts1193() {
    let source = r#"declare export { x } from "mod";"#;
    let cs = codes(source);
    assert!(
        cs.contains(&1193),
        "expected TS1193 for {source:?}, got {cs:?}"
    );
    assert!(!cs.contains(&1029), "got {cs:?}");
}

#[test]
fn declare_export_type_only_named_emits_ts1193() {
    // `declare export type { x }` is a type-only *export declaration*, not
    // the type-alias form `declare export type X = Y` — the parser must
    // disambiguate via lookahead the same way the non-ambient path does.
    let source = "declare export type { x };";
    let cs = codes(source);
    assert!(
        cs.contains(&1193),
        "expected TS1193 for {source:?}, got {cs:?}"
    );
    assert!(!cs.contains(&1029), "got {cs:?}");
}

#[test]
fn declare_export_type_only_star_emits_ts1193() {
    let source = r#"declare export type * as ns from "mod";"#;
    let cs = codes(source);
    assert!(
        cs.contains(&1193),
        "expected TS1193 for {source:?}, got {cs:?}"
    );
    assert!(!cs.contains(&1029), "got {cs:?}");
}

#[test]
fn declare_export_type_alias_does_not_emit_ts1193() {
    // Negative / adjacent: `declare export type X = ...` is a genuine type
    // alias declaration (tsc: TS1029 only, `declare` after `export`), not a
    // plain export declaration — must not draw TS1193.
    let source = "declare export type X = number;";
    let cs = codes(source);
    assert!(
        !cs.contains(&1193),
        "must not emit TS1193 for {source:?}, got {cs:?}"
    );
    assert!(
        cs.contains(&1029),
        "expected TS1029 for {source:?}, got {cs:?}"
    );
}

#[test]
fn plain_export_named_stays_clean() {
    // Negative: no `declare` modifier at all.
    assert_eq!(codes("export { x };"), Vec::<u32>::new());
}

#[test]
fn plain_export_empty_stays_clean() {
    assert_eq!(codes("export {};"), Vec::<u32>::new());
}

#[test]
fn declare_export_class_still_reports_ts1029_not_ts1193() {
    // Adjacent/negative: a `declare export`-prefixed declaration that is NOT
    // a plain export declaration (class here) keeps its pre-existing TS1029
    // behavior and must not regress to TS1193.
    let source = "declare export class C {}";
    let cs = codes(source);
    assert!(
        cs.contains(&1029),
        "expected TS1029 for {source:?}, got {cs:?}"
    );
    assert!(
        !cs.contains(&1193),
        "must not emit TS1193 for {source:?}, got {cs:?}"
    );
}

#[test]
fn export_declare_export_named_emits_ts1193_at_first_export() {
    // `export declare export { x };` — TS1029 already fired for the
    // out-of-order `export`/`declare` pair (existing behavior, unrelated to
    // this fix); TS1193 still fires for the inner plain export declaration,
    // anchored at the *first* modifier (the outer `export`).
    let source = "export declare export { x };";
    let cs = codes(source);
    assert!(
        cs.contains(&1193),
        "expected TS1193 for {source:?}, got {cs:?}"
    );
    assert_eq!(first_diag_start(source, 1193), 0);
}

#[test]
fn nested_declare_export_in_ambient_namespace_does_not_emit_ts1193() {
    // Known adjacent gap, not a regression: inside an already-ambient body
    // (`declare namespace N { ... }`), tsc reports TS1038 ("declare modifier
    // cannot be used in an already ambient context") for a redundant nested
    // `declare`, never TS1193 — the same precedence the pre-existing TS1029
    // check already follows for this position. `ExportDeclData` carries no
    // modifiers field for the checker's TS1038 pass to read, so this shape
    // stays silently-clean rather than regressing to a wrong TS1193.
    let source = "declare namespace N {\n  declare export { x };\n}\n";
    let cs = codes(source);
    assert!(
        !cs.contains(&1193),
        "must not emit TS1193 inside an already-ambient body, got {cs:?}"
    );
}

#[test]
fn export_in_ambient_namespace_without_declare_stays_clean() {
    // Adjacent negative: an export declaration inside `declare namespace`
    // that does NOT itself carry a `declare` modifier is unaffected.
    let source = "declare namespace N {\n  export { x };\n}\n";
    assert_eq!(codes(source), Vec::<u32>::new());
}
