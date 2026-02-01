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

            // Check if after-cursor text starts with a definite keyword
            let after_starts_with_keyword = ["function ", "class ", "interface ", "enum ", "type "]
                .iter()
                .any(|kw| after_cursor_on_line.starts_with(kw));

            if !after_cursor_on_line.is_empty()
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

            // Check if there's already a JSDoc comment before the cursor
            let before_pos = source_text[..offset].trim_end();
            if before_pos.ends_with("*/") {
                return None;
            }

            // Determine cursor indentation for the doc comment prefix
            let indent = if before_cursor.chars().all(|c| c == ' ' || c == '\t') {
                before_cursor
            } else {
                ""
            };

            // Extract parameters from the declaration
            let params = Self::extract_function_params(&decl_text, source_text, decl_offset);

            // Check for return statement in function body if generate_return is enabled
            let has_return = if generate_return {
                Self::function_has_return(&decl_text, source_text, decl_offset)
            } else {
                false
            };

            // Build the doc comment template
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
                lines.push(format!("{} * ", indent));

                for param in &params {
                    lines.push(format!("{} * @param {}", indent, param));
                }

                if has_return {
                    lines.push(format!("{} * @returns", indent));
                }

                // Add trailing indent when cursor and declaration are on the same line
                let cursor_on_same_line = !after_cursor_on_line.is_empty();
                lines.push(format!("{} */", indent));
                if cursor_on_same_line {
                    lines.push(format!("{}", decl_indent));
                }

                let new_text = lines.join("\n");
                // Caret offset: "/**\n<indent> * " -> caret is after " * " on second line
                let caret_offset = 3 + 1 + indent.len() + 3; // "/**" + "\n" + indent + " * "

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
    fn extract_function_params(decl: &str, _source: &str, _decl_offset: usize) -> Vec<String> {
        // Find the opening paren - handle methods, functions, constructors, arrow functions
        let paren_start = match Self::find_param_list_start(decl) {
            Some(pos) => pos,
            None => return Vec::new(),
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
        let mut unnamed_counter = 0;

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

            // Handle rest parameter
            let s = if s.starts_with("...") { &s[3..] } else { s };

            // Handle destructured params
            if s.starts_with('{') || s.starts_with('[') {
                unnamed_counter += 1;
                params.push(format!("param{}", unnamed_counter));
                continue;
            }

            // Extract identifier (before : or ? or = or ,)
            let name: String = s
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                .collect();

            if !name.is_empty() {
                params.push(name);
            }
        }

        params
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
        let mut paren_depth = 0;
        let mut brace_start = None;
        for (i, c) in full_decl.char_indices() {
            match c {
                '(' => paren_depth += 1,
                ')' => paren_depth -= 1,
                '{' if paren_depth == 0 => {
                    brace_start = Some(i);
                    break;
                }
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

            if target_line_idx >= lines.len() {
                return Some(serde_json::json!({"position": position, "indentation": 0}));
            }

            // Smart indentation: compute brace/bracket/paren depth up to the target line
            // by scanning all lines before it, then adjust for the current line.
            let mut depth: i32 = 0;
            let mut in_block_comment = false;
            let _in_single_line_string = false;

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
        let result = (|| -> Option<serde_json::Value> {
            let file = request.arguments.get("file")?.as_str()?;
            let start_line = request.arguments.get("startLine")?.as_u64()? as usize;
            let end_line = request.arguments.get("endLine")?.as_u64()? as usize;
            let source_text = self.open_files.get(file)?.clone();

            let all_lines: Vec<&str> = source_text.lines().collect();
            // Convert 1-based to 0-based
            let first = start_line.saturating_sub(1);
            let last = end_line
                .saturating_sub(1)
                .min(all_lines.len().saturating_sub(1));

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
                        // If there's a space before //, remove it too (symmetric with comment)
                        let start_col = if ws_len > 0 { ws_len - 1 } else { ws_len };
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
                    if min_indent > 0 {
                        // Replace the space at min_indent-1 with //
                        edits.push(serde_json::json!({
                            "start": {"line": one_line, "offset": min_indent},
                            "end": {"line": one_line, "offset": min_indent + 1},
                            "newText": "//"
                        }));
                    } else {
                        // No indent: insert // at position 0
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

    pub(crate) fn handle_toggle_multiline_comment(
        &mut self,
        seq: u64,
        request: &TsServerRequest,
    ) -> TsServerResponse {
        // TODO: Implement multiline comment toggle
        self.stub_response(seq, request, Some(serde_json::json!([])))
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
