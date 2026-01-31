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

            // Get tab size from options (default 4)
            let tab_size = request
                .arguments
                .get("options")
                .and_then(|o| o.get("tabSize"))
                .and_then(|v| v.as_u64())
                .unwrap_or(4) as usize;

            // Basic smart indentation: look at previous lines to determine context
            let lines: Vec<&str> = source_text.lines().collect();
            let target_line_idx = if line > 0 { line - 1 } else { 0 }; // 1-based to 0-based

            if target_line_idx >= lines.len() {
                return Some(serde_json::json!({"position": position, "indentation": 0}));
            }

            // Find the previous non-empty line
            let mut prev_line_idx = if target_line_idx > 0 {
                target_line_idx - 1
            } else {
                0
            };
            while prev_line_idx > 0 && lines[prev_line_idx].trim().is_empty() {
                prev_line_idx -= 1;
            }

            let prev_line = if prev_line_idx < lines.len() {
                lines[prev_line_idx]
            } else {
                ""
            };

            // Count leading spaces of previous line
            let prev_indent = prev_line.len() - prev_line.trim_start().len();

            // Determine if we should indent further
            let prev_trimmed = prev_line.trim();
            let should_indent_more = prev_trimmed.ends_with('{')
                || prev_trimmed.ends_with('(')
                || prev_trimmed.ends_with('[')
                || prev_trimmed.ends_with(':')
                || prev_trimmed.ends_with("=>")
                || prev_trimmed.ends_with(',');

            // Check if current line starts with closing bracket
            let current_trimmed = lines[target_line_idx].trim();
            let starts_with_closing = current_trimmed.starts_with('}')
                || current_trimmed.starts_with(')')
                || current_trimmed.starts_with(']');

            let indentation = if starts_with_closing {
                // Closing bracket: match the line with the opening bracket
                if prev_indent >= tab_size {
                    prev_indent - tab_size
                } else {
                    0
                }
            } else if should_indent_more {
                prev_indent + tab_size
            } else {
                prev_indent
            };

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
