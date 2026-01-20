//! Diagnostic Infrastructure
//!
//! This module provides infrastructure for collecting and formatting compilation
//! errors and warnings. It is designed to work with AST nodes and spans rather
//! than raw string positions.
//!
//! # Components
//!
//! - `Diagnostic` - A single diagnostic message with location and severity
//! - `DiagnosticBag` - A collection of diagnostics for a compilation phase
//! - `DiagnosticSeverity` - Error, Warning, Info, or Hint
//! - `DiagnosticCode` - TypeScript-compatible error codes
//!
//! # Example
//!
//! ```ignore
//! let mut bag = DiagnosticBag::new();
//! bag.error(span, "Cannot find name 'foo'", 2304);
//! bag.warning(span, "Unused variable", 6133);
//!
//! for diag in bag.iter() {
//!     println!("{}", diag.format(&source));
//! }
//! ```

use crate::lsp::position::Range;
use crate::source_file::SourceFile;
use crate::span::Span;
use serde::{Deserialize, Serialize};
use std::fmt;

// =============================================================================
// Diagnostic Severity
// =============================================================================

/// The severity level of a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    /// A hint (lowest severity)
    Hint = 4,
    /// Informational message
    Info = 3,
    /// A warning
    Warning = 2,
    /// An error (highest severity)
    Error = 1,
}

impl DiagnosticSeverity {
    /// Get the severity name for display.
    pub fn name(&self) -> &'static str {
        match self {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Info => "info",
            DiagnosticSeverity::Hint => "hint",
        }
    }

    /// Check if this is an error.
    pub fn is_error(&self) -> bool {
        matches!(self, DiagnosticSeverity::Error)
    }

    /// Check if this is a warning.
    pub fn is_warning(&self) -> bool {
        matches!(self, DiagnosticSeverity::Warning)
    }
}

impl fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl Default for DiagnosticSeverity {
    fn default() -> Self {
        DiagnosticSeverity::Error
    }
}

// =============================================================================
// Related Information
// =============================================================================

/// Additional information related to a diagnostic.
///
/// This is used to provide "see also" locations, such as where a type
/// was declared when reporting a type mismatch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiagnosticRelatedInfo {
    /// File containing the related information
    pub file_name: String,
    /// Location span
    pub span: Span,
    /// Message explaining the relationship
    pub message: String,
}

impl DiagnosticRelatedInfo {
    /// Create new related information.
    pub fn new(file_name: impl Into<String>, span: Span, message: impl Into<String>) -> Self {
        DiagnosticRelatedInfo {
            file_name: file_name.into(),
            span,
            message: message.into(),
        }
    }
}

// =============================================================================
// Diagnostic
// =============================================================================

/// A diagnostic message with location, severity, and error code.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagnostic {
    /// The file containing the diagnostic
    pub file_name: String,
    /// The source span (byte offsets)
    pub span: Span,
    /// The diagnostic message
    pub message: String,
    /// The severity level
    pub severity: DiagnosticSeverity,
    /// The diagnostic code (e.g., TS2304)
    pub code: u32,
    /// Optional related information
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub related: Vec<DiagnosticRelatedInfo>,
    /// Optional source string (e.g., "typescript")
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source: Option<String>,
}

impl Diagnostic {
    /// Create a new diagnostic.
    pub fn new(
        file_name: impl Into<String>,
        span: Span,
        message: impl Into<String>,
        severity: DiagnosticSeverity,
        code: u32,
    ) -> Self {
        Diagnostic {
            file_name: file_name.into(),
            span,
            message: message.into(),
            severity,
            code,
            related: Vec::new(),
            source: Some("typescript".to_string()),
        }
    }

    /// Create an error diagnostic.
    pub fn error(
        file_name: impl Into<String>,
        span: Span,
        message: impl Into<String>,
        code: u32,
    ) -> Self {
        Self::new(file_name, span, message, DiagnosticSeverity::Error, code)
    }

