use crate::emitter::Printer;
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
    pub(in crate::emitter) fn emit_trailing_comments(&mut self, end_pos: u32) {
        self.emit_trailing_comments_impl(end_pos, u32::MAX);
    }

    /// Like `emit_trailing_comments` but only emit comments whose start position
    /// is before `max_pos`. Used to prevent a statement inside a block from
    /// consuming comments that belong on the block's closing line.
    pub(in crate::emitter) fn emit_trailing_comments_before(&mut self, end_pos: u32, max_pos: u32) {
        self.emit_trailing_comments_impl(end_pos, max_pos);
    }

    /// Check if there is at least one unconsumed trailing comment on the same
    /// source line as `end_pos`, without advancing `comment_emit_idx`.
    pub(in crate::emitter) fn has_trailing_comment_on_same_line(
        &self,
        end_pos: u32,
        max_pos: u32,
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let bytes = text.as_bytes();
        let mut idx = self.comment_emit_idx;
        while idx < self.all_comments.len() {
            let c_pos = self.all_comments[idx].pos;
            if c_pos < end_pos {
                let gap_end = std::cmp::min(end_pos as usize, bytes.len());
                if bytes[c_pos as usize..gap_end]
                    .iter()
                    .any(|&b| b == b'\n' || b == b'\r')
                {
                    return false;
                }
                idx += 1;
                continue;
            }
            if c_pos >= max_pos {
                return false;
            }
            let gap_start = end_pos as usize;
            let gap_end = std::cmp::min(c_pos as usize, bytes.len());
            let has_line_break = bytes[gap_start..gap_end]
                .iter()
                .any(|&b| b == b'\n' || b == b'\r');
            return !has_line_break;
        }
        false
    }

    fn emit_trailing_comments_impl(&mut self, end_pos: u32, max_pos: u32) {
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

            // Don't consume comments past the max position boundary
            if c_pos >= max_pos {
                break;
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
            if let Ok(comment_text) = crate::safe_slice::slice(text, c_pos as usize, c_end as usize)
                && !comment_text.is_empty()
            {
                self.write_comment_with_reindent(comment_text, Some(c_pos));
            }
            self.comment_emit_idx += 1;
        }
    }

    /// Advance `comment_emit_idx` past same-line trailing comments after `end_pos`
    /// without emitting them. Used to suppress comments on function body opening
    /// braces — tsc drops these but preserves them on control-flow blocks.
    pub(in crate::emitter) fn skip_trailing_same_line_comments(
        &mut self,
        end_pos: u32,
        max_pos: u32,
    ) {
        let Some(text) = self.source_text else {
            return;
        };
        let bytes = text.as_bytes();
        while self.comment_emit_idx < self.all_comments.len() {
            let c_pos = self.all_comments[self.comment_emit_idx].pos;

            // Comment before end_pos on the same line — skip it
            if c_pos < end_pos {
                let gap_end = std::cmp::min(end_pos as usize, bytes.len());
                if bytes[c_pos as usize..gap_end]
                    .iter()
                    .any(|&b| b == b'\n' || b == b'\r')
                {
                    break;
                }
                self.comment_emit_idx += 1;
                continue;
            }

            // Don't consume comments past the max position boundary
            if c_pos >= max_pos {
                break;
            }

            // Check for line break between end_pos and comment
            let gap_start = end_pos as usize;
            let gap_end = std::cmp::min(c_pos as usize, bytes.len());
            let has_line_break = bytes[gap_start..gap_end]
                .iter()
                .any(|&b| b == b'\n' || b == b'\r');
            if has_line_break {
                break;
            }

            // Same-line trailing comment — skip without emitting
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
    pub(in crate::emitter) fn find_token_end_before_trivia(&self, pos: u32, end: u32) -> u32 {
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
                    // Track end of string as a non-trivia position so that
                    // standalone string expression statements (ASI, no `;`)
                    // get the correct token_end for trailing comment detection.
                    if depth == 0 {
                        last_non_trivia_at_depth0 = Some(i);
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
        // Only apply the suffix scan when we didn't find meaningful code
        // content in the main scan (i.e., both token_end trackers are None).
        // When `last_non_trivia_at_depth0` was found, the main scan already
        // located the correct boundary and the suffix scan would overshoot
        // into parent closing braces (e.g., `}` of an object literal after
        // the last property).
        if last_token_end.is_none() && last_non_trivia_at_depth0.is_none() && token_end <= end {
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

    pub(in crate::emitter) fn find_block_opening_brace_pos(&self, node: &Node) -> Option<u32> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let end = std::cmp::min(node.end as usize, bytes.len());
        let mut pos = self.skip_trivia_forward(node.pos, node.end) as usize;

        while pos < end {
            match bytes[pos] {
                b'{' => return Some(pos as u32),
                b' ' | b'\t' | b'\r' | b'\n' => pos += 1,
                _ => break,
            }
        }

        bytes[pos..end]
            .iter()
            .position(|&b| b == b'{')
            .map(|offset| (pos + offset) as u32)
    }

    pub(in crate::emitter) fn find_block_closing_brace_end(&self, node: &Node) -> u32 {
        let Some(text) = self.source_text else {
            return self.find_token_end_before_trivia(node.pos, node.end);
        };
        let bytes = text.as_bytes();
        let end = std::cmp::min(node.end as usize, bytes.len());
        let Some(open_pos) = self.find_block_opening_brace_pos(node) else {
            return self.find_token_end_before_trivia(node.pos, node.end);
        };

        let mut depth: i32 = 0;
        let mut pos = open_pos as usize;
        while pos < end {
            match bytes[pos] {
                b'{' => {
                    depth += 1;
                    pos += 1;
                }
                b'}' => {
                    depth -= 1;
                    pos += 1;
                    if depth == 0 {
                        return pos as u32;
                    }
                    if depth < 0 {
                        break;
                    }
                }
                b'\'' | b'"' | b'`' => {
                    let quote = bytes[pos];
                    pos += 1;
                    while pos < end {
                        if bytes[pos] == b'\\' {
                            pos += 2;
                        } else if bytes[pos] == quote {
                            pos += 1;
                            break;
                        } else {
                            pos += 1;
                        }
                    }
                }
                b'/' if pos + 1 < end && bytes[pos + 1] == b'/' => {
                    pos += 2;
                    while pos < end && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                        pos += 1;
                    }
                }
                b'/' if pos + 1 < end && bytes[pos + 1] == b'*' => {
                    pos += 2;
                    while pos + 1 < end {
                        if bytes[pos] == b'*' && bytes[pos + 1] == b'/' {
                            pos += 2;
                            break;
                        }
                        pos += 1;
                    }
                }
                _ => pos += 1,
            }
        }

        self.find_token_end_before_trivia(node.pos, node.end)
    }

    /// Check if there are pending comments whose end is before `pos` without
    /// advancing the cursor. Used to conditionally emit whitespace before
    /// inline comments (e.g., `( /* comment */expr`).
    pub(in crate::emitter) fn has_pending_comment_before(&self, pos: u32) -> bool {
        if self.ctx.options.remove_comments {
            return false;
        }
        if self.comment_emit_idx >= self.all_comments.len() {
            return false;
        }
        let actual_start = self.skip_trivia_forward(pos, pos + 1024);
        self.all_comments[self.comment_emit_idx].end <= actual_start
    }

    /// Emit all pending comments from `all_comments` whose end position is before `pos`.
    /// Uses the `comment_emit_idx` cursor to advance through comments.
    /// Similar to the top-level statement comment emission logic.
    pub(in crate::emitter) fn emit_comments_before_pos(&mut self, pos: u32) {
        if self.ctx.options.remove_comments {
            return;
        }
        let actual_start = self.skip_trivia_forward(pos, pos + 1024);
        if let Some(text) = self.source_text {
            while self.comment_emit_idx < self.all_comments.len() {
                let c_end = self.all_comments[self.comment_emit_idx].end;
                if c_end <= actual_start {
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                    if let Ok(comment_text) =
                        crate::safe_slice::slice(text, c_pos as usize, c_end as usize)
                    {
                        self.write_comment_with_reindent(comment_text, Some(c_pos));
                        if c_trailing {
                            self.write_line();
                        } else if comment_text.starts_with("/*") {
                            self.pending_block_comment_space = true;
                        }
                    }
                    self.comment_emit_idx += 1;
                } else {
                    break;
                }
            }
        }
    }

    /// Emit comments written between a `...` token and the following operand
    /// or binding name. TSC keeps these comments glued to the spread/rest token
    /// instead of treating them as ordinary leading comments on the operand.
    pub(in crate::emitter) fn emit_comments_after_dot_dot_dot(
        &mut self,
        dot_dot_dot_pos: u32,
        target_pos: u32,
        preserve_newline_before_comment: bool,
    ) -> bool {
        if self.ctx.options.remove_comments {
            return false;
        }

        let Some(text) = self.source_text else {
            return false;
        };

        let token_start = self.skip_trivia_forward(dot_dot_dot_pos, target_pos);
        let token_end = token_start.saturating_add(3);
        let mut scan_idx = self.comment_emit_idx;
        while scan_idx < self.all_comments.len() && self.all_comments[scan_idx].end <= token_end {
            scan_idx += 1;
        }
        if scan_idx >= self.all_comments.len() {
            return false;
        }

        let first = &self.all_comments[scan_idx];
        if first.pos < token_end || first.end > target_pos {
            return false;
        }

        let bytes = text.as_bytes();
        let has_newline_before_first = bytes
            .get(token_end as usize..std::cmp::min(first.pos as usize, bytes.len()))
            .is_some_and(|gap| gap.iter().any(|&b| b == b'\n' || b == b'\r'));

        if preserve_newline_before_comment && has_newline_before_first {
            self.write_line();
        } else {
            self.write_space();
        }

        let first_comment_started_on_later_line = has_newline_before_first;
        while scan_idx < self.all_comments.len() {
            let comment = &self.all_comments[scan_idx];
            if comment.pos < token_end || comment.end > target_pos {
                break;
            }

            let c_pos = comment.pos;
            let c_end = comment.end;
            if let Ok(comment_text) = crate::safe_slice::slice(text, c_pos as usize, c_end as usize)
                && !comment_text.is_empty()
            {
                self.write_comment_with_reindent(comment_text, Some(c_pos));
            }
            self.comment_emit_idx = scan_idx + 1;
            scan_idx += 1;

            let next_pos = if scan_idx < self.all_comments.len()
                && self.all_comments[scan_idx].pos >= token_end
                && self.all_comments[scan_idx].end <= target_pos
            {
                self.all_comments[scan_idx].pos
            } else {
                target_pos
            };
            let has_newline_after = bytes
                .get(c_end as usize..std::cmp::min(next_pos as usize, bytes.len()))
                .is_some_and(|gap| gap.iter().any(|&b| b == b'\n' || b == b'\r'));

            if next_pos == target_pos {
                if preserve_newline_before_comment
                    && first_comment_started_on_later_line
                    && has_newline_after
                {
                    self.write_line();
                } else if preserve_newline_before_comment && first_comment_started_on_later_line {
                    self.write_space();
                }
            } else if preserve_newline_before_comment && has_newline_after {
                self.write_line();
            } else {
                self.write_space();
            }
        }

        self.pending_block_comment_space = false;
        true
    }

    /// Check (without advancing the cursor) whether there is a comment in
    /// `[start_pos, end_pos)` that introduces a newline break. This is used
    /// to detect cases where a line comment between a keyword (`yield`) and
    /// its operand would trigger ASI, requiring the operand to be wrapped in
    /// parentheses.
    pub(in crate::emitter) fn has_newline_comment_in_range(
        &self,
        start_pos: u32,
        end_pos: u32,
    ) -> bool {
        if self.ctx.options.remove_comments {
            return false;
        }
        let mut idx = self.comment_emit_idx;
        while idx < self.all_comments.len() {
            let comment = &self.all_comments[idx];
            if comment.pos >= end_pos {
                break;
            }
            if comment.end > start_pos && comment.has_trailing_new_line {
                return true;
            }
            idx += 1;
        }
        false
    }

    /// Emit all comments whose span lies within `[start_pos, end_pos)`.
    /// When `insert_space_for_adjacent_inline` is true, a space is emitted before
    /// same-line inline comments when the comment starts immediately after source
    /// code (useful for JSX expression trailing comments).
    /// When `normalize_leading_text` is true, indentation between line breaks in
    /// the text leading up to a comment is normalized using the current writer
    /// indentation instead of preserved byte-for-byte.
    ///
    /// Returns:
    /// (`did_emit_comment`, `last_comment_end_pos`, `last_comment_had_trailing_newline`)
    pub(in crate::emitter) fn emit_comments_in_range(
        &mut self,
        start_pos: u32,
        end_pos: u32,
        insert_space_for_adjacent_inline: bool,
        normalize_leading_text: bool,
    ) -> (bool, u32, bool) {
        if self.ctx.options.remove_comments {
            return (false, 0, false);
        }

        let Some(text) = self.source_text else {
            return (false, 0, false);
        };

        let mut emitted_any = false;
        let mut last_comment_end = 0u32;
        let mut last_comment_had_trailing_newline = false;
        // When normalizing JSX comment leading text, trim any leading
        // horizontal whitespace before the first comment (e.g. `{ // x}` → `{// x}`).
        let mut previous_comment_had_trailing_newline = normalize_leading_text;
        let mut cursor_pos = start_pos as usize;

        while self.comment_emit_idx < self.all_comments.len() {
            let (comment_pos, comment_end, comment_has_new_line) = {
                let comment = &self.all_comments[self.comment_emit_idx];
                (comment.pos, comment.end, comment.has_trailing_new_line)
            };

            if comment_pos >= end_pos {
                break;
            }

            if comment_end <= start_pos {
                self.comment_emit_idx += 1;
                continue;
            }

            let comment_pos_usize = comment_pos as usize;
            if comment_pos_usize > cursor_pos {
                // Best-effort: on a bad span we skip emitting leading trivia
                // (it's a compiler bug; surfaces via tracing::debug! in slice).
                if let Ok(leading_text) =
                    crate::safe_slice::slice(text, cursor_pos, comment_pos_usize)
                {
                    if normalize_leading_text {
                        self.write_normalized_jsx_comment_leading_text(
                            leading_text,
                            previous_comment_had_trailing_newline,
                        );
                    } else {
                        self.write(leading_text);
                    }
                }
            } else if insert_space_for_adjacent_inline
                && !comment_has_new_line
                && !self.comment_preceded_by_newline(comment_pos)
            {
                self.write_space();
            }

            if let Ok(comment_text) =
                crate::safe_slice::slice(text, comment_pos as usize, comment_end as usize)
            {
                self.write_comment_with_reindent(comment_text, Some(comment_pos));
            }
            if comment_has_new_line {
                self.write_line();
                cursor_pos = comment_end as usize;
                if let Some(next) = text.as_bytes().get(comment_end as usize..) {
                    if next.starts_with(b"\r\n") {
                        cursor_pos += 2;
                    } else if matches!(next.first(), Some(b'\n' | b'\r')) {
                        cursor_pos += 1;
                    }
                }
            } else {
                cursor_pos = comment_end as usize;
            }

            emitted_any = true;
            last_comment_end = comment_end;
            last_comment_had_trailing_newline = comment_has_new_line;
            self.comment_emit_idx += 1;
            previous_comment_had_trailing_newline = comment_has_new_line;
        }

        (
            emitted_any,
            last_comment_end,
            last_comment_had_trailing_newline,
        )
    }

    fn write_normalized_jsx_comment_leading_text(
        &mut self,
        text: &str,
        trim_leading_line_whitespace: bool,
    ) {
        if text.is_empty() {
            return;
        }
        let bytes = text.as_bytes();
        let mut cursor = 0usize;
        if trim_leading_line_whitespace {
            while cursor < bytes.len() && (bytes[cursor] == b' ' || bytes[cursor] == b'\t') {
                cursor += 1;
            }
        }
        while cursor < bytes.len() {
            if bytes[cursor] == b'\n' || bytes[cursor] == b'\r' {
                self.write_line();
                if bytes[cursor] == b'\r' && bytes.get(cursor + 1) == Some(&b'\n') {
                    cursor += 2;
                } else {
                    cursor += 1;
                }
                while cursor < bytes.len() && (bytes[cursor] == b' ' || bytes[cursor] == b'\t') {
                    cursor += 1;
                }
                continue;
            }

            let mut end = cursor + 1;
            while end < bytes.len() && bytes[end] != b'\n' && bytes[end] != b'\r' {
                end += 1;
            }
            self.write(&text[cursor..end]);
            cursor = end;
        }
    }

    /// Compute the number of leading whitespace chars on the line containing `pos`.
    /// Used by `write_comment_with_reindent` to determine how much source indentation
    /// to strip from multi-line comment continuation lines.
    fn source_column_at(&self, pos: u32) -> u32 {
        let Some(text) = self.source_text else {
            return 0;
        };
        let bytes = text.as_bytes();
        let pos = pos as usize;
        if pos == 0 || pos > bytes.len() {
            return 0;
        }
        // Scan backwards to find the start of the line
        let mut i = pos;
        while i > 0 {
            i -= 1;
            if bytes[i] == b'\n' || bytes[i] == b'\r' {
                break;
            }
        }
        let line_start = if i == 0 && bytes[0] != b'\n' && bytes[0] != b'\r' {
            0
        } else {
            i + 1
        };
        // Count leading whitespace chars on this line
        let mut ws = 0;
        for &b in &bytes[line_start..pos] {
            if b == b' ' || b == b'\t' {
                ws += 1;
            } else {
                break;
            }
        }
        ws as u32
    }

    /// Write comment text, trimming trailing whitespace from each line of multi-line comments.
    /// TypeScript strips trailing whitespace from multi-line comment lines in its emitter.
    pub(in crate::emitter) fn write_comment(&mut self, text: &str) {
        self.write_comment_with_reindent(text, None);
    }

    /// Write comment text with optional reindentation for multi-line comments.
    /// When `source_pos` is provided, computes the source column of the comment and
    /// reindents continuation lines to match the current output indentation level,
    /// matching tsc's behavior of adjusting multi-line comment indentation.
    pub(in crate::emitter) fn write_comment_with_reindent(
        &mut self,
        text: &str,
        source_pos: Option<u32>,
    ) {
        if text.contains('\n') {
            // Multi-line comment: reindent continuation lines.
            // tsc computes the indent of the line containing the opening /*, then
            // strips that many leading whitespace chars from each continuation line,
            // letting the output indentation system re-add the correct amount.
            let source_indent = source_pos
                .map(|pos| self.source_column_at(pos))
                .unwrap_or(0) as usize;

            let mut first = true;
            for line in text.split('\n') {
                if first {
                    // First line (starts at /*): write as-is, indentation is already
                    // handled by ensure_indent() from the caller's context
                    self.write(line.trim_end());
                    first = false;
                } else {
                    // Continuation line: use write_line() to properly trigger
                    // ensure_indent() on the next write, then strip source-level
                    // indentation and write the rest
                    self.write_line();
                    let trimmed = strip_leading_whitespace(line.trim_end(), source_indent);
                    self.write(trimmed);
                }
            }
        } else {
            self.write(text);
        }
    }

    /// Collect trailing same-line comment texts after a code position.
    /// Scans `all_comments` from the beginning (not `comment_emit_idx`) to find
    /// comments on the same line as `actual_end`. Does NOT advance the cursor.
    /// Used during pre-scan phases where `comment_emit_idx` may not have
    /// advanced to the relevant position yet.
    pub(in crate::emitter) fn collect_trailing_comments_in_range(
        &self,
        actual_end: u32,
    ) -> Vec<String> {
        let Some(text) = self.source_text else {
            return Vec::new();
        };
        let bytes = text.as_bytes();
        // Find line end from actual_end
        let mut line_end_pos = actual_end as usize;
        while line_end_pos < bytes.len()
            && bytes[line_end_pos] != b'\n'
            && bytes[line_end_pos] != b'\r'
        {
            line_end_pos += 1;
        }
        let line_end = line_end_pos as u32;

        let mut trailing = Vec::new();
        for c in &self.all_comments {
            if c.pos >= actual_end
                && c.end <= line_end
                && let Ok(comment_text) =
                    crate::safe_slice::slice(text, c.pos as usize, c.end as usize)
            {
                trailing.push(comment_text.to_string());
            }
            if c.pos > line_end {
                break;
            }
        }
        trailing
    }

    /// Collect leading comment texts in a source range.
    /// Scans `all_comments` from the beginning (not `comment_emit_idx`) to find
    /// comments between `range_start` and the actual token start of `node_pos`
    /// (skipping trivia).  Does NOT advance the cursor.
    /// Used during pre-scan phases where `comment_emit_idx` may not have
    /// advanced to the relevant position yet.
    pub(in crate::emitter) fn collect_leading_comments_in_range(
        &self,
        range_start: u32,
        node_pos: u32,
    ) -> Vec<String> {
        if self.ctx.options.remove_comments {
            return Vec::new();
        }
        let Some(text) = self.source_text else {
            return Vec::new();
        };
        // node_pos is typically the start of leading trivia; skip past trivia
        // to find the actual token start so we can find comments within the trivia.
        let actual_start = self.skip_trivia_forward(node_pos, node_pos + 2048);
        let mut result = Vec::new();
        let bytes = text.as_bytes();
        for c in &self.all_comments {
            if c.pos >= range_start && c.end <= actual_start {
                // Only collect block comments (/* ... */), not line comments (// ...).
                // Line comments between members are trailing comments of the previous
                // member (e.g. `get p() { ... } // error`), not leading comments of
                // the next property being lowered into the constructor.
                if c.pos as usize + 1 < bytes.len()
                    && bytes[c.pos as usize] == b'/'
                    && bytes[c.pos as usize + 1] == b'*'
                    && let Ok(comment_text) =
                        crate::safe_slice::slice(text, c.pos as usize, c.end as usize)
                {
                    result.push(comment_text.to_string());
                }
            }
            if c.pos >= actual_start {
                break;
            }
        }
        result
    }

    /// Collect leading comment texts for a node at the given position.
    /// Returns (text, `source_pos`) tuples for comments whose end is before `pos`
    /// and that haven't been emitted yet.
    /// Does NOT advance the comment index — use this before `skip_comments_for_erased_node`.
    pub(in crate::emitter) fn collect_leading_comments(&self, pos: u32) -> Vec<(String, u32)> {
        if self.ctx.options.remove_comments {
            return Vec::new();
        }
        let Some(text) = self.source_text else {
            return Vec::new();
        };
        let actual_start = self.skip_trivia_forward(pos, pos + 1024);
        let mut result = Vec::new();
        let mut idx = self.comment_emit_idx;
        while idx < self.all_comments.len() {
            let c = &self.all_comments[idx];
            if c.end <= actual_start {
                if let Ok(comment_text) =
                    crate::safe_slice::slice(text, c.pos as usize, c.end as usize)
                {
                    result.push((comment_text.to_string(), c.pos));
                }
                idx += 1;
            } else {
                break;
            }
        }
        result
    }

    /// Skip (suppress) all comments within a source range.
    /// Used to consume comments inside erased syntax regions like type parameter lists.
    ///
    /// Narrows the end boundary using `find_token_end_before_trivia` because our
    /// parser's `node.end` extends past trailing trivia into the next token's
    /// position. Without narrowing, comments in the trailing trivia (which logically
    /// belong to the next statement) would be consumed.
    pub(in crate::emitter) fn skip_comments_in_range(&mut self, start: u32, end: u32) {
        let actual_end = self.find_token_end_before_trivia(start, end);
        while self.comment_emit_idx < self.all_comments.len() {
            let c = &self.all_comments[self.comment_emit_idx];
            if c.pos >= start && c.end <= actual_end {
                self.comment_emit_idx += 1;
            } else {
                break;
            }
        }
    }

    /// Skip (suppress) all comments that belong to an erased declaration (interface, type alias).
    /// Advances `comment_emit_idx` past any comments whose end position falls within the node's range,
    /// including trailing same-line comments (e.g. `// ERROR` after a constructor overload).
    ///
    /// Crucially, this does NOT consume comments that follow other code tokens on the
    /// same line. For example, in `class C { foo: string; } // error`, when erasing
    /// `foo: string;`, the `// error` comment belongs to the closing `}`, not to the
    /// erased member. We detect this by checking whether any non-whitespace code token
    /// exists between the erased node's end and the comment start.
    pub(in crate::emitter) fn skip_comments_for_erased_node(&mut self, node: &Node) {
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

        // Use node.end as the gap-check anchor, not actual_end. The
        // `find_token_end_before_trivia` suffix scan can overshoot past node.end
        // (e.g., finding a parent `}` beyond the member), so node.end is the safer
        // boundary for deciding whether intervening code separates the erased node
        // from a trailing comment.
        let gap_anchor = node.end;
        let source_bytes = self.source_text.map(|t| t.as_bytes());

        while self.comment_emit_idx < self.all_comments.len() {
            let c = &self.all_comments[self.comment_emit_idx];
            if c.end <= line_end {
                // For comments that start at or after the erased node's end,
                // check if there's any non-whitespace code token between the node
                // boundary and the comment. If so, the comment belongs to that code
                // (e.g., a closing `}`), not to the erased node — don't consume it.
                if c.pos >= gap_anchor
                    && let Some(bytes) = source_bytes
                {
                    let gap_start = gap_anchor as usize;
                    let gap_end = std::cmp::min(c.pos as usize, bytes.len());
                    let has_code_between = bytes[gap_start..gap_end]
                        .iter()
                        .any(|&b| !matches!(b, b' ' | b'\t'));
                    if has_code_between {
                        break;
                    }
                }
                self.comment_emit_idx += 1;
            } else {
                break;
            }
        }
    }
}

/// Strip up to `count` leading whitespace characters from a string.
/// This mirrors tsc's behavior of removing the source-level indentation from
/// multi-line comment continuation lines before the output indentation is applied.
fn strip_leading_whitespace(s: &str, count: usize) -> &str {
    let bytes = s.as_bytes();
    let mut stripped = 0;
    for &b in bytes.iter().take(count) {
        if b == b' ' || b == b'\t' {
            stripped += 1;
        } else {
            break;
        }
    }
    &s[stripped..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_leading_whitespace_basic() {
        assert_eq!(strip_leading_whitespace("   * @type", 2), " * @type");
        assert_eq!(strip_leading_whitespace("   * @type", 3), "* @type");
        assert_eq!(strip_leading_whitespace("   * @type", 0), "   * @type");
    }

    #[test]
    fn test_strip_leading_whitespace_strips_up_to_count() {
        // When count exceeds available whitespace, only strip actual whitespace
        assert_eq!(strip_leading_whitespace(" * foo", 4), "* foo");
        assert_eq!(strip_leading_whitespace("* foo", 4), "* foo");
    }

    #[test]
    fn test_strip_leading_whitespace_stops_at_non_whitespace() {
        // Non-whitespace characters stop the stripping even within count
        assert_eq!(strip_leading_whitespace("abc", 3), "abc");
        assert_eq!(strip_leading_whitespace("  abc", 4), "abc");
    }

    #[test]
    fn test_strip_leading_whitespace_tabs() {
        assert_eq!(strip_leading_whitespace("\t\t* foo", 2), "* foo");
        assert_eq!(strip_leading_whitespace("\t * foo", 1), " * foo");
    }

    #[test]
    fn test_strip_leading_whitespace_empty() {
        assert_eq!(strip_leading_whitespace("", 3), "");
        assert_eq!(strip_leading_whitespace("   ", 2), " ");
    }
}
