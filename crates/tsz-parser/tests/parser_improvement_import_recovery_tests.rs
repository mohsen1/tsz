//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — import recovery.

use crate::parser::test_fixture::parse_source;
use crate::parser::{NodeIndex, ParserState, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_typeof_import_with_member_access() {
    // typeof import("...").A.foo should parse without TS1005
    // This is a valid TypeScript syntax for accessing static members
    let source = r#"
export const foo: typeof import("./a").A.foo;
"#;
    let (parser, _root) = parse_source(source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();

    // Should not emit TS1005 for member access after import()
    assert!(
        !codes.contains(&1005),
        "Expected no TS1005 errors for typeof import with member access, got {codes:?}",
    );

    // The typeof-import type parses cleanly; the declaration is a `const` with no
    // initializer, so the one expected diagnostic is TS1155 ("'const' declarations
    // must be initialized.") — matching tsc for `export const foo: T;`.
    assert_eq!(
        codes,
        vec![1155],
        "Expected only the uninitialized-const TS1155, got {codes:?}",
    );
}

#[test]
fn test_typeof_import_with_nested_member_access() {
    // typeof import("...").A.B.C should parse correctly
    let source = r#"
export const foo: typeof import("./module").A.B.C;
"#;
    let (parser, _root) = parse_source(source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();

    // The nested member-access type parses cleanly; the only expected diagnostic
    // is the uninitialized-`const` TS1155 (the declaration has no initializer).
    assert_eq!(
        codes,
        vec![1155],
        "Expected only the uninitialized-const TS1155 for nested member access, got {codes:?}",
    );
}

#[test]
fn test_typeof_import_without_member_access() {
    // typeof import("...") without member access should still work
    let source = r#"
export const foo: typeof import("./module");
"#;
    let (parser, _root) = parse_source(source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();

    // The bare `typeof import(...)` type parses cleanly; the only expected
    // diagnostic is the uninitialized-`const` TS1155.
    assert_eq!(
        codes,
        vec![1155],
        "Expected only the uninitialized-const TS1155, got {codes:?}",
    );
}

#[test]
fn test_typeof_import_non_string_literal_reports_ts1141() {
    let source = r#"
type ImportByKey<K extends string> = typeof import(K);
type MappedImport<T extends string[]> = {
    [K in T[number]]: typeof import(K);
};
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1141_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::STRING_LITERAL_EXPECTED)
        .count();
    assert_eq!(
        ts1141_count, 2,
        "Expected TS1141 for both typeof import(K) type queries, got {diagnostics:?}",
    );
}

#[test]
fn test_import_type_without_typeof() {
    // import("...").Type should parse without typeof
    let source = r#"
export const a: import("./test1").T = null as any;
"#;
    let (parser, _root) = parse_source(source);

    // Should not emit parse errors
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    let ts1109_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1109)
        .count();
    let ts1359_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1359)
        .count();

    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for import type, got {ts1005_count}",
    );
    assert_eq!(
        ts1109_count, 0,
        "Expected no TS1109 errors for import type, got {ts1109_count}",
    );
    assert_eq!(
        ts1359_count, 0,
        "Expected no TS1359 errors for import type, got {ts1359_count}",
    );
}

