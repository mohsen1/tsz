use super::*;

// =============================================================================
// CommentRange::new
// =============================================================================

#[test]
fn test_comment_range_new() {
    let cr = CommentRange::new(5, 15, false, true);
    assert_eq!(cr.pos, 5);
    assert_eq!(cr.end, 15);
    assert!(!cr.is_multi_line);
    assert!(cr.has_trailing_new_line);
}

#[test]
fn test_comment_range_new_multi_line() {
    let cr = CommentRange::new(0, 20, true, false);
    assert!(cr.is_multi_line);
    assert!(!cr.has_trailing_new_line);
}

// =============================================================================
// CommentRange::get_text
// =============================================================================

#[test]
fn test_get_text_basic() {
    let source = "// hello world";
    let cr = CommentRange::new(0, 14, false, false);
    assert_eq!(cr.get_text(source), "// hello world");
}

#[test]
fn test_get_text_partial() {
    let source = "prefix // comment suffix";
    let cr = CommentRange::new(7, 17, false, false);
    assert_eq!(cr.get_text(source), "// comment");
}

#[test]
fn test_get_text_out_of_bounds_returns_empty() {
    let source = "short";
    let cr = CommentRange::new(0, 100, false, false);
    assert_eq!(cr.get_text(source), "");
}

#[test]
fn test_get_text_start_equals_end_returns_empty() {
    let source = "some source";
    let cr = CommentRange::new(5, 5, false, false);
    assert_eq!(cr.get_text(source), "");
}

#[test]
fn test_get_text_start_greater_than_end_returns_empty() {
    let source = "some source";
    let cr = CommentRange::new(10, 5, false, false);
    assert_eq!(cr.get_text(source), "");
}

// =============================================================================
// get_comment_ranges - single-line comments
// =============================================================================

#[test]
fn test_single_line_comment() {
    let source = "// hello";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].pos, 0);
    assert_eq!(comments[0].end, 8);
    assert!(!comments[0].is_multi_line);
    assert!(!comments[0].has_trailing_new_line);
}

#[test]
fn test_single_line_comment_with_trailing_newline() {
    let source = "// hello\n";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].pos, 0);
    assert_eq!(comments[0].end, 8); // end is before the newline
    assert!(!comments[0].is_multi_line);
    assert!(comments[0].has_trailing_new_line);
}

#[test]
fn test_single_line_comment_crlf() {
    let source = "// hello\r\n";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].pos, 0);
    // end is before \r\n
    assert_eq!(comments[0].end, 8);
    assert!(!comments[0].is_multi_line);
    assert!(comments[0].has_trailing_new_line);
}

#[test]
fn test_empty_single_line_comment() {
    let source = "//";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].pos, 0);
    assert_eq!(comments[0].end, 2);
    assert!(!comments[0].is_multi_line);
    assert!(!comments[0].has_trailing_new_line);
}

#[test]
fn test_empty_single_line_comment_with_newline() {
    let source = "//\n";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].get_text(source), "//");
    assert!(comments[0].has_trailing_new_line);
}

// =============================================================================
// get_comment_ranges - multi-line comments
// =============================================================================

#[test]
fn test_multi_line_comment() {
    let source = "/* hello */";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].pos, 0);
    assert_eq!(comments[0].end, 11);
    assert!(comments[0].is_multi_line);
    assert!(!comments[0].has_trailing_new_line);
}

#[test]
fn test_multi_line_comment_with_trailing_newline() {
    let source = "/* hello */\n";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].pos, 0);
    assert_eq!(comments[0].end, 11);
    assert!(comments[0].is_multi_line);
    assert!(comments[0].has_trailing_new_line);
}

#[test]
fn test_empty_multi_line_comment() {
    let source = "/**/";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].pos, 0);
    assert_eq!(comments[0].end, 4);
    assert!(comments[0].is_multi_line);
    assert_eq!(comments[0].get_text(source), "/**/");
}

#[test]
fn test_multi_line_comment_spanning_lines() {
    let source = "/*\n * line 1\n * line 2\n */";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert!(comments[0].is_multi_line);
    assert_eq!(comments[0].get_text(source), source);
}

#[test]
fn test_unclosed_multi_line_comment() {
    let source = "/* unclosed comment";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert!(comments[0].is_multi_line);
    // Should extend to end of source
    assert_eq!(comments[0].end, source.len() as u32);
}

// =============================================================================
// get_comment_ranges - JSDoc comments
// =============================================================================

#[test]
fn test_jsdoc_comment_is_multi_line() {
    let source = "/** JSDoc */";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert!(comments[0].is_multi_line);
    assert_eq!(comments[0].get_text(source), "/** JSDoc */");
}

