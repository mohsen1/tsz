use super::*;
use tsz_checker::types::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
};
use tsz_common::position::LineMap;

fn make_diagnostic(
    file: &str,
    start: u32,
    length: u32,
    message: &str,
    category: DiagnosticCategory,
    code: u32,
) -> Diagnostic {
    Diagnostic {
        file: file.to_string(),
        start,
        length,
        message_text: message.to_string(),
        category,
        code,
        related_information: Vec::new(),
    }
}

#[test]
fn test_convert_diagnostic_with_related_info() {
    let source = "line1\nline2\nline3";
    let line_map = LineMap::build(source);
    let related = DiagnosticRelatedInformation {
        file: "test.ts".to_string(),
        start: 0,
        length: 5,
        message_text: "Related here".to_string(),
        category: DiagnosticCategory::Message,
        code: 0,
    };
    let related_other = DiagnosticRelatedInformation {
        file: "other.ts".to_string(),
        start: 0,
        length: 5,
        message_text: "Ignored".to_string(),
        category: DiagnosticCategory::Message,
        code: 0,
    };
    let diag = Diagnostic {
        file: "test.ts".to_string(),
        start: 6,
        length: 5,
        message_text: "Main error".to_string(),
        category: DiagnosticCategory::Error,
        code: 1001,
        related_information: vec![related, related_other],
    };
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert_eq!(lsp_diag.message, "Main error");
    assert_eq!(lsp_diag.range.start.line, 1);
    let related_info = lsp_diag.related_information.expect("Expected related info");
    assert_eq!(related_info.len(), 1);
    assert_eq!(related_info[0].message, "Related here");
    assert_eq!(related_info[0].location.range.start.line, 0);
}

#[test]
fn test_category_to_severity_mapping() {
    assert_eq!(
        category_to_severity(DiagnosticCategory::Error),
        DiagnosticSeverity::Error
    );
    assert_eq!(
        category_to_severity(DiagnosticCategory::Warning),
        DiagnosticSeverity::Warning
    );
    assert_eq!(
        category_to_severity(DiagnosticCategory::Suggestion),
        DiagnosticSeverity::Hint
    );
    assert_eq!(
        category_to_severity(DiagnosticCategory::Message),
        DiagnosticSeverity::Information
    );
}

#[test]
fn test_category_to_string_values() {
    assert_eq!(category_to_string(DiagnosticCategory::Error), "error");
    assert_eq!(category_to_string(DiagnosticCategory::Warning), "warning");
    assert_eq!(
        category_to_string(DiagnosticCategory::Suggestion),
        "suggestion"
    );
    assert_eq!(category_to_string(DiagnosticCategory::Message), "message");
}