    /// Create a warning diagnostic.
    pub fn warning(
        file_name: impl Into<String>,
        span: Span,
        message: impl Into<String>,
        code: u32,
    ) -> Self {
        Self::new(file_name, span, message, DiagnosticSeverity::Warning, code)
    }

    /// Create an info diagnostic.
    pub fn info(
        file_name: impl Into<String>,
        span: Span,
        message: impl Into<String>,
        code: u32,
    ) -> Self {
        Self::new(file_name, span, message, DiagnosticSeverity::Info, code)
    }

    /// Create a hint diagnostic.
    pub fn hint(
        file_name: impl Into<String>,
        span: Span,
        message: impl Into<String>,
        code: u32,
    ) -> Self {
        Self::new(file_name, span, message, DiagnosticSeverity::Hint, code)
    }

    /// Add related information.
    pub fn with_related(mut self, info: DiagnosticRelatedInfo) -> Self {
        self.related.push(info);
        self
    }

    /// Add multiple related information items.
    pub fn with_related_all(mut self, infos: Vec<DiagnosticRelatedInfo>) -> Self {
        self.related.extend(infos);
        self
    }

    /// Set the source identifier.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Check if this is an error.
    pub fn is_error(&self) -> bool {
        self.severity.is_error()
    }

    /// Check if this is a warning.
    pub fn is_warning(&self) -> bool {
        self.severity.is_warning()
    }

    /// Get the start position (byte offset).
    pub fn start(&self) -> u32 {
        self.span.start
    }

    /// Get the length.
    pub fn length(&self) -> u32 {
        self.span.len()
    }

    /// Format the diagnostic for display.
    ///
    /// Returns a string like: "file.ts(1,5): error TS2304: Cannot find name 'foo'."
    pub fn format(&self, source_file: &mut SourceFile) -> String {
        let pos = source_file.offset_to_position(self.span.start);
        format!(
            "{}({},{}): {} TS{}: {}",
            self.file_name,
            pos.line + 1,
            pos.character + 1,
            self.severity,
            self.code,
            self.message
        )
    }

    /// Format the diagnostic in a simple format.
    ///
    /// Returns a string like: "error[TS2304]: Cannot find name 'foo'"
    pub fn format_simple(&self) -> String {
        format!("{}[TS{}]: {}", self.severity, self.code, self.message)
    }

    /// Convert to LSP Range (requires source file for position conversion).
    pub fn to_range(&self, source_file: &mut SourceFile) -> Range {
        source_file.span_to_range(self.span)
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_simple())
    }
}

// =============================================================================
// DiagnosticBag
// =============================================================================

/// A collection of diagnostics for a compilation phase.
///
/// DiagnosticBag provides a convenient interface for collecting diagnostics
/// during parsing, binding, or type checking. It tracks error counts and
/// provides filtering capabilities.
#[derive(Clone, Debug, Default)]
pub struct DiagnosticBag {
    /// The collected diagnostics
    diagnostics: Vec<Diagnostic>,
    /// The file name for diagnostics added without explicit file
    default_file: String,
    /// Error count
    error_count: usize,
    /// Warning count
    warning_count: usize,
}

impl DiagnosticBag {
    /// Create a new empty diagnostic bag.
    pub fn new() -> Self {
        DiagnosticBag {
            diagnostics: Vec::new(),
            default_file: String::new(),
            error_count: 0,
            warning_count: 0,
        }
    }

    /// Create a new diagnostic bag with a default file name.
    pub fn with_file(file_name: impl Into<String>) -> Self {
        DiagnosticBag {
            diagnostics: Vec::new(),
            default_file: file_name.into(),
            error_count: 0,
            warning_count: 0,
        }
    }

    /// Set the default file name.
    pub fn set_default_file(&mut self, file_name: impl Into<String>) {
        self.default_file = file_name.into();
    }

    /// Get the default file name.
    pub fn default_file(&self) -> &str {
        &self.default_file
    }