#[test]
fn test_import_type_with_member_access() {
    // import("...").Type.SubType should parse correctly
    let source = r#"
export const a: import("./test1").T.U = null as any;
"#;
    let (parser, _root) = parse_source(source);

    // Should not emit parse errors
    assert!(
        parser.get_diagnostics().iter().all(|d| d.code >= 2000),
        "Expected no parser errors (1xxx) for import type with member access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_import_type_with_generic_arguments() {
    // import("...").Type<T> should parse correctly
    let source = r#"
export const a: import("./test1").T<typeof import("./test2").theme> = null as any;
"#;
    let (parser, _root) = parse_source(source);

    // Should not emit parse errors
    let parse_errors = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .count();
    assert_eq!(
        parse_errors,
        0,
        "Expected no parser errors for import type with generics, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_import_type_with_invalid_import_attribute_key_reports_ts1478() {
    let source = r#"
const a = (null as any as import("pkg", { with: {1234, "resolution-mode": "require"} }).RequireInterface);
"#;
    let (parser, _root) = parse_source(source);

    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_OR_STRING_LITERAL_EXPECTED),
        "Expected TS1478 for invalid import-attribute key, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected tail recovery to surface TS1128 diagnostics, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        codes.contains(&diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER),
        "Expected tail recovery to surface TS1434 diagnostics, got {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    let source_file = arena.get_source_file_at(_root).unwrap();
    assert!(
        source_file.statements.nodes.iter().any(|&stmt| {
            arena
                .get(stmt)
                .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        }),
        "invalid import-attribute entries should recover as statement tails"
    );
}

#[test]
fn test_typeof_import_defer_reports_missing_parens_in_type_query() {
    let source = r#"
export type X = typeof import.defer("./a").Foo;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPECTED)
        .map(|d| d.message.as_str())
        .collect();

    assert!(
        ts1005_messages.iter().any(|m| m.contains("'(' expected.")),
        "Expected TS1005 '(' expected for typeof import.defer, got {diagnostics:?}",
    );
    assert!(
        ts1005_messages.iter().any(|m| m.contains("')' expected.")),
        "Expected TS1005 ')' expected for typeof import.defer, got {diagnostics:?}",
    );
}

#[test]
fn test_import_attributes_double_comma_recovers_with_missing_brace_and_ts1128() {
    let source = r#"
export type Test3 = typeof import("./a.json", {
  with: {
    type: "json"
  },,
});
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message.contains("'}' expected.")),
        "Expected TS1005 '}}' expected recovery for malformed import attributes, got {diagnostics:?}",
    );

    let ts1128_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED)
        .count();
    assert!(
        ts1128_count >= 2,
        "Expected at least two TS1128 diagnostics in tail recovery, got {diagnostics:?}",
    );
}

#[test]
fn test_import_attributes_nested_double_comma_reports_ts1478_without_ts1128_tail() {
    let source = r#"
export type Test4 = typeof import("./a.json", {
  with: {
    type: "json",,
  }
});
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::IDENTIFIER_OR_STRING_LITERAL_EXPECTED),
        "Expected TS1478 for malformed nested import-attribute key, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected no TS1128 tail cascade for nested comma invalid-key recovery, got {diagnostics:?}",
    );
}

#[test]
fn test_import_type_options_array_recovery_in_intersection_reports_semicolon_and_ts1128() {
    let source = r#"
export type LocalInterface =
    & import("pkg", [ {"resolution-mode": "require"} ]).RequireInterface
    & import("pkg", [ {"resolution-mode": "import"} ]).ImportInterface;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message.contains("'{' expected.")),
        "Expected TS1005 '{{' expected for array import options recovery, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message.contains("';' expected.")),
        "Expected TS1005 ';' expected for array import options recovery in intersections, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected TS1128 statement-tail recovery for array import options in intersections, got {diagnostics:?}",
    );
}

#[test]
fn test_import_type_options_identifier_recovery_reports_ts1134() {
    let source = r#"
type Attribute1 = { with: {"resolution-mode": "require"} };
export const a = (null as any as import("pkg", Attribute1).RequireInterface);
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message.contains("'{' expected.")),
        "Expected TS1005 '{{' expected for indirected import options, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::VARIABLE_DECLARATION_EXPECTED),
        "Expected TS1134 for indirected import options recovery, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message.contains("',' expected.")),
        "Expected TS1005 ',' expected for indirected import options recovery, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected no TS1128 tail cascade for indirected import options recovery, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER),
        "Expected no TS1434 tail cascade for indirected import options recovery, got {diagnostics:?}",
    );
}

