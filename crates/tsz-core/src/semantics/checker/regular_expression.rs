use crate::bind::Meaning;
use crate::program::SemanticCompletion;
use crate::semantics::types::TypeId;
use crate::source::{FileId, Span};
use crate::syntax::RegularExpressionLiteral;

use super::Checker;

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
            let known = match flag {
                'g' => Some(0),
                'i' => Some(1),
                'm' => Some(2),
                'u' => Some(3),
                'y' => Some(4),
                _ => None,
            };
            let span = relative_span(literal.flags_span, offset, flag.len_utf8());
            let Some(index) = known else {
                self.push_diagnostic(
                    file,
                    span,
                    "Unknown regular expression flag.".to_string(),
                    1499,
                );
                continue;
            };
            if seen[index] {
                self.push_diagnostic(
                    file,
                    span,
                    "Duplicate regular expression flag.".to_string(),
                    1500,
                );
            } else {
                seen[index] = true;
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
                self.push_diagnostic(
                    file,
                    relative_span(literal.pattern_span, close, 1),
                    "Hexadecimal digit expected.".to_string(),
                    1125,
                );
                offset = close + 1;
                continue;
            }

            if let Some(invalid) = digits.iter().position(|byte| !byte.is_ascii_hexdigit()) {
                self.push_diagnostic(
                    file,
                    relative_span(literal.pattern_span, digits_start + invalid, 1),
                    "Hexadecimal digit expected.".to_string(),
                    1125,
                );
                self.push_diagnostic(
                    file,
                    relative_span(literal.pattern_span, close, 1),
                    "Unexpected '}'. Did you mean to escape it with backslash?".to_string(),
                    1508,
                );
                offset = close + 1;
                continue;
            }

            let value = digits.iter().fold(0_u64, |value, byte| {
                value
                    .saturating_mul(16)
                    .saturating_add(u64::from(hex_value(*byte)))
            });
            if value > 0x10_ffff {
                self.push_diagnostic(
                    file,
                    relative_span(literal.pattern_span, digits_start, digits.len()),
                    "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive."
                        .to_string(),
                    1198,
                );
            }
            offset = close + 1;
        }
        true
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

const fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}
