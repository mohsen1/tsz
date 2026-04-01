//! Tests for statement parsing in the parser.
use crate::parser::{NodeIndex, ParserState};
use tsz_common::diagnostics::diagnostic_codes;

fn parse_source(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn assert_function_body_recovery_uses_statement_errors(source: &str) {
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 for the missing `(`, got {diags:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "expected downstream TS1109 from the malformed body statement, got {diags:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "expected TS1128 from `static` statement recovery, got {diags:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED),
        "should not parse the function body as an object/parameter list, got {diags:?}"
    );
}

#[test]
fn parse_statement_recovery_on_malformed_top_level_diagnostics() {
    let (parser, root) = parse_source("const x = 1\nconst y = ;\nconst z = 3;");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    assert!(sf.statements.nodes.len() >= 2);
    assert!(!parser.get_diagnostics().is_empty());
}

#[test]
fn parse_static_block_statement_is_supported() {
    let (parser, root) =
        parse_source("class Holder {\n    static {\n        const v = 1;\n    }\n}\nconst ok = 1;");
    assert_eq!(parser.get_diagnostics().len(), 0);
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    assert_eq!(sf.statements.nodes.len(), 2);
}

#[test]
fn parse_with_statement_with_recovery_when_expression_missing() {
    let (parser, _root) = parse_source("with () {}\nconst ok = 1;");
    assert!(!parser.get_diagnostics().is_empty());
}

#[test]
fn if_statement_with_invalid_character_and_asterisk_reports_ts1127_and_ts1109() {
    let source = "class C {\n  foo() {\n    if (a) ¬ * bar;\n  }\n}";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::INVALID_CHARACTER),
        "expected TS1127 for the invalid character before `*`, got {diags:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "expected TS1109 for malformed `*` body, got {diags:?}"
    );
    assert!(
        !codes
            .contains(&diagnostic_codes::THE_BODY_OF_AN_IF_STATEMENT_CANNOT_BE_THE_EMPTY_STATEMENT),
        "should not emit TS1313 for this pattern, got {diags:?}"
    );
}

#[test]
fn function_declaration_missing_open_paren_recovers_into_body() {
    assert_function_body_recovery_uses_statement_errors(
        "function boo {\n  static test()\n  static test(name: string)\n  static test(name?: any) {}\n}\nconst ok = 1;",
    );
}

#[test]
fn function_declaration_missing_open_paren_keeps_downstream_ts1005s() {
    let source = "function boo {\n  static test()\n  static test(name: string)\n  static test(name?: any) {}\n}";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let colon_pos = source.find(": string").expect("type annotation") as u32;
    let body_pos = source.rfind('{').expect("body brace") as u32;

    assert!(
        diags
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPECTED
                && diag.start == colon_pos
                && diag.message == "',' expected."),
        "expected TS1005 ',' expected at the parameter type annotation, got {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPECTED
                && diag.start == body_pos
                && diag.message == "';' expected."),
        "expected TS1005 ';' expected at the trailing body brace, got {diags:?}"
    );
}

#[test]
fn function_expression_missing_open_paren_recovers_into_body() {
    assert_function_body_recovery_uses_statement_errors(
        "const f = function boo {\n  static test()\n  static test(name: string)\n  static test(name?: any) {}\n};\nconst ok = 1;",
    );
}

#[test]
fn export_default_function_missing_open_paren_recovers_into_body() {
    assert_function_body_recovery_uses_statement_errors(
        "export default function {\n  static test()\n  static test(name: string)\n  static test(name?: any) {}\n}\nconst ok = 1;",
    );
}

#[test]
fn parse_block_followed_by_equals_emits_ts2809_instead_of_ts1128() {
    let (parser, _root) = parse_source(
        r#"
declare function fn(): { a: 1, b: 2 }
let a: number;
let b: number;

{ a, b } = fn();
{ a, b }
= fn();
"#,
    );
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    let ts2809_count = codes
        .iter()
        .filter(|&&code| {
            code
                == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED_THIS_FOLLOWS_A_BLOCK_OF_STATEMENTS_SO_IF_YOU_I
        })
        .count();
    assert_eq!(
        ts2809_count, 2,
        "expected two TS2809 diagnostics, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "should not fall back to generic TS1128, got {codes:?}"
    );
}

