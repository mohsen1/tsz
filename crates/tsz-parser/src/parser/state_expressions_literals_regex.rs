//! Regex literal parsing extracted from `state_expressions_literals.rs`.
//!
//! Pure file-organization move; no logic changes. Keeps `state_expressions_literals.rs`
//! under the parser LOC ceiling.

use super::state::ParserState;
use crate::parser::{NodeIndex, node::LiteralData};
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::TokenFlags;

/// Map a UTF-8 `start` byte offset and a (possibly surrogate-pair) `char`
/// into the UTF-16 code-unit offsets used by regex range-order analysis.
///
/// Pathological inputs whose absolute offset does not fit in `u32` would
/// otherwise panic on the inner `u32::try_from`. We drop unrepresentable
/// offsets rather than panic — range-order analysis tolerates a shorter
/// offset vector and simply skips the affected atoms. See issue #4787.
fn split_non_unicode_atom_offsets(start: usize, ch: char) -> Vec<u32> {
    let utf16_len = ch.len_utf16();
    let utf8_len = ch.len_utf8();
    ch.encode_utf16(&mut [0; 2])
        .iter()
        .enumerate()
        .filter_map(|(i, _)| u32::try_from(start + (i * utf8_len) / utf16_len).ok())
        .collect()
}

impl ParserState {
    fn regex_literal_follows_invalid_shebang(&self, start_pos: u32) -> bool {
        let source = self.scanner.source_text().as_bytes();
        let start = start_pos as usize;
        start >= 2
            && source.get(start - 2) == Some(&b'#')
            && source.get(start - 1) == Some(&b'!')
            && start != 2
    }

    /// Parse regex literal: /pattern/flags
    pub(crate) fn parse_regex_literal(&mut self) -> NodeIndex {
        fn regex_body_end(raw_text: &str) -> Option<usize> {
            let bytes = raw_text.as_bytes();
            if bytes.first().copied() != Some(b'/') {
                return None;
            }

            let mut i = 1usize;
            let mut escaped = false;
            let mut in_character_class = false;
            while i < bytes.len() {
                let ch = bytes[i];
                if escaped {
                    escaped = false;
                    i += 1;
                    continue;
                }
                match ch {
                    b'\\' => {
                        escaped = true;
                        i += 1;
                    }
                    b'[' => {
                        in_character_class = true;
                        i += 1;
                    }
                    b']' => {
                        in_character_class = false;
                        i += 1;
                    }
                    b'/' if !in_character_class => return Some(i),
                    _ => i += 1,
                }
            }
            None
        }

        fn decode_surrogate_pair(high: u32, low: u32) -> Option<u32> {
            if !(0xD800..=0xDBFF).contains(&high) || !(0xDC00..=0xDFFF).contains(&low) {
                return None;
            }
            Some(0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00))
        }

        fn parse_hex_u32(raw_text: &str, start: usize, len: usize) -> Option<u32> {
            raw_text
                .get(start..start + len)
                .and_then(|slice| u32::from_str_radix(slice, 16).ok())
        }

        fn regex_range_order_errors(raw_text: &str, body_end: usize) -> Vec<(u32, u32)> {
            #[derive(Clone, Copy)]
            enum ClassToken {
                Atom { value: u32, start: u32 },
                OpaqueAtom,
                Hyphen,
            }

            type ClassAtomParse = (Vec<(u32, u32)>, usize);

            fn parse_class_atom(
                raw_text: &str,
                start: usize,
                class_end: usize,
                unicode_mode: bool,
            ) -> Option<ClassAtomParse> {
                let rest = raw_text.get(start..class_end)?;
                let mut chars = rest.chars();
                let ch = chars.next()?;
                if ch == '\\' {
                    let next_start = start + ch.len_utf8();
                    let next = raw_text.get(next_start..class_end)?.chars().next()?;
                    if next == 'u' {
                        let brace_start = next_start + next.len_utf8();
                        if raw_text.as_bytes().get(brace_start).copied() == Some(b'{') {
                            let hex_start = brace_start + 1;
                            let mut hex_end = hex_start;
                            while hex_end < class_end
                                && raw_text.as_bytes().get(hex_end).copied() != Some(b'}')
                            {
                                hex_end += 1;
                            }
                            if hex_end < class_end
                                && let Some(value) =
                                    parse_hex_u32(raw_text, hex_start, hex_end - hex_start)
                            {
                                return Some((
                                    vec![(value, u32::try_from(start).ok()?)],
                                    hex_end + 1,
                                ));
                            }
                        } else if let Some(value) = parse_hex_u32(raw_text, brace_start, 4) {
                            let next_index = brace_start + 4;
                            if unicode_mode
                                && let Some(after_first) = raw_text.get(next_index..class_end)
                                && after_first.starts_with("\\u")
                                && let Some(low) = parse_hex_u32(raw_text, next_index + 2, 4)
                                && let Some(code_point) = decode_surrogate_pair(value, low)
                            {
                                return Some((
                                    vec![(code_point, u32::try_from(start).ok()?)],
                                    next_index + 6,
                                ));
                            }
                            return Some((vec![(value, u32::try_from(start).ok()?)], next_index));
                        }
                    } else if next == 'x' {
                        let hex_start = next_start + next.len_utf8();
                        if let Some(value) = parse_hex_u32(raw_text, hex_start, 2) {
                            return Some((
                                vec![(value, u32::try_from(start).ok()?)],
                                hex_start + 2,
                            ));
                        }
                    }

                    let escaped_start = next_start;
                    let escaped = raw_text.get(escaped_start..class_end)?.chars().next()?;
                    if matches!(escaped, 'd' | 'D' | 's' | 'S' | 'w' | 'W' | 'p' | 'P') {
                        return Some((Vec::new(), escaped_start + escaped.len_utf8()));
                    }
                    if unicode_mode {
                        Some((
                            vec![(escaped as u32, u32::try_from(start).ok()?)],
                            escaped_start + escaped.len_utf8(),
                        ))
                    } else {
                        Some((
                            escaped
                                .encode_utf16(&mut [0; 2])
                                .iter()
                                .zip(split_non_unicode_atom_offsets(start, escaped))
                                .map(|(u, offset)| (*u as u32, offset))
                                .collect(),
                            escaped_start + escaped.len_utf8(),
                        ))
                    }
                } else if unicode_mode {
                    Some((
                        vec![(ch as u32, u32::try_from(start).ok()?)],
                        start + ch.len_utf8(),
                    ))
                } else {
                    Some((
                        ch.encode_utf16(&mut [0; 2])
                            .iter()
                            .zip(split_non_unicode_atom_offsets(start, ch))
                            .map(|(u, offset)| (*u as u32, offset))
                            .collect(),
                        start + ch.len_utf8(),
                    ))
                }
            }

            let flags = &raw_text[body_end + 1..];
            let unicode_mode = flags.contains('u') || flags.contains('v');
            let bytes = raw_text.as_bytes();
            let mut errors = Vec::new();
            let mut i = 1usize;

            while i < body_end {
                match bytes[i] {
                    b'\\' => {
                        i += 1;
                        if i < body_end {
                            i += 1;
                        }
                    }
                    b'[' => {
                        i += 1;
                        let mut tokens = Vec::new();
                        while i < body_end {
                            if bytes[i] == b']' {
                                i += 1;
                                break;
                            }
                            if bytes[i] == b'-' {
                                tokens.push(ClassToken::Hyphen);
                                i += 1;
                                continue;
                            }
                            let Some((atoms, next_i)) =
                                parse_class_atom(raw_text, i, body_end, unicode_mode)
                            else {
                                break;
                            };
                            if atoms.is_empty() {
                                tokens.push(ClassToken::OpaqueAtom);
                            } else {
                                tokens.extend(
                                    atoms
                                        .into_iter()
                                        .map(|(value, start)| ClassToken::Atom { value, start }),
                                );
                            }
                            i = next_i;
                        }

                        let mut token_index = 0usize;
                        while token_index + 2 < tokens.len() {
                            match &tokens[token_index..token_index + 3] {
                                [
                                    ClassToken::Atom { value: left, start },
                                    ClassToken::Hyphen,
                                    ClassToken::Atom { value: right, .. },
                                ] => {
                                    if left > right {
                                        errors.push((*start, 1));
                                    }
                                    token_index += 3;
                                }
                                _ => token_index += 1,
                            }
                        }
                    }
                    _ => {
                        if let Some(ch) = raw_text.get(i..body_end).and_then(|s| s.chars().next()) {
                            i += ch.len_utf8();
                        } else {
                            break;
                        }
                    }
                }
            }

            errors
        }

