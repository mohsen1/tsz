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

use tsz_checker::diagnostics::{
    Diagnostic as CheckerDiagnostic, DiagnosticCategory, diagnostic_codes,
};
use tsz_common::position::{LineMap, Location, Position, Range};

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
    fn from(severity: DiagnosticSeverity) -> Self {
        severity as Self
    }
}

impl TryFrom<u8> for DiagnosticSeverity {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, <Self as TryFrom<u8>>::Error> {
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
pub const fn category_to_string(category: DiagnosticCategory) -> &'static str {
    match category {
        DiagnosticCategory::Error => "error",
        DiagnosticCategory::Warning => "warning",
        DiagnosticCategory::Suggestion => "suggestion",
        DiagnosticCategory::Message => "message",
    }
}

/// Map a checker `DiagnosticCategory` to the LSP `DiagnosticSeverity`.
pub const fn category_to_severity(category: DiagnosticCategory) -> DiagnosticSeverity {
    match category {
        DiagnosticCategory::Error => DiagnosticSeverity::Error,
        DiagnosticCategory::Warning => DiagnosticSeverity::Warning,
        DiagnosticCategory::Suggestion => DiagnosticSeverity::Hint,
        DiagnosticCategory::Message => DiagnosticSeverity::Information,
    }
}

/// Returns true if the diagnostic code represents an "unused" or "unnecessary" construct.
pub const fn is_unnecessary_code(code: u32) -> bool {
    matches!(
        code,
        diagnostic_codes::IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ
            | diagnostic_codes::ALL_VARIABLES_ARE_UNUSED
            | diagnostic_codes::UNREACHABLE_CODE_DETECTED
            | diagnostic_codes::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS
            | diagnostic_codes::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS
            | 6196
            | 6138
    )
}

/// Returns true if the diagnostic code represents a deprecated symbol usage.
pub const fn is_deprecated_code(code: u32) -> bool {
    matches!(code, 6385 | 6387)
}

/// Returns true if the code falls in the syntactic/parser error range (1xxx).
pub fn is_syntactic_error_code(code: u32) -> bool {
    tsz_common::diagnostics::is_parser_grammar_diagnostic(code)
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

    let reports_unnecessary = is_unnecessary_code(diag.code).then_some(true);
    let reports_deprecated = is_deprecated_code(diag.code).then_some(true);

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

/// Convert an LSP Position (0-indexed) to a tsserver `TsPosition` (1-indexed).
const fn lsp_to_ts_position(pos: Position) -> TsPosition {
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

    let reports_unnecessary = is_unnecessary_code(diag.code).then_some(true);
    let reports_deprecated = is_deprecated_code(diag.code).then_some(true);

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
    format!("TS{code}")
}

// ---------------------------------------------------------------------------
// Workspace diagnostics (pull model)
// ---------------------------------------------------------------------------

/// The result kind for a workspace diagnostic report item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentDiagnosticReportKind {
    /// Full set of diagnostics for the document.
    Full,
    /// Diagnostics unchanged since last request (use `result_id` to verify).
    Unchanged,
}

/// A full diagnostic report for a single document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullDocumentDiagnosticReport {
    /// Discriminator — always `Full`.
    pub kind: DocumentDiagnosticReportKind,
    /// An optional result ID to support incremental updates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_id: Option<String>,
    /// The actual diagnostics.
    pub items: Vec<LspDiagnostic>,
}

/// An unchanged diagnostic report for a single document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnchangedDocumentDiagnosticReport {
    /// Discriminator — always `Unchanged`.
    pub kind: DocumentDiagnosticReportKind,
    /// The result ID from the previous request.
    pub result_id: String,
}

/// A workspace diagnostic report item — either full or unchanged per document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDiagnosticReportItem {
    /// The URI of the document.
    pub uri: String,
    /// An optional version number of the document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,
    /// Discriminator — `Full` or `Unchanged`.
    pub kind: DocumentDiagnosticReportKind,
    /// Present when `kind == Full`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_id: Option<String>,
    /// Present when `kind == Full`. The diagnostics for this document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<LspDiagnostic>>,
}

/// The full workspace diagnostic report returned by `workspace/diagnostic`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDiagnosticReport {
    pub items: Vec<WorkspaceDiagnosticReportItem>,
}

impl WorkspaceDiagnosticReport {
    /// Create a workspace diagnostic report from a map of file → diagnostics.
    pub fn from_file_diagnostics(
        file_diagnostics: impl IntoIterator<Item = (String, Vec<LspDiagnostic>)>,
    ) -> Self {
        let items = file_diagnostics
            .into_iter()
            .map(|(uri, diagnostics)| WorkspaceDiagnosticReportItem {
                uri,
                version: None,
                kind: DocumentDiagnosticReportKind::Full,
                result_id: None,
                items: Some(diagnostics),
            })
            .collect();
        Self { items }
    }

    /// Create an empty report (no diagnostics for any file).
    pub const fn empty() -> Self {
        Self { items: Vec::new() }
    }
}

#[cfg(test)]
#[path = "../../tests/diagnostics_tests.rs"]
mod diagnostics_tests;
