//! Regex literal parsing extracted from `state_expressions_literals.rs`.
//!
//! Pure file-organization move; no logic changes. Keeps `state_expressions_literals.rs`
//! under the parser LOC ceiling.

use super::regex_group_names;
use super::regex_unicode_properties::{
    BINARY_UNICODE_PROPERTIES_OF_STRINGS, canonical_non_binary_property_name,
    is_known_unicode_property_name_or_value, unicode_property_value_is_known,
};
use super::state::ParserState;
use crate::parser::{NodeIndex, node::LiteralData};
use std::cell::RefCell;
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::TokenFlags;

mod class_string_disjunction;
mod range_order;

use class_string_disjunction::scan_class_string_disjunction_body;

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

/// Combine a high/low UTF-16 surrogate pair into its code point, or `None`
/// when the halves are not a valid surrogate pair. Shared by the regex body
/// scanner and the `range_order` range-order pass.
const fn decode_surrogate_pair(high: u32, low: u32) -> Option<u32> {
    if high < 0xD800 || high > 0xDBFF || low < 0xDC00 || low > 0xDFFF {
        return None;
    }
    Some(0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00))
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
                Character,
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
                capturing_group_count: u32,
                /// Per-alternative capturing-group name scopes, shared across
                /// the whole recursive scan. `RefCell` because the scan hands
                /// `&RegexScanContext` down every level.
                group_names: &'a RefCell<regex_group_names::GroupNameScopes>,
            }

            struct CharEscapeScanCtx<'a> {
                body: &'a [u8],
                strict_mode: bool,
                unicode_sets_mode: bool,
                end: usize,
                capturing_group_count: u32,
            }

            /// Count the capturing groups in the whole pattern, the way tsc's
            /// `scanRegularExpressionWorker` does on its first pass.
            ///
            /// A backreference is legal when it names a group that exists
            /// *anywhere* in the pattern, including one that appears later or in
            /// another alternative, so the count has to be known before any
            /// escape is judged. Capturing forms are `(` and `(?<name>`;
            /// `(?:`, `(?=`, `(?!`, `(?<=`, `(?<!` and modifier groups are not.
            fn count_capturing_groups(body: &[u8], body_end: usize) -> u32 {
                let mut count = 0u32;
                let mut in_class = false;
                let mut pos = 1usize;

                while pos < body_end {
                    match body[pos] {
                        // Skip the escaped byte so `\(` and `\[` never open
                        // anything. Continuation bytes of a multi-byte escaped
                        // character are all >= 0x80, so they match no arm here.
                        b'\\' => pos += 1,
                        b'[' if !in_class => in_class = true,
                        b']' if in_class => in_class = false,
                        b'(' if !in_class => {
                            if body.get(pos + 1) == Some(&b'?') {
                                // `(?<name>` captures; `(?<=` and `(?<!` are
                                // lookbehind assertions and do not.
                                if body.get(pos + 2) == Some(&b'<')
                                    && !matches!(body.get(pos + 3), Some(&b'=' | &b'!'))
                                {
                                    count += 1;
                                }
                            } else {
                                count += 1;
                            }
                        }
                        _ => {}
                    }
                    pos += 1;
                }

                count
            }

            fn scan_digits(body: &[u8], end: usize, pos: &mut usize) -> usize {
                let start = *pos;
                while *pos < end && body[*pos].is_ascii_digit() {
                    *pos += 1;
                }
                *pos - start
            }

            /// Consume a legacy (Annex-B) octal digit run starting at `pos`
            /// (which points at the leading digit, already known to be
            /// `0`..=`7`) and return the exclusive end. Mirrors tsc's
            /// per-leading-digit maximum: a leading `0`-`3` may pull up to 3
            /// octal digits (`\377` fits in a byte), a leading `4`-`7` only 2
            /// (`\477` parses as `\47` followed by a literal `7`).
            fn scan_legacy_octal_digits(body: &[u8], end: usize, pos: usize) -> usize {
                let max_digits = if body[pos] <= b'3' { 3 } else { 2 };
                let mut octal_end = pos + 1;
                let mut count = 1usize;
                while count < max_digits
                    && octal_end < end
                    && (b'0'..=b'7').contains(&body[octal_end])
                {
                    octal_end += 1;
                    count += 1;
                }
                octal_end
            }

            /// Interpret `body[start..octal_end]` (a run produced by
            /// `scan_legacy_octal_digits`) as a base-8 integer.
            fn legacy_octal_value(body: &[u8], start: usize, octal_end: usize) -> u32 {
                let mut value = 0u32;
                for &byte in &body[start..octal_end] {
                    value = value * 8 + u32::from(byte - b'0');
                }
                value
            }

            const fn hex_byte_value(byte: u8) -> Option<u32> {
                match byte {
                    b'0'..=b'9' => Some((byte - b'0') as u32),
                    b'a'..=b'f' => Some((byte - b'a' + 10) as u32),
                    b'A'..=b'F' => Some((byte - b'A' + 10) as u32),
                    _ => None,
                }
            }

            use crate::parser::regex_modifier_groups::next_utf8_char;

            /// Scan a `(?<name>` or `\k<name>` group name and report the
            /// grammar errors tsc's `scanGroupName` reports there.
            ///
            /// `pos` must sit just past the `<`. Reports `TS1514` when no
            /// identifier is present, `TS1515` when a *declaration*'s name is
            /// already visible in the current or an enclosing alternative, and
            /// consumes the closing `>` or reports `'>' expected.` in its
            /// place. Reference names (`\k<...>`) are only shape-checked here;
            /// their resolution against the pattern's declared names is the
            /// checker's existing `TS1532` pass.
            fn scan_group_name_and_delimiter<F>(
                parser: &mut ParserState,
                ctx: &RegexScanContext<'_, F>,
                pos: &mut usize,
                is_reference: bool,
            ) where
                F: Fn(&mut ParserState, usize, u32, &str, u32),
            {
                let name_start = *pos;
                let scanned = regex_group_names::scan_group_name(ctx.body, ctx.body_end, pos);

                match scanned {
                    None => {
                        (ctx.emit)(
                            parser,
                            name_start,
                            0,
                            diagnostic_messages::EXPECTED_A_CAPTURING_GROUP_NAME,
                            diagnostic_codes::EXPECTED_A_CAPTURING_GROUP_NAME,
                        );
                    }
                    Some(name) if !is_reference => {
                        if !ctx.group_names.borrow_mut().declare(&name) {
                            (ctx.emit)(
                                parser,
                                name_start,
                                (*pos - name_start) as u32,
                                diagnostic_messages::NAMED_CAPTURING_GROUPS_WITH_THE_SAME_NAME_MUST_BE_MUTUALLY_EXCLUSIVE_TO_EACH_OTH,
                                diagnostic_codes::NAMED_CAPTURING_GROUPS_WITH_THE_SAME_NAME_MUST_BE_MUTUALLY_EXCLUSIVE_TO_EACH_OTH,
                            );
                        }
                    }
                    Some(_) => {}
                }

                if *pos < ctx.body_end && ctx.body[*pos] == b'>' {
                    *pos += 1;
                } else {
                    // tsc's `scanExpectedChar`. A zero-length report at the
                    // same position as the `TS1514` above is dropped by the
                    // parser's same-position dedup, which is why
                    // `/(?<1a>x)/` is one diagnostic in tsc and not two.
                    (ctx.emit)(parser, *pos, 0, "'>' expected.", diagnostic_codes::EXPECTED);
                }
            }

            fn read_fixed_hex(body: &[u8], pos: usize, len: usize) -> Option<u32> {
                if pos + len > body.len() {
                    return None;
                }
                let mut value = 0u32;
                for offset in 0..len {
                    value = (value << 4) | hex_byte_value(body[pos + offset])?;
                }
                Some(value)
            }

            fn scan_braced_unicode_escape_value(
                parser: &mut ParserState,
                emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
                body: &[u8],
                end: usize,
                pos: &mut usize,
                strict_mode: bool,
            ) {
                // `*pos` is at the `{`. Under the Unicode (`u`) or Unicode-sets
                // (`v`) flag, `\u{ HexDigits }` is a code-point escape whose value
                // must be a run of hex digits no greater than 0x10FFFF; tsc
                // validates it (mirrors the string-literal `\u{…}` checks). Without
                // a Unicode flag the `{` merely opens a quantifier, so no
                // validation applies. The broader malformed-escape diagnostics
                // (TS1508 "Unexpected …") live in the full regex-grammar validator
                // (task #74); this helper is the separable seam it will extend.
                *pos += 1; // consume `{`
                let hex_start = *pos;
                let mut has_digit = false;
                while *pos < end && body[*pos].is_ascii_hexdigit() {
                    has_digit = true;
                    *pos += 1;
                }
                let hex_end = *pos;
                if strict_mode && hex_end < end && body[hex_end] == b'}' {
                    if !has_digit {
                        emit(
                            parser,
                            hex_end,
                            1,
                            diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                            diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                        );
                    } else {
                        let hex = std::str::from_utf8(&body[hex_start..hex_end]).unwrap_or("");
                        let out_of_range =
                            u32::from_str_radix(hex, 16).map_or(true, |value| value > 0x10FFFF);
                        if out_of_range {
                            emit(
                                parser,
                                hex_start,
                                (hex_end - hex_start) as u32,
                                diagnostic_messages::AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE,
                                diagnostic_codes::AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE,
                            );
                        }
                    }
                }
                // Recovery: skim to and past the closing `}` (legacy behavior) so
                // the outer body scan continues correctly regardless of validity.
                while *pos < end && body[*pos] != b'}' {
                    *pos += 1;
                }
                if *pos < end {
                    *pos += 1;
                }
            }

            use crate::parser::regex_modifier_groups::scan_modifier_group_prelude;

            fn scan_character_escape(
                parser: &mut ParserState,
                emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
                scan_ctx: &CharEscapeScanCtx<'_>,
                pos: &mut usize,
                atom_escape: bool,
                escape_start: usize,
            ) {
                let body = scan_ctx.body;
                let strict_mode = scan_ctx.strict_mode;
                let end = scan_ctx.end;
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
                            // At atom position there is no enclosing class to
                            // judge, so the strings answer is discarded; only
                            // the `\P` rejection above can fire here.
                            let mut may_contain_strings = false;
                            scan_unicode_property_value_expression(
                                parser,
                                emit,
                                body,
                                PropertyExpressionMode {
                                    unicode: strict_mode,
                                    unicode_sets: scan_ctx.unicode_sets_mode,
                                    negated: escape_char == b'P',
                                },
                                end,
                                pos,
                                escape_start,
                                &mut may_contain_strings,
                            );
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
                    // `\q` is a character-class-only atom (`scan_character_class_escape`
                    // owns its in-class form). Outside a class under the `v` flag it is
                    // still a reserved escape, but tsc names the specific reason
                    // (TS1511) rather than falling through to the generic
                    // THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION this arm's
                    // `_` fallback would otherwise report. Not gated on `atom_escape`:
                    // this function is never reached for `q` from inside a class while
                    // `unicode_sets_mode` is true, because
                    // `scan_character_class_escape`'s own `b'q' if unicode_sets_mode`
                    // arm always claims it first.
                    b'q' if scan_ctx.unicode_sets_mode => {
                        emit(
                            parser,
                            escape_start,
                            2,
                            diagnostic_messages::Q_IS_ONLY_AVAILABLE_INSIDE_CHARACTER_CLASS,
                            diagnostic_codes::Q_IS_ONLY_AVAILABLE_INSIDE_CHARACTER_CLASS,
                        );
                        *pos += 1;
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
                            scan_braced_unicode_escape_value(
                                parser,
                                emit,
                                body,
                                end,
                                pos,
                                strict_mode,
                            );
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
                    // `\0` is judged the same way in every context — atom
                    // position or character class — so this arm is not
                    // gated on `atom_escape`. Mirrors tsc's per-leading-digit
                    // octal maximum (leading 0-3 allows up to 3 octal digits,
                    // leading 4-7 only 2) via the same computation as
                    // `report_invalid_template_octal_escape`.
                    b'0' => {
                        // `\0` not followed by another digit is the NUL
                        // escape, legal everywhere.
                        if *pos + 1 < end && body[*pos + 1].is_ascii_digit() {
                            let octal_end = scan_legacy_octal_digits(body, end, *pos);
                            let value = legacy_octal_value(body, *pos, octal_end);
                            let message = format_message(
                                diagnostic_messages::OCTAL_ESCAPE_SEQUENCES_ARE_NOT_ALLOWED_USE_THE_SYNTAX,
                                &[&format!("\\x{value:02x}")],
                            );
                            emit(
                                parser,
                                escape_start,
                                (octal_end - escape_start) as u32,
                                &message,
                                diagnostic_codes::OCTAL_ESCAPE_SEQUENCES_ARE_NOT_ALLOWED_USE_THE_SYNTAX,
                            );
                            *pos = octal_end;
                        } else {
                            *pos += 1;
                        }
                    }
                    b'1'..=b'9' if atom_escape => {
                        // A decimal escape outside a character class is a
                        // backreference, in every mode: tsc validates it with
                        // and without the Unicode flags, so this arm must not be
                        // gated on `strict_mode`. The span covers the digits
                        // only — the backslash is not part of it.
                        let digits_start = *pos;
                        while *pos < end && body[*pos].is_ascii_digit() {
                            *pos += 1;
                        }
                        let digits_len = (*pos - digits_start) as u32;
                        let group_number = std::str::from_utf8(&body[digits_start..*pos])
                            .ok()
                            .and_then(|digits| digits.parse::<u32>().ok())
                            // A digit run too long for `u32` cannot name any
                            // group in a pattern this scanner could reach.
                            .unwrap_or(u32::MAX);

                        if scan_ctx.capturing_group_count == 0 {
                            emit(
                                parser,
                                digits_start,
                                digits_len,
                                diagnostic_messages::THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_NO_CAPTURING,
                                diagnostic_codes::THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_NO_CAPTURING,
                            );
                        } else if group_number > scan_ctx.capturing_group_count {
                            let message = format_message(
                                diagnostic_messages::THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_ONLY_CAPTURIN,
                                &[&scan_ctx.capturing_group_count.to_string()],
                            );
                            emit(
                                parser,
                                digits_start,
                                digits_len,
                                &message,
                                diagnostic_codes::THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_ONLY_CAPTURIN,
                            );
                        }
                    }
                    // `\1`..`\9` outside a character class are backreferences,
                    // matched by the `if atom_escape` arm above; this arm only
                    // judges them inside a class, where a decimal escape is
                    // never a backreference and always a legacy octal
                    // (`\1`-`\7`) or bare decimal (`\8`/`\9`) escape instead.
                    b'1'..=b'7' if !atom_escape => {
                        let octal_end = scan_legacy_octal_digits(body, end, *pos);
                        let value = legacy_octal_value(body, *pos, octal_end);
                        let message = format_message(
                            diagnostic_messages::OCTAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS_I,
                            &[&format!("\\x{value:02x}")],
                        );
                        emit(
                            parser,
                            escape_start,
                            (octal_end - escape_start) as u32,
                            &message,
                            diagnostic_codes::OCTAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS_I,
                        );
                        *pos = octal_end;
                    }
                    b'8' | b'9' if !atom_escape => {
                        emit(
                            parser,
                            escape_start,
                            (*pos + 1 - escape_start) as u32,
                            diagnostic_messages::DECIMAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS,
                            diagnostic_codes::DECIMAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS,
                        );
                        *pos += 1;
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

            /// Scans the `{…}` body of a `\p` / `\P` Unicode property value
            /// expression, with `pos` positioned at the opening brace.
            ///
            /// Mirrors `tsc`'s regular-expression scanner: the property name
            /// and value are scanned as ASCII word characters only, the two
            /// halves are validated independently so `\p{=}` reports both
            /// TS1523 and TS1525, and the closing brace is consumed only when
            /// it is already at `pos`. Anything else is deliberately left for
            /// the surrounding walker to report as ordinary regex text, which
            /// is how `\p{ Script=Latin }` gets TS1527 followed by a TS1508 on
            /// the trailing brace.
            /// The three mode bits `scan_unicode_property_value_expression`
            /// judges by, bundled so the parameter list stays readable.
            #[derive(Clone, Copy)]
            struct PropertyExpressionMode {
                unicode: bool,
                unicode_sets: bool,
                /// `true` for `\P{...}`, whose complement is only defined over
                /// single characters — see the properties-of-strings arm.
                negated: bool,
            }

            fn scan_unicode_property_value_expression(
                parser: &mut ParserState,
                emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
                body: &[u8],
                mode: PropertyExpressionMode,
                end: usize,
                pos: &mut usize,
                escape_start: usize,
                // Set when this expression names a property of *strings*, so
                // the enclosing class can judge TS1518 for itself.
                may_contain_strings: &mut bool,
            ) {
                let PropertyExpressionMode {
                    unicode: unicode_mode,
                    unicode_sets: unicode_sets_mode,
                    negated: property_negated,
                } = mode;

                fn scan_word(body: &[u8], end: usize, pos: &mut usize) -> usize {
                    let start = *pos;
                    while *pos < end && (body[*pos] == b'_' || body[*pos].is_ascii_alphanumeric()) {
                        *pos += 1;
                    }
                    *pos - start
                }

                if !unicode_mode {
                    emit(
                        parser,
                        escape_start,
                        2,
                        diagnostic_messages::UNICODE_PROPERTY_VALUE_EXPRESSIONS_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR,
                        diagnostic_codes::UNICODE_PROPERTY_VALUE_EXPRESSIONS_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR,
                    );
                }

                *pos += 1;
                let name_start = *pos;
                let name_len = scan_word(body, end, pos);

                if *pos < end && body[*pos] == b'=' {
                    *pos += 1;
                    let value_start = *pos;
                    let value_len = scan_word(body, end, pos);
                    let canonical_name = if name_len > 0 {
                        canonical_non_binary_property_name(&body[name_start..name_start + name_len])
                    } else {
                        None
                    };
                    if name_len == 0 {
                        emit(
                            parser,
                            name_start,
                            0,
                            diagnostic_messages::EXPECTED_A_UNICODE_PROPERTY_NAME,
                            diagnostic_codes::EXPECTED_A_UNICODE_PROPERTY_NAME,
                        );
                    } else if canonical_name.is_none() {
                        emit(
                            parser,
                            name_start,
                            name_len as u32,
                            diagnostic_messages::UNKNOWN_UNICODE_PROPERTY_NAME,
                            diagnostic_codes::UNKNOWN_UNICODE_PROPERTY_NAME,
                        );
                    }
                    if value_len == 0 {
                        emit(
                            parser,
                            value_start,
                            0,
                            diagnostic_messages::EXPECTED_A_UNICODE_PROPERTY_VALUE,
                            diagnostic_codes::EXPECTED_A_UNICODE_PROPERTY_VALUE,
                        );
                    } else if let Some(canonical_name) = canonical_name
                        && !unicode_property_value_is_known(
                            canonical_name,
                            &body[value_start..value_start + value_len],
                        )
                    {
                        emit(
                            parser,
                            value_start,
                            value_len as u32,
                            diagnostic_messages::UNKNOWN_UNICODE_PROPERTY_VALUE,
                            diagnostic_codes::UNKNOWN_UNICODE_PROPERTY_VALUE,
                        );
                    }
                } else {
                    if name_len == 0 {
                        emit(
                            parser,
                            name_start,
                            0,
                            diagnostic_messages::EXPECTED_A_UNICODE_PROPERTY_NAME_OR_VALUE,
                            diagnostic_codes::EXPECTED_A_UNICODE_PROPERTY_NAME_OR_VALUE,
                        );
                    } else if BINARY_UNICODE_PROPERTIES_OF_STRINGS
                        .contains(&&body[name_start..name_start + name_len])
                    {
                        // Matches a sequence rather than a single character,
                        // so it is only accepted under the Unicode Sets (`v`)
                        // flag.
                        if !unicode_sets_mode {
                            emit(
                                parser,
                                name_start,
                                name_len as u32,
                                diagnostic_messages::ANY_UNICODE_PROPERTY_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_O,
                                diagnostic_codes::ANY_UNICODE_PROPERTY_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_O,
                            );
                        } else if property_negated {
                            // `\P{...}` complements the property, and a
                            // complement is only defined over single
                            // characters — so a property of strings is
                            // rejected outright, in or out of a negated
                            // class, and reported on the property *name*
                            // rather than on the escape. Because the
                            // operand is already an error it does not also
                            // feed the enclosing class's own TS1518 check:
                            // `/[^\P{RGI_Emoji}]/v` draws exactly one.
                            emit(
                                parser,
                                name_start,
                                name_len as u32,
                                diagnostic_messages::ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSID,
                                diagnostic_codes::ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSID,
                            );
                        } else {
                            *may_contain_strings = true;
                        }
                    } else if !is_known_unicode_property_name_or_value(
                        &body[name_start..name_start + name_len],
                    ) {
                        emit(
                            parser,
                            name_start,
                            name_len as u32,
                            diagnostic_messages::UNKNOWN_UNICODE_PROPERTY_NAME_OR_VALUE,
                            diagnostic_codes::UNKNOWN_UNICODE_PROPERTY_NAME_OR_VALUE,
                        );
                    }
                }

                if *pos < end && body[*pos] == b'}' {
                    *pos += 1;
                }
            }

            fn scan_character_class_escape(
                parser: &mut ParserState,
                emit: &impl Fn(&mut ParserState, usize, u32, &str, u32),
                body: &[u8],
                strict_mode: bool,
                unicode_sets_mode: bool,
                end: usize,
                pos: &mut usize,
                // Set when this escape denotes a set that can match something
                // other than exactly one character, which is what the
                // enclosing negated class rejects with TS1518.
                may_contain_strings: &mut bool,
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
                    b'x' => {
                        *pos += 1;
                        if *pos + 1 < body.len() && read_fixed_hex(body, *pos, 2).is_some() {
                            *pos += 2;
                            Some(ClassAtomKind::Character)
                        } else {
                            while *pos < body.len()
                                && *pos < start + 3
                                && body[*pos].is_ascii_hexdigit()
                            {
                                *pos += 1;
                            }
                            Some(ClassAtomKind::Unknown)
                        }
                    }
                    b'u' => {
                        *pos += 1;
                        if *pos < body.len() && body[*pos] == b'{' {
                            *pos += 1;
                            let mut value = 0u32;
                            let mut valid = false;
                            while *pos < body.len() && body[*pos] != b'}' {
                                if let Some(digit) = hex_byte_value(body[*pos]) {
                                    valid = true;
                                    value = (value << 4) | digit;
                                } else {
                                    valid = false;
                                }
                                *pos += 1;
                            }
                            if *pos < body.len() {
                                *pos += 1;
                            }
                            if valid && char::from_u32(value).is_some() {
                                Some(ClassAtomKind::Character)
                            } else {
                                Some(ClassAtomKind::Unknown)
                            }
                        } else if let Some(value) = read_fixed_hex(body, *pos, 4) {
                            *pos += 4;
                            if strict_mode
                                && let Some(low) = body
                                    .get(*pos..)
                                    .filter(|rest| {
                                        rest.len() >= 6 && rest[0] == b'\\' && rest[1] == b'u'
                                    })
                                    .and_then(|rest| read_fixed_hex(rest, 2, 4))
                                && decode_surrogate_pair(value, low).is_some()
                            {
                                *pos += 6;
                                return Some(ClassAtomKind::Character);
                            }
                            Some(ClassAtomKind::Character)
                        } else {
                            if strict_mode && *pos + 1 < body.len() && body[*pos] == b'\\' {
                                *pos += 2;
                            } else {
                                while *pos < body.len()
                                    && *pos < start + 5
                                    && body[*pos].is_ascii_hexdigit()
                                {
                                    *pos += 1;
                                }
                            }
                            Some(ClassAtomKind::Unknown)
                        }
                    }
                    b'q' if unicode_sets_mode => {
                        *pos += 1;
                        if *pos < body.len() && body[*pos] == b'{' {
                            // `\q{...}` denotes a set of string literals; it can
                            // match other than exactly one character as soon as
                            // any `|`-separated alternative is not one code point
                            // — including the empty alternative in `\q{}`.
                            *pos += 1;
                            // `\q{...}` denotes a set of string literals. It
                            // can match other than exactly one character as
                            // soon as any single `|`-separated alternative is
                            // not exactly one code point long — including the
                            // empty alternative in `\q{}`, which matches the
                            // empty string.
                            if scan_class_string_disjunction_body(parser, emit, body, pos) {
                                *may_contain_strings = true;
                            }
                            if *pos < body.len() {
                                *pos += 1;
                            }
                            Some(ClassAtomKind::Class)
                        } else {
                            // `ClassSetOperand` only admits `\q` as the head of
                            // `\q{ ... }`. Without the brace there is no string
                            // disjunction, so the operand degrades to the single
                            // character `q` — reported, but still a character,
                            // which is what keeps a following `-` from also
                            // drawing the class-bounded-range diagnostic.
                            emit(
                                parser,
                                start - 1,
                                2,
                                "'\\q' must be followed by string alternatives enclosed in braces.",
                                diagnostic_codes::Q_MUST_BE_FOLLOWED_BY_STRING_ALTERNATIVES_ENCLOSED_IN_BRACES,
                            );
                            Some(ClassAtomKind::Character)
                        }
                    }
                    b'p' | b'P' => {
                        let negated = body[*pos] == b'P';
                        *pos += 1;
                        if *pos < body.len() && body[*pos] == b'{' {
                            scan_unicode_property_value_expression(
                                parser,
                                emit,
                                body,
                                PropertyExpressionMode {
                                    unicode: strict_mode,
                                    unicode_sets: unicode_sets_mode,
                                    negated,
                                },
                                end,
                                pos,
                                start - 1,
                                may_contain_strings,
                            );
                            Some(ClassAtomKind::Class)
                        } else if strict_mode {
                            emit(
                                parser,
                                start - 1,
                                2,
                                if negated {
                                    "'\\P' must be followed by a Unicode property value expression enclosed in braces."
                                } else {
                                    "'\\p' must be followed by a Unicode property value expression enclosed in braces."
                                },
                                diagnostic_codes::MUST_BE_FOLLOWED_BY_A_UNICODE_PROPERTY_VALUE_EXPRESSION_ENCLOSED_IN_BRACES,
                            );
                            Some(ClassAtomKind::Class)
                        } else {
                            // Annex B: `\p` / `\P` without braces is treated as
                            // the literal character `p` / `P`. Position is
                            // already past it, so emit a Character atom
                            // directly rather than returning None and letting
                            // the caller re-scan (which would consume the next
                            // escape).
                            Some(ClassAtomKind::Character)
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
                may_contain_strings: &mut bool,
                // When false, the caller has already reported this operand's
                // position (it is a stray in a committed set-op class, drawing
                // TS1005) so the bare `ClassSetSyntaxCharacter`/reserved-double
                // reports are suppressed while consumption stays identical.
                report_bare_char_errors: bool,
            ) where
                F: Fn(&mut ParserState, usize, u32, &str, u32),
            {
                if *pos >= ctx.body_end {
                    return;
                }
                let ch = ctx.body[*pos];
                // `ClassSetExpression` (the `v` flag) nests: a `[` inside a
                // character class opens another class instead of contributing
                // a literal `[`. Without the `v` flag the nested form is not
                // grammar, so this must stay gated.
                if ctx.unicode_sets_mode && ch == b'[' {
                    *pos += 1;
                    // A nested class contributes its own strings answer to the
                    // class that encloses it: `/[^[\q{ab}]]/v` is reported on
                    // the nested `[`, not on the `\q` inside it.
                    *may_contain_strings |= scan_class_ranges(parser, ctx, pos);
                    range.push(ClassAtomKind::Class);
                    return;
                }
                // `v`-mode `ClassSetReservedDoublePunctuator`: a handful of
                // ASCII punctuators are reserved when they appear doubled
                // inside a class, so a typo like `[!!]` (meant to escape one
                // of them) is caught instead of silently matching two
                // literal characters. `&&`/`--` are excluded: those are the
                // defined class-set operators, handled by the caller before
                // `scan_class_atom` is ever reached for their first byte.
                const fn is_class_set_reserved_double_punctuator_char(ch: u8) -> bool {
                    matches!(
                        ch,
                        b'!' | b'#'
                            | b'%'
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
                if ctx.unicode_sets_mode
                    && is_class_set_reserved_double_punctuator_char(ch)
                    && *pos + 1 < ctx.body_end
                    && ctx.body[*pos + 1] == ch
                {
                    if report_bare_char_errors {
                        (ctx.emit)(
                            parser,
                            *pos,
                            2,
                            diagnostic_messages::A_CHARACTER_CLASS_MUST_NOT_CONTAIN_A_RESERVED_DOUBLE_PUNCTUATOR_DID_YOU_MEAN_TO,
                            diagnostic_codes::A_CHARACTER_CLASS_MUST_NOT_CONTAIN_A_RESERVED_DOUBLE_PUNCTUATOR_DID_YOU_MEAN_TO,
                        );
                    }
                    range.push(ClassAtomKind::Character);
                    *pos += 2;
                    return;
                }
                if ch == b'\\' {
                    *pos += 1;
                    if *pos >= ctx.body_end {
                        return;
                    }

                    match scan_character_class_escape(
                        parser,
                        ctx.emit,
                        &ctx.body[..ctx.body_end],
                        ctx.strict_mode,
                        ctx.unicode_sets_mode,
                        ctx.body_end,
                        pos,
                        may_contain_strings,
                    ) {
                        Some(atom) => range.push(atom),
                        None => {
                            let current_pos = *pos;
                            scan_character_escape(
                                parser,
                                ctx.emit,
                                &CharEscapeScanCtx {
                                    body: ctx.body,
                                    strict_mode: ctx.strict_mode,
                                    unicode_sets_mode: ctx.unicode_sets_mode,
                                    end: ctx.body_end,
                                    capturing_group_count: ctx.capturing_group_count,
                                },
                                pos,
                                false,
                                current_pos.saturating_sub(1),
                            );
                            if *pos > current_pos {
                                range.push(ClassAtomKind::Character);
                            }
                        }
                    }
                    return;
                }

                // A bare `v`-mode `ClassSetSyntaxCharacter` the surrounding
                // productions do not claim (`[`/`]`/`\` handled above, a
                // range-separator `-` by the caller): tsc reports TS1508.
                if ctx.unicode_sets_mode
                    && report_bare_char_errors
                    && let Some(symbol) = match ch {
                        b'(' => Some("("),
                        b')' => Some(")"),
                        b'{' => Some("{"),
                        b'}' => Some("}"),
                        b'/' => Some("/"),
                        b'|' => Some("|"),
                        b'-' => Some("-"),
                        _ => None,
                    }
                {
                    (ctx.emit)(
                        parser,
                        *pos,
                        1,
                        &format_message(
                            diagnostic_messages::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                            &[symbol],
                        ),
                        diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                    );
                }

                if let Some((_ch, char_len)) = next_utf8_char(ctx.body, ctx.body_end, *pos) {
                    range.push(ClassAtomKind::Character);
                    *pos += char_len;
                }

                if *pos <= ctx.body_end && *pos > 0 && ctx.body[*pos - 1] == b'-' {
                    range.push(ClassAtomKind::Unknown);
                }
            }

            /// Returns whether this class may match something other than
            /// exactly one character, for the benefit of a class that encloses
            /// it. See the TS1518 block below for why that answer is the
            /// *first* operand's rather than the union of every operand's.
            fn scan_class_ranges<F>(
                parser: &mut ParserState,
                ctx: &RegexScanContext<'_, F>,
                pos: &mut usize,
            ) -> bool
            where
                F: Fn(&mut ParserState, usize, u32, &str, u32),
            {
                fn is_class_set_operator_at(body: &[u8], pos: usize, end: usize) -> bool {
                    pos + 1 < end
                        && ((body[pos] == b'&' && body[pos + 1] == b'&')
                            || (body[pos] == b'-' && body[pos + 1] == b'-'))
                }

                /// Which `ClassSetExpression` production a class has committed
                /// to. A plain range or a bare `-` is a union; the two
                /// class-set operators are their own kinds. Mixing any two of
                /// these in one class is TS1519.
                #[derive(Clone, Copy, PartialEq, Eq)]
                enum ClassSetKind {
                    Union,
                    Subtraction,
                    Intersection,
                }

                /// Classify the operator at `pos`, which the caller has already
                /// confirmed with `is_class_set_operator_at`.
                fn class_set_operator_kind(body: &[u8], pos: usize) -> ClassSetKind {
                    if body[pos] == b'-' {
                        ClassSetKind::Subtraction
                    } else {
                        ClassSetKind::Intersection
                    }
                }

                /// Record `kind` as this class's operator, reporting TS1519 on
                /// the first operator that disagrees with the one already
                /// committed. The committed kind is deliberately left unchanged
                /// on a mismatch so the report stays keyed to the class's
                /// original production.
                fn note_class_set_kind<F>(
                    parser: &mut ParserState,
                    ctx: &RegexScanContext<'_, F>,
                    committed: &mut Option<ClassSetKind>,
                    mixed_reported: &mut bool,
                    kind: ClassSetKind,
                    at: usize,
                    len: u32,
                ) where
                    F: Fn(&mut ParserState, usize, u32, &str, u32),
                {
                    match *committed {
                        None => *committed = Some(kind),
                        Some(existing) if existing != kind && !*mixed_reported => {
                            *mixed_reported = true;
                            (ctx.emit)(
                                parser,
                                at,
                                len,
                                diagnostic_messages::OPERATORS_MUST_NOT_BE_MIXED_WITHIN_A_CHARACTER_CLASS_WRAP_IT_IN_A_NESTED_CLASS_I,
                                diagnostic_codes::OPERATORS_MUST_NOT_BE_MIXED_WITHIN_A_CHARACTER_CLASS_WRAP_IT_IN_A_NESTED_CLASS_I,
                            );
                        }
                        Some(_) => {}
                    }
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

                /// Drain a class already committed to `&&`/`--` once the caller
                /// has met content that is neither that operator nor `]`.
                ///
                /// A `ClassIntersection`/`ClassSubtraction` admits, after each
                /// operand, only more of its own operator or the closing `]`.
                /// tsc reports `TS1005 '&&'/'--' expected.` for each stray
                /// union operand until `]`, re-syncing to a later valid operator
                /// (so `/[a&&b c&&d]/v` reports only the two strays). A lone `&`
                /// is consumed as a malformed `&&` and draws `TS1508` instead; a
                /// bare `-` or the *other* operator is mixing and draws `TS1519`
                /// through the shared `note_class_set_kind`. Stray operands are
                /// consumed with `report_bare_char_errors = false` so a bare
                /// syntax character or reserved double punctuator does not add
                /// its own report on top of the `TS1005`.
                fn drain_committed_set_op_tail<F>(
                    parser: &mut ParserState,
                    ctx: &RegexScanContext<'_, F>,
                    committed: &mut Option<ClassSetKind>,
                    mixed_reported: &mut bool,
                    pos: &mut usize,
                ) where
                    F: Fn(&mut ParserState, usize, u32, &str, u32),
                {
                    let expected = if *committed == Some(ClassSetKind::Subtraction) {
                        "--"
                    } else {
                        "&&"
                    };
                    // Reused scratch for the operands the drain consumes but does
                    // not inspect; `clear()` keeps the one allocation across the
                    // recovery instead of re-minting it per stray operand, and the
                    // strings flag is write-only garbage here.
                    let mut sink = Vec::new();
                    let mut sink_strings = false;
                    while *pos < ctx.body_end && ctx.body[*pos] != b']' {
                        sink.clear();
                        if is_class_set_operator_at(ctx.body, *pos, ctx.body_end) {
                            note_class_set_kind(
                                parser,
                                ctx,
                                committed,
                                mixed_reported,
                                class_set_operator_kind(ctx.body, *pos),
                                *pos,
                                2,
                            );
                            scan_class_set_operator(parser, ctx.emit, ctx.body, ctx.body_end, pos);
                        } else if ctx.body[*pos] == b'&' {
                            (ctx.emit)(
                                parser,
                                *pos,
                                1,
                                &format_message(
                                    diagnostic_messages::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                                    &["&"],
                                ),
                                diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                            );
                            *pos += 1;
                        } else if ctx.body[*pos] == b'-' {
                            note_class_set_kind(
                                parser,
                                ctx,
                                committed,
                                mixed_reported,
                                ClassSetKind::Union,
                                *pos,
                                1,
                            );
                            *pos += 1;
                        } else {
                            (ctx.emit)(
                                parser,
                                *pos,
                                1,
                                &format_message(diagnostic_messages::EXPECTED, &[expected]),
                                diagnostic_codes::EXPECTED,
                            );
                            scan_class_atom(parser, ctx, pos, &mut sink, &mut sink_strings, false);
                            continue;
                        }
                        // After an operator (or the malformed `&`/mixing `-`)
                        // a right operand is expected; consume one when present.
                        if *pos < ctx.body_end && ctx.body[*pos] != b']' {
                            scan_class_atom(parser, ctx, pos, &mut sink, &mut sink_strings, true);
                        }
                    }
                    if *pos < ctx.body_end {
                        *pos += 1; // consume the closing `]`
                    }
                }

                // Consume optional leading ^
                let negated = *pos < ctx.body_end && ctx.body[*pos] == b'^';
                if negated {
                    *pos += 1;
                }

                // A negated class complements its contents, and a complement
                // is only defined over single characters, so an operand that
                // may match a string is TS1518.
                //
                // In a UNION, tsc consults only the class's FIRST operand:
                // `/[^\q{xy}b]/v` is reported and `/[^b\q{xy}]/v` is not, and
                // likewise `/[^\p{RGI_Emoji}a]/v` against
                // `/[^a\p{RGI_Emoji}]/v`. The ECMAScript grammar makes
                // `MayContainStrings` of a union true when *any* operand may
                // contain strings, so the second member of each pair is a tsc
                // miss rather than intended behaviour — filed upstream, and
                // matched here deliberately, because parity with tsc is the
                // contract.
                //
                // INTERSECTION is not subject to that miss and is spec-exact
                // in tsc: `A&&B` may contain strings only when *every* operand
                // does, so `/[^\q{ab}&&\q{a}]/v` is clean while
                // `/[^\q{ab}&&\q{cd}]/v` is not. SUBTRACTION takes its first
                // operand's answer, which the union rule already gives.
                //
                // Because an intersection's verdict is not known until its
                // last operand, the report is deferred to the end of the class
                // and anchored to the first operand's start — which is where
                // tsc points for every shape above.
                let mut class_may_contain_strings = false;
                let mut first_operand_start = None;
                let mut operand_index = 0usize;

                // The first operator a class uses fixes its kind, and mixing a
                // different one in the same class is TS1519. The commitment is
                // per class, not per pattern: a nested class recurses into its
                // own `scan_class_ranges` and so gets its own `committed`,
                // which is why `/[[a--b]&&c]/v` is legal.
                let mut committed: Option<ClassSetKind> = None;
                // tsc reports the mixture once, on the first operator that
                // disagrees — `/[a--b&&c--d]/v` draws a single TS1519.
                let mut mixed_reported = false;

                while *pos < ctx.body_end {
                    if ctx.body[*pos] == b']' {
                        *pos += 1;
                        break;
                    }
                    if ctx.unicode_sets_mode
                        && is_class_set_operator_at(ctx.body, *pos, ctx.body_end)
                    {
                        // An operator reaching the top of the loop with no
                        // operand scanned and no kind committed yet opens the
                        // class (or follows `^`), so its *left* operand is
                        // missing — TS1520 at the operator's first character.
                        // `committed.is_none()` fires it once per class, so a
                        // leading operator run (`/[&&&&]/v`) still draws a single
                        // leading report; the missing *right* operand is a
                        // separate report from `scan_class_set_operator` below,
                        // which is why `/[&&]/v` draws two. (See the
                        // `regex_class_set_operand_missing_tests` matrix.)
                        if operand_index == 0 && committed.is_none() {
                            (ctx.emit)(
                                parser,
                                *pos,
                                1,
                                diagnostic_messages::EXPECTED_A_CLASS_SET_OPERAND,
                                diagnostic_codes::EXPECTED_A_CLASS_SET_OPERAND,
                            );
                        }
                        note_class_set_kind(
                            parser,
                            ctx,
                            &mut committed,
                            &mut mixed_reported,
                            class_set_operator_kind(ctx.body, *pos),
                            *pos,
                            2,
                        );
                        scan_class_set_operator(parser, ctx.emit, ctx.body, ctx.body_end, pos);
                        continue;
                    }

                    // A fresh-atom `-` reaches `scan_class_atom` for its TS1508;
                    // a range-separator `-` is consumed below and stays legal.
                    let mut atoms = Vec::new();
                    let min_start = *pos;
                    let mut atom_may_contain_strings = false;
                    scan_class_atom(
                        parser,
                        ctx,
                        pos,
                        &mut atoms,
                        &mut atom_may_contain_strings,
                        true,
                    );
                    if operand_index == 0 {
                        class_may_contain_strings = atom_may_contain_strings;
                        first_operand_start = Some(min_start);
                    } else if committed == Some(ClassSetKind::Intersection) {
                        class_may_contain_strings &= atom_may_contain_strings;
                    }
                    operand_index += 1;
                    if ctx.unicode_sets_mode
                        && is_class_set_operator_at(ctx.body, *pos, ctx.body_end)
                    {
                        note_class_set_kind(
                            parser,
                            ctx,
                            &mut committed,
                            &mut mixed_reported,
                            class_set_operator_kind(ctx.body, *pos),
                            *pos,
                            2,
                        );
                        scan_class_set_operator(parser, ctx.emit, ctx.body, ctx.body_end, pos);
                        continue;
                    }
                    if *pos >= ctx.body_end || ctx.body[*pos] != b'-' {
                        // A class committed to `&&`/`--` admits only more of that
                        // operator (handled above) or `]` after each operand;
                        // any other content is stray union material that tsc
                        // rejects with `TS1005 '&&'/'--' expected.`, so hand the
                        // rest of the class to the set-op tail drainer instead of
                        // looping back to scan it as a fresh union member.
                        if ctx.unicode_sets_mode
                            && matches!(
                                committed,
                                Some(ClassSetKind::Subtraction | ClassSetKind::Intersection)
                            )
                            && *pos < ctx.body_end
                            && ctx.body[*pos] != b']'
                        {
                            drain_committed_set_op_tail(
                                parser,
                                ctx,
                                &mut committed,
                                &mut mixed_reported,
                                pos,
                            );
                            break;
                        }
                        continue;
                    }
                    // A class already committed to `--`/`&&` admits no ranges,
                    // so this `-` is a union operator mixed into a set
                    // expression rather than a range separator.
                    if matches!(
                        committed,
                        Some(ClassSetKind::Subtraction | ClassSetKind::Intersection)
                    ) {
                        note_class_set_kind(
                            parser,
                            ctx,
                            &mut committed,
                            &mut mixed_reported,
                            ClassSetKind::Union,
                            *pos,
                            1,
                        );
                        *pos += 1;
                        continue;
                    }
                    committed = Some(ClassSetKind::Union);

                    *pos += 1;

                    if *pos < ctx.body_end && ctx.body[*pos] == b']' {
                        *pos += 1;
                        break;
                    }

                    let max_start = *pos;
                    let mut max_atoms = Vec::new();
                    // The upper bound of a range is not an operand of the
                    // class, so its strings answer is not the class's; a
                    // non-single-character bound is already TS1517's business.
                    let mut max_atom_may_contain_strings = false;
                    scan_class_atom(
                        parser,
                        ctx,
                        pos,
                        &mut max_atoms,
                        &mut max_atom_may_contain_strings,
                        true,
                    );

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

                    // TS1517 range-order diagnostics are emitted by
                    // `regex_range_order_errors`, which handles escaped atoms
                    // and surrogate pairs consistently. This scanner still
                    // validates class-boundary rules above.
                }

                if let Some(report_at) = first_operand_start
                    && ctx.unicode_sets_mode
                    && negated
                    && class_may_contain_strings
                {
                    (ctx.emit)(
                        parser,
                        report_at,
                        1,
                        diagnostic_messages::ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSID,
                        diagnostic_codes::ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSID,
                    );
                }

                class_may_contain_strings
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
                                    scan_group_name_and_delimiter(
                                        parser, ctx, pos, /*is_reference*/ true,
                                    );
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
                                    &CharEscapeScanCtx {
                                        body: ctx.body,
                                        strict_mode: ctx.strict_mode,
                                        unicode_sets_mode: ctx.unicode_sets_mode,
                                        end: ctx.body_end,
                                        capturing_group_count: ctx.capturing_group_count,
                                    },
                                    pos,
                                    true,
                                    escape_start,
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
                                // tsc reads the character after `?` through
                                // `charCodeChecked`, so end-of-body is not a
                                // termination condition here: `/(?/` still
                                // enters the modifier-group arm and reports the
                                // missing `:` where the body ran out.
                                match ctx.body.get(*pos).copied().unwrap_or(b'\0') {
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
                                            scan_group_name_and_delimiter(
                                                parser, ctx, pos, /*is_reference*/ false,
                                            );
                                            is_previous_term_quantifiable = true;
                                        }
                                        scan_disjunction(parser, ctx, pos, true);
                                    }
                                    // Modifier group, including the degenerate
                                    // `(?:` form: tsc reaches both through this
                                    // same `default` arm and never backtracks
                                    // out of it.
                                    _ => {
                                        scan_modifier_group_prelude(
                                            parser,
                                            ctx.emit,
                                            ctx.body,
                                            ctx.body_end,
                                            pos,
                                        );
                                        is_previous_term_quantifiable = true;
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
                            // A top-level class has no enclosing class to
                            // answer to; it has already reported TS1518 for
                            // itself if it needed to.
                            let _ = scan_class_ranges(parser, ctx, pos);
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
                    // tsc brackets every alternative with a fresh capturing-group
                    // name scope, so sibling alternatives are mutually exclusive
                    // while enclosing alternatives stay visible.
                    ctx.group_names.borrow_mut().enter_alternative();
                    scan_alternative(parser, ctx, pos, in_group);
                    ctx.group_names.borrow_mut().leave_alternative();

                    if *pos >= ctx.body_end || ctx.body[*pos] != b'|' {
                        return;
                    }

                    *pos += 1;
                }
            }

            let group_names = RefCell::new(regex_group_names::GroupNameScopes::new());
            let ctx = RegexScanContext {
                emit: &emit,
                body: bytes,
                body_end,
                strict_mode,
                unicode_sets_mode,
                capturing_group_count: count_capturing_groups(bytes, body_end),
                group_names: &group_names,
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
                    if bytes[i] == b'\\' {
                        // `\X` is one escape atom: the backslash escapes the
                        // next byte. Skip both so an escaped backslash `\\`
                        // cannot leave its second `\` to seed a phantom `\u{`
                        // extended escape (e.g. `\\u{abc}` is a literal
                        // backslash followed by literal `u{abc}`).
                        i += 2;
                        continue;
                    }
                    i += 1;
                }
                errors
            })
            .unwrap_or_default();
        let range_order_errors = regex_body_end(&raw_text)
            .map(|body_end| range_order::regex_range_order_errors(&raw_text, body_end))
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
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            for error in flag_errors {
                let (message, code) = match error.kind {
                    tsz_scanner::scanner_impl::RegexFlagErrorKind::Duplicate => (
                        diagnostic_messages::DUPLICATE_REGULAR_EXPRESSION_FLAG,
                        diagnostic_codes::DUPLICATE_REGULAR_EXPRESSION_FLAG,
                    ),
                    tsz_scanner::scanner_impl::RegexFlagErrorKind::InvalidFlag => (
                        diagnostic_messages::UNKNOWN_REGULAR_EXPRESSION_FLAG,
                        diagnostic_codes::UNKNOWN_REGULAR_EXPRESSION_FLAG,
                    ),
                    tsz_scanner::scanner_impl::RegexFlagErrorKind::IncompatibleFlags => (
                        diagnostic_messages::THE_UNICODE_U_FLAG_AND_THE_UNICODE_SETS_V_FLAG_CANNOT_BE_SET_SIMULTANEOUSLY,
                        diagnostic_codes::THE_UNICODE_U_FLAG_AND_THE_UNICODE_SETS_V_FLAG_CANNOT_BE_SET_SIMULTANEOUSLY,
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
}

mod closing_token;

#[cfg(test)]
mod tests;
