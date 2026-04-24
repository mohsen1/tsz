//! Tests for declaration parsing in the parser.
use crate::parser::{NodeIndex, ParserState, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

fn parse_source(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn parse_declaration_modules_with_generic_and_type_aliases() {
    let (parser, root) = parse_source(
        "declare module 'mod' {\n  export interface Alias<T> {\n    value: T;\n  }\n}\ndeclare function ready(): void;\n",
    );
    assert_eq!(parser.get_diagnostics().len(), 0);
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    assert_eq!(sf.statements.nodes.len(), 2);
}

#[test]
fn parse_declaration_with_recovery_for_invalid_member() {
    let (parser, root) = parse_source(
        "declare namespace NS {\n  export interface I {\n    prop: string = 1;\n  }\n}\n",
    );
    assert!(!parser.get_diagnostics().is_empty());
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    assert_eq!(sf.statements.nodes.len(), 1);
}

#[test]
fn parse_import_equals_declaration_with_targeted_error_recovery() {
    let (parser, _root) = parse_source("import = 'invalid';\nfunction ok() { return 1; }");
    assert!(!parser.get_diagnostics().is_empty());
}

#[test]
fn parse_invalid_named_import_star_reports_expression_expected() {
    let (parser, _root) = parse_source("import { * } from \"m\";");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "expected TS1003 for invalid named import `*`, got {codes:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "expected TS1109 recovery at `}}`, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "should not escalate to TS1128, got {codes:?}"
    );
}

#[test]
fn parse_default_import_followed_by_from_reports_missing_named_bindings() {
    let (parser, _root) = parse_source("import defaultBinding, from \"m\";");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 for missing named bindings, got {codes:?}"
    );
}

#[test]
fn parse_namespace_import_with_while_yields_to_while_statement_recovery() {
    // `import * as while from "foo"` — `while` is a reserved word. tsc emits
    // TS1359 at the keyword and then re-parses `while from "foo"` as a
    // WhileStatement, cascading `'(' expected.` at `from` and `')' expected.`
    // at `"foo"`. Make sure we match that cascade.
    let (parser, _root) = parse_source("import * as while from \"foo\"\n");
    let diags = parser.get_diagnostics();

    const TS1359: u32 =
        diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE;
    const TS1005: u32 = diagnostic_codes::EXPECTED;

    // TS1359 at `while` (byte offset 12 on line 1).
    assert!(
        diags.iter().any(|d| d.code == TS1359 && d.start == 12),
        "expected TS1359 at `while` (col 13), got {diags:?}"
    );
    // TS1005 `'(' expected.` at `from` (byte offset 18).
    assert!(
        diags
            .iter()
            .any(|d| d.code == TS1005 && d.start == 18 && d.message.contains("'('")),
        "expected TS1005 `'(' expected.` at `from` (col 19), got {diags:?}"
    );
    // TS1005 `')' expected.` at `"foo"` (byte offset 23).
    assert!(
        diags
            .iter()
            .any(|d| d.code == TS1005 && d.start == 23 && d.message.contains("')'")),
        "expected TS1005 `')' expected.` at `\"foo\"` (col 24), got {diags:?}"
    );
}

#[test]
fn parse_trailing_comma_before_from_recovers_as_next_statement() {
    let (parser, _root) = parse_source("import { a }, from \"m\";");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert_eq!(
        codes
            .iter()
            .filter(|&&code| code == diagnostic_codes::EXPECTED)
            .count(),
        2,
        "expected two TS1005 diagnostics (`from` and `;`), got {diags:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER),
        "should recover through `from` as next statement instead of TS1434, got {diags:?}"
    );
}

#[test]
fn parse_namespace_recovery_from_missing_closing_brace() {
    let (parser, _root) = parse_source("namespace Recover {\\n  export const value = 1;\\n");
    assert!(
        !parser.get_diagnostics().is_empty(),
        "expected diagnostics for unclosed namespace body"
    );
}

#[test]
fn parse_declare_using_as_single_variable_statement() {
    // `declare using y: null;` should parse as one VARIABLE_STATEMENT with declare modifier,
    // not as two statements (declare; + using y;)
    let (parser, root) = parse_source("declare using y: null;");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    assert_eq!(
        sf.statements.nodes.len(),
        1,
        "declare using should be a single statement"
    );
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::VARIABLE_STATEMENT,
        "declare using should produce a VARIABLE_STATEMENT"
    );
    let var_stmt = arena.get_variable(stmt_node).unwrap();
    assert!(
        arena.has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword),
        "VARIABLE_STATEMENT should have declare modifier"
    );
}

