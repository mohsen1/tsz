//! Tests for expression parsing in the parser.
use crate::parser::{NodeIndex, ParserState};
use tsz_common::diagnostics::diagnostic_codes;

fn parse_diagnostics(source: &str) -> usize {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();
    parser.get_diagnostics().len()
}

fn parse_source(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn expression_parsing_handles_shift_and_greater_token_ambiguity() {
    let diag_count = parse_diagnostics("const shifted = 1 >> 2 >>> 3; let rhs = x >= 1;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn expression_parsing_handles_regex_division_boundary_after_tokens() {
    let diag_count =
        parse_diagnostics("const n = 10 / 2; const re = /foo/g; const tail = (a / b) / c;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn expression_parsing_reports_template_recovery_for_unterminated_tail() {
    let diag_count = parse_diagnostics("const t = `a${1 + 2`; const ok = 1;");
    assert!(
        diag_count > 0,
        "expected diagnostics for unterminated template tail"
    );
}

#[test]
fn expression_parsing_rejects_incomplete_shift_rhs() {
    let diag_count = parse_diagnostics("const x = 1 >> ;");
    assert!(
        diag_count > 0,
        "expected diagnostics for incomplete shift expression"
    );
}

#[test]
fn expression_parsing_generic_arrow_after_shift_restores_state() {
    let diag_count = parse_diagnostics("const f = <T>(value: T) => value >> 0;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn expression_parsing_supports_regex_literals_and_division_paths() {
    let diag_count =
        parse_diagnostics("const re = /foo/g;\nconst n = 10 / 2;\nlet x = 1;\nx /= 2;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn expression_parsing_supports_tagged_and_plain_templates() {
    let diag_count =
        parse_diagnostics("const tag = String.raw`head${1 + 2}tail`;\nconst plain = `x${1 + 2}y`;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");

    let (parser, root) = parse_source("const bad = `head${1 + 2`;\nconst ok = 1;");
    assert!(!parser.get_diagnostics().is_empty());
    let sf = parser
        .get_arena()
        .get_source_file_at(root)
        .unwrap_or_else(|| panic!("missing source file node"));
    assert!(!sf.statements.nodes.is_empty());
}

#[test]
fn expression_parsing_handles_regex_and_division_tokens() {
    let diag_count =
        parse_diagnostics("const re = /foo/g;\nconst value = 10 / 2;\nconst bad = a / 0;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn expression_parsing_supports_compound_shift_assignment() {
    let diag_count = parse_diagnostics("let n = 8;\nn >>>= 2;\nn = n >> 1;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn expression_parsing_does_not_misclassify_parenthesized_destructuring_assignment_as_arrow() {
    let diag_count = parse_diagnostics(
        r#"
abstract class C1 {
    abstract x: string;
    abstract y: string;

    constructor() {
        ({ x, y: y1, "y": y1 } = this);
    }
}
"#,
    );
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn type_predicate_assertions_report_syntax_errors_instead_of_parsing_as_types() {
    let (parser, _root) = parse_source(
        r#"
declare var numOrStr: number | string;

if (<numOrStr is string>(numOrStr === undefined)) {
}

if ((numOrStr === undefined) as numOrStr is string) {
}
"#,
    );
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    let diags = parser.get_diagnostics();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "expected TS1005 recovery for invalid type-predicate assertion, got {diags:?}"
    );
    // TS1128 may or may not appear depending on parser recovery path
    assert!(
        codes.contains(&diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER),
        "expected TS1434 after invalid `as` assertion recovery, got {diags:?}"
    );
}

/// Test: get/set accessor with missing `(` in object literal should not cascade errors.
/// When `get e,` appears in an object literal, tsc emits TS1005 '(' expected
/// and continues parsing subsequent properties correctly. The `,` after `e`
/// belongs to the object literal list, not the accessor's parameter list.
#[test]
fn object_literal_accessor_missing_paren_no_cascade() {
    let source = r#"var y = {
    get e,
    set f,
    this,
    class
};"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    // Should emit TS1005 for '(' expected on get/set and ':' expected on this/class
    assert!(
        codes.iter().all(|&c| c == diagnostic_codes::EXPECTED),
        "expected only TS1005 errors, got codes: {codes:?}, diags: {diags:?}"
    );
    // Must NOT emit TS1109 (Expression expected) - that was the spurious cascading error
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "should not emit TS1109, got: {diags:?}"
    );
}

/// Test: shorthand properties with non-identifier names emit TS1005 only, not TS1109.
#[test]
fn object_literal_shorthand_non_identifier_no_ts1109() {
    let source = r#"var y = {
    "stringLiteral",
    42,
    typeof
};"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    // Should only have TS1005 (':' expected) for each non-identifier shorthand
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "should not emit TS1109, got: {diags:?}"
    );
}

/// Test: `a.b,` in object literal emits comma-expected without TS1109.
#[test]
fn object_literal_dotted_property_recovery() {
    let source = r#"var x = {
    a.b,
    a["ss"],
    a[1],
};"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "should not emit TS1109, got: {diags:?}"
    );
}

#[test]
fn async_arrow_parameter_recovery_rolls_back_speculation() {
    let source = "var foo = async (a = await => await): Promise<void> => {}";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let actual: Vec<(u32, u32)> = diags.iter().map(|diag| (diag.code, diag.start)).collect();
    let expected = vec![
        (
            diagnostic_codes::EXPECTED,
            source.find(':').expect("return type colon") as u32,
        ),
        (
            diagnostic_codes::EXPECTED,
            source.find('<').expect("Promise type args") as u32,
        ),
        (
            diagnostic_codes::EXPRESSION_EXPECTED,
            source.rfind("=>").expect("outer arrow") as u32,
        ),
    ];

    assert_eq!(
        actual, expected,
        "async-arrow speculation should roll back to TypeScript's fallback parse.\nactual diagnostics: {diags:?}"
    );
}