    /// Add a diagnostic.
    pub fn add(&mut self, diagnostic: Diagnostic) {
        match diagnostic.severity {
            DiagnosticSeverity::Error => self.error_count += 1,
            DiagnosticSeverity::Warning => self.warning_count += 1,
            _ => {}
        }
        self.diagnostics.push(diagnostic);
    }

    /// Add an error diagnostic.
    pub fn error(&mut self, span: Span, message: impl Into<String>, code: u32) {
        self.add(Diagnostic::error(&self.default_file, span, message, code));
    }

    /// Add an error diagnostic with explicit file.
    pub fn error_in(
        &mut self,
        file_name: impl Into<String>,
        span: Span,
        message: impl Into<String>,
        code: u32,
    ) {
        self.add(Diagnostic::error(file_name, span, message, code));
    }

    /// Add a warning diagnostic.
    pub fn warning(&mut self, span: Span, message: impl Into<String>, code: u32) {
        self.add(Diagnostic::warning(&self.default_file, span, message, code));
    }

    /// Add a warning diagnostic with explicit file.
    pub fn warning_in(
        &mut self,
        file_name: impl Into<String>,
        span: Span,
        message: impl Into<String>,
        code: u32,
    ) {
        self.add(Diagnostic::warning(file_name, span, message, code));
    }

    /// Add an info diagnostic.
    pub fn info(&mut self, span: Span, message: impl Into<String>, code: u32) {
        self.add(Diagnostic::info(&self.default_file, span, message, code));
    }

    /// Add a hint diagnostic.
    pub fn hint(&mut self, span: Span, message: impl Into<String>, code: u32) {
        self.add(Diagnostic::hint(&self.default_file, span, message, code));
    }

    /// Check if there are any diagnostics.
    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    /// Check if there are any warnings.
    pub fn has_warnings(&self) -> bool {
        self.warning_count > 0
    }

    /// Get the number of diagnostics.
    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// Check if the bag is empty.
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Get the error count.
    pub fn error_count(&self) -> usize {
        self.error_count
    }

    /// Get the warning count.
    pub fn warning_count(&self) -> usize {
        self.warning_count
    }

    /// Get all diagnostics as a slice.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Iterate over diagnostics.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter()
    }

    /// Get only errors.
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
    }

    /// Get only warnings.
    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Warning)
    }

    /// Filter diagnostics by file.
    pub fn for_file<'a>(&'a self, file_name: &'a str) -> impl Iterator<Item = &'a Diagnostic> {
        self.diagnostics
            .iter()
            .filter(move |d| d.file_name == file_name)
    }

    /// Filter diagnostics by code.
    pub fn by_code(&self, code: u32) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(move |d| d.code == code)
    }

    /// Sort diagnostics by file, then by position.
    pub fn sort(&mut self) {
        self.diagnostics
            .sort_by(|a, b| match a.file_name.cmp(&b.file_name) {
                std::cmp::Ordering::Equal => a.span.start.cmp(&b.span.start),
                other => other,
            });
    }

    /// Clear all diagnostics.
    pub fn clear(&mut self) {
        self.diagnostics.clear();
        self.error_count = 0;
        self.warning_count = 0;
    }

    /// Take all diagnostics, leaving the bag empty.
    pub fn take(&mut self) -> Vec<Diagnostic> {
        self.error_count = 0;
        self.warning_count = 0;
        std::mem::take(&mut self.diagnostics)
    }

    /// Merge another DiagnosticBag into this one.
    pub fn merge(&mut self, other: DiagnosticBag) {
        for diag in other.diagnostics {
            self.add(diag);
        }
    }

    /// Get error codes as a vector (for testing).
    pub fn error_codes(&self) -> Vec<u32> {
        self.errors().map(|d| d.code).collect()
    }

    /// Format all diagnostics for display.
    pub fn format_all(&self, source_file: &mut SourceFile) -> String {
        let mut result = String::new();
        for diag in &self.diagnostics {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&diag.format(source_file));
        }
        result
    }
}