#[test]
fn test_ts_diagnostic_format_matches_tsserver() {
    let source = "const x: string = 123;";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic(
        "test.ts",
        6,
        1,
        "Type 'number' is not assignable to type 'string'.",
        DiagnosticCategory::Error,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    let ts_diag = convert_to_ts_diagnostic(&diag, &line_map, source);
    assert_eq!(ts_diag.start.line, 1);
    assert_eq!(ts_diag.start.offset, 7);
    assert_eq!(ts_diag.end.line, 1);
    assert_eq!(ts_diag.end.offset, 8);
    assert_eq!(
        ts_diag.text,
        "Type 'number' is not assignable to type 'string'."
    );
    assert_eq!(ts_diag.code, 2322);
    assert_eq!(ts_diag.category, "error");
}

#[test]
fn test_is_unnecessary_code_detection() {
    assert!(is_unnecessary_code(
        diagnostic_codes::ALL_VARIABLES_ARE_UNUSED
    ));
    assert!(is_unnecessary_code(
        diagnostic_codes::UNREACHABLE_CODE_DETECTED
    ));
    assert!(is_unnecessary_code(
        diagnostic_codes::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS
    ));
    assert!(is_unnecessary_code(
            diagnostic_codes::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS
        ));
    assert!(!is_unnecessary_code(
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
    assert!(!is_unnecessary_code(diagnostic_codes::CANNOT_FIND_NAME));
}

#[test]
fn test_is_deprecated_code_detection() {
    assert!(is_deprecated_code(6385));
    assert!(is_deprecated_code(6387));
    assert!(!is_deprecated_code(
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
    assert!(!is_deprecated_code(
        diagnostic_codes::ALL_VARIABLES_ARE_UNUSED
    ));
}

#[test]
fn test_reports_unnecessary_flag_on_lsp_diagnostic() {
    let source = "const x = 1;";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic(
        "test.ts",
        6,
        1,
        "'x' is declared but its value is never read.",
        DiagnosticCategory::Warning,
        diagnostic_codes::IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ,
    );
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert_eq!(lsp_diag.reports_unnecessary, Some(true));
    assert_eq!(lsp_diag.reports_deprecated, None);
    assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::Warning));
    assert_eq!(lsp_diag.code, Some(6133));
}

#[test]
fn test_reports_deprecated_flag_on_lsp_diagnostic() {
    let source = "foo();";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic(
        "test.ts",
        0,
        3,
        "'foo' is deprecated.",
        DiagnosticCategory::Suggestion,
        6385,
    );
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert_eq!(lsp_diag.reports_deprecated, Some(true));
    assert_eq!(lsp_diag.reports_unnecessary, None);
    assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::Hint));
}

#[test]
fn test_regular_error_has_no_unnecessary_or_deprecated_flags() {
    let source = "const x: string = 123;";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic(
        "test.ts",
        6,
        1,
        "Type 'number' is not assignable to type 'string'.",
        DiagnosticCategory::Error,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert_eq!(lsp_diag.reports_unnecessary, None);
    assert_eq!(lsp_diag.reports_deprecated, None);
    assert_eq!(lsp_diag.code, Some(2322));
}

#[test]
fn test_filter_semantic_diagnostics() {
    let diags = vec![
        make_diagnostic("t.ts", 0, 5, "Type error", DiagnosticCategory::Error, 2322),
        make_diagnostic("t.ts", 0, 5, "Parse error", DiagnosticCategory::Error, 1005),
        make_diagnostic(
            "t.ts",
            0,
            5,
            "Suggestion",
            DiagnosticCategory::Suggestion,
            80006,
        ),
        make_diagnostic("t.ts", 0, 5, "Cannot find", DiagnosticCategory::Error, 2304),
    ];
    let semantic = filter_semantic_diagnostics(&diags);
    assert_eq!(semantic.len(), 2);
    assert_eq!(semantic[0].code, 2322);
    assert_eq!(semantic[1].code, 2304);
}

#[test]
fn test_filter_syntactic_diagnostics() {
    let diags = vec![
        make_diagnostic("t.ts", 0, 5, "Type error", DiagnosticCategory::Error, 2322),
        make_diagnostic("t.ts", 0, 5, "Parse error", DiagnosticCategory::Error, 1005),
        make_diagnostic("t.ts", 0, 5, "Expression", DiagnosticCategory::Error, 1109),
    ];
    let syntactic = filter_syntactic_diagnostics(&diags);
    assert_eq!(syntactic.len(), 2);
    assert_eq!(syntactic[0].code, 1005);
    assert_eq!(syntactic[1].code, 1109);
}

#[test]
fn test_filter_suggestion_diagnostics() {
    let diags = vec![
        make_diagnostic("t.ts", 0, 5, "Type error", DiagnosticCategory::Error, 2322),
        make_diagnostic("t.ts", 0, 5, "Unused", DiagnosticCategory::Warning, 6133),
        make_diagnostic("t.ts", 0, 5, "Async", DiagnosticCategory::Suggestion, 80006),
    ];
    let suggestions = filter_suggestion_diagnostics(&diags);
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].code, 80006);
}

#[test]
fn test_convert_diagnostics_batch() {
    let source = "const x: string = 123;\nlet y: number = 'hello';";
    let line_map = LineMap::build(source);
    let diags = vec![
        make_diagnostic("test.ts", 6, 1, "err1", DiagnosticCategory::Error, 2322),
        make_diagnostic("test.ts", 27, 1, "err2", DiagnosticCategory::Error, 2322),
    ];
    let lsp_diags = convert_diagnostics_batch(&diags, &line_map, source);
    assert_eq!(lsp_diags.len(), 2);
    assert_eq!(lsp_diags[0].range.start.line, 0);
    assert_eq!(lsp_diags[1].range.start.line, 1);
}

