//! Byte-level source-text scanning primitives.
//!
//! Phases that scan raw source or display text at the byte level all need the
//! same handful of leaf operations: "is this byte/char an ASCII identifier
//! char", "skip past a quoted literal", "is this needle a standalone
//! (whole-word) token", and "skip ASCII whitespace". This module is the single
//! source of truth for those primitives so the emitter, CLI, parser, binder,
//! checker, and LSP stop re-deriving (and silently drifting) their own copies.
//!
//! The identifier predicates are the ASCII fast path mirroring
//! `tsz_scanner`'s Unicode `is_ecmascript_identifier_*`: an identifier *start*
//! is `_`, `$`, or an ASCII letter; an identifier *continue* additionally
//! admits ASCII digits. Callers that need full Unicode identifier semantics
//! must use the scanner helpers; these cover the ASCII-only byte/char scans
//! that walk already-emitted text or display strings.
//!
//! # Char-boundary-safe text windowing
//!
//! Several phases scan only the leading portion of a source file — JSX pragma
//! detection (`@jsx`, `@jsxImportSource`, `@jsxRuntime`, `@jsxFrag`) caps the
//! scan at a fixed byte budget so it does not walk megabytes of source for a
//! pragma that, by spec, can only appear in a leading comment.
//!
//! The naive form of that cap is a raw byte slice:
//!
//! ```ignore
//! let scan_limit = text.len().min(4096);
//! let scan_text = &text[..scan_limit];   // panics: byte 4096 may be mid-char
//! ```
//!
//! Slicing a `&str` panics when the end index does not fall on a UTF-8 char
//! boundary. Any file at least `max_bytes` long whose `max_bytes`-th byte sits
//! inside a multi-byte codepoint (common in files with non-ASCII license
//! headers, locale tables, or translations near the top) crashes the worker.
//! That is a `tsc`-parity failure: `tsc` never panics on such input.
//!
//! [`leading_window`] is the single source of truth for that capped-prefix
//! pattern. It floors the requested cap down to the nearest char boundary using
//! stable `str` APIs, so the resulting slice is always valid while never
//! extending the window past the cap (a pragma a few bytes beyond the budget is
//! out of scope anyway).

/// Byte budget for leading JSX-pragma comment scans.
///
/// Pragmas such as `@jsx`, `@jsxImportSource`, `@jsxRuntime`, and `@jsxFrag`
/// may only appear in a leading comment, so every pragma scanner caps its
/// search at this many bytes rather than walking the whole file. Centralized
/// here so the policy lives in one place and is always paired with
/// [`leading_window`], which keeps the cap on a char boundary.
pub const JSX_PRAGMA_SCAN_BYTES: usize = 4096;

/// Borrow the leading prefix of `text`, capped at `max_bytes` and floored to
/// the nearest UTF-8 character boundary so the slice can never panic.
///
/// - When `text` is shorter than `max_bytes`, the whole string is returned.
/// - When `max_bytes` lands inside a multi-byte codepoint, the window shrinks
///   to the start of that codepoint, so no partial codepoint is ever included.
///
/// The returned slice is always a valid `&str` and its length is always
/// `<= max_bytes`.
///
/// # Examples
///
/// ```
/// use tsz_common::text_scan::leading_window;
///
/// // Pure ASCII: capped exactly at the byte budget.
/// assert_eq!(leading_window("abcdef", 3), "abc");
///
/// // Budget shorter than the string but landing mid-codepoint: the
/// // two-byte 'Н' (U+041D) straddling bytes 1..3 is excluded rather than
/// // sliced in half.
/// assert_eq!(leading_window("aНb", 2), "a");
///
/// // Budget larger than the string: the whole string comes back.
/// assert_eq!(leading_window("aНb", 99), "aНb");
/// ```
#[inline]
#[must_use]
pub fn leading_window(text: &str, max_bytes: usize) -> &str {
    let mut limit = max_bytes.min(text.len());
    while !text.is_char_boundary(limit) {
        limit -= 1;
    }
    &text[..limit]
}

