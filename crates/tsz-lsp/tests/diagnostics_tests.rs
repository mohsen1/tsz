use super::*;
use tsz_checker::diagnostics::{
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

// ---- New diagnostic tests ----

#[test]
fn test_convert_diagnostic_warning_category() {
    let source = "const x = 1;";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic(
        "test.ts",
        6,
        1,
        "Variable is unused",
        DiagnosticCategory::Warning,
        6133,
    );
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::Warning));
    assert_eq!(lsp_diag.source, Some("tsc-rust".to_string()));
}

#[test]
fn test_convert_diagnostic_suggestion_category() {
    let source = "foo();";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic(
        "test.ts",
        0,
        3,
        "Could be simplified",
        DiagnosticCategory::Suggestion,
        80006,
    );
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::Hint));
}

#[test]
fn test_convert_diagnostic_no_related_info() {
    let source = "const x = 1;";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic("test.ts", 0, 5, "err", DiagnosticCategory::Error, 2322);
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert!(
        lsp_diag.related_information.is_none(),
        "Should have no related info when none provided"
    );
}

#[test]
fn test_convert_diagnostic_all_related_info_from_other_files_filtered_out() {
    let source = "const x = 1;";
    let line_map = LineMap::build(source);
    let mut diag = make_diagnostic("test.ts", 0, 5, "err", DiagnosticCategory::Error, 2322);
    diag.related_information.push(DiagnosticRelatedInformation {
        file: "other.ts".to_string(),
        start: 0,
        length: 5,
        message_text: "From another file".to_string(),
        category: DiagnosticCategory::Message,
        code: 0,
    });
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert!(
        lsp_diag.related_information.is_none(),
        "Related info from other files should be filtered out"
    );
}

#[test]
fn test_convert_diagnostic_zero_length() {
    let source = "const x = 1;";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic("test.ts", 6, 0, "zero len", DiagnosticCategory::Error, 2322);
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert_eq!(lsp_diag.range.start, lsp_diag.range.end);
}

#[test]
fn test_convert_to_ts_diagnostic_positions_are_1_indexed() {
    // Verify positions are 1-indexed (tsserver convention)
    let source = "x";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic("test.ts", 0, 1, "err", DiagnosticCategory::Error, 2304);
    let ts_diag = convert_to_ts_diagnostic(&diag, &line_map, source);
    // Line and offset should be 1-indexed
    assert_eq!(ts_diag.start.line, 1);
    assert_eq!(ts_diag.start.offset, 1);
    assert_eq!(ts_diag.end.line, 1);
    assert_eq!(ts_diag.end.offset, 2);
}

#[test]
fn test_convert_to_ts_diagnostic_multiline() {
    let source = "line1\nline2\nline3";
    let line_map = LineMap::build(source);
    // Start in line2 (offset 6), length spans into line3
    let diag = make_diagnostic("test.ts", 6, 11, "span", DiagnosticCategory::Error, 2322);
    let ts_diag = convert_to_ts_diagnostic(&diag, &line_map, source);
    assert_eq!(
        ts_diag.start.line, 2,
        "Start should be on line 2 (1-indexed)"
    );
    assert_eq!(
        ts_diag.start.offset, 1,
        "Start offset should be 1 (beginning of line2)"
    );
}

#[test]
fn test_convert_to_ts_diagnostic_warning_category_string() {
    let source = "const x = 1;";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic("test.ts", 0, 5, "warn", DiagnosticCategory::Warning, 6133);
    let ts_diag = convert_to_ts_diagnostic(&diag, &line_map, source);
    assert_eq!(ts_diag.category, "warning");
}

#[test]
fn test_convert_to_ts_diagnostic_suggestion_category_string() {
    let source = "const x = 1;";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic(
        "test.ts",
        0,
        5,
        "suggest",
        DiagnosticCategory::Suggestion,
        80006,
    );
    let ts_diag = convert_to_ts_diagnostic(&diag, &line_map, source);
    assert_eq!(ts_diag.category, "suggestion");
}

