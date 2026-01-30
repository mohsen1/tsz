//! LSP diagnostics and conversion from checker diagnostics.
//!
//! This module provides:
//! - `LspDiagnostic` - the LSP-native diagnostic format (used by LSP clients)
//! - `TsDiagnostic` - the tsserver-compatible diagnostic format (used by fourslash tests)
//! - Conversion functions between checker diagnostics and both output formats
//! - Filtering helpers for semantic, syntactic, and suggestion diagnostics
//!
//! ## tsserver response format
//!
//! The `semanticDiagnosticsSync` command returns:
//! ```json
//! [{"start":{"line":1,"offset":5},"end":{"line":1,"offset":10},
//!   "text":"Type 'number' is not assignable to type 'string'.",
//!   "code":2322,"category":"error"}]
//! ```
//!
//! The `syntacticDiagnosticsSync` command returns parser errors in the same format.
//! The `suggestionDiagnosticsSync` command returns suggestion-level diagnostics.

use serde::{Deserialize, Serialize};

use crate::checker::types::diagnostics::{
    Diagnostic as CheckerDiagnostic, DiagnosticCategory, diagnostic_codes,
};
use crate::lsp::position::{LineMap, Location, Position, Range};

const DIAGNOSTIC_SOURCE: &str = "tsc-rust";

/// Diagnostic severity level (matches LSP specification).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

impl From<DiagnosticSeverity> for u8 {
    fn from(severity: DiagnosticSeverity) -> u8 {
        severity as u8
    }
}

impl TryFrom<u8> for DiagnosticSeverity {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, <DiagnosticSeverity as TryFrom<u8>>::Error> {
        match value {
            1 => Ok(Self::Error),
            2 => Ok(Self::Warning),
            3 => Ok(Self::Information),
            4 => Ok(Self::Hint),
            _ => Err("invalid diagnostic severity"),
        }
    }
}

/// LSP diagnostic payload used by LSP clients (VS Code, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspDiagnostic {
    pub range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<DiagnosticSeverity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_information: Option<Vec<LspDiagnosticRelatedInformation>>,
    /// Set to true when the diagnostic marks code that is unnecessary (unused variables, etc.).
    /// LSP clients may render this with a fade-out effect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reports_unnecessary: Option<bool>,
    /// Set to true when the diagnostic marks code that uses a deprecated API.
    /// LSP clients may render this with a strikethrough effect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reports_deprecated: Option<bool>,
}

/// Related diagnostic information for LSP clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspDiagnosticRelatedInformation {
    pub location: Location,
    pub message: String,
}

/// A position in the tsserver protocol format.
/// tsserver uses 1-indexed line and offset (column).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsPosition {
    pub line: u32,
    pub offset: u32,
}

/// tsserver-compatible diagnostic format.
///
/// This is the format returned by `semanticDiagnosticsSync`,
/// `syntacticDiagnosticsSync`, and `suggestionDiagnosticsSync`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TsDiagnostic {
    pub start: TsPosition,
    pub end: TsPosition,
    pub text: String,
    pub code: u32,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_information: Option<Vec<TsDiagnosticRelatedInformation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reports_unnecessary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reports_deprecated: Option<bool>,
}

/// Related information in the tsserver diagnostic format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TsDiagnosticRelatedInformation {
    pub start: TsPosition,
    pub end: TsPosition,
    pub message: String,
}

/// Returns the tsserver category string for a `DiagnosticCategory`.
pub fn category_to_string(category: DiagnosticCategory) -> &'static str {
    match category {
        DiagnosticCategory::Error => "error",
        DiagnosticCategory::Warning => "warning",
        DiagnosticCategory::Suggestion => "suggestion",
        DiagnosticCategory::Message => "message",
    }
}

/// Map a checker `DiagnosticCategory` to the LSP `DiagnosticSeverity`.
pub fn category_to_severity(category: DiagnosticCategory) -> DiagnosticSeverity {
    match category {
        DiagnosticCategory::Error => DiagnosticSeverity::Error,
        DiagnosticCategory::Warning => DiagnosticSeverity::Warning,
        DiagnosticCategory::Suggestion => DiagnosticSeverity::Hint,
        DiagnosticCategory::Message => DiagnosticSeverity::Information,
    }
}

