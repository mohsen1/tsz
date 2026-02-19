use super::Printer;
use tsz_parser::parser::node::Node;

impl<'a> Printer<'a> {
    // =========================================================================
    // Comment Emission Helpers
    // =========================================================================

    /// Emit trailing comments after a node's end position.
    /// Trailing comments are comments on the same line as `end_pos`,
    /// e.g. `foo(); // comment` — the `// comment` is trailing.
    ///
    /// Uses `all_comments` exclusively (not dynamic source scanning) to avoid
    /// duplicate emission when the leading comment scanner has already advanced
    /// `comment_emit_idx` past a comment.
    pub(super) fn emit_trailing_comments(&mut self, end_pos: u32) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };

        // Look for comments in all_comments starting from comment_emit_idx
        // that are on the same line as end_pos (i.e., trailing comments).
        // A trailing comment must start at or after end_pos, and there must be
        // no line break between end_pos and the comment start.
        let bytes = text.as_bytes();
        while self.comment_emit_idx < self.all_comments.len() {
            let c_pos = self.all_comments[self.comment_emit_idx].pos;
            let c_end = self.all_comments[self.comment_emit_idx].end;

            // Comment must start at or after end_pos
            if c_pos < end_pos {
                // If there's a line break between c_pos and end_pos, the comment is
                // on a different line — don't skip it; the next statement's leading
                // comment loop will pick it up.
                let gap_end = std::cmp::min(end_pos as usize, bytes.len());
                if bytes[c_pos as usize..gap_end]
                    .iter()
                    .any(|&b| b == b'\n' || b == b'\r')
                {
                    break;
                }
                // Same line — skip to avoid double emission.
                self.comment_emit_idx += 1;
                continue;
            }

            // Check if there's a line break between end_pos and the comment.
            // If so, this is a leading comment for the next construct, not trailing.
            let gap_start = end_pos as usize;
            let gap_end = std::cmp::min(c_pos as usize, bytes.len());
            let has_line_break = bytes[gap_start..gap_end]
                .iter()
                .any(|&b| b == b'\n' || b == b'\r');
            if has_line_break {
                break;
            }

            // This is a trailing comment on the same line — emit it
            self.write_space();
            let comment_text =
                crate::printer::safe_slice::slice(text, c_pos as usize, c_end as usize);
            if !comment_text.is_empty() {
                self.write_comment(comment_text);
            }
            self.comment_emit_idx += 1;
        }
    }

    /// Find the position right after the node's actual code content ends.
    /// This gives us the position where trailing comments begin. Our parser's
    /// node.end extends past trailing trivia into the next token's position,
    /// so we need to find the actual end of the node's code.
    ///
    /// Uses forward scanning with brace depth tracking to find the correct
    /// closing `}` at depth 0, avoiding confusion with parent scope braces.
    pub(super) fn find_token_end_before_trivia(&self, pos: u32, end: u32) -> u32 {
        let Some(text) = self.source_text else {
            return end;
        };
        let bytes = text.as_bytes();
        let end_pos = std::cmp::min(end as usize, bytes.len());
        let start_pos = pos as usize;

        if start_pos >= end_pos {
            return end;
        }

        // Forward scan: track the last `}` or `;` at brace depth 0.
        // This correctly identifies the node's own closing token without
        // accidentally matching a parent scope's closing brace.
        let mut depth: i32 = 0;
        let mut last_token_end: Option<usize> = None;
        let mut last_non_trivia_at_depth0: Option<usize> = None;
        let mut i = start_pos;

        while i < end_pos {
            let ch = bytes[i];
            match ch {
                b'{' => {
                    depth += 1;
                    i += 1;
                }
                b'}' => {
                    depth -= 1;
                    if depth < 0 {
                        // We've gone past our scope into a parent - stop
                        break;
                    }
                    if depth == 0 {
                        // This is the closing brace at the top level of this node
                        last_token_end = Some(i + 1);
                        last_non_trivia_at_depth0 = Some(i + 1);
                    }
                    i += 1;
                }
                b';' => {
                    if depth == 0 {
                        last_token_end = Some(i + 1);
                        last_non_trivia_at_depth0 = Some(i + 1);
                    }
                    i += 1;
                }
                b'\'' | b'"' | b'`' => {
                    // Skip string literals to avoid false matches
                    let quote = ch;
                    i += 1;
                    while i < end_pos {
                        if bytes[i] == b'\\' {
                            i += 2; // skip escaped char
                        } else if bytes[i] == quote {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
                b'/' if i + 1 < end_pos && bytes[i + 1] == b'/' => {
                    // Skip single-line comments
                    i += 2;
                    while i < end_pos && bytes[i] != b'\n' && bytes[i] != b'\r' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < end_pos && bytes[i + 1] == b'*' => {
                    // Skip multi-line comments
                    i += 2;
                    while i + 1 < end_pos {
                        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    if depth == 0 && !matches!(ch, b' ' | b'\t' | b'\r' | b'\n') {
                        last_non_trivia_at_depth0 = Some(i + 1);
                    }
                    i += 1;
                }
            }
        }

        let mut token_end = last_token_end
            .or(last_non_trivia_at_depth0)
            .map_or(end, |e| e as u32);

        // Some transformed nodes report `end` before the terminating `;`.
        // Recover by scanning a short same-line suffix for `;` or `}`.
        if token_end <= end {
            let mut j = end_pos;
            while j < bytes.len() {
                match bytes[j] {
                    b' ' | b'\t' => j += 1,
                    b'/' if j + 1 < bytes.len() && bytes[j + 1] == b'/' => break,
                    b'/' if j + 1 < bytes.len() && bytes[j + 1] == b'*' => break,
                    b';' | b'}' => {
                        token_end = (j + 1) as u32;
                        break;
                    }
                    _ => break,
                }
            }
        }

        token_end
    }

    /// Emit all pending comments from `all_comments` whose end position is before `pos`.
    /// Uses the `comment_emit_idx` cursor to advance through comments.
    /// Similar to the top-level statement comment emission logic.
    pub(super) fn emit_comments_before_pos(&mut self, pos: u32) {
        if self.ctx.options.remove_comments {
            return;
        }
        let actual_start = self.skip_whitespace_forward(pos, pos + 1024);
        if let Some(text) = self.source_text {
            while self.comment_emit_idx < self.all_comments.len() {
                let c_end = self.all_comments[self.comment_emit_idx].end;
                if c_end <= actual_start {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                    let comment_text =
                        crate::printer::safe_slice::slice(text, c_pos as usize, c_end as usize);
                    self.write_comment(comment_text);
                    if c_trailing {
                        self.write_line();
                    }
                    self.comment_emit_idx += 1;
                } else {
                    break;
                }
            }
        }
    }

    /// Write comment text, trimming trailing whitespace from each line of multi-line comments.
    /// TypeScript strips trailing whitespace from multi-line comment lines in its emitter.
    pub(super) fn write_comment(&mut self, text: &str) {
        if text.contains('\n') {
            // Multi-line comment: trim trailing whitespace from each line
            let mut first = true;
            for line in text.split('\n') {
                if !first {
                    self.write("\n");
                }
                self.write(line.trim_end());
                first = false;
            }
        } else {
            self.write(text);
        }
    }

    /// Collect leading comment texts for a node at the given position.
    /// Returns the text of comments whose end is before `pos` and that haven't been emitted yet.
    /// Does NOT advance the comment index — use this before `skip_comments_for_erased_node`.
    pub(super) fn collect_leading_comments(&self, pos: u32) -> Vec<String> {
        if self.ctx.options.remove_comments {
            return Vec::new();
        }
        let Some(text) = self.source_text else {
            return Vec::new();
        };
        let actual_start = self.skip_whitespace_forward(pos, pos + 1024);
        let mut result = Vec::new();
        let mut idx = self.comment_emit_idx;
        while idx < self.all_comments.len() {
            let c = &self.all_comments[idx];
            if c.end <= actual_start {
                let comment_text =
                    crate::printer::safe_slice::slice(text, c.pos as usize, c.end as usize);
                result.push(comment_text.to_string());
                idx += 1;
            } else {
                break;
            }
        }
        result
    }

    /// Skip (suppress) all comments that belong to an erased declaration (interface, type alias).
    /// Advances `comment_emit_idx` past any comments whose end position falls within the node's range,
    /// including trailing same-line comments (e.g. `// ERROR` after a constructor overload).
    pub(super) fn skip_comments_for_erased_node(&mut self, node: &Node) {
        // Find the actual end of the node's code content, excluding trailing trivia.
        let actual_end = self.find_token_end_before_trivia(node.pos, node.end);

        // Also find the end of the line containing the node's last token.
        // This lets us skip trailing same-line comments (like `// ERROR` after `;`)
        // that are beyond node.end but logically belong to the erased construct.
        let line_end = if let Some(text) = self.source_text {
            let bytes = text.as_bytes();
            let mut pos = actual_end as usize;
            while pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                pos += 1;
            }
            pos as u32
        } else {
            actual_end
        };

        while self.comment_emit_idx < self.all_comments.len() {
            let c = &self.all_comments[self.comment_emit_idx];
            if c.end <= line_end {
                self.comment_emit_idx += 1;
            } else {
                break;
            }
        }
    }
}