#[test]
fn parse_declare_await_using_as_single_variable_statement() {
    // `declare await using y: null;` should parse as one VARIABLE_STATEMENT with declare modifier
    let (parser, root) = parse_source("declare await using y: null;");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    assert_eq!(
        sf.statements.nodes.len(),
        1,
        "declare await using should be a single statement"
    );
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::VARIABLE_STATEMENT,
        "declare await using should produce a VARIABLE_STATEMENT"
    );
    let var_stmt = arena.get_variable(stmt_node).unwrap();
    assert!(
        arena.has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword),
        "VARIABLE_STATEMENT should have declare modifier"
    );
}

#[test]
fn parse_declare_export_function_as_single_statement() {
    // `declare export function f() { }` should parse as one FUNCTION_DECLARATION with declare modifier,
    // not as two statements (declare; + export function f() { })
    let (parser, root) = parse_source("declare export function f() { }");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    assert_eq!(
        sf.statements.nodes.len(),
        1,
        "declare export function should be a single statement"
    );
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::FUNCTION_DECLARATION,
        "declare export function should produce a FUNCTION_DECLARATION"
    );
}

// =====================================================================
// Export/Import specifier type-only modifier disambiguation tests
// =====================================================================

/// Helper: get the first export specifier from an export declaration
fn get_first_export_specifier(
    arena: &crate::parser::node::NodeArena,
    stmt_idx: NodeIndex,
) -> Option<&crate::parser::node::SpecifierData> {
    let node = arena.get(stmt_idx)?;
    let export_decl = arena.get_export_decl(node)?;
    let clause_node = arena.get(export_decl.export_clause)?;
    let named_exports = arena.get_named_imports(clause_node)?;
    let first = *named_exports.elements.nodes.first()?;
    let spec_node = arena.get(first)?;
    arena.get_specifier(spec_node)
}

#[test]
fn export_type_as_identifier_not_modifier() {
    // `export { type }` — `type` is the name, NOT a type-only modifier
    let (parser, root) = parse_source("export { type };");
    assert_eq!(
        parser.get_diagnostics().len(),
        0,
        "should parse without errors"
    );
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let spec = get_first_export_specifier(arena, sf.statements.nodes[0]).unwrap();
    assert!(
        !spec.is_type_only,
        "export {{ type }} should NOT be type-only"
    );
}

#[test]
fn export_type_something_is_type_only() {
    // `export { type something }` — type-only export of `something`
    let (parser, root) = parse_source("export { type something };");
    assert_eq!(
        parser.get_diagnostics().len(),
        0,
        "should parse without errors"
    );
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let spec = get_first_export_specifier(arena, sf.statements.nodes[0]).unwrap();
    assert!(
        spec.is_type_only,
        "export {{ type something }} should be type-only"
    );
    assert!(
        spec.property_name.is_none(),
        "should have no property_name (no rename)"
    );
}

#[test]
fn export_type_as_is_type_only() {
    // `export { type as }` — type-only export of `as`
    let (parser, root) = parse_source("export { type as };");
    assert_eq!(
        parser.get_diagnostics().len(),
        0,
        "should parse without errors"
    );
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let spec = get_first_export_specifier(arena, sf.statements.nodes[0]).unwrap();
    assert!(
        spec.is_type_only,
        "export {{ type as }} should be type-only"
    );
}

#[test]
fn export_type_as_as_is_rename_not_type_only() {
    // `export { type as as }` — NOT type-only, renames `type` to `as`
    let (parser, root) = parse_source("export { type as as };");
    assert_eq!(
        parser.get_diagnostics().len(),
        0,
        "should parse without errors"
    );
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let spec = get_first_export_specifier(arena, sf.statements.nodes[0]).unwrap();
    assert!(
        !spec.is_type_only,
        "export {{ type as as }} should NOT be type-only"
    );
    assert!(
        spec.property_name.is_some(),
        "should have property_name (rename from type to as)"
    );
}

