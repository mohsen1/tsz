//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — jsdoc type recovery.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_type_argument_with_empty_jsdoc_wildcard_reports_ts1110() {
    // A bare `?` (no operand) in type-argument position has no JSDoc
    // meaning left to recover — tsc 7.0.2 reports TS1110 ("Type expected")
    // at the token after `?`, not TS8020 (oracle-verified: `Foo<?>` -> TS1110
    // at the `>`). TS8020 stays reserved for genuinely JSDoc-only syntax
    // (`*`, `Array.<T>`).
    let source = r#"
type T = Foo<?>;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected TS1110 for `Foo<?>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected no TS8020 for `Foo<?>`, got {:?}",
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
fn test_expression_type_argument_with_empty_jsdoc_wildcard_reports_ts1110() {
    // Same bare-wildcard rule as the type-position case: oracle-verified
    // (`const WhatFoo = foo<?>;` -> TS1110 at the `>`).
    let source = r#"
const WhatFoo = foo<?>;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected TS1110 for `foo<?>`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected no TS8020 for `foo<?>`, got {:?}",
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
    // TS 7.0.2 removed the JSDoc-legacy `function(...)` type recovery: in a
    // parameter type position each `function` parses as a type-reference
    // identifier and the following `(` re-enters normal recovery, so the
    // parser reports plain TS1005 recoveries and never TS8020.
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

    assert!(
        !diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "The removed TS8020 JSDoc-legacy recovery must not fire, got {:?}",
        parser.get_diagnostics()
    );
    assert!(
        diagnostics.iter().filter(|code| **code == 1005).count() >= 2,
        "Expected a TS1005 recovery at each `function(` parameter type, got {:?}",
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

/// Adjacent-case matrix for the bare `?`/`!` type-position wildcard, oracle-
/// verified against `typescript@7.0.2`. `?`/`!` with no following type
/// operand is not a JSDoc-recoverable construct (unlike `*` or `Array.<T>`),
/// so tsc reports plain TS1110 ("Type expected") at the token after the
/// wildcard, not TS8020.
#[test]
fn test_bare_question_mark_type_annotation_reports_ts1110() {
    let source = "var x: ?;";
    let (parser, _root) = parse_source(source);

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected TS1110 for `var x: ?;`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected no TS8020 for `var x: ?;`, got {:?}",
        parser.get_diagnostics(),
    );
}

#[test]
fn test_bare_exclamation_mark_type_annotation_reports_ts1110() {
    let source = "var y: !;";
    let (parser, _root) = parse_source(source);

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected TS1110 for `var y: !;`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected no TS8020 for `var y: !;`, got {:?}",
        parser.get_diagnostics(),
    );
}

#[test]
fn test_bare_question_mark_after_union_separator_reports_ts1110() {
    // Renamed-binder + union-separator variant: the bare `?` follows a
    // consumed `|`, a *required* constituent position, same as the plain
    // annotation case.
    let source = "type Renamed = string | ?;";
    let (parser, _root) = parse_source(source);

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected TS1110 for `string | ?;`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected no TS8020 for `string | ?;`, got {:?}",
        parser.get_diagnostics(),
    );
}

#[test]
fn test_map_type_argument_with_bare_wildcard_reports_ts1110() {
    // Renamed generic + multi-argument variant of the type-argument case:
    // the bare `?` is the second argument, immediately followed by `>`.
    let source = "let a: RenamedMap<string, ?>;";
    let (parser, _root) = parse_source(source);

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected TS1110 for `RenamedMap<string, ?>;`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected no TS8020 for `RenamedMap<string, ?>;`, got {:?}",
        parser.get_diagnostics(),
    );
}

#[test]
fn test_prefix_question_mark_with_real_type_still_reports_ts17020_not_ts1110() {
    // Negative control: the wildcard *followed by* a real type operand stays
    // the existing TS17020 JSDoc-nullable-prefix recovery — only the bare
    // (no-operand) wildcard changed in this fix.
    let source = "type Renamed = ?number;";
    let (parser, _root) = parse_source(source);

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(&diagnostic_codes::AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE),
        "Expected TS17020 for `?number`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected no TS1110 for `?number`, got {:?}",
        parser.get_diagnostics(),
    );
}

#[test]
fn test_prefix_exclamation_mark_with_real_type_still_reports_ts17020_not_ts1110() {
    let source = "var renamedVar: !number;";
    let (parser, _root) = parse_source(source);

    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(&diagnostic_codes::AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE),
        "Expected TS17020 for `!number`, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected no TS1110 for `!number`, got {:?}",
        parser.get_diagnostics(),
    );
}
