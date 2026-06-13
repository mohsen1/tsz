//! Editing utility handlers for tsz-server.
//!
//! Handles commands related to editing assists: breakpoints, JSX closing tags,
//! brace completion, comments, doc templates, indentation, classifications, etc.

use super::{Server, TsServerRequest, TsServerResponse};
use tsz::lsp::position::LineMap;

impl Server {
    pub(crate) fn handle_breakpoint_statement(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64().unwrap_or(1) as u32;
            let source_text = self.open_files.get(file)?;
            let line_map = LineMap::build(source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let byte_offset = line_map.position_to_offset(position, source_text)? as usize;

            // Find the statement that contains this position.
            // Walk through lines to find the line's content and determine if it's a
            // valid breakpoint target (non-empty, non-comment-only, non-declaration-only).
            let line_start = source_text[..byte_offset].rfind('\n').map_or(0, |i| i + 1);
            let line_end = source_text[byte_offset..]
                .find('\n')
                .map_or(source_text.len(), |i| byte_offset + i);
            let line_text = source_text[line_start..line_end].trim();

            // Skip empty lines and pure comment lines
            if line_text.is_empty()
                || line_text.starts_with("//")
                || line_text.starts_with("/*")
                || line_text == "*/"
                || line_text.starts_with('*')
            {
                return None;
            }

            // Skip lines that are only closing braces/brackets
            if line_text == "}" || line_text == "};" || line_text == "]" || line_text == "];" {
                // These are valid breakpoint targets in TypeScript
            }

            // Return the text span for the whole line content
            let content_start = line_start
                + source_text[line_start..line_end]
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .count();
            let content_end = line_end;

            Some(Self::text_span_body(content_start, content_end))
        })();

