//! Parser-owned service for locating incomplete call contexts in source text.
//!
//! When the LSP cursor sits inside an unclosed `(...)` or `<...>` that the
//! parser has not yet matched (because the file is being edited), signature
//! help falls back to backward byte-scanning to find the open delimiter and
//! determine the active argument index.
//!
//! Centralising this logic here means the scanning algorithm has unit tests
//! independent of the LSP layer, and future improvements (e.g. full
//! string/comment awareness) are made in one place.

/// Which delimiter opened the incomplete call context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallDelimiter {
    /// An opening `(` — regular call or `new` expression.
    Paren,
    AngleBracket,
}

/// Structured result of a backward-scan call-trigger search.
///
/// This is returned by [`find_incomplete_paren_call`] and
/// [`find_incomplete_angle_call`] when a matching open delimiter is found
/// before the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompleteCallContext {
    /// The identifier text immediately preceding the open delimiter.
    pub callee_name: String,
    /// Byte offset one past the last byte of the callee name.
    /// This is the end of the identifier itself, not the delimiter position.
    pub callee_end_offset: usize,
    /// Which delimiter opened this call context.
    pub delimiter: CallDelimiter,
    /// Number of top-level commas between the delimiter and the cursor.
    /// Used as the zero-based active-parameter index.
    pub active_parameter: u32,
    /// Byte offset of the first character inside the delimiter.
    pub span_start: usize,
    /// Byte length of the span from `span_start` to the cursor.
    pub span_length: usize,
    /// `true` when the callee is preceded by the `new` keyword.
    pub is_new_expression: bool,
}

/// Declaration keywords that, when they immediately precede an identifier,
/// indicate the identifier is being *declared*, not called.
const DECLARATION_KEYWORDS: [&str; 7] = [
    "function",
    "class",
    "interface",
    "type",
    "enum",
    "namespace",
    "module",
];

#[inline]
const fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

#[inline]
fn is_less_than_or_equal(bytes: &[u8], idx: usize) -> bool {
    idx + 1 < bytes.len() && bytes[idx + 1] == b'='
}

#[inline]
fn is_greater_than_or_equal(bytes: &[u8], idx: usize) -> bool {
    idx > 0 && bytes[idx - 1] == b'='
}

fn preceded_by_declaration_keyword(bytes: &[u8], probe: usize) -> bool {
    DECLARATION_KEYWORDS.iter().any(|keyword| {
        let kw = keyword.as_bytes();
        if probe < kw.len() {
            return false;
        }
        let start = probe - kw.len();
        if &bytes[start..probe] != kw {
            return false;
        }
        start == 0 || !is_ident_byte(bytes[start - 1])
    })
}

/// Advance `i` past a `//` or `/* */` comment starting at `bytes[i]` within
/// `bytes[..end]`.  The caller must ensure `bytes[i] == b'/'`.
/// Always advances by at least one byte, so callers using `continue` will not
/// spin when `/` is a division operator rather than a comment start.
#[inline]
fn skip_comment(bytes: &[u8], mut i: usize, end: usize) -> usize {
    if i + 1 < end && bytes[i + 1] == b'/' {
        i += 2;
        while i < end && bytes[i] != b'\n' && bytes[i] != b'\r' {
            i += 1;
        }
    } else if i + 1 < end && bytes[i + 1] == b'*' {
        i += 2;
        while i + 1 < end {
            if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                return i + 2;
            }
            i += 1;
        }
        i = end;
    } else {
        // bare `/` (division) — advance past it so callers don't spin
        i += 1;
    }
    i
}

/// Extract the callee name, its end offset, and the `is_new` flag from the
/// source immediately before `delimiter_offset`.
///
/// Returns `None` when no valid identifier is found or when the identifier is
/// preceded by a declaration keyword.
fn extract_callee(source: &str, delimiter_offset: usize) -> Option<(String, usize, bool)> {
    let bytes = source.as_bytes();
    let mut end = delimiter_offset;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let callee_end = end;
    let mut start = end;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    if start == end {
        return None;
    }
    // `is_ident_byte` only matches ASCII, so `start..end` is always on a char boundary.
    let callee_name = source[start..end].to_string();

    let mut probe = start;
    while probe > 0 && bytes[probe - 1].is_ascii_whitespace() {
        probe -= 1;
    }
    if preceded_by_declaration_keyword(bytes, probe) {
        return None;
    }
    let is_new = probe >= 3 && {
        let prefix = &bytes[probe - 3..probe];
        let boundary_ok = probe == 3 || !is_ident_byte(bytes[probe - 4]);
        prefix == b"new" && boundary_ok
    };

    Some((callee_name, callee_end, is_new))
}

fn build_context(
    source: &str,
    delimiter_offset: usize,
    cursor: usize,
    delimiter: CallDelimiter,
) -> Option<IncompleteCallContext> {
    let (callee_name, callee_end_offset, is_new_expression) =
        extract_callee(source, delimiter_offset)?;
    let scan_start = (delimiter_offset + 1).min(cursor);
    let active_parameter = count_top_level_commas(source, scan_start, cursor);
    Some(IncompleteCallContext {
        callee_name,
        callee_end_offset,
        delimiter,
        active_parameter,
        span_start: scan_start,
        span_length: cursor.saturating_sub(scan_start),
        is_new_expression,
    })
}

