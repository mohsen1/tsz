use crate::bind::Meaning;
use crate::program::SemanticCompletion;
use crate::semantics::types::TypeId;
use crate::source::{FileId, Span};
use crate::syntax::RegularExpressionLiteral;

use super::Checker;

macro_rules! diag {
    ($checker:expr, $file:expr, $span:expr, $code:expr) => {
        $checker.push_diagnostic($file, $span, regex_message($code).into(), $code)
    };
}

impl Checker<'_> {
    /// TypeScript 7 validates a regular-expression pattern only on the checked
    /// path. The scanner owns token extent and unterminated recovery; this
    /// query owns the bounded flag/Unicode grammar and the resulting global
    /// `RegExp` identity.
    pub(super) fn infer_regular_expression(
        &mut self,
        file: FileId,
        literal: &RegularExpressionLiteral,
    ) -> TypeId {
        if literal.validation_supported() {
            if literal.terminated {
                self.validate_regular_expression(file, literal);
            }
        } else {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
        }

        let Some(declaration) = self
            .program
            .standard_library
            .resolve("RegExp", Meaning::Type)
        else {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
            return self.store.builtins.error;
        };

        // Script declarations merge with the ambient RegExp interface. Until
        // declaration merging is represented, retain the ambient identity but
        // never claim the merged semantic result as complete.
        if self.program.global_types.contains_key("RegExp") {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
        }
        self.store.symbolic_reference(declaration, Vec::new())
    }

    /// Widening consumes literal freshness. A program-owned reference to the
    /// ambient `RegExp` declaration is already nonliteral and must remain
    /// symbolic until a member/relation query genuinely demands its shape.
    pub(super) fn is_symbolic_regular_expression_type(&self, ty: TypeId) -> bool {
        let Some(declaration) = self
            .program
            .standard_library
            .resolve("RegExp", Meaning::Type)
        else {
            return false;
        };
        self.store.is_unapplied_symbolic_reference(ty, declaration)
    }

    fn validate_regular_expression(&mut self, file: FileId, literal: &RegularExpressionLiteral) {
        self.validate_regular_expression_flags(file, literal);
        if literal.flags.contains('u') && !self.validate_extended_unicode_escapes(file, literal) {
            self.observe_file_completion(file, SemanticCompletion::Deferred);
        }
    }

    fn validate_regular_expression_flags(
        &mut self,
        file: FileId,
        literal: &RegularExpressionLiteral,
    ) {
        let mut seen = [false; 5];
        for (offset, flag) in literal.flags.char_indices() {
            let span = relative_span(literal.flags_span, offset, flag.len_utf8());
            let Some(index) = "gimuy".find(flag) else {
                diag!(self, file, span, 1499);
                continue;
            };
            if std::mem::replace(&mut seen[index], true) {
                diag!(self, file, span, 1500);
            }
        }
    }

    /// Returns false only when an unfinished escape escaped the deliberately
    /// bounded grammar. Closed malformed escapes produce the pinned TS7 facts.
    fn validate_extended_unicode_escapes(
        &mut self,
        file: FileId,
        literal: &RegularExpressionLiteral,
    ) -> bool {
        let bytes = literal.pattern.as_bytes();
        let mut offset = 0;
        while offset < bytes.len() {
            if bytes[offset] != b'\\' {
                offset += 1;
                continue;
            }
            if bytes.get(offset + 1) == Some(&b'\\') {
                offset += 2;
                continue;
            }
            if bytes.get(offset + 1..offset + 3) != Some(b"u{") {
                offset += 2;
                continue;
            }

            let digits_start = offset + 3;
            let Some(relative_close) = bytes[digits_start..].iter().position(|byte| *byte == b'}')
            else {
                return false;
            };
            let close = digits_start + relative_close;
            let digits = &bytes[digits_start..close];
            if digits.is_empty() {
                let span = relative_span(literal.pattern_span, close, 1);
                diag!(self, file, span, 1125);
                offset = close + 1;
                continue;
            }

            if let Some(invalid) = digits.iter().position(|byte| !byte.is_ascii_hexdigit()) {
                let invalid_span = relative_span(literal.pattern_span, digits_start + invalid, 1);
                diag!(self, file, invalid_span, 1125);
                let close_span = relative_span(literal.pattern_span, close, 1);
                diag!(self, file, close_span, 1508);
                offset = close + 1;
                continue;
            }

            let value = digits.iter().fold(0_u64, |value, byte| {
                value.saturating_mul(16).saturating_add(u64::from(
                    char::from(*byte)
                        .to_digit(16)
                        .expect("validated hexadecimal digit"),
                ))
            });
            if value > 0x10_ffff {
                let span = relative_span(literal.pattern_span, digits_start, digits.len());
                diag!(self, file, span, 1198);
            }
            offset = close + 1;
        }
        true
    }
}

const fn regex_message(code: u32) -> &'static str {
    match code {
        1125 => "Hexadecimal digit expected.",
        1198 => "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive.",
        1499 => "Unknown regular expression flag.",
        1500 => "Duplicate regular expression flag.",
        1508 => "Unexpected '}'. Did you mean to escape it with backslash?",
        _ => unreachable!(),
    }
}

const fn relative_span(base: Span, start: usize, length: usize) -> Span {
    let start = base.start.saturating_add(start as u32);
    Span {
        file: base.file,
        start,
        end: start.saturating_add(length as u32),
    }
}
