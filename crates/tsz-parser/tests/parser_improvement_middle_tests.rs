//! Tests for parser improvements to reduce TS1005 and TS2300 false positives

use crate::parser::ParserState;
use crate::parser::test_fixture::{parse_source, parse_source_with_language_version};
use tsz_common::ScriptTarget;
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_type_literal_property_initializer_emits_ts1247() {
    let source = r"
type T = {
    x: number = 1;
};
";
    let (parser, _root) = parse_source(source);

    let ts1247_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1247)
        .count();
    assert_eq!(
        ts1247_count, 1,
        "Expected 1 TS1247 error for type literal property initializer, got {ts1247_count}",
    );
}

// =============================================================================
// Primitive Type Keywords Tests
// =============================================================================

#[test]
fn test_void_return_type() {
    // void return type should be parsed correctly without TS1110/TS1109 errors
    let source = r"
declare function fn(arg0: boolean): void;
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors for void return type
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for void return type, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_type_keywords() {
    // All primitive type keywords should be parsed correctly
    let source = r"
declare function fn1(): void;
declare function fn2(): string;
declare function fn3(): number;
declare function fn4(): boolean;
declare function fn5(): symbol;
declare function fn6(): bigint;
declare function fn7(): any;
declare function fn8(): unknown;
declare function fn9(): never;
declare function fn10(): null;
declare function fn11(): undefined;
declare function fn12(): object;
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors for primitive type keywords
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive type keywords, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_types_in_type_aliases() {
    // Primitive type keywords should work in type aliases
    let source = r"
type T1 = void;
type T2 = string;
type T3 = number;
type T4 = boolean;
type T5 = any;
type T6 = unknown;
type T7 = never;
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive types in type aliases, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_types_in_parameters() {
    // Primitive type keywords should work in parameter types
    let source = r"
declare function fn(a: void, b: string, c: number): boolean;
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive types in parameters, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_types_in_arrow_functions() {
    // Primitive type keywords should work in arrow function types
    let source = r#"
const arrow1: () => void = () => {};
const arrow2: (x: number) => string = (x) => "";
"#;
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive types in arrow functions, got {:?}",
        parser.get_diagnostics()
    );
}

// =============================================================================
// Incremental Parsing Tests
// =============================================================================

#[test]
fn test_incremental_parse_from_middle_of_file() {
    // Test parsing from an offset in the middle of a source file
    let source = r"const a = 1;
const b = 2;
function foo() {
    return a + b;
}
const c = 3;";

    // Parse from the start of "function foo()"
    let offset = u32::try_from(
        source
            .find("function")
            .expect("pattern should exist in source"),
    )
    .expect("function offset should fit in u32");

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        offset,
    );

    // Should have parsed the remaining statements (function and const c)
    let statement_count = result.statements.len();
    assert!(
        statement_count >= 2,
        "Expected at least 2 statements from offset, got {statement_count}",
    );

    // Should not produce errors for valid code
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors for incremental parse, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_incremental_parse_from_start() {
    // Test incremental parsing from offset 0 (should be equivalent to full parse)
    let source = r#"const x = 42;
let y = "hello";"#;

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        0,
    );

    // Should have parsed both statements
    let statement_count = result.statements.len();
    assert_eq!(
        statement_count, 2,
        "Expected 2 statements, got {statement_count}",
    );

    // reparse_start should be 0
    assert_eq!(result.reparse_start, 0);

    // Should not produce errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_incremental_parse_from_end() {
    // Test incremental parsing from beyond the end of file
    let source = "const x = 1;";

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        1000, // Beyond EOF
    );

    // Should handle gracefully - clamped to source length
    assert!(
        result.statements.is_empty(),
        "Expected no statements when starting at EOF"
    );
}

#[test]
fn test_incremental_parse_records_reparse_start() {
    // Test that reparse_start is recorded correctly
    let source = "const a = 1;\nconst b = 2;";
    let offset = 13u32; // Start of "const b"

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        offset,
    );

    // reparse_start should match the offset we provided
    let reparse_start = result.reparse_start;
    assert_eq!(
        reparse_start, offset,
        "Expected reparse_start to be {offset}, got {reparse_start}",
    );
}

