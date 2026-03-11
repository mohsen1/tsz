//! Tests for declaration parsing in the parser.
use crate::parser::{NodeIndex, ParserState, syntax_kind_ext};
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