#[test]
fn test_import_type_options_array_recovery_in_cast_reports_trailing_comma_without_ts1128_tail() {
    let source = r#"
export const a = (null as any as import("pkg", [ {"resolution-mode": "require"} ]).RequireInterface);
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message.contains("'{' expected.")),
        "Expected TS1005 '{{' expected for array import options in casts, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message.contains("',' expected.")),
        "Expected TS1005 ',' expected at outer ')' for array import options in casts, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected no TS1128 tail cascade for array import options in casts, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER),
        "Expected no TS1434 tail cascade for array import options in casts, got {diagnostics:?}",
    );
}

#[test]
fn test_import_type_options_identifier_recovery_in_intersection_reports_ts1128_without_comma() {
    let source = r#"
export type LocalInterface =
    & import("pkg", Attribute1).RequireInterface;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message.contains("'{' expected.")),
        "Expected TS1005 '{{' expected for identifier import options in intersections, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED)
            .count()
            >= 1,
        "Expected TS1128 statement-tail recovery for identifier import options in intersections, got {diagnostics:?}",
    );
}

#[test]
fn test_import_defer_namespace_parses_clean() {
    // `import defer * as ns from "mod"` is valid — no parse errors
    let source = r#"import defer * as ns from "./a";"#;
    let (parser, _root) = parse_source(source);

    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors for valid defer namespace import, got {parse_errors:?}",
    );
}

#[test]
fn test_import_defer_as_binding_name() {
    // `import defer from "mod"` — defer is the default import NAME, not a modifier
    let source = r#"import defer from "./a";"#;
    let (parser, _root) = parse_source(source);

    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors when 'defer' is used as binding name, got {parse_errors:?}",
    );
}

#[test]
fn test_import_dot_defer_call_no_parse_error() {
    // `import.defer("./a")` — valid dynamic defer import, no parse error
    let source = r#"import.defer("./a.js");"#;
    let (parser, _root) = parse_source(source);

    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors for import.defer() call, got {parse_errors:?}",
    );
}

#[test]
fn test_import_dot_defer_standalone_emits_ts1005() {
    // `import.defer` without () should emit TS1005 "'(' expected."
    let source = r"const x = import.defer;";
    let (parser, _root) = parse_source(source);

    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    assert_eq!(
        ts1005_count, 1,
        "Expected 1 TS1005 for standalone import.defer, got {ts1005_count}",
    );
}

#[test]
fn test_import_dot_defer_invalid_standalone_fingerprints_match_tsc() {
    let function_arg_source = "Function(import.defer);";
    let (function_arg_parser, _root) = parse_source(function_arg_source);
    assert!(
        function_arg_parser
            .get_diagnostics()
            .iter()
            .any(|d| d.code == 1005 && d.message == "'(' expected."),
        "Expected Function(import.defer) to report missing call paren; diagnostics: {:?}",
        function_arg_parser.get_diagnostics()
    );

    let source =
        "import.defer;\n\n(import.defer)(\"a\");\n\nFunction(import.defer);\n\nimport.defer";
    let (parser, _root) = parse_source(source);

    let ts1005: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005 && d.message == "'(' expected.")
        .map(|d| d.start)
        .collect();
    let expected = vec![
        source.find(';').expect("first semicolon") as u32,
        source.find(")(\"a\")").expect("parenthesized close paren") as u32,
        source.find(");\n\nimport.defer").expect("call close paren") as u32,
        source.len() as u32,
    ];
    assert_eq!(
        ts1005,
        expected,
        "Expected import.defer without call parens to report at each missing '(' anchor; diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_import_dot_invalid_meta_property_ts17012() {
    // `import.foo` (not in call) should emit TS17012
    let source = r"const x = import.foo;";
    let (parser, _root) = parse_source(source);

    let ts17012_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 17012)
        .count();
    assert_eq!(
        ts17012_count, 1,
        "Expected 1 TS17012 for invalid import.foo, got {ts17012_count}",
    );
}

