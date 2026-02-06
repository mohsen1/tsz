//! Comment Preservation
//!
//! This module handles extracting and emitting comments from TypeScript source.
//! Comments are not part of the AST, so they must be extracted separately
//! from the source text and associated with nodes for emission.

use serde::{Deserialize, Serialize};

/// A range representing a comment in the source text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommentRange {
    /// Start position (byte offset)
    pub pos: u32,
    /// End position (byte offset)
    pub end: u32,
    /// Whether this is a multi-line comment
    pub is_multi_line: bool,
    /// Whether this comment has a trailing newline
    pub has_trailing_new_line: bool,
}

impl CommentRange {
    /// Create a new comment range.
    pub fn new(pos: u32, end: u32, is_multi_line: bool, has_trailing_new_line: bool) -> Self {
        CommentRange {
            pos,
            end,
            is_multi_line,
            has_trailing_new_line,
        }
    }

    /// Get the comment text from source.
    pub fn get_text<'a>(&self, source: &'a str) -> &'a str {
        let start = self.pos as usize;
        let end = self.end as usize;
        if end <= source.len() && start < end {
            &source[start..end]
        } else {
            ""
        }
    }
}

/// Extract all comment ranges from source text.
///
/// This scans the source text and returns all single-line (//) and
/// multi-line (/* */) comments with their positions.
pub fn get_comment_ranges(source: &str) -> Vec<CommentRange> {
    let mut comments = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut pos = 0;

    while pos < len {
        let ch = bytes[pos];

        // Skip whitespace
        if ch == b' ' || ch == b'\t' || ch == b'\r' || ch == b'\n' {
            pos += 1;
            continue;
        }

        // Check for comment start
        if ch == b'/' && pos + 1 < len {
            let next = bytes[pos + 1];

            if next == b'/' {
                // Single-line comment
                let start = pos as u32;
                pos += 2;

                // Scan to end of line
                while pos < len && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                    pos += 1;
                }

                let has_trailing_new_line = pos < len;
                comments.push(CommentRange::new(
                    start,
                    pos as u32,
                    false,
                    has_trailing_new_line,
                ));

                // Skip the newline
                if pos < len && bytes[pos] == b'\r' {
                    pos += 1;
                }
                if pos < len && bytes[pos] == b'\n' {
                    pos += 1;
                }
                continue;
            } else if next == b'*' {
                // Multi-line comment
                let start = pos as u32;
                pos += 2;

                // Scan to closing */
                let mut closed = false;
                while pos + 1 < len {
                    if bytes[pos] == b'*' && bytes[pos + 1] == b'/' {
                        pos += 2;
                        closed = true;
                        break;
                    }
                    pos += 1;
                }

                if !closed {
                    pos = len; // Unclosed comment - go to end
                }

                // Check for trailing newline
                let has_trailing_new_line =
                    pos < len && (bytes[pos] == b'\n' || bytes[pos] == b'\r');

                comments.push(CommentRange::new(
                    start,
                    pos as u32,
                    true,
                    has_trailing_new_line,
                ));
                continue;
            }
        }

        // Not in a comment or whitespace, skip this character
        // (In practice, we'd stop at actual code, but for simplicity
        // we're just extracting top-level comments here)
        pos += 1;
    }

    comments
}

/// Get leading comments before a position.
///
/// Returns comments that appear before `pos` and after any previous code.
pub fn get_leading_comments(
    _source: &str,
    pos: u32,
    all_comments: &[CommentRange],
) -> Vec<CommentRange> {
    all_comments
        .iter()
        .filter(|c| c.end <= pos)
        .cloned()
        .collect()
}

/// Get trailing comments after a position.
///
/// Returns comments that appear after `pos` on the same line.
pub fn get_trailing_comments(
    source: &str,
    pos: u32,
    all_comments: &[CommentRange],
) -> Vec<CommentRange> {
    let bytes = source.as_bytes();

    // Find the next newline after pos
    let mut line_end = pos as usize;
    while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
        line_end += 1;
    }

    all_comments
        .iter()
        .filter(|c| c.pos >= pos && c.pos < line_end as u32 && !c.is_multi_line)
        .cloned()
        .collect()
}

