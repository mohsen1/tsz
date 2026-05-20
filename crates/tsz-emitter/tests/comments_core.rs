//! Behavior locks for the pure comment-range scanners.
//!
//! These tests pin the current behavior of the two exported functions in
//! `tsz_emitter::emitter::comments::core`:
//!
//! - [`get_leading_comment_ranges`] — scan forward from `pos` over whitespace
//!   and consecutive comments, returning every comment found before the first
//!   non-whitespace, non-comment token.
//! - [`get_trailing_comment_ranges`] — scan forward from `pos` over inline
//!   whitespace, returning comments that appear on the same line; bail at the
//!   first hard newline or non-whitespace, non-comment token.
//!
//! Both functions operate on raw source text and are pure: they take a `&str`
//! and a byte offset, and return `Vec<CommentRange>` without consulting any
//! shared state.
//!
//! Coverage targets:
//! - Empty input, position out-of-bounds, position at end-of-text.
//! - Single-line `//` and multi-line `/* */` comments.
//! - Trailing vs leading semantics around newlines.
//! - `\r\n` line ending handled as a single newline.
//! - Shebang `#!` skip at file start (`pos == 0`) only.
//! - Multi-byte UTF-8 (BMP + surrogate-pair) inside comment bodies stays
//!   UTF-8-safe and never panics.
//! - `CommentKind` discrimination.
//! - Unterminated `/* ...` to EOF yields a comment that stops one byte before
//!   the EOF (current behavior — locked here, not asserted as correct).
//! - Adjacent comments emit separately.

use super::{CommentKind, CommentRange, get_leading_comment_ranges, get_trailing_comment_ranges};

// ----------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------

fn slice<'a>(text: &'a str, c: &CommentRange) -> &'a str {
    &text[c.pos as usize..c.end as usize]
}

// ----------------------------------------------------------------------
// get_trailing_comment_ranges
// ----------------------------------------------------------------------

#[test]
fn trailing_empty_text_returns_no_comments() {
    let comments = get_trailing_comment_ranges("", 0);
    assert!(comments.is_empty());
}

#[test]
fn trailing_position_at_end_returns_no_comments() {
    let text = "let x = 1;";
    let comments = get_trailing_comment_ranges(text, text.len());
    assert!(comments.is_empty());
}

#[test]
fn trailing_position_past_end_returns_no_comments() {
    let text = "let x = 1;";
    // The function clamps via the `i < len` loop guard, so out-of-bounds is safe.
    let comments = get_trailing_comment_ranges(text, text.len() + 100);
    assert!(comments.is_empty());
}

#[test]
fn trailing_picks_up_inline_single_line_comment() {
    let text = "let x = 1; // trailing";
    // pos=10 sits right after the `;`.
    let comments = get_trailing_comment_ranges(text, 10);
    assert_eq!(comments.len(), 1);
    let c = &comments[0];
    assert_eq!(c.kind, CommentKind::SingleLine);
    assert_eq!(slice(text, c), "// trailing");
    // No newline after the comment in this fixture.
    assert!(!c.has_trailing_newline);
}

#[test]
fn trailing_single_line_with_newline_after_marks_trailing_newline() {
    let text = "let x = 1; // trailing\nlet y = 2;";
    let comments = get_trailing_comment_ranges(text, 10);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].kind, CommentKind::SingleLine);
    assert!(comments[0].has_trailing_newline);
}

#[test]
fn trailing_picks_up_inline_multi_line_comment() {
    let text = "let x = 1; /* trailing */";
    let comments = get_trailing_comment_ranges(text, 10);
    assert_eq!(comments.len(), 1);
    let c = &comments[0];
    assert_eq!(c.kind, CommentKind::MultiLine);
    assert_eq!(slice(text, c), "/* trailing */");
    // No newline inside the comment body.
    assert!(!c.has_trailing_newline);
}

#[test]
fn trailing_stops_at_newline_before_following_comment() {
    let text = "let x = 1;\n// not trailing\n";
    // No comment on the same line as `;`, so we expect no trailing comments.
    let comments = get_trailing_comment_ranges(text, 10);
    assert!(comments.is_empty());
}

#[test]
fn trailing_multiline_comment_with_internal_newline_breaks_after_emit() {
    // The function pushes the comment with has_trailing_newline=true and bails.
    let text = "x; /* a\nb */ /* second */";
    let comments = get_trailing_comment_ranges(text, 2);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].kind, CommentKind::MultiLine);
    assert!(comments[0].has_trailing_newline);
    assert_eq!(slice(text, &comments[0]), "/* a\nb */");
}

#[test]
fn trailing_consecutive_inline_multi_line_comments() {
    // Two single-line-body multi-line comments — both should be returned and
    // neither should set has_trailing_newline.
    let text = "x; /*a*/ /*b*/";
    let comments = get_trailing_comment_ranges(text, 2);
    assert_eq!(comments.len(), 2);
    assert_eq!(slice(text, &comments[0]), "/*a*/");
    assert_eq!(slice(text, &comments[1]), "/*b*/");
    assert!(!comments[0].has_trailing_newline);
    assert!(!comments[1].has_trailing_newline);
}