/// True when `b` can begin an ASCII identifier: `_`, `$`, or an ASCII letter.
///
/// The ASCII fast path for `tsz_scanner`'s Unicode `is_ecmascript_identifier_start`.
#[inline]
#[must_use]
pub const fn is_ascii_identifier_start(b: u8) -> bool {
    b == b'_' || b == b'$' || b.is_ascii_alphabetic()
}

/// True when `b` can continue an ASCII identifier: an identifier-start byte or
/// an ASCII digit.
///
/// The ASCII fast path for `tsz_scanner`'s Unicode `is_ecmascript_identifier_part`.
#[inline]
#[must_use]
pub const fn is_ascii_identifier_continue(b: u8) -> bool {
    is_ascii_identifier_start(b) || b.is_ascii_digit()
}

/// `char` variant of [`is_ascii_identifier_start`]: `_`, `$`, or an ASCII letter.
///
/// Non-ASCII characters return `false`; callers that need Unicode identifier
/// semantics must use the scanner helpers instead.
#[inline]
#[must_use]
pub const fn is_ascii_identifier_start_char(c: char) -> bool {
    c == '_' || c == '$' || c.is_ascii_alphabetic()
}

/// `char` variant of [`is_ascii_identifier_continue`]: an identifier-start char
/// or an ASCII digit.
#[inline]
#[must_use]
pub const fn is_ascii_identifier_continue_char(c: char) -> bool {
    is_ascii_identifier_start_char(c) || c.is_ascii_digit()
}

/// Advance past a quoted literal that opens at `start` (the opening `quote`
/// byte) and return the index just after its closing quote.
///
/// Policy (one source of truth for every byte-level quoted-literal skip):
/// - A backslash escapes the next byte (the escape pair is consumed, clamped at
///   end-of-input).
/// - A matching `quote` byte closes the literal; the returned index points just
///   past it.
/// - A raw line terminator (`\n`/`\r`) terminates a single-line string literal
///   (`'`/`"`): the literal is treated as closed and the returned index points
///   *at* the terminator, so the caller resumes scanning there. This matches
///   the TS grammar (single-line strings cannot contain raw newlines) and lets
///   an unterminated string fail gracefully instead of consuming to EOF.
/// - Template literals (`` ` ``) may span newlines, so raw line terminators do
///   not terminate them.
///
/// When the literal is unterminated, the input length is returned.
#[inline]
#[must_use]
pub fn skip_quoted_literal(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut pos = start + 1;
    while pos < bytes.len() {
        match bytes[pos] {
            b'\\' => pos = (pos + 2).min(bytes.len()),
            b if b == quote => return pos + 1,
            b'\n' | b'\r' if quote != b'`' => return pos,
            _ => pos += 1,
        }
    }
    pos
}

/// Skip ASCII inline/line-terminator whitespace (`' '`, `'\t'`, `'\r'`, `'\n'`)
/// starting at `from`, returning the index of the first non-whitespace byte (or
/// `bytes.len()`).
///
/// This is the explicit, single whitespace set for byte-level source-text
/// scans; it deliberately excludes form-feed/vertical-tab so every scanner
/// agrees on what "skip whitespace" means.
#[inline]
#[must_use]
pub fn skip_ascii_whitespace(bytes: &[u8], from: usize) -> usize {
    let mut pos = from;
    while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t' | b'\r' | b'\n') {
        pos += 1;
    }
    pos
}

