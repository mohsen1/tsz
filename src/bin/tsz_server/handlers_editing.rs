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
                        Self::match_descriptor_at(source_text, i, &descriptor_texts, &mut results);
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
                                    && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'*')
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
            let line = request.arguments.get("line")?.as_u64()? as usize;
            let _offset = request.arguments.get("offset")?.as_u64().unwrap_or(1);
            let source_text = self.open_files.get(file)?;
            let generate_return = request
                .arguments
                .get("generateReturnInDocTemplate")
                .and_then(|v| v.as_bool())
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
                if !after_jsdoc.is_empty()
                    && after_jsdoc != "*"
                    && !after_jsdoc.starts_with("*/")
                {
                    // JSDoc has meaningful content - don't regenerate
                    return None;
                }
            }

            // Check for multi-declarator variable statements (e.g. `let a = 1, b = 2;`)
            // These should not extract params from initializer functions
            let is_multi_declarator = Self::is_multi_declarator_var(&decl_text, source_text, decl_offset);

            // Extract parameters from the declaration
            let params = if is_multi_declarator {
                Vec::new()
            } else {
                Self::extract_function_params(&decl_text, source_text, decl_offset)
            };

            // Check for return statement in function body if generate_return is enabled
            let has_return = if generate_return && !is_multi_declarator {
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
                lines.push(format!("{} * ", template_indent));

                for param in &params {
                    if is_js_file {
                        if let Some(name) = param.strip_prefix("...") {
                            lines.push(format!(
                                "{} * @param {{...any}} {}",
                                template_indent, name
                            ));
                        } else {
                            lines.push(format!(
                                "{} * @param {{any}} {}",
                                template_indent, param
                            ));
                        }
                    } else {
                        // For TS, strip the ... prefix
                        let name = param.strip_prefix("...").unwrap_or(param);
                        lines.push(format!("{} * @param {}", template_indent, name));
                    }
                }

                if has_return {
                    lines.push(format!("{} * @returns", template_indent));
                }

                lines.push(format!("{} */", template_indent));
                // Add trailing indent when cursor and declaration are on the same line
                // and cursor is at the very start of the line (only whitespace before it)
                if _decl_on_same_line && before_cursor.chars().all(|c| c == ' ' || c == '\t') {
                    lines.push(decl_indent.clone());
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
        self.stub_response(
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
        for i in paren_start..chars.len() {
            match chars[i] {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
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
                let name = format!("param{}", param_index);
                params.push(if is_rest {
                    format!("...{}", name)
                } else {
                    name
                });
                param_index += 1;
                continue;
            }

            // Extract identifier (before : or ? or = or ,)
            let name: String = s
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                .collect();

            if !name.is_empty() {
                params.push(if is_rest {
                    format!("...{}", name)
                } else {
                    name
                });
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
        let bare_keywords = [
            "if", "else", "while", "for", "do", "else if",
        ];
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
                ';' if depth == 0 => break,
                // Stop at newline when at depth 0 (end of statement without semicolon)
                '\n' if depth == 0 => break,
                '=' if depth == 0 => {
                    let prev = if i > 0 { chars[i - 1] } else { ' ' };
                    let next = chars.get(i + 1).copied().unwrap_or(' ');
                    // Exclude ==, !=, >=, <=, =>
                    if prev != '!' && prev != '<' && prev != '>' && prev != '='
                        && next != '=' && next != '>'
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
        let rest = if let Some(r) = decl.strip_prefix("var ") {
            r
        } else if let Some(r) = decl.strip_prefix("let ") {
            r
        } else if let Some(r) = decl.strip_prefix("const ") {
            r
        } else if let Some(r) = decl.strip_prefix("export var ") {
            r
        } else if let Some(r) = decl.strip_prefix("export let ") {
            r
        } else if let Some(r) = decl.strip_prefix("export const ") {
            r
        } else {
            return None;
        };

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
            match eq_search.find('=') {
                Some(pos) => decl_offset + pos + 1,
                None => return None,
            }
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
                    let _after_trimmed = after_close.trim();
                    // Check if paren wraps the expression:
                    // what follows should be end-of-statement or another closing paren
                    let after_on_line = after_close
                        .split('\n')
                        .next()
                        .unwrap_or("")
                        .trim();
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
                if let Some(ctor_pos) = class_body.find("constructor(")
                    .or_else(|| class_body.find("constructor ("))
                {
                    let ctor_decl = &full[brace_start + ctor_pos..];
                    return Some(ctor_decl.to_string());
                }
            }
            return Some(rhs.to_string());
        }

        Some(rhs.to_string())
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
                '>' => {
                    if angle_depth > 0 {
                        angle_depth -= 1;
                    }
                }
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

            // Smart indentation: compute brace/bracket/paren depth up to the target line
            // by scanning all lines before it, then adjust for the current line.
            // When target_line_idx >= lines.len() (e.g. cursor past EOF), scan all
            // available lines and treat the target as an empty line.
            let scan_end = target_line_idx.min(lines.len());
            let mut depth: i32 = 0;
            let mut in_block_comment = false;
            let _in_single_line_string = false;

            for line_idx in 0..scan_end {
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
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let start_line = request.arguments.get("startLine")?.as_u64()? as usize;
            let end_line = request.arguments.get("endLine")?.as_u64()? as usize;
            let end_offset = request
                .arguments
                .get("endOffset")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as usize;
            let source_text = self.open_files.get(file)?.clone();

            let all_lines: Vec<&str> = source_text.lines().collect();
            // Convert 1-based to 0-based
            let first = start_line.saturating_sub(1);
            let mut last = end_line
                .saturating_sub(1)
                .min(all_lines.len().saturating_sub(1));

            // When the selection ends at the beginning of a line (offset 1),
            // exclude that line from commenting (TypeScript behavior)
            if first != last && end_offset == 1 && last > 0 {
                last -= 1;
            }

            // Collect the lines in range, skipping empty lines for analysis
            let non_empty_lines: Vec<(usize, &str)> = (first..=last)
                .filter_map(|i| {
                    let line = all_lines.get(i)?;
                    if line.trim().is_empty() {
                        None
                    } else {
                        Some((i, *line))
                    }
                })
                .collect();

            if non_empty_lines.is_empty() {
                return Some(serde_json::json!([]));
            }

            // Check if ALL non-empty lines are commented (start with //)
            let all_commented = non_empty_lines
                .iter()
                .all(|(_, line)| line.trim_start().starts_with("//"));

            let mut edits = Vec::new();

            if all_commented {
                // Uncomment: remove the // and one preceding space if present
                for &(line_idx, line) in &non_empty_lines {
                    let ws_len = line.len() - line.trim_start().len();
                    let rest = &line[ws_len..];
                    if rest.starts_with("//") {
                        let one_line = line_idx + 1; // 1-based
                        let start_col = ws_len;
                        let end_col = ws_len + 2; // past the //
                        edits.push(serde_json::json!({
                            "start": {"line": one_line, "offset": start_col + 1},
                            "end": {"line": one_line, "offset": end_col + 1},
                            "newText": ""
                        }));
                    }
                }
            } else {
                // Comment: insert // replacing one space at min_indent position
                let min_indent = non_empty_lines
                    .iter()
                    .map(|(_, line)| line.len() - line.trim_start().len())
                    .min()
                    .unwrap_or(0);

                for &(line_idx, _) in &non_empty_lines {
                    let one_line = line_idx + 1; // 1-based
                        // Insert // at min_indent position (zero-length insertion)
                    let insert_col = min_indent + 1; // 1-based offset
                    edits.push(serde_json::json!({
                        "start": {"line": one_line, "offset": insert_col},
                        "end": {"line": one_line, "offset": insert_col},
                        "newText": "//"
                    }));
                }
            }

            Some(serde_json::json!(edits))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_toggle_multiline_comment(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let start_line = request.arguments.get("startLine")?.as_u64()? as usize;
            let start_offset = request.arguments.get("startOffset")?.as_u64()? as usize;
            let end_line = request.arguments.get("endLine")?.as_u64()? as usize;
            let end_offset = request.arguments.get("endOffset")?.as_u64()? as usize;
            let source_text = self.open_files.get(file)?.clone();

            // Compute byte offsets from 1-based line/offset
            let sel_start = Self::line_offset_to_byte(&source_text, start_line as u32, start_offset as u32);
            let sel_end = Self::line_offset_to_byte(&source_text, end_line as u32, end_offset as u32);
            let lines: Vec<&str> = source_text.lines().collect();

            // Find all /* */ comment ranges in the source
            let comment_ranges = Self::find_multiline_comments(&source_text);

            // Check if selection is fully inside an existing comment
            let enclosing = comment_ranges.iter().find(|(cs, ce)| *cs <= sel_start && sel_end <= *ce);

            // Find comments that overlap with the selection
            let overlapping: Vec<(usize, usize)> = comment_ranges
                .iter()
                .filter(|(cs, ce)| *cs < sel_end && *ce > sel_start)
                .map(|&(cs, ce)| (cs, ce))
                .collect();

            // Check if selection contains only comments and whitespace
            let only_comments_and_ws = if !overlapping.is_empty() && sel_start != sel_end {
                let _sel_bytes = source_text.get(sel_start..sel_end).map(str::as_bytes);
                let mut all_covered = true;
                let mut pos = sel_start;
                for &(cs, ce) in &overlapping {
                    // Check non-comment text before this comment
                    let gap_start = pos.max(sel_start);
                    let gap_end = cs.max(sel_start).min(sel_end);
                    if gap_start < gap_end {
                        let gap = &source_text[gap_start..gap_end];
                        if gap.chars().any(|c| !c.is_whitespace()) {
                            all_covered = false;
                            break;
                        }
                    }
                    pos = ce;
                }
                if all_covered && pos < sel_end {
                    let gap = &source_text[pos..sel_end];
                    if gap.chars().any(|c| !c.is_whitespace()) {
                        all_covered = false;
                    }
                }
                all_covered
            } else {
                false
            };

            let mut edits = Vec::new();

            if let Some(&(comment_start, comment_end)) = enclosing {
                // Selection is inside an existing comment → remove the comment
                // Remove /* at comment_start
                let (sl, so) = Self::byte_to_line_offset(&lines, comment_start)?;
                edits.push(serde_json::json!({
                    "start": {"line": sl, "offset": so},
                    "end": {"line": sl, "offset": so + 2},
                    "newText": ""
                }));
                // Remove */ at comment_end - 2
                let close_pos = comment_end - 2;
                let (el, eo) = Self::byte_to_line_offset(&lines, close_pos)?;
                edits.push(serde_json::json!({
                    "start": {"line": el, "offset": eo},
                    "end": {"line": el, "offset": eo + 2},
                    "newText": ""
                }));
            } else if only_comments_and_ws {
                // Selection only contains comments and whitespace → remove all comments
                // Process in reverse order to preserve positions
                for &(cs, ce) in overlapping.iter().rev() {
                    let close_pos = ce - 2;
                    let (el, eo) = Self::byte_to_line_offset(&lines, close_pos)?;
                    edits.push(serde_json::json!({
                        "start": {"line": el, "offset": eo},
                        "end": {"line": el, "offset": eo + 2},
                        "newText": ""
                    }));
                    let (sl, so) = Self::byte_to_line_offset(&lines, cs)?;
                    edits.push(serde_json::json!({
                        "start": {"line": sl, "offset": so},
                        "end": {"line": sl, "offset": so + 2},
                        "newText": ""
                    }));
                }
            } else if sel_start == sel_end {
                // Empty selection, not inside a comment → insert /**/
                let (sl, so) = Self::byte_to_line_offset(&lines, sel_start)?;
                edits.push(serde_json::json!({
                    "start": {"line": sl, "offset": so},
                    "end": {"line": sl, "offset": so},
                    "newText": "/**/"
                }));
            } else {
                // Selection not inside a comment → wrap with /* */
                // Handle any existing /* or */ inside the selection by
                // closing and reopening comments around them
                if overlapping.is_empty() {
                    // Simple case: no existing comments in selection
                    let (sl, so) = Self::byte_to_line_offset(&lines, sel_start)?;
                    let (el, eo) = Self::byte_to_line_offset(&lines, sel_end)?;
                    edits.push(serde_json::json!({
                        "start": {"line": sl, "offset": so},
                        "end": {"line": sl, "offset": so},
                        "newText": "/*"
                    }));
                    edits.push(serde_json::json!({
                        "start": {"line": el, "offset": eo},
                        "end": {"line": el, "offset": eo},
                        "newText": "*/"
                    }));
                } else {
                    // Complex case: close and reopen around existing comment boundaries
                    let (sl, so) = Self::byte_to_line_offset(&lines, sel_start)?;
                    edits.push(serde_json::json!({
                        "start": {"line": sl, "offset": so},
                        "end": {"line": sl, "offset": so},
                        "newText": "/*"
                    }));

                    for &(cs, ce) in &overlapping {
                        if cs > sel_start && cs < sel_end {
                            // Close our comment before the existing /*
                            let (cl, co) = Self::byte_to_line_offset(&lines, cs)?;
                            edits.push(serde_json::json!({
                                "start": {"line": cl, "offset": co},
                                "end": {"line": cl, "offset": co},
                                "newText": "*/"
                            }));
                        }
                        if ce > sel_start && ce < sel_end {
                            // Reopen our comment after the existing */
                            let (cl, co) = Self::byte_to_line_offset(&lines, ce)?;
                            edits.push(serde_json::json!({
                                "start": {"line": cl, "offset": co},
                                "end": {"line": cl, "offset": co},
                                "newText": "/*"
                            }));
                        }
                    }

                    let (el, eo) = Self::byte_to_line_offset(&lines, sel_end)?;
                    edits.push(serde_json::json!({
                        "start": {"line": el, "offset": eo},
                        "end": {"line": el, "offset": eo},
                        "newText": "*/"
                    }));
                }
            }

            Some(serde_json::json!(edits))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    /// Convert byte position to 1-based line/offset for multiline comment handler
    fn byte_to_line_offset(lines: &[&str], byte_pos: usize) -> Option<(usize, usize)> {
        let mut pos = 0usize;
        for (i, l) in lines.iter().enumerate() {
            let line_end = pos + l.len();
            if byte_pos <= line_end {
                return Some((i + 1, byte_pos - pos + 1)); // 1-based
            }
            pos = line_end + 1; // +1 for \n
        }
        None
    }

    /// Find all /* */ comment ranges as (start, end) byte positions
    fn find_multiline_comments(text: &str) -> Vec<(usize, usize)> {
        let bytes = text.as_bytes();
        let mut ranges = Vec::new();
        let mut i = 0;
        while i < bytes.len().saturating_sub(1) {
            if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                let start = i;
                i += 2;
                // Find matching */
                while i < bytes.len().saturating_sub(1) {
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        ranges.push((start, i));
                        break;
                    }
                    i += 1;
                }
            } else if bytes[i] == b'/' && bytes[i + 1] == b'/' {
                // Skip single-line comments to avoid matching /* inside them
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            } else if bytes[i] == b'"' || bytes[i] == b'\'' || bytes[i] == b'`' {
                // Skip string literals to avoid matching /* inside strings
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2; // skip escaped char
                        continue;
                    }
                    if bytes[i] == quote {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        ranges
    }

    pub(crate) fn handle_comment_selection(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let start_line = request.arguments.get("startLine")?.as_u64()? as usize;
            let start_offset = request.arguments.get("startOffset")?.as_u64()? as usize;
            let end_line = request.arguments.get("endLine")?.as_u64()? as usize;
            let end_offset = request.arguments.get("endOffset")?.as_u64()? as usize;
            let source_text = self.open_files.get(file)?.clone();

            let all_lines: Vec<&str> = source_text.lines().collect();
            let first = start_line.saturating_sub(1);
            let last = end_line
                .saturating_sub(1)
                .min(all_lines.len().saturating_sub(1));

            let mut edits = Vec::new();

            if first == last && start_offset != end_offset {
                // Single-line partial selection: use block comment /* ... */
                let line = all_lines.get(first)?;
                let sel_start = start_offset.saturating_sub(1);
                let sel_end = end_offset.saturating_sub(1).min(line.len());
                if sel_start < sel_end && sel_start < line.len() {
                    // Wrap selection in /* */
                    edits.push(serde_json::json!({
                        "start": {"line": start_line, "offset": start_offset},
                        "end": {"line": start_line, "offset": start_offset},
                        "newText": "/*"
                    }));
                    // After inserting /*, the end offset shifts by 2
                    edits.push(serde_json::json!({
                        "start": {"line": end_line, "offset": end_offset},
                        "end": {"line": end_line, "offset": end_offset},
                        "newText": "*/"
                    }));
                }
            } else {
                // Multi-line or cursor: add // to each non-empty line
                let non_empty_lines: Vec<(usize, &str)> = (first..=last)
                    .filter_map(|i| {
                        let line = all_lines.get(i)?;
                        if line.trim().is_empty() {
                            None
                        } else {
                            Some((i, *line))
                        }
                    })
                    .collect();

                if non_empty_lines.is_empty() {
                    return Some(serde_json::json!([]));
                }

                let min_indent = non_empty_lines
                    .iter()
                    .map(|(_, line)| line.len() - line.trim_start().len())
                    .min()
                    .unwrap_or(0);

                for &(line_idx, _) in &non_empty_lines {
                    let one_line = line_idx + 1;
                    if min_indent > 0 {
                        // Replace the space at min_indent-1 with //
                        edits.push(serde_json::json!({
                            "start": {"line": one_line, "offset": min_indent},
                            "end": {"line": one_line, "offset": min_indent + 1},
                            "newText": "//"
                        }));
                    } else {
                        edits.push(serde_json::json!({
                            "start": {"line": one_line, "offset": 1},
                            "end": {"line": one_line, "offset": 1},
                            "newText": "//"
                        }));
                    }
                }
            }

            Some(serde_json::json!(edits))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
    }

    pub(crate) fn handle_uncomment_selection(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let start_line = request.arguments.get("startLine")?.as_u64()? as usize;
            let end_line = request.arguments.get("endLine")?.as_u64()? as usize;
            let source_text = self.open_files.get(file)?.clone();

            let all_lines: Vec<&str> = source_text.lines().collect();
            let first = start_line.saturating_sub(1);
            let last = end_line
                .saturating_sub(1)
                .min(all_lines.len().saturating_sub(1));

            let mut edits = Vec::new();

            // Check for block comments /* */ in the range and remove them
            // Also check for line comments //
            for line_idx in first..=last {
                let line = match all_lines.get(line_idx) {
                    Some(l) => *l,
                    None => continue,
                };

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let one_line = line_idx + 1; // 1-based

                // Check for line comment: remove leading //
                let ws_len = line.len() - trimmed.len();
                if trimmed.starts_with("//") {
                    let remove_len = if trimmed.starts_with("// ") { 3 } else { 2 };
                    let start_off = ws_len + 1;
                    edits.push(serde_json::json!({
                        "start": {"line": one_line, "offset": start_off},
                        "end": {"line": one_line, "offset": start_off + remove_len},
                        "newText": ""
                    }));
                    continue;
                }

                // Check for block comments {/* ... */} or /* ... */
                // Find and remove /* and */ pairs
                let mut col = 0;
                let chars: Vec<char> = line.chars().collect();
                while col < chars.len() {
                    if col + 1 < chars.len() && chars[col] == '/' && chars[col + 1] == '*' {
                        // Remove /*
                        edits.push(serde_json::json!({
                            "start": {"line": one_line, "offset": col + 1},
                            "end": {"line": one_line, "offset": col + 3},
                            "newText": ""
                        }));
                        col += 2;
                    } else if col + 1 < chars.len() && chars[col] == '*' && chars[col + 1] == '/' {
                        // Remove */
                        edits.push(serde_json::json!({
                            "start": {"line": one_line, "offset": col + 1},
                            "end": {"line": one_line, "offset": col + 3},
                            "newText": ""
                        }));
                        col += 2;
                    } else {
                        col += 1;
                    }
                }
            }

            Some(serde_json::json!(edits))
        })();
        self.stub_response(seq, request, Some(result.unwrap_or(serde_json::json!([]))))
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
