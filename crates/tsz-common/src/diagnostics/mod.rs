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

pub fn get_message_template(code: u32) -> Option<&'static str> {
    use self::data::DIAGNOSTIC_MESSAGES;
    DIAGNOSTIC_MESSAGES
        .iter()
        .find(|m| m.code == code)
        .map(|m| m.message)
}

pub fn format_message(message: &str, args: &[&str]) -> String {
    let mut result = message.to_string();
    for (i, arg) in args.iter().enumerate() {
        result = result.replace(&format!("{{{i}}}"), arg);
    }
    result
}