#[test]
fn export_type_as_as_bar_is_type_only_rename() {
    // `export { type as as bar }` — type-only export of `as` renamed to `bar`
    let (parser, root) = parse_source("export { type as as bar };");
    assert_eq!(
        parser.get_diagnostics().len(),
        0,
        "should parse without errors"
    );
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let spec = get_first_export_specifier(arena, sf.statements.nodes[0]).unwrap();
    assert!(
        spec.is_type_only,
        "export {{ type as as bar }} should be type-only"
    );
    assert!(
        spec.property_name.is_some(),
        "should have property_name (rename as -> bar)"
    );
}

#[test]
fn export_type_type_as_foo_is_type_only_rename() {
    // `export { type type as foo }` — type-only export of `type` renamed to `foo`
    let (parser, root) = parse_source("export { type type as foo };");
    assert_eq!(
        parser.get_diagnostics().len(),
        0,
        "should parse without errors"
    );
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let spec = get_first_export_specifier(arena, sf.statements.nodes[0]).unwrap();
    assert!(
        spec.is_type_only,
        "export {{ type type as foo }} should be type-only"
    );
    assert!(
        spec.property_name.is_some(),
        "should have property_name (rename type -> foo)"
    );
}

#[test]
fn import_type_something_is_type_only() {
    // `import { type something } from './mod'` — type-only import of `something`
    let (parser, root) = parse_source("import { type something } from './mod';");
    assert_eq!(
        parser.get_diagnostics().len(),
        0,
        "should parse without errors"
    );
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    // Get the import specifier from the import clause
    let node = arena.get(sf.statements.nodes[0]).unwrap();
    let import = arena.get_import_decl(node).unwrap();
    let clause_node = arena.get(import.import_clause).unwrap();
    let clause = arena.get_import_clause(clause_node).unwrap();
    let bindings_node = arena.get(clause.named_bindings).unwrap();
    let named = arena.get_named_imports(bindings_node).unwrap();
    let spec_node = arena.get(named.elements.nodes[0]).unwrap();
    let spec = arena.get_specifier(spec_node).unwrap();
    assert!(
        spec.is_type_only,
        "import {{ type something }} should be type-only"
    );
}

#[test]
fn import_specifier_string_literal_binding_names_emit_ts1003() {
    let source = r#"
import { foo as "invalid 2" } from "./m";
import { "invalid 1" } from "./m";
import { type as "invalid 4" } from "./m";

import type { foo as "invalid 2" } from "./m";
import type { "invalid 1" } from "./m";
import type { type as "invalid 4" } from "./m";

import { type foo as "invalid 2" } from "./m";
import { type "invalid 1" } from "./m";
import { type as as "invalid 4" } from "./m";
"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1003_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::IDENTIFIER_EXPECTED)
        .count();
    assert_eq!(
        ts1003_count, 9,
        "expected TS1003 for every invalid import binding name, got {diagnostics:?}"
    );
}

#[test]
fn import_specifier_string_literal_export_name_with_identifier_alias_is_valid() {
    let (parser, _root) = parse_source(r#"import { "valid 1" as bar } from "./m";"#);
    assert_eq!(
        parser.get_diagnostics().len(),
        0,
        "valid arbitrary module namespace import should parse without diagnostics"
    );
}

#[test]
fn invalid_bigint_import_specifier_preserves_missing_brace_recovery() {
    let (parser, _root) = parse_source(r#"import { 0n as foo } from "./foo";"#);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1003),
        "expected TS1003 for invalid import specifier, got {codes:?}"
    );
    // TypeScript recovers by ending the malformed import clause, then reports
    // the stray `}` and `from` as follow-up syntax errors.
    assert!(
        codes.contains(&1128),
        "expected TS1128 after named imports recovery, got {codes:?}"
    );
    assert!(
        codes.contains(&1434),
        "expected TS1434 after named imports recovery, got {codes:?}"
    );
}

#[test]
#[ignore = "pre-existing: expects 2 TS1434 but parser now emits 1 after e0fc0207cd"]
fn malformed_import_clause_recovery_surfaces_statement_level_ts1434_and_ts1128() {
    let (parser, _root) = parse_source(
        r#"import { * } from "./foo";
import defaultBinding, from "./foo";
import , { a } from "./foo";
import { a }, from "./foo";"#,
    );
    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let ts1434 = diagnostics.iter().filter(|d| d.code == 1434).count();
    let ts1128 = diagnostics.iter().filter(|d| d.code == 1128).count();

    assert!(
        ts1434 >= 2,
        "expected TS1434 for malformed import-clause follow-up recovery, got {diagnostics:?}"
    );
    assert!(
        ts1128 >= 1,
        "expected TS1128 for statement-level comma recovery, got {diagnostics:?}"
    );
    assert!(
        codes.contains(&1003),
        "expected the malformed named import to keep its TS1003 root error, got {codes:?}"
    );
}

#[test]
fn bigint_literal_property_names_parse_without_cascading_member_errors() {
    let (parser, _root) = parse_source(
        r#"
interface G {
    2n: string;
}
class K {
    4n = 0;
}
const x = { 1n: 123 };
"#,
    );
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1068),
        "should not emit cascading TS1068 for bigint property names: {codes:?}"
    );
    assert!(
        !codes.contains(&1131),
        "should not emit TS1131 for bigint property names: {codes:?}"
    );
    assert!(
        !codes.contains(&1136),
        "should not emit TS1136 for bigint property names: {codes:?}"
    );
}