#[test]
fn test_import_dot_invalid_meta_property_call_ts18061() {
    // `import.foo()` (in call) should emit TS18061
    let source = r#"import.foo("./a");"#;
    let (parser, _root) = parse_source(source);

    let ts18061_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 18061)
        .count();
    assert_eq!(
        ts18061_count, 1,
        "Expected 1 TS18061 for import.foo() call, got {ts18061_count}",
    );
}

#[test]
fn test_import_defer_with_default_sets_deferred_flag() {
    // `import defer foo from "./a"` — defer is modifier, foo is default name
    // Parser should set is_deferred = true
    let source = r#"import defer foo from "./a";"#;
    let (parser, root) = parse_source(source);

    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt = sf.statements.nodes[0];
    let stmt_node = arena.get(stmt).unwrap();
    let import = arena.get_import_decl(stmt_node).unwrap();
    let clause_node = arena.get(import.import_clause).unwrap();
    let clause = arena.get_import_clause(clause_node).unwrap();
    assert!(
        clause.is_deferred,
        "Expected is_deferred to be true for 'import defer foo from'"
    );
    assert!(
        clause.name.is_some(),
        "Expected default import name to be present"
    );
}

#[test]
fn test_import_defer_from_as_name_not_deferred() {
    // `import defer from "./a"` — defer is the import NAME, not modifier
    // Parser should NOT set is_deferred = true
    let source = r#"import defer from "./a";"#;
    let (parser, root) = parse_source(source);

    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt = sf.statements.nodes[0];
    let stmt_node = arena.get(stmt).unwrap();
    let import = arena.get_import_decl(stmt_node).unwrap();
    let clause_node = arena.get(import.import_clause).unwrap();
    let clause = arena.get_import_clause(clause_node).unwrap();
    assert!(
        !clause.is_deferred,
        "Expected is_deferred to be false for 'import defer from' (defer is name)"
    );
}

#[test]
fn test_import_defer_type_modifier_conflict_anchors_from_at_namespace_token() {
    // `import defer type * as ns from "./a"` is illegal (defer + type modifier
    // conflict) but tsc still parses it as: `defer` modifier, `type` as the
    // default-import name (contextual keyword), then expects `from`. The
    // resulting `'from' expected` diagnostic anchors at the `*` (column 19),
    // not at the `type` keyword (column 14) or with an incorrect `'='
    // expected` from the import-equals lookahead path.
    let source = r#"import defer type * as ns from "./a";"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005: Vec<_> = diagnostics.iter().filter(|d| d.code == 1005).collect();
    assert!(
        !ts1005.is_empty(),
        "Expected at least one TS1005 for `import defer type *`, got {diagnostics:?}"
    );
    // No `'=' expected.` (would be the import-equals lookahead misroute).
    assert!(
        !ts1005.iter().any(|d| d.message.contains("'=' expected")),
        "Should not emit `'=' expected.` for `import defer type *`: {ts1005:?}"
    );
    // The `'from' expected.` should anchor at column 19 (the `*`), 0-indexed
    // start = 18.
    let from_expected: Vec<_> = ts1005
        .iter()
        .filter(|d| d.message.contains("'from' expected"))
        .collect();
    assert!(
        from_expected.iter().any(|d| d.start == 18),
        "Expected `'from' expected.` anchored at column 19 (start=18), got {from_expected:?}"
    );
}

