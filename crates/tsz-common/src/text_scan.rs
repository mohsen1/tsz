//! Char-boundary-safe text windowing helpers.
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

#[cfg(test)]
mod tests {
    use super::leading_window;

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