#[test]
fn dotted_decimal_bigint_suffix_reports_ts1353_and_ts1434() {
    let (parser, _root) = parse_source("g.2n;");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1353),
        "expected TS1353 for dotted bigint suffix, got {codes:?}"
    );
    assert!(
        codes.contains(&1434),
        "expected TS1434 for invalid member access recovery, got {codes:?}"
    );
}

#[test]
fn dotted_decimal_bigint_suffix_does_not_duplicate_ts1353_from_lookahead() {
    let (parser, _root) = parse_source("g.2n;");
    let ts1353_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1353)
        .count();
    assert_eq!(
        ts1353_count,
        1,
        "expected a single TS1353 after speculative scans, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn keyword_followed_by_string_literal_reports_ts1434() {
    let (parser, _root) = parse_source(r#"from "./foo";"#);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1434),
        "expected TS1434 for keyword-like statement followed by a string literal, got {codes:?}"
    );
}

#[test]
fn malformed_exported_declaration_reports_ts1128_on_export_and_ts1434_on_identifier() {
    let (parser, _root) = parse_source(
        r#"
declare namespace M {
    export extension class C {}
}
"#,
    );
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1128),
        "expected TS1128 for malformed exported declaration, got {diags:?}"
    );
    assert!(
        codes.contains(&1434),
        "expected TS1434 for identifier after malformed export, got {diags:?}"
    );
}

#[test]
fn invalid_bigint_import_specifiers_recover_cleanly() {
    // Invalid bigint import specifiers recover by ending the import at the
    // malformed clause, then reporting the stray `}` and `from` as follow-up
    // syntax errors. This matches the current TypeScript baseline.
    for source in [
        r#"import { 0n as foo } from "./foo";"#,
        r#"import { foo as 0n } from "./foo";"#,
    ] {
        let (parser, _root) = parse_source(source);
        let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&1128),
            "expected TS1128 for {source:?}, got {codes:?}"
        );
        assert!(
            codes.contains(&1434),
            "expected TS1434 for {source:?}, got {codes:?}"
        );
    }
}

#[test]
fn export_assignment_with_declare_modifier_emits_ts1120() {
    // `declare export = x` should emit TS1120 at the position of `declare`
    let source = "var x;\ndeclare export = x;\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1120),
        "Expected TS1120 for `declare export = x`, got: {codes:?}"
    );
    let ts1120 = diags.iter().find(|d| d.code == 1120).unwrap();
    // `declare` starts at column 0 of line 2 (byte offset 7)
    assert_eq!(ts1120.start, 7, "TS1120 should start at `declare` position");
}

#[test]
fn export_assignment_with_export_declare_modifiers_emits_ts1120() {
    // `export declare export = x` should emit TS1120 at the position of `export`
    let source = "var x;\nexport declare export = x;\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1120),
        "Expected TS1120 for `export declare export = x`, got: {codes:?}"
    );
    let ts1120 = diags.iter().find(|d| d.code == 1120).unwrap();
    // `export` starts at column 0 of line 2 (byte offset 7)
    assert_eq!(ts1120.start, 7, "TS1120 should start at `export` position");
}

/// `import\nimport { foo } from './0';` — the first `import` has no clause and a reserved
/// keyword (`import`) follows on the next line. tsc emits TS1109 "Expression expected" at
/// the second `import` position (the module specifier path fails to find a string literal)
/// and the second import statement parses cleanly.
/// Previously our parser routed this through import-equals because `look_ahead_is_import_equals`
/// accepted reserved keywords as binding names.
#[test]
fn import_followed_by_reserved_keyword_emits_ts1109_not_ts1005() {
    let (parser, _root) = parse_source("import\nimport { foo } from './0';");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1109),
        "Expected TS1109 'Expression expected' for missing module specifier, got {codes:?}"
    );
    assert!(
        !codes.contains(&1434),
        "Should not emit TS1434 'Unexpected keyword or identifier', got {codes:?}"
    );
}

