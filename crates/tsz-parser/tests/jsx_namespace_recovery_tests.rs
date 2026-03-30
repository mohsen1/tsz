use crate::parser::state::ParserState;
use tsz_common::diagnostics::diagnostic_codes;

fn parse_diagnostics(source: &str) -> Vec<(u32, u32, String)> {
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    parser
        .parse_diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start, diag.message.clone()))
        .collect()
}

#[test]
fn jsx_namespaced_tag_with_extra_colon_reports_identifier_expected_at_second_colon() {
    let source = "declare var React: any;\nvar x = <a:ele:ment />;\n";
    let diagnostics = parse_diagnostics(source);
    let second_colon = source.rfind(':').expect("second colon") as u32;

    assert_eq!(
        diagnostics,
        vec![(
            diagnostic_codes::IDENTIFIER_EXPECTED,
            second_colon,
            "Identifier expected.".to_string(),
        )],
        "expected tsc-style recovery for extra JSX namespace separator, got {diagnostics:?}"
    );
}

#[test]
fn jsx_namespaced_tag_with_space_after_colon_recovers_through_spread_attribute_path() {
    let source = "declare var React: any;\nvar x = <a: attr={\"value\"} />;\n";
    let diagnostics = parse_diagnostics(source);
    let equals_pos = source.find("={").expect("equals") as u32;
    let quote_pos = source.find("\"value\"").expect("quote") as u32;

    assert_eq!(
        diagnostics,
        vec![
            (
                diagnostic_codes::IDENTIFIER_EXPECTED,
                equals_pos,
                "Identifier expected.".to_string(),
            ),
            (
                diagnostic_codes::EXPECTED,
                quote_pos,
                "'...' expected.".to_string(),
            ),
        ],
        "expected tsc-style recovery for spaced JSX namespace tag name, got {diagnostics:?}"
    );
}

#[test]
fn jsx_namespaced_tag_with_space_before_colon_recovers_through_spread_attribute_path() {
    let source = "declare var React: any;\nvar x = <a :attr={\"value\"} />;\n";
    let diagnostics = parse_diagnostics(source);
    let equals_pos = source.find("={").expect("equals") as u32;
    let quote_pos = source.find("\"value\"").expect("quote") as u32;

    assert_eq!(
        diagnostics,
        vec![
            (
                diagnostic_codes::IDENTIFIER_EXPECTED,
                equals_pos,
                "Identifier expected.".to_string(),
            ),
            (
                diagnostic_codes::EXPECTED,
                quote_pos,
                "'...' expected.".to_string(),
            ),
        ],
        "expected tsc-style recovery for spaced JSX namespace tag name, got {diagnostics:?}"
    );
}

#[test]
fn jsx_tag_cannot_start_with_namespace_colon_in_expression_context() {
    let source = "declare var React: any;\nvar x = <:a attr={\"value\"} />;\n";
    let diagnostics = parse_diagnostics(source);
    let less_than_pos = source.find('<').expect("opening angle") as u32;
    let colon_pos = source[less_than_pos as usize + 1..]
        .find(':')
        .map(|offset| less_than_pos + 1 + offset as u32)
        .expect("colon");
    let attr_pos = source.find("attr").expect("attr") as u32;
    let close_brace_pos = source.rfind('}').expect("close brace") as u32;
    let greater_than_pos = source.rfind('>').expect("greater-than") as u32;

    assert_eq!(
        diagnostics,
        vec![
            (
                diagnostic_codes::EXPRESSION_EXPECTED,
                less_than_pos,
                "Expression expected.".to_string(),
            ),
            (
                diagnostic_codes::EXPRESSION_EXPECTED,
                colon_pos,
                "Expression expected.".to_string(),
            ),
            (
                diagnostic_codes::EXPECTED,
                attr_pos,
                "',' expected.".to_string(),
            ),
            (
                diagnostic_codes::EXPECTED,
                close_brace_pos,
                "':' expected.".to_string(),
            ),
            (
                diagnostic_codes::EXPRESSION_EXPECTED,
                greater_than_pos,
                "Expression expected.".to_string(),
            ),
            (
                diagnostic_codes::EXPRESSION_EXPECTED,
                greater_than_pos + 1,
                "Expression expected.".to_string(),
            ),
        ],
        "expected tsc-style recovery when JSX tag starts with ':', got {diagnostics:?}"
    );
}

#[test]
fn jsx_closing_tag_with_extra_namespace_separator_keeps_tail_outside_closing_name() {
    let source = "declare var React: any;\nvar x = <a:ele:ment>{\"text\"}</a:ele:ment>;\n";
    let diagnostics = parse_diagnostics(source);
    let second_colon = source.rfind(':').expect("closing tag second colon") as u32;
    let closing_gt = source.rfind('>').expect("closing tag >") as u32;
    let semicolon = source.rfind(';').expect("semicolon") as u32;

    assert_eq!(
        diagnostics,
        vec![
            (
                diagnostic_codes::IDENTIFIER_EXPECTED,
                source.find("a:ele:ment").expect("opening tag") as u32 + 5,
                "Identifier expected.".to_string(),
            ),
            (
                diagnostic_codes::EXPECTED,
                second_colon,
                "'>' expected.".to_string(),
            ),
            (
                diagnostic_codes::EXPECTED,
                closing_gt,
                "',' expected.".to_string(),
            ),
            (
                diagnostic_codes::EXPRESSION_EXPECTED,
                semicolon,
                "Expression expected.".to_string(),
            ),
        ],
        "expected tsc-style recovery for malformed JSX closing tag, got {diagnostics:?}"
    );
}
