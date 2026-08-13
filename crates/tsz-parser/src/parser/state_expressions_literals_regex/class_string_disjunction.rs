//! `v`-mode `\q{...}` `ClassStringDisjunction` scanning and interior
//! validation, extracted from `state_expressions_literals_regex.rs`.
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

/// Scan a `\q{...}` body starting just after the opening `{`, advancing
/// `pos` to the terminating `}` (or the body end) and validating each
/// interior `ClassSetCharacter` along the way, returning whether the
/// `ClassStringDisjunction` may contain a string — true as soon as any
/// `|`-separated alternative is not exactly one code point. A `\` escapes
/// the next character (so `\}` does not close the disjunction) and `\u{H+}`
/// spans its own braces; both contribute one code point, so `\q{a}` and
/// `\q{\u{1F600}}` are single characters and must not be judged by their
/// source bytes or truncated at a brace that belongs to an escape.
pub(super) fn scan_class_string_disjunction_body(
    parser: &mut ParserState,
    emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
    body: &[u8],
    pos: &mut usize,
) -> bool {
    let mut code_points = 0usize;
    let mut may_contain_strings = false;

    while *pos < body.len() && body[*pos] != b'}' {
        let ch = body[*pos];
        match ch {
            b'|' => {
                may_contain_strings |= code_points != 1;
                code_points = 0;
                *pos += 1;
            }
            b'\\' => {
                code_points += 1;
                *pos += 1;
                if body.get(*pos) == Some(&b'u') && body.get(*pos + 1) == Some(&b'{') {
                    *pos += 2;
                    while *pos < body.len() && body[*pos] != b'}' {
                        *pos += 1;
                    }
                }
                *pos += 1;
            }
            _ if is_class_string_reserved_double_punctuator_char(ch)
                && body.get(*pos + 1) == Some(&ch) =>
            {
                emit(
                    parser,
                    *pos,
                    2,
                    diagnostic_messages::A_CHARACTER_CLASS_MUST_NOT_CONTAIN_A_RESERVED_DOUBLE_PUNCTUATOR_DID_YOU_MEAN_TO,
                    diagnostic_codes::A_CHARACTER_CLASS_MUST_NOT_CONTAIN_A_RESERVED_DOUBLE_PUNCTUATOR_DID_YOU_MEAN_TO,
                );
                code_points += 1;
                *pos += 2;
            }
            _ if is_class_string_reserved_syntax_character(ch) => {
                let ch_str = (ch as char).to_string();
                let message = format_message(
                    diagnostic_messages::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                    &[ch_str.as_str()],
                );
                emit(
                    parser,
                    *pos,
                    1,
                    &message,
                    diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                );
                code_points += 1;
                *pos += 1;
            }
            _ => {
                let advance =
                    next_utf8_char(body, body.len(), *pos).map_or(1, |(_ch, char_len)| char_len);
                code_points += 1;
                *pos += advance;
            }
        }
    }

    may_contain_strings || code_points != 1
}