        fn validate_regex_literal_body(
            parser: &mut ParserState,
            raw_text: &str,
            start_pos: u32,
            body_end: usize,
        ) {
            if body_end <= 1 {
                return;
            }

            let bytes = raw_text.as_bytes();
            let flags = &raw_text[body_end + 1..];
            let unicode_sets_mode = flags.contains('v');
            let any_unicode_mode = flags.contains('u') || unicode_sets_mode;
            let strict_mode = any_unicode_mode;

            #[derive(Clone, Copy)]
            enum ClassAtomKind {
                Character { value: u32, utf16_len: usize },
                Class,
                Unknown,
            }

            let emit =
                |parser: &mut ParserState, pos: usize, len: u32, message: &str, code: u32| {
                    parser.parse_error_at(start_pos + pos as u32, len, message, code);
                };

            struct RegexScanContext<'a, F>
            where
                F: Fn(&mut ParserState, usize, u32, &str, u32),
            {
                emit: &'a F,
                body: &'a [u8],
                body_end: usize,
                strict_mode: bool,
                unicode_sets_mode: bool,
                start_pos: u32,
            }

            fn scan_digits(body: &[u8], end: usize, pos: &mut usize) -> usize {
                let start = *pos;
                while *pos < end && body[*pos].is_ascii_digit() {
                    *pos += 1;
                }
                *pos - start
            }

            fn next_utf8_char(bytes: &[u8], end: usize, pos: usize) -> Option<(char, usize)> {
                std::str::from_utf8(&bytes[pos..end])
                    .ok()
                    .and_then(|slice| slice.chars().next())
                    .map(|ch| (ch, ch.len_utf8()))
            }

            const fn is_word_char(ch: u8) -> bool {
                ch == b'_' || ch.is_ascii_alphanumeric() || ch >= 0x80
            }

            fn scan_identifier(body: &[u8], end: usize, pos: &mut usize) {
                while *pos < end && is_word_char(body[*pos]) {
                    *pos += 1;
                }
            }

            fn scan_braced_unicode_escape_value(
                _parser: &mut ParserState,
                _emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
                body: &[u8],
                end: usize,
                pos: &mut usize,
            ) {
                // Skip past `\u{...}` body without validating the code-point
                // range. tsc treats out-of-range escapes inside regex literals
                // as a runtime concern and does not emit TS1198 here, even
                // with the `u` flag.
                *pos += 1;
                while *pos < end && body[*pos] != b'}' {
                    *pos += 1;
                }
                if *pos < end {
                    *pos += 1;
                }
            }

            const fn hex_value(byte: u8) -> Option<u32> {
                match byte {
                    b'0'..=b'9' => Some((byte - b'0') as u32),
                    b'a'..=b'f' => Some((byte - b'a' + 10) as u32),
                    b'A'..=b'F' => Some((byte - b'A' + 10) as u32),
                    _ => None,
                }
            }

            fn parse_fixed_hex_escape(body: &[u8], start: usize, len: usize) -> Option<u32> {
                let mut value = 0u32;
                for offset in 0..len {
                    value = (value << 4) | hex_value(*body.get(start + offset)?)?;
                }
                Some(value)
            }

            fn class_character_escape_atom(
                body: &[u8],
                end: usize,
                escape_start: usize,
            ) -> Option<ClassAtomKind> {
                let escaped = *body.get(escape_start)?;
                let value = match escaped {
                    b'x' => parse_fixed_hex_escape(body, escape_start + 1, 2)?,
                    b'u' if body.get(escape_start + 1).copied() == Some(b'{') => {
                        let hex_start = escape_start + 2;
                        let mut hex_end = hex_start;
                        while hex_end < end && body[hex_end] != b'}' {
                            hex_end += 1;
                        }
                        if hex_end >= end || hex_end == hex_start {
                            return None;
                        }
                        parse_fixed_hex_escape(body, hex_start, hex_end - hex_start)?
                    }
                    b'u' => parse_fixed_hex_escape(body, escape_start + 1, 4)?,
                    _ => u32::from(escaped),
                };
                Some(ClassAtomKind::Character {
                    value,
                    utf16_len: if value > 0xFFFF { 2 } else { 1 },
                })
            }

            fn is_identifier_part_for_regex_flags(ch: char) -> bool {
                if ch.is_ascii() {
                    matches!(
                        ch,
                        '_' | '$' | 'a'..='z' | 'A'..='Z' | '0'..='9'
                    )
                } else {
                    ch.is_alphabetic()
                        || ch.is_ascii_digit()
                        || ch == '\u{200c}'
                        || ch == '\u{200d}'
                }
            }

            const fn is_regex_flag(ch: char) -> bool {
                matches!(ch, 'g' | 'i' | 'm' | 's' | 'u' | 'v' | 'y' | 'd')
            }

            fn scan_regex_modifier_segment(
                parser: &mut ParserState,
                emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
                body: &[u8],
                end: usize,
                pos: &mut usize,
            ) -> bool {
                let mut consumed_any = false;

                while *pos < end {
                    let Some((ch, char_len)) = next_utf8_char(body, end, *pos) else {
                        break;
                    };

                    if is_regex_flag(ch) {
                        *pos += char_len;
                        consumed_any = true;
                        continue;
                    }

                    if is_identifier_part_for_regex_flags(ch) {
                        emit(
                            parser,
                            *pos,
                            1,
                            diagnostic_messages::UNKNOWN_REGULAR_EXPRESSION_FLAG,
                            diagnostic_codes::UNKNOWN_REGULAR_EXPRESSION_FLAG,
                        );
                        *pos += char_len;
                        consumed_any = true;
                        continue;
                    }

                    break;
                }

                consumed_any
            }

            #[allow(clippy::too_many_arguments)]
            fn scan_character_escape(
                parser: &mut ParserState,
                emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
                body: &[u8],
                strict_mode: bool,
                end: usize,
                pos: &mut usize,
                atom_escape: bool,
                escape_start: usize,
                _start_pos: u32,
            ) {
                if *pos >= end {
                    return;
                }

                let ch = body[*pos];

                match ch {
                    b'c' => {
                        *pos += 1;
                        if *pos < end && body[*pos].is_ascii_alphabetic() {
                            *pos += 1;
                        } else if strict_mode {
                            emit(
                                parser,
                                escape_start,
                                2,
                                "'\\c' must be followed by an ASCII letter.",
                                diagnostic_codes::C_MUST_BE_FOLLOWED_BY_AN_ASCII_LETTER,
                            );
                        } else if atom_escape {
                            *pos = (*pos).saturating_sub(1);
                        }
                    }
                    b'p' | b'P' => {
                        let escape_char = ch;
                        *pos += 1;
                        if *pos < end && body[*pos] == b'{' {
                            *pos += 1;
                            while *pos < end && body[*pos] != b'}' {
                                *pos += 1;
                            }
                            if *pos < end {
                                *pos += 1;
                            }
                        } else if strict_mode {
                            let message = if escape_char == b'P' {
                                "'\\P' must be followed by a Unicode property value expression enclosed in braces."
                            } else {
                                "'\\p' must be followed by a Unicode property value expression enclosed in braces."
                            };
                            emit(
                                parser,
                                escape_start,
                                2,
                                message,
                                diagnostic_codes::MUST_BE_FOLLOWED_BY_A_UNICODE_PROPERTY_VALUE_EXPRESSION_ENCLOSED_IN_BRACES,
                            );
                        }
                    }
                    b'o' if atom_escape => {
                        if strict_mode {
                            emit(
                                parser,
                                escape_start,
                                2,
                                "This character cannot be escaped in a regular expression.",
                                diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION,
                            );
                        }
                        *pos += 1;
                    }
                    b'u' => {
                        *pos += 1;
                        if *pos < end && body[*pos] == b'{' {
                            scan_braced_unicode_escape_value(parser, emit, body, end, pos);
                        } else {
                            let mut digits = 0usize;
                            while *pos < end && digits < 4 && body[*pos].is_ascii_hexdigit() {
                                *pos += 1;
                                digits += 1;
                            }
                            if strict_mode && digits == 0 && *pos + 1 < end && body[*pos] == b'\\' {
                                *pos += 2;
                            }
                        }
                    }
                    b'x' => {
                        *pos += 1;
                        if *pos < end && body[*pos].is_ascii_hexdigit() {
                            *pos += 1;
                        }
                        if *pos < end && body[*pos].is_ascii_hexdigit() {
                            *pos += 1;
                        }
                    }
                    b'0'..=b'9' => {
                        while *pos < end && body[*pos].is_ascii_digit() {
                            *pos += 1;
                        }
                    }
                    b'_' if strict_mode => {
                        emit(
                            parser,
                            escape_start,
                            2,
                            "This character cannot be escaped in a regular expression.",
                            diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION,
                        );
                        *pos += 1;
                    }
                    b'b' | b'd' | b'D' | b's' | b'S' | b'w' | b'W' | b't' | b'n' | b'v' | b'f'
                    | b'r' | b'^' | b'$' | b'/' | b'\\' | b'.' | b'*' | b'+' | b'?' | b'('
                    | b')' | b'[' | b']' | b'{' | b'}' | b'|' | b'-' | b',' | b'_' | b'#'
                    | b'%' | b';' | b':' | b'<' | b'=' | b'>' | b'@' | b'`' | b'~' => {
                        *pos += 1;
                    }
                    _ => {
                        if strict_mode {
                            emit(
                                parser,
                                escape_start,
                                2,
                                "This character cannot be escaped in a regular expression.",
                                diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION,
                            );
                        }
                        *pos += 1;
                    }
                }
            }

            fn scan_character_class_escape(
                parser: &mut ParserState,
                emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
                body: &[u8],
                strict_mode: bool,
                unicode_sets_mode: bool,
                _end: usize,
                pos: &mut usize,
                _start_pos: u32,
            ) -> Option<ClassAtomKind> {
                if *pos >= body.len() {
                    return None;
                }

                let start = *pos;
                match body[*pos] {
                    b'd' | b'D' | b's' | b'S' | b'w' | b'W' => {
                        *pos += 1;
                        Some(ClassAtomKind::Class)
                    }
                    b'q' if unicode_sets_mode => {
                        *pos += 1;
                        if *pos < body.len() && body[*pos] == b'{' {
                            *pos += 1;
                            while *pos < body.len() && body[*pos] != b'}' {
                                *pos += 1;
                            }
                            if *pos < body.len() {
                                *pos += 1;
                            }
                            Some(ClassAtomKind::Class)
                        } else {
                            Some(ClassAtomKind::Unknown)
                        }
                    }
                    b'P' => {
                        *pos += 1;
                        if *pos < body.len() && body[*pos] == b'{' {
                            *pos += 1;
                            while *pos < body.len() && body[*pos] != b'}' {
                                *pos += 1;
                            }
                            if *pos < body.len() {
                                *pos += 1;
                            }
                            Some(ClassAtomKind::Class)
                        } else if strict_mode {
                            emit(
                                parser,
                                start - 1,
                                2,
                                "'\\P' must be followed by a Unicode property value expression enclosed in braces.",
                                diagnostic_codes::MUST_BE_FOLLOWED_BY_A_UNICODE_PROPERTY_VALUE_EXPRESSION_ENCLOSED_IN_BRACES,
                            );
                            Some(ClassAtomKind::Class)
                        } else {
                            // Annex B: `\P` without braces is treated as the
                            // literal character `P`. Position is already past
                            // `P`, so emit a Character atom directly rather
                            // than returning None and letting the caller
                            // re-scan (which would consume the next escape).
                            Some(ClassAtomKind::Character {
                                value: u32::from(b'P'),
                                utf16_len: 1,
                            })
                        }
                    }
                    b'p' => {
                        *pos += 1;
                        if *pos < body.len() && body[*pos] == b'{' {
                            *pos += 1;
                            while *pos < body.len() && body[*pos] != b'}' {
                                *pos += 1;
                            }
                            if *pos < body.len() {
                                *pos += 1;
                            }
                            Some(ClassAtomKind::Class)
                        } else if strict_mode {
                            emit(
                                parser,
                                start - 1,
                                2,
                                "'\\p' must be followed by a Unicode property value expression enclosed in braces.",
                                diagnostic_codes::MUST_BE_FOLLOWED_BY_A_UNICODE_PROPERTY_VALUE_EXPRESSION_ENCLOSED_IN_BRACES,
                            );
                            Some(ClassAtomKind::Class)
                        } else {
                            // Annex B: `\p` without braces is treated as the
                            // literal character `p`. See `\P` above.
                            Some(ClassAtomKind::Character {
                                value: u32::from(b'p'),
                                utf16_len: 1,
                            })
                        }
                    }
                    _ => None,
                }
            }

            fn scan_class_atom<F>(
                parser: &mut ParserState,
                ctx: &RegexScanContext<'_, F>,
                pos: &mut usize,
                range: &mut Vec<ClassAtomKind>,
            ) where
                F: Fn(&mut ParserState, usize, u32, &str, u32),
            {
                if *pos >= ctx.body_end {
                    return;
                }
                let ch = ctx.body[*pos];
                if ch == b'\\' {
                    *pos += 1;
                    if *pos >= ctx.body_end {
                        return;
                    }

                    let class_escape_start = *pos;
                    match scan_character_class_escape(
                        parser,
                        ctx.emit,
                        &ctx.body[..ctx.body_end],
                        ctx.strict_mode,
                        ctx.unicode_sets_mode,
                        ctx.body_end,
                        pos,
                        ctx.start_pos,
                    ) {
                        Some(atom) => range.push(atom),
                        None => {
                            let current_pos = *pos;
                            scan_character_escape(
                                parser,
                                ctx.emit,
                                ctx.body,
                                ctx.strict_mode,
                                ctx.body_end,
                                pos,
                                false,
                                current_pos.saturating_sub(1),
                                ctx.start_pos,
                            );
                            if *pos > current_pos {
                                range.push(
                                    class_character_escape_atom(
                                        ctx.body,
                                        ctx.body_end,
                                        class_escape_start,
                                    )
                                    .unwrap_or(
                                        ClassAtomKind::Character {
                                            value: u32::from(ctx.body[class_escape_start]),
                                            utf16_len: 1,
                                        },
                                    ),
                                );
                            }
                        }
                    }
                    return;
                }

                if let Some((ch, char_len)) = next_utf8_char(ctx.body, ctx.body_end, *pos) {
                    range.push(ClassAtomKind::Character {
                        value: ch as u32,
                        utf16_len: ch.len_utf16(),
                    });
                    *pos += char_len;
                }

                if *pos <= ctx.body_end && *pos > 0 && ctx.body[*pos - 1] == b'-' {
                    range.push(ClassAtomKind::Unknown);
                }
            }

            fn scan_class_ranges<F>(
                parser: &mut ParserState,
                ctx: &RegexScanContext<'_, F>,
                pos: &mut usize,
            ) where
                F: Fn(&mut ParserState, usize, u32, &str, u32),
            {
                fn is_class_set_operator_at(body: &[u8], pos: usize, end: usize) -> bool {
                    pos + 1 < end
                        && ((body[pos] == b'&' && body[pos + 1] == b'&')
                            || (body[pos] == b'-' && body[pos + 1] == b'-'))
                }

                fn scan_class_set_operator(
                    parser: &mut ParserState,
                    emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
                    body: &[u8],
                    body_end: usize,
                    pos: &mut usize,
                ) {
                    *pos += 2;
                    if *pos >= body_end
                        || body[*pos] == b']'
                        || is_class_set_operator_at(body, *pos, body_end)
                    {
                        emit(
                            parser,
                            *pos,
                            1,
                            diagnostic_messages::EXPECTED_A_CLASS_SET_OPERAND,
                            diagnostic_codes::EXPECTED_A_CLASS_SET_OPERAND,
                        );
                    }
                }

                // Consume optional leading ^
                if *pos < ctx.body_end && ctx.body[*pos] == b'^' {
                    *pos += 1;
                }

                while *pos < ctx.body_end {
                    if ctx.body[*pos] == b']' {
                        *pos += 1;
                        break;
                    }
                    if ctx.unicode_sets_mode
                        && is_class_set_operator_at(ctx.body, *pos, ctx.body_end)
                    {
                        scan_class_set_operator(parser, ctx.emit, ctx.body, ctx.body_end, pos);
                        continue;
                    }

                    let mut atoms = Vec::new();
                    let min_start = *pos;
                    scan_class_atom(parser, ctx, pos, &mut atoms);
                    if ctx.unicode_sets_mode
                        && is_class_set_operator_at(ctx.body, *pos, ctx.body_end)
                    {
                        scan_class_set_operator(parser, ctx.emit, ctx.body, ctx.body_end, pos);
                        continue;
                    }
                    if *pos >= ctx.body_end || ctx.body[*pos] != b'-' {
                        continue;
                    }

                    *pos += 1;

                    if *pos < ctx.body_end && ctx.body[*pos] == b']' {
                        *pos += 1;
                        break;
                    }

                    let max_start = *pos;
                    let mut max_atoms = Vec::new();
                    scan_class_atom(parser, ctx, pos, &mut max_atoms);

                    let min_atom = atoms.first().copied();
                    let max_atom = max_atoms.first().copied();

                    if ctx.strict_mode {
                        if matches!(
                            min_atom,
                            Some(ClassAtomKind::Unknown | ClassAtomKind::Class)
                        ) {
                            (ctx.emit)(
                                parser,
                                min_start,
                                1,
                                "A character class range must not be bounded by another character class.",
                                diagnostic_codes::A_CHARACTER_CLASS_RANGE_MUST_NOT_BE_BOUNDED_BY_ANOTHER_CHARACTER_CLASS,
                            );
                        }
                        if matches!(
                            max_atom,
                            Some(ClassAtomKind::Unknown | ClassAtomKind::Class)
                        ) {
                            (ctx.emit)(
                                parser,
                                max_start,
                                1,
                                "A character class range must not be bounded by another character class.",
                                diagnostic_codes::A_CHARACTER_CLASS_RANGE_MUST_NOT_BE_BOUNDED_BY_ANOTHER_CHARACTER_CLASS,
                            );
                        }
                    }

                    if let (
                        Some(ClassAtomKind::Character {
                            value: left,
                            utf16_len: 1,
                        }),
                        Some(ClassAtomKind::Character {
                            value: right,
                            utf16_len: 1,
                        }),
                    ) = (min_atom, max_atom)
                        && left > right
                    {
                        (ctx.emit)(
                            parser,
                            min_start,
                            (max_start as u32).saturating_sub(min_start as u32),
                            "Range out of order in character class.",
                            diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS,
                        );
                    }
                }
            }

            fn scan_alternative<F>(
                parser: &mut ParserState,
                ctx: &RegexScanContext<'_, F>,
                pos: &mut usize,
                in_group: bool,
            ) where
                F: Fn(&mut ParserState, usize, u32, &str, u32),
            {
                let mut is_previous_term_quantifiable = false;

                while *pos < ctx.body_end {
                    let current = ctx.body[*pos];
                    match current {
                        b'^' | b'$' => {
                            *pos += 1;
                            is_previous_term_quantifiable = false;
                        }
                        b'\\' => {
                            *pos += 1;
                            if *pos >= ctx.body_end {
                                break;
                            }

                            let escape_start = *pos - 1;

                            if ctx.body[*pos] == b'k' {
                                *pos += 1;
                                if *pos < ctx.body_end && ctx.body[*pos] == b'<' {
                                    *pos += 1;
                                    scan_identifier(ctx.body, ctx.body_end, pos);
                                    if *pos < ctx.body_end && ctx.body[*pos] == b'>' {
                                        *pos += 1;
                                    }
                                } else if ctx.strict_mode {
                                    (ctx.emit)(
                                        parser,
                                        escape_start,
                                        2,
                                        "'\\k' must be followed by a capturing group name enclosed in angle brackets.",
                                        diagnostic_codes::K_MUST_BE_FOLLOWED_BY_A_CAPTURING_GROUP_NAME_ENCLOSED_IN_ANGLE_BRACKETS,
                                    );
                                }
                            } else {
                                scan_character_escape(
                                    parser,
                                    ctx.emit,
                                    ctx.body,
                                    ctx.strict_mode,
                                    ctx.body_end,
                                    pos,
                                    true,
                                    escape_start,
                                    ctx.start_pos,
                                );
                            }

                            is_previous_term_quantifiable = true;
                        }
                        b'(' => {
                            *pos += 1;
                            if *pos >= ctx.body_end {
                                break;
                            }

                            if ctx.body[*pos] == b'?' {
                                *pos += 1;
                                if *pos >= ctx.body_end {
                                    break;
                                }
                                match ctx.body[*pos] {
                                    b'=' | b'!' => {
                                        *pos += 1;
                                        is_previous_term_quantifiable = !ctx.strict_mode;
                                        scan_disjunction(parser, ctx, pos, true);
                                    }
                                    b'<' => {
                                        *pos += 1;
                                        if *pos < ctx.body_end
                                            && (ctx.body[*pos] == b'=' || ctx.body[*pos] == b'!')
                                        {
                                            *pos += 1;
                                            is_previous_term_quantifiable = false;
                                        } else {
                                            scan_identifier(ctx.body, ctx.body_end, pos);
                                            if *pos < ctx.body_end && ctx.body[*pos] == b'>' {
                                                *pos += 1;
                                            }
                                            is_previous_term_quantifiable = true;
                                        }
                                        scan_disjunction(parser, ctx, pos, true);
                                    }
                                    _ => {
                                        let saved_pos = *pos;
                                        let has_first = scan_regex_modifier_segment(
                                            parser,
                                            ctx.emit,
                                            ctx.body,
                                            ctx.body_end,
                                            pos,
                                        );

                                        if has_first
                                            && *pos < ctx.body_end
                                            && ctx.body[*pos] == b'-'
                                        {
                                            *pos += 1;
                                            if *pos < ctx.body_end {
                                                let has_second = scan_regex_modifier_segment(
                                                    parser,
                                                    ctx.emit,
                                                    ctx.body,
                                                    ctx.body_end,
                                                    pos,
                                                );

                                                if !has_second {
                                                    *pos = saved_pos;
                                                }
                                            } else {
                                                *pos = saved_pos;
                                            }
                                        }

                                        let is_modifier_group = has_first
                                            && *pos < ctx.body_end
                                            && ctx.body[*pos] == b':';

                                        if !is_modifier_group {
                                            *pos = saved_pos;
                                        } else {
                                            *pos += 1;
                                            is_previous_term_quantifiable = true;
                                        }

                                        if !is_modifier_group {
                                            is_previous_term_quantifiable = true;
                                        }

                                        scan_disjunction(parser, ctx, pos, true);
                                    }
                                }
                            } else {
                                is_previous_term_quantifiable = true;
                                scan_disjunction(parser, ctx, pos, true);
                            }

                            if *pos < ctx.body_end && ctx.body[*pos] == b')' {
                                *pos += 1;
                            }
                        }
                        b'{' => {
                            let brace_start = *pos;
                            let had_quantifiable_term = is_previous_term_quantifiable;
                            let mut reported_nothing_at_brace = false;
                            *pos += 1;
                            let min_start = *pos;
                            let min_length = scan_digits(ctx.body, ctx.body_end, pos);
                            let min_empty = min_length == 0;

                            let min_text = if !min_empty && min_start < *pos {
                                &ctx.body[min_start..*pos]
                            } else {
                                b""
                            };

                            if *pos < ctx.body_end && ctx.body[*pos] == b',' {
                                let comma_pos = *pos;
                                *pos += 1;
                                let max_start = *pos;
                                let max_length = scan_digits(ctx.body, ctx.body_end, pos);
                                let max_empty = max_length == 0;

                                let has_closing = *pos < ctx.body_end && ctx.body[*pos] == b'}';
                                if min_empty {
                                    if ctx.strict_mode && (max_length > 0 || has_closing) {
                                        if !had_quantifiable_term {
                                            (ctx.emit)(
                                                parser,
                                                brace_start,
                                                1,
                                                "There is nothing available for repetition.",
                                                diagnostic_codes::THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION,
                                            );
                                            reported_nothing_at_brace = true;
                                        }
                                        (ctx.emit)(
                                            parser,
                                            comma_pos,
                                            1,
                                            "Incomplete quantifier. Digit expected.",
                                            diagnostic_codes::INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED,
                                        );
                                    } else if ctx.strict_mode {
                                        (ctx.emit)(
                                            parser,
                                            brace_start,
                                            1,
                                            "Unexpected '{'. Did you mean to escape it with backslash?",
                                            diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                                        );
                                        is_previous_term_quantifiable = true;
                                        continue;
                                    } else {
                                        is_previous_term_quantifiable = true;
                                        continue;
                                    }
                                } else if max_length > 0 && !max_empty {
                                    let max_value: u32 = ctx.body[max_start..*pos]
                                        .iter()
                                        .fold(0u32, |acc, b| acc * 10 + u32::from(*b - b'0'));
                                    let min_value: u32 = min_text
                                        .iter()
                                        .fold(0u32, |acc, b| acc * 10 + u32::from(*b - b'0'));
                                    if max_value < min_value && (ctx.strict_mode || has_closing) {
                                        (ctx.emit)(
                                            parser,
                                            min_start,
                                            (min_start.max(*pos).saturating_sub(min_start)) as u32,
                                            "Numbers out of order in quantifier.",
                                            diagnostic_codes::NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER,
                                        );
                                    }
                                }

                                if *pos >= ctx.body_end || ctx.body[*pos] != b'}' {
                                    if ctx.strict_mode {
                                        if !had_quantifiable_term
                                            && !min_empty
                                            && !reported_nothing_at_brace
                                        {
                                            (ctx.emit)(
                                                parser,
                                                brace_start,
                                                1,
                                                "There is nothing available for repetition.",
                                                diagnostic_codes::THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION,
                                            );
                                        }
                                        (ctx.emit)(
                                            parser,
                                            *pos,
                                            0,
                                            "'}' expected.",
                                            diagnostic_codes::EXPECTED,
                                        );
                                        if *pos + 1 < ctx.body_end
                                            && ctx.body[*pos] == b'?'
                                            && ctx.body[*pos + 1] == b'?'
                                        {
                                            *pos += 1;
                                        }
                                        is_previous_term_quantifiable = false;
                                        continue;
                                    }
                                    is_previous_term_quantifiable = true;
                                    continue;
                                }

                                if !had_quantifiable_term && !reported_nothing_at_brace {
                                    (ctx.emit)(
                                        parser,
                                        brace_start,
                                        1,
                                        "There is nothing available for repetition.",
                                        diagnostic_codes::THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION,
                                    );
                                }
                                *pos += 1;
                                is_previous_term_quantifiable = false;
                                if *pos < ctx.body_end && ctx.body[*pos] == b'?' {
                                    *pos += 1;
                                }
                                continue;
                            } else if min_empty {
                                if ctx.strict_mode {
                                    (ctx.emit)(
                                        parser,
                                        brace_start,
                                        1,
                                        "Unexpected '{'. Did you mean to escape it with backslash?",
                                        diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                                    );
                                    is_previous_term_quantifiable = true;
                                    continue;
                                }
                                is_previous_term_quantifiable = true;
                                continue;
                            } else if *pos >= ctx.body_end || ctx.body[*pos] != b'}' {
                                if ctx.strict_mode {
                                    if !had_quantifiable_term && !reported_nothing_at_brace {
                                        (ctx.emit)(
                                            parser,
                                            brace_start,
                                            1,
                                            "There is nothing available for repetition.",
                                            diagnostic_codes::THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION,
                                        );
                                    }
                                    (ctx.emit)(
                                        parser,
                                        *pos,
                                        0,
                                        "'}' expected.",
                                        diagnostic_codes::EXPECTED,
                                    );
                                    if *pos + 1 < ctx.body_end
                                        && ctx.body[*pos] == b'?'
                                        && ctx.body[*pos + 1] == b'?'
                                    {
                                        *pos += 1;
                                    }
                                    is_previous_term_quantifiable = false;
                                    continue;
                                }
                                is_previous_term_quantifiable = true;
                                continue;
                            }

                            if !had_quantifiable_term && !reported_nothing_at_brace {
                                (ctx.emit)(
                                    parser,
                                    brace_start,
                                    1,
                                    "There is nothing available for repetition.",
                                    diagnostic_codes::THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION,
                                );
                            }
                            *pos += 1;
                            is_previous_term_quantifiable = false;
                            if *pos < ctx.body_end && ctx.body[*pos] == b'?' {
                                *pos += 1;
                            }
                        }
                        b'*' | b'+' | b'?' => {
                            let quantifier_start = *pos;
                            *pos += 1;
                            if *pos < ctx.body_end && ctx.body[*pos] == b'?' {
                                *pos += 1;
                            }
                            if !is_previous_term_quantifiable {
                                (ctx.emit)(
                                    parser,
                                    quantifier_start,
                                    (*pos as u32).saturating_sub(quantifier_start as u32),
                                    "There is nothing available for repetition.",
                                    diagnostic_codes::THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION,
                                );
                            }
                            is_previous_term_quantifiable = false;
                        }
                        b'[' => {
                            *pos += 1;
                            scan_class_ranges(parser, ctx, pos);
                            is_previous_term_quantifiable = true;
                        }
                        b')' => {
                            if in_group {
                                return;
                            }
                            if ctx.strict_mode {
                                (ctx.emit)(
                                    parser,
                                    *pos,
                                    1,
                                    "Unexpected ')'. Did you mean to escape it with backslash?",
                                    diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                                );
                            }
                            *pos += 1;
                            is_previous_term_quantifiable = true;
                        }
                        b']' => {
                            if ctx.strict_mode {
                                (ctx.emit)(
                                    parser,
                                    *pos,
                                    1,
                                    "Unexpected ']'. Did you mean to escape it with backslash?",
                                    diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                                );
                            }
                            *pos += 1;
                            is_previous_term_quantifiable = true;
                        }
                        b'}' => {
                            if ctx.strict_mode {
                                (ctx.emit)(
                                    parser,
                                    *pos,
                                    1,
                                    "Unexpected '}'. Did you mean to escape it with backslash?",
                                    diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                                );
                            }
                            *pos += 1;
                            is_previous_term_quantifiable = true;
                        }
                        b'/' | b'|' => return,
                        _ => {
                            if let Some((_ch, ch_len)) =
                                next_utf8_char(ctx.body, ctx.body_end, *pos)
                            {
                                *pos += ch_len;
                            } else {
                                break;
                            }
                            is_previous_term_quantifiable = true;
                        }
                    }
                }
            }

            fn scan_disjunction<F>(
                parser: &mut ParserState,
                ctx: &RegexScanContext<'_, F>,
                pos: &mut usize,
                in_group: bool,
            ) where
                F: Fn(&mut ParserState, usize, u32, &str, u32),
            {
                loop {
                    scan_alternative(parser, ctx, pos, in_group);

                    if *pos >= ctx.body_end || ctx.body[*pos] != b'|' {
                        return;
                    }

                    *pos += 1;
                }
            }

            let ctx = RegexScanContext {
                emit: &emit,
                body: bytes,
                body_end,
                strict_mode,
                unicode_sets_mode,
                start_pos,
            };
            let mut pos = 1usize;
            scan_disjunction(parser, &ctx, &mut pos, false);
        }