#[test]
fn test_incremental_parse_with_syntax_error() {
    // Test incremental parsing recovers from syntax errors
    let source = r"const a = 1;
const b = ;
const c = 3;";

    // Parse from start of "const b = ;" (syntax error)
    let offset = u32::try_from(
        source
            .find("const b")
            .expect("pattern should exist in source"),
    )
    .expect("const b offset should fit in u32");

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        offset,
    );

    // Should still parse statements (with recovery)
    let statement_count = result.statements.len();
    assert!(
        !result.statements.is_empty(),
        "Expected at least 1 statement after recovery, got {statement_count}",
    );

    // Should produce an error for the syntax issue
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected at least one diagnostic for syntax error"
    );
}

// =============================================================================
// Conditional Type ASI Tests
// =============================================================================

#[test]
fn test_interface_extends_property_with_asi() {
    // 'extends' as a property name in interface with ASI (no semicolons)
    // Should NOT parse as conditional type
    let source = r"
interface JSONSchema4 {
  a?: number
  extends?: string | string[]
}
";
    let (parser, _root) = parse_source(source);

    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parser errors for 'extends' property with ASI, got {diags:?}",
    );
}

// =============================================================================
// Expression Statement Recovery Tests
// =============================================================================

#[test]
fn test_incomplete_binary_expression_recovery() {
    // Test recovery from incomplete binary expression: a +
    let source = r"const result = a +;
const next = 1;";

    let (parser, _root) = parse_source(source);

    // Should produce an error for missing RHS
    let has_error = !parser.get_diagnostics().is_empty();
    assert!(has_error, "Expected error for incomplete binary expression");

    // Parser should recover and continue parsing
    // The error count should be limited (no cascading errors)
    let error_count = parser.get_diagnostics().len();
    assert!(
        error_count <= 2,
        "Expected at most 2 errors for recovery, got {error_count}",
    );
}

#[test]
fn test_incomplete_assignment_recovery() {
    // Test recovery from incomplete assignment: x =
    let source = r"let x =;
let y = 2;";

    let (parser, _root) = parse_source(source);

    // Should produce an error for missing RHS
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected error for incomplete assignment"
    );

    // Parser should recover - not too many errors
    let error_count = parser.get_diagnostics().len();
    assert!(
        error_count <= 2,
        "Expected at most 2 errors after recovery, got {error_count}",
    );
}

#[test]
fn test_incomplete_conditional_expression_recovery() {
    // Test recovery from incomplete conditional: a ? b :
    let source = r"const result = a ? b :;
const next = 1;";

    let (parser, _root) = parse_source(source);

    // Should produce error for missing false branch
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected error for incomplete conditional"
    );
}

#[test]
fn test_expression_recovery_at_statement_boundary() {
    // Test that parser properly recovers at statement boundaries
    let source = r"const a = 1 +
const b = 2;";

    let (parser, _root) = parse_source(source);

    // Should have errors but recover for next statement
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected error for incomplete expression"
    );
}

#[test]
fn test_expression_recovery_preserves_valid_code() {
    // Test that valid code after error is still parsed correctly
    let source = r"const bad = ;
function validFunction() {
    return 42;
}";

    let (parser, _root) = parse_source(source);

    // Should have error for bad assignment
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected error for invalid assignment"
    );

    // Error count should be limited
    let error_count = parser.get_diagnostics().len();
    assert!(
        error_count <= 2,
        "Expected limited errors with recovery, got {error_count}",
    );
}