#[test]
fn test_jsdoc_multi_line() {
    let source = "/**\n * @param {string} name\n * @returns {number}\n */";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert!(comments[0].is_multi_line);
    assert_eq!(comments[0].get_text(source), source);
}

// =============================================================================
// get_comment_ranges - multiple comments
// =============================================================================

#[test]
fn test_multiple_single_line_comments() {
    let source = "// first\n// second\n// third\n";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 3);
    assert_eq!(comments[0].get_text(source), "// first");
    assert_eq!(comments[1].get_text(source), "// second");
    assert_eq!(comments[2].get_text(source), "// third");
}

#[test]
fn test_mixed_comment_types() {
    let source = "// single\n/* multi */\n/** jsdoc */";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 3);

    assert!(!comments[0].is_multi_line);
    assert_eq!(comments[0].get_text(source), "// single");

    assert!(comments[1].is_multi_line);
    assert_eq!(comments[1].get_text(source), "/* multi */");

    assert!(comments[2].is_multi_line);
    assert_eq!(comments[2].get_text(source), "/** jsdoc */");
}

#[test]
fn test_comments_with_whitespace_between() {
    let source = "  // first\n  // second\n";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].get_text(source), "// first");
    assert_eq!(comments[1].get_text(source), "// second");
}

// =============================================================================
// get_comment_ranges - edge cases
// =============================================================================

#[test]
fn test_empty_source() {
    let comments = get_comment_ranges("");
    assert!(comments.is_empty());
}

#[test]
fn test_whitespace_only_source() {
    let comments = get_comment_ranges("   \n\t\r\n  ");
    assert!(comments.is_empty());
}

#[test]
fn test_slash_not_followed_by_slash_or_star() {
    // A single slash should not create a comment
    let source = "/";
    let comments = get_comment_ranges(source);
    assert!(comments.is_empty());
}

#[test]
fn test_comment_like_text_inside_strings_is_ignored() {
    let source = r#"const a = "not // a comment"; const b = 'not /* a comment */'; // real"#;
    let comments = get_comment_ranges(source);

    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].get_text(source), "// real");
}

#[test]
fn test_comment_like_text_inside_regex_character_class_is_ignored() {
    let source = r#"var foo2 = "a//".replace(/.[//]/g, ""); // real"#;
    let comments = get_comment_ranges(source);

    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].get_text(source), "// real");
}

#[test]
fn test_comment_after_return_regex_is_found() {
    let source = r#"function f() { return /.[/*]/g; } /* real */"#;
    let comments = get_comment_ranges(source);

    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].get_text(source), "/* real */");
}

#[test]
fn test_comment_after_modulo_regex_is_found() {
    // `%` is a single-character binary operator that can precede a regex
    // literal just like `+`, `-`, `*`, etc. The leading `/` after `%` must
    // be recognized as starting a regex literal so the trailing comment is
    // identified correctly.
    let source = r#"x % /[//]/g; /* real */"#;
    let comments = get_comment_ranges(source);

    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].get_text(source), "/* real */");
}

#[test]
fn test_adjacent_multi_line_comments() {
    let source = "/* a *//* b */";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].get_text(source), "/* a */");
    assert_eq!(comments[1].get_text(source), "/* b */");
}

// =============================================================================
// get_comment_ranges - byte offset correctness
// =============================================================================

#[test]
fn test_comment_positions_with_leading_whitespace() {
    let source = "    // comment";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].pos, 4);
    assert_eq!(comments[0].end, 14);
}

#[test]
fn test_comment_positions_multi_line_after_whitespace() {
    let source = "\n\n  /* block */";
    let comments = get_comment_ranges(source);
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].pos, 4);
    assert_eq!(comments[0].end, 15);
    assert_eq!(comments[0].get_text(source), "/* block */");
}

// =============================================================================
// is_jsdoc_comment
// =============================================================================

#[test]
fn test_is_jsdoc_comment_true() {
    let source = "/** @param {string} x */";
    let cr = CommentRange::new(0, source.len() as u32, true, false);
    assert!(is_jsdoc_comment(&cr, source));
}

#[test]
fn test_is_jsdoc_comment_false_for_regular_multi_line() {
    let source = "/* regular comment */";
    let cr = CommentRange::new(0, source.len() as u32, true, false);
    assert!(!is_jsdoc_comment(&cr, source));
}

#[test]
fn test_is_jsdoc_comment_false_for_triple_star() {
    // /*** is NOT JSDoc - it starts with three stars
    let source = "/*** not jsdoc */";
    let cr = CommentRange::new(0, source.len() as u32, true, false);
    assert!(!is_jsdoc_comment(&cr, source));
}

#[test]
fn test_is_jsdoc_comment_empty_jsdoc() {
    let source = "/** */";
    let cr = CommentRange::new(0, source.len() as u32, true, false);
    assert!(is_jsdoc_comment(&cr, source));
}

