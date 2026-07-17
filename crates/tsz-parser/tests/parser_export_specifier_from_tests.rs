//! Tests that the contextual keyword `from` is accepted as an *exported name*
//! inside export specifier braces, matching `tsc`.
//!
//! Regression for the parser treating the first `from` in
//! `export { from } from "./mod";` as the start of the from-clause instead of
//! an exported identifier, which produced bogus `TS1005`/`TS1141`/`TS1434`.
//!
//! The disambiguation mirrors the import side: a `from` token is only the
//! from-clause keyword when the following token cannot continue a specifier
//! name (i.e. it is not `as`, `,`, or `}`). Otherwise `from` is a name.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::{assert_no_errors, parse_source};

/// Every valid form where `from` (and other contextual keywords) appears as an
/// exported name must parse with zero diagnostics, exactly like `tsc`.
#[test]
fn from_as_exported_name_parses_clean() {
    let cases = [
        // The canonical repro from rxjs `dist/types/index.d.ts`.
        r#"export { from } from "./internal/observable/from";"#,
        // Local re-export (no module specifier) of a binding named `from`.
        r#"const from = 1; export { from };"#,
        // `from` alongside ordinary names, in any position.
        r#"export { a, from } from "./mod";"#,
        r#"export { from, a } from "./mod";"#,
        r#"export { a, from, b } from "./mod";"#,
        // `from` renamed via `as`.
        r#"export { from as origin } from "./mod";"#,
        // `from` used as the *alias* target.
        r#"export { origin as from } from "./mod";"#,
        // String-literal module export name renamed to `from`.
        r#"export { "x" as from } from "./mod";"#,
        // Type-only re-export with `from` as the name.
        r#"export type { from } from "./mod";"#,
        // `from` together with a type-only modifier on the same name.
        r#"export { type from } from "./mod";"#,
        // Trailing comma must not change the disambiguation.
        r#"export { from, } from "./mod";"#,
    ];
    for source in cases {
        assert_no_errors(source);
    }
}

/// The fix must not regress the import side, which already handled `from` as a
/// specifier name. Kept here so import/export parity stays locked together.
#[test]
fn from_as_imported_name_parses_clean() {
    let cases = [
        r#"import { from } from "./mod";"#,
        r#"import { from as origin } from "./mod";"#,
        r#"import { a, from } from "./mod";"#,
        r#"import { type from } from "./mod";"#,
    ];
    for source in cases {
        assert_no_errors(source);
    }
}

/// `from` parsed as an exported name must actually land in the AST as an export
/// specifier (not be silently dropped), and the declaration must still capture
/// the trailing from-clause module specifier.
#[test]
fn from_specifier_lands_in_ast_with_module_specifier() {
    let source = r#"export { from } from "./internal/observable/from";"#;
    let (parser, root) = parse_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected clean parse, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("source file node");
    let source_file = arena.get_source_file(root_node).expect("source file data");

    // Walk: source file -> export declaration -> named exports -> `from` specifier.
    let export_decl = source_file
        .statements
        .nodes
        .iter()
        .filter_map(|&stmt| arena.get(stmt))
        .find(|node| node.kind == syntax_kind_ext::EXPORT_DECLARATION)
        .and_then(|node| arena.get_export_decl(node))
        .expect("an export declaration");

    assert!(
        export_decl.module_specifier.is_some(),
        "the trailing from-clause module specifier should be captured"
    );

    let clause_node = arena
        .get(export_decl.export_clause)
        .expect("export clause node");
    let named_exports = arena
        .get_named_imports(clause_node)
        .expect("named exports data");
    let names: Vec<&str> = named_exports
        .elements
        .nodes
        .iter()
        .filter_map(|&spec| arena.get(spec))
        .filter_map(|node| arena.get_specifier(node))
        .filter_map(|spec| arena.get(spec.name))
        .map(|name_node| &source[name_node.pos as usize..name_node.end as usize])
        .collect();
    assert_eq!(
        names,
        vec!["from"],
        "`from` should parse as the single export specifier name"
    );
}

/// A genuinely malformed re-export where `from` is followed directly by the
/// module string (missing both the name list payload and the closing brace)
/// must still terminate the specifier list — `from` only acts as a name when
/// the following token can continue one.
#[test]
fn bare_from_string_still_terminates_specifier_list() {
    // `export { from "./mod" }` — `from` is followed by a string literal, which
    // cannot continue a specifier name, so it is treated as the from-clause and
    // the input is reported as malformed rather than silently accepted.
    let source = r#"export { from "./mod";"#;
    let (parser, _root) = parse_source(source);
    assert!(
        !parser.get_diagnostics().is_empty(),
        "malformed `export {{ from \"./mod\"` should still report diagnostics"
    );
}

/// `export * from <identifier>` — the module specifier after `from` must be a
/// string literal. tsc reports TS1141 "String literal expected." for a
/// non-string specifier (even inside a `namespace`, where tsz previously
/// accepted the identifier silently and let the checker report TS1194 instead).
/// Vary the specifier name to keep the check structural.
#[test]
fn export_star_from_non_string_specifier_reports_ts1141() {
    for source in [
        "namespace Ns { export * from Target; }",
        "namespace Outer { export * from Origin; }",
    ] {
        let (parser, _root) = parse_source(source);
        let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&1141),
            "expected TS1141 for `{source}`: {:?}",
            parser.get_diagnostics()
        );
    }
}