/// If a `//` line comment or a `/* */` block comment opens at `from`, return
/// the index just past it; otherwise return `None`.
///
/// Policy (one source of truth for every byte-level comment skip):
/// - A line comment (`//`) runs to — but not past — the next line terminator,
///   so the returned index points *at* the `\n`/`\r` (the caller then consumes
///   it as whitespace). At end of input the returned index is `bytes.len()`.
/// - A block comment (`/* */`) runs through its closing `*/`; the returned
///   index points just past the `/`. An unterminated block comment runs to end
///   of input (`bytes.len()`), matching a scanner that consumes the rest of the
///   file as comment trivia.
/// - A lone `/` that is not followed by `/` or `*` is not a comment: `None`.
///
/// This is a *leading-trivia* primitive: it never inspects string-literal
/// contents, so callers must not invoke it while positioned inside a string
/// (a `//` inside `"http://..."` is only a comment if the scanner is not first
/// skipping the enclosing string via [`skip_quoted_literal`]).
#[inline]
#[must_use]
pub fn skip_comment(bytes: &[u8], from: usize) -> Option<usize> {
    if bytes.get(from) != Some(&b'/') {
        return None;
    }
    match bytes.get(from + 1) {
        Some(b'/') => {
            let mut pos = from + 2;
            while pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                pos += 1;
            }
            Some(pos)
        }
        Some(b'*') => {
            let mut pos = from + 2;
            while pos < bytes.len() {
                if bytes[pos] == b'*' && bytes.get(pos + 1) == Some(&b'/') {
                    return Some(pos + 2);
                }
                pos += 1;
            }
            Some(bytes.len())
        }
        _ => None,
    }
}

/// Skip any run of ASCII whitespace and `//` / `/* */` comment trivia starting
/// at `from`, returning the index of the first byte that is neither. Whitespace
/// is [`skip_ascii_whitespace`]'s set; comments are [`skip_comment`]'s two
/// forms. Interleaved whitespace and comments in any order are all consumed.
///
/// Like the primitives it composes, this never recognizes a comment inside a
/// string literal — it is a leading-trivia skip, so callers must not invoke it
/// while positioned inside a string.
#[inline]
#[must_use]
pub fn skip_trivia(bytes: &[u8], from: usize) -> usize {
    let mut pos = from;
    loop {
        let after_ws = skip_ascii_whitespace(bytes, pos);
        match skip_comment(bytes, after_ws) {
            Some(next) => pos = next,
            None => return after_ws,
        }
    }
}