/// Format a single-line comment for output.
pub fn format_single_line_comment(text: &str) -> String {
    // Already includes // prefix
    text.to_string()
}

/// Format a multi-line comment for output.
pub fn format_multi_line_comment(text: &str, indent: &str) -> String {
    // For multi-line comments, we need to add indentation to each line
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 1 {
        return text.to_string();
    }

    let mut result = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            result.push('\n');
            // Add indentation for continuation lines (except first line)
            if !line.trim().is_empty() {
                result.push_str(indent);
            }
        }
        result.push_str(line);
    }
    result
}

/// Check if a comment is a JSDoc comment.
pub fn is_jsdoc_comment(comment: &CommentRange, source: &str) -> bool {
    let text = comment.get_text(source);
    text.starts_with("/**") && !text.starts_with("/***")
}

/// Check if a comment is a triple-slash directive.
pub fn is_triple_slash_directive(comment: &CommentRange, source: &str) -> bool {
    let text = comment.get_text(source);
    text.starts_with("///")
}

/// Extract the content of a JSDoc comment (without the delimiters).
pub fn get_jsdoc_content(comment: &CommentRange, source: &str) -> String {
    let text = comment.get_text(source);
    if text.starts_with("/**") && text.ends_with("*/") {
        let inner = &text[3..text.len() - 2];
        // Remove leading * from each line
        inner
            .lines()
            .map(|line| {
                let trimmed = line.trim_start();
                if trimmed.starts_with('*') {
                    trimmed[1..].trim_start()
                } else {
                    trimmed
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        text.to_string()
    }
}

/// Get leading comments from cached comment ranges.
///
/// This is an optimized version that uses pre-computed comment ranges
/// instead of rescanning the source. Returns comments that precede the
/// given position.
///
/// # Arguments
/// * `comments` - The cached comment ranges from SourceFileData
/// * `pos` - The position to find leading comments for
///
/// # Returns
/// Vector of comment ranges that appear before the given position.
/// Comments are filtered to only include those immediately preceding
/// the position (with at most one line of whitespace between).
pub fn get_leading_comments_from_cache(
    comments: &[CommentRange],
    pos: u32,
    source: &str,
) -> Vec<CommentRange> {
    if comments.is_empty() {
        return Vec::new();
    }

    // Binary search to find the partition point where comments end at or before `pos`
    // Comments are sorted by their start position, but we need ones that *end* before pos
    let idx = comments.partition_point(|c| c.end <= pos);

    if idx == 0 {
        return Vec::new(); // No comments before this position
    }

    let mut result: Vec<CommentRange> = Vec::new();

    // Iterate backwards from the last comment that ends at or before `pos`
    // Stop when we encounter comments that are too far away (> 2 newlines)
    for i in (0..idx).rev() {
        let comment = &comments[i];

        // Check if there's too much whitespace between comment and target position
        // For the first comment, check against `pos`; for subsequent ones, check against previous comment
        let check_pos = if result.is_empty() {
            pos
        } else {
            result.last().unwrap().pos
        };
        let text_between = &source[comment.end as usize..check_pos as usize];
        // Count newlines with early exit — we only need to know if count > 2
        let mut newline_count = 0usize;
        for byte in text_between.as_bytes() {
            if *byte == b'\n' {
                newline_count += 1;
                if newline_count > 2 {
                    break;
                }
            }
        }

        // Allow up to 2 newlines (JSDoc pattern: /** comment */ \n function)
        if newline_count > 2 {
            break;
        }

        result.push(comment.clone());

        // Stop after collecting adjacent comments
        // (if we've collected some and hit a gap, that's the boundary)
        if newline_count >= 1 && result.len() > 1 {
            break;
        }
    }

    result.reverse(); // Restore original order
    result
}
