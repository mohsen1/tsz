//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — import recovery.

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_typeof_import_with_member_access() {
    // typeof import("...").A.foo should parse without TS1005
    // This is a valid TypeScript syntax for accessing static members
    let source = r#"
export const foo: typeof import("./a").A.foo;
"#;
    let (parser, _root) = parse_source(source);

    // Should not emit TS1005 for member access after import()
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for typeof import with member access, got {ts1005_count}",
    );

    // Should have no errors at all
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for typeof import with member access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_typeof_import_with_nested_member_access() {
    // typeof import("...").A.B.C should parse correctly
    let source = r#"
export const foo: typeof import("./module").A.B.C;
"#;
    let (parser, _root) = parse_source(source);

    // Should not emit any errors for nested member access after import()
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for typeof import with nested member access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_typeof_import_without_member_access() {
    // typeof import("...") without member access should still work
    let source = r#"
export const foo: typeof import("./module");
"#;
    let (parser, _root) = parse_source(source);

    // Should not emit any errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for typeof import without member access, got {:?}",
        parser.get_diagnostics()
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
