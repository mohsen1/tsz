use crate::emitter::Printer;
use tsz_parser::parser::node::{FunctionData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

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
        let Some((first_keyword, second_keyword, initializer_text)) =
            self.recovered_reserved_array_binding_source_parts(node)
        else {
            return false;
        };

        if !self.writer.is_at_line_start() {
            self.write_line();
        }
        self.write("var [];");
        self.write_line();
        self.write(first_keyword);
        self.write(";");
        self.write_line();
        self.write(second_keyword);
        self.write(" ()");
        self.write_line();
        self.increase_indent();
        self.write(";");
        self.write_line();
        self.decrease_indent();
        self.write(&initializer_text);
        self.write(";");
        self.suppress_next_anonymous_enum_var_after_recovered_array_binding = true;
        true
    }

    fn recovered_reserved_array_binding_source_parts(
        &self,
        node: &Node,
    ) -> Option<(&'static str, &'static str, String)> {
        let text = self.source_text?;
        let line = self.source_line_from_node(node)?;
        let open = line.find('[')?;
        let close = line[open..].find(']')? + open;
        let binding = &line[open + 1..close];
        let mut parts = binding.split(',').map(str::trim);
        let first = self.reserved_keyword_text(parts.next()?)?;
        let second = self.reserved_keyword_text(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        let equals = line[close..].find('=')? + close;
        let initializer = line[equals + 1..].trim().trim_end_matches(';').trim();
        if initializer.is_empty() {
            return None;
        }
        let source_start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let absolute_initializer_start = text[source_start..].find(initializer)? + source_start;
        let initializer_end = absolute_initializer_start + initializer.len();
        let initializer_text =
            crate::safe_slice::slice(text, absolute_initializer_start, initializer_end)
                .ok()?
                .trim()
                .to_string();
        Some((first, second, initializer_text))
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

    fn reserved_keyword_text_at_declaration_start(&self, node: &Node) -> Option<&'static str> {
        let text = self.source_text?;
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let bytes = text.as_bytes();
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_alphabetic() {
            end += 1;
        }
        let keyword = crate::safe_slice::slice(text, start, end).ok()?;
        self.reserved_keyword_text(keyword)
    }

    fn reserved_keyword_text(&self, keyword: &str) -> Option<&'static str> {
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
        if rest.as_bytes().first().is_some_and(is_identifier_continue) {
            return None;
        }

        let trailing_comment = line
            .find("//")
            .map(|comment_start| line[comment_start..].trim());
        Some((line_end as u32, trailing_comment))
    }
}

const fn is_identifier_continue(byte: &u8) -> bool {
    byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'$'
}

fn strip_recovery_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(keyword)?;
    if rest.as_bytes().first().is_some_and(is_identifier_continue) {
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
