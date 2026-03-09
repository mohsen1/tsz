pub mod data;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticCategory {
    Warning,
    Error,
    Suggestion,
    Message,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticMessage {
    pub code: u32,
    pub category: DiagnosticCategory,
    pub message: &'static str,
}

pub mod diagnostic_messages {
    pub use super::data::diagnostic_messages::*;
}

pub mod diagnostic_codes {
    pub use super::data::diagnostic_codes::*;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRelatedInformation {
    pub category: DiagnosticCategory,
    pub code: u32,
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub category: DiagnosticCategory,
    pub code: u32,
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub related_information: Vec<DiagnosticRelatedInformation>,
}

impl Diagnostic {
    pub fn error(
        file: impl Into<String>,
        start: u32,
        length: u32,
        message: impl Into<String>,
        code: u32,
    ) -> Self {
        Self {
            category: DiagnosticCategory::Error,
            message_text: message.into(),
            code,
            file: file.into(),
            start,
            length,
            related_information: Vec::new(),
        }
    }

    /// Create a diagnostic by looking up the message template and category from
    /// the diagnostic code. The template's `{0}`, `{1}`, ... placeholders are
    /// replaced with the provided `args`.
    ///
    /// Panics (debug) if the code is not found in the generated diagnostic table.
    pub fn from_code(
        code: u32,
        file: impl Into<String>,
        start: u32,
        length: u32,
        args: &[&str],
    ) -> Self {
        let info = lookup_diagnostic(code).unwrap_or(DiagnosticMessage {
            code,
            category: DiagnosticCategory::Error,
            message: "Unknown diagnostic",
        });
        debug_assert!(
            lookup_diagnostic(code).is_some(),
            "diagnostic code {code} not found in generated table"
        );
        Self {
            category: info.category,
            code,
            file: file.into(),
            start,
            length,
            message_text: format_message(info.message, args),
            related_information: Vec::new(),
        }
    }

    pub fn with_related(
        mut self,
        file: impl Into<String>,
        start: u32,
        length: u32,
        message: impl Into<String>,
    ) -> Self {
        self.related_information.push(DiagnosticRelatedInformation {
            category: DiagnosticCategory::Message,
            code: 0,
            file: file.into(),
            start,
            length,
            message_text: message.into(),
        });
        self
    }
}

/// Look up a `DiagnosticMessage` (code + category + template) by numeric code.
/// Uses binary search over the sorted generated table — O(log n).
pub fn lookup_diagnostic(code: u32) -> Option<DiagnosticMessage> {
    use self::data::DIAGNOSTIC_MESSAGES;
    DIAGNOSTIC_MESSAGES
        .binary_search_by_key(&code, |m| m.code)
        .ok()
        .map(|idx| DIAGNOSTIC_MESSAGES[idx])
}

pub fn get_message_template(code: u32) -> Option<&'static str> {
    lookup_diagnostic(code).map(|m| m.message)
}

pub fn format_message(message: &str, args: &[&str]) -> String {
    let mut result = message.to_string();
    for (i, arg) in args.iter().enumerate() {
        result = result.replace(&format!("{{{i}}}"), arg);
    }
    result
}