/// Scan backward from `cursor` to find an unmatched `(` that looks like the
/// start of an incomplete call expression.
///
/// The scan stops at statement boundaries (`\n`, `\r`, `;`) when the paren
/// depth is 0.
///
/// Returns `None` when no trigger can be located.
pub fn find_incomplete_paren_call(source: &str, cursor: usize) -> Option<IncompleteCallContext> {
    let bytes = source.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let cursor = cursor.min(bytes.len());

    let mut depth = 0i32;
    let mut idx = cursor;
    while idx > 0 {
        idx -= 1;
        match bytes[idx] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    return build_context(source, idx, cursor, CallDelimiter::Paren);
                }
                depth -= 1;
            }
            b';' | b'\n' | b'\r' if depth == 0 => break,
            _ => {}
        }
    }
    None
}

/// Scan backward from `cursor` to find an unmatched `<` that looks like the
/// start of an incomplete generic type-argument list.
///
/// `<=` and `>=` are not treated as angle brackets.  The scan stops at
/// statement boundaries (`\n`, `\r`, `;`) when the angle depth is 0.
///
/// Returns `None` when no trigger can be located.
pub fn find_incomplete_angle_call(source: &str, cursor: usize) -> Option<IncompleteCallContext> {
    let bytes = source.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let cursor = cursor.min(bytes.len());

    let mut depth = 0i32;
    let mut idx = cursor;
    while idx > 0 {
        idx -= 1;
        match bytes[idx] {
            b'>' if !is_greater_than_or_equal(bytes, idx) => depth += 1,
            b'<' if !is_less_than_or_equal(bytes, idx) => {
                if depth == 0 {
                    return build_context(source, idx, cursor, CallDelimiter::AngleBracket);
                }
                depth -= 1;
            }
            b';' | b'\n' | b'\r' if depth == 0 => break,
            _ => {}
        }
    }
    None
}

/// Count the number of commas at nesting depth zero in `source[start..end]`.
///
/// Nesting is tracked for `(`, `)`, `[`, `]`, `{`, `}`, and `<`/`>` pairs.
/// `//` line comments and `/* */` block comments are skipped so that commas
/// inside comments are not counted.
pub fn count_top_level_commas(source: &str, start: usize, end: usize) -> u32 {
    let end = end.min(source.len());
    if start >= end {
        return 0;
    }
    let bytes = source.as_bytes();
    let mut commas = 0u32;
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut angle = 0i32;
    let mut i = start;
    while i < end {
        match bytes[i] {
            b'/' => {
                i = skip_comment(bytes, i, end);
                continue;
            }
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b'{' => brace += 1,
            b'}' => brace = brace.saturating_sub(1),
            b'<' if !is_less_than_or_equal(bytes, i) => angle += 1,
            b'>' if !is_greater_than_or_equal(bytes, i) => angle = angle.saturating_sub(1),
            b',' if paren == 0 && bracket == 0 && brace == 0 && angle == 0 => commas += 1,
            _ => {}
        }
        i += 1;
    }
    commas
}

