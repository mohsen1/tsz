use crate::emitter::Printer;
use tsz_parser::parser::node::{FunctionData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::ScannerState;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn recovered_yield_call_statement_text(
        &self,
        node: &Node,
    ) -> Option<String> {
        let expr_stmt = self.arena.get_expression_statement(node)?;
        let expr_node = self.arena.get(expr_stmt.expression)?;
        let is_recovered_yield = if expr_node.kind == syntax_kind_ext::YIELD_EXPRESSION {
            let yield_expr = self.arena.get_unary_expr_ex(expr_node)?;
            yield_expr.expression.is_none()
        } else {
            self.arena
                .get_identifier(expr_node)
                .is_some_and(|ident| ident.escaped_text == "yield")
        };
        if !is_recovered_yield {
            return None;
        }

        let text = self.source_text?;
        let bytes = text.as_bytes();
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let mut pos = start.checked_add("yield".len())?;
        while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t') {
            pos += 1;
        }
        if bytes.get(pos) != Some(&b'(') {
            return None;
        }

        let mut depth = 0_i32;
        let mut end = pos;
        while end < bytes.len() {
            match bytes[end] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        end += 1;
                        break;
                    }
                }
                b'\n' | b'\r' => return None,
                _ => {}
            }
            end += 1;
        }
        if depth != 0 {
            return None;
        }

        let recovered = crate::safe_slice::slice(text, start, end).ok()?.trim_end();
        Some(format!("{recovered};"))
    }

    pub(in crate::emitter) fn recovered_invalid_jsx_closing_fragment_statement_text(
        &self,
        node: &Node,
    ) -> Option<String> {
        if node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return None;
        }

        let text = self.source_text?;
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let tail = text.get(start..)?;
        tail.starts_with("</>").then(|| " > ;".to_string())
    }

    pub(in crate::emitter) fn is_invalid_export_recovery_statement(&self, node: &Node) -> bool {
        self.arena.get_export_decl(node).is_some_and(|export| {
            export.export_clause.is_none() && export.module_specifier.is_none()
        }) || self.is_recovered_invalid_numeric_export_declaration_name(node)
    }

    pub(in crate::emitter) fn emit_recovered_invalid_numeric_declaration_name_statement(
        &mut self,
        node: &Node,
    ) -> bool {
        let Some(statements) = self.recovered_invalid_numeric_declaration_name_statements(node)
        else {
            return false;
        };

        if !self.writer.is_at_line_start() {
            self.write_line();
        }
        for statement in statements {
            self.write(&statement);
            self.write_line();
        }
        true
    }

    pub(in crate::emitter) fn emit_recovered_reserved_variable_declaration_name_statement(
        &mut self,
        node: &Node,
    ) -> bool {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return false;
        };
        let [decl_list_idx] = var_stmt.declarations.nodes.as_slice() else {
            return false;
        };
        let Some(decl_list_node) = self.arena.get(*decl_list_idx) else {
            return false;
        };
        let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
            return false;
        };
        let [decl_idx] = decl_list.declarations.nodes.as_slice() else {
            return false;
        };
        let Some(decl_node) = self.arena.get(*decl_idx) else {
            return false;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if decl.initializer.is_none() {
            return false;
        }
        let Some(name_node) = self.arena.get(decl.name) else {
            return false;
        };
        if self.arena.get_identifier(name_node).is_none() {
            return false;
        }
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(keyword) = self.reserved_keyword_text_at_declaration_start(decl_node) else {
            return false;
        };

        if !self.writer.is_at_line_start() {
            self.write_line();
        }
        self.write("var ;");
        self.write_line();
        self.write(keyword);
        self.write(" ;");
        self.write_line();
        self.emit_expression(decl.initializer);
        self.write_semicolon();
        true
    }

    pub(in crate::emitter) fn emit_recovered_reserved_array_binding_variable_statement(
        &mut self,
        node: &Node,
    ) -> bool {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return false;
        };
        let [decl_list_idx] = var_stmt.declarations.nodes.as_slice() else {
            return false;
        };
        let Some(decl_list_node) = self.arena.get(*decl_list_idx) else {
            return false;
        };
        let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
            return false;
        };
        let [decl_idx] = decl_list.declarations.nodes.as_slice() else {
            return false;
        };
        let Some(decl_node) = self.arena.get(*decl_idx) else {
            return false;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        let Some(name_node) = self.arena.get(decl.name) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return false;
        }
        let Some((keywords, initializer_text)) =
            self.recovered_reserved_array_binding_parts(node, name_node)
        else {
            return false;
        };
        if keywords.is_empty() || keywords.len() > 3 {
            return false;
        }

        if !self.writer.is_at_line_start() {
            self.write_line();
        }
        self.write("var [];");
        self.write_line();
        self.write(keywords[0]);
        self.write(";");
        self.write_line();
        match keywords.as_slice() {
            [_] => {}
            [_, second] => self.emit_recovered_reserved_array_binding_condition(second),
            [_, second, third] => {
                self.write(second);
                self.write(" (, )");
                self.write_line();
                self.increase_indent();
                self.emit_recovered_reserved_array_binding_condition(third);
                self.decrease_indent();
            }
            _ => return false,
        }
        self.write(&initializer_text);
        self.write_semicolon();
        self.suppress_next_anonymous_enum_var_after_recovered_array_binding = true;
        true
    }

    fn emit_recovered_reserved_array_binding_condition(&mut self, keyword: &str) {
        self.write(keyword);
        self.write(" ()");
        self.write_line();
        self.increase_indent();
        self.write(";");
        self.write_line();
        self.decrease_indent();
    }

    /// Collect the reserved-word keywords that appear as *binding-element
    /// names* inside an array binding pattern, plus the trailing initializer
    /// text.
    ///
    /// Only reserved words at the top level of the pattern and in name
    /// position are binding names. Reserved words sitting in a default-value
    /// initializer (`[a = true]`) or in the right-hand initializer expression
    /// (`= [1, null]`) are values, not names, and must be ignored. The scan
    /// therefore tracks bracket/brace/paren depth, only collects reserved words
    /// at the pattern's top level (depth 1) outside a default value, and stops
    /// the name-collection phase at the matching `]`. The initializer text is
    /// read from source (the AST is unreliable here because the malformed
    /// pattern derails the parser).
    ///
    /// The scan window runs from the pattern's `[` to the end of the enclosing
    /// variable statement. The statement span is reliable even when the
    /// malformed pattern truncates `name_node.end`, and bounding it avoids
    /// copying the rest of the file into the scanner for every array-binding
    /// declaration.
    fn recovered_reserved_array_binding_parts(
        &self,
        statement: &Node,
        name_node: &Node,
    ) -> Option<(Vec<&'static str>, String)> {
        let text = self.source_text?;
        let start = self.skip_trivia_forward(name_node.pos, name_node.end) as usize;
        let source = text.get(start..statement.end as usize)?;
        let mut scanner = ScannerState::new(source.to_string(), true);
        let mut keywords = Vec::new();
        let mut depth: i32 = 0;
        let mut pattern_closed = false;
        let mut in_default_value = false;
        let mut initializer_start = None;
        let mut initializer_end = None;

        loop {
            let token = scanner.scan();
            if token == SyntaxKind::EndOfFileToken {
                break;
            }
            if !pattern_closed {
                match token {
                    SyntaxKind::OpenBracketToken
                    | SyntaxKind::OpenBraceToken
                    | SyntaxKind::OpenParenToken => depth += 1,
                    SyntaxKind::CloseBracketToken
                    | SyntaxKind::CloseBraceToken
                    | SyntaxKind::CloseParenToken => {
                        depth -= 1;
                        if depth <= 0 {
                            pattern_closed = true;
                        }
                    }
                    SyntaxKind::EqualsToken if depth == 1 => in_default_value = true,
                    SyntaxKind::CommaToken if depth == 1 => in_default_value = false,
                    _ if depth == 1
                        && !in_default_value
                        && tsz_scanner::token_is_reserved_word(token) =>
                    {
                        keywords.extend(tsz_scanner::keyword_to_text_static(token));
                    }
                    _ => {}
                }
            } else {
                match token {
                    SyntaxKind::EqualsToken if initializer_start.is_none() => {
                        initializer_start = Some(scanner.get_token_end());
                    }
                    SyntaxKind::SemicolonToken => {
                        initializer_end = Some(scanner.get_token_start());
                        break;
                    }
                    _ => {}
                }
            }
        }

        let initializer_start = initializer_start?;
        let initializer_end = initializer_end.unwrap_or(scanner.get_pos());
        let initializer_text = source
            .get(initializer_start..initializer_end)?
            .trim()
            .to_string();
        if initializer_text.is_empty() {
            return None;
        }
        Some((keywords, initializer_text))
    }

    pub(in crate::emitter) fn emit_recovered_reserved_import_equals_declaration_name(
        &mut self,
        node: &Node,
    ) -> bool {
        if node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return false;
        }
        let Some(import) = self.arena.get_import_decl(node) else {
            return false;
        };
        if import.module_specifier.is_some() {
            return false;
        }
        let Some(name_node) = self.arena.get(import.import_clause) else {
            return false;
        };
        let Some(keyword) = self.reserved_keyword_text_from_source_span(name_node) else {
            return false;
        };
        let Some(require_text) = self.recovered_import_equals_require_call_text(node) else {
            return false;
        };

        if !self.writer.is_at_line_start() {
            self.write_line();
        }
        self.write("require();");
        self.write_line();
        self.write(keyword);
        self.write(" ( = ");
        self.write(&require_text);
        self.write(")");
        self.write_line();
        self.increase_indent();
        self.write(";");
        self.decrease_indent();
        self.write_line();
        true
    }

    fn recovered_import_equals_require_call_text(&self, node: &Node) -> Option<String> {
        let line = self.source_line_from_node(node)?;
        let equals = line.find('=')?;
        let tail = line[equals + 1..].trim().trim_end_matches(';').trim();
        if !tail.starts_with("require") {
            return None;
        }
        Some(tail.to_string())
    }

    /// Return the reserved keyword text when the declaration's name token is
    /// *exactly* a reserved word (e.g. `var typeof = 10`).
    ///
    /// The whole leading identifier token is read — including the
    /// digit/`_`/`$` characters that continue it — so ordinary identifiers
    /// that merely begin with a keyword (`var1`, `function1`) read as a single
    /// non-reserved identifier rather than matching the keyword prefix.
    /// Reading from the source (rather than the name node's span) also works
    /// when the parser synthesizes an empty name node for a keyword used in
    /// name position.
    fn reserved_keyword_text_at_declaration_start(&self, node: &Node) -> Option<&'static str> {
        let text = self.source_text?;
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let bytes = text.as_bytes();
        let mut end = start;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || bytes[end] == b'$')
        {
            end += 1;
        }
        let keyword = crate::safe_slice::slice(text, start, end).ok()?;
        let token = tsz_scanner::string_to_token(keyword);
        tsz_scanner::token_is_reserved_word(token)
            .then(|| tsz_scanner::keyword_to_text_static(token))?
    }

    fn source_line_from_node(&self, node: &Node) -> Option<&str> {
        let text = self.source_text?;
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let bytes = text.as_bytes();
        let mut end = start;
        while end < bytes.len() && !matches!(bytes[end], b'\n' | b'\r') {
            end += 1;
        }
        crate::safe_slice::slice(text, start, end).ok()
    }

    pub(in crate::emitter) fn emit_recovered_reserved_function_declaration_name(
        &mut self,
        node: &Node,
        func: &FunctionData,
    ) -> bool {
        let Some(name_node) = self.arena.get(func.name) else {
            return false;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(keyword) = self.reserved_keyword_text_from_source_span(name_node) else {
            return false;
        };
        let Some(body_node) = self.arena.get(func.body) else {
            return false;
        };
        if !self
            .arena
            .get_block(body_node)
            .is_some_and(|block| block.statements.nodes.is_empty())
        {
            return false;
        }

        self.write("function ");
        self.write("(");
        let search_start = func
            .parameters
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map_or(node.pos, |n| n.pos);
        self.function_scope_depth += 1;
        self.emit_function_parameters_with_trailing_comments(
            &func.parameters.nodes,
            name_node.end,
            search_start,
            body_node.pos,
        );
        self.function_scope_depth -= 1;
        self.write(") { }");
        self.write_line();
        self.write(keyword);
        self.write(" () => { };");
        true
    }

    pub(in crate::emitter) fn emit_recovered_this_parameter_initializer_function_declaration(
        &mut self,
        node: &Node,
        func: &FunctionData,
    ) -> bool {
        if !self.function_has_recovered_this_parameter_new_initializer(node, func) {
            return false;
        }

        self.write("();");
        self.write_line();
        if let Some(return_type) = self.recovered_function_return_type_text(func) {
            self.write(&return_type);
            self.write_semicolon();
            self.write_line();
        }
        self.emit(func.body);
        true
    }

    fn function_has_recovered_this_parameter_new_initializer(
        &self,
        node: &Node,
        func: &FunctionData,
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(body_node) = self.arena.get(func.body) else {
            return false;
        };
        let Some(first_param_idx) = func.parameters.nodes.first().copied() else {
            return false;
        };
        let Some(first_param_node) = self.arena.get(first_param_idx) else {
            return false;
        };
        let Some(first_param) = self.arena.get_parameter(first_param_node) else {
            return false;
        };
        if first_param.initializer.is_some() || !self.parameter_name_is_this(first_param.name) {
            return false;
        }

        let Some(open_paren) = self.function_parameter_open_paren(func, node, body_node) else {
            return false;
        };
        let Some(close_paren) =
            Self::matching_close_paren(text, open_paren, body_node.pos as usize)
        else {
            return false;
        };
        let first_param_end = Self::first_parameter_text_end(text, open_paren + 1, close_paren);
        let Some(first_param_text) = text.get(open_paren + 1..first_param_end) else {
            return false;
        };
        let Some(equals) = Self::top_level_equals(first_param_text) else {
            return false;
        };
        Self::starts_with_new_call_initializer(&first_param_text[equals + 1..])
    }

    fn parameter_name_is_this(&self, name: tsz_parser::parser::NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name) else {
            return false;
        };
        if name_node.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        }
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(text) = self.source_text else {
            return false;
        };
        crate::safe_slice::slice(text, name_node.pos as usize, name_node.end as usize)
            .is_ok_and(|name_text| name_text.trim() == "this")
    }

    fn function_parameter_open_paren(
        &self,
        func: &FunctionData,
        node: &Node,
        body_node: &Node,
    ) -> Option<usize> {
        let text = self.source_text?;
        let start = if func.name.is_some() {
            self.arena.get(func.name).map_or(node.pos, |name| name.end)
        } else {
            node.pos
        } as usize;
        let end = body_node.pos as usize;
        text.get(start..end)?
            .as_bytes()
            .iter()
            .position(|&byte| byte == b'(')
            .map(|offset| start + offset)
    }

    fn matching_close_paren(text: &str, open_paren: usize, limit: usize) -> Option<usize> {
        let bytes = text.as_bytes();
        let mut depth = 0_i32;
        let end = limit.min(bytes.len());
        let mut pos = open_paren;
        while pos < end {
            match bytes[pos] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(pos);
                    }
                }
                b'"' | b'\'' | b'`' => {
                    pos = Self::skip_string_like(bytes, pos, end);
                }
                b'/' if bytes.get(pos + 1) == Some(&b'/') => {
                    pos += 2;
                    while pos < end && !matches!(bytes[pos], b'\n' | b'\r') {
                        pos += 1;
                    }
                    continue;
                }
                b'/' if bytes.get(pos + 1) == Some(&b'*') => {
                    pos += 2;
                    while pos + 1 < end && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                        pos += 1;
                    }
                    pos = (pos + 2).min(end);
                    continue;
                }
                _ => {}
            }
            pos += 1;
        }
        None
    }

    fn first_parameter_text_end(text: &str, start: usize, close_paren: usize) -> usize {
        let bytes = text.as_bytes();
        let mut depth = 0_i32;
        let mut pos = start;
        while pos < close_paren {
            match bytes[pos] {
                b',' if depth == 0 => return pos,
                b'(' | b'[' | b'{' | b'<' => depth += 1,
                b')' | b']' | b'}' | b'>' => depth -= 1,
                b'"' | b'\'' | b'`' => {
                    pos = Self::skip_string_like(bytes, pos, close_paren);
                }
                b'/' if bytes.get(pos + 1) == Some(&b'/') => {
                    pos += 2;
                    while pos < close_paren && !matches!(bytes[pos], b'\n' | b'\r') {
                        pos += 1;
                    }
                    continue;
                }
                b'/' if bytes.get(pos + 1) == Some(&b'*') => {
                    pos += 2;
                    while pos + 1 < close_paren && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                        pos += 1;
                    }
                    pos = (pos + 2).min(close_paren);
                    continue;
                }
                _ => {}
            }
            pos += 1;
        }
        close_paren
    }

    fn top_level_equals(text: &str) -> Option<usize> {
        let bytes = text.as_bytes();
        let mut depth = 0_i32;
        let mut pos = 0;
        while pos < bytes.len() {
            match bytes[pos] {
                b'=' if depth == 0 => return Some(pos),
                b'(' | b'[' | b'{' | b'<' => depth += 1,
                b')' | b']' | b'}' | b'>' => depth -= 1,
                b'"' | b'\'' | b'`' => {
                    pos = Self::skip_string_like(bytes, pos, bytes.len());
                }
                _ => {}
            }
            pos += 1;
        }
        None
    }

    fn starts_with_new_call_initializer(text: &str) -> bool {
        let trimmed = text.trim_start();
        let Some(after_new) = trimmed.strip_prefix("new") else {
            return false;
        };
        if !after_new
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            return false;
        }
        let Some(open_paren) = after_new.find('(') else {
            return false;
        };
        let args_start = open_paren + 1;
        let Some(close_paren) = Self::matching_close_paren(after_new, open_paren, after_new.len())
        else {
            return false;
        };
        after_new[args_start..close_paren].trim().is_empty()
            && after_new[close_paren + 1..].trim().is_empty()
    }

    fn recovered_function_return_type_text(&self, func: &FunctionData) -> Option<String> {
        let text = self.source_text?;
        let type_node = self.arena.get(func.type_annotation)?;
        crate::safe_slice::slice(text, type_node.pos as usize, type_node.end as usize)
            .ok()
            .map(str::trim)
            .filter(|type_text| !type_text.is_empty())
            .map(ToString::to_string)
    }

    fn skip_string_like(bytes: &[u8], start: usize, limit: usize) -> usize {
        let quote = bytes[start];
        let mut pos = start + 1;
        while pos < limit {
            if bytes[pos] == b'\\' {
                pos = (pos + 2).min(limit);
                continue;
            }
            if bytes[pos] == quote {
                return pos;
            }
            pos += 1;
        }
        limit
    }

    fn reserved_keyword_text_from_source_span(&self, node: &Node) -> Option<&'static str> {
        let text = self.source_text?;
        let keyword = crate::safe_slice::slice(text, node.pos as usize, node.end as usize)
            .ok()?
            .trim();
        let token = tsz_scanner::string_to_token(keyword);
        tsz_scanner::token_is_reserved_word(token)
            .then(|| tsz_scanner::keyword_to_text_static(token))?
    }

    pub(in crate::emitter) fn emit_recovered_reserved_namespace_declaration_name(
        &mut self,
        node: &Node,
    ) -> bool {
        let Some(module) = self.arena.get_module(node) else {
            return false;
        };
        let Some(name_node) = self.arena.get(module.name) else {
            return false;
        };
        let Some(keyword) = self.reserved_keyword_text_from_source_span(name_node) else {
            return false;
        };
        if module.body.is_none() {
            return false;
        }
        let Some(body_node) = self.arena.get(module.body) else {
            return false;
        };
        if !self.arena.get_module_block(body_node).is_some_and(|block| {
            block
                .statements
                .as_ref()
                .is_none_or(|stmts| stmts.nodes.is_empty())
        }) {
            return false;
        }

        if !self.writer.is_at_line_start() {
            self.write_line();
        }
        self.write("namespace;");
        self.write_line();
        self.write(keyword);
        self.write(" {};");
        self.write_line();
        true
    }

    fn recovered_invalid_numeric_declaration_name_statements(
        &self,
        node: &Node,
    ) -> Option<Vec<String>> {
        let keywords: &[&str] = if node.kind == syntax_kind_ext::MODULE_DECLARATION {
            &["namespace", "module"]
        } else if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
            &["interface"]
        } else if node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            &["type"]
        } else if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            &["namespace", "module", "interface", "type"]
        } else {
            return None;
        };

        let text = self.source_text?;
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let bytes = text.as_bytes();
        let mut line_end = start;
        while line_end < bytes.len() && !matches!(bytes[line_end], b'\n' | b'\r') {
            line_end += 1;
        }
        let line = crate::safe_slice::slice(text, start, line_end)
            .ok()?
            .trim_start();
        let line = strip_recovery_keyword(line, "export")
            .map(str::trim_start)
            .unwrap_or(line);

        for keyword in keywords {
            let Some(rest) = strip_recovery_keyword(line, keyword) else {
                continue;
            };
            let rest = rest.trim_start();
            let (number, after_number) = take_numeric_literal_prefix(rest)?;
            if !is_empty_recovered_block(after_number) {
                return None;
            }
            return Some(vec![
                format!("{keyword};"),
                format!("{number};"),
                "{ }".to_string(),
            ]);
        }

        None
    }

    pub(in crate::emitter) fn is_recovered_invalid_numeric_export_declaration_name(
        &self,
        node: &Node,
    ) -> bool {
        if !self.invalid_numeric_declaration_has_export_modifier(node) {
            return false;
        }
        self.recovered_invalid_numeric_declaration_name_statements(node)
            .is_some()
    }

    pub(in crate::emitter) fn emit_recovered_unparsed_token_assignment_statement(
        &mut self,
        node: &Node,
        next: Option<&Node>,
    ) -> bool {
        let Some(next) = next else {
            return false;
        };
        let Some(expr_stmt) = self.arena.get_expression_statement(node) else {
            return false;
        };
        let Some(next_expr_stmt) = self.arena.get_expression_statement(next) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_stmt.expression) else {
            return false;
        };
        let Some(next_expr_node) = self.arena.get(next_expr_stmt.expression) else {
            return false;
        };
        if expr_node.end <= node.end || next_expr_node.pos >= expr_node.end {
            return false;
        }
        if !self.has_trailing_recovered_equality_token(node) {
            return false;
        }

        self.emit_expression_in_statement_position(expr_stmt.expression);
        self.write(" = ");
        self.emit_expression_in_statement_position(next_expr_stmt.expression);
        self.write_semicolon();
        true
    }

    pub(in crate::emitter) fn recovered_ambient_class_parenthesized_tail_text(
        &self,
        node: &Node,
    ) -> Option<String> {
        if node.kind != syntax_kind_ext::CLASS_DECLARATION {
            return None;
        }

        let class = self.arena.get_class(node)?;
        if !self.arena.is_declare(&class.modifiers) || class.heritage_clauses.is_some() {
            return None;
        }

        let text = self.source_text?;
        let cursor = class.type_parameters.as_ref().map_or_else(
            || self.arena.get(class.name).map(|name| name.end),
            |params| Some(params.end),
        )?;
        let start = self.skip_trivia_forward(cursor, node.end) as usize;
        let bytes = text.as_bytes();
        if bytes.get(start) != Some(&b'(') {
            return None;
        }

        let mut depth = 0_i32;
        let mut end = start;
        while end < bytes.len() {
            match bytes[end] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        end += 1;
                        break;
                    }
                }
                b'\n' | b'\r' => return None,
                _ => {}
            }
            end += 1;
        }
        if depth != 0 || end > node.end as usize {
            return None;
        }

        let recovered = crate::safe_slice::slice(text, start, end).ok()?.trim_end();
        Some(format!("{recovered};"))
    }

    fn has_trailing_recovered_equality_token(&self, node: &Node) -> bool {
        let text = match self.source_text {
            Some(text) => text,
            None => return false,
        };
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let end = (node.end as usize).min(text.len());
        let current = match text.get(start..end) {
            Some(current) => current.trim_end(),
            None => return false,
        };
        if current.contains('\n') || current.contains('\r') {
            return false;
        }
        current
            .as_bytes()
            .iter()
            .rev()
            .take_while(|&&byte| byte == b'=')
            .count()
            >= 4
    }

    pub(in crate::emitter) fn is_recovered_yield_operand_statement(&self, node: &Node) -> bool {
        if node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return false;
        }
        let Some(text) = self.source_text else {
            return false;
        };
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        text.as_bytes().get(start) == Some(&b'(')
    }

    pub(in crate::emitter) fn recovered_trailing_binary_operator_text(
        &self,
        previous: &Node,
        current: &Node,
    ) -> Option<String> {
        if previous.kind != syntax_kind_ext::EXPRESSION_STATEMENT
            || current.kind != syntax_kind_ext::EXPRESSION_STATEMENT
        {
            return None;
        }

        // The previous statement's source text can end with `/` for reasons
        // unrelated to a trailing division operator: a JSX expression statement
        // closed by `</` (e.g. `<>hi</div>`, where the mismatched named closing
        // tag is left to reparse as the next statement) ends with the `/` of its
        // closing fragment/element. Treating that `/` as a binary operator would
        // splice the JSX closing slash onto the following statement. Skip the
        // trailing-operator recovery for JSX expression statements.
        if self
            .arena
            .get_expression_statement(previous)
            .and_then(|stmt| self.arena.get(stmt.expression))
            .is_some_and(|expr| {
                matches!(
                    expr.kind,
                    syntax_kind_ext::JSX_ELEMENT
                        | syntax_kind_ext::JSX_FRAGMENT
                        | syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                )
            })
        {
            return None;
        }

        let text = self.source_text?;
        let bytes = text.as_bytes();
        let previous_start = (previous.pos as usize).min(bytes.len());
        let mut previous_end = (previous.end as usize).min(bytes.len());
        while previous_end > previous_start
            && matches!(bytes[previous_end - 1], b' ' | b'\t' | b'\r' | b'\n')
        {
            previous_end -= 1;
        }

        let previous_text = text.get(previous_start..previous_end)?;
        let operator = [
            "instanceof",
            "===",
            "!==",
            ">>>",
            "&&",
            "||",
            "??",
            "==",
            "!=",
            "<=",
            ">=",
            "<<",
            ">>",
            "**",
            "in",
            "|",
            "&",
            "^",
            "<",
            ">",
            "+",
            "-",
            "*",
            "/",
            "%",
        ]
        .into_iter()
        .find(|operator| previous_text.ends_with(operator))?;

        let mut start = previous_end.checked_sub(operator.len())?;

        if (operator == "+" || operator == "-")
            && start > previous_start
            && bytes.get(start - 1) == operator.as_bytes().first()
        {
            return None;
        }

        while start > previous_start && matches!(bytes[start - 1], b' ' | b'\t') {
            start -= 1;
        }

        let current_expr = self
            .arena
            .get_expression_statement(current)
            .and_then(|stmt| self.arena.get(stmt.expression))?;
        let end = (current_expr.pos as usize).min(bytes.len());
        if end < previous_end {
            return None;
        }

        let recovered = text.get(start..end)?;
        if recovered.contains('\n') || recovered.contains('\r') {
            return None;
        }
        Some(recovered.to_string())
    }

    pub(in crate::emitter) fn recovered_leading_arrow_chain_text(
        &self,
        previous: &Node,
        current: &Node,
    ) -> Option<String> {
        if previous.kind != syntax_kind_ext::EXPRESSION_STATEMENT
            || current.kind != syntax_kind_ext::EXPRESSION_STATEMENT
        {
            return None;
        }

        let text = self.source_text?;
        let previous_text = text.get(previous.pos as usize..previous.end as usize)?;
        if !previous_text.trim_end().ends_with('?') {
            return None;
        }

        let current_expr = self
            .arena
            .get_expression_statement(current)
            .and_then(|stmt| self.arena.get(stmt.expression))?;
        let start = (previous.end as usize).min(text.len());
        let end = (current_expr.pos as usize).min(text.len());
        if start >= end {
            return None;
        }

        let gap = text.get(start..end)?.trim();
        if !gap.ends_with("=>") {
            return None;
        }

        let mut parts = gap.split("=>").map(str::trim).collect::<Vec<_>>();
        if parts.len() < 2 || parts.pop() != Some("") || parts.iter().any(|part| part.is_empty()) {
            return None;
        }

        Some(format!("{} => ", parts.join(" => ")))
    }

    fn invalid_numeric_declaration_has_export_modifier(&self, node: &Node) -> bool {
        if let Some(text) = self.source_text {
            let start = self.skip_trivia_forward(node.pos, node.end) as usize;
            if let Some(line) = text.get(start..) {
                let line = line.trim_start();
                if strip_recovery_keyword(line, "export").is_some() {
                    return true;
                }
            }
        }
        if node.kind == syntax_kind_ext::MODULE_DECLARATION {
            return self.arena.get_module(node).is_some_and(|module| {
                self.arena
                    .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword)
            });
        }
        if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
            return self.arena.get_interface(node).is_some_and(|interface| {
                self.arena
                    .has_modifier(&interface.modifiers, SyntaxKind::ExportKeyword)
            });
        }
        if node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            return self.arena.get_type_alias(node).is_some_and(|alias| {
                self.arena
                    .has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword)
            });
        }
        false
    }

    pub(in crate::emitter) fn recovered_debugger_namespace_line(
        &self,
        node: &Node,
    ) -> Option<(u32, Option<&'a str>)> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let mut line_end = start;
        while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
            line_end += 1;
        }

        let line = crate::safe_slice::slice(text, start, line_end).ok()?;
        let trimmed = line.trim_start();
        let rest = trimmed.strip_prefix("declare namespace debugger")?;
        if rest
            .as_bytes()
            .first()
            .is_some_and(|&b| tsz_common::text_scan::is_ascii_identifier_continue(b))
        {
            return None;
        }

        let trailing_comment = line
            .find("//")
            .map(|comment_start| line[comment_start..].trim());
        Some((line_end as u32, trailing_comment))
    }
}

fn strip_recovery_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(keyword)?;
    if rest
        .as_bytes()
        .first()
        .is_some_and(|&b| tsz_common::text_scan::is_ascii_identifier_continue(b))
    {
        return None;
    }
    Some(rest)
}

fn take_numeric_literal_prefix(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let mut end = 1;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'.' | b'_' | b'+' | b'-'))
    {
        end += 1;
    }
    Some((&text[..end], &text[end..]))
}

fn is_empty_recovered_block(text: &str) -> bool {
    let trimmed = text.trim_start();
    let Some(after_open) = trimmed.strip_prefix('{') else {
        return false;
    };
    let Some(close_pos) = after_open.find('}') else {
        return false;
    };
    after_open[..close_pos].trim().is_empty() && after_open[close_pos + 1..].trim().is_empty()
}
