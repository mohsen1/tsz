//! Tests for comments.rs

use crate::comments::*;

#[test]
fn test_extract_single_line_comment() {
    let source = "// hello\nconst x = 1;";
    let comments = get_comment_ranges(source);

    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].pos, 0);
    assert_eq!(comments[0].end, 8);
    assert!(!comments[0].is_multi_line);
    assert!(comments[0].has_trailing_new_line);
    assert_eq!(comments[0].get_text(source), "// hello");
}

#[test]
fn test_extract_multi_line_comment() {
    let source = "/* hello */const x = 1;";
    let comments = get_comment_ranges(source);

    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].pos, 0);
    assert_eq!(comments[0].end, 11);
    assert!(comments[0].is_multi_line);
    assert_eq!(comments[0].get_text(source), "/* hello */");
}

#[test]
fn test_extract_multiple_comments() {
    let source = "// first\n// second\nconst x = 1;";
    let comments = get_comment_ranges(source);

    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].get_text(source), "// first");
    assert_eq!(comments[1].get_text(source), "// second");
}

#[test]
fn test_jsdoc_detection() {
    let source = "/** @param x */";
    let comments = get_comment_ranges(source);

    assert_eq!(comments.len(), 1);
    assert!(is_jsdoc_comment(&comments[0], source));
}

#[test]
fn test_not_jsdoc() {
    let source = "/*** not jsdoc */";
    let comments = get_comment_ranges(source);

    assert_eq!(comments.len(), 1);
    assert!(!is_jsdoc_comment(&comments[0], source));
}

#[test]
fn test_triple_slash_directive() {
    let source = "/// <reference path=\"foo.d.ts\" />";
    let comments = get_comment_ranges(source);

    assert_eq!(comments.len(), 1);
    assert!(is_triple_slash_directive(&comments[0], source));
}

#[test]
fn test_jsdoc_content_extraction() {
    let source = "/**\n * Hello\n * World\n */";
    let comments = get_comment_ranges(source);

    assert_eq!(comments.len(), 1);
    let content = get_jsdoc_content(&comments[0], source);
    assert_eq!(content, "Hello\nWorld");
}

#[test]
fn test_format_multiline_comment() {
    let text = "/*\n * Line 1\n * Line 2\n */";
    let formatted = format_multi_line_comment(text, "    ");
    assert!(formatted.contains("Line 1"));
    assert!(formatted.contains("Line 2"));
}

#[test]
fn test_empty_source() {
    let source = "";
    let comments = get_comment_ranges(source);
    assert!(comments.is_empty());
}

#[test]
fn test_no_comments() {
    let source = "const x = 1;";
    let comments = get_comment_ranges(source);
    // This will include the source as non-comment text
    // The function stops at actual code
    assert!(comments.is_empty());
}

#[test]
fn test_nested_comment_markers() {
    let source = "/* outer /* inner */ end */";
    let comments = get_comment_ranges(source);

    // Should find the first complete comment
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].get_text(source), "/* outer /* inner */");
}
