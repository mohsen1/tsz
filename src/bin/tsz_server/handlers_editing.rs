//! Editing utility handlers for tsz-server.
//!
//! Handles commands related to editing assists: breakpoints, JSX closing tags,
//! brace completion, comments, doc templates, indentation, classifications, etc.

use super::{Server, TsServerRequest, TsServerResponse};
use wasm::lsp::position::LineMap;

impl Server {
    pub(crate) fn handle_breakpoint_statement(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // breakpointStatement returns a TextSpan or undefined
        // Return undefined (no body) to indicate no breakpoint at this position
        self.stub_response(seq, request, None)
    }

    pub(crate) fn handle_jsx_closing_tag(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // jsxClosingTag returns { newText: string } or undefined
        // Return undefined (no body) to indicate no closing tag needed
        self.stub_response(seq, request, None)
    }

    pub(crate) fn handle_brace_completion(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // braceCompletion returns boolean
        // Default to true (brace completion is valid)
        self.stub_response(seq, request, Some(serde_json::json!(true)))
    }

    pub(crate) fn handle_span_of_enclosing_comment(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // getSpanOfEnclosingComment returns TextSpan or undefined
        // Return undefined (no body) to indicate not inside a comment
        self.stub_response(seq, request, None)
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
                    let priority = d.get("priority").and_then(|p| p.as_i64()).unwrap_or(0);
                    Some((text, priority))
                })
                .collect();

            if descriptor_texts.is_empty() {
                return Some(serde_json::json!([]));
            }

            let mut results = Vec::new();
            let bytes = source_text.as_bytes();
            let len = bytes.len();

            // Implements TypeScript's todo comment matching algorithm:
            // The regex pattern is: (preamble)(descriptor + message)(endOfLine|*/)
            // where preamble is one of:
            //   - //+\s*  (single line comment)
            //   - /*+\s*  (block comment start)
            //   - ^[\s*]* (start of line with spaces/asterisks, for continued block comments)
            let mut i = 0;
            while i < len {
                // Skip string literals to avoid false matches
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
                    if bytes[i + 1] == b'/' {
                        // Line comment: //+\s* then check for descriptor
                        i += 2;
                        while i < len && bytes[i] == b'/' {
                            i += 1;
                        }
                        while i < len && (bytes[i] == b' ' || bytes[i] == b'\t') {
                            i += 1;
                        }
                        // Check for descriptor at current position
                        Self::match_descriptor_at(
                            source_text,
                            i,
                            &descriptor_texts,
                            &mut results,
                        );
                        // Skip to end of line
                        while i < len && bytes[i] != b'\n' && bytes[i] != b'\r' {
                            i += 1;
                        }
                        continue;
                    } else if bytes[i + 1] == b'*' {
                        // Block comment: /*+\s* then content with ^[\s*]* per line
                        i += 2;
                        // Skip additional asterisks (but not closing */)
                        while i < len && bytes[i] == b'*' {
                            if i + 1 < len && bytes[i + 1] == b'/' {
                                break;
                            }
                            i += 1;
                        }
                        // Skip whitespace after comment start
                        while i < len && (bytes[i] == b' ' || bytes[i] == b'\t') {
                            i += 1;
                        }
                        // Check for descriptor right after the comment opening
                        if i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                            Self::match_descriptor_at(
                                source_text,
                                i,
                                &descriptor_texts,
                                &mut results,
                            );
                        }
                        // Scan through block comment content
                        while i + 1 < len {
                            if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                                i += 2;
                                break;
                            }
                            if bytes[i] == b'\n' || bytes[i] == b'\r' {
                                // Handle \r\n
                                if bytes[i] == b'\r' && i + 1 < len && bytes[i + 1] == b'\n' {
                                    i += 1;
                                }
                                i += 1;
                                // Skip leading whitespace and asterisks (^[\s*]*)
                                while i < len
                                    && (bytes[i] == b' '
                                        || bytes[i] == b'\t'
                                        || bytes[i] == b'*')
                                {
                                    if bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                                        break;
                                    }
                                    i += 1;
                                }
                                // Check for descriptor
                                if i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                                    Self::match_descriptor_at(
                                        source_text,
                                        i,
                                        &descriptor_texts,
                                        &mut results,
                                    );
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

            Some(serde_json::json!(results))
        })();

        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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
                results.push(serde_json::json!({
                    "descriptor": { "text": text, "priority": priority },
                    "message": message,
                    "position": pos,
                }));
                return; // Only match first descriptor at this position
            }
        }
    }

    pub(crate) fn handle_doc_comment_template(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let _line = request.arguments.get("line")?.as_u64()? as usize;
            let _offset = request.arguments.get("offset")?.as_u64().unwrap_or(1);
            let source_text = self.open_files.get(file)?;

            // Find the next non-whitespace content after the cursor position
            // to determine if we're before a documentable declaration.
            let line_map = LineMap::build(source_text);
            let position = Self::tsserver_to_lsp_position(_line as u32, _offset as u32);
            let offset = line_map.position_to_offset(position, source_text)? as usize;

            // Look at what follows - find next non-whitespace line
            let rest = &source_text[offset..];
            let trimmed = rest.trim_start();

            // Check if cursor is before a documentable declaration
            let is_documentable = trimmed.starts_with("function ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("interface ")
                || trimmed.starts_with("type ")
                || trimmed.starts_with("enum ")
                || trimmed.starts_with("namespace ")
                || trimmed.starts_with("module ")
                || trimmed.starts_with("export ")
                || trimmed.starts_with("const ")
                || trimmed.starts_with("let ")
                || trimmed.starts_with("var ")
                || trimmed.starts_with("abstract ")
                || trimmed.starts_with("async ")
                || trimmed.starts_with("public ")
                || trimmed.starts_with("private ")
                || trimmed.starts_with("protected ")
                || trimmed.starts_with("static ")
                || trimmed.starts_with("readonly ")
                || trimmed.starts_with("get ")
                || trimmed.starts_with("set ")
                // method-like: identifier followed by (
                || trimmed.chars().next().map_or(false, |c| c.is_alphabetic() || c == '_');

            if !is_documentable {
                return None;
            }

            // Basic template: /** */
            // TODO: Parse function parameters and generate @param tags
            Some(serde_json::json!({
                "newText": "/** */",
                "caretOffset": 3
            }))
        })();

        // Always return a body so processResponse(request) works.
        // When no template, return {newText: "", caretOffset: 0} which
        // is truthy for processResponse but signals "no template" to adapter.
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({
                "newText": "",
                "caretOffset": 0
            }))),
        )
    }

    pub(crate) fn handle_indentation(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // indentation returns { position: number, indentation: number }
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let line = request.arguments.get("line")?.as_u64()? as usize;
            let position = request.arguments.get("offset")?.as_u64().unwrap_or(1);
            let source_text = self.open_files.get(file)?;

            // Get indent size from options (default 4)
            let indent_size = request
                .arguments
                .get("options")
                .and_then(|o| {
                    o.get("indentSize")
                        .and_then(|v| v.as_u64())
                        .or_else(|| o.get("tabSize").and_then(|v| v.as_u64()))
                })
                .unwrap_or(4) as usize;

            let base_indent = request
                .arguments
                .get("options")
                .and_then(|o| o.get("baseIndentSize"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            let lines: Vec<&str> = source_text.lines().collect();
            let target_line_idx = if line > 0 { line - 1 } else { 0 };

            if target_line_idx >= lines.len() {
                return Some(serde_json::json!({"position": position, "indentation": 0}));
            }

            // Smart indentation: compute brace/bracket/paren depth up to the target line
            // by scanning all lines before it, then adjust for the current line.
            let mut depth: i32 = 0;
            let mut in_block_comment = false;
            let mut in_single_line_string = false;

            for line_idx in 0..target_line_idx {
                let line_text = lines[line_idx];
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
                        b'}' | b')' | b']' => {
                            if depth > 0 {
                                depth -= 1;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
            }

            // Check if the current line starts with a closing bracket
            let current_trimmed = lines[target_line_idx].trim();
            let starts_with_closing = current_trimmed.starts_with('}')
                || current_trimmed.starts_with(')')
                || current_trimmed.starts_with(']');

            // Also look at the previous non-empty line for context
            let mut prev_line_idx = if target_line_idx > 0 {
                target_line_idx - 1
            } else {
                0
            };
            while prev_line_idx > 0 && lines[prev_line_idx].trim().is_empty() {
                prev_line_idx -= 1;
            }
            let prev_trimmed = lines.get(prev_line_idx).map(|l| l.trim()).unwrap_or("");

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
                let needs_continuation = prev_trimmed.ends_with("=>")
                    || (prev_trimmed.ends_with(':')
                        && (prev_trimmed.starts_with("case ")
                            || prev_trimmed.starts_with("default:")));
                if needs_continuation {
                    indentation += indent_size;
                }
            }

            Some(serde_json::json!({
                "position": position,
                "indentation": indentation
            }))
        })();
        self.stub_response(
            seq,
            request,
            Some(result.unwrap_or(serde_json::json!({"position": 1, "indentation": 0}))),
        )
    }

    pub(crate) fn handle_toggle_line_comment(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // toggleLineComment returns TextChange[]
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_toggle_multiline_comment(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // toggleMultilineComment returns TextChange[]
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_comment_selection(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // commentSelection returns TextChange[]
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_uncomment_selection(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // uncommentSelection returns TextChange[]
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_smart_selection_range(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // getSmartSelectionRange returns SelectionRange
        // This is different from selectionRange - it's the smart version
        // Return a minimal selection range
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_syntactic_classifications(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // getSyntacticClassifications returns ClassifiedSpan[]
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_semantic_classifications(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // getSemanticClassifications returns ClassifiedSpan[]
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }

    pub(crate) fn handle_compiler_options_diagnostics(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // getCompilerOptionsDiagnostics returns Diagnostic[]
        self.stub_response(seq, request, Some(serde_json::json!([])))
    }
}