#[test]
fn test_import_defer_from_equals_routes_to_import_declaration() {
    // `import defer from = require("m")` — `defer` has no import-equals form,
    // so the lookahead must route this to import-declaration. tsc parses it as
    // `defer` modifier + `from` binding name, then expects the `from` keyword
    // and finds `=` at column 19 (start=18). The lookahead must NOT route to
    // import-equals (which would emit `'=' expected.` at column 14 plus a
    // trailing `';' expected.`).
    let source = r#"import defer from = require("m");"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005: Vec<_> = diagnostics.iter().filter(|d| d.code == 1005).collect();
    assert!(
        !ts1005.iter().any(|d| d.message.contains("'=' expected")),
        "Should not emit `'=' expected.` for `import defer from = require(...)`: {ts1005:?}"
    );
    assert!(
        !ts1005.iter().any(|d| d.message.contains("';' expected")),
        "Should not emit `';' expected.` for `import defer from = require(...)`: {ts1005:?}"
    );
    let from_expected: Vec<_> = ts1005
        .iter()
        .filter(|d| d.message.contains("'from' expected"))
        .collect();
    assert!(
        from_expected.iter().any(|d| d.start == 18),
        "Expected `'from' expected.` anchored at column 19 (start=18), got {from_expected:?} (all ts1005: {ts1005:?})"
    );
}

#[test]
fn test_import_type_from_equals_still_routes_to_import_equals() {
    // Regression for the sibling case: `import type from = require("m")` IS
    // valid type-only import-equals (with `from` as the binding name). The
    // narrow `defer` fix must not regress the `type` branch, so the parser
    // should accept this without parser-level recovery diagnostics.
    let source = r#"import type from = require("m");"#;
    let (parser, _root) = parse_source(source);

    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors for `import type from = require(...)`, got {parse_errors:?}"
    );
}

// =============================================================================
// Import attribute line-break handling
// =============================================================================

// Helpers shared across the ASI tests below.

fn parse_first_import_has_attributes(source: &str) -> bool {
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    arena
        .get_import_decl(stmt_node)
        .is_some_and(|i| i.attributes.is_some())
}

fn parse_first_export_has_attributes(source: &str) -> bool {
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    arena
        .get_export_decl(stmt_node)
        .is_some_and(|e| e.attributes.is_some())
}

fn parse_first_import_attributes_multi_line(source: &str) -> bool {
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    let import = arena.get_import_decl(stmt_node).unwrap();
    let attr_node = arena
        .get(import.attributes)
        .expect("should have attributes");
    arena
        .get_import_attributes_data(attr_node)
        .expect("should have attributes data")
        .multi_line
}

#[test]
fn test_import_attributes_same_line_extensionless_parsed() {
    // `import './foo' with { type: 'json' }` — extensionless path, same line:
    // 'with' is on the same line as the module specifier, so it IS import attributes.
    let (parser, root) = parse_source(r#"import './foo' with { type: 'json' };"#);
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    assert!(
        arena
            .get_import_decl(stmt_node)
            .is_some_and(|i| i.attributes.is_some()),
        "extensionless same-line 'with' must be parsed as import attributes"
    );
    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors for extensionless with-attributes, got {parse_errors:?}"
    );
}

#[test]
fn test_import_attributes_newline_extensionless_parsed_as_attributes() {
    // Static import declarations parse `with` attributes even when `with` starts
    // on the next line after the module specifier.
    assert!(
        parse_first_import_has_attributes("import './foo'\nwith { type: 'json' };"),
        "newline before 'with' must still be parsed as import attributes"
    );
}

#[test]
fn test_import_attributes_same_line_with_extension_parsed() {
    // Same-line attributes work with extension-bearing paths too.
    assert!(
        parse_first_import_has_attributes(r#"import './data.json' with { type: 'json' };"#),
        "extension-bearing same-line 'with' must be parsed as import attributes"
    );
}

#[test]
fn test_import_attributes_newline_with_extension_parsed_as_attributes() {
    assert!(
        parse_first_import_has_attributes("import './data.json'\nwith { type: 'json' };"),
        "newline before 'with' must still be parsed as import attributes (extension-bearing path)"
    );
}

#[test]
fn test_import_attributes_default_import_newline_from_and_with_parsed() {
    let source = "import data\n  from './data.json'\n  with { type: 'json' };";
    let (parser, root) = parse_source(source);
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();

    assert!(
        arena
            .get_import_decl(stmt_node)
            .is_some_and(|i| i.attributes.is_some()),
        "default import with multiline module specifier and 'with' must parse attributes"
    );
    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors for multiline import attributes, got {parse_errors:?}"
    );
}

