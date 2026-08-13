//! `v`-mode `\q{...}` `ClassStringDisjunction` interior validation, extracted
//! from `state_expressions_literals_regex.rs`.
//!
//! Pure file-organization move; no logic changes beyond what landed with the
//! extraction (see the call site in `scan_character_class_escape`'s `b'q'`
//! arm). Keeps the parent under the parser LOC ceiling.
use crate::parser::regex_modifier_groups::next_utf8_char;
use crate::parser::state::ParserState;
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

/// A `\q{...}` alternative has no range or class-set-operator grammar, so —
/// unlike the enclosing class body, where `&&`/`--` are the defined
/// intersection/subtraction operators — doubling either of those two here is
/// just a reserved punctuator like any other, and reports TS1522. Otherwise
/// identical to the enclosing class body's reserved set (empirically derived
/// against `typescript@7.0.2`; see
/// `regex_class_set_reserved_double_punctuator_tests.rs`).
const fn is_class_string_reserved_double_punctuator_char(ch: u8) -> bool {
    matches!(
        ch,
        b'!' | b'#'
            | b'%'
            | b'&'
            | b'*'
            | b'+'
            | b','
            | b'.'
            | b':'
            | b';'
            | b'<'
            | b'='
            | b'>'
            | b'?'
            | b'@'
            | b'`'
            | b'~'
    )
}

/// A `ClassSetSyntaxCharacter` other than `\` (escape) and `|` (the
/// alternative separator, legal here). Unlike the enclosing class body,
/// `\q{...}` has no range grammar, so `-` is reserved in every position
/// rather than only as a fresh atom.
const fn is_class_string_reserved_syntax_character(ch: u8) -> bool {
    matches!(ch, b'(' | b')' | b'{' | b'}' | b'/' | b'[' | b']' | b'-')
}

/// Scans a `\q{...}` body — the raw bytes between the braces — validating
/// each interior `ClassSetCharacter` and returning whether the disjunction
/// denotes a set containing anything other than single characters. Each
/// `|`-separated alternative is a `ClassString`, and the disjunction may
/// contain strings as soon as one alternative is not exactly one code point.
/// An escape sequence contributes exactly one code point, so `\q{a}` is a
/// single character and must not be judged by its six source bytes.
pub(super) fn scan_class_string_disjunction_body(
    parser: &mut ParserState,
    emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
    alternatives: &[u8],
    alternatives_start: usize,
) -> bool {
    let mut code_points = 0usize;
    let mut pos = 0usize;
    // Every character still needs validating even once the
    // may-contain-strings answer is already known, so this accumulates
    // rather than returning early on the first multi-code-point
    // alternative.
    let mut may_contain_strings = false;

    while pos < alternatives.len() {
        let ch = alternatives[pos];
        match ch {
            b'|' => {
                may_contain_strings |= code_points != 1;
                code_points = 0;
                pos += 1;
            }
            b'\\' => {
                code_points += 1;
                pos += 1;
                // `\u{H+}` spans to its closing brace; every other escape is
                // sized by the walker that follows, and consuming the
                // single byte after the backslash is enough to keep `|` and
                // code-point counting aligned for all of them.
                if alternatives.get(pos) == Some(&b'u') && alternatives.get(pos + 1) == Some(&b'{')
                {
                    pos += 2;
                    while pos < alternatives.len() && alternatives[pos] != b'}' {
                        pos += 1;
                    }
                }
                pos += 1;
            }
            _ if is_class_string_reserved_double_punctuator_char(ch)
                && alternatives.get(pos + 1) == Some(&ch) =>
            {
                emit(
                    parser,
                    alternatives_start + pos,
                    2,
                    diagnostic_messages::A_CHARACTER_CLASS_MUST_NOT_CONTAIN_A_RESERVED_DOUBLE_PUNCTUATOR_DID_YOU_MEAN_TO,
                    diagnostic_codes::A_CHARACTER_CLASS_MUST_NOT_CONTAIN_A_RESERVED_DOUBLE_PUNCTUATOR_DID_YOU_MEAN_TO,
                );
                code_points += 1;
                pos += 2;
            }
            _ if is_class_string_reserved_syntax_character(ch) => {
                let ch_str = (ch as char).to_string();
                let message = format_message(
                    diagnostic_messages::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                    &[ch_str.as_str()],
                );
                emit(
                    parser,
                    alternatives_start + pos,
                    1,
                    &message,
                    diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                );
                code_points += 1;
                pos += 1;
            }
            _ => {
                let advance = next_utf8_char(alternatives, alternatives.len(), pos)
                    .map_or(1, |(_ch, char_len)| char_len);
                code_points += 1;
                pos += advance;
            }
        }
    }

    may_contain_strings || code_points != 1
}