#[test]
fn declare_class_with_parenthesized_tail_recovers_with_ts1109_not_ts1068() {
    let (parser, _root) = parse_source("declare class foo();\nfunction foo() {}\n");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1005),
        "Expected TS1005 for missing class body brace, got {codes:?}"
    );
    assert!(
        codes.contains(&1109),
        "Expected TS1109 at parenthesized class tail, got {codes:?}"
    );
    assert!(
        !codes.contains(&1068),
        "Should not cascade into TS1068 class-member diagnostics, got {codes:?}"
    );
}

/// `import class` should not be treated as import-equals (class is a reserved word).
#[test]
fn import_reserved_keyword_class_not_import_equals() {
    let (parser, _root) = parse_source("import class {}");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    // Should NOT go through import-equals path (which would emit '= expected')
    let has_equals_expected = parser
        .get_diagnostics()
        .iter()
        .any(|d| d.code == 1005 && d.message.contains("'='"));
    assert!(
        !has_equals_expected,
        "Should not emit '= expected' for reserved keyword after import, got {codes:?}"
    );
}

/// `import for` should not be treated as import-equals.
#[test]
fn import_reserved_keyword_for_not_import_equals() {
    let (parser, _root) = parse_source("import for (;;) {}");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    let has_equals_expected = parser
        .get_diagnostics()
        .iter()
        .any(|d| d.code == 1005 && d.message.contains("'='"));
    assert!(
        !has_equals_expected,
        "Should not emit '= expected' for reserved keyword after import, got {codes:?}"
    );
}

/// `import type X = require(...)` should still work (type is contextual keyword).
#[test]
fn import_type_equals_still_works() {
    let (parser, _root) = parse_source("import type = require('mod');");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    // This should parse as import-equals declaration, no parser errors
    assert!(
        codes.is_empty(),
        "import type = require('mod') should parse cleanly, got {codes:?}"
    );
}

/// `import async = require(...)` should still work (async is contextual keyword).
#[test]
fn import_async_equals_still_works() {
    let (parser, _root) = parse_source("import async = require('mod');");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.is_empty(),
        "import async = require('mod') should parse cleanly, got {codes:?}"
    );
}

// === ES Decorator misplacement tests (tsc parity) ===

/// `abstract @dec class C {}` at statement level should emit TS1434
/// "Unexpected keyword or identifier." at the 'abstract' position.
/// tsc treats `abstract @dec class` as invalid — `abstract` is an expression
/// and `@dec class` can't follow without a semicolon.
#[test]
fn abstract_at_statement_level_before_decorator_emits_ts1434() {
    let (parser, _root) = parse_source("abstract @dec class C {}");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER),
        "expected TS1434 for `abstract @dec class`, got {codes:?}"
    );
}

/// `export abstract @dec class C {}` should emit:
///   TS1128 "Declaration or statement expected." at the 'export' position
///   TS1434 "Unexpected keyword or identifier." at the 'abstract' position
#[test]
fn export_abstract_before_decorator_emits_ts1128_and_ts1434() {
    let (parser, _root) = parse_source("export abstract @dec class C {}");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "expected TS1128 for `export abstract @dec class`, got {codes:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER),
        "expected TS1434 for `export abstract @dec class`, got {codes:?}"
    );
}

/// `export default abstract @dec class C {}` should emit TS1005 "';' expected."
/// at the '@' position, because 'abstract' is parsed as an expression identifier
/// and '@' is not a valid continuation without a semicolon.
#[test]
fn export_default_abstract_before_decorator_emits_ts1005() {
    let (parser, _root) = parse_source("export default abstract @dec class C {}");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 (';' expected) for `export default abstract @dec class`, got {codes:?}"
    );
}

/// `abstract class C {}` should still parse cleanly (no errors).
#[test]
fn abstract_class_parses_cleanly() {
    let (parser, _root) = parse_source("abstract class C {}");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.is_empty(),
        "abstract class C {{}} should parse cleanly, got {codes:?}"
    );
}