// =============================================================================
// Import Type Tests
// =============================================================================

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
fn test_type_argument_with_empty_jsdoc_wildcard_has_no_ts1110() {
    // `Foo<?>` should emit TS8020 but avoid TS17020/TS1110 cascading.
    let source = r#"
type T = Foo<?>;
"#;
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
fn test_type_argument_with_jsdoc_prefix_type_simplifies_ts17020_suggestion() {
    let source = r#"
type T = Foo<?undefined>;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostic = parser
        .get_diagnostics()
        .iter()
        .find(|d| d.code == 17020)
        .unwrap_or_else(|| {
            panic!(
                "Expected TS17020 for `Foo<?undefined>`, got {:?}",
                parser.get_diagnostics()
            )
        });
    assert_eq!(
        diagnostic.message,
        "'?' at the start of a type is not valid TypeScript syntax. Did you mean to write 'null | undefined'?"
    );
}

#[test]
fn test_expression_type_argument_with_empty_jsdoc_wildcard_emits_ts8020_only() {
    let source = r#"
const WhatFoo = foo<?>;
"#;
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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

    let (parser, _root) = parse_source(source);

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
fn test_argument_list_colon_followed_by_var_keyword_emits_ts1135() {
    // Regression for `f(x: var ...)` parser recovery.
    //
    // tsc emits:
    //   - TS1005 ',' expected at the spurious `:`
    //   - TS1135 "Argument expression expected." at `var`
    //   - TS1134 "Variable declaration expected." at `(`
    // The keyword should also break the argument list so the outer statement
    // parser can keep recovering. This prevents earlier behaviour where the
    // colon branch tried to parse `var` as a type, followed by another TS1005
    // ',' expected at `(`.
    let source = "f(x: var (--a)\n);";

    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1135 = diagnostics.iter().filter(|d| d.code == 1135).count();
    let ts1134 = diagnostics.iter().filter(|d| d.code == 1134).count();
    let ts1110 = diagnostics.iter().filter(|d| d.code == 1110).count();

    assert!(
        ts1135 >= 1,
        "Expected TS1135 'Argument expression expected.' at `var`, got: {diagnostics:?}"
    );
    assert!(
        ts1134 >= 1,
        "Expected TS1134 'Variable declaration expected.' downstream of `var (`, got: {diagnostics:?}"
    );
    assert_eq!(
        ts1110, 0,
        "Expected no TS1110 'Type expected.' (the colon branch must not parse `var` as a type), got: {diagnostics:?}"
    );

    // Ensure we don't double-report `,` expected at `:` and at `(` of the
    // call site (the previous bug emitted both).
    let ts1005_at_paren = diagnostics
        .iter()
        .filter(|d| d.code == 1005)
        .filter(|d| {
            let pos = d.start as usize;
            pos < source.len() && &source[pos..=pos] == "("
        })
        .count();
    assert_eq!(
        ts1005_at_paren, 0,
        "TS1005 should not be emitted at `(` of `var (...)` after recovery, got: {diagnostics:?}"
    );
}

#[test]
fn test_invalid_unicode_escape_in_var_no_extra_semicolon_error() {
    let source = r"var arg\uxxxx";
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
fn test_escaped_combining_mark_as_variable_name_reports_ts1127() {
    let source = r"var \u0345 = 1;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 1127 && d.start == source.find('\\').unwrap() as u32),
        "Expected TS1127 at escaped combining mark, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn invalid_escaped_private_use_identifier_part_reports_ts1127() {
    let source = r"var _\uD4A5\u7204\uC316\uE59F = local;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let invalid_escape = source.find(r"\uE59F").expect("invalid escape") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 1127 && d.start == invalid_escape),
        "Expected TS1127 at escaped private-use identifier part, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn invalid_surrogate_unicode_escapes_in_class_member_emit_ts1127() {
    let source = r"class C { \uD800\uDEA7: string; }";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let first_escape = source.find(r"\uD800").expect("first escape") as u32;
    let second_escape = source.find(r"\uDEA7").expect("second escape") as u32;
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, first_escape),
            (diagnostic_codes::INVALID_CHARACTER, second_escape),
        ],
        "invalid surrogate escapes in class member names should report scanner-shaped TS1127 diagnostics, got {diagnostics:?}",
    );
}

#[test]
fn invalid_surrogate_unicode_escapes_in_import_alias_emit_ts1127_without_cascade() {
    let source = r#"import { foo as \uD800\uDEA7 } from "mod";"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let first_escape = source.find(r"\uD800").expect("first escape") as u32;
    let second_escape = source.find(r"\uDEA7").expect("second escape") as u32;
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, first_escape),
            (diagnostic_codes::INVALID_CHARACTER, second_escape),
        ],
        "invalid surrogate escapes in import aliases should report scanner-shaped TS1127 diagnostics without parser cascades, got {diagnostics:?}",
    );
}