/// Returns true if the diagnostic code represents an "unused" or "unnecessary" construct.
pub fn is_unnecessary_code(code: u32) -> bool {
    matches!(
        code,
        diagnostic_codes::UNUSED_VARIABLE
            | diagnostic_codes::UNREACHABLE_CODE_DETECTED
            | diagnostic_codes::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS
            | diagnostic_codes::ASYNC_FUNCTION_WITHOUT_AWAIT
            | 6196
            | 6138
    )
}

/// Returns true if the diagnostic code represents a deprecated symbol usage.
pub fn is_deprecated_code(code: u32) -> bool {
    matches!(code, 6385 | 6387)
}

/// Returns true if the code falls in the syntactic/parser error range (1xxx).
pub fn is_syntactic_error_code(code: u32) -> bool {
    (1000..2000).contains(&code)
}

/// Returns true if the code falls in the semantic/type-checking error range (2xxx+).
pub fn is_semantic_error_code(code: u32) -> bool {
    !is_syntactic_error_code(code)
}

/// Returns true if the diagnostic should be classified as a suggestion.
pub fn is_suggestion_diagnostic(diag: &CheckerDiagnostic) -> bool {
    diag.category == DiagnosticCategory::Suggestion
}

/// Convert a checker diagnostic to an LSP diagnostic.
pub fn convert_diagnostic(
    diag: &CheckerDiagnostic,
    line_map: &LineMap,
    source: &str,
) -> LspDiagnostic {
    let start = line_map.offset_to_position(diag.start, source);
    let end = line_map.offset_to_position(diag.start.saturating_add(diag.length), source);
    let severity = category_to_severity(diag.category);

    let related_information = if diag.related_information.is_empty() {
        None
    } else {
        let items: Vec<_> = diag
            .related_information
            .iter()
            .filter(|related| related.file == diag.file)
            .map(|related| {
                let related_start = line_map.offset_to_position(related.start, source);
                let related_end = line_map
                    .offset_to_position(related.start.saturating_add(related.length), source);
                LspDiagnosticRelatedInformation {
                    location: Location {
                        file_path: related.file.clone(),
                        range: Range::new(related_start, related_end),
                    },
                    message: related.message_text.clone(),
                }
            })
            .collect();
        if items.is_empty() { None } else { Some(items) }
    };

    let reports_unnecessary = if is_unnecessary_code(diag.code) {
        Some(true)
    } else {
        None
    };
    let reports_deprecated = if is_deprecated_code(diag.code) {
        Some(true)
    } else {
        None
    };

    LspDiagnostic {
        range: Range::new(start, end),
        severity: Some(severity),
        code: Some(diag.code),
        source: Some(DIAGNOSTIC_SOURCE.to_string()),
        message: diag.message_text.clone(),
        related_information,
        reports_unnecessary,
        reports_deprecated,
    }
}

/// Convert multiple checker diagnostics to LSP diagnostics.
pub fn convert_diagnostics_batch(
    diagnostics: &[CheckerDiagnostic],
    line_map: &LineMap,
    source: &str,
) -> Vec<LspDiagnostic> {
    diagnostics
        .iter()
        .map(|diag| convert_diagnostic(diag, line_map, source))
        .collect()
}

/// Convert an LSP Position (0-indexed) to a tsserver TsPosition (1-indexed).
fn lsp_to_ts_position(pos: Position) -> TsPosition {
    TsPosition {
        line: pos.line + 1,
        offset: pos.character + 1,
    }
}