/// Return the byte offset of the first occurrence of `needle` in `haystack`
/// that appears as a standalone (whole-word) ASCII identifier token — i.e. not
/// flanked by identifier-continue bytes on either side.
///
/// Returns `None` when `needle` is empty or never appears as a whole word.
#[inline]
#[must_use]
pub fn find_standalone_token(haystack: &str, needle: &str) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() || needle_bytes.len() > bytes.len() {
        return None;
    }
    let mut i = 0;
    while i + needle_bytes.len() <= bytes.len() {
        if &bytes[i..i + needle_bytes.len()] == needle_bytes {
            let prev_ok = i == 0 || !is_ascii_identifier_continue(bytes[i - 1]);
            let next_ok = i + needle_bytes.len() == bytes.len()
                || !is_ascii_identifier_continue(bytes[i + needle_bytes.len()]);
            if prev_ok && next_ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// True when `haystack` contains `needle` as a standalone (whole-word) ASCII
/// identifier token. See [`find_standalone_token`].
#[inline]
#[must_use]
pub fn contains_standalone_token(haystack: &str, needle: &str) -> bool {
    find_standalone_token(haystack, needle).is_some()
}

#[cfg(test)]
mod tests {
    use super::{
        contains_standalone_token, find_standalone_token, is_ascii_identifier_continue,
        is_ascii_identifier_continue_char, is_ascii_identifier_start,
        is_ascii_identifier_start_char, leading_window, skip_ascii_whitespace, skip_comment,
        skip_quoted_literal, skip_trivia,
    };

    #[test]
    fn identifier_start_admits_underscore_dollar_and_letters_only() {
        for b in *b"_$aZ" {
            assert!(is_ascii_identifier_start(b));
        }
        for b in *b"09 .-" {
            assert!(!is_ascii_identifier_start(b));
        }
        // Non-ASCII bytes are never identifier-start under the ASCII fast path.
        assert!(!is_ascii_identifier_start(0xC3));
    }

    #[test]
    fn identifier_continue_additionally_admits_digits() {
        for b in *b"_$aZ09" {
            assert!(is_ascii_identifier_continue(b));
        }
        for b in *b" .-(" {
            assert!(!is_ascii_identifier_continue(b));
        }
    }

    #[test]
    fn char_variants_match_byte_variants_for_ascii() {
        for c in ['_', '$', 'a', 'Z', '0', '9', ' ', '.', '-'] {
            assert_eq!(
                is_ascii_identifier_start_char(c),
                is_ascii_identifier_start(c as u8)
            );
            assert_eq!(
                is_ascii_identifier_continue_char(c),
                is_ascii_identifier_continue(c as u8)
            );
        }
        // Non-ASCII chars are rejected by the ASCII fast path.
        assert!(!is_ascii_identifier_start_char('é'));
        assert!(!is_ascii_identifier_continue_char('é'));
    }

    #[test]
    fn skip_quoted_basic_string_returns_past_close() {
        let s = b"'abc' rest";
        // Opens at 0, closes at index 4; returns 5 (the space).
        assert_eq!(skip_quoted_literal(s, 0, b'\''), 5);
    }

    #[test]
    fn skip_quoted_honors_backslash_escape() {
        let s = br#""a\"b" rest"#;
        // The escaped quote does not close the literal; real close is index 5.
        assert_eq!(skip_quoted_literal(s, 0, b'"'), 6);
    }

    #[test]
    fn skip_quoted_trailing_backslash_at_eof_clamps() {
        let s = b"'ab\\";
        assert_eq!(skip_quoted_literal(s, 0, b'\''), s.len());
    }

    #[test]
    fn skip_quoted_single_line_terminates_on_raw_newline() {
        let s = b"'unterminated\nnext";
        // Returns the index *at* the newline (13), not EOF.
        assert_eq!(skip_quoted_literal(s, 0, b'\''), 13);
        assert_eq!(s[13], b'\n');
    }

    #[test]
    fn skip_quoted_template_spans_newlines() {
        let s = b"`line1\nline2` rest";
        // Backtick literals are not terminated by raw newlines.
        let end = skip_quoted_literal(s, 0, b'`');
        assert_eq!(s[end - 1], b'`');
        assert_eq!(end, 13);
    }

    #[test]
    fn skip_whitespace_skips_inline_and_line_terminators_only() {
        let s = b" \t\r\nx";
        assert_eq!(skip_ascii_whitespace(s, 0), 4);
        // Form-feed (0x0C) is deliberately not part of the set.
        let ff = b"\x0Cx";
        assert_eq!(skip_ascii_whitespace(ff, 0), 0);
        // From past the end is a no-op clamp.
        assert_eq!(skip_ascii_whitespace(s, s.len()), s.len());
    }

    #[test]
    fn standalone_token_requires_word_boundaries() {
        assert!(contains_standalone_token("a + react + b", "react"));
        assert!(!contains_standalone_token("react_2 = 1", "react"));
        assert!(!contains_standalone_token("preact", "react"));
        assert_eq!(find_standalone_token("x react y", "react"), Some(2));
        // Token at the very start and end of input.
        assert!(contains_standalone_token("react", "react"));
        // Empty needle never matches.
        assert!(!contains_standalone_token("anything", ""));
    }

    #[test]
    fn skip_comment_line_stops_at_line_terminator() {
        let s = b"// comment\nrest";
        // Returns the index *at* the newline (10), not past it.
        assert_eq!(skip_comment(s, 0), Some(10));
        assert_eq!(s[10], b'\n');
    }

    #[test]
    fn skip_comment_line_runs_to_eof_when_unterminated() {
        let s = b"// trailing to end";
        assert_eq!(skip_comment(s, 0), Some(s.len()));
    }

    #[test]
    fn skip_comment_block_returns_past_close() {
        let s = b"/* c */rest";
        // Closing `*/` ends at index 7; returns 7 (the `r`).
        assert_eq!(skip_comment(s, 0), Some(7));
        assert_eq!(&s[7..], b"rest");
    }

    #[test]
    fn skip_comment_block_unterminated_runs_to_eof() {
        let s = b"/* never closed";
        assert_eq!(skip_comment(s, 0), Some(s.len()));
    }

    #[test]
    fn skip_comment_rejects_non_comment() {
        // Lone slash (division/regex), not a comment.
        assert_eq!(skip_comment(b"/ x", 0), None);
        // A trailing slash at EOF has no second byte.
        assert_eq!(skip_comment(b"/", 0), None);
        // Not positioned on a slash at all.
        assert_eq!(skip_comment(b"abc", 0), None);
    }

    #[test]
    fn skip_trivia_consumes_interleaved_whitespace_and_comments() {
        // Whitespace, a line comment, more whitespace, a block comment, then
        // the first real token `x` at the end.
        let s = b"  // a\n\t/* b */ x";
        let pos = skip_trivia(s, 0);
        assert_eq!(s[pos], b'x');
        assert_eq!(pos, s.len() - 1);
    }

    #[test]
    fn skip_trivia_is_a_noop_on_a_real_token() {
        // A lone `/` is not trivia, so the cursor does not advance.
        assert_eq!(skip_trivia(b"/ rest", 0), 0);
        assert_eq!(skip_trivia(b"xyz", 0), 0);
    }

    #[test]
    fn skip_trivia_runs_to_end_on_all_trivia() {
        let s = b"  /* only */ // comment\n  ";
        assert_eq!(skip_trivia(s, 0), s.len());
    }

    #[test]
    fn ascii_caps_exactly_at_budget() {
        assert_eq!(leading_window("hello world", 5), "hello");
    }

    #[test]
    fn budget_at_or_past_len_returns_whole_string() {
        assert_eq!(leading_window("hi", 2), "hi");
        assert_eq!(leading_window("hi", 100), "hi");
    }

    #[test]
    fn zero_budget_returns_empty() {
        assert_eq!(leading_window("hello", 0), "");
    }

    #[test]
    fn empty_input_is_empty_for_any_budget() {
        assert_eq!(leading_window("", 0), "");
        assert_eq!(leading_window("", 4096), "");
    }

    #[test]
    fn budget_mid_two_byte_codepoint_floors_back() {
        // 'Н' (U+041D) occupies bytes 1..3.
        let s = "aНb";
        assert_eq!(s.len(), 4);
        // Cap inside the codepoint -> floor to its start.
        assert_eq!(leading_window(s, 2), "a");
        // Cap at the codepoint end -> include it.
        assert_eq!(leading_window(s, 3), "aН");
    }

    #[test]
    fn budget_mid_four_byte_codepoint_floors_back() {
        // '😀' (U+1F600) occupies 4 bytes.
        let s = "x😀y";
        assert_eq!(s.len(), 6);
        assert_eq!(leading_window(s, 1), "x");
        assert_eq!(leading_window(s, 2), "x");
        assert_eq!(leading_window(s, 3), "x");
        assert_eq!(leading_window(s, 4), "x");
        assert_eq!(leading_window(s, 5), "x😀");
    }

    #[test]
    fn reproduces_issue_window_at_4096() {
        // Mirror the original panic: a two-byte codepoint straddling the
        // 4096-byte cap. Comment-style ASCII padding fills the run-up.
        let mut s = "/".repeat(2) + &" ".repeat(4095 - 2);
        s.push('Н'); // 2-byte 'Н' lands across bytes 4095..4097
        s.push_str(" x\nexport const x = 1;\n");
        // The cap (4096) lands inside 'Н'; must not panic and must floor back.
        let window = leading_window(&s, 4096);
        assert_eq!(window.len(), 4095);
        assert!(window.is_char_boundary(window.len()));
    }
}