#[test]
fn es5_import_specifier_identifier_tail_reports_invalid_astral_without_comma_cascade() {
    let source = r#"import { _𐊧 as \uD800\uDEA7 } from "mod";"#;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let raw_astral = source.find('𐊧').expect("raw astral") as u32;
    let first_escape = source.find(r"\uD800").expect("first escape") as u32;
    let second_escape = source.find(r"\uDEA7").expect("second escape") as u32;
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, raw_astral),
            (diagnostic_codes::INVALID_CHARACTER, first_escape),
            (diagnostic_codes::INVALID_CHARACTER, second_escape),
        ],
        "ES5 import specifier invalid identifier tails should report scanner-shaped TS1127 diagnostics without comma recovery cascades, got {diagnostics:?}",
    );
}

#[test]
fn es5_astral_identifier_chars_recover_as_invalid_declaration_tail() {
    let source = "export var _𐊧 = new Foo();";
    let astral_pos = source.find('𐊧').expect("astral identifier char") as u32;
    let equals_pos = source.find('=').expect("equals") as u32;
    let new_pos = source.find("new").expect("new keyword") as u32;

    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::INVALID_CHARACTER && d.start == astral_pos),
        "ES5 astral identifier char must emit TS1127 at its source position, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(
            |d| d.code == diagnostic_codes::VARIABLE_DECLARATION_EXPECTED && d.start == equals_pos
        ),
        "ES5 astral identifier recovery must keep `=` visible for TS1134, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|d| d.code
            == diagnostic_codes::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME
            && d.start == new_pos),
        "ES5 astral identifier recovery must report TS1389 at `new`, got {diagnostics:?}"
    );
}

#[test]
fn es2015_astral_identifier_chars_remain_valid_identifier_parts() {
    let source = "export var _𐊧 = new Foo();";
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES2015);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::INVALID_CHARACTER),
        "ES2015 astral identifier chars should remain valid identifier parts, got {diagnostics:?}"
    );
}

#[test]
fn es2015_braced_astral_escape_remains_valid_identifier_start() {
    let source = r"export var \u{102A7} = new Foo();";
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES2015);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::INVALID_CHARACTER),
        "ES2015 braced astral identifier escape should scan as a valid identifier start, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_escape_remains_invalid_identifier_start() {
    let source = r"export var \u{102A7} = new Foo();";
    let escape_pos = source.find(r"\u{102A7}").expect("unicode escape") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::INVALID_CHARACTER && d.start == escape_pos),
        "ES5 braced astral identifier escape should report TS1127 at the escape, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_escape_after_identifier_recovers_inside_variable_list() {
    let source = r"export var _\u{102A7} = new Foo();";
    let escape_pos = source.find('\\').expect("unicode escape") as u32;
    let open_brace_pos = source.find('{').expect("open brace") as u32;
    let numeric_tail_pos = source.find("A7").expect("numeric literal tail") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, escape_pos),
            (diagnostic_codes::EXPECTED, open_brace_pos),
            (
                diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                numeric_tail_pos,
            ),
        ],
        "ES5 escaped astral identifier tail should recover like tsc, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_escape_after_identifier_recovers_across_same_line_trivia() {
    let source = r"export var _ /*tail*/ \u{102A7} = new Foo();";
    let escape_pos = source.find('\\').expect("unicode escape") as u32;
    let open_brace_pos = source.find('{').expect("open brace") as u32;
    let numeric_tail_pos = source.find("A7").expect("numeric literal tail") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, escape_pos),
            (diagnostic_codes::EXPECTED, open_brace_pos),
            (
                diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                numeric_tail_pos,
            ),
        ],
        "same-line trivia before escaped astral debris should recover like tsc, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_escape_after_import_alias_recovers_as_specifier_tail() {
    let source = r#"import { _x as _\u{102A7} } from "mod";"#;
    let escape_pos = source.find('\\').expect("unicode escape") as u32;
    let open_brace_pos = source.find(r"\u{102A7}").expect("unicode escape") as u32 + 2;
    let numeric_tail_pos = source.find("A7").expect("numeric literal tail") as u32;
    let close_brace_pos = source.find("} from").expect("specifier close brace") as u32;
    let from_pos = source.find("from").expect("from keyword") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, escape_pos),
            (diagnostic_codes::EXPECTED, open_brace_pos),
            (
                diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                numeric_tail_pos,
            ),
            (
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                close_brace_pos,
            ),
            (
                diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                from_pos,
            ),
        ],
        "import alias escaped astral tail should recover like tsc, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_escape_after_export_alias_recovers_as_specifier_tail() {
    let source = r#"export { _x as _\u{102A7} } from "mod";"#;
    let escape_pos = source.find('\\').expect("unicode escape") as u32;
    let open_brace_pos = source.find(r"\u{102A7}").expect("unicode escape") as u32 + 2;
    let numeric_tail_pos = source.find("A7").expect("numeric literal tail") as u32;
    let close_brace_pos = source.find("} from").expect("specifier close brace") as u32;
    let from_pos = source.find("from").expect("from keyword") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert_eq!(
        actual,
        vec![
            (diagnostic_codes::INVALID_CHARACTER, escape_pos),
            (diagnostic_codes::EXPECTED, open_brace_pos),
            (
                diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                numeric_tail_pos,
            ),
            (
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                close_brace_pos,
            ),
            (
                diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                from_pos,
            ),
        ],
        "export alias escaped astral tail should recover like tsc, got {diagnostics:?}"
    );
}