#[test]
fn trailing_skips_leading_inline_whitespace() {
    let text = "x;   \t // c";
    let comments = get_trailing_comment_ranges(text, 2);
    assert_eq!(comments.len(), 1);
    assert_eq!(slice(text, &comments[0]), "// c");
}

#[test]
fn trailing_stops_at_non_whitespace_non_comment() {
    let text = "x; let y;";
    let comments = get_trailing_comment_ranges(text, 2);
    assert!(comments.is_empty());
}

#[test]
fn trailing_isolated_slash_is_not_a_comment() {
    // Single `/` should not be treated as a comment start.
    let text = "x; / nope";
    let comments = get_trailing_comment_ranges(text, 2);
    assert!(comments.is_empty());
}

#[test]
fn trailing_handles_utf8_bmp_inside_comment() {
    // 3-byte UTF-8 (CJK) inside the comment body must not panic.
    let text = "x; /* 中文 */ tail";
    let comments = get_trailing_comment_ranges(text, 2);
    assert_eq!(comments.len(), 1);
    assert_eq!(slice(text, &comments[0]), "/* 中文 */");
    assert!(!comments[0].has_trailing_newline);
}

#[test]
fn trailing_handles_utf8_surrogate_pair_inside_comment() {
    // 4-byte UTF-8 (rocket emoji) inside the comment body must not panic.
    let text = "x; // 🚀 launch\n";
    let comments = get_trailing_comment_ranges(text, 2);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].kind, CommentKind::SingleLine);
    assert_eq!(slice(text, &comments[0]), "// 🚀 launch");
    assert!(comments[0].has_trailing_newline);
}

// ----------------------------------------------------------------------
// get_leading_comment_ranges
// ----------------------------------------------------------------------

#[test]
fn leading_empty_text_returns_no_comments() {
    let comments = get_leading_comment_ranges("", 0);
    assert!(comments.is_empty());
}

#[test]
fn leading_position_past_end_returns_no_comments() {
    let text = "let x = 1;";
    let comments = get_leading_comment_ranges(text, text.len() + 50);
    assert!(comments.is_empty());
}

#[test]
fn leading_picks_up_single_line_before_token() {
    let text = "// header\nlet x = 1;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    let c = &comments[0];
    assert_eq!(c.kind, CommentKind::SingleLine);
    assert_eq!(slice(text, c), "// header");
    // The newline after the comment marks has_trailing_newline.
    assert!(c.has_trailing_newline);
}

#[test]
fn leading_picks_up_multi_line_before_token() {
    let text = "/** doc */\nlet x = 1;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    let c = &comments[0];
    assert_eq!(c.kind, CommentKind::MultiLine);
    assert_eq!(slice(text, c), "/** doc */");
    assert!(c.has_trailing_newline);
}

#[test]
fn leading_groups_multiple_comments_separated_by_newlines() {
    let text = "// first\n// second\n/* third */\nlet x;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 3);
    assert_eq!(slice(text, &comments[0]), "// first");
    assert_eq!(slice(text, &comments[1]), "// second");
    assert_eq!(slice(text, &comments[2]), "/* third */");
    // Each comment is followed by a newline before the next token, so all are
    // marked has_trailing_newline=true.
    assert!(comments[0].has_trailing_newline);
    assert!(comments[1].has_trailing_newline);
    assert!(comments[2].has_trailing_newline);
}

#[test]
fn leading_final_comment_without_trailing_newline_has_flag_false() {
    // The final pending comment (no newline before the token) keeps
    // has_trailing_newline=false.
    let text = "/* hugged */let x;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    assert_eq!(slice(text, &comments[0]), "/* hugged */");
    assert!(!comments[0].has_trailing_newline);
}

#[test]
fn leading_handles_crlf_line_ending() {
    let text = "// header\r\nlet x;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    assert_eq!(slice(text, &comments[0]), "// header");
    assert!(comments[0].has_trailing_newline);
}

#[test]
fn leading_skips_shebang_at_start_of_file() {
    let text = "#!/usr/bin/env node\n// real header\nlet x;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    assert_eq!(slice(text, &comments[0]), "// real header");
}

#[test]
fn leading_does_not_skip_shebang_at_non_zero_position() {
    // The shebang skip only fires when pos == 0. From a non-zero position, a
    // bare `#` is not a comment start, so scanning bails immediately.
    let text = "x;#! not a comment\n";
    let comments = get_leading_comment_ranges(text, 2);
    assert!(comments.is_empty());
}

#[test]
fn leading_stops_at_first_non_comment_token() {
    let text = "// a\nlet x;\n// b\n";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    assert_eq!(slice(text, &comments[0]), "// a");
}