#[test]
fn variable_declaration_recovery_prefers_ts1134_over_regex_tail_ts1161() {
    let (parser, _root) = parse_source("var v = /[]/]/");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 from the malformed declaration tail, got {codes:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::VARIABLE_DECLARATION_EXPECTED),
        "expected TS1134 at the trailing slash, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::UNTERMINATED_REGULAR_EXPRESSION_LITERAL),
        "should not reparse the trailing slash as a fresh regex literal, got {codes:?}"
    );
}

#[test]
fn variable_declaration_recovery_prefers_ts1134_over_negated_regex_tail_ts1161() {
    let (parser, _root) = parse_source("var v = /[^]/]/");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 from the malformed declaration tail, got {codes:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::VARIABLE_DECLARATION_EXPECTED),
        "expected TS1134 at the trailing slash, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::UNTERMINATED_REGULAR_EXPRESSION_LITERAL),
        "should not reparse the trailing slash as a fresh regex literal, got {codes:?}"
    );
}

#[test]
fn parse_invalid_import_non_clause_start_reports_ts1128() {
    // `import 10;` — `10` is not a valid import clause start (not an identifier,
    // not `*`, `{`, `type`, or `defer`). tsc emits TS1128 "Declaration or statement
    // expected" because `import` followed by a non-import-clause token is treated
    // as an invalid statement, not an invalid expression.
    let (parser, _root) = parse_source("import 10;");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "expected TS1128 for import followed by non-clause token, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "should not emit TS1109 for non-clause import, got {codes:?}"
    );
}

#[test]
fn parse_import_with_operator_reports_ts1128() {
    // `import + x;` — operator after import can't start an import clause.
    let (parser, _root) = parse_source("import + x;");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "expected TS1128 for import followed by operator, got {codes:?}"
    );
}

#[test]
fn parse_import_string_literal_still_works() {
    // `import "module";` — valid import declaration, should produce no TS1128.
    let (parser, _root) = parse_source("import \"module\";");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "valid import should not emit TS1128, got {codes:?}"
    );
}

#[test]
fn parse_import_identifier_from_still_works() {
    // `import x from "module";` — valid default import.
    let (parser, _root) = parse_source("import x from \"module\";");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "valid default import should not emit TS1128, got {codes:?}"
    );
}

#[test]
fn function_signature_invalid_character_prefers_ts1127_over_ts1144() {
    let (parser, _root) = parse_source("function Foo() ¬ { }");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::INVALID_CHARACTER),
        "expected TS1127 for invalid character after function signature, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::OR_EXPECTED),
        "should not fall back to TS1144 after the invalid character, got {codes:?}"
    );
}

#[test]
fn parameter_list_colon_start_prefers_ts1138_over_ts1003() {
    let (parser, _root) = parse_source("namespace M {\nfunction a(\n    : T) { }\n}");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::PARAMETER_DECLARATION_EXPECTED),
        "expected TS1138 for colon-led parameter recovery, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "should not emit generic TS1003 for the colon-led parameter, got {codes:?}"
    );
}

#[test]
fn parse_mid_file_shebang_reports_ts18026_and_argument_semicolon_error() {
    let (parser, _root) = parse_source("var foo = 1;\n#!/usr/bin/env node\n");
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::CAN_ONLY_BE_USED_AT_THE_START_OF_A_FILE),
        "expected TS18026 for mid-file shebang, got {codes:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 for shebang argument recovery, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "should not fall back to TS1128, got {codes:?}"
    );
    assert!(
        !codes.contains(&1499),
        "should not emit regex flag errors, got {codes:?}"
    );
}

#[test]
fn parse_template_recovery_preserves_follow_up_statement() {
    let (parser, root) = parse_source("const bad = `head${1 + 2`;\nconst ok = 1;");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();

    assert!(!sf.statements.nodes.is_empty());
    assert!(!parser.get_diagnostics().is_empty() || !sf.statements.nodes.is_empty());
}

#[test]
fn parse_return_statement_outside_function_recovers_and_continues() {
    let (parser, root) = parse_source("return;\nconst ok = 1;");
    let sf = parser.get_arena().get_source_file_at(root).unwrap();

    assert!(!sf.statements.nodes.is_empty());
}

#[test]
fn parse_index_signature_optional_param_emits_ts1019() {
    let (parser, _root) = parse_source("interface Foo { [p2?: string]; }");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    // Should emit TS1019 (optional param in index sig), NOT TS1109 (expression expected)
    assert!(
        codes.contains(&1019),
        "Expected TS1019, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&1109),
        "Should NOT emit TS1109, got codes: {codes:?}"
    );
}