impl IntoIterator for DiagnosticBag {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.into_iter()
    }
}

impl<'a> IntoIterator for &'a DiagnosticBag {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.iter()
    }
}

impl Extend<Diagnostic> for DiagnosticBag {
    fn extend<T: IntoIterator<Item = Diagnostic>>(&mut self, iter: T) {
        for diag in iter {
            self.add(diag);
        }
    }
}

// =============================================================================
// Diagnostic Formatting Utilities
// =============================================================================

/// Format a diagnostic message with placeholders.
///
/// Replaces {0}, {1}, etc. with the provided arguments.
///
/// # Example
/// ```ignore
/// let msg = format_message("Type '{0}' is not assignable to type '{1}'.", &["number", "string"]);
/// assert_eq!(msg, "Type 'number' is not assignable to type 'string'.");
/// ```
pub fn format_message(template: &str, args: &[&str]) -> String {
    let mut result = template.to_string();
    for (i, arg) in args.iter().enumerate() {
        result = result.replace(&format!("{{{}}}", i), arg);
    }
    result
}

/// Format a code snippet with a span underline.
///
/// Returns a string like:
/// ```text
/// const x = 1;
///       ^
/// ```
pub fn format_code_snippet(text: &str, span: Span, _context_lines: usize) -> String {
    let mut result = String::new();

    // Find line containing the span start
    let mut line_start = 0;
    for (i, ch) in text.char_indices() {
        if i >= span.start as usize {
            break;
        }
        if ch == '\n' {
            line_start = i + 1;
        }
    }

    // Find line end
    let line_end = text[line_start..]
        .find('\n')
        .map(|i| line_start + i)
        .unwrap_or(text.len());

    // Get the line text
    let line_text = &text[line_start..line_end];
    result.push_str(line_text);
    result.push('\n');

    // Create underline
    let col = span.start as usize - line_start;
    let underline_len = (span.len() as usize)
        .min(line_end - span.start as usize)
        .max(1);
    result.push_str(&" ".repeat(col));
    result.push_str(&"^".repeat(underline_len));

    result
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_severity() {
        assert_eq!(DiagnosticSeverity::Error.name(), "error");
        assert!(DiagnosticSeverity::Error.is_error());
        assert!(!DiagnosticSeverity::Warning.is_error());
        assert!(DiagnosticSeverity::Warning.is_warning());
    }

    #[test]
    fn test_diagnostic_creation() {
        let diag = Diagnostic::error("test.ts", Span::new(10, 20), "Test error", 2304);
        assert_eq!(diag.file_name, "test.ts");
        assert_eq!(diag.span, Span::new(10, 20));
        assert_eq!(diag.message, "Test error");
        assert_eq!(diag.code, 2304);
        assert!(diag.is_error());
    }

    #[test]
    fn test_diagnostic_with_related() {
        let diag =
            Diagnostic::error("test.ts", Span::new(10, 20), "Test error", 2304).with_related(
                DiagnosticRelatedInfo::new("other.ts", Span::new(5, 10), "See here"),
            );

        assert_eq!(diag.related.len(), 1);
        assert_eq!(diag.related[0].file_name, "other.ts");
    }

    #[test]
    fn test_diagnostic_format_simple() {
        let diag = Diagnostic::error("test.ts", Span::new(10, 20), "Cannot find name", 2304);
        assert_eq!(diag.format_simple(), "error[TS2304]: Cannot find name");
    }

    #[test]
    fn test_diagnostic_bag_basic() {
        let mut bag = DiagnosticBag::with_file("test.ts");
        assert!(bag.is_empty());
        assert!(!bag.has_errors());

        bag.error(Span::new(0, 5), "Error 1", 2304);
        bag.warning(Span::new(10, 15), "Warning 1", 6133);

        assert_eq!(bag.len(), 2);
        assert!(bag.has_errors());
        assert!(bag.has_warnings());
        assert_eq!(bag.error_count(), 1);
        assert_eq!(bag.warning_count(), 1);
    }

    #[test]
    fn test_diagnostic_bag_iteration() {
        let mut bag = DiagnosticBag::with_file("test.ts");
        bag.error(Span::new(0, 5), "Error 1", 2304);
        bag.error(Span::new(10, 15), "Error 2", 2322);
        bag.warning(Span::new(20, 25), "Warning 1", 6133);

        let errors: Vec<_> = bag.errors().collect();
        assert_eq!(errors.len(), 2);

        let warnings: Vec<_> = bag.warnings().collect();
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_diagnostic_bag_filter_by_code() {
        let mut bag = DiagnosticBag::with_file("test.ts");
        bag.error(Span::new(0, 5), "Error 1", 2304);
        bag.error(Span::new(10, 15), "Error 2", 2322);
        bag.error(Span::new(20, 25), "Error 3", 2304);

        let code_2304: Vec<_> = bag.by_code(2304).collect();
        assert_eq!(code_2304.len(), 2);
    }

    #[test]
    fn test_diagnostic_bag_merge() {
        let mut bag1 = DiagnosticBag::with_file("test.ts");
        bag1.error(Span::new(0, 5), "Error 1", 2304);

        let mut bag2 = DiagnosticBag::with_file("other.ts");
        bag2.error(Span::new(10, 15), "Error 2", 2322);

        bag1.merge(bag2);

        assert_eq!(bag1.len(), 2);
        assert_eq!(bag1.error_count(), 2);
    }

    #[test]
    fn test_diagnostic_bag_take() {
        let mut bag = DiagnosticBag::with_file("test.ts");
        bag.error(Span::new(0, 5), "Error 1", 2304);

        let diagnostics = bag.take();
        assert_eq!(diagnostics.len(), 1);
        assert!(bag.is_empty());
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn test_diagnostic_bag_sort() {
        let mut bag = DiagnosticBag::new();
        bag.error_in("b.ts", Span::new(10, 15), "B error", 2304);
        bag.error_in("a.ts", Span::new(5, 10), "A error 2", 2322);
        bag.error_in("a.ts", Span::new(0, 5), "A error 1", 2304);

        bag.sort();

        let diagnostics: Vec<_> = bag.iter().collect();
        assert_eq!(diagnostics[0].file_name, "a.ts");
        assert_eq!(diagnostics[0].span.start, 0);
        assert_eq!(diagnostics[1].file_name, "a.ts");
        assert_eq!(diagnostics[1].span.start, 5);
        assert_eq!(diagnostics[2].file_name, "b.ts");
    }

    #[test]
    fn test_format_message() {
        let msg = format_message(
            "Type '{0}' is not assignable to type '{1}'.",
            &["number", "string"],
        );
        assert_eq!(msg, "Type 'number' is not assignable to type 'string'.");
    }

    #[test]
    fn test_format_code_snippet() {
        let text = "const x = 1;";
        let span = Span::new(6, 7); // "x"
        let snippet = format_code_snippet(text, span, 0);
        assert!(snippet.contains("const x = 1;"));
        assert!(snippet.contains("^"));
    }

    #[test]
    fn test_diagnostic_format_with_source() {
        let mut source = SourceFile::new("test.ts", "const x = 1;");
        let diag = Diagnostic::error("test.ts", Span::new(6, 7), "Cannot find name 'x'", 2304);
        let formatted = diag.format(&mut source);

        assert!(formatted.contains("test.ts(1,7)"));
        assert!(formatted.contains("error"));
        assert!(formatted.contains("TS2304"));
    }

    #[test]
    fn test_error_codes() {
        let mut bag = DiagnosticBag::with_file("test.ts");
        bag.error(Span::new(0, 5), "Error 1", 2304);
        bag.error(Span::new(10, 15), "Error 2", 2322);
        bag.warning(Span::new(20, 25), "Warning 1", 6133);

        let codes = bag.error_codes();
        assert_eq!(codes, vec![2304, 2322]);
    }
}
