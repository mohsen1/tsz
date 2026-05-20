//! Tests for pure helpers in `transforms::emit_utils`.
//!
//! This file is included via `#[path = "../../tests/emit_utils.rs"] mod tests;`
//! inside `crates/tsz-emitter/src/transforms/emit_utils.rs`, so tests have
//! access to all `pub(crate)` items in the parent module.

use super::{is_valid_identifier_name, next_temp_var_name, skip_trivia_forward};

// =============================================================================
// is_valid_identifier_name
// =============================================================================

#[test]
fn identifier_accepts_simple_letter_start() {
    assert!(is_valid_identifier_name("foo"));
    assert!(is_valid_identifier_name("Bar"));
    assert!(is_valid_identifier_name("a"));
    assert!(is_valid_identifier_name("Z"));
}

#[test]
fn identifier_accepts_underscore_or_dollar_start() {
    assert!(is_valid_identifier_name("_"));
    assert!(is_valid_identifier_name("__"));
    assert!(is_valid_identifier_name("$"));
    assert!(is_valid_identifier_name("$$"));
    assert!(is_valid_identifier_name("_foo"));
    assert!(is_valid_identifier_name("$foo"));
    assert!(is_valid_identifier_name("_$"));
    assert!(is_valid_identifier_name("$_"));
}

#[test]
fn identifier_accepts_digits_after_first() {
    assert!(is_valid_identifier_name("a1"));
    assert!(is_valid_identifier_name("foo123"));
    assert!(is_valid_identifier_name("_0"));
    assert!(is_valid_identifier_name("$1"));
    assert!(is_valid_identifier_name("camelCase42"));
    assert!(is_valid_identifier_name("snake_case_99"));
}

#[test]
fn identifier_rejects_empty() {
    assert!(!is_valid_identifier_name(""));
}

#[test]
fn identifier_rejects_digit_start() {
    assert!(!is_valid_identifier_name("1"));
    assert!(!is_valid_identifier_name("9foo"));
    assert!(!is_valid_identifier_name("0_"));
}

#[test]
fn identifier_rejects_invalid_punctuation() {
    assert!(!is_valid_identifier_name("foo-bar"));
    assert!(!is_valid_identifier_name("foo.bar"));
    assert!(!is_valid_identifier_name("foo bar"));
    assert!(!is_valid_identifier_name("foo+bar"));
    assert!(!is_valid_identifier_name("foo,bar"));
    assert!(!is_valid_identifier_name("foo/bar"));
    assert!(!is_valid_identifier_name("foo'bar"));
    assert!(!is_valid_identifier_name("foo\"bar"));
}

#[test]
fn identifier_rejects_leading_punctuation() {
    assert!(!is_valid_identifier_name("-foo"));
    assert!(!is_valid_identifier_name(".foo"));
    assert!(!is_valid_identifier_name(" foo"));
    assert!(!is_valid_identifier_name("@foo"));
}

#[test]
fn identifier_accepts_unicode_alphabetic() {
    // `is_alphabetic` accepts Unicode letters, matching `\p{Alpha}`.
    assert!(is_valid_identifier_name("naïve"));
    assert!(is_valid_identifier_name("π"));
    assert!(is_valid_identifier_name("Ω"));
    assert!(is_valid_identifier_name("café"));
}

#[test]
fn identifier_rejects_symbols_and_emoji() {
    // Emoji and pictographs are not alphabetic.
    assert!(!is_valid_identifier_name("😀"));
    assert!(!is_valid_identifier_name("☃"));
    // A non-alphabetic char in the middle still rejects.
    assert!(!is_valid_identifier_name("foo😀"));
}

// =============================================================================
// next_temp_var_name
// =============================================================================

#[test]
fn next_temp_var_returns_underscore_letters_in_order() {
    let mut counter: u32 = 0;
    let names: Vec<String> = (0..5).map(|_| next_temp_var_name(&mut counter)).collect();
    assert_eq!(names, vec!["_a", "_b", "_c", "_d", "_e"]);
    assert_eq!(counter, 5);
}

#[test]
fn next_temp_var_wraps_after_z() {
    let mut counter: u32 = 25;
    assert_eq!(next_temp_var_name(&mut counter), "_z");
    assert_eq!(next_temp_var_name(&mut counter), "_a");
    assert_eq!(next_temp_var_name(&mut counter), "_b");
    assert_eq!(counter, 28);
}

#[test]
fn next_temp_var_advances_counter_each_call() {
    let mut counter: u32 = 0;
    let _ = next_temp_var_name(&mut counter);
    assert_eq!(counter, 1);
    let _ = next_temp_var_name(&mut counter);
    let _ = next_temp_var_name(&mut counter);
    assert_eq!(counter, 3);
}