#[test]
fn parse_index_signature_rest_param_emits_ts1017() {
    let (parser, _root) = parse_source("interface Foo { [...p3: any[]]; }");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1017),
        "Expected TS1017, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&1109),
        "Should NOT emit TS1109, got codes: {codes:?}"
    );
}

#[test]
fn parse_reserved_word_as_var_name_emits_ts1389() {
    // TS1389: '{0}' is not allowed as a variable declaration name.
    // tsc emits TS1389 (not TS1359) when a reserved word is used as a var declaration name.
    let (parser, _root) = parse_source("var export;");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1389),
        "Expected TS1389 for 'var export;', got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&1359),
        "Should NOT emit TS1359 (generic reserved word), got codes: {codes:?}"
    );
}

#[test]
fn parse_reserved_word_as_const_name_emits_ts1389() {
    let (parser, _root) = parse_source("const class = 1;");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1389),
        "Expected TS1389 for 'const class = 1;', got codes: {codes:?}"
    );
}

#[test]
fn parse_reserved_word_as_let_name_emits_ts1389() {
    let (parser, _root) = parse_source("let typeof = 10;");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1389),
        "Expected TS1389 for 'let typeof = 10;', got codes: {codes:?}"
    );
}

#[test]
fn parse_contextual_keyword_as_var_name_no_ts1389() {
    // Contextual keywords (type, interface, etc.) should NOT trigger TS1389
    // — they're valid as variable names.
    let (parser, _root) = parse_source("var type = 1;");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1389),
        "Contextual keyword 'type' should NOT trigger TS1389, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&1359),
        "Contextual keyword 'type' should NOT trigger TS1359, got codes: {codes:?}"
    );
}

#[test]
fn reserved_word_tail_after_missing_comma_in_type_annotation_stops_after_ts1389() {
    let source = "class C {\n  foo() {\n    const x: \"\".typeof(this.foo);\n  }\n}";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();

    let comma_pos = source.find(".typeof").expect("dot before typeof") as u32;
    let typeof_pos = comma_pos + 1;
    let open_paren_pos = source.find("typeof(").expect("call tail after typeof") as u32 + 6;

    assert!(
        diags.iter().any(|diag| {
            diag.code == diagnostic_codes::EXPECTED
                && diag.start == comma_pos
                && diag.message == "',' expected."
        }),
        "Expected TS1005 ',' expected at the dot before typeof, got {diags:?}"
    );
    assert!(
        diags.iter().any(|diag| {
            diag.code == diagnostic_codes::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME
                && diag.start == typeof_pos
        }),
        "Expected TS1389 at the typeof token, got {diags:?}"
    );
    assert!(
        !diags.iter().any(|diag| {
            diag.code == diagnostic_codes::EXPECTED
                && diag.start == open_paren_pos
                && diag.message == "';' expected."
        }),
        "Recovery should skip the typeof call tail instead of emitting a stray ';' expected at `(`: {diags:?}"
    );
}

#[test]
fn member_call_tail_after_missing_comma_in_type_annotation_emits_second_comma_error() {
    let source = "declare const x: \"foo\".charCodeAt(0);";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();

    let dot_pos = source.find(".charCodeAt").expect("dot before charCodeAt") as u32;
    let open_paren_pos = source.find("charCodeAt(").expect("call tail") as u32 + 10;

    let comma_diags: Vec<_> = diags
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPECTED && diag.message == "',' expected.")
        .collect();

    assert_eq!(
        comma_diags.len(),
        2,
        "Expected exactly two TS1005 ',' expected diagnostics, got {diags:?}"
    );
    assert!(
        comma_diags.iter().any(|diag| diag.start == dot_pos),
        "Expected TS1005 ',' expected at the dot before charCodeAt, got {diags:?}"
    );
    assert!(
        comma_diags.iter().any(|diag| diag.start == open_paren_pos),
        "Expected TS1005 ',' expected at the opening paren of charCodeAt, got {diags:?}"
    );
}

#[test]
fn class_field_initializer_does_not_asi_before_computed_member() {
    let (parser, _root) = parse_source("class C {\n    [e]: number = 0\n    [e2]: number\n}");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 for missing semicolon before computed member, got {diags:?}"
    );
    assert!(
        !codes.contains(&1068),
        "should recover as a semicolon error, not TS1068, got {diags:?}"
    );
}