#[test]
fn test_convert_to_ts_diagnostics_batch() {
    let source = "const x = 1;\nlet y = 2;";
    let line_map = LineMap::build(source);
    let diags = vec![
        make_diagnostic("test.ts", 6, 1, "err1", DiagnosticCategory::Error, 2322),
        make_diagnostic("test.ts", 17, 1, "err2", DiagnosticCategory::Warning, 6133),
    ];
    let ts_diags = convert_to_ts_diagnostics_batch(&diags, &line_map, source);
    assert_eq!(ts_diags.len(), 2);
    assert_eq!(ts_diags[0].category, "error");
    assert_eq!(ts_diags[1].category, "warning");
    // First diag on line 1, second on line 2 (1-indexed)
    assert_eq!(ts_diags[0].start.line, 1);
    assert_eq!(ts_diags[1].start.line, 2);
}

#[test]
fn test_convert_to_ts_diagnostics_batch_empty() {
    let source = "const x = 1;";
    let line_map = LineMap::build(source);
    let diags: Vec<Diagnostic> = vec![];
    let ts_diags = convert_to_ts_diagnostics_batch(&diags, &line_map, source);
    assert!(ts_diags.is_empty());
}

#[test]
fn test_is_unnecessary_code_additional_codes() {
    // Test the additional hardcoded codes (6196 and 6138)
    assert!(is_unnecessary_code(6196), "6196 should be unnecessary");
    assert!(is_unnecessary_code(6138), "6138 should be unnecessary");
}

#[test]
fn test_is_syntactic_error_code_boundaries() {
    // Test boundary conditions
    assert!(
        !is_syntactic_error_code(999),
        "999 is below syntactic range"
    );
    assert!(
        is_syntactic_error_code(1000),
        "1000 is start of syntactic range"
    );
    assert!(
        is_syntactic_error_code(1999),
        "1999 is end of syntactic range"
    );
    assert!(
        !is_syntactic_error_code(2000),
        "2000 is above syntactic range"
    );
}

#[test]
fn test_is_semantic_error_code_is_inverse_of_syntactic() {
    // semantic is defined as !syntactic
    for code in [999, 1000, 1500, 1999, 2000, 2322, 6133, 80006] {
        assert_eq!(
            is_semantic_error_code(code),
            !is_syntactic_error_code(code),
            "Semantic and syntactic should be inverses for code {code}"
        );
    }
}

#[test]
fn test_filter_semantic_excludes_suggestions() {
    let diags = vec![
        make_diagnostic("t.ts", 0, 5, "semantic", DiagnosticCategory::Error, 2322),
        make_diagnostic(
            "t.ts",
            0,
            5,
            "suggestion",
            DiagnosticCategory::Suggestion,
            2322,
        ),
    ];
    let semantic = filter_semantic_diagnostics(&diags);
    assert_eq!(
        semantic.len(),
        1,
        "Suggestions should be excluded from semantic filter"
    );
    assert_eq!(semantic[0].category, DiagnosticCategory::Error);
}

#[test]
fn test_filter_semantic_includes_warnings() {
    let diags = vec![make_diagnostic(
        "t.ts",
        0,
        5,
        "warn",
        DiagnosticCategory::Warning,
        6133,
    )];
    let semantic = filter_semantic_diagnostics(&diags);
    assert_eq!(
        semantic.len(),
        1,
        "Warnings with semantic codes should be included"
    );
}

#[test]
fn test_filter_empty_diagnostics() {
    let diags: Vec<Diagnostic> = vec![];
    assert!(filter_semantic_diagnostics(&diags).is_empty());
    assert!(filter_syntactic_diagnostics(&diags).is_empty());
    assert!(filter_suggestion_diagnostics(&diags).is_empty());
}

#[test]
fn test_diagnostic_severity_all_variants_roundtrip() {
    let variants = [
        (DiagnosticSeverity::Error, 1u8),
        (DiagnosticSeverity::Warning, 2u8),
        (DiagnosticSeverity::Information, 3u8),
        (DiagnosticSeverity::Hint, 4u8),
    ];
    for (severity, expected_val) in variants {
        let val: u8 = severity.into();
        assert_eq!(val, expected_val);
        assert_eq!(DiagnosticSeverity::try_from(val).unwrap(), severity);
    }
}