/// Convert a checker diagnostic to a tsserver-compatible diagnostic.
pub fn convert_to_ts_diagnostic(
    diag: &CheckerDiagnostic,
    line_map: &LineMap,
    source: &str,
) -> TsDiagnostic {
    let start = line_map.offset_to_position(diag.start, source);
    let end = line_map.offset_to_position(diag.start.saturating_add(diag.length), source);
    let category = category_to_string(diag.category);

    let related_information = if diag.related_information.is_empty() {
        None
    } else {
        let items: Vec<_> = diag
            .related_information
            .iter()
            .filter(|related| related.file == diag.file)
            .map(|related| {
                let related_start = line_map.offset_to_position(related.start, source);
                let related_end = line_map
                    .offset_to_position(related.start.saturating_add(related.length), source);
                TsDiagnosticRelatedInformation {
                    start: lsp_to_ts_position(related_start),
                    end: lsp_to_ts_position(related_end),
                    message: related.message_text.clone(),
                }
            })
            .collect();
        if items.is_empty() { None } else { Some(items) }
    };

    let reports_unnecessary = if is_unnecessary_code(diag.code) {
        Some(true)
    } else {
        None
    };
    let reports_deprecated = if is_deprecated_code(diag.code) {
        Some(true)
    } else {
        None
    };

    TsDiagnostic {
        start: lsp_to_ts_position(start),
        end: lsp_to_ts_position(end),
        text: diag.message_text.clone(),
        code: diag.code,
        category: category.to_string(),
        source: Some(DIAGNOSTIC_SOURCE.to_string()),
        related_information,
        reports_unnecessary,
        reports_deprecated,
    }
}

/// Convert multiple checker diagnostics to tsserver format.
pub fn convert_to_ts_diagnostics_batch(
    diagnostics: &[CheckerDiagnostic],
    line_map: &LineMap,
    source: &str,
) -> Vec<TsDiagnostic> {
    diagnostics
        .iter()
        .map(|diag| convert_to_ts_diagnostic(diag, line_map, source))
        .collect()
}

/// Filter diagnostics for `semanticDiagnosticsSync`.
pub fn filter_semantic_diagnostics(diagnostics: &[CheckerDiagnostic]) -> Vec<&CheckerDiagnostic> {
    diagnostics
        .iter()
        .filter(|d| is_semantic_error_code(d.code) && d.category != DiagnosticCategory::Suggestion)
        .collect()
}

/// Filter diagnostics for `syntacticDiagnosticsSync`.
pub fn filter_syntactic_diagnostics(diagnostics: &[CheckerDiagnostic]) -> Vec<&CheckerDiagnostic> {
    diagnostics
        .iter()
        .filter(|d| is_syntactic_error_code(d.code))
        .collect()
}

/// Filter diagnostics for `suggestionDiagnosticsSync`.
pub fn filter_suggestion_diagnostics(diagnostics: &[CheckerDiagnostic]) -> Vec<&CheckerDiagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::Suggestion)
        .collect()
}

/// Format a TypeScript error code string, e.g., "TS2322".
pub fn format_ts_error_code(code: u32) -> String {
    format!("TS{}", code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::types::diagnostics::{
        Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    };
    use crate::lsp::position::LineMap;

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
            diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
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
        assert!(is_unnecessary_code(diagnostic_codes::UNUSED_VARIABLE));
        assert!(is_unnecessary_code(
            diagnostic_codes::UNREACHABLE_CODE_DETECTED
        ));
        assert!(is_unnecessary_code(
            diagnostic_codes::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS
        ));
        assert!(is_unnecessary_code(
            diagnostic_codes::ASYNC_FUNCTION_WITHOUT_AWAIT
        ));
        assert!(!is_unnecessary_code(
            diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE
        ));
        assert!(!is_unnecessary_code(diagnostic_codes::CANNOT_FIND_NAME));
    }

    #[test]
    fn test_is_deprecated_code_detection() {
        assert!(is_deprecated_code(6385));
        assert!(is_deprecated_code(6387));
        assert!(!is_deprecated_code(
            diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE
        ));
        assert!(!is_deprecated_code(diagnostic_codes::UNUSED_VARIABLE));
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
            diagnostic_codes::UNUSED_VARIABLE,
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
            diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
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
}
