//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — let array ambiguity.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_let_array_ambiguity_reports_ts1181_then_statement_recovery() {
    let source = r#"
var let: any;
let[0] = 100;
"#;
    let (parser, _root) = parse_source(source);

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
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![1005, 1181, 1005, 1128],
        "Expected TS1005/TS1181/TS1005/TS1128 recovery for `for (let of [...])`, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn invalid_let_array_reserved_word_emits_destructuring_diagnostic() {
    // `let [while]` — `while` is a reserved word; not a valid binding element.
    let source = "let [while];\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "expected array-element-destructuring diagnostic for `let [while]`, got {diags:?}",
    );
}

#[test]
fn invalid_let_array_for_keyword_emits_destructuring_diagnostic() {
    // `let [for]` — different reserved word; same structural rule.
    let source = "let [for];\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "expected array-element-destructuring diagnostic for `let [for]`, got {diags:?}",
    );
}

#[test]
fn invalid_let_array_numeric_literal_emits_destructuring_diagnostic() {
    // `let [42]` — numeric literal; not a binding name.
    let source = "let [42];\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "expected array-element-destructuring diagnostic for `let [42]`, got {diags:?}",
    );
}

#[test]
fn invalid_let_array_string_literal_emits_destructuring_diagnostic() {
    // `let ["key"]` — string literal; not a binding name.
    let source = r#"let ["key"];"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "expected array-element-destructuring diagnostic for `let [\"key\"]`, got {diags:?}",
    );
}

#[test]
fn valid_let_array_identifier_does_not_trigger_recovery() {
    // `let [x] = []` — valid destructuring; no recovery diagnostic.
    let source = "let [x] = [];\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        !diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "valid `let [x] = []` must not emit array-element-destructuring diagnostic, got {diags:?}",
    );
}

#[test]
fn valid_let_array_empty_brackets_does_not_trigger_recovery() {
    // `let [] = []` — valid empty destructuring.
    let source = "let [] = [];\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        !diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "valid `let [] = []` must not emit array-element-destructuring diagnostic, got {diags:?}",
    );
}

#[test]
fn valid_let_array_rest_element_does_not_trigger_recovery() {
    // `let [...rest] = []` — valid rest-element pattern.
    let source = "let [...rest] = [];\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        !diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "valid `let [...rest] = []` must not emit array-element-destructuring diagnostic, got {diags:?}",
    );
}

#[test]
fn invalid_let_array_recovery_does_not_crash_on_assignment() {
    // `let [+] = 1` — bad first element followed by `=`; parser must not panic.
    let source = "let [+] = 1;\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        !diags.is_empty(),
        "expected at least one diagnostic for `let [+] = 1`, got none",
    );
}