#[test]
fn leading_isolated_slash_is_not_a_comment_in_leading_position() {
    let text = "/ not a comment";
    let comments = get_leading_comment_ranges(text, 0);
    assert!(comments.is_empty());
}

#[test]
fn leading_handles_indented_block_of_comments() {
    let text = "    // a\n    /* b */\nlet x;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 2);
    assert_eq!(slice(text, &comments[0]), "// a");
    assert_eq!(slice(text, &comments[1]), "/* b */");
    assert!(comments[0].has_trailing_newline);
    assert!(comments[1].has_trailing_newline);
}

#[test]
fn leading_multi_line_body_yields_multi_line_kind() {
    // A multi-line `/* ... */` whose body contains a real `\n` is still kind
    // MultiLine, not a separate kind.
    let text = "/*\n * doc\n */\nlet x;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].kind, CommentKind::MultiLine);
    assert_eq!(slice(text, &comments[0]), "/*\n * doc\n */");
}

#[test]
fn leading_handles_utf8_bmp_inside_comment_body() {
    let text = "// 中文 doc\nlet x;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    assert_eq!(slice(text, &comments[0]), "// 中文 doc");
    assert!(comments[0].has_trailing_newline);
}

#[test]
fn leading_handles_utf8_surrogate_pair_inside_comment_body() {
    // 4-byte UTF-8 (rocket emoji) inside the multi-line comment body must not
    // split a UTF-8 boundary.
    let text = "/* 🚀 launch */\nlet x;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].kind, CommentKind::MultiLine);
    assert_eq!(slice(text, &comments[0]), "/* 🚀 launch */");
    assert!(comments[0].has_trailing_newline);
}

#[test]
fn leading_unterminated_multiline_does_not_panic() {
    // Unterminated `/* ...` to EOF currently terminates the loop one byte
    // before the end (because the inner loop guard is `i + 1 < len`). This
    // test locks the current behavior: the comment is still emitted, with
    // `end` strictly less than `text.len()` for non-trivially-short inputs,
    // and no panic occurs.
    let text = "/* unterminated";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].kind, CommentKind::MultiLine);
    // The current end is `text.len() - 1`, not `text.len()`. Lock this.
    assert_eq!(comments[0].end as usize, text.len() - 1);
    assert_eq!(comments[0].pos, 0);
    // No newline → has_trailing_newline stays false.
    assert!(!comments[0].has_trailing_newline);
}

#[test]
fn leading_empty_single_line_comment() {
    // `//` followed immediately by a newline.
    let text = "//\nlet x;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].kind, CommentKind::SingleLine);
    assert_eq!(slice(text, &comments[0]), "//");
    assert!(comments[0].has_trailing_newline);
}

#[test]
fn leading_empty_multi_line_comment() {
    // `/**/` is a valid multi-line comment with empty body.
    let text = "/**/\nlet x;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].kind, CommentKind::MultiLine);
    assert_eq!(slice(text, &comments[0]), "/**/");
}

#[test]
fn leading_at_non_zero_position_skips_shebang_logic() {
    // pos != 0 means even an actual `#!...` line would not be skipped; but
    // since `#` is not a comment starter, scanning halts before reaching
    // anything interesting.
    let text = "// a\n#!shebang\n// b\n";
    let comments = get_leading_comment_ranges(text, 0);
    // First comment captured; then `#` is not a comment start, so we bail.
    assert_eq!(comments.len(), 1);
    assert_eq!(slice(text, &comments[0]), "// a");
}

// ----------------------------------------------------------------------
// CommentRange / CommentKind contract checks
// ----------------------------------------------------------------------

#[test]
fn comment_kind_is_copy_and_eq() {
    // Lock the ergonomic contract: SingleLine != MultiLine, both Copy.
    let a = CommentKind::SingleLine;
    let b = a;
    let c = CommentKind::MultiLine;
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn comment_range_pos_is_strictly_less_than_end_for_non_empty_comment() {
    // Any non-zero-length comment must satisfy pos < end. Empty `//` lives at
    // pos..(pos + 2).
    let text = "// nonempty\n";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    assert!(comments[0].pos < comments[0].end);
}

#[test]
fn trailing_inline_comment_position_anchors_at_slash() {
    // The pos of the comment range should equal the byte offset of the leading
    // `/` of the comment marker, not the offset of the search start.
    let text = "x;     /*c*/";
    let comments = get_trailing_comment_ranges(text, 2);
    assert_eq!(comments.len(), 1);
    let c = &comments[0];
    // The first `/` lives at byte 7.
    assert_eq!(c.pos, 7);
    assert_eq!(slice(text, c), "/*c*/");
}

#[test]
fn leading_pos_anchors_at_slash_after_indent() {
    let text = "    // c\nx;";
    let comments = get_leading_comment_ranges(text, 0);
    assert_eq!(comments.len(), 1);
    // The `//` starts at byte 4.
    assert_eq!(comments[0].pos, 4);
}