#[test]
fn test_is_jsdoc_comment_triple_star_closing() {
    // /***/ starts with /*** which is excluded by the !starts_with("/***") check
    let source = "/***/";
    let cr = CommentRange::new(0, source.len() as u32, true, false);
    assert!(!is_jsdoc_comment(&cr, source));
}

// =============================================================================
// is_triple_slash_directive
// =============================================================================

#[test]
fn test_is_triple_slash_directive_true() {
    let source = "/// <reference path=\"lib.d.ts\" />";
    let cr = CommentRange::new(0, source.len() as u32, false, false);
    assert!(is_triple_slash_directive(&cr, source));
}

#[test]
fn test_is_triple_slash_directive_false_for_double_slash() {
    let source = "// regular comment";
    let cr = CommentRange::new(0, source.len() as u32, false, false);
    assert!(!is_triple_slash_directive(&cr, source));
}

#[test]
fn test_is_triple_slash_directive_false_for_multi_line() {
    let source = "/* not a directive */";
    let cr = CommentRange::new(0, source.len() as u32, true, false);
    assert!(!is_triple_slash_directive(&cr, source));
}

// =============================================================================
// get_jsdoc_content
// =============================================================================

#[test]
fn test_get_jsdoc_content_simple() {
    let source = "/** Hello */";
    let cr = CommentRange::new(0, source.len() as u32, true, false);
    assert_eq!(get_jsdoc_content(&cr, source), "Hello");
}

#[test]
fn test_get_jsdoc_content_multi_line() {
    let source = "/**\n * Line 1\n * Line 2\n */";
    let cr = CommentRange::new(0, source.len() as u32, true, false);
    let content = get_jsdoc_content(&cr, source);
    assert!(content.contains("Line 1"));
    assert!(content.contains("Line 2"));
}

#[test]
fn test_get_jsdoc_content_strips_leading_stars() {
    let source = "/**\n * @param x\n * @returns y\n */";
    let cr = CommentRange::new(0, source.len() as u32, true, false);
    let content = get_jsdoc_content(&cr, source);
    assert!(content.contains("@param x"));
    assert!(content.contains("@returns y"));
    // Should not have leading * characters
    assert!(!content.contains(" * @param"));
}

#[test]
fn test_get_jsdoc_content_empty_jsdoc() {
    let source = "/** */";
    let cr = CommentRange::new(0, source.len() as u32, true, false);
    let content = get_jsdoc_content(&cr, source);
    assert_eq!(content, "");
}

#[test]
fn test_get_jsdoc_content_non_jsdoc_returns_text() {
    // If not a proper JSDoc (no closing */), returns full text
    let source = "/* not jsdoc";
    let cr = CommentRange::new(0, source.len() as u32, true, false);
    let content = get_jsdoc_content(&cr, source);
    assert_eq!(content, source);
}

#[test]
fn test_get_jsdoc_content_trims_indentation_and_keeps_plain_lines() {
    let source = "/**\n * first\n   second\n * third\n */";
    let cr = CommentRange::new(0, source.len() as u32, true, false);

    let content = get_jsdoc_content(&cr, source);
    assert_eq!(content, "first\nsecond\nthird");
}

// =============================================================================
// get_leading_comments
// =============================================================================

#[test]
fn test_get_leading_comments_before_position() {
    let source = "// first\n// second\n";
    let all = get_comment_ranges(source);
    // Get comments leading up to position 18 (end of second comment + newline)
    let leading = get_leading_comments(source, 18, &all);
    assert_eq!(leading.len(), 2);
}

#[test]
fn test_get_leading_comments_no_comments_before() {
    let source = "// comment at end";
    let all = get_comment_ranges(source);
    // Position 0 - nothing before
    let leading = get_leading_comments(source, 0, &all);
    assert!(leading.is_empty());
}

#[test]
fn test_get_leading_comments_filters_by_position() {
    let source = "// first\n// second\n// third\n";
    let all = get_comment_ranges(source);
    // Position just after first comment
    let leading = get_leading_comments(source, 9, &all);
    assert_eq!(leading.len(), 1);
    assert_eq!(leading[0].get_text(source), "// first");
}

// =============================================================================
// get_trailing_comments
// =============================================================================

#[test]
fn test_get_trailing_comments_same_line() {
    let source = "code // trailing";
    let all = get_comment_ranges(source);
    let trailing = get_trailing_comments(source, 4, &all);
    assert_eq!(trailing.len(), 1);
    assert_eq!(trailing[0].get_text(source), "// trailing");
}

#[test]
fn test_get_trailing_comments_excludes_next_line() {
    let source = "code\n// next line comment";
    let all = get_comment_ranges(source);
    // Trailing comments from position 0 should not include comment on next line
    let trailing = get_trailing_comments(source, 0, &all);
    assert!(trailing.is_empty());
}