#[test]
fn reset_clears_braced_unicode_specifier_tail_recovery_state() {
    let source = r#"import { _x as _\u{102A7} } from "mod";"#;
    let (mut parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);
    assert!(
        parser.current_specifier_recovered_braced_unicode_escape_debris,
        "sanity check: first parse should exercise braced unicode specifier recovery"
    );

    parser.reset(
        "test.ts".to_string(),
        r#"import { value } from "mod";"#.to_string(),
    );

    assert!(
        !parser.current_specifier_recovered_braced_unicode_escape_debris,
        "reset should clear stale specifier recovery state"
    );
}

#[test]
fn es5_raw_astral_variable_name_reports_declaration_expected_at_type_tail() {
    let source = "declare var 𐊧: string;";
    let raw_astral = source.find('𐊧').expect("raw astral") as u32;
    let colon_pos = source.find(':').expect("colon") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert!(
        actual.contains(&(diagnostic_codes::INVALID_CHARACTER, raw_astral)),
        "ES5 raw astral declaration name should report TS1127 at the astral character, got {diagnostics:?}"
    );
    assert!(
        actual.contains(&(diagnostic_codes::VARIABLE_DECLARATION_EXPECTED, colon_pos)),
        "ES5 raw astral declaration recovery should report TS1134 at the type tail, got {diagnostics:?}"
    );
    assert!(
        !actual.contains(&(
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            colon_pos
        )),
        "ES5 raw astral declaration recovery should not reclassify the type tail as TS1128, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_variable_name_reports_missing_comma_before_recovered_identifier() {
    let source = r"declare var \u{102A7}: string;";
    let escape_pos = source.find('\\').expect("unicode escape") as u32;
    let recovered_open_brace = source.find('{').expect("recovered open brace") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert!(
        actual.contains(&(diagnostic_codes::INVALID_CHARACTER, escape_pos)),
        "ES5 braced astral declaration name should report TS1127 at the escape, got {diagnostics:?}"
    );
    assert!(
        actual.contains(&(diagnostic_codes::EXPECTED, recovered_open_brace)),
        "ES5 braced astral declaration recovery should report TS1005 at the recovered braced tail, got {diagnostics:?}"
    );
    assert!(
        !actual.contains(&(diagnostic_codes::EXPECTED, escape_pos + 1)),
        "ES5 braced astral declaration recovery should not emit a duplicate TS1005 before the recovered identifier, got {diagnostics:?}"
    );
}

#[test]
fn es5_raw_astral_statement_assignment_reports_statement_expected_at_equals() {
    let source = "if (true) { 𐊧 = \"hello\"; }";
    let raw_astral = source.find('𐊧').expect("raw astral") as u32;
    let equals_pos = source.find('=').expect("equals") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert!(
        actual.contains(&(diagnostic_codes::INVALID_CHARACTER, raw_astral)),
        "ES5 raw astral statement assignment should report TS1127 at the astral character, got {diagnostics:?}"
    );
    assert!(
        actual.contains(&(
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            equals_pos
        )),
        "ES5 raw astral statement assignment should report TS1128 at the assignment tail, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_statement_assignment_recovers_block_followed_by_equals() {
    let source = r#"if (true) { \u{102A7} = "hallo"; }"#;
    let escape_pos = source.find('\\').expect("unicode escape") as u32;
    let recovered_identifier = escape_pos + 1;
    let equals_pos = source.find('=').expect("equals") as u32;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();

    assert!(
        actual.contains(&(diagnostic_codes::INVALID_CHARACTER, escape_pos)),
        "ES5 braced astral statement assignment should report TS1127 at the escape, got {diagnostics:?}"
    );
    assert!(
        actual.contains(&(
            diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
            recovered_identifier
        )),
        "ES5 braced astral statement assignment should report TS1434 at the recovered identifier tail, got {diagnostics:?}"
    );
    assert!(
        actual.contains(&(
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED_THIS_FOLLOWS_A_BLOCK_OF_STATEMENTS_SO_IF_YOU_I,
            equals_pos
        )),
        "ES5 braced astral statement assignment should recover the braced tail as a block and report TS2809 at `=`, got {diagnostics:?}"
    );
}

#[test]
fn es2015_braced_astral_escape_remains_valid_in_class_and_member_access() {
    let source = r#"
class Foo {
    \u{102A7}: string;
    constructor() {
        this.\u{102A7} = " world";
    }
    methodA() {
        return this.\u{102A7};
    }
}
export var _\u{102A7} = new Foo().\u{102A7};
"#;
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES2015);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::INVALID_CHARACTER),
        "ES2015 braced astral identifier escapes should remain valid across declarations, class members, and member access, got {diagnostics:?}"
    );
}