        self.success_response(seq, request, result)
    }

    pub(crate) fn handle_jsx_closing_tag(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // jsxClosingTag returns { newText: string } or undefined
        // The request fires when the user types '>' after a JSX tag name,
        // and we should return the closing tag text (e.g., "</div>").
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64().unwrap_or(1) as u32;

            let source_text = self.open_files.get(file)?;
            let line_map = LineMap::build(source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let byte_offset = line_map.position_to_offset(position, source_text)? as usize;

            // The cursor is right after '>'. Look backward to find the tag name.
            // We need to find a pattern like '<TagName ...>' ending at byte_offset.
            if byte_offset == 0 {
                return None;
            }

            let bytes = source_text.as_bytes();

            // The character just before cursor should be '>'
            if byte_offset > 0 && bytes[byte_offset - 1] != b'>' {
                return None;
            }

            // Don't close self-closing tags like <br />
            if byte_offset >= 2 && bytes[byte_offset - 2] == b'/' {
                return None;
            }

            // Pre-compute string and comment byte ranges so the backward scan
            // can skip over `<`/`>` bytes that live inside attribute strings,
            // JSX-expression strings, or comments. Without this, an attribute
            // like `<div title="a>b">` corrupts the depth counter and the
            // opening `<` is never found at depth 0.
            let skip_ranges = collect_skip_ranges(bytes, byte_offset);
            let in_skip_range = |idx: usize| -> bool {
                skip_ranges
                    .iter()
                    .any(|&(start, end)| idx >= start && idx < end)
            };

            // Scan backwards past attributes to find '<TagName'
            let mut i = byte_offset - 1; // skip the '>'
            let mut depth = 0;

            // Skip past attributes, strings, etc. to find the '<'
            while i > 0 {
                i -= 1;
                if in_skip_range(i) {
                    continue;
                }
                match bytes[i] {
                    b'<' if depth == 0 => {
                        // Found the opening '<', now extract the tag name
                        let tag_start = i + 1;
                        // Check it's not a closing tag '</'
                        if tag_start < bytes.len() && bytes[tag_start] == b'/' {
                            return None;
                        }
                        // Extract tag name (alphanumeric, dots, underscores, dashes, dollar signs)
                        let mut tag_end = tag_start;
                        while tag_end < byte_offset - 1 {
                            let c = bytes[tag_end];
                            if c.is_ascii_alphanumeric()
                                || c == b'.'
                                || c == b'_'
                                || c == b'-'
                                || c == b'$'
                            {
                                tag_end += 1;
                            } else {
                                break;
                            }
                        }

                        if tag_end == tag_start {
                            return None; // No tag name found
                        }

                        let tag_name = &source_text[tag_start..tag_end];

                        // Issue #3731: tsserver's jsxClosingTag returns a
                        // closing tag for any JSX element — including
                        // intrinsic HTML void elements like `<input>` —
                        // because the JSX runtime requires explicit
                        // close tags. The previous void-suppression list
                        // was hand-built and didn't match tsc.

                        // Don't auto-close if the closing tag already follows the cursor
                        let expected_close = format!("</{tag_name}>");
                        if source_text[byte_offset..].starts_with(&expected_close) {
                            return None;
                        }

                        // tsserver's protocol shape is `TextInsertion`:
                        // `{ newText, caretOffset }` (issue #3731).
                        // The previous `$0` snippet syntax was a VS Code
                        // editor convention, not tsserver's wire format.
                        return Some(serde_json::json!({
                            "newText": format!("</{tag_name}>"),
                            "caretOffset": 0,
                        }));
                    }
                    b'>' => depth += 1,
                    b'<' if depth > 0 => depth -= 1,
                    _ => {}
                }
            }

            None
        })();

        self.success_response(seq, request, result)
    }

    pub(crate) fn handle_brace_completion(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // braceCompletion returns boolean indicating whether the opening brace
        // should be auto-completed with the closing one.
        // We should NOT complete if we're inside a string or comment.
        if request
            .arguments
            .get("openingBrace")
            .and_then(|v| v.as_str())
            == Some("<")
        {
            return TsServerResponse {
                seq,
                msg_type: "response".to_string(),
                command: request.command.clone(),
                request_seq: request.seq,
                success: false,
                message: Some("No content available.".to_string()),
                body: None,
            };
        }

        let result = (|| -> Option<Result<serde_json::Value, ()>> {
            let file = request.arguments.get("file")?.as_str()?;
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64().unwrap_or(1) as u32;
            let opening_brace = request
                .arguments
                .get("openingBrace")
                .and_then(|v| v.as_str())
                .unwrap_or("{");

            let source_text = self.open_files.get(file)?;
            let line_map = LineMap::build(source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let byte_offset = line_map.position_to_offset(position, source_text)? as usize;

            // Check if position is inside a string or comment
            let bytes = source_text.as_bytes();
            let mut i = 0;
            let mut in_string = false;
            let mut string_char: u8 = 0;
            let mut in_line_comment = false;
            let mut in_block_comment = false;
            let mut in_template = false;
            let mut template_depth: u32 = 0;

            while i < byte_offset && i < bytes.len() {
                if in_line_comment {
                    if bytes[i] == b'\n' {
                        in_line_comment = false;
                    }
                    i += 1;
                    continue;
                }
                if in_block_comment {
                    if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        in_block_comment = false;
                        i += 2;
                        continue;
                    }
                    i += 1;
                    continue;
                }
                if in_string {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == string_char {
                        in_string = false;
                    }
                    i += 1;
                    continue;
                }
                if in_template {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'`' {
                        in_template = false;
                        i += 1;
                        continue;
                    }
                    if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                        template_depth += 1;
                        in_template = false;
                        i += 2;
                        continue;
                    }
                    i += 1;
                    continue;
                }

                if bytes[i] == b'"' || bytes[i] == b'\'' {
                    in_string = true;
                    string_char = bytes[i];
                    i += 1;
                    continue;
                }
                if bytes[i] == b'`' {
                    in_template = true;
                    i += 1;
                    continue;
                }
                if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
                    in_line_comment = true;
                    i += 2;
                    continue;
                }
                if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    in_block_comment = true;
                    i += 2;
                    continue;
                }
                if bytes[i] == b'}' && template_depth > 0 {
                    template_depth -= 1;
                    in_template = true;
                }
                i += 1;
            }

            let is_quote_like = matches!(opening_brace, "'" | "\"" | "`");

            if is_quote_like && (in_line_comment || in_block_comment) {
                return Some(Err(()));
            }

            // Don't auto-complete inside strings or template literals.
            if in_string || in_template {
                return Some(Ok(serde_json::json!(false)));
            }

            // All valid opening braces should be completed.
            let valid = matches!(opening_brace, "{" | "(" | "[" | "'" | "\"" | "`");
            Some(Ok(serde_json::json!(valid)))
        })();

        match result {
            Some(Err(())) => TsServerResponse {
                seq,
                msg_type: "response".to_string(),
                command: request.command.clone(),
                request_seq: request.seq,
                success: false,
                message: Some("No content available.".to_string()),
                body: None,
            },
            Some(Ok(body)) => self.success_response(seq, request, Some(body)),
            None => self.success_response(seq, request, Some(serde_json::json!(true))),
        }
    }

    pub(crate) fn handle_span_of_enclosing_comment(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64().unwrap_or(1) as u32;
            let only_multi_line = request
                .arguments
                .get("onlyMultiLine")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);

            let source_text = self.open_files.get(file)?;
            let line_map = LineMap::build(source_text);
            let position = Self::tsserver_to_lsp_position(line, offset);
            let byte_offset = line_map.position_to_offset(position, source_text)? as usize;
            let bytes = source_text.as_bytes();
            let len = bytes.len();

            // Scan for comments that contain the position
            let mut i = 0;
            while i < len {
                // Skip string literals
                if bytes[i] == b'"' || bytes[i] == b'\'' || bytes[i] == b'`' {
                    let quote = bytes[i];
                    i += 1;
                    while i < len {
                        if bytes[i] == b'\\' {
                            i += 2;
                            continue;
                        }
                        if bytes[i] == quote {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    continue;
                }

                if i + 1 < len && bytes[i] == b'/' {
                    if bytes[i + 1] == b'/' && !only_multi_line {
                        // Single-line comment
                        let comment_start = i;
                        let comment_end = source_text[i..].find('\n').map_or(len, |j| i + j);
                        if byte_offset >= comment_start && byte_offset <= comment_end {
                            return Some(Self::text_span_body(comment_start, comment_end));
                        }
                        i = comment_end;
                        continue;
                    } else if bytes[i + 1] == b'*' {
                        // Multi-line comment
                        let comment_start = i;
                        i += 2;
                        while i + 1 < len {
                            if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                                i += 2;
                                break;
                            }
                            i += 1;
                        }
                        let comment_end = i;
                        if byte_offset >= comment_start && byte_offset <= comment_end {
                            return Some(Self::text_span_body(comment_start, comment_end));
                        }
                        continue;
                    }
                }
                i += 1;
            }

            None
        })();

        self.success_response(seq, request, result)
    }

    fn text_span_body(start: usize, end: usize) -> serde_json::Value {
        serde_json::json!({
            "start": start,
            "length": end.saturating_sub(start),
        })
    }

    pub(crate) fn handle_todo_comments(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let descriptors = request.arguments.get("descriptors")?.as_array()?;

            // Exclude node_modules files (TypeScript skips these)
            if file.contains("/node_modules/") {
                return Some(serde_json::json!([]));
            }

            let source_text = self.open_files.get(file)?;

            let descriptor_texts: Vec<(String, i64)> = descriptors
                .iter()
                .filter_map(|d| {
                    let text = d.get("text")?.as_str()?.to_string();
                    let priority = d
                        .get("priority")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(0);
                    Some((text, priority))
                })
                .collect();

            if descriptor_texts.is_empty() {
                return Some(serde_json::json!([]));
            }

            let mut results = Vec::new();

            // Implements TypeScript's todo comment matching algorithm:
            // The regex pattern is: (preamble)(descriptor + message)(endOfLine|*/)
            // where preamble is one of:
            //   - //+\s*  (single line comment)
            //   - /*+\s*  (block comment start)
            //   - ^[\s*]* (start of line with spaces/asterisks, for continued block comments)
            //
            // Templates are scanned for ${...} substitutions, which can contain
            // nested code (and therefore nested comments). See #4003.
            Self::scan_todos_in_range(source_text, &descriptor_texts, &mut results, 0, false);

            Some(serde_json::json!(results))
        })();

        self.success_or_empty_array(seq, request, result)
    }

    /// Scan `source_text` from `start_pos` for TODO comments. Skips string and
    /// template literals, but recurses into `${...}` template substitutions
    /// because comments inside substitutions are real comments (#4003).
    ///
    /// When `stop_at_unbalanced_close_brace` is true, returns at the first `}`
    /// that has no matching `{` in this scope — used to bound a single
    /// substitution. Otherwise scans to end of input.
    fn scan_todos_in_range(
        source_text: &str,
        descriptors: &[(String, i64)],
        results: &mut Vec<serde_json::Value>,
        start_pos: usize,
        stop_at_unbalanced_close_brace: bool,
    ) -> usize {
        let bytes = source_text.as_bytes();
        let len = bytes.len();
        let mut i = start_pos;
        let mut brace_depth: u32 = 0;

        while i < len {
            let b = bytes[i];

            // String literal: skip quoted content (with escapes).
            if b == b'"' || b == b'\'' {
                let quote = b;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            // Template literal: scan content with substitution recursion.
            if b == b'`' {
                i += 1;
                i = Self::scan_todos_in_template(source_text, descriptors, results, i);
                continue;
            }

            // Track brace nesting to find the end of a `${...}` substitution.
            if stop_at_unbalanced_close_brace {
                if b == b'{' {
                    brace_depth += 1;
                } else if b == b'}' {
                    if brace_depth == 0 {
                        return i + 1;
                    }
                    brace_depth -= 1;
                }
            }

            if i + 1 < len && b == b'/' {
                if bytes[i + 1] == b'/' {
                    // Line comment: //+\s* then check for descriptor.
                    i += 2;
                    while i < len && bytes[i] == b'/' {
                        i += 1;
                    }
                    while i < len && (bytes[i] == b' ' || bytes[i] == b'\t') {
                        i += 1;
                    }
                    Self::match_descriptor_at(source_text, i, descriptors, results);
                    while i < len && bytes[i] != b'\n' && bytes[i] != b'\r' {
                        i += 1;
                    }
                    continue;
                } else if bytes[i + 1] == b'*' {
                    // Block comment: /*+\s* then content with ^[\s*]* per line.
                    i += 2;
                    while i < len && bytes[i] == b'*' {
                        if i + 1 < len && bytes[i + 1] == b'/' {
                            break;
                        }
                        i += 1;
                    }
                    while i < len && (bytes[i] == b' ' || bytes[i] == b'\t') {
                        i += 1;
                    }
                    if i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        Self::match_descriptor_at(source_text, i, descriptors, results);
                    }
                    while i + 1 < len {
                        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                            i += 2;
                            break;
                        }
                        if bytes[i] == b'\n' || bytes[i] == b'\r' {
                            if bytes[i] == b'\r' && i + 1 < len && bytes[i + 1] == b'\n' {
                                i += 1;
                            }
                            i += 1;
                            while i < len
                                && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'*')
                            {
                                if bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                                    break;
                                }
                                i += 1;
                            }
                            if i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                                Self::match_descriptor_at(source_text, i, descriptors, results);
                            }
                            continue;
                        }
                        i += 1;
                    }
                    continue;
                }
            }
            i += 1;
        }
        i
    }

    /// Scan a template literal body. `start_pos` points just past the opening
    /// backtick. On `${`, recurses into `scan_todos_in_range` with brace-end
    /// tracking so comments inside substitutions are still matched. Returns
    /// the position just past the closing backtick (or end of input).
    fn scan_todos_in_template(
        source_text: &str,
        descriptors: &[(String, i64)],
        results: &mut Vec<serde_json::Value>,
        start_pos: usize,
    ) -> usize {
        let bytes = source_text.as_bytes();
        let len = bytes.len();
        let mut i = start_pos;
        while i < len {
            match bytes[i] {
                b'\\' if i + 1 < len => i += 2,
                b'`' => return i + 1,
                b'$' if i + 1 < len && bytes[i + 1] == b'{' => {
                    i += 2;
                    i = Self::scan_todos_in_range(source_text, descriptors, results, i, true);
                }
                _ => i += 1,
            }
        }
        i
    }

    /// Check if any descriptor matches at the given position (case-insensitive).
    /// If matched, checks word boundary and extracts the message.
    fn match_descriptor_at(
        source_text: &str,
        pos: usize,
        descriptors: &[(String, i64)],
        results: &mut Vec<serde_json::Value>,
    ) {
        let bytes = source_text.as_bytes();
        let len = bytes.len();
        for (text, priority) in descriptors {
            let text_len = text.len();
            if pos + text_len > len {
                continue;
            }
            // Case-insensitive match
            if source_text[pos..pos + text_len].eq_ignore_ascii_case(text) {
                // Word boundary: next char must not be letter/digit
                if pos + text_len < len {
                    let next = bytes[pos + text_len];
                    if next.is_ascii_alphanumeric() || next == b'_' {
                        continue;
                    }
                }
                // Get message: from descriptor to end of line or */
                let rest = &source_text[pos..];
                let mut msg_end = rest.len();
                for (j, &b) in rest.as_bytes().iter().enumerate() {
                    if b == b'\n' || b == b'\r' {
                        msg_end = j;
                        break;
                    }
                    if j + 1 < rest.len() && b == b'*' && rest.as_bytes()[j + 1] == b'/' {
                        msg_end = j;
                        break;
                    }
                }
                let message = &rest[..msg_end];
                let position = Self::utf16_position_for_byte_offset(source_text, pos);
                results.push(serde_json::json!({
                    "descriptor": { "text": text, "priority": priority },
                    "message": message,
                    "position": position,
                }));
                return; // Only match first descriptor at this position
            }
        }
    }

    fn utf16_position_for_byte_offset(source_text: &str, byte_offset: usize) -> usize {
        let byte_offset = byte_offset.min(source_text.len());
        let boundary = if source_text.is_char_boundary(byte_offset) {
            byte_offset
        } else {
            (0..byte_offset)
                .rev()
                .find(|&idx| source_text.is_char_boundary(idx))
                .unwrap_or(0)
        };
        source_text[..boundary].encode_utf16().count()
    }

    pub(crate) fn handle_doc_comment_template(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let line = request.arguments.get("line")?.as_u64()? as usize;
            let _offset = request.arguments.get("offset")?.as_u64().unwrap_or(1);
            let source_text = self.open_files.get(file)?;
            // Resolution order matches tsserver:
            //   1. per-request argument `generateReturnInDocTemplate`
            //   2. user preference set via `configure`
            //   3. tsserver default (`true`)
            let generate_return = request
                .arguments
                .get("generateReturnInDocTemplate")
                .and_then(serde_json::Value::as_bool)
                .or(self.generate_return_in_doc_template)
                .unwrap_or(true);

            // Detect JS files for JSDoc type annotation format
            let is_js_file = file.ends_with(".js") || file.ends_with(".jsx");

            let line_map = LineMap::build(source_text);
            let position = Self::tsserver_to_lsp_position(line as u32, _offset as u32);
            let offset = line_map.position_to_offset(position, source_text)? as usize;

            let line_start = source_text[..offset].rfind('\n').map_or(0, |i| i + 1);
            let before_cursor = &source_text[line_start..offset];
            let line_end = source_text[offset..]
                .find('\n')
                .map_or(source_text.len(), |i| offset + i);
            let after_cursor_on_line = source_text[offset..line_end].trim();

            // Determine declaration text: could be on the same line as cursor, or on the next line(s)
            let decl_text: String;
            let decl_offset: usize;
            let decl_indent: String;
            let _decl_on_same_line: bool;

            // Check if after-cursor text starts with a definite keyword
            let after_starts_with_keyword = ["function ", "class ", "interface ", "enum ", "type "]
                .iter()
                .any(|kw| after_cursor_on_line.starts_with(kw));

            // Check if after-cursor text is only comment-closing syntax (e.g. `*/` or `  */`)
            let after_is_comment_close = {
                let t = after_cursor_on_line.trim();
                t == "*/" || t == "*" || t.is_empty()
            };

            if !after_cursor_on_line.is_empty()
                && !after_is_comment_close
                && (before_cursor.chars().all(|c| c == ' ' || c == '\t')
                    || after_starts_with_keyword)
            {
                // Text follows the cursor on the same line - this IS the declaration
                // Allow if before_cursor is all whitespace, or after starts with a keyword
                // (covers `const x = /*marker*/ function f(p) {}` cases)
                decl_text = after_cursor_on_line.to_string();
                decl_offset = offset
                    + source_text[offset..line_end]
                        .find(after_cursor_on_line)
                        .unwrap_or(0);
                decl_indent = if before_cursor.chars().all(|c| c == ' ' || c == '\t') {
                    before_cursor.to_string()
                } else {
                    // Extract whitespace prefix from before_cursor
                    before_cursor
                        .chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .collect()
                };
                _decl_on_same_line = true;
            } else {
                // Look at the next non-empty line(s) after cursor
                let rest_after_line = if line_end < source_text.len() {
                    &source_text[line_end + 1..]
                } else {
                    return None;
                };

                let mut found_text = String::new();
                let mut found_indent = String::new();
                let mut found_offset = 0usize;
                for text_line in rest_after_line.lines() {
                    let trimmed = text_line.trim();
                    if !trimmed.is_empty() {
                        found_text = trimmed.to_string();
                        let indent_len = text_line.len() - text_line.trim_start().len();
                        found_indent = text_line[..indent_len].to_string();
                        found_offset = (line_end + 1)
                            + (text_line.as_ptr() as usize - rest_after_line.as_ptr() as usize)
                            + indent_len;
                        break;
                    }
                }

                if found_text.is_empty() {
                    return None;
                }

                decl_text = found_text;
                decl_offset = found_offset;
                decl_indent = found_indent;
                _decl_on_same_line = false;
            }

            // Check if it's a documentable declaration
            let declaration_keywords = [
                "function ",
                "class ",
                "interface ",
                "type ",
                "enum ",
                "namespace ",
                "module ",
                "export ",
                "const ",
                "let ",
                "var ",
                "abstract ",
                "async ",
                "public ",
                "private ",
                "protected ",
                "static ",
                "readonly ",
                "get ",
                "set ",
                "constructor",
                "constructor(",
            ];

            // Method-like: identifier followed by ( or <
            let is_method_like = {
                let first_ch = decl_text.chars().next().unwrap_or(' ');
                (first_ch.is_alphabetic() || first_ch == '_' || first_ch == '[')
                    && (decl_text.contains('(') || decl_text.contains('<'))
            };

            // Property-like: identifier followed by : or ?: or ;
            let is_property_like = {
                let first_ch = decl_text.chars().next().unwrap_or(' ');
                (first_ch.is_alphabetic() || first_ch == '_')
                    && (decl_text.contains(':')
                        || decl_text.contains(';')
                        || decl_text.ends_with(','))
            };

            // Enum member: identifier optionally followed by = value, then , or end of line
            let is_enum_member = {
                let first_ch = decl_text.chars().next().unwrap_or(' ');
                let trimmed_decl = decl_text.trim_end_matches(',').trim();
                (first_ch.is_alphabetic() || first_ch == '_')
                    && !decl_text.contains('(')
                    && !decl_text.contains('{')
                    && !decl_text.contains('.')
                    && (trimmed_decl.ends_with(',')
                        || trimmed_decl
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '=' || c == ' '))
            };

            let is_documentable = declaration_keywords
                .iter()
                .any(|kw| decl_text.starts_with(kw))
                || is_method_like
                || is_property_like
                || is_enum_member;

            if !is_documentable {
                return None;
            }

            // Check if there's already a complete JSDoc comment before the cursor's line
            let before_line = source_text[..line_start].trim_end();
            if before_line.ends_with("*/") {
                return None;
            }

            // Check if cursor is inside an existing JSDoc that already has content
            // (e.g. `/** Doc */` → don't expand; `/**  */` or `/** */` → expand)
            if let Some(jsdoc_pos) = before_cursor.find("/**") {
                let after_jsdoc = before_cursor[jsdoc_pos + 3..].trim();
                if !after_jsdoc.is_empty() && after_jsdoc != "*" && !after_jsdoc.starts_with("*/") {
                    // JSDoc has meaningful content - don't regenerate
                    return None;
                }
            }

            // Check for multi-declarator variable statements (e.g. `let a = 1, b = 2;`)
            // These should not extract params from initializer functions
            let is_multi_declarator =
                Self::is_multi_declarator_var(&decl_text, source_text, decl_offset);

            // Issue #3752: tsc leaves the existing one-line JSDoc alone when
            // the documented declaration is non-callable (type alias,
            // interface, class, enum, namespace, module) even if a nested
            // function-like signature appears on the same line. The previous
            // line-scan happily found the nested `(...)` and produced
            // spurious `@param`/`@returns` tags.
            let is_non_callable_decl = [
                "type ",
                "interface ",
                "class ",
                "enum ",
                "namespace ",
                "module ",
            ]
            .iter()
            .any(|kw| decl_text.starts_with(kw));

            let suppress_signature_extraction = is_multi_declarator || is_non_callable_decl;

            // Extract parameters from the declaration
            let params = if suppress_signature_extraction {
                Vec::new()
            } else {
                Self::extract_function_params(&decl_text, source_text, decl_offset)
            };

            // Check for return statement in function body if generate_return is enabled
            let has_return = if generate_return && !suppress_signature_extraction {
                Self::function_has_return(&decl_text, source_text, decl_offset)
            } else {
                false
            };

            // Build the doc comment template
            // Use leading whitespace from the cursor's line for indentation
            let template_indent: String = before_cursor
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();

            if params.is_empty() && !has_return {
                // Simple template
                Some(serde_json::json!({
                    "newText": "/** */",
                    "caretOffset": 3
                }))
            } else {
                // Multi-line template with @param and/or @returns tags
                let mut lines = Vec::new();
                lines.push("/**".to_string());
                lines.push(format!("{template_indent} * "));

                for param in &params {
                    if is_js_file {
                        if let Some(name) = param.strip_prefix("...") {
                            lines.push(format!("{template_indent} * @param {{...any}} {name}"));
                        } else {
                            lines.push(format!("{template_indent} * @param {{any}} {param}"));
                        }
                    } else {
                        // For TS, strip the ... prefix
                        let name = param.strip_prefix("...").unwrap_or(param);
                        lines.push(format!("{template_indent} * @param {name}"));
                    }
                }

                if has_return {
                    lines.push(format!("{template_indent} * @returns"));
                }

                lines.push(format!("{template_indent} */"));
                // Add trailing indent when cursor and declaration are on the same line
                // and cursor is at the very start of the line (only whitespace before it)
                if _decl_on_same_line && before_cursor.chars().all(|c| c == ' ' || c == '\t') {
                    lines.push(decl_indent);
                }

                let new_text = lines.join("\n");
                // Caret offset: "/**\n<indent> * " -> caret is after " * " on second line
                let caret_offset = 3 + 1 + template_indent.len() + 3; // "/**" + "\n" + indent + " * "

                Some(serde_json::json!({
                    "newText": new_text,
                    "caretOffset": caret_offset
                }))
            }
        })();

        // Always return a body so processResponse(request) works.
        // When no template, return {newText: "", caretOffset: 0} which
        // is truthy for processResponse but signals "no template" to adapter.
        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "newText": "",
                "caretOffset": 0
            }))),
        )
    }

    /// Extract function parameter names from a declaration line.
    /// Handles destructured params ({x, y}) as param1, param2, etc.
    /// Strips access modifiers (public, private, protected), rest (...), and optional (?).
    fn extract_function_params(decl: &str, source: &str, decl_offset: usize) -> Vec<String> {
        // For variable declarations, extract the initializer and analyze it
        let effective_decl = Self::get_effective_decl(decl, source, decl_offset);
        let decl = effective_decl.as_deref().unwrap_or(decl);

        // Find the opening paren - handle methods, functions, constructors, arrow functions
        let paren_start = match Self::find_param_list_start(decl) {
            Some(pos) => pos,
            None => {
                // No parens found - check for arrow function without parens
                // Pattern: identifier => ...
                if let Some(arrow_pos) = decl.find("=>") {
                    let before_arrow = decl[..arrow_pos].trim_end();
                    // Extract the last identifier token before =>
                    let param: String = before_arrow
                        .chars()
                        .rev()
                        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    if !param.is_empty()
                        && param
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
                    {
                        return vec![param];
                    }
                }
                return Vec::new();
            }
        };

        // Extract content between parens, handling nesting
        let chars: Vec<char> = decl.chars().collect();
        let mut depth = 0;
        let mut end = paren_start;
        for (offset, ch) in chars.iter().skip(paren_start).enumerate() {
            match *ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = paren_start + offset;
                        break;
                    }
                }
                _ => {}
            }
        }

        if depth != 0 {
            return Vec::new();
        }

        let inner: String = chars[paren_start + 1..end].iter().collect();
        if inner.trim().is_empty() {
            return Vec::new();
        }

        // Split by commas at depth 0
        let parts = Self::split_params(&inner);
        let mut params = Vec::new();
        let mut param_index = 0;

        for part in &parts {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Strip access modifiers
            let mut s = trimmed;
            for modifier in &["public ", "private ", "protected ", "readonly "] {
                if s.starts_with(modifier) {
                    s = &s[modifier.len()..];
                }
            }
            let s = s.trim();

            // Handle rest parameter - preserve prefix for JS @param format
            let is_rest = s.starts_with("...");
            let s = if is_rest { &s[3..] } else { s };

            // Handle destructured params - use parameter index for naming
            if s.starts_with('{') || s.starts_with('[') {
                let name = format!("param{param_index}");
                params.push(if is_rest { format!("...{name}") } else { name });
                param_index += 1;
                continue;
            }

            // Extract identifier (before : or ? or = or ,)
            let name: String = s
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                .collect();

            if !name.is_empty() {
                params.push(if is_rest { format!("...{name}") } else { name });
            }
            param_index += 1;
        }

        params
    }

    /// Check if a line ending with ')' is a braceless control flow statement
    /// like `if (...)`, `for (...)`, `while (...)`, `for ... of (...)`, etc.
    fn is_control_flow_paren(line: &str) -> bool {
        let trimmed = line.trim_start();
        trimmed.starts_with("if ")
            || trimmed.starts_with("if(")
            || trimmed.starts_with("else if ")
            || trimmed.starts_with("else if(")
            || trimmed.starts_with("for ")
            || trimmed.starts_with("for(")
            || trimmed.starts_with("with ")
            || trimmed.starts_with("with(")
    }

    /// Check if `while (...)` is a standalone while loop (not part of do-while).
    /// Look at the line before it: if it ends with `}`, this is likely `do {...} while(...)`.
    fn is_standalone_while(lines: &[&str], prev_line_idx: usize) -> bool {
        // Find the non-empty line before the while line
        let mut check_idx = if prev_line_idx > 0 {
            prev_line_idx - 1
        } else {
            return true; // while on first line → standalone
        };
        while check_idx > 0 && lines[check_idx].trim().is_empty() {
            check_idx -= 1;
        }
        let before_while = lines[check_idx].trim();
        // If the line before the while ends with '}', it's do-while
        !before_while.ends_with('}')
    }

    /// Check if the previous line is an incomplete statement/keyword needing
    /// continuation indentation on the next line.
    fn needs_keyword_continuation(prev_trimmed: &str) -> bool {
        // Bare control flow keywords without parens or braces
        let bare_keywords = ["if", "else", "while", "for", "do", "else if"];
        for kw in &bare_keywords {
            if prev_trimmed == *kw {
                return true;
            }
        }
        // Incomplete function/class declarations (no opening brace)
        if (prev_trimmed.starts_with("function ")
            || prev_trimmed.starts_with("function(")
            || prev_trimmed == "function"
            || prev_trimmed.starts_with("class ")
            || prev_trimmed == "class")
            && !prev_trimmed.ends_with('{')
            && !prev_trimmed.ends_with('}')
            && !prev_trimmed.ends_with(';')
        {
            return true;
        }
        // Incomplete variable declarations (var/let/const without semicolon)
        if (prev_trimmed.starts_with("var ")
            || prev_trimmed.starts_with("let ")
            || prev_trimmed.starts_with("const ")
            || prev_trimmed == "var"
            || prev_trimmed == "let"
            || prev_trimmed == "const")
            && !prev_trimmed.ends_with(';')
            && !prev_trimmed.ends_with('{')
            && !prev_trimmed.ends_with('}')
        {
            return true;
        }
        // `else` keyword (already covered by bare_keywords above, but
        // also handle `else` followed by something that's not `if` or `{`)
        false
    }

    /// Check if a declaration is a multi-declarator variable statement.
    /// E.g. `let a = 1, b = 2;` has multiple `=` at depth 0.
    fn is_multi_declarator_var(decl: &str, source: &str, decl_offset: usize) -> bool {
        // Only applies to variable declarations
        let is_var_decl = decl.starts_with("var ")
            || decl.starts_with("let ")
            || decl.starts_with("const ")
            || decl.starts_with("export var ")
            || decl.starts_with("export let ")
            || decl.starts_with("export const ");
        if !is_var_decl {
            return false;
        }

        let full_stmt = &source[decl_offset..];
        let mut depth = 0i32;
        let mut eq_count = 0;
        let chars: Vec<char> = full_stmt.chars().collect();
        for i in 0..chars.len() {
            match chars[i] {
                '(' | '{' | '[' => depth += 1,
                ')' | '}' | ']' => depth = (depth - 1).max(0),
                ';' | '\n' if depth == 0 => break,
                '=' if depth == 0 => {
                    let prev = if i > 0 { chars[i - 1] } else { ' ' };
                    let next = chars.get(i + 1).copied().unwrap_or(' ');
                    // Exclude ==, !=, >=, <=, =>
                    if prev != '!'
                        && prev != '<'
                        && prev != '>'
                        && prev != '='
                        && next != '='
                        && next != '>'
                    {
                        eq_count += 1;
                        if eq_count > 1 {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// For variable declarations (`var/let/const name = initializer`), extract
    /// the effective declaration from the initializer. Strips outer grouping
    /// parens and handles function expressions, arrow functions, and class
    /// expressions with constructors.
    fn get_effective_decl(decl: &str, source: &str, decl_offset: usize) -> Option<String> {
        // Only apply to variable declarations
        let rest = decl
            .strip_prefix("var ")
            .or_else(|| decl.strip_prefix("let "))
            .or_else(|| decl.strip_prefix("const "))
            .or_else(|| decl.strip_prefix("export var "))
            .or_else(|| decl.strip_prefix("export let "))
            .or_else(|| decl.strip_prefix("export const "))?;

        // Find the `=` in the declaration (skip the variable name)
        let eq_pos = rest.find('=')?;
        // Make sure it's `=` not `==` or `=>`
        let after_eq = rest.get(eq_pos + 1..)?;
        if after_eq.starts_with('=') || after_eq.starts_with('>') {
            return None;
        }
        // Find the RHS start position in source for multi-line scanning
        let eq_byte_offset = {
            let eq_search = &source[decl_offset..];
            decl_offset + eq_search.find('=')? + 1
        };
        // Skip whitespace after = to find RHS start
        let mut rhs_source_start = eq_byte_offset;
        while rhs_source_start < source.len() {
            let ch = source.as_bytes()[rhs_source_start];
            if ch == b' ' || ch == b'\t' || ch == b'\r' || ch == b'\n' {
                rhs_source_start += 1;
            } else {
                break;
            }
        }

        let mut rhs = after_eq.trim().to_string();

        // Strip outer grouping parens using source text for multi-line support
        loop {
            if rhs_source_start >= source.len() {
                break;
            }
            if source.as_bytes()[rhs_source_start] == b'(' {
                let scan_text = &source[rhs_source_start..];
                let chars: Vec<char> = scan_text.chars().collect();
                let mut depth = 0;
                let mut close_pos = None;
                for (i, &c) in chars.iter().enumerate() {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                close_pos = Some(i);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                // Only strip if the paren wraps the entire expression
                if let Some(cp) = close_pos {
                    let after_close: String = chars[cp + 1..].iter().collect();
                    // Check if paren wraps the expression:
                    // what follows should be end-of-statement or another closing paren
                    let after_on_line = after_close.split('\n').next().unwrap_or("").trim();
                    if after_on_line.is_empty()
                        || after_on_line.starts_with(';')
                        || after_on_line.starts_with(',')
                        || after_on_line.starts_with(')')
                    {
                        let inner: String = chars[1..cp].iter().collect();
                        let trimmed_inner = inner.trim();
                        // Don't strip if inner looks like arrow params: (x, y) => ...
                        if !trimmed_inner.contains("=>") || trimmed_inner.starts_with('(') {
                            rhs = trimmed_inner.to_string();
                            // Advance source offset past opening paren + whitespace
                            rhs_source_start += 1; // skip '('
                            while rhs_source_start < source.len() {
                                let ch = source.as_bytes()[rhs_source_start];
                                if ch == b' ' || ch == b'\t' || ch == b'\r' || ch == b'\n' {
                                    rhs_source_start += 1;
                                } else {
                                    break;
                                }
                            }
                            continue;
                        }
                    }
                }
            }
            break;
        }

        // For class expressions, look for constructor in the class body only
        if rhs.starts_with("class ") || rhs.starts_with("class{") {
            let full = &source[rhs_source_start..];
            // Find the opening brace of the class body
            if let Some(brace_start) = full.find('{') {
                // Find the matching closing brace
                let mut depth = 0;
                let mut brace_end = full.len();
                for (i, c) in full[brace_start..].char_indices() {
                    match c {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                brace_end = brace_start + i;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let class_body = &full[brace_start..brace_end];
                // Search for constructor only within the class body
                if let Some(ctor_pos) = class_body
                    .find("constructor(")
                    .or_else(|| class_body.find("constructor ("))
                {
                    let ctor_decl = &full[brace_start + ctor_pos..];
                    return Some(ctor_decl.to_string());
                }
            }
            return Some(rhs);
        }

        Some(rhs)
    }

    /// Find the start of the parameter list (opening paren) in a declaration.
    fn find_param_list_start(decl: &str) -> Option<usize> {
        let chars: Vec<char> = decl.chars().collect();

        // For computed property names like [Symbol.iterator](...), skip the brackets
        let mut i = 0;
        if chars.first() == Some(&'[') {
            let mut depth = 0;
            while i < chars.len() {
                match chars[i] {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        }

        // Skip identifier, generic params, etc. to find '('
        let mut angle_depth = 0;
        while i < chars.len() {
            match chars[i] {
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' if angle_depth == 0 => return Some(i),
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Split parameter string by commas at depth 0 (respecting nested parens/braces/brackets).
    fn split_params(s: &str) -> Vec<String> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut depth = 0;
        for c in s.chars() {
            match c {
                '(' | '{' | '[' | '<' => {
                    depth += 1;
                    current.push(c);
                }
                ')' | '}' | ']' | '>' => {
                    depth -= 1;
                    current.push(c);
                }
                ',' if depth == 0 => {
                    parts.push(current.clone());
                    current.clear();
                }
                _ => current.push(c),
            }
        }
        if !current.trim().is_empty() {
            parts.push(current);
        }
        parts
    }

    /// Check if a function body contains a return statement.
    fn function_has_return(decl: &str, source: &str, decl_offset: usize) -> bool {
        // For arrow functions like `const f = () => expr`, check if it's a concise body
        // (no braces = implicit return)
        if decl.contains("=>") {
            // Check if the arrow is followed by something other than {
            if let Some(arrow_pos) = decl.find("=>") {
                let after_arrow = decl[arrow_pos + 2..].trim();
                if !after_arrow.starts_with('{') && !after_arrow.is_empty() {
                    return true;
                }
            }
        }

        // Find the function body (opening brace after declaration)
        let full_decl = &source[decl_offset..];

        // Find opening brace at depth 0 (skip param parens)
        // Stop at `;` or `\n` at paren_depth=0 to avoid crossing statement boundaries
        let mut paren_depth: i32 = 0;
        let mut brace_start = None;
        for (i, c) in full_decl.char_indices() {
            match c {
                '(' => paren_depth += 1,
                ')' => paren_depth = (paren_depth - 1).max(0),
                '{' if paren_depth == 0 => {
                    brace_start = Some(i);
                    break;
                }
                // Stop at statement boundaries to avoid scanning into next statement
                ';' | '\n' if paren_depth == 0 => break,
                _ => {}
            }
        }

        let brace_start = match brace_start {
            Some(pos) => pos,
            None => return false,
        };

        // Find the matching closing brace
        let mut depth = 0;
        let mut brace_end = full_decl.len();
        for (i, c) in full_decl[brace_start..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        brace_end = brace_start + i;
                        break;
                    }
                }
                _ => {}
            }
        }

        let body = &full_decl[brace_start + 1..brace_end];

        // Check for return statement (simple text search)
        // Need to be careful not to match "return" in nested functions
        // Simple approach: look for "return " or "return;" or "return\n" at the
        // top-level function scope (depth 0)
        let mut fn_depth = 0;
        let body_chars: Vec<char> = body.chars().collect();
        let mut i = 0;
        while i < body_chars.len() {
            match body_chars[i] {
                '{' => fn_depth += 1,
                '}' => fn_depth -= 1,
                'r' if fn_depth == 0 => {
                    let remaining: String = body_chars[i..].iter().take(7).collect();
                    if remaining.starts_with("return") {
                        // Check that "return" is followed by a non-identifier char
                        let after = body_chars.get(i + 6).copied().unwrap_or(' ');
                        if !after.is_alphanumeric() && after != '_' {
                            return true;
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }

        false
    }

    pub(crate) fn handle_indentation(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // indentation returns { position: number, indentation: number }
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let line = request.arguments.get("line")?.as_u64()? as u32;
            let offset = request.arguments.get("offset")?.as_u64().unwrap_or(1) as u32;
            let source_text = self.open_files.get(file)?;
            let line_map = LineMap::build(source_text);
            let requested_position = line_map
                .position_to_offset(Self::tsserver_to_lsp_position(line, offset), source_text)?;

            // Get indent size from options (default 4)
            let indent_size = request
                .arguments
                .get("options")
                .and_then(|o| {
                    o.get("indentSize")
                        .and_then(serde_json::Value::as_u64)
                        .or_else(|| o.get("tabSize").and_then(serde_json::Value::as_u64))
                })
                .unwrap_or(4) as usize;

            let base_indent = request
                .arguments
                .get("options")
                .and_then(|o| o.get("baseIndentSize"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;

            let lines: Vec<&str> = source_text.lines().collect();
            let target_line_idx = line.saturating_sub(1) as usize;

            // Smart indentation: compute brace/bracket/paren depth up to the target line
            // by scanning all lines before it, then adjust for the current line.
            // When target_line_idx >= lines.len() (e.g. cursor past EOF), scan all
            // available lines and treat the target as an empty line.
            let scan_end = target_line_idx.min(lines.len());
            let mut depth: i32 = 0;
            let mut in_block_comment = false;

            for line_text in lines.iter().take(scan_end) {
                let bytes = line_text.as_bytes();
                let mut j = 0;
                while j < bytes.len() {
                    if in_block_comment {
                        if j + 1 < bytes.len() && bytes[j] == b'*' && bytes[j + 1] == b'/' {
                            in_block_comment = false;
                            j += 2;
                            continue;
                        }
                        j += 1;
                        continue;
                    }
                    // Skip strings
                    if bytes[j] == b'"' || bytes[j] == b'\'' {
                        let q = bytes[j];
                        j += 1;
                        while j < bytes.len() {
                            if bytes[j] == b'\\' {
                                j += 2;
                                continue;
                            }
                            if bytes[j] == q {
                                j += 1;
                                break;
                            }
                            j += 1;
                        }
                        continue;
                    }
                    if bytes[j] == b'`' {
                        // Template literals can span lines - simplified handling
                        j += 1;
                        while j < bytes.len() {
                            if bytes[j] == b'\\' {
                                j += 2;
                                continue;
                            }
                            if bytes[j] == b'`' {
                                j += 1;
                                break;
                            }
                            j += 1;
                        }
                        continue;
                    }
                    // Skip line comments
                    if j + 1 < bytes.len() && bytes[j] == b'/' && bytes[j + 1] == b'/' {
                        break; // rest of line is comment
                    }
                    // Block comment start
                    if j + 1 < bytes.len() && bytes[j] == b'/' && bytes[j + 1] == b'*' {
                        in_block_comment = true;
                        j += 2;
                        continue;
                    }
                    match bytes[j] {
                        b'{' | b'(' | b'[' => depth += 1,
                        b'}' | b')' | b']' if depth > 0 => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
            }

            // Check if the current line starts with a closing bracket
            // If target is past EOF, treat as empty line
            let current_trimmed = if target_line_idx < lines.len() {
                lines[target_line_idx].trim()
            } else {
                ""
            };
            let starts_with_closing = current_trimmed.starts_with('}')
                || current_trimmed.starts_with(')')
                || current_trimmed.starts_with(']');

            // Also look at the previous non-empty line for context
            let prev_search_start = if target_line_idx > 0 {
                (target_line_idx - 1).min(lines.len().saturating_sub(1))
            } else {
                0
            };
            let mut prev_line_idx = prev_search_start;
            while prev_line_idx > 0 && lines[prev_line_idx].trim().is_empty() {
                prev_line_idx -= 1;
            }
            let prev_trimmed = lines.get(prev_line_idx).map_or("", |l| l.trim());

            // Adjust: if previous line ends with opener, we've already counted it in depth
            // The depth represents how many unclosed openers exist before this line
            let mut indentation = (depth as usize) * indent_size + base_indent;

            // If current line starts with closer, reduce by one level
            if starts_with_closing && indentation >= indent_size {
                indentation -= indent_size;
            }

            // Special case: if previous line ends with opener and current line is empty
            // (new line just inserted), the depth already accounts for it
            // But if previous line doesn't end with opener and has continuation context
            // (like after =>, case:, etc.) add one level
            let prev_ends_with_opener = prev_trimmed.ends_with('{')
                || prev_trimmed.ends_with('(')
                || prev_trimmed.ends_with('[');

            if !prev_ends_with_opener && !starts_with_closing {
                // Check for continuation contexts that need extra indentation
                let is_braceless_control = prev_trimmed.ends_with(')')
                    && !current_trimmed.starts_with('{')
                    && (Self::is_control_flow_paren(prev_trimmed)
                        || ((prev_trimmed.trim_start().starts_with("while ")
                            || prev_trimmed.trim_start().starts_with("while("))
                            && Self::is_standalone_while(&lines, prev_line_idx)));

                // Check if prev line has unbalanced openers - if so, the depth
                // counter already accounts for the indentation increase.
                let prev_has_unbalanced_opener = {
                    let mut d = 0i32;
                    for c in prev_trimmed.chars() {
                        match c {
                            '(' | '[' | '{' => d += 1,
                            ')' | ']' | '}' => d -= 1,
                            _ => {}
                        }
                    }
                    d > 0
                };

                let needs_continuation = prev_trimmed.ends_with("=>")
                    || (prev_trimmed.ends_with(':')
                        && (prev_trimmed.starts_with("case ")
                            || prev_trimmed.starts_with("default:")))
                    || is_braceless_control
                    || (!prev_has_unbalanced_opener
                        && !current_trimmed.starts_with('{')
                        && Self::needs_keyword_continuation(prev_trimmed));
                if needs_continuation {
                    indentation += indent_size;
                }
            }

            Some(serde_json::json!({
                "position": requested_position,
                "indentation": indentation
            }))
        })();
        self.success_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"position": 1, "indentation": 0}))),
        )
    }

    pub(crate) fn handle_compiler_options_diagnostics(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // getCompilerOptionsDiagnostics — validate tsconfig options
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let config_path = Self::find_nearest_tsconfig(file)?;
            let config_text = std::fs::read_to_string(&config_path).ok()?;

            let mut diagnostics: Vec<serde_json::Value> = Vec::new();

            // Basic JSON parse validation
            if let Err(e) = serde_json::from_str::<serde_json::Value>(&config_text) {
                diagnostics.push(serde_json::json!({
                    "start": { "line": 1, "offset": 1 },
                    "end": { "line": 1, "offset": 1 },
                    "text": format!("Invalid JSON in tsconfig: {e}"),
                    "code": 5083,
                    "category": "error",
                }));
            }

            Some(serde_json::json!(diagnostics))
        })();
        self.success_or_empty_array(seq, request, result)
    }
}

/// Forward-scan `bytes[..end]` and collect byte ranges that should be ignored
/// when matching JSX angle brackets — namely string/template literals and
/// `//` / `/* ... */` comments. The returned ranges are half-open `[start, end)`
/// and include the surrounding quotes/comment delimiters.
///
/// This is intentionally a lightweight tokenizer rather than a full JSX
/// scanner: it is sufficient to keep `<` and `>` inside attribute strings
/// (and JSX-expression strings) from being treated as tag boundaries.
pub(crate) fn collect_skip_ranges(bytes: &[u8], end: usize) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let limit = end.min(bytes.len());
    let mut j = 0;
    while j < limit {
        match bytes[j] {
            quote @ (b'"' | b'\'' | b'`') => {
                let start = j;
                j += 1;
                while j < limit && bytes[j] != quote {
                    if bytes[j] == b'\\' && j + 1 < limit {
                        j += 2;
                    } else if bytes[j] == b'\n' && quote != b'`' {
                        // Unterminated single/double-quoted string: stop at
                        // newline so a stray quote does not swallow the rest
                        // of the file.
                        break;
                    } else {
                        j += 1;
                    }
                }
                if j < limit {
                    j += 1; // consume closing quote (or stop byte)
                }
                ranges.push((start, j));
            }
            b'/' if j + 1 < limit && bytes[j + 1] == b'/' => {
                let start = j;
                j += 2;
                while j < limit && bytes[j] != b'\n' {
                    j += 1;
                }
                ranges.push((start, j));
            }
            b'/' if j + 1 < limit && bytes[j + 1] == b'*' => {
                let start = j;
                j += 2;
                while j + 1 < limit && !(bytes[j] == b'*' && bytes[j + 1] == b'/') {
                    j += 1;
                }
                if j + 1 < limit {
                    j += 2; // consume closing */
                } else {
                    j = limit;
                }
                ranges.push((start, j));
            }
            _ => j += 1,
        }
    }
    ranges
}