        let start_pos = self.token_pos();

        // Rescan the / or /= as a regex literal
        self.scanner.re_scan_slash_token();
        self.current_token = self.scanner.get_token();

        // Check for unterminated regex literal (TS1161)
        let regex_is_unterminated =
            (self.scanner.get_token_flags() & TokenFlags::Unterminated as u32) != 0;
        if regex_is_unterminated {
            // Suppress TS1161 when the unterminated "regex" body is the tail of a
            // JSX closing tag (e.g., `</a:b>` parsed outside JSX context where `/`
            // is misinterpreted as a regex start). The slash must be immediately
            // preceded by `<`; ordinary regex bodies may contain `<...>` text.
            let regex_body = self.scanner.get_token_text_ref();
            let slash_starts_jsx_closing_tag = start_pos > 0
                && self
                    .get_source_text()
                    .as_bytes()
                    .get(start_pos as usize - 1)
                    == Some(&b'<');
            let is_jsx_artifact = slash_starts_jsx_closing_tag
                && regex_body.find('>').is_some_and(|gt_pos| {
                    regex_body
                        .find(';')
                        .is_none_or(|semi_pos| gt_pos < semi_pos)
                });
            if !is_jsx_artifact {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    1,
                    "Unterminated regular expression literal.",
                    diagnostic_codes::UNTERMINATED_REGULAR_EXPRESSION_LITERAL,
                );
            }
        }

        // Get the regex text (including slashes and flags)
        let text = self.scanner.get_token_value_ref().to_string();
        let raw_text = self.scanner.get_token_text_ref().to_string();

        // Capture regex flag errors BEFORE calling parse_expected (which clears them via next_token)
        let flag_errors: Vec<_> = self.scanner.get_regex_flag_errors().to_vec();
        self.report_invalid_regular_expression_escape_errors();
        let extended_unicode_escape_errors = regex_body_end(&raw_text)
            .filter(|body_end| {
                let flags = &raw_text[*body_end + 1..];
                !flags.contains('u') && !flags.contains('v')
            })
            .map(|body_end| {
                let bytes = raw_text.as_bytes();
                let mut errors = Vec::new();
                let mut i = 1usize;
                while i + 2 < body_end {
                    if bytes[i] == b'\\' && bytes[i + 1] == b'u' && bytes[i + 2] == b'{' {
                        let mut j = i + 3;
                        while j < body_end && bytes[j] != b'}' {
                            j += 1;
                        }
                        if j < body_end {
                            errors.push((start_pos + i as u32, (j + 1 - i) as u32));
                            i = j + 1;
                            continue;
                        }
                    }
                    i += 1;
                }
                errors
            })
            .unwrap_or_default();
        let range_order_errors = regex_body_end(&raw_text)
            .map(|body_end| regex_range_order_errors(&raw_text, body_end))
            .unwrap_or_default();
        regex_body_end(&raw_text).into_iter().for_each(|body_end| {
            validate_regex_literal_body(self, &raw_text, start_pos, body_end);
        });

        // Capture the regex token end before consuming it so missing-token diagnostics
        // anchor to the actual regex literal location, not the following token.
        let regex_end_pos = self.token_end();
        let regex_body_end = regex_body_end(&raw_text);

        self.parse_expected(SyntaxKind::RegularExpressionLiteral);

        if !regex_is_unterminated && let Some(missing) = self.missing_regex_closing_token(&text) {
            // Position the missing-token message at the end of the regex body (the
            // slash/flag boundary), matching tsc behavior for malformed character
            // classes and groups.
            let missing_pos = if let Some(body_end) = regex_body_end {
                start_pos + body_end as u32
            } else {
                regex_end_pos.saturating_sub(1)
            };

            let message = if missing == b']' {
                "']' expected."
            } else {
                "')' expected."
            };
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(missing_pos, 1, message, diagnostic_codes::EXPECTED);
        }

        // Emit errors for all regex flag issues detected by scanner
        if !self.regex_literal_follows_invalid_shebang(start_pos) {
            for error in flag_errors {
                let (message, code) = match error.kind {
                    tsz_scanner::scanner_impl::RegexFlagErrorKind::Duplicate => {
                        ("Duplicate regular expression flag.", 1500)
                    }
                    tsz_scanner::scanner_impl::RegexFlagErrorKind::InvalidFlag => {
                        ("Unknown regular expression flag.", 1499)
                    }
                    tsz_scanner::scanner_impl::RegexFlagErrorKind::IncompatibleFlags => (
                        "The Unicode 'u' flag and the Unicode Sets 'v' flag cannot be set simultaneously.",
                        1502,
                    ),
                };
                self.parse_error_at(self.u32_from_usize(error.pos), 1, message, code);
            }
        }
        for (pos, len) in extended_unicode_escape_errors {
            self.parse_error_at(
                pos,
                len,
                tsz_common::diagnostics::diagnostic_messages::UNICODE_ESCAPE_SEQUENCES_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR_THE_UNICO,
                tsz_common::diagnostics::diagnostic_codes::UNICODE_ESCAPE_SEQUENCES_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR_THE_UNICO,
            );
        }
        for (pos, len) in range_order_errors {
            self.parse_error_at(
                start_pos + pos,
                len,
                tsz_common::diagnostics::diagnostic_messages::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS,
                tsz_common::diagnostics::diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS,
            );
        }

        self.arena.add_literal(
            SyntaxKind::RegularExpressionLiteral as u16,
            start_pos,
            regex_end_pos,
            LiteralData {
                text,
                raw_text: Some(raw_text),
                value: None,
                has_invalid_escape: false,
            },
        )
    }

    fn missing_regex_closing_token(&self, text: &str) -> Option<u8> {
        let bytes = text.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'/' {
            return None;
        }

        // Mirror the regex scan state for body extraction.
        let mut in_escape = false;
        let mut in_character_class = false;
        let mut body_end = bytes.len();

        for (i, ch) in bytes.iter().enumerate().skip(1) {
            let ch = *ch;
            if in_escape {
                in_escape = false;
                continue;
            }
            if ch == b'\\' {
                in_escape = true;
            } else if ch == b'[' && !in_character_class {
                in_character_class = true;
            } else if ch == b']' && in_character_class {
                in_character_class = false;
            } else if ch == b'/' && !in_character_class {
                body_end = i;
                break;
            }
        }

        if body_end <= 1 {
            return None;
        }

        let mut missing = None;
        let mut paren_depth = 0i32;
        in_escape = false;
        in_character_class = false;
        for &ch in &bytes[1..body_end] {
            if in_escape {
                in_escape = false;
                continue;
            }
            if ch == b'\\' {
                in_escape = true;
                continue;
            }
            if in_character_class {
                if ch == b']' {
                    in_character_class = false;
                }
                continue;
            }
            match ch {
                b'[' => in_character_class = true,
                b'(' => paren_depth += 1,
                b')' if paren_depth > 0 => paren_depth -= 1,
                _ => {}
            }
        }

        if in_character_class {
            missing = Some(b']');
        }
        if missing.is_none() && paren_depth > 0 {
            missing = Some(b')');
        }

        missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regression for issue #4787: the regex range-order helper used
    // `u32::try_from(start).expect(...)` for absolute UTF-8 offsets, so a
    // sufficiently large absolute offset (in pathological / crafted input)
    // would panic the parser instead of degrading gracefully. After the
    // fix, oversized offsets simply produce an empty offset vector — the
    // range-order pass loses precision for those atoms but the parser
    // does not crash.
    #[test]
    fn split_non_unicode_atom_offsets_returns_empty_vec_when_offset_overflows_u32() {
        // start near usize::MAX guarantees `start + ...` cannot fit in u32
        // on 64-bit platforms.
        let offsets = split_non_unicode_atom_offsets(usize::MAX, 'a');
        assert!(
            offsets.is_empty(),
            "expected empty offset vec on u32 overflow, got {offsets:?}",
        );
    }

    #[test]
    fn split_non_unicode_atom_offsets_returns_offsets_for_bmp_chars() {
        // BMP char: one UTF-16 code unit, one UTF-8 byte. Offsets should
        // round-trip the start position unchanged.
        let offsets = split_non_unicode_atom_offsets(7, 'a');
        assert_eq!(offsets, vec![7]);
    }

    #[test]
    fn split_non_unicode_atom_offsets_returns_two_offsets_for_surrogate_pair() {
        // U+1F600 (😀) encodes to two UTF-16 code units and four UTF-8
        // bytes; the helper should yield two distinct offsets that both
        // fit in u32 for normal inputs.
        let offsets = split_non_unicode_atom_offsets(0, '\u{1F600}');
        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets[0], 0);
        // Second surrogate's offset = (1 * 4) / 2 = 2.
        assert_eq!(offsets[1], 2);
    }

    #[test]
    fn split_non_unicode_atom_offsets_drops_only_overflowing_entries() {
        // Pick a `start` such that the FIRST surrogate fits in u32 but the
        // SECOND does not. With a surrogate-pair char the second offset is
        // `start + 2`, so a start of `u32::MAX as usize - 1` makes the
        // first offset = u32::MAX - 1 (fits) and the second = u32::MAX + 1
        // (overflows). filter_map drops only the overflowing entry.
        let start = u32::MAX as usize - 1;
        let offsets = split_non_unicode_atom_offsets(start, '\u{1F600}');
        assert_eq!(
            offsets,
            vec![u32::MAX - 1],
            "first surrogate kept, second dropped",
        );
    }
}
