//! LSP diagnostics and conversion from checker diagnostics.

use serde::{Deserialize, Serialize};

use crate::checker::types::diagnostics::{Diagnostic as CheckerDiagnostic, DiagnosticCategory};
use crate::lsp::position::{LineMap, Location, Range};

const DIAGNOSTIC_SOURCE: &str = "tsc-rust";

/// Diagnostic severity level (matches LSP).
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

/// LSP diagnostic payload.
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
}

/// Related diagnostic information for LSP clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspDiagnosticRelatedInformation {
    pub location: Location,
    pub message: String,
}

/// Convert a checker diagnostic to an LSP diagnostic.
pub fn convert_diagnostic(
    diag: &CheckerDiagnostic,
    line_map: &LineMap,
    source: &str,
) -> LspDiagnostic {
    let start = line_map.offset_to_position(diag.start, source);
    let end = line_map.offset_to_position(diag.start.saturating_add(diag.length), source);
    let severity = match diag.category {
        DiagnosticCategory::Error => DiagnosticSeverity::Error,
        DiagnosticCategory::Warning => DiagnosticSeverity::Warning,
        DiagnosticCategory::Suggestion => DiagnosticSeverity::Hint,
        DiagnosticCategory::Message => DiagnosticSeverity::Information,
    };

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

    LspDiagnostic {
        range: Range::new(start, end),
        severity: Some(severity),
        code: Some(diag.code),
        source: Some(DIAGNOSTIC_SOURCE.to_string()),
        message: diag.message_text.clone(),
        related_information,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::types::diagnostics::{
        Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation,
    };
    use crate::lsp::position::LineMap;

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
}