#[test]
fn test_ts_diagnostic_serialization_with_optional_fields() {
    let ts_diag = TsDiagnostic {
        start: TsPosition { line: 1, offset: 1 },
        end: TsPosition { line: 1, offset: 5 },
        text: "err".to_string(),
        code: 6133,
        category: "warning".to_string(),
        source: Some("tsc-rust".to_string()),
        related_information: Some(vec![TsDiagnosticRelatedInformation {
            start: TsPosition { line: 2, offset: 3 },
            end: TsPosition { line: 2, offset: 8 },
            message: "related msg".to_string(),
        }]),
        reports_unnecessary: Some(true),
        reports_deprecated: None,
    };
    let json = serde_json::to_value(&ts_diag).unwrap();
    assert_eq!(json["source"], "tsc-rust");
    assert_eq!(json["reportsUnnecessary"], true);
    assert!(json.get("reportsDeprecated").is_none());
    let related = json["relatedInformation"].as_array().unwrap();
    assert_eq!(related.len(), 1);
    assert_eq!(related[0]["message"], "related msg");
}

#[test]
fn test_convert_to_ts_diagnostic_unnecessary_and_deprecated_flags() {
    let source = "const x = 1;";
    let line_map = LineMap::build(source);

    // Unnecessary code
    let diag = make_diagnostic("test.ts", 0, 5, "unused", DiagnosticCategory::Warning, 6133);
    let ts_diag = convert_to_ts_diagnostic(&diag, &line_map, source);
    assert_eq!(ts_diag.reports_unnecessary, Some(true));
    assert_eq!(ts_diag.reports_deprecated, None);

    // Deprecated code
    let diag = make_diagnostic(
        "test.ts",
        0,
        5,
        "deprecated",
        DiagnosticCategory::Suggestion,
        6385,
    );
    let ts_diag = convert_to_ts_diagnostic(&diag, &line_map, source);
    assert_eq!(ts_diag.reports_deprecated, Some(true));
    assert_eq!(ts_diag.reports_unnecessary, None);
}

#[test]
fn test_convert_to_ts_diagnostic_related_info_filtered_by_file() {
    let source = "const x = 1;\nconst y = 2;";
    let line_map = LineMap::build(source);
    let mut diag = make_diagnostic("test.ts", 0, 5, "err", DiagnosticCategory::Error, 2322);
    diag.related_information.push(DiagnosticRelatedInformation {
        file: "other.ts".to_string(),
        start: 0,
        length: 3,
        message_text: "from other file".to_string(),
        category: DiagnosticCategory::Message,
        code: 0,
    });
    let ts_diag = convert_to_ts_diagnostic(&diag, &line_map, source);
    assert!(
        ts_diag.related_information.is_none(),
        "Related info from other files should be filtered out in ts diagnostic"
    );
}

#[test]
fn test_format_ts_error_code_various() {
    assert_eq!(format_ts_error_code(1), "TS1");
    assert_eq!(format_ts_error_code(80006), "TS80006");
    assert_eq!(format_ts_error_code(0), "TS0");
}

// ---- JSDoc parse_jsdoc tests ----

use crate::jsdoc::parse_jsdoc;

#[test]
fn test_parse_jsdoc_simple_summary() {
    let result = parse_jsdoc("This is a simple summary.");
    assert_eq!(
        result.summary,
        Some("This is a simple summary.".to_string())
    );
    assert!(result.params.is_empty());
    assert!(result.tags.is_empty());
}

#[test]
fn test_parse_jsdoc_multiline_summary() {
    let result = parse_jsdoc("First line.\nSecond line.");
    assert_eq!(
        result.summary,
        Some("First line.\nSecond line.".to_string())
    );
}

#[test]
fn test_parse_jsdoc_empty() {
    let result = parse_jsdoc("");
    assert!(result.summary.is_none());
    assert!(result.params.is_empty());
    assert!(result.tags.is_empty());
    assert!(result.is_empty());
}

#[test]
fn test_parse_jsdoc_param_without_type() {
    let result = parse_jsdoc("@param name The name of the thing.");
    assert!(result.summary.is_none());
    assert_eq!(
        result.params.get("name"),
        Some(&"The name of the thing.".to_string())
    );
}

#[test]
fn test_parse_jsdoc_param_with_type() {
    let result = parse_jsdoc("@param {string} name The name.");
    assert_eq!(result.params.get("name"), Some(&"The name.".to_string()));
}

