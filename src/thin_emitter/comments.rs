// Comment utilities shared by the thin emitter.

/// Represents a comment range in the source text.
#[derive(Debug, Clone, Copy)]
pub struct CommentRange {
    pub pos: u32,
    pub end: u32,
    pub kind: CommentKind,
    pub has_trailing_newline: bool,
}

/// Kind of comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentKind {
    SingleLine, // // comment
    MultiLine,  // /* comment */
}

/// Check if a character is a line break.
fn is_line_break(ch: char) -> bool {
    ch == '\n' || ch == '\r' || ch == '\u{2028}' || ch == '\u{2029}'
}

/// Check if a character is whitespace (but not a line break).
fn is_whitespace_single_line(ch: char) -> bool {
    ch == ' ' || ch == '\t' || ch == '\u{000B}' || ch == '\u{000C}'
}

/// UTF-8 safe helper to get the character at a byte position.
/// Returns None if pos is out of bounds or not on a char boundary.
fn char_at(text: &str, pos: usize) -> Option<char> {
    if pos >= text.len() {
        return None;
    }
    text[pos..].chars().next()
}

/// Get trailing comments starting at a position in the source text.
/// Trailing comments are comments that appear on the same line after a token,
/// before a newline.
pub fn get_trailing_comment_ranges(text: &str, pos: usize) -> Vec<CommentRange> {
    let mut comments = Vec::new();
    let len = text.len();
    let mut i = pos;

    // Scan for trailing comments (on the same line, before newline)
    while i < len {
        let ch = char_at(text, i).unwrap_or('\0');
        let char_len = ch.len_utf8();

        // Skip whitespace (but not newlines)
        if is_whitespace_single_line(ch) {
            i += char_len;
            continue;
        }

        // Stop at newline - trailing comments end here
        if is_line_break(ch) {
            break;
        }

        // Check for comment start (/ is ASCII, safe to check byte directly)
        if ch == '/' && i + 1 < len {
            let next_byte = text.as_bytes()[i + 1];

            if next_byte == b'/' {
                // Single-line comment: // ...
                let start = i;
                i += 2;
                while i < len {
                    let c = char_at(text, i).unwrap_or('\0');
                    if is_line_break(c) {
                        break;
                    }
                    i += c.len_utf8();
                }
                comments.push(CommentRange {
                    pos: start as u32,
                    end: i as u32,
                    kind: CommentKind::SingleLine,
                    has_trailing_newline: i < len
                        && is_line_break(char_at(text, i).unwrap_or('\0')),
                });
                continue;
            } else if next_byte == b'*' {
                // Multi-line comment: /* ... */
                let start = i;
                i += 2;
                let mut has_newline = false;
                while i + 1 < len {
                    let c = char_at(text, i).unwrap_or('\0');
                    if c == '*' && text.as_bytes()[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    if is_line_break(c) {
                        has_newline = true;
                    }
                    i += c.len_utf8();
                }
                // For trailing comments, we stop after the first multi-line comment
                // if it spans multiple lines
                comments.push(CommentRange {
                    pos: start as u32,
                    end: i as u32,
                    kind: CommentKind::MultiLine,
                    has_trailing_newline: has_newline,
                });
                if has_newline {
                    break;
                }
                continue;
            }
        }

        // Non-whitespace, non-comment character - stop scanning
        break;
    }

    comments
}

/// Get leading comments before a position in the source text.
/// Leading comments are comments that appear before a token,
/// potentially on preceding lines.
pub fn get_leading_comment_ranges(text: &str, pos: usize) -> Vec<CommentRange> {
    let mut comments = Vec::new();
    let len = text.len();
    let mut i = pos;

    // Skip shebang at the start of file
    if i == 0 && len >= 2 && text.as_bytes()[0] == b'#' && text.as_bytes()[1] == b'!' {
        while i < len {
            let c = char_at(text, i).unwrap_or('\0');
            if is_line_break(c) {
                break;
            }
            i += c.len_utf8();
        }
    }

    // Scan for leading comments
    let mut pending: Option<CommentRange> = None;

    while i < len {
        let ch = char_at(text, i).unwrap_or('\0');
        let char_len = ch.len_utf8();

        // Skip whitespace
        if is_whitespace_single_line(ch) {
            i += char_len;
            continue;
        }

        // Handle newlines - they mark comment boundaries
        if is_line_break(ch) {
            i += char_len;
            // Skip \r\n as a single newline
            if ch == '\r' && i < len && text.as_bytes()[i] == b'\n' {
                i += 1;
            }
            if let Some(mut p) = pending.take() {
                p.has_trailing_newline = true;
                comments.push(p);
            }
            continue;
        }

        // Check for comment start (/ is ASCII, safe to check byte directly)
        if ch == '/' && i + 1 < len {
            let next_byte = text.as_bytes()[i + 1];

            if next_byte == b'/' {
                // Emit any pending comment first
                if let Some(p) = pending.take() {
                    comments.push(p);
                }
                // Single-line comment
                let start = i;
                i += 2;
                while i < len {
                    let c = char_at(text, i).unwrap_or('\0');
                    if is_line_break(c) {
                        break;
                    }
                    i += c.len_utf8();
                }
                pending = Some(CommentRange {
                    pos: start as u32,
                    end: i as u32,
                    kind: CommentKind::SingleLine,
                    has_trailing_newline: false,
                });
                continue;
            } else if next_byte == b'*' {
                // Emit any pending comment first
                if let Some(p) = pending.take() {
                    comments.push(p);
                }
                // Multi-line comment
                let start = i;
                i += 2;
                while i + 1 < len {
                    if text.as_bytes()[i] == b'*' && text.as_bytes()[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    let c = char_at(text, i).unwrap_or('\0');
                    i += c.len_utf8();
                }
                pending = Some(CommentRange {
                    pos: start as u32,
                    end: i as u32,
                    kind: CommentKind::MultiLine,
                    has_trailing_newline: false,
                });
                continue;
            }
        }

        // Non-whitespace, non-comment - we're done
        break;
    }

    // Emit final pending comment
    if let Some(p) = pending {
        comments.push(p);
    }

    comments
}
