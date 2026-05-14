use crate::emitter::Printer;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;

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
