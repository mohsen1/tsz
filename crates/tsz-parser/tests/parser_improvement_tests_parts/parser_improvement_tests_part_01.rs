#[test]
fn test_import_type_options_identifier_recovery_reports_ts1134() {
    let source = r#"
type Attribute1 = { with: {"resolution-mode": "require"} };
export const a = (null as any as import("pkg", Attribute1).RequireInterface);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
fn test_type_argument_with_empty_jsdoc_wildcard_has_no_ts1110() {
    // `Foo<?>` should emit TS8020 but avoid TS17020/TS1110 cascading.
    let source = r#"
type T = Foo<?>;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected TS8020 for `Foo<?>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected no TS1110 for `Foo<?>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(&diagnostic_codes::AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE),
        "Expected no TS17020 for `Foo<?>`, got {:?}",
        parser.get_diagnostics(),
    );
}
#[test]
fn test_type_argument_with_jsdoc_prefix_type_emits_ts17020() {
    // `Foo<?string>` should emit TS17020 for the JSDoc-style leading `?`, but
    // the operand is still a real type so this is not the bare-wildcard TS8020 case.
    let source = r#"
type T = Foo<?string>;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(&diagnostic_codes::AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE),
        "Expected TS17020 for `Foo<?string>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected no TS8020 for `Foo<?string>`, got {:?}",
        parser.get_diagnostics(),
    );
}
#[test]
fn test_expression_type_argument_with_empty_jsdoc_wildcard_emits_ts8020_only() {
    let source = r#"
const WhatFoo = foo<?>;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected TS8020 for `foo<?>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected no TS1110 for `foo<?>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(&diagnostic_codes::AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE),
        "Expected no TS17020 for `foo<?>`, got {:?}",
        parser.get_diagnostics(),
    );
}
#[test]
fn test_expression_type_argument_with_jsdoc_prefix_type_emits_ts17020_only() {
    let source = r#"
const NopeFoo = foo<?string>;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(
            &diagnostic_codes::AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE
        ),
        "Expected TS17020 for `foo<?string>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected no TS8020 for `foo<?string>`, got {:?}",
        parser.get_diagnostics(),
    );
}
#[test]
fn test_old_jsdoc_qualified_name_generic_reports_ts8020() {
    // Old JSDoc generic syntax `Array.<T>` should recover with TS8020.
    let source = r#"
type T = Array.<string>;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected TS8020 for `Array.<string>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected no TS1003 fallback for `Array.<string>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected no TS1110 fallback for `Array.<string>`, got {:?}",
        parser.get_diagnostics(),
    );
}
#[test]
fn test_jsdoc_legacy_function_type_reports_ts8020_without_parse_cascade() {
    let source = r#"
function hof(ctor: function(new: number, string)) {
    return new ctor('hi');
}

function hof2(f: function(this: number, string): string) {
    return f(12, 'hullo');
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();

    let ts8020_count = diagnostics
        .iter()
        .filter(|code| {
            **code == diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        })
        .count();
    assert_eq!(
        ts8020_count,
        2,
        "Expected TS8020 for both legacy function types, got {:?}",
        parser.get_diagnostics()
    );

    // TS2554 (too many arguments) is a checker-level diagnostic; it cannot appear
    // in parser diagnostics. The parser's job is to recover JSDoc `function(...)` into
    // a well-formed FunctionType so the checker can later produce TS2554.

    assert!(
        !diagnostics
            .iter()
            .any(|code| *code == 1003 || *code == 1005 || *code == 1109),
        "Did not expect parser-level recovery diagnostics for legacy function types, got {:?}",
        parser.get_diagnostics()
    );
}
#[test]
fn test_jsdoc_wildcard_type_reports_ts8020_only() {
    let source = r"
let whatevs: * = 1001;
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert_eq!(
        diagnostics,
        vec![diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS],
        "Expected only TS8020 for wildcard type, got {:?}",
        parser.get_diagnostics()
    );
}

// =============================================================================
// Tuple Type Tests
// =============================================================================
#[test]
fn test_optional_tuple_element() {
    // [T?] should parse correctly without TS1005/TS1110
    let source = r"
interface Buzz { id: number; }
type T = [Buzz?];
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS1005 or TS1110 for optional tuple element
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    let ts1110_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1110)
        .count();

    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for optional tuple element, got {ts1005_count}",
    );
    assert_eq!(
        ts1110_count, 0,
        "Expected no TS1110 errors for optional tuple element, got {ts1110_count}",
    );
}
#[test]
fn test_readonly_optional_tuple_element() {
    // readonly [T?] should parse correctly
    let source = r"
interface Buzz { id: number; }
type T = readonly [Buzz?];
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for readonly optional tuple, got {:?}",
        parser.get_diagnostics()
    );
}
#[test]
fn test_named_tuple_element_still_works() {
    // name?: T should still parse as a named tuple element
    let source = r"
type T = [name?: string];
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for named optional tuple element, got {:?}",
        parser.get_diagnostics()
    );
}
#[test]
fn test_mixed_tuple_elements() {
    // Mix of optional, named, and rest elements should work
    let source = r"
interface A { a: number; }
interface B { b: string; }
type T = [A?, name: B, ...rest: string[]];
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for mixed tuple elements, got {:?}",
        parser.get_diagnostics()
    );
}
#[test]
fn test_argument_list_recovery_on_return_keyword() {
    let source = r"
const x = fn(
  return
);
const y = 1;
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1135_count = diagnostics.iter().filter(|d| d.code == 1135).count();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert!(
        ts1135_count >= 1,
        "Expected at least 1 TS1135 for malformed argument list, got diagnostics: {diagnostics:?}"
    );
    assert!(
        ts1005_count <= 2,
        "Expected limited TS1005 cascade for malformed argument list, got {ts1005_count} diagnostics: {diagnostics:?}",
    );
}
#[test]
fn test_invalid_unicode_escape_in_var_no_extra_semicolon_error() {
    let source = r"var arg\uxxxx";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for invalid unicode escape, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1005_count, 0,
        "Expected no extra TS1005 for invalid unicode escape, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_invalid_unicode_escape_as_variable_name_no_var_decl_cascade() {
    let source = r"var \u0031a;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    let ts1123_count = diagnostics.iter().filter(|d| d.code == 1123).count();
    let ts1134_count = diagnostics.iter().filter(|d| d.code == 1134).count();

    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for invalid unicode escape, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1123_count, 0,
        "Expected no TS1123 variable declaration cascade, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1134_count, 0,
        "Expected no TS1134 variable declaration cascade, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_class_method_string_names_use_string_literal_nodes() {
    let source = r#"
class C {
    "foo"();
    "bar"() { }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    let class_idx = source_file.statements.nodes[0];
    let class_node = parser.get_arena().get(class_idx).unwrap();
    let class_data = parser.get_arena().get_class(class_node).unwrap();
    let kinds: Vec<_> = class_data
        .members
        .nodes
        .iter()
        .filter_map(|&member_idx| {
            let member_node = parser.get_arena().get(member_idx)?;
            (member_node.kind == crate::parser::syntax_kind_ext::METHOD_DECLARATION).then_some({
                let method = parser.get_arena().get_method_decl(member_node)?;
                let name_node = parser.get_arena().get(method.name)?;
                (
                    method.name,
                    name_node.kind,
                    parser
                        .get_arena()
                        .get_literal(name_node)
                        .map(|lit| lit.text.clone()),
                )
            })
        })
        .collect();

    assert_eq!(kinds.len(), 2);
    for (_name_idx, kind, text) in kinds {
        assert_eq!(
            kind,
            tsz_scanner::SyntaxKind::StringLiteral as u16,
            "expected string literal name node"
        );
        assert!(text.is_some());
    }
}

// =============================================================================
// Yield Expression Tests
// =============================================================================
#[test]
fn test_yield_after_type_assertion_requires_parens() {
    // yield without parentheses after type assertion should emit TS1109
    let source = r"
function* f() {
    <number> yield 0;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();

    assert_eq!(
        ts1109_count, 1,
        "Expected 1 TS1109 error for yield without parens after type assertion, got {ts1109_count}. Diagnostics: {diagnostics:?}",
    );

    // Check that the error mentions expression
    let has_expression_expected = diagnostics
        .iter()
        .any(|d| d.code == 1109 && d.message.to_lowercase().contains("expression"));
    assert!(
        has_expression_expected,
        "Expected TS1109 error to mention 'expression', got diagnostics: {diagnostics:?}",
    );
}
#[test]
fn test_yield_with_parens_after_type_assertion_is_valid() {
    // yield with parentheses after type assertion should be valid
    let source = r"
function* f() {
    <number> (yield 0);
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();

    // Should not emit TS1109 for yield in parentheses
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();
    assert_eq!(
        ts1109_count, 0,
        "Expected no TS1109 errors for yield with parens, got {ts1109_count}. Diagnostics: {diagnostics:?}",
    );
}
#[test]
fn test_generator_recovery_keeps_yield_statement_after_broken_initializer() {
    let source = r"
function* f() {
    )
    yield 1;
    const ok = 2;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    let function_idx = source_file.statements.nodes[0];
    let function_node = parser.get_arena().get(function_idx).unwrap();
    let function_data = parser.get_arena().get_function(function_node).unwrap();
    let body = parser.get_arena().get_block_at(function_data.body).unwrap();

    assert!(!parser.get_diagnostics().is_empty());
    assert_eq!(
        body.statements.nodes.len(),
        2,
        "Expected parser recovery to keep yield statement after an invalid token"
    );

    let yield_stmt_node = parser
        .get_arena()
        .get(body.statements.nodes[0])
        .expect("expected yield statement in generator body");
    assert_eq!(
        yield_stmt_node.kind,
        crate::parser::syntax_kind_ext::EXPRESSION_STATEMENT,
        "Expected first recovered statement to be an expression statement containing yield"
    );

    let yield_stmt_data = parser
        .get_arena()
        .get_expression_statement(yield_stmt_node)
        .expect("expected expression statement data for recovered yield statement");
    let yield_expr_node = parser
        .get_arena()
        .get(yield_stmt_data.expression)
        .expect("expected recovered yield expression node");
    let yield_text = &source[yield_expr_node.pos as usize..yield_expr_node.end as usize];
    assert!(
        yield_text.trim_start().starts_with("yield"),
        "Expected recovered statement text to start with `yield`, got: {yield_text:?}"
    );
}

// =============================================================================
// Orphan Catch/Finally Tests
// =============================================================================
#[test]
fn test_orphan_catch_block_emits_ts1005() {
    // catch block without try should emit TS1005: 'try' expected
    let source = r"
function fn() {
    catch(x) { }
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert_eq!(
        ts1005_count, 1,
        "Expected 1 TS1005 error for orphan catch block, got {ts1005_count}. Diagnostics: {diagnostics:?}",
    );

    // Check that the error message mentions 'try'
    let has_try_expected = diagnostics
        .iter()
        .any(|d| d.code == 1005 && d.message.to_lowercase().contains("try"));
    assert!(
        has_try_expected,
        "Expected TS1005 error to mention 'try', got diagnostics: {diagnostics:?}",
    );
}
#[test]
fn test_orphan_finally_block_emits_ts1005() {
    // finally block without try should emit TS1005: 'try' expected
    let source = r"
function fn() {
    finally { }
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert_eq!(
        ts1005_count, 1,
        "Expected 1 TS1005 error for orphan finally block, got {ts1005_count}. Diagnostics: {diagnostics:?}",
    );

    // Check that the error message mentions 'try'
    let has_try_expected = diagnostics
        .iter()
        .any(|d| d.code == 1005 && d.message.to_lowercase().contains("try"));
    assert!(
        has_try_expected,
        "Expected TS1005 error to mention 'try', got diagnostics: {diagnostics:?}",
    );
}
#[test]
fn test_multiple_orphan_blocks_emit_separate_ts1005() {
    // Multiple orphan catch/finally blocks should each emit TS1005
    let source = r"
function fn() {
    finally { }
    catch (x) { }
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert_eq!(
        ts1005_count, 2,
        "Expected 2 TS1005 errors for two orphan blocks, got {ts1005_count}. Diagnostics: {diagnostics:?}",
    );
}
#[test]
fn test_ts1131_emitted_for_invalid_interface_member() {
    // Invalid token inside an interface body should emit TS1131
    // "Property or signature expected."
    let source = r"
interface Foo {
    ?;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1131_count = diagnostics.iter().filter(|d| d.code == 1131).count();
    assert!(
        ts1131_count >= 1,
        "Expected at least 1 TS1131 for invalid interface member, got {ts1131_count}. Diagnostics: {diagnostics:?}",
    );
}
#[test]
fn test_ts1131_emitted_for_invalid_type_literal_member() {
    // Invalid token inside a type literal should emit TS1131
    let source = r"
type T = {
    !;
};
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1131_count = diagnostics.iter().filter(|d| d.code == 1131).count();
    assert!(
        ts1131_count >= 1,
        "Expected at least 1 TS1131 for invalid type literal member, got {ts1131_count}. Diagnostics: {diagnostics:?}",
    );
}
#[test]
fn test_type_literal_statement_recovery_matches_interface_extending_class2() {
    let source = r"
class Foo {
    x: string;
    y() { }
    get Z() {
        return 1;
    }
    [x: string]: Object;
}

interface I2 extends Foo {
    a: {
        toString: () => {
            return 1;
        };
    }
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![1131, 1128, 1128],
        "Expected parser recovery to match tsc for malformed type literal member body, got {diagnostics:?}"
    );
}
#[test]
fn test_ts1131_not_emitted_for_valid_interface() {
    // Valid interface should not emit TS1131
    let source = r"
interface Foo {
    x: number;
    y: string;
    z(): void;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1131_count = diagnostics.iter().filter(|d| d.code == 1131).count();
    assert_eq!(
        ts1131_count, 0,
        "Expected no TS1131 for valid interface, got {ts1131_count}. Diagnostics: {diagnostics:?}",
    );
}

// =============================================================================
// Import Defer Tests
// =============================================================================
#[test]
fn test_import_defer_namespace_parses_clean() {
    // `import defer * as ns from "mod"` is valid — no parse errors
    let source = r#"import defer * as ns from "./a";"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
fn test_regex_named_capturing_groups_do_not_emit_unexpected_paren() {
    let source = r#"const re = /(?<year>\d{4})-(?<month>\d{2})-(?<day>\d{2})/u;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();
    let ts1508: Vec<_> = diagnostics.iter().filter(|d| d.code == 1508).collect();
    assert!(
        ts1508.is_empty(),
        "Expected valid named capturing groups to avoid TS1508, got {diagnostics:?}"
    );
}
#[test]
fn test_regex_unicode_brace_escape_variants_do_not_emit_ts1125() {
    let source = r#"
const a = /\u{-DDDD}/gu;
const b = /\u{r}\u{n}\u{t}/gu;
const c = /\u{}/gu;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();
    let ts1125: Vec<_> = diagnostics.iter().filter(|d| d.code == 1125).collect();
    assert!(
        ts1125.is_empty(),
        "Expected brace-form regex unicode escapes to avoid TS1125, got {diagnostics:?}"
    );
}

// =============================================================================
// Bare Hash Character Recovery (TS1127)
// =============================================================================
#[test]
fn test_bare_hash_at_top_level_emits_ts1127() {
    // Bare `#` at top level should emit TS1127, not cascading errors
    let source = "# foo";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for bare '#', got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_bare_hash_in_class_emits_ts1127() {
    // Bare `#` in class body should emit TS1127, not cascading errors
    let source = r"
class C {
    # name;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for bare '#' in class body, got diagnostics: {diagnostics:?}"
    );
    // Should NOT cascade into TS1003/TS1005/TS1068/TS1128
    let cascade_count = diagnostics
        .iter()
        .filter(|d| matches!(d.code, 1003 | 1005 | 1068 | 1128))
        .count();
    assert_eq!(
        cascade_count, 0,
        "Bare '#' should not cascade into other errors, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_valid_private_name_no_ts1127() {
    // Valid private names should not emit TS1127
    let source = r"
class C {
    #name = 42;
    get #value() { return this.#name; }
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    assert_eq!(
        ts1127_count, 0,
        "Valid private names should not emit TS1127, got diagnostics: {diagnostics:?}"
    );
}

// =============================================================================
// Nullable Type Syntax Recovery (TS17019/TS17020)
// =============================================================================
#[test]
fn test_postfix_question_emits_ts17019() {
    // `string?` should emit TS17019, not TS1005 or TS1110
    let source = "let x: string?;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts17019_count = diagnostics.iter().filter(|d| d.code == 17019).count();
    assert!(
        ts17019_count >= 1,
        "Expected TS17019 for postfix '?' on type, got diagnostics: {diagnostics:?}"
    );
    // Should NOT emit TS1005 or TS1110 cascade
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    let ts1110_count = diagnostics.iter().filter(|d| d.code == 1110).count();
    assert_eq!(
        ts1005_count, 0,
        "Should not emit TS1005 for nullable type, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1110_count, 0,
        "Should not emit TS1110 for nullable type, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_prefix_question_emits_ts17020() {
    // `?string` should emit TS17020, not TS1110
    let source = "let x: ?string;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17020_count >= 1,
        "Expected TS17020 for prefix '?' on type, got diagnostics: {diagnostics:?}"
    );
    // Should NOT emit TS1110 cascade
    let ts1110_count = diagnostics.iter().filter(|d| d.code == 1110).count();
    assert_eq!(
        ts1110_count, 0,
        "Should not emit TS1110 for nullable type, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_multiple_nullable_types() {
    // Multiple nullable types in different positions
    let source = r"
function f(x: string?): ?number {
    return null;
}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts17019_count = diagnostics.iter().filter(|d| d.code == 17019).count();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17019_count >= 1,
        "Expected at least 1 TS17019 for postfix '?', got diagnostics: {diagnostics:?}"
    );
    assert!(
        ts17020_count >= 1,
        "Expected at least 1 TS17020 for prefix '?', got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_nullable_type_in_type_predicate() {
    // `x is ?string` should emit TS17020
    let source = "function f(x: any): x is ?string { return true; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17020_count >= 1,
        "Expected TS17020 for '?string' in type predicate, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_nullable_type_no_cascade() {
    // Nullable type should not cause cascading errors
    let source = r#"
let a: string? = "hello";
let b: ?number = 42;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    // Should only have TS17019 and TS17020, no cascade
    let cascade_codes: Vec<u32> = diagnostics
        .iter()
        .filter(|d| d.code == 1005 || d.code == 1109 || d.code == 1110 || d.code == 1128)
        .map(|d| d.code)
        .collect();
    assert!(
        cascade_codes.is_empty(),
        "Nullable types should not cause cascading errors, got: {cascade_codes:?}. All: {diagnostics:?}"
    );
}
#[test]
fn test_adjacent_jsx_roots_in_tsx_report_ts2657() {
    let source = r"
declare namespace JSX { interface Element { } }

<div></div>
<div></div>

var x = <div></div><div></div>
";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts2657_count = diagnostics.iter().filter(|d| d.code == 2657).count();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();

    // tsc emits TS2657 for adjacent JSX roots in ALL JSX files (.tsx, .jsx, .js)
    assert!(
        ts2657_count >= 1,
        "Expected TS2657 for adjacent JSX siblings in TSX, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1003_count, 0,
        "Adjacent JSX recovery should not leak TS1003, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1109_count, 0,
        "Adjacent JSX recovery should not leak TS1109, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_jsx_type_arguments_in_js_report_ts2657() {
    let source = r#"
/// <reference path="/.lib/react.d.ts" />
import { MyComp, Prop } from "./component";
import * as React from "react";

let x = <MyComp<Prop> a={10} b="hi" />; // error, no type arguments in js
"#;
    let mut parser = ParserState::new("file.jsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&2657),
        "Expected TS2657 for JSX type arguments in JS recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&1003),
        "Expected TS1003 alongside TS2657 for illegal JSX type-argument syntax, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_js_call_type_argument_syntax_prefers_relational_parsing() {
    let source = r#"
Foo<number>();
Foo<number>(1);
Foo<number>``;
"#;
    let mut parser = ParserState::new("a.jsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();

    assert_eq!(
        ts1109_count, 1,
        "Expected only the empty-call JS generic syntax case to emit TS1109, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1003_count, 0,
        "Non-JSX JS generic-call syntax should not leak JSX TS1003 recovery diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_jsx_type_arguments_in_js_with_closing_tag_report_ts17002() {
    let source = r#"
<Foo<number>></Foo>;
"#;
    let mut parser = ParserState::new("a.jsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&17002),
        "Expected TS17002 for the mismatched closing tag after JS JSX type-argument recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&2657),
        "Expected TS2657 for the recovered adjacent JSX roots, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_let_array_ambiguity_reports_ts1181_then_statement_recovery() {
    let source = r#"
var let: any;
let[0] = 100;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![1181, 1005, 1128],
        "Expected TS1181/TS1005/TS1128 recovery for ambiguous `let[` statement, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_for_header_let_disambiguation_matches_invalid_for_of_recovery() {
    let source = r#"
var let = 10;
for (let of [1,2,3]) {}

for (let in [1,2,3]) {}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![1005, 1181, 1005, 1128],
        "Expected TS1005/TS1181/TS1005/TS1128 recovery for `for (let of [...])`, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_invalid_nonnullable_type_recovery_reports_ts17019_and_ts17020() {
    let source = r#"
function f1(a: string): a is string! { return true; }
function f2(a: string): a is !string { return true; }
const a = 1 as any!;
const b = 1 as !any;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![17019, 17020, 17019, 17020],
        "Expected TS17019/TS17020 recovery for invalid non-nullable type syntax, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_unclosed_jsx_fragment_after_unary_plus_in_tsx_reports_ts17014() {
    let source = r#"
const x = "oops";
const y = + <> x;
"#;
    let mut parser = ParserState::new("index.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&17014),
        "Expected TSX unary `+ <>` recovery to report TS17014, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_js_unclosed_jsx_fragment_after_unary_plus_reports_ts17014() {
    let source = r#"
const x = "oops";
const y = + <> x;
"#;
    let mut parser = ParserState::new("index.js".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&17014),
        "Expected TS17014 for JS unary `+ <>` JSX-fragment recovery, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_js_unary_tilde_then_malformed_jsx_reports_ts1003() {
    let source = "~< <";
    let mut parser = ParserState::new("a.js".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();

    assert!(
        codes.contains(&1003),
        "Expected TS1003 for malformed JSX after unary `~`, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1003_count, 1,
        "Expected exactly one TS1003 for malformed JSX after unary `~`, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1109_count, 1,
        "Expected exactly one trailing TS1109 for malformed JSX after unary `~`, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_js_unary_plus_then_numeric_jsx_head_reports_ts1003_without_ts1109() {
    let source = r#"
const x = "oops";
const y = + <1234> x;
"#;
    let mut parser = ParserState::new("index.js".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected TS1003 for malformed JSX tag head `<1234>`, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected no TS1109 fallback for malformed numeric JSX tag head, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_tsx_unary_plus_mixed_type_assertion_and_fragment_matches_conformance_shape() {
    let source = r#"
const x = "oops";

const a = + <number> x;
const b = + <> x;
const c = + <1234> x;
"#;
    let mut parser = ParserState::new("index.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::JSX_ELEMENT_HAS_NO_CORRESPONDING_CLOSING_TAG),
        "Expected TS17008 from unary `+ <number> x` JSX recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG),
        "Expected TS17014 from unary `+ <> x` JSX recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected TS1003 for malformed numeric JSX tag head `<1234>`, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT),
        "Expected TS1382 on malformed numeric JSX tag head close token, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "Expected TS1005 recovery tail after malformed JSX unary expressions, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_js_unary_bang_then_braced_jsx_head_reports_ts17008_without_ts1109() {
    let source = "!< {:>";
    let mut parser = ParserState::new("a.js".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected TS1003 for malformed braced JSX tag head, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::JSX_ELEMENT_HAS_NO_CORRESPONDING_CLOSING_TAG),
        "Expected TS17008 unclosed JSX element recovery for `!< {{:>`, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected no TS1109 fallback for malformed braced JSX tag head, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_tsx_malformed_extends_in_generic_arrow_ambiguity_prefers_jsx_ts1382() {
    let source = r#"
declare namespace JSX {
    interface Element { isElement; }
}

var x4 = <T extends={true}>() => {}</T>;
x4.isElement;

var x5 = <T extends>() => {}</T>;
x5.isElement;
"#;
    let mut parser = ParserState::new("file.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let ts1382_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT)
        .count();

    assert!(
        ts1382_count >= 2,
        "Expected malformed `extends` TSX ambiguity to emit TS1382 on both forms, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected no TS1110 Type expected diagnostics for malformed `extends` JSX ambiguity, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected no TS1109 diagnostics for malformed `extends` JSX ambiguity, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_jsx_and_type_assertion_conformance_codes_exclude_ts1003() {
    let source = r#"
declare var createElement: any;

class foo {}

var x: any;
x = <any> { test: <any></any> };

x = <any><any></any>;
 
x = <foo>hello {<foo>{}} </foo>;

x = <foo test={<foo>{}}>hello</foo>;

x = <foo test={<foo>{}}>hello{<foo>{}}</foo>;

x = <foo>x</foo>, x = <foo/>;

<foo>{<foo><foo>{/foo/.test(x) ? <foo><foo></foo> : <foo><foo></foo>}</foo>}</foo>
"#;
    let mut parser = ParserState::new("jsxAndTypeAssertion.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();
    let malformed_jsx_statement_terminators = [
        "x = <foo>hello {<foo>{}} </foo>;",
        "x = <foo test={<foo>{}}>hello</foo>;",
        "x = <foo test={<foo>{}}>hello{<foo>{}}</foo>;",
    ]
    .into_iter()
    .map(|statement| {
        source
            .find(statement)
            .map(|start| start as u32 + statement.len() as u32 - 1)
            .expect("target JSX statement should exist")
    })
    .collect::<Vec<_>>();

    assert_eq!(
        ts1003_count, 0,
        "Expected no TS1003 for jsxAndTypeAssertion.tsx parser diagnostics, got diagnostics: {diagnostics:?}"
    );
    for semicolon_pos in malformed_jsx_statement_terminators {
        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == diagnostic_codes::EXPECTED
                    && diag.start == semicolon_pos
                    && diag.message == "'}' expected."
            }),
            "Expected TS1005 \"'}}' expected.\" at malformed JSX statement terminator pos {semicolon_pos}, got diagnostics: {diagnostics:?}"
        );
    }
}
#[test]
fn test_type_literal_invalid_member_lt_minus_reports_ts1109_not_ts1128() {
    let source = r#"
var f: {
    x: number;
    <-
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::TYPE_PARAMETER_DECLARATION_EXPECTED),
        "Expected TS1139 from malformed call-signature type parameters, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected TS1109 at the type-literal synchronizing close brace, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected no top-level TS1128 stray-brace cascade for `<-` type-member recovery, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_tsx_fragment_errors_conformance_shape_matches_mismatch_then_eof_sequence() {
    let source = r#"
declare namespace JSX {
	interface Element { }
	interface IntrinsicElements {
		[s: string]: any;
	}
}
declare var React: any;

<>hi</div>

<>eof
"#;
    let mut parser = ParserState::new("file.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT,
            diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
            diagnostic_codes::EXPECTED,
        ],
        "Expected TS17015/TS17014/TS1005 recovery for malformed + EOF JSX fragments, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_tsx_fragment_errors_actual_conformance_file_matches_expected_codes() {
    let fixture_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../TypeScript/tests/cases/conformance/jsx/tsxFragmentErrors.tsx"
    );
    let source = match std::fs::read_to_string(fixture_path) {
        Ok(source) => source,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return;
        }
        Err(err) => {
            panic!("failed to read tsxFragmentErrors conformance fixture {fixture_path}: {err}")
        }
    };
    let mut parser = ParserState::new("file.tsx".to_string(), source);
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT,
            diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
            diagnostic_codes::EXPECTED,
        ],
        "Expected TS17015/TS17014/TS1005 on actual tsxFragmentErrors conformance file, got diagnostics: {diagnostics:?}"
    );
}
#[test]
fn test_tsx_fragment_errors_stripped_source_matches_expected_positions() {
    let source = r#"
declare namespace JSX {
	interface Element { }
	interface IntrinsicElements {
		[s: string]: any;
	}
}
declare var React: any;

<>hi</div> // Error

<>eof   // Error
"#
    .to_string();
    let line_map = LineMap::build(&source);
    let mut parser = ParserState::new("file.tsx".to_string(), source.clone());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<(u32, u32, u32)> = diagnostics
        .iter()
        .filter(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT
                    | diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG
            )
        })
        .map(|diag| {
            let pos = line_map.offset_to_position(diag.start, &source);
            (diag.code, pos.line + 1, pos.character + 1)
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            (
                diagnostic_codes::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT,
                10,
                7,
            ),
            (
                diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
                10,
                11,
            ),
        ],
        "Expected JSX fragment recovery positions to match tsc for tsxFragmentErrors.tsx, got {diagnostics:?}"
    );
}
#[test]
fn test_trailing_decimal_numeric_literal_recovery_matches_conformance_shape() {
    let source = "1.toString();\nvar test2 = 2.toString();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
            diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
            diagnostic_codes::EXPECTED,
            diagnostic_codes::EXPRESSION_EXPECTED,
        ],
        "Trailing-decimal recovery should match the numeric literal conformance shape, got diagnostics: {diagnostics:?}"
    );

    let standalone_identifier_pos = source.find("toString").unwrap();
    assert!(
        diagnostics
            .iter()
            .all(|diag| !(diag.code == diagnostic_codes::EXPECTED
                && diag.start as usize == standalone_identifier_pos)),
        "Standalone `1.toString()` should not emit a spurious missing-semicolon diagnostic: {diagnostics:?}"
    );

    let var_stmt_start = source.find("var test2 = 2.toString();").unwrap();
    let open_paren_pos = var_stmt_start + "var test2 = 2.toString".len();
    let close_paren_pos = open_paren_pos + 1;

    let ts1005 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPECTED)
        .expect("expected TS1005 for the recovered call tail");
    assert_eq!(
        ts1005.start as usize, open_paren_pos,
        "TS1005 should anchor at the opening paren after the recovered identifier tail: {diagnostics:?}"
    );

    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for the empty recovered call expression");
    assert_eq!(
        ts1109.start as usize, close_paren_pos,
        "TS1109 should anchor at the closing paren after the recovered empty call: {diagnostics:?}"
    );
}
#[test]
fn test_decorator_type_assertion_reports_brace_expected_and_expression_expected_at_end_of_type_token()
 {
    let source = "@<[[import(obju2c77,\n";
    let mut parser = ParserState::new(
        "parseUnmatchedTypeAssertion.ts".to_string(),
        source.to_string(),
    );
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_positions: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .map(|diag| diag.start as usize)
        .collect();
    let ts1005_positions: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPECTED)
        .map(|diag| diag.start)
        .collect();

    let ts1109_count = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .count();
    assert_eq!(
        ts1109_count, 1,
        "Decorator type assertion recovery should emit one TS1109 diagnostic at the type assertion start, got {diagnostics:?}"
    );
    assert_eq!(
        ts1109_positions,
        vec![1],
        "TS1109 should anchor at the decorator type assertion start, got positions: {ts1109_positions:?}. Full diagnostics: {diagnostics:?}"
    );

    let ts1005_count = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPECTED)
        .count();
    assert_eq!(
        ts1005_count, 1,
        "Decorator type assertion recovery should emit a single TS1005 for the missing class body brace, got {diagnostics:?}"
    );
    assert_eq!(
        ts1005_positions,
        vec![21],
        "TS1005 should anchor at decorator tail, got positions: {ts1005_positions:?}. Full diagnostics: {diagnostics:?}"
    );
    let ts1146_count = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::DECLARATION_EXPECTED)
        .count();
    assert_eq!(
        ts1146_count, 0,
        "Decorator type assertion recovery should not emit TS1146, got {diagnostics:?}"
    );
}
#[test]
fn test_ts1125_tagged_template_does_not_emit_errors() {
    // Tagged templates (ES2018+) allow invalid escape sequences per spec.
    // tsc does NOT emit TS1125 for tagged templates — only for untagged templates.
    let source =
        r#"const x = tag`\u{hello} ${ 100 } \xtraordinary ${ 200 } wonderful ${ 300 } \uworld`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();

    let ts1125_diagnostics: Vec<_> = diagnostics.iter().filter(|d| d.code == 1125).collect();

    // Tagged templates should NOT get TS1125 errors
    assert_eq!(
        ts1125_diagnostics.len(),
        0,
        "Expected 0 TS1125 errors for tagged template, got {}: {:?}",
        ts1125_diagnostics.len(),
        ts1125_diagnostics
    );
}
#[test]
fn test_ts1125_untagged_template_emits_errors() {
    // Untagged templates with invalid escape sequences DO get TS1125.
    let source =
        r#"const y = `\u{hello} ${ 100 } \xtraordinary ${ 200 } wonderful ${ 300 } \uworld`;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();

    let ts1125_diagnostics: Vec<_> = diagnostics.iter().filter(|d| d.code == 1125).collect();

    // We should get 3 TS1125 errors (for \u{hello}, \xtraordinary, \uworld)
    assert_eq!(
        ts1125_diagnostics.len(),
        3,
        "Expected 3 TS1125 errors (for \\u{{hello}}, \\xtraordinary, \\uworld), got {}: {:?}",
        ts1125_diagnostics.len(),
        ts1125_diagnostics
    );
}
