use super::ES5ClassTransformer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;

impl<'a> ES5ClassTransformer<'a> {
    /// Extract leading `JSDoc` comment from a node (if any).
    /// Returns the comment text including the `/** ... */` delimiters.
    ///
    /// Scans backward from `node.pos` (the token start, not including trivia)
    /// looking for an immediately adjacent block comment separated only by
    /// whitespace.  This avoids the pitfall of the old forward-scan approach
    /// which was confused when `node.end` of the previous sibling included
    /// the current member's trivia.
    pub(super) fn extract_leading_comment(&self, node: &Node) -> Option<String> {
        let source_text = self.source_text?;
        let bytes = source_text.as_bytes();
        let pos = node.pos as usize;
        if pos == 0 {
            return None;
        }

        // Scan backward from `pos` skipping whitespace/newlines.
        // If we find `*/` we look further back for the matching `/*`.
        let mut i = pos;
        // Skip trailing whitespace/newlines before the token
        while i > 0 && matches!(bytes[i - 1], b' ' | b'\t' | b'\r' | b'\n') {
            i -= 1;
        }

        // Check if we landed on `*/` (end of a block comment)
        if i >= 2 && bytes[i - 1] == b'/' && bytes[i - 2] == b'*' {
            let comment_end = i; // exclusive end of comment text
            // Scan backwards to find the matching `/*`
            // We look for the LAST `/*` before this position that is a true
            // comment opener (not inside a string — simplified scan).
            let mut j = i - 2; // j points at `*` of `*/`
            loop {
                if j < 2 {
                    break;
                }
                // Look for `/*` or `/**`
                if bytes[j - 1] == b'/' && bytes[j] == b'*' {
                    // Found `/*` at j-1..j+1
                    let comment_start = j - 1;
                    let comment_text = &source_text[comment_start..comment_end];
                    if comment_text.starts_with("/**") && !comment_text.starts_with("/***") {
                        return Some(comment_text.to_string());
                    }
                    if comment_text.starts_with("/*") {
                        return Some(comment_text.to_string());
                    }
                    break;
                }
                j -= 1;
            }
        }

        // Check for line comment (`// ...`).
        // At this point `i` is just past the last non-whitespace char before the node.
        // Scan backward to find the start of that line, then check for `//`.
        if i > 0 {
            let line_end = i;
            let mut line_start = i;
            while line_start > 0 && bytes[line_start - 1] != b'\n' {
                line_start -= 1;
            }
            let line = source_text[line_start..line_end].trim_start();
            if line.starts_with("//") {
                return Some(line.to_string());
            }
        }

        None
    }

    /// Extract trailing comment on the same line as a class method's closing `}`.
    ///
    /// Finds the first `}` at brace depth 0 within the body block — that is, the
    /// actual closing brace of the function body — and returns any trailing comment
    /// on the same line.  Previous code scanned the entire body range and picked the
    /// LAST `}` with a trailing comment, which could accidentally pick up the class's
    /// closing brace comment instead of the method's own comment.
    pub(super) fn extract_trailing_comment_for_method(
        &self,
        body_idx: NodeIndex,
    ) -> Option<String> {
        let source_text = self.source_text?;
        let close_brace = self.body_closing_brace_pos(body_idx)?;
        crate::emitter::get_trailing_comment_ranges(source_text, close_brace + 1)
            .first()
            .map(|c| source_text[c.pos as usize..c.end as usize].to_string())
    }

    pub(super) fn body_closing_brace_pos(&self, body_idx: NodeIndex) -> Option<usize> {
        let source_text = self.source_text?;
        let body_node = self.arena.get(body_idx)?;
        let bytes = source_text.as_bytes();
        let start = body_node.pos as usize;
        let end = (body_node.end as usize).min(bytes.len());
        if start >= end {
            return None;
        }
        // Track brace depth starting from the opening `{` of the block.
        // We skip the initial opening brace (depth stays 0 initially).
        // For each `{` after that, depth increments; for each `}`, if depth==0
        // we have found the matching closing brace of the block; otherwise decrement.
        let mut depth: usize = 0;
        let mut in_string: Option<u8> = None; // `'` or `"`
        let mut i = start;
        while i < end {
            let byte = bytes[i];
            // Rudimentary string/template literal skip to avoid counting braces inside strings
            if in_string.is_none() {
                match byte {
                    b'{' => {
                        // Skip the opening brace of the body block itself (depth stays 0)
                        if i == start {
                            // opening brace of the block — don't count
                        } else {
                            depth += 1;
                        }
                    }
                    b'}' => {
                        if depth == 0 {
                            return Some(i);
                        }
                        depth -= 1;
                    }
                    b'\'' | b'"' | b'`' => {
                        in_string = Some(byte);
                    }
                    _ => {}
                }
            } else if let Some(delim) = in_string {
                if byte == b'\\' {
                    i += 1; // skip escaped char
                } else if byte == delim {
                    in_string = None;
                }
            }
            i += 1;
        }
        None
    }

    pub(super) fn extract_trailing_comment_for_node(&self, node: &Node) -> Option<String> {
        let source_text = self.source_text?;
        for comment in crate::emitter::get_trailing_comment_ranges(source_text, node.end as usize) {
            let comment_text = &source_text[comment.pos as usize..comment.end as usize];
            let trimmed = comment_text.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                return Some(comment_text.to_string());
            }
        }

        None
    }

    pub(super) fn extract_trailing_comment_for_class_field(&self, node: &Node) -> Option<String> {
        if let Some(comment) = self.extract_trailing_comment_for_node(node) {
            return Some(comment);
        }

        let source_text = self.source_text?;
        let start = node.pos as usize;
        let end = (node.end as usize).min(source_text.len());
        if start >= end {
            return None;
        }

        let line_end = source_text[end..]
            .find(['\n', '\r'])
            .map_or(source_text.len(), |offset| end + offset);
        let mut after_field = end;
        while after_field < line_end {
            let ch = source_text[after_field..].chars().next()?;
            if ch.is_whitespace() {
                after_field += ch.len_utf8();
                continue;
            }
            if ch == ';' {
                for comment in
                    crate::emitter::get_trailing_comment_ranges(source_text, after_field + 1)
                {
                    let comment_text = &source_text[comment.pos as usize..comment.end as usize];
                    let trimmed = comment_text.trim_start();
                    if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                        return Some(comment_text.to_string());
                    }
                }
            }
            break;
        }

        for comment in tsz_common::comments::get_comment_ranges(&source_text[start..end]) {
            let comment_pos = start + comment.pos as usize;
            let comment_end = start + comment.end as usize;
            let line_start = source_text[..comment_pos]
                .rfind(['\n', '\r'])
                .map_or(0, |pos| pos + 1);
            if !source_text[line_start..comment_pos].trim().is_empty() {
                return Some(source_text[comment_pos..comment_end].to_string());
            }
        }

        None
    }
}