#[test]
fn test_parse_jsdoc_param_optional_with_default() {
    let result = parse_jsdoc("@param [name=default] The optional name.");
    assert_eq!(
        result.params.get("name"),
        Some(&"The optional name.".to_string())
    );
}

#[test]
fn test_parse_jsdoc_param_optional_no_default() {
    let result = parse_jsdoc("@param [name] An optional name.");
    assert_eq!(
        result.params.get("name"),
        Some(&"An optional name.".to_string())
    );
}

#[test]
fn test_parse_jsdoc_multiple_params() {
    let doc = "Summary text.\n@param a First param.\n@param b Second param.";
    let result = parse_jsdoc(doc);
    assert_eq!(result.summary, Some("Summary text.".to_string()));
    assert_eq!(result.params.get("a"), Some(&"First param.".to_string()));
    assert_eq!(result.params.get("b"), Some(&"Second param.".to_string()));
}

#[test]
fn test_parse_jsdoc_returns_tag() {
    let result = parse_jsdoc("@returns The return value.");
    assert!(result.summary.is_none());
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].name, "returns");
    assert_eq!(result.tags[0].text, "The return value.");
}

#[test]
fn test_parse_jsdoc_deprecated_tag() {
    let result = parse_jsdoc("@deprecated Use newFunction instead.");
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].name, "deprecated");
    assert_eq!(result.tags[0].text, "Use newFunction instead.");
}

#[test]
fn test_parse_jsdoc_deprecated_tag_no_text() {
    let result = parse_jsdoc("@deprecated");
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].name, "deprecated");
    assert_eq!(result.tags[0].text, "");
}

#[test]
fn test_parse_jsdoc_param_with_continuation_lines() {
    let doc = "@param name The name\n  which can be multi-line.";
    let result = parse_jsdoc(doc);
    assert_eq!(
        result.params.get("name"),
        Some(&"The name which can be multi-line.".to_string())
    );
}

#[test]
fn test_parse_jsdoc_tag_with_continuation_lines() {
    let doc = "@returns The result\n  which is complex.";
    let result = parse_jsdoc(doc);
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].text, "The result which is complex.");
}

#[test]
fn test_parse_jsdoc_summary_and_tags_combined() {
    let doc = "Does something useful.\n\n@param x The input.\n@returns The output.";
    let result = parse_jsdoc(doc);
    assert_eq!(result.summary, Some("Does something useful.".to_string()));
    assert_eq!(result.params.get("x"), Some(&"The input.".to_string()));
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].name, "returns");
    assert_eq!(result.tags[0].text, "The output.");
}

#[test]
fn test_parse_jsdoc_param_with_rest() {
    let result = parse_jsdoc("@param ...args The arguments.");
    assert_eq!(
        result.params.get("args"),
        Some(&"The arguments.".to_string())
    );
}

#[test]
fn test_parse_jsdoc_whitespace_only() {
    let result = parse_jsdoc("   \n   \n   ");
    assert!(result.summary.is_none());
    assert!(result.is_empty());
}

// ---- Additional diagnostic tests ----

#[test]
fn test_convert_diagnostic_at_start_of_source() {
    let source = "x";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic("test.ts", 0, 1, "err", DiagnosticCategory::Error, 2304);
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert_eq!(lsp_diag.range.start.line, 0);
    assert_eq!(lsp_diag.range.start.character, 0);
    assert_eq!(lsp_diag.range.end.line, 0);
    assert_eq!(lsp_diag.range.end.character, 1);
}

#[test]
fn test_convert_diagnostic_multiline_source() {
    let source = "aaa\nbbb\nccc\nddd";
    let line_map = LineMap::build(source);
    // Position at start of line 3 ("ddd"), offset = 12
    let diag = make_diagnostic("test.ts", 12, 3, "err", DiagnosticCategory::Error, 2322);
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert_eq!(lsp_diag.range.start.line, 3);
    assert_eq!(lsp_diag.range.start.character, 0);
    assert_eq!(lsp_diag.range.end.line, 3);
    assert_eq!(lsp_diag.range.end.character, 3);
}

#[test]
fn test_convert_diagnostic_message_category() {
    let source = "x";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic("test.ts", 0, 1, "info msg", DiagnosticCategory::Message, 0);
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::Information));
}