/// Returns `true` when there is at least one comma in `source[start..end]`
/// that is not inside a `//` or `/* */` comment.
///
/// Unlike [`count_top_level_commas`], this function does **not** track bracket
/// nesting — a comma inside `(x, y)` will return `true`.  Use only when a
/// coarse presence check is sufficient (e.g. detecting whether any separator
/// exists between two AST node spans, not whether it is a top-level argument
/// separator).
pub fn has_comma_between_offsets(source: &str, start: usize, end: usize) -> bool {
    let max = source.len();
    let start = start.min(max);
    let end = end.min(max);
    if start >= end {
        return false;
    }
    let bytes = source.as_bytes();
    let mut i = start;
    while i < end {
        match bytes[i] {
            b',' => return true,
            b'/' => {
                i = skip_comment(bytes, i, end);
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- find_incomplete_paren_call ---

    #[test]
    fn paren_basic_call() {
        let src = "foo(a, b";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "foo");
        assert_eq!(ctx.callee_end_offset, 3); // end of "foo"
        assert_eq!(ctx.delimiter, CallDelimiter::Paren);
        assert_eq!(ctx.active_parameter, 1);
        assert!(!ctx.is_new_expression);
    }

    #[test]
    fn paren_callee_end_is_identifier_end_not_delimiter() {
        // "foo  (a" — callee_end_offset must point after "foo", not at "("
        let src = "foo  (a";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "foo");
        assert_eq!(ctx.callee_end_offset, 3); // end of "foo", not 5 (the `(`)
    }

    #[test]
    fn paren_renamed_callee() {
        let src = "myFunction(x, y, z";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "myFunction");
        assert_eq!(ctx.active_parameter, 2);
    }

    #[test]
    fn paren_new_expression() {
        let src = "new Foo(a";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "Foo");
        assert!(ctx.is_new_expression);
    }

    #[test]
    fn paren_new_expression_different_name() {
        let src = "new MyClass(x, y";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "MyClass");
        assert!(ctx.is_new_expression);
        assert_eq!(ctx.active_parameter, 1);
    }

    #[test]
    fn paren_nested_call_cursor_in_outer() {
        let src = "outer(inner(), x";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "outer");
        assert_eq!(ctx.active_parameter, 1);
    }

    #[test]
    fn paren_stops_at_semicolon() {
        let src = "let x = 1; foo(a";
        let ctx = find_incomplete_paren_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "foo");
        assert_eq!(ctx.active_parameter, 0);
    }

    #[test]
    fn paren_declaration_keyword_excluded() {
        let src = "function foo(a";
        assert!(find_incomplete_paren_call(src, src.len()).is_none());
    }

    #[test]
    fn paren_no_trigger() {
        assert!(find_incomplete_paren_call("no parens here", 14).is_none());
    }

    // --- find_incomplete_angle_call ---

    #[test]
    fn angle_basic_generic() {
        let src = "foo<A, B";
        let ctx = find_incomplete_angle_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "foo");
        assert_eq!(ctx.callee_end_offset, 3); // end of "foo"
        assert_eq!(ctx.delimiter, CallDelimiter::AngleBracket);
        assert_eq!(ctx.active_parameter, 1);
    }

    #[test]
    fn angle_different_callee() {
        let src = "myGeneric<T, U, V";
        let ctx = find_incomplete_angle_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "myGeneric");
        assert_eq!(ctx.active_parameter, 2);
    }

    #[test]
    fn angle_nested_angle_brackets() {
        let src = "foo<Array<number>, ";
        let ctx = find_incomplete_angle_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "foo");
        assert_eq!(ctx.active_parameter, 1);
    }

    #[test]
    fn angle_no_trigger() {
        assert!(find_incomplete_angle_call("no angle brackets", 17).is_none());
    }

    #[test]
    fn angle_less_equal_comparison_is_not_generic_trigger() {
        let src = "foo <= value";
        assert!(find_incomplete_angle_call(src, src.len()).is_none());
    }

    #[test]
    fn angle_nested_less_equal_comparison_is_not_generic_trigger() {
        let src = "outer(foo <= value";
        assert!(find_incomplete_angle_call(src, src.len()).is_none());
    }

    #[test]
    fn angle_less_than_generic_trigger_still_works() {
        let src = "foo<T";
        let ctx = find_incomplete_angle_call(src, src.len()).unwrap();
        assert_eq!(ctx.callee_name, "foo");
        assert_eq!(ctx.active_parameter, 0);
    }

    // --- count_top_level_commas ---

    #[test]
    fn commas_basic() {
        assert_eq!(count_top_level_commas("a, b, c", 0, 7), 2);
    }

    #[test]
    fn commas_nested_parens_ignored() {
        assert_eq!(count_top_level_commas("a, foo(x, y), b", 0, 15), 2);
    }

    #[test]
    fn commas_in_line_comment_skipped() {
        let src = "a, // , not counted\nb";
        assert_eq!(count_top_level_commas(src, 0, src.len()), 1);
    }

    #[test]
    fn commas_in_block_comment_skipped() {
        let src = "a, /* , not counted */ b";
        assert_eq!(count_top_level_commas(src, 0, src.len()), 1);
    }

    #[test]
    fn commas_nested_brackets() {
        assert_eq!(count_top_level_commas("a, [x, y], b", 0, 12), 2);
    }

    #[test]
    fn commas_after_less_equal_comparison_are_top_level() {
        let src = "a <= b, c";
        assert_eq!(count_top_level_commas(src, 0, src.len()), 1);
    }

    // --- has_comma_between_offsets ---

    #[test]
    fn has_comma_yes() {
        assert!(has_comma_between_offsets("a, b", 0, 4));
    }

    #[test]
    fn has_comma_no() {
        assert!(!has_comma_between_offsets("ab", 0, 2));
    }

    #[test]
    fn has_comma_in_line_comment_skipped() {
        let src = "// , ignored\na";
        assert!(!has_comma_between_offsets(src, 0, src.len()));
    }

    #[test]
    fn has_comma_in_block_comment_skipped() {
        let src = "/* , ignored */ a";
        assert!(!has_comma_between_offsets(src, 0, src.len()));
    }

    #[test]
    fn has_comma_ignores_nesting_by_design() {
        // Unlike count_top_level_commas, a comma inside nested brackets is detected.
        // This reflects the coarse semantics: any comma in the range, not just top-level.
        assert!(has_comma_between_offsets("(x, y)", 0, 6));
    }

    // --- division operator does not spin ---

    #[test]
    fn commas_with_division_operator() {
        // A bare `/` (not `//` or `/*`) must not cause an infinite loop.
        assert_eq!(count_top_level_commas("a / b, c", 0, 8), 1);
    }

    #[test]
    fn has_comma_with_division_operator() {
        assert!(has_comma_between_offsets("a / b, c", 0, 8));
    }
}