#[test]
fn test_get_trailing_comments_excludes_multi_line() {
    let source = "code /* multi */ // single";
    let all = get_comment_ranges(source);
    // Trailing after "code " should include only single-line comment
    let trailing = get_trailing_comments(source, 5, &all);
    // Multi-line comments are filtered out by the implementation
    assert_eq!(trailing.len(), 1);
    assert!(!trailing[0].is_multi_line);
}

#[test]
fn test_get_trailing_comments_respects_crlf_boundaries_and_out_of_range_positions() {
    let source = "code // first\r\nnext // second";
    let all = get_comment_ranges(source);

    let trailing = get_trailing_comments(source, 4, &all);
    assert_eq!(trailing.len(), 1);
    assert_eq!(trailing[0].get_text(source), "// first");

    let past_end = get_trailing_comments(source, source.len() as u32 + 10, &all);
    assert!(past_end.is_empty());
}

// =============================================================================
// format_single_line_comment
// =============================================================================

#[test]
fn test_format_single_line_comment() {
    assert_eq!(format_single_line_comment("// hello"), "// hello");
}

#[test]
fn test_format_single_line_comment_empty() {
    assert_eq!(format_single_line_comment("//"), "//");
}

// =============================================================================
// format_multi_line_comment
// =============================================================================

#[test]
fn test_format_multi_line_comment_single_line() {
    let text = "/* hello */";
    assert_eq!(format_multi_line_comment(text, "  "), "/* hello */");
}

#[test]
fn test_format_multi_line_comment_multiple_lines() {
    let text = "/*\n * line1\n * line2\n */";
    let formatted = format_multi_line_comment(text, "    ");
    // Each continuation line should have the indent prepended
    let lines: Vec<&str> = formatted.lines().collect();
    assert_eq!(lines[0], "/*");
    assert!(lines[1].starts_with("    "));
    assert!(lines[2].starts_with("    "));
    assert!(lines[3].starts_with("    "));
}

#[test]
fn test_format_multi_line_comment_empty_lines_no_indent() {
    // Empty continuation lines should NOT get indentation
    let text = "/*\n\n */";
    let formatted = format_multi_line_comment(text, "  ");
    let lines: Vec<&str> = formatted.lines().collect();
    assert_eq!(lines[0], "/*");
    assert_eq!(lines[1], "");
    assert_eq!(lines[2], "   */");
}

// =============================================================================
// get_leading_comments_from_cache
// =============================================================================

#[test]
fn test_get_leading_comments_from_cache_empty() {
    let comments: Vec<CommentRange> = Vec::new();
    let result = get_leading_comments_from_cache(&comments, 100, "source text");
    assert!(result.is_empty());
}

#[test]
fn test_get_leading_comments_from_cache_basic() {
    let source = "// comment\ncode";
    let comments = get_comment_ranges(source);
    // Position at "code" (byte 11)
    let leading = get_leading_comments_from_cache(&comments, 11, source);
    assert_eq!(leading.len(), 1);
    assert_eq!(leading[0].get_text(source), "// comment");
}

#[test]
fn test_get_leading_comments_from_cache_no_comments_before() {
    let source = "code // trailing";
    let comments = get_comment_ranges(source);
    // Position 0 - nothing before
    let leading = get_leading_comments_from_cache(&comments, 0, source);
    assert!(leading.is_empty());
}

#[test]
fn test_get_leading_comments_from_cache_too_many_newlines() {
    let source = "// comment\n\n\n\ncode";
    let comments = get_comment_ranges(source);
    // 3+ newlines between comment and code position should be rejected (> 2 newlines)
    let leading = get_leading_comments_from_cache(&comments, 14, source);
    assert!(leading.is_empty());
}

#[test]
fn test_get_leading_comments_from_cache_adjacent_jsdoc() {
    let source = "/** docs */\nfunction foo() {}";
    let comments = get_comment_ranges(source);
    // Position at "function" (byte 12)
    let leading = get_leading_comments_from_cache(&comments, 12, source);
    assert_eq!(leading.len(), 1);
    assert!(leading[0].is_multi_line);
}

// =============================================================================
// CommentRange - equality / serialization
// =============================================================================

#[test]
fn test_comment_range_equality() {
    let a = CommentRange::new(0, 10, false, true);
    let b = CommentRange::new(0, 10, false, true);
    let c = CommentRange::new(0, 10, true, true);
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn test_comment_range_clone() {
    let a = CommentRange::new(5, 15, true, false);
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn test_comment_range_debug() {
    let cr = CommentRange::new(0, 5, false, true);
    let debug = format!("{cr:?}");
    assert!(debug.contains("CommentRange"));
    assert!(debug.contains("pos"));
    assert!(debug.contains("end"));
}
