//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — jsdoc type recovery.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_type_argument_with_empty_jsdoc_wildcard_emits_ts1110() {
    // A bare `?` wildcard type argument (`Foo<?>`) is `Type expected.` (TS1110)
    // in tsc, not TS8020 — tsc reserves TS8020 for `*` and dotted `Foo.<T>`.
    // Oracle (`typescript@7.0.2`): `type T = Foo<?>;` -> TS1110 at the `>`.
    assert_bare_wildcard_is_ts1110("type T = Foo<?>;\n", "`Foo<?>`");
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
fn test_expression_type_argument_with_empty_jsdoc_wildcard_emits_ts1110() {
    // A bare `?` wildcard in an expression's type-argument list (`foo<?>`) is
    // TS1110 in tsc, not TS8020. Oracle (`typescript@7.0.2`):
    // `const WhatFoo = foo<?>;` -> TS1110 at the `>`.
    assert_bare_wildcard_is_ts1110("const WhatFoo = foo<?>;\n", "`foo<?>`");
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

// ---------------------------------------------------------------------------
// #17001: a *bare* `?`/`!` JSDoc wildcard (no operand) in a type position is
// TS1110 (`Type expected.`) in tsc, not TS8020. Every case below is
// oracle-verified against `typescript@7.0.2`
// (`tsc --noEmit --strict --lib es2022 --target es2022`). The `*` all-type and
// the dotted `Foo.<T>` legacy generic genuinely stay TS8020 (pinned above);
// a wildcard *followed by a real type* (`?string`) stays TS17020 (pinned
// above). Only the bare, operand-less wildcard is TS1110.
// ---------------------------------------------------------------------------

/// Assert a bare-wildcard source reports TS1110 and neither TS8020 nor TS17020.
fn assert_bare_wildcard_is_ts1110(source: &str, label: &str) {
    let (parser, _root) = parse_source(source);
    let diagnostics: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        diagnostics.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected TS1110 for {label}, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(
            &diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS
        ),
        "Expected no TS8020 for {label}, got {:?}",
        parser.get_diagnostics(),
    );
    assert!(
        !diagnostics.contains(
            &diagnostic_codes::AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE
        ),
        "Expected no TS17020 for {label}, got {:?}",
        parser.get_diagnostics(),
    );
}

#[test]
fn test_bare_question_wildcard_in_annotation_is_ts1110() {
    // Oracle: `var x: ?;` -> TS1110 at the `;`.
    assert_bare_wildcard_is_ts1110("var x: ?;\n", "`var x: ?;`");
}

#[test]
fn test_bare_question_wildcard_in_required_position_is_ts1110() {
    // Oracle: `type T = string | ?;` -> TS1110 (a `|`-separated constituent is a
    // *required* type position, so tsc reports even at a terminator).
    assert_bare_wildcard_is_ts1110("type T = string | ?;\n", "`string | ?`");
}

#[test]
fn test_bare_question_wildcard_in_tuple_is_ts1110() {
    // Oracle: `type T = [?];` -> TS1110.
    assert_bare_wildcard_is_ts1110("type T = [?];\n", "`[?]`");
}

#[test]
fn test_bare_question_wildcard_with_following_comma_is_ts1110() {
    // Oracle: `type T = Map<?, string>;` -> TS1110 at the `,`.
    assert_bare_wildcard_is_ts1110("type T = Map<?, string>;\n", "`Map<?, string>`");
}

#[test]
fn test_bare_bang_wildcard_in_annotation_is_ts1110() {
    // Oracle: `var x: !;` -> TS1110.
    assert_bare_wildcard_is_ts1110("var x: !;\n", "`var x: !;`");
}

#[test]
fn test_bare_bang_wildcard_in_type_argument_is_ts1110() {
    // Oracle: `type T = Foo<!>;` -> TS1110 (the `!` flows through the ordinary
    // primary-type parse for each type argument).
    assert_bare_wildcard_is_ts1110("type T = Foo<!>;\n", "`Foo<!>`");
}