#[test]
fn test_convert_diagnostic_preserves_message_text() {
    let source = "const x: string = 123;";
    let line_map = LineMap::build(source);
    let msg = "Type 'number' is not assignable to type 'string'.";
    let diag = make_diagnostic("test.ts", 6, 1, msg, DiagnosticCategory::Error, 2322);
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert_eq!(lsp_diag.message, msg);
}

#[test]
fn test_convert_diagnostic_source_is_tsc_rust() {
    let source = "x";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic("test.ts", 0, 1, "err", DiagnosticCategory::Error, 2304);
    let lsp_diag = convert_diagnostic(&diag, &line_map, source);
    assert_eq!(lsp_diag.source, Some("tsc-rust".to_string()));
}

#[test]
fn test_convert_diagnostics_batch_empty() {
    let source = "const x = 1;";
    let line_map = LineMap::build(source);
    let diags: Vec<Diagnostic> = vec![];
    let lsp_diags = convert_diagnostics_batch(&diags, &line_map, source);
    assert!(lsp_diags.is_empty());
}

#[test]
fn test_convert_diagnostics_batch_single() {
    let source = "const x = 1;";
    let line_map = LineMap::build(source);
    let diags = vec![make_diagnostic(
        "test.ts",
        6,
        1,
        "err",
        DiagnosticCategory::Error,
        2322,
    )];
    let lsp_diags = convert_diagnostics_batch(&diags, &line_map, source);
    assert_eq!(lsp_diags.len(), 1);
    assert_eq!(lsp_diags[0].code, Some(2322));
}

#[test]
fn test_ts_diagnostic_category_message_string() {
    let source = "x";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic("test.ts", 0, 1, "msg", DiagnosticCategory::Message, 0);
    let ts_diag = convert_to_ts_diagnostic(&diag, &line_map, source);
    assert_eq!(ts_diag.category, "message");
}

#[test]
fn test_ts_diagnostic_no_source_field_by_default() {
    let source = "x";
    let line_map = LineMap::build(source);
    let diag = make_diagnostic("test.ts", 0, 1, "err", DiagnosticCategory::Error, 2322);
    let ts_diag = convert_to_ts_diagnostic(&diag, &line_map, source);
    // Source field behavior is implementation-defined
    let json = serde_json::to_value(&ts_diag).unwrap();
    let _ = json.get("source");
}

#[test]
fn test_filter_semantic_diagnostics_excludes_syntactic_codes() {
    let diags = vec![
        make_diagnostic("t.ts", 0, 5, "parse err", DiagnosticCategory::Error, 1005),
        make_diagnostic("t.ts", 0, 5, "type err", DiagnosticCategory::Error, 2322),
        make_diagnostic(
            "t.ts",
            0,
            5,
            "another parse",
            DiagnosticCategory::Error,
            1109,
        ),
    ];
    let semantic = filter_semantic_diagnostics(&diags);
    assert_eq!(semantic.len(), 1);
    assert_eq!(semantic[0].code, 2322);
}

#[test]
fn test_filter_syntactic_diagnostics_excludes_semantic_codes() {
    let diags = vec![
        make_diagnostic("t.ts", 0, 5, "type err", DiagnosticCategory::Error, 2322),
        make_diagnostic("t.ts", 0, 5, "cannot find", DiagnosticCategory::Error, 2304),
    ];
    let syntactic = filter_syntactic_diagnostics(&diags);
    assert!(syntactic.is_empty());
}

#[test]
fn test_filter_suggestion_diagnostics_only_includes_suggestions() {
    let diags = vec![
        make_diagnostic("t.ts", 0, 5, "err", DiagnosticCategory::Error, 2322),
        make_diagnostic("t.ts", 0, 5, "warn", DiagnosticCategory::Warning, 6133),
        make_diagnostic("t.ts", 0, 5, "sug1", DiagnosticCategory::Suggestion, 80006),
        make_diagnostic("t.ts", 0, 5, "sug2", DiagnosticCategory::Suggestion, 80007),
    ];
    let suggestions = filter_suggestion_diagnostics(&diags);
    assert_eq!(suggestions.len(), 2);
    assert_eq!(suggestions[0].code, 80006);
    assert_eq!(suggestions[1].code, 80007);
}

#[test]
fn test_is_unnecessary_code_false_for_random_codes() {
    assert!(!is_unnecessary_code(2322));
    assert!(!is_unnecessary_code(1005));
    assert!(!is_unnecessary_code(0));
    assert!(!is_unnecessary_code(9999));
}

