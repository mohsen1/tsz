//! Structured diagnostics. Rendering is a process-adapter concern.

use std::cmp::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::source::{FileId, SourceCoordinateIndex, SourceText, Span};

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
    /// One-based nesting within the primary diagnostic's message chain.
    #[serde(default)]
    pub depth: u32,
}

impl RelatedInformation {
    #[must_use]
    pub fn unlocated(message_text: impl Into<String>, code: u32, depth: u32) -> Self {
        Self {
            file: String::new(),
            start: 0,
            length: 0,
            message_text: message_text.into(),
            code,
            depth,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub file: String,
    /// Absolute TypeScript source position in UTF-16 code units.
    pub start: u32,
    /// Diagnostic extent in UTF-16 code units.
    pub length: u32,
    pub message_text: String,
    pub category: DiagnosticCategory,
    pub code: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_information: Vec<RelatedInformation>,
    #[serde(skip)]
    pub(crate) file_id: Option<FileId>,
    /// Text for a diagnostic owned by a non-program input such as
    /// `tsconfig.json`. This keeps byte spans structured while still allowing
    /// the process adapter to render line and column information.
    #[serde(skip)]
    external_source: Option<Arc<str>>,
}

impl Diagnostic {
    #[must_use]
    pub const fn source_file_id(&self) -> Option<FileId> {
        self.file_id
    }

    /// Construct a diagnostic whose coordinates are already public UTF-16
    /// units, or whose location is global when `file` is empty.
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
            external_source: None,
        }
    }

    /// Construct a diagnostic from byte offsets in retained external source
    /// text, such as `tsconfig.json`.
    #[must_use]
    pub fn error_at_text(
        file: String,
        start: u32,
        length: u32,
        source_text: Arc<str>,
        message_text: String,
        code: u32,
    ) -> Self {
        let coordinates = SourceCoordinateIndex::new(&source_text);
        let (start, length) =
            coordinates.byte_span(&source_text, start, start.saturating_add(length));
        Self {
            file,
            start,
            length,
            message_text,
            category: DiagnosticCategory::Error,
            code,
            related_information: Vec::new(),
            file_id: None,
            external_source: Some(source_text),
        }
    }

    #[must_use]
    pub fn at(source: &SourceText, span: Span, message_text: String, code: u32) -> Self {
        let (start, length) = source.utf16_span(span);
        Self {
            file: source.path.to_string_lossy().replace('\\', "/"),
            start,
            length,
            message_text,
            category: DiagnosticCategory::Error,
            code,
            related_information: Vec::new(),
            file_id: Some(source.id),
            external_source: None,
        }
    }

    #[must_use]
    pub const fn global(message_text: String, code: u32) -> Self {
        Self::error(String::new(), 0, 0, message_text, code)
    }

    #[must_use]
    pub fn with_related_information(
        mut self,
        related_information: Vec<RelatedInformation>,
    ) -> Self {
        self.related_information = related_information;
        self
    }

    #[must_use]
    pub fn render(&self, source: Option<&SourceText>) -> String {
        let mut rendered = if let Some(source) = source {
            let (line, column) = source.line_and_column(self.start);
            format!(
                "{}({line},{column}): {} TS{}: {}",
                source.path.to_string_lossy().replace('\\', "/"),
                self.category.as_str(),
                self.code,
                self.message_text
            )
        } else if let Some(source_text) = &self.external_source {
            let (line, column) =
                SourceCoordinateIndex::new(source_text).line_and_column(self.start);
            format!(
                "{}({line},{column}): {} TS{}: {}",
                self.file,
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
        };
        for related in &self.related_information {
            rendered.push('\n');
            let depth = related.depth.max(1) as usize;
            rendered.push_str(&"  ".repeat(depth));
            rendered.push_str(&related.message_text);
        }
        rendered
    }
}

/// Merge worker-private diagnostic buffers under one deterministic total key.
pub fn sort_and_deduplicate(diagnostics: &mut Vec<Diagnostic>) {
    sort_and_deduplicate_by(diagnostics, |diagnostic| diagnostic.file_id);
}

fn sort_and_deduplicate_by<K: Ord>(
    diagnostics: &mut Vec<Diagnostic>,
    file_key: impl Fn(&Diagnostic) -> Option<K>,
) {
    diagnostics.sort_by(|left, right| {
        let left_key = (
            file_key(left),
            &left.file,
            left.start,
            left.code,
            left.category,
            &left.message_text,
        );
        let right_key = (
            file_key(right),
            &right.file,
            right.start,
            right.code,
            right.category,
            &right.message_text,
        );
        let ordering = left_key.cmp(&right_key);
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
            && left.related_information == right.related_information
    });
}

#[cfg(test)]
#[path = "../rewrite-tests/diagnostics_unit.rs"]
mod tests;