#[test]
fn next_temp_var_starts_at_specific_offset() {
    let mut counter: u32 = 13; // 'a' + 13 = 'n'
    assert_eq!(next_temp_var_name(&mut counter), "_n");
    assert_eq!(counter, 14);
}

// =============================================================================
// skip_trivia_forward
// =============================================================================

#[test]
fn skip_trivia_returns_start_when_source_is_none() {
    assert_eq!(skip_trivia_forward(None, 0, 100), 0);
    assert_eq!(skip_trivia_forward(None, 7, 100), 7);
    assert_eq!(skip_trivia_forward(None, u32::MAX, u32::MAX), u32::MAX);
}

#[test]
fn skip_trivia_skips_whitespace() {
    let src = "    foo";
    assert_eq!(skip_trivia_forward(Some(src), 0, src.len() as u32), 4);
}

#[test]
fn skip_trivia_skips_tabs_and_newlines() {
    let src = " \t\r\n  bar";
    assert_eq!(skip_trivia_forward(Some(src), 0, src.len() as u32), 6);
}

#[test]
fn skip_trivia_handles_empty_source() {
    let src = "";
    assert_eq!(skip_trivia_forward(Some(src), 0, 0), 0);
    // start past end is clamped by min(end, bytes.len())
    assert_eq!(skip_trivia_forward(Some(src), 5, 5), 5);
}

#[test]
fn skip_trivia_returns_start_when_no_trivia() {
    let src = "foo";
    assert_eq!(skip_trivia_forward(Some(src), 0, src.len() as u32), 0);
}

#[test]
fn skip_trivia_skips_single_line_comment_to_newline() {
    let src = "// comment\nbar";
    let pos = skip_trivia_forward(Some(src), 0, src.len() as u32);
    // Stops at the `\n` (which is then consumed as whitespace), then on `b`.
    assert_eq!(pos, 11);
    assert_eq!(&src[pos as usize..], "bar");
}

#[test]
fn skip_trivia_skips_consecutive_single_line_comments() {
    let src = "// a\n// b\nx";
    let pos = skip_trivia_forward(Some(src), 0, src.len() as u32);
    assert_eq!(&src[pos as usize..], "x");
}

#[test]
fn skip_trivia_skips_multi_line_comment() {
    let src = "/* hello */foo";
    let pos = skip_trivia_forward(Some(src), 0, src.len() as u32);
    assert_eq!(&src[pos as usize..], "foo");
}

#[test]
fn skip_trivia_skips_multi_line_comment_with_newlines() {
    let src = "/* line1\nline2\n*/zzz";
    let pos = skip_trivia_forward(Some(src), 0, src.len() as u32);
    assert_eq!(&src[pos as usize..], "zzz");
}

#[test]
fn skip_trivia_skips_mixed_trivia() {
    let src = "  /* outer */\n  // inner\n  qux";
    let pos = skip_trivia_forward(Some(src), 0, src.len() as u32);
    assert_eq!(&src[pos as usize..], "qux");
}

#[test]
fn skip_trivia_respects_end_bound() {
    let src = "    foo";
    // Cap end at 2 — should advance through whitespace to position 2 only.
    assert_eq!(skip_trivia_forward(Some(src), 0, 2), 2);
}

#[test]
fn skip_trivia_clamps_end_above_text_len() {
    let src = "  ";
    // end is larger than text, should clamp internally to bytes.len() (2).
    assert_eq!(skip_trivia_forward(Some(src), 0, 1000), 2);
}

#[test]
fn skip_trivia_lone_slash_is_not_a_comment() {
    let src = "/x";
    // `/` followed by non-`/` and non-`*` ends the trivia run.
    assert_eq!(skip_trivia_forward(Some(src), 0, src.len() as u32), 0);
}

#[test]
fn skip_trivia_unterminated_block_comment_consumes_to_end() {
    // Without `*/`, the inner loop scans to the end without breaking, so
    // pos remains at `len-1` (the loop condition is `pos + 1 < end`).
    let src = "/* never ends";
    let pos = skip_trivia_forward(Some(src), 0, src.len() as u32);
    // Inner loop advances to `pos + 1 >= end`, which leaves pos at len - 1.
    assert_eq!(pos as usize, src.len() - 1);
}

#[test]
fn skip_trivia_starts_partway_through() {
    let src = "abc   def";
    // Start at position 3 (the spaces); should skip to 'd' at position 6.
    assert_eq!(skip_trivia_forward(Some(src), 3, src.len() as u32), 6);
}
