//! Structured diagnostics. Rendering is a process-adapter concern.
use crate::source::{FileId, SourceCoordinateIndex, SourceText, Span};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum DiagnosticPhase {
    #[default]
    Config,
    Program,
    Both,
}
macro_rules! diagnostic_categories {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "lowercase")]
        pub enum DiagnosticCategory { $($variant),+ }
        impl DiagnosticCategory {
            #[must_use]
            pub const fn as_str(self) -> &'static str { match self { $(Self::$variant => $name),+ } }
        }
    };
}
diagnostic_categories! { Warning => "warning", Error => "error", Suggestion => "suggestion", Message => "message" }
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RelatedInformation {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub code: u32,
    #[serde(default)]
    pub depth: u32,
}
impl RelatedInformation {
    #[must_use]
    pub fn unlocated(message_text: impl Into<String>, code: u32, depth: u32) -> Self {
        Self {
            message_text: message_text.into(),
            code,
            depth,
            ..Self::default()
        }
    }
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
    #[serde(skip)]
    pub(crate) phase: DiagnosticPhase,
    #[serde(skip)]
    external_source: Option<Arc<str>>,
}
impl Diagnostic {
    fn identity(
        &self,
    ) -> (
        &str,
        u32,
        u32,
        u32,
        &str,
        DiagnosticCategory,
        &[RelatedInformation],
    ) {
        (
            &self.file,
            self.start,
            self.length,
            self.code,
            &self.message_text,
            self.category,
            &self.related_information,
        )
    }
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
            phase: DiagnosticPhase::Config,
            external_source: None,
        }
    }
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
        let mut diagnostic = Self::error(file, start, length, message_text, code);
        diagnostic.external_source = Some(source_text);
        diagnostic
    }
    #[must_use]
    pub fn at(source: &SourceText, span: Span, message_text: String, code: u32) -> Self {
        let (start, length) = source.utf16_span(span);
        let mut diagnostic = Self::error(
            source.path.to_string_lossy().replace('\\', "/"),
            start,
            length,
            message_text,
            code,
        );
        diagnostic.file_id = Some(source.id);
        diagnostic
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
        let suffix = format!(
            "{} TS{}: {}",
            self.category.as_str(),
            self.code,
            self.message_text
        );
        let mut rendered = if let Some(source) = source {
            let (line, column) = source.line_and_column(self.start);
            format!(
                "{}({line},{column}): {suffix}",
                source.path.to_string_lossy().replace('\\', "/")
            )
        } else if let Some(source_text) = &self.external_source {
            let (line, column) =
                SourceCoordinateIndex::new(source_text).position_from_utf16(self.start);
            format!("{}({line},{column}): {suffix}", self.file)
        } else if self.file.is_empty() {
            suffix
        } else {
            format!("{}: {suffix}", self.file)
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
pub fn sort_and_deduplicate(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.sort_by(|left, right| {
        left.identity()
            .cmp(&right.identity())
            .then_with(|| left.file_id.cmp(&right.file_id))
    });
    diagnostics.dedup_by(merge_duplicate_diagnostics);
}
pub(crate) fn sort_and_deduplicate_by_path(diagnostics: &mut Vec<Diagnostic>) {
    sort_and_deduplicate(diagnostics);
    diagnostics.sort_by(|left, right| left.file.cmp(&right.file));
}
pub(crate) fn sort_and_deduplicate_for_cli(diagnostics: &mut Vec<Diagnostic>) {
    sort_and_deduplicate(diagnostics);
    let bucket = |diagnostic: &Diagnostic| match diagnostic.file_id {
        _ if diagnostic.file.is_empty() => 0,
        Some(FileId(u32::MAX)) => 2,
        _ => 1,
    };
    diagnostics
        .sort_by(|left, right| (bucket(left), &left.file).cmp(&(bucket(right), &right.file)));
}
fn merge_duplicate_diagnostics(left: &mut Diagnostic, right: &mut Diagnostic) -> bool {
    let duplicate = left.identity() == right.identity();
    if duplicate && left.phase != right.phase {
        left.phase = DiagnosticPhase::Both;
        right.phase = DiagnosticPhase::Both;
    }
    duplicate
}
#[cfg(test)]
#[path = "../rewrite-tests/diagnostics_unit.rs"]
mod tests;