#[test]
fn test_ts_diagnostic_with_related_information() {
    let source = "const x = 1;\nconst y: string = x;";
    let line_map = LineMap::build(source);
    let mut diag = make_diagnostic("test.ts", 19, 1, "err", DiagnosticCategory::Error, 2322);
    diag.related_information.push(DiagnosticRelatedInformation {
        file: "test.ts".to_string(),
        start: 6,
        length: 1,
        message_text: "The expected type comes from property 'x'.".to_string(),
        category: DiagnosticCategory::Message,
        code: 0,
    });
    let ts_diag = convert_to_ts_diagnostic(&diag, &line_map, source);
    assert_eq!(ts_diag.code, 2322);
    let related = ts_diag.related_information.expect("Expected related info");
    assert_eq!(related.len(), 1);
    assert_eq!(related[0].start.line, 1);
    assert_eq!(related[0].start.offset, 7);
}

#[test]
fn test_format_ts_error_code() {
    assert_eq!(format_ts_error_code(2322), "TS2322");
    assert_eq!(format_ts_error_code(2345), "TS2345");
    assert_eq!(format_ts_error_code(2304), "TS2304");
    assert_eq!(format_ts_error_code(1005), "TS1005");
    assert_eq!(format_ts_error_code(6133), "TS6133");
}

#[test]
fn test_is_syntactic_vs_semantic_error_code() {
    assert!(is_syntactic_error_code(1005));
    assert!(is_syntactic_error_code(1109));
    assert!(is_syntactic_error_code(1003));
    assert!(!is_syntactic_error_code(2322));
    assert!(!is_syntactic_error_code(2304));
    assert!(!is_syntactic_error_code(6133));
    assert!(is_semantic_error_code(2322));
    assert!(!is_semantic_error_code(1005));
}

#[test]
fn test_ts_diagnostic_serialization_matches_tsserver_format() {
    let ts_diag = TsDiagnostic {
        start: TsPosition { line: 1, offset: 5 },
        end: TsPosition {
            line: 1,
            offset: 10,
        },
        text: "Type 'number' is not assignable to type 'string'.".to_string(),
        code: 2322,
        category: "error".to_string(),
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };
    let json = serde_json::to_value(&ts_diag).unwrap();
    assert_eq!(json["start"]["line"], 1);
    assert_eq!(json["start"]["offset"], 5);
    assert_eq!(json["end"]["line"], 1);
    assert_eq!(json["end"]["offset"], 10);
    assert_eq!(json["code"], 2322);
    assert_eq!(json["category"], "error");
    assert!(json.get("source").is_none());
    assert!(json.get("relatedInformation").is_none());
    assert!(json.get("reportsUnnecessary").is_none());
    assert!(json.get("reportsDeprecated").is_none());
}

#[test]
fn test_diagnostic_severity_roundtrip() {
    let val: u8 = DiagnosticSeverity::Error.into();
    assert_eq!(val, 1);
    assert_eq!(
        DiagnosticSeverity::try_from(val).unwrap(),
        DiagnosticSeverity::Error
    );
    let val: u8 = DiagnosticSeverity::Hint.into();
    assert_eq!(val, 4);
    assert_eq!(
        DiagnosticSeverity::try_from(val).unwrap(),
        DiagnosticSeverity::Hint
    );
    assert!(DiagnosticSeverity::try_from(0u8).is_err());
    assert!(DiagnosticSeverity::try_from(5u8).is_err());
}

#[test]
fn test_lsp_diagnostic_preserves_error_codes() {
    let source = "foo; bar; baz;";
    let line_map = LineMap::build(source);
    let codes = vec![
        (2322, "TS2322"),
        (2345, "TS2345"),
        (2304, "TS2304"),
        (2552, "TS2552"),
        (2339, "TS2339"),
        (2554, "TS2554"),
        (2532, "TS2532"),
    ];
    for (code, expected_ts_code) in codes {
        let diag = make_diagnostic("test.ts", 0, 3, "err", DiagnosticCategory::Error, code);
        let lsp_diag = convert_diagnostic(&diag, &line_map, source);
        assert_eq!(lsp_diag.code, Some(code));
        assert_eq!(format_ts_error_code(code), expected_ts_code);
    }
}