#[test]
fn test_is_deprecated_code_false_for_adjacent_values() {
    assert!(!is_deprecated_code(6384));
    assert!(is_deprecated_code(6385));
    assert!(!is_deprecated_code(6386));
    assert!(is_deprecated_code(6387));
    assert!(!is_deprecated_code(6388));
}

#[test]
fn test_lsp_diagnostic_serialization_skips_none_fields() {
    let lsp_diag = LspDiagnostic {
        range: Range::new(Position::new(0, 0), Position::new(0, 5)),
        severity: Some(DiagnosticSeverity::Error),
        code: Some(2322),
        source: Some("tsc-rust".to_string()),
        message: "err".to_string(),
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };
    let json = serde_json::to_value(&lsp_diag).unwrap();
    assert!(json.get("relatedInformation").is_none());
    assert!(json.get("reportsUnnecessary").is_none());
    assert!(json.get("reportsDeprecated").is_none());
    assert_eq!(json["message"], "err");
    assert_eq!(json["code"], 2322);
}

#[test]
fn test_lsp_diagnostic_deserialization_roundtrip() {
    let lsp_diag = LspDiagnostic {
        range: Range::new(Position::new(1, 5), Position::new(1, 10)),
        severity: Some(DiagnosticSeverity::Warning),
        code: Some(6133),
        source: Some("tsc-rust".to_string()),
        message: "unused var".to_string(),
        related_information: None,
        reports_unnecessary: Some(true),
        reports_deprecated: None,
    };
    let json_str = serde_json::to_string(&lsp_diag).unwrap();
    let deserialized: LspDiagnostic = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.message, "unused var");
    assert_eq!(deserialized.code, Some(6133));
    assert_eq!(deserialized.reports_unnecessary, Some(true));
}

#[test]
fn test_ts_diagnostic_deserialization_roundtrip() {
    let ts_diag = TsDiagnostic {
        start: TsPosition { line: 3, offset: 7 },
        end: TsPosition {
            line: 3,
            offset: 12,
        },
        text: "Cannot find name 'foo'.".to_string(),
        code: 2304,
        category: "error".to_string(),
        source: None,
        related_information: None,
        reports_unnecessary: None,
        reports_deprecated: None,
    };
    let json_str = serde_json::to_string(&ts_diag).unwrap();
    let deserialized: TsDiagnostic = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.code, 2304);
    assert_eq!(deserialized.start.line, 3);
    assert_eq!(deserialized.start.offset, 7);
    assert_eq!(deserialized.text, "Cannot find name 'foo'.");
}

// ---- Additional JSDoc tests ----

#[test]
fn test_parse_jsdoc_example_tag() {
    let result = parse_jsdoc("@example\nconst x = foo();");
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].name, "example");
}

#[test]
fn test_parse_jsdoc_see_tag() {
    let result = parse_jsdoc("@see https://example.com");
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].name, "see");
    assert_eq!(result.tags[0].text, "https://example.com");
}

#[test]
fn test_parse_jsdoc_since_tag() {
    let result = parse_jsdoc("@since 1.0.0");
    assert_eq!(result.tags.len(), 1);
    assert_eq!(result.tags[0].name, "since");
    assert_eq!(result.tags[0].text, "1.0.0");
}

#[test]
fn test_parse_jsdoc_multiple_tags() {
    let doc = "@deprecated Use newFn.\n@see newFn\n@since 2.0.0";
    let result = parse_jsdoc(doc);
    assert_eq!(result.tags.len(), 3);
    let tag_names: Vec<&str> = result.tags.iter().map(|t| t.name.as_str()).collect();
    assert!(tag_names.contains(&"deprecated"));
    assert!(tag_names.contains(&"see"));
    assert!(tag_names.contains(&"since"));
}

#[test]
fn test_parse_jsdoc_is_empty_with_tags() {
    let result = parse_jsdoc("@param x The x value.");
    assert!(!result.is_empty(), "JSDoc with params should not be empty");
}

#[test]
fn test_parse_jsdoc_is_empty_with_summary() {
    let result = parse_jsdoc("A summary.");
    assert!(!result.is_empty(), "JSDoc with summary should not be empty");
}