#[test]
fn test_import_assert_same_line_parsed() {
    // 'assert' clause (deprecated TS 4.5–5.2 syntax) on the same line is still parsed.
    assert!(
        parse_first_import_has_attributes(r#"import './data.json' assert { type: 'json' };"#),
        "same-line 'assert' clause must be parsed as import attributes"
    );
}

#[test]
fn test_import_assert_newline_not_parsed_as_attributes() {
    // Legacy `assert` still has the no-line-break guard.
    assert!(
        !parse_first_import_has_attributes("import './data.json'\nassert { type: 'json' };"),
        "newline before 'assert' must not be parsed as import attributes"
    );
}

#[test]
fn test_export_attributes_same_line_parsed() {
    // Export declarations also support 'with' attributes on the same line.
    let (parser, root) = parse_source(r#"export * from './foo' with { type: 'json' };"#);
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    assert!(
        arena
            .get_export_decl(stmt_node)
            .is_some_and(|e| e.attributes.is_some()),
        "same-line 'with' on export must be parsed as import attributes"
    );
    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors for export with-attributes, got {parse_errors:?}"
    );
}

#[test]
fn test_export_attributes_newline_not_parsed_as_attributes() {
    // 'with' on a new line after export's module specifier = new statement (ASI).
    assert!(
        !parse_first_export_has_attributes("export * from './foo'\nwith { type: 'json' };"),
        "newline before 'with' on export must not be parsed as import attributes"
    );
}

#[test]
fn test_import_attributes_multi_line_sets_multi_line_flag() {
    // When the attribute block spans multiple lines, `multi_line` should be true.
    assert!(
        parse_first_import_attributes_multi_line("import './foo' with {\n  type: 'json'\n};"),
        "multi_line must be true when attribute block spans multiple lines"
    );
}

#[test]
fn test_import_attributes_single_line_clears_multi_line_flag() {
    // When attributes are on one line, `multi_line` should be false.
    assert!(
        !parse_first_import_attributes_multi_line(r#"import './foo' with { type: 'json' };"#),
        "multi_line must be false for single-line attribute block"
    );
}

#[test]
fn test_import_attributes_named_import_extensionless_same_line() {
    // Named import with extensionless path and same-line attributes.
    assert!(
        parse_first_import_has_attributes(
            r#"import { foo } from './module' with { type: 'json' };"#
        ),
        "named import with extensionless path: same-line 'with' must be attributes"
    );
}

#[test]
fn test_import_attributes_named_import_extensionless_newline() {
    assert!(
        parse_first_import_has_attributes("import { foo } from './module'\nwith { type: 'json' };"),
        "named import with extensionless path: newline 'with' must be attributes"
    );
}

// =============================================================================
// Helpers for the new tests below.
// =============================================================================

/// Parse `source`, assert no parser-level errors (code < 2000), and return
/// the `(ParserState, NodeIndex)` pair for further structural assertions.
fn parse_clean(source: &str, label: &str) -> (ParserState, NodeIndex) {
    let (parser, root) = parse_source(source);
    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "{label}: unexpected parse errors {parse_errors:?}"
    );
    (parser, root)
}

/// Assert that `source` parses cleanly AND the first statement's import
/// declaration carries import attributes.
fn assert_import_has_attributes_cleanly(source: &str, label: &str) {
    let (parser, root) = parse_clean(source, label);
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    assert!(
        arena
            .get_import_decl(stmt_node)
            .is_some_and(|i| i.attributes.is_some()),
        "{label}: import declaration must carry attributes"
    );
}

/// Assert that `source` parses cleanly AND the first statement's export
/// declaration carries import attributes.
fn assert_export_has_attributes_cleanly(source: &str, label: &str) {
    let (parser, root) = parse_clean(source, label);
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    assert!(
        arena
            .get_export_decl(stmt_node)
            .is_some_and(|e| e.attributes.is_some()),
        "{label}: export declaration must carry attributes"
    );
}

/// Assert that `source` produces no parser-level errors (code < 2000).
fn assert_parses_cleanly(source: &str, label: &str) {
    parse_clean(source, label);
}

// =============================================================================
// Type-only import with attributes — comprehensive coverage
// =============================================================================

#[test]
fn test_import_type_only_named_with_attributes_same_line() {
    assert_import_has_attributes_cleanly(
        r#"import type { Foo } from './module' with { type: 'json' };"#,
        "type-only named import, same-line 'with'",
    );
}

#[test]
fn test_import_type_only_named_with_attributes_newline() {
    // `with` on next line after module specifier is still import attributes for
    // `import type` (same rule as bare `import`).
    assert!(
        parse_first_import_has_attributes(
            "import type { Foo } from './module'\nwith { type: 'json' };"
        ),
        "type-only named import: newline before 'with' must still be attributes"
    );
}

#[test]
fn test_import_type_only_namespace_with_attributes_same_line() {
    assert_import_has_attributes_cleanly(
        r#"import type * as ns from './module' with { type: 'json' };"#,
        "type-only namespace import, same-line 'with'",
    );
}

#[test]
fn test_import_type_only_namespace_with_attributes_newline() {
    assert!(
        parse_first_import_has_attributes(
            "import type * as ns from './module'\nwith { type: 'json' };"
        ),
        "type-only namespace import: newline before 'with' must still be attributes"
    );
}

#[test]
fn test_import_namespace_with_attributes_same_line() {
    assert_import_has_attributes_cleanly(
        r#"import * as ns from './module' with { type: 'json' };"#,
        "namespace import, same-line 'with'",
    );
}

#[test]
fn test_import_type_only_named_alias_with_attributes() {
    // Named import with alias: `{ Foo as Bar }` should carry attributes identically
    // to `{ Foo }`.
    assert_import_has_attributes_cleanly(
        r#"import type { Foo as Bar } from './module' with { type: 'json' };"#,
        "type-only aliased import",
    );
}

#[test]
fn test_import_type_only_multiple_named_with_attributes() {
    assert_import_has_attributes_cleanly(
        r#"import type { Foo, Bar, Baz } from './module' with { type: 'json' };"#,
        "multi-named type-only import",
    );
}

// =============================================================================
// Re-export with attributes
// =============================================================================

#[test]
fn test_export_type_named_with_attributes_same_line() {
    assert_export_has_attributes_cleanly(
        r#"export type { Foo } from './module' with { type: 'json' };"#,
        "type-only named re-export",
    );
}

#[test]
fn test_export_type_named_with_attributes_newline_not_parsed() {
    // Exports do NOT allow newline before `with` (unlike imports).
    assert!(
        !parse_first_export_has_attributes(
            "export type { Foo } from './module'\nwith { type: 'json' };"
        ),
        "type-only re-export: newline before 'with' must NOT be parsed as attributes"
    );
}

#[test]
fn test_export_star_namespace_with_attributes_same_line() {
    // `export * as ns from './foo' with { type: 'json' }` — namespace re-export.
    assert_export_has_attributes_cleanly(
        r#"export * as ns from './foo' with { type: 'json' };"#,
        "namespace re-export",
    );
}

// =============================================================================
// import() type expressions with valid options AND dotted member access
// =============================================================================

#[test]
fn test_import_type_expr_with_valid_options() {
    // Each case is a distinct surface: `with` options, legacy `assert` options,
    // multi-dotted member access, intersection context, and bare type annotation.
    let cases: &[(&str, &str)] = &[
        (
            r#"const a = (null as any as import("pkg", { with: { "resolution-mode": "require" } }).RequireInterface);"#,
            "import type expr: 'with' options + single member (cast context)",
        ),
        (
            r#"const a = (null as any as import("pkg", { assert: { type: "json" } }).RequireInterface);"#,
            "import type expr: legacy 'assert' options + member (cast context)",
        ),
        (
            r#"const a = (null as any as import("pkg", { with: { "resolution-mode": "require" } }).A.B.C);"#,
            "import type expr: 'with' options + multi-dotted member (cast context)",
        ),
        (
            "export type LocalInterface =\n    & import(\"pkg\", { with: { \"resolution-mode\": \"require\" } }).RequireInterface\n    & import(\"pkg\", { with: { \"resolution-mode\": \"import\" } }).ImportInterface;",
            "import type expr: 'with' options in intersection",
        ),
        (
            r#"type T = import("pkg", { with: { "resolution-mode": "require" } }).A;"#,
            "import type expr: 'with' options in type annotation",
        ),
    ];
    for (source, label) in cases {
        assert_parses_cleanly(source, label);
    }
}

// =============================================================================
// Extensionless path variants
// =============================================================================

#[test]
fn test_import_type_only_extensionless_path_with_attributes() {
    assert_import_has_attributes_cleanly(
        r#"import type { Foo } from './extensionless' with { type: 'json' };"#,
        "type-only import, extensionless path",
    );
}

#[test]
fn test_import_namespace_extensionless_path_with_attributes() {
    assert_import_has_attributes_cleanly(
        r#"import * as ns from './extensionless' with { type: 'json' };"#,
        "namespace import, extensionless path",
    );
}

#[test]
fn test_import_type_only_extensionless_newline_with_attributes() {
    assert!(
        parse_first_import_has_attributes(
            "import type { Foo } from './extensionless'\nwith { type: 'json' };"
        ),
        "type-only import, extensionless path: newline 'with' must still be attributes"
    );
}

// =============================================================================
// assert (legacy) vs with for type-only imports
// =============================================================================

#[test]
fn test_import_type_only_assert_same_line_parsed() {
    // `import type { X } from './mod' assert { type: 'json' }` — same-line assert,
    // type-only. Still valid attribute syntax (deprecated but parseable).
    assert!(
        parse_first_import_has_attributes(
            r#"import type { X } from './mod' assert { type: 'json' };"#
        ),
        "type-only import: same-line 'assert' must be parsed as import attributes"
    );
}

#[test]
fn test_import_type_only_assert_newline_not_attributes() {
    // Newline before `assert` — NOT import attributes, even for type-only imports.
    assert!(
        !parse_first_import_has_attributes(
            "import type { X } from './mod'\nassert { type: 'json' };"
        ),
        "type-only import: newline before 'assert' must not be parsed as attributes"
    );
}

#[test]
fn test_new_dot_targ_meta_property_ts17012() {
    // `new.targ` (misspelled `new.target`) should emit TS17012
    let source = "function f() { return new.targ; }";
    let (parser, _root) = parse_source(source);

    let ts17012_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_DID_YOU_MEAN
        })
        .count();
    assert_eq!(
        ts17012_count,
        1,
        "Expected 1 TS17012 for new.targ, got {ts17012_count}. All diagnostics: {:?}",
        parser.get_diagnostics(),
    );
}

#[test]
fn test_new_dot_target_no_ts17012() {
    // `new.target` (correctly spelled) must not emit TS17012
    let source = "function f() { return new.target; }";
    let (parser, _root) = parse_source(source);

    let ts17012_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_DID_YOU_MEAN
        })
        .count();
    assert_eq!(
        ts17012_count,
        0,
        "Expected no TS17012 for valid new.target, got {ts17012_count}. All diagnostics: {:?}",
        parser.get_diagnostics(),
    );
}