#[test]
fn es5_braced_astral_escape_reports_invalid_character_across_identifier_contexts() {
    let source = r#"
class Foo {
    \u{102A7}: string;
    constructor() {
        this.\u{102A7} = " world";
    }
}
export var _\u{102A7} = new Foo().\u{102A7};
"#;
    let expected_escape_positions: Vec<_> = source
        .match_indices(r"\u{102A7}")
        .map(|(pos, _)| pos as u32)
        .collect();
    let (parser, _root) = parse_source_with_language_version(source, ScriptTarget::ES5);

    let diagnostics = parser.get_diagnostics();
    for escape_pos in expected_escape_positions {
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::INVALID_CHARACTER && d.start == escape_pos),
            "ES5 braced astral identifier escape should report TS1127 at {escape_pos}, got {diagnostics:?}"
        );
    }
}

#[test]
fn test_class_method_string_names_use_string_literal_nodes() {
    let source = r#"
class C {
    "foo"();
    "bar"() { }
}
"#;
    let (parser, root) = parse_source(source);
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
    let (parser, _root) = parse_source(source);

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

/// `<div>` followed by a Git merge conflict marker (and EOF) parses as a
/// type assertion in `.ts` context. tsc anchors the missing-expression
/// TS1109 at the position right after `>`, not at EOF after the conflict
/// marker.
#[test]
fn test_type_assertion_missing_operand_anchors_at_after_gt_after_conflict_marker() {
    let source = "const x = <div>\n<<<<<<< HEAD";
    let (parser, _root) = parse_source(source);
    let ts1109: Vec<_> = parser
        .parse_diagnostics
        .iter()
        .filter(|d| d.code == 1109)
        .collect();
    assert_eq!(
        ts1109.len(),
        1,
        "Expected exactly one TS1109, got: {ts1109:?}",
    );
    let after_gt = source.find("<div>").unwrap() as u32 + "<div>".len() as u32;
    let actual_start = ts1109[0].start;
    assert_eq!(
        actual_start, after_gt,
        "TS1109 must anchor at end of `<div>` (offset {after_gt}), got offset {actual_start}",
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
    let (parser, _root) = parse_source(source);

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
    let (parser, root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

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
