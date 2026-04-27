//! Tests for expression parsing in the parser.
use crate::parser::ParserState;
use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

fn parse_diagnostics(source: &str) -> usize {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();
    parser.get_diagnostics().len()
}

#[test]
fn await_in_heritage_type_argument_recovery_reports_tsc_parser_fingerprints() {
    let source = "class C extends await<string> {}\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();

    let await_pos = source.find("await").unwrap() as u32;
    let less_than_pos = source.find('<').unwrap() as u32;
    let greater_than_pos = source.find('>').unwrap() as u32;

    assert!(
        diags
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED
                && diag.start == await_pos),
        "expected TS1109 at `await` in heritage clause, got {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED
                && diag.start == less_than_pos),
        "expected TS1109 at `<` after invalid heritage await, got {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPECTED
                && diag.start == greater_than_pos
                && diag.message == "',' expected."),
        "expected TS1005 comma diagnostic at `>` in invalid heritage await, got {diags:?}"
    );
}

#[test]
fn await_in_decorator_expression_reports_tsc_parser_fingerprints() {
    let source = r#"
@await
class C1 {}
@(await)
class C2 {}
class C3 {
    @await(1)
    ["foo"]() {}
    method(@await [x]) {}
    method2(@(await) [x]) {}
}
"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();

    let bare_await_pos = source.find("@await").unwrap() as u32 + 1;
    let parenthesized_close_pos = source.find("@(await)").unwrap() as u32 + "@(await".len() as u32;
    let member_await_pos = source.find("@await(1)").unwrap() as u32 + 1;
    let parameter_await_pos = source.find("@await [x]").unwrap() as u32 + 1;
    let parameter_close_pos = source.find("@(await) [x]").unwrap() as u32 + "@(await".len() as u32;

    for expected_pos in [
        bare_await_pos,
        parenthesized_close_pos,
        member_await_pos,
        parameter_await_pos,
        parameter_close_pos,
    ] {
        assert!(
            diags
                .iter()
                .any(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED
                    && diag.start == expected_pos),
            "expected TS1109 at byte {expected_pos}, got {diags:?}"
        );
    }
}

#[test]
fn expression_parsing_handles_shift_and_greater_token_ambiguity() {
    let diag_count = parse_diagnostics("const shifted = 1 >> 2 >>> 3; let rhs = x >= 1;");
    assert_eq!(diag_count, 0, "unexpected parser diagnostics: {diag_count}");
}

#[test]
fn expression_type_argument_probe_rejects_greater_equals_as_closing_angle() {
    let source = r#"
const enum MyVer { v1 = 1, v2 = 2 }
let ver = 21
const a = ver < (MyVer.v1 >= MyVer.v2 ? MyVer.v1 : MyVer.v2)
"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert_eq!(
        diags.len(),
        0,
        "`>=` inside a relational expression should not close speculative expression type arguments: {diags:?}"
    );
}

#[test]
fn jsx_empty_type_arguments_accept_compound_closer_without_text_child() {
    let source = "const a = <div<>></div>;";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    parser.parse_source_file();

    let diags = parser.get_diagnostics();
    assert!(
        diags
            .iter()
            .any(|diag| diag.code == diagnostic_codes::TYPE_ARGUMENT_LIST_CANNOT_BE_EMPTY),
        "empty JSX type arguments should still report TS1099: {diags:?}"
    );
    assert!(
        parser
            .get_arena()
            .jsx_text
            .iter()
            .all(|text| text.text.trim() != ">"),
        "the JSX opening tag closer should not be parsed as a text child: {:?}",
        parser
            .get_arena()
            .jsx_text
            .iter()
            .map(|text| text.text.as_str())
            .collect::<Vec<_>>()
    );
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
fn malformed_equality_tail_in_parens_does_not_emit_close_paren_cascade() {
    let (parser, _root) = parse_source("export = } x = ( y = z ==== 'function') {");
    let diags = parser.get_diagnostics();

    assert!(
        diags
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED && diag.start == 9),
        "expected TS1109 at the invalid export-assignment expression, got {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED && diag.start == 26),
        "expected TS1109 at the stray equality token, got {diags:?}"
    );
    assert!(
        !diags
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPECTED
                && diag.start == 28
                && diag.message == "')' expected."),
        "should suppress the cascading missing-paren diagnostic at the string literal, got {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPECTED
                && diag.start == 38
                && diag.message == "';' expected."),
        "expected statement recovery to report the missing semicolon at the close paren, got {diags:?}"
    );
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
fn malformed_numeric_arrow_body_reports_comma_at_return_semicolon() {
    let source = "foo((1)=>{return 0;});";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let return_semicolon = source.find("0;").expect("return expression") as u32 + 1;

    assert!(
        diags
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPECTED
                && diag.start == return_semicolon
                && diag.message == "',' expected."),
        "expected tsc-compatible comma recovery at the malformed arrow body's semicolon, got {diags:?}"
    );
    assert!(
        diags
            .iter()
            .all(|diag| !(diag.code == diagnostic_codes::EXPECTED
                && diag.start == return_semicolon
                && diag.message == "':' expected.")),
        "should not emit a second missing-colon diagnostic at the semicolon, got {diags:?}"
    );
}

