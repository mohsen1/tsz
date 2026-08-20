//! Structured diagnostics. Rendering is a process-adapter concern.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::source::{FileId, SourceText, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticCategory {
    Warning,
    Error,
    Suggestion,
    Message,
}

impl DiagnosticCategory {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Suggestion => "suggestion",
            Self::Message => "message",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedInformation {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub code: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub category: DiagnosticCategory,
    pub code: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_information: Vec<RelatedInformation>,
    #[serde(skip)]
    pub(crate) file_id: Option<FileId>,
}

impl Diagnostic {
    #[must_use]
    pub const fn source_file_id(&self) -> Option<FileId> {
        self.file_id
    }

    #[must_use]
    pub const fn error(
        file: String,
        start: u32,
        length: u32,
        message_text: String,
        code: u32,
    ) -> Self {
        Self {
            file,
            start,
            length,
            message_text,
            category: DiagnosticCategory::Error,
            code,
            related_information: Vec::new(),
            file_id: None,
        }
    }

    #[must_use]
    pub fn at(source: &SourceText, span: Span, message_text: String, code: u32) -> Self {
        Self {
            file: source.path.to_string_lossy().replace('\\', "/"),
            start: span.start,
            length: span.len(),
            message_text,
            category: DiagnosticCategory::Error,
            code,
            related_information: Vec::new(),
            file_id: Some(source.id),
        }
    }

    #[must_use]
    pub const fn global(message_text: String, code: u32) -> Self {
        Self::error(String::new(), 0, 0, message_text, code)
    }

    #[must_use]
    pub fn render(&self, source: Option<&SourceText>) -> String {
        if let Some(source) = source {
            let (line, column) = source.line_and_column(self.start);
            format!(
                "{}({line},{column}): {} TS{}: {}",
                source.path.to_string_lossy().replace('\\', "/"),
                self.category.as_str(),
                self.code,
                self.message_text
            )
        } else if self.file.is_empty() {
            format!(
                "{} TS{}: {}",
                self.category.as_str(),
                self.code,
                self.message_text
            )
        } else {
            format!(
                "{}: {} TS{}: {}",
                self.file,
                self.category.as_str(),
                self.code,
                self.message_text
            )
        }
    }

    fn sort_key(&self) -> (Option<FileId>, &str, u32, u32, DiagnosticCategory, &str) {
        (
            self.file_id,
            &self.file,
            self.start,
            self.code,
            self.category,
            &self.message_text,
        )
    }
}

/// Merge worker-private diagnostic buffers under one deterministic total key.
pub fn sort_and_deduplicate(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.sort_by(|left, right| {
        let ordering = left.sort_key().cmp(&right.sort_key());
        if ordering == Ordering::Equal {
            left.length.cmp(&right.length)
        } else {
            ordering
        }
    });
    diagnostics.dedup_by(|left, right| {
        left.file == right.file
            && left.start == right.start
            && left.length == right.length
            && left.code == right.code
            && left.category == right.category
            && left.message_text == right.message_text
    });
}