#[test]
fn class_field_initializer_comma_continuation_prefers_semicolon_error() {
    let source =
        "class Game {\n    private position = new DisplayPosition([), 3, 3, 0], NoMove, 0);\n}";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let first_comma = source.find(", 3").expect("first comma") as u32;

    assert!(
        diags
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPECTED
                && diag.start == first_comma
                && diag.message == "';' expected."),
        "expected TS1005 at the first comma after the malformed initializer, got {diags:?}"
    );
    assert!(
        !diags.iter().any(|diag| {
            diag.code
                == diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
                && diag.start == first_comma
        }),
        "should not recover the first comma as TS1068, got {diags:?}"
    );
}

#[test]
fn invalid_var_like_class_member_does_not_emit_keyword_suggestion_cascade() {
    let (parser, _root) = parse_source(
        "class C {\n    public const var export foo = 10;\n\n    var constructor() { }\n}",
    );
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::VARIABLE_DECLARATION_NOT_ALLOWED_AT_THIS_LOCATION),
        "expected TS1440 on invalid class member var recovery, got {diags:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::UNKNOWN_KEYWORD_OR_IDENTIFIER_DID_YOU_MEAN),
        "should not emit TS1435 after TS1440 var-like class member recovery, got {diags:?}"
    );
}

#[test]
fn modifier_led_nested_class_member_recovery_prefers_ts1068_and_ts1128() {
    for source in [
        "class C {\n  public class D {\n}\n}",
        "class C {\n  public enum E {\n}\n}",
    ] {
        let (parser, _root) = parse_source(source);
        let diags = parser.get_diagnostics();
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(
                &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
            ),
            "expected TS1068 for {source:?}, got {diags:?}"
        );
        assert!(
            codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
            "expected TS1128 for {source:?}, got {diags:?}"
        );
        assert!(
            !codes.contains(&diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER),
            "should not emit TS1434 after modifier-led nested declaration recovery for {source:?}, got {diags:?}"
        );
        assert!(
            !codes.contains(&diagnostic_codes::UNKNOWN_KEYWORD_OR_IDENTIFIER_DID_YOU_MEAN),
            "should not emit TS1435 after modifier-led nested declaration recovery for {source:?}, got {diags:?}"
        );
    }
}

#[test]
fn modifier_led_try_block_in_class_body_prefers_ts1068() {
    let (parser, _root) = parse_source("class C {\n  public try {\n  }\n}");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(
            &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        ),
        "expected TS1068 for modifier-led try recovery, got {diags:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER),
        "should not emit TS1434 for modifier-led try recovery, got {diags:?}"
    );
}

#[test]
fn modifier_led_keyword_named_members_still_parse() {
    let (parser, _root) = parse_source("class C {\n  public class;\n  public enum() {}\n}");
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "valid keyword-named members should still parse after class-member recovery changes, got {diags:?}"
    );
}

#[test]
fn bare_var_statement_in_class_body_recovers_as_ts1068_then_ts1128() {
    let (parser, _root) = parse_source("class Foo2 {\n  var icecream = \"chocolate\";\n}");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        vec![
            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
        ],
        "bare variable statements in class bodies should recover as TS1068 + TS1128, got {diags:?}"
    );
}

#[test]
fn stray_at_before_enum_prefers_ts1109_over_decorator_recovery() {
    let source =
        "// @target: es2015\nnamespace M {\n   ¬\n   class C {\n   }\n   @\n   enum E {\n   ¬\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    let at_pos = source.find('@').unwrap() as u32;
    let enum_pos = source.find("enum E").unwrap() as u32;
    let eof_pos = source.len() as u32;
    assert!(
        codes.contains(&diagnostic_codes::INVALID_CHARACTER),
        "expected TS1127 for invalid characters, got {diags:?}"
    );
    let ts1109 = diags
        .iter()
        .find(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for stray '@' before enum");
    assert_eq!(
        ts1109.start, enum_pos,
        "TS1109 should land on `enum`, not `@`: {diags:?}"
    );
    assert_ne!(
        ts1109.start, at_pos,
        "TS1109 should not be reported at the stray `@`: {diags:?}"
    );
    let ts1005 = diags
        .iter()
        .find(|d| d.code == diagnostic_codes::EXPECTED)
        .expect("expected TS1005 for the unclosed enum tail");
    assert_eq!(
        ts1005.start, eof_pos,
        "TS1005 should be emitted once at EOF for the missing `}}`: {diags:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_EXPECTED),
        "should not emit TS1146 for stray '@' before enum, got {diags:?}"
    );
}