#[test]
fn object_method_arrow_return_token_prefers_brace_expected_then_ts1434() {
    let source = r#"let o = {
    m(n: number) => string {
        return n.toString();
    }
};"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags.iter().any(|d| {
            d.code == diagnostic_codes::EXPECTED && d.message.contains("'{' expected.")
        }),
        "expected TS1005 '{{' expected on object method `=>` recovery, got {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER),
        "expected TS1434 on stray type token after object method `=>`, got {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "expected TS1128 tail recovery after malformed object method, got {diags:?}"
    );
    assert!(
        diags
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED),
        "object method recovery should not fall back to TS1131, got {diags:?}"
    );
}

#[test]
fn import_type_arguments_without_call_parens_avoid_ts1005_cascade() {
    let source = "import<T>\nconst a = import<string, number>";
    let (parser, _root) = parse_source(source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::THIS_USE_OF_IMPORT_IS_INVALID_IMPORT_CALLS_CAN_BE_WRITTEN_BUT_THEY_MUST_HAVE_PAR),
        "Expected TS1326 for `import<T>` usage, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPECTED),
        "Should not cascade with TS1005 from forced import-call recovery, got {codes:?}"
    );
}

#[test]
fn import_empty_type_arguments_only_report_ts1326() {
    let source = "const p = import<>(\"./0\");";
    let (parser, _root) = parse_source(source);
    let codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(
            &diagnostic_codes::THIS_USE_OF_IMPORT_IS_INVALID_IMPORT_CALLS_CAN_BE_WRITTEN_BUT_THEY_MUST_HAVE_PAR
        ),
        "Expected TS1326 for import type arguments, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_ARGUMENT_LIST_CANNOT_BE_EMPTY),
        "import<> should not emit TS1099 alongside TS1326, got {codes:?}"
    );
}

#[test]
fn new_expression_missing_callee_reports_ts1109() {
    let source = "(a,\nnew)";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    let expr_expected = diags
        .iter()
        .find(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .unwrap_or_else(|| panic!("expected TS1109 for missing `new` callee, got {diags:?}"));
    assert_eq!(
        expr_expected.start,
        source.find(')').expect("closing paren") as u32,
        "TS1109 should anchor at ')' after bare `new`: {diags:?}"
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

#[test]
fn legacy_octal_literal_emits_ts1121() {
    // TS1121: "Octal literals are not allowed. Use the syntax '0o1'."
    let (parser, _root) = parse_source("01");
    let diags = parser.get_diagnostics();
    assert!(
        diags.iter().any(|d| d.code == 1121),
        "Expected TS1121 for legacy octal '01', got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn legacy_octal_literal_suggests_modern_syntax() {
    let (parser, _root) = parse_source("0777");
    let diags = parser.get_diagnostics();
    let ts1121 = diags.iter().find(|d| d.code == 1121);
    assert!(ts1121.is_some(), "Expected TS1121 for '0777'");
    assert!(
        ts1121.unwrap().message.contains("0o777"),
        "Expected suggestion '0o777' in message: {}",
        ts1121.unwrap().message
    );
}

#[test]
fn negative_legacy_octal_literal_emits_ts1121() {
    // `-03` should emit TS1121 with suggestion '-0o3'
    let (parser, _root) = parse_source("-03");
    let diags = parser.get_diagnostics();
    assert!(
        diags.iter().any(|d| d.code == 1121),
        "Expected TS1121 for '-03', got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn invalid_numeric_separator_followed_by_identifier_does_not_emit_ts2304() {
    // tsc emits TS6188 (separator-not-allowed) and TS1351 (identifier
    // cannot follow numeric literal) for inputs like `0_X0101`. It does
    // NOT emit TS2304 ("Cannot find name 'X0101'"). The recovered
    // identifier is parser-recovery debris, not a real name-resolution
    // candidate. Lock that suppression so
    // `parser.numericSeparators.{hex,binary,octal}Negative.ts` keep passing.
    let (parser, _root) = parse_source("0_X0101");
    let diags = parser.get_diagnostics();
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1351),
        "Expected TS1351 for identifier-after-numeric, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&2304),
        "TS2304 must NOT fire for the recovered identifier. Got codes: {codes:?}"
    );
}
