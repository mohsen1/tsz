//! Diagnostic types and message lookup for the type checker.
//!
//! Auto-generated message data lives in `data.rs`.
//! Run `node scripts/gen_diagnostics.mjs` to regenerate the data.
//!
//! When a locale is set via `--locale`, messages are translated using
//! TypeScript's official locale files.

use serde::Serialize;

// Auto-generated diagnostic messages, diagnostic_messages, and diagnostic_codes
mod data;
pub use data::{DIAGNOSTIC_MESSAGES, diagnostic_codes, diagnostic_messages};

// =============================================================================
// Diagnostic Types
// =============================================================================

/// Diagnostic category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum DiagnosticCategory {
    Warning = 0,
    Error = 1,
    Suggestion = 2,
    Message = 3,
}

/// Related information for a diagnostic (e.g., "see also" locations).
#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticRelatedInformation {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub category: DiagnosticCategory,
    pub code: u32,
}

/// A type-checking diagnostic message with optional related information.
#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub category: DiagnosticCategory,
    pub code: u32,
    /// Related information spans (e.g., where a type was declared)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related_information: Vec<DiagnosticRelatedInformation>,
}

impl Diagnostic {
    /// Create a new error diagnostic.
    #[must_use]
    pub const fn error(file: String, start: u32, length: u32, message: String, code: u32) -> Self {
        Self {
            file,
            start,
            length,
            message_text: message,
            category: DiagnosticCategory::Error,
            code,
            related_information: Vec::new(),
        }
    }

    /// Add related information to this diagnostic.
    #[must_use]
    pub fn with_related(mut self, file: String, start: u32, length: u32, message: String) -> Self {
        self.related_information.push(DiagnosticRelatedInformation {
            file,
            start,
            length,
            message_text: message,
            category: DiagnosticCategory::Message,
            code: 0,
        });
        self
    }
}

/// Format a diagnostic message by replacing {0}, {1}, etc. with arguments.
#[must_use]
pub fn format_message(template: &str, args: &[&str]) -> String {
    let mut result = template.to_string();
    for (i, arg) in args.iter().enumerate() {
        result = result.replace(&format!("{{{i}}}"), arg);
    }
    result
}

/// A diagnostic message definition with code, category, and message template.
#[derive(Clone, Copy, Debug)]
pub struct DiagnosticMessage {
    pub code: u32,
    pub category: DiagnosticCategory,
    pub message: &'static str,
}

/// Look up a diagnostic message definition by code.
///
/// Returns the `DiagnosticMessage` with template string containing `{0}`, `{1}`, etc. placeholders.
/// Use `format_message()` to fill in the placeholders.
#[must_use]
pub fn get_diagnostic_message(code: u32) -> Option<&'static DiagnosticMessage> {
    DIAGNOSTIC_MESSAGES.iter().find(|m| m.code == code)
}

/// Get the message template for a diagnostic code.
///
/// Returns the template string with `{0}`, `{1}`, etc. placeholders.
/// Use `format_message()` to fill in the placeholders.
#[must_use]
pub fn get_message_template(code: u32) -> Option<&'static str> {
    get_diagnostic_message(code).map(|m| m.message)
}

/// Get the category for a diagnostic code.
#[must_use]
pub fn get_diagnostic_category(code: u32) -> Option<DiagnosticCategory> {
    get_diagnostic_message(code).map(|m| m.category)
}
