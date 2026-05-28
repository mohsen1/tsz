//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — jsdoc type recovery.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

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