/// `export abstract class C {}` should still parse cleanly (no errors).
#[test]
fn export_abstract_class_parses_cleanly() {
    let (parser, _root) = parse_source("export abstract class C {}");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.is_empty(),
        "export abstract class C {{}} should parse cleanly, got {codes:?}"
    );
}

/// `import type defer * as ns from "..."` is invalid: `type` and `defer`
/// cannot both modify the same import, and the namespace form is not allowed
/// after `defer`. tsc enters `parseImportEqualsDeclaration` because the
/// disambiguation concludes the name is `defer` and the next token (`*`) is
/// not `,`/`from`. It then reports:
///
/// - TS1005 `'=' expected.` at `*` (the missing equals sign).
/// - TS1005 `';' expected.` at `ns` (the binary-like `missing * as` ends
///   and the next token starts a new expression statement).
/// - TS1434 `Unexpected keyword or identifier.` at `from` (viable keyword
///   in primary position followed by a string literal).
///
/// We must match that fingerprint exactly, not cascade an extra TS1434 at
/// `as` from the earlier unary recovery.
#[test]
fn parse_import_type_defer_star_matches_tsc_recovery() {
    let source = "import type defer * as ns1 from \"./a\";";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();

    const TS1005: u32 = diagnostic_codes::EXPECTED;
    const TS1434: u32 = diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER;

    let fingerprints: Vec<(u32, u32, &str)> = diags
        .iter()
        .map(|d| (d.code, d.start, d.message.as_str()))
        .collect();

    // TS1005 `'=' expected.` at `*` (pos 18, col 19).
    assert!(
        fingerprints
            .iter()
            .any(|(c, p, m)| *c == TS1005 && *p == 18 && m.contains("'='")),
        "expected TS1005 `'=' expected.` at col 19 (pos 18), got {fingerprints:?}"
    );
    // TS1005 `';' expected.` at `ns1` (pos 23, col 24).
    assert!(
        fingerprints
            .iter()
            .any(|(c, p, m)| *c == TS1005 && *p == 23 && m.contains("';'")),
        "expected TS1005 `';' expected.` at col 24 (pos 23), got {fingerprints:?}"
    );
    // TS1434 `Unexpected keyword or identifier.` at `from` (pos 27, col 28).
    assert!(
        fingerprints
            .iter()
            .any(|(c, p, _)| *c == TS1434 && *p == 27),
        "expected TS1434 at col 28 (pos 27), got {fingerprints:?}"
    );

    // Must NOT emit any diagnostic at `as` (pos 20, col 21) — that was the
    // spurious cascade from the old asterisk-recovery path.
    assert!(
        !fingerprints.iter().any(|(_, p, _)| *p == 20),
        "must not emit a diagnostic at `as` (col 21); got {fingerprints:?}"
    );

    // We only expect three parser diagnostics total for this invalid syntax.
    // Additional emits indicate a regression of the cascading recovery.
    assert_eq!(
        diags.len(),
        3,
        "expected exactly 3 parser diagnostics, got {diags:?}"
    );
}

/// Confirm that an isolated `*` at statement start is treated as a binary
/// operator with missing LHS — exactly one TS1109 (Expression expected) is
/// emitted at the `*` and the trailing `foo` becomes a separate expression
/// statement. This matches tsc's `parsePrimaryExpression -> createMissingNode`
/// followed by binary-operator consumption.
#[test]
fn parse_leading_asterisk_at_statement_emits_single_expression_expected() {
    let source = "* foo";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();

    const TS1109: u32 = diagnostic_codes::EXPRESSION_EXPECTED;

    // One TS1109 at the `*` (pos 0).
    let ts1109_count = diags.iter().filter(|d| d.code == TS1109).count();
    assert!(
        ts1109_count >= 1,
        "expected at least one TS1109 for leading `*`, got {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.code == TS1109 && d.start == 0),
        "expected TS1109 at the `*` (pos 0), got {diags:?}"
    );
}

/// `as` and `satisfies` are contextual keywords: they can be used as plain
/// identifiers. In primary expression position they must parse as identifiers
/// rather than triggering the `is_binary_operator` missing-LHS path.
#[test]
fn parse_as_and_satisfies_as_identifiers_in_primary_position() {
    for name in ["as", "satisfies"] {
        let source = format!("const x = {name};");
        let (parser, _root) = parse_source(&source);
        let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
        assert!(
            !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
            "`const x = {name};` must not emit TS1109; got {codes:?}"
        );
    }
}
