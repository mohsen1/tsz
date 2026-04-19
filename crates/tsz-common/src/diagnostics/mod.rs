pub mod data;

/// True when `code` is in the TypeScript parser/grammar diagnostic range
/// (1000–1999) — syntactic errors and grammar rule violations.
pub fn is_parser_grammar_diagnostic(code: u32) -> bool {
    (1000..2000).contains(&code)
}

/// True when `code` is in the TypeScript JavaScript grammar diagnostic range
/// (8000–8999) — JS-specific parser errors emitted for `.js`/`.jsx` sources.
pub fn is_js_grammar_diagnostic(code: u32) -> bool {
    (8000..9000).contains(&code)
}

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
    fn normalize_template_placeholder_spacing(arg: &str) -> String {
        if !arg.contains("${") {
            return arg.to_string();
        }

        let chars: Vec<char> = arg.chars().collect();
        let mut out = String::with_capacity(arg.len());
        let mut i = 0usize;

        while i < chars.len() {
            if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
                out.push('$');
                out.push('{');
                i += 2;

                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }

                let mut depth = 1usize;
                let mut inner = String::new();
                while i < chars.len() {
                    let ch = chars[i];
                    i += 1;
                    if ch == '{' {
                        depth += 1;
                        inner.push(ch);
                        continue;
                    }
                    if ch == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        inner.push(ch);
                        continue;
                    }
                    inner.push(ch);
                }

                out.push_str(inner.trim_end());
                out.push('}');
                continue;
            }

            out.push(chars[i]);
            i += 1;
        }

        out
    }

    let mut result = message.to_string();
    for (i, arg) in args.iter().enumerate() {
        let normalized = normalize_template_placeholder_spacing(arg);
        result = result.replace(&format!("{{{i}}}"), &normalized);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_diagnostic_finds_known_code_and_rejects_unknown_code() {
        let known = data::DIAGNOSTIC_MESSAGES
            .first()
            .copied()
            .expect("generated diagnostic table should not be empty");

        let lookup = lookup_diagnostic(known.code).expect("known code should resolve");
        assert_eq!(lookup, known);
        assert!(lookup_diagnostic(u32::MAX).is_none());
    }

    #[test]
    fn get_message_template_matches_lookup_and_returns_none_for_unknown_code() {
        let known = data::DIAGNOSTIC_MESSAGES
            .first()
            .copied()
            .expect("generated diagnostic table should not be empty");

        assert_eq!(get_message_template(known.code), Some(known.message));
        assert_eq!(get_message_template(u32::MAX), None);
    }

    #[test]
    fn format_message_replaces_placeholders_and_leaves_missing_ones_intact() {
        let formatted = format_message("{0} + {1} + {0} + {2}", &["a", "b"]);
        assert_eq!(formatted, "a + b + a + {2}");
    }

    #[test]
    fn diagnostic_from_code_uses_table_entry_for_known_code() {
        let known = data::DIAGNOSTIC_MESSAGES
            .first()
            .copied()
            .expect("generated diagnostic table should not be empty");
        let args = ["left", "right", "extra"];
        let expected_message = format_message(known.message, &args);

        let diagnostic = Diagnostic::from_code(known.code, "test.ts", 4, 8, &args);

        assert_eq!(diagnostic.category, known.category);
        assert_eq!(diagnostic.code, known.code);
        assert_eq!(diagnostic.file, "test.ts");
        assert_eq!(diagnostic.start, 4);
        assert_eq!(diagnostic.length, 8);
        assert_eq!(diagnostic.message_text, expected_message);
        assert!(diagnostic.related_information.is_empty());
    }

    #[test]
    fn diagnostic_from_code_uses_unknown_fallback_for_missing_code() {
        let result = std::panic::catch_unwind(|| {
            Diagnostic::from_code(u32::MAX, "missing.ts", 1, 2, &["ignored"])
        });

        if cfg!(debug_assertions) {
            assert!(
                result.is_err(),
                "debug builds should trip the diagnostic lookup assertion"
            );
        } else {
            let diagnostic = result.expect("release builds should return the fallback diagnostic");
            assert_eq!(diagnostic.category, DiagnosticCategory::Error);
            assert_eq!(diagnostic.code, u32::MAX);
            assert_eq!(diagnostic.file, "missing.ts");
            assert_eq!(diagnostic.start, 1);
            assert_eq!(diagnostic.length, 2);
            assert_eq!(diagnostic.message_text, "Unknown diagnostic");
            assert!(diagnostic.related_information.is_empty());
        }
    }

    #[test]
    fn diagnostic_with_related_appends_message_information() {
        let diagnostic = Diagnostic::error("file.ts", 10, 3, "message", 1234)
            .with_related("other.ts", 20, 5, "see also");

        assert_eq!(diagnostic.related_information.len(), 1);
        let related = &diagnostic.related_information[0];
        assert_eq!(related.category, DiagnosticCategory::Message);
        assert_eq!(related.code, 0);
        assert_eq!(related.file, "other.ts");
        assert_eq!(related.start, 20);
        assert_eq!(related.length, 5);
        assert_eq!(related.message_text, "see also");
    }
}
