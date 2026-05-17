//! Object-literal member helper routines extracted from `type_inference.rs`.
//!
//! These helpers keep object member name recovery, member comment insertion,
//! and object-member type formatting together instead of leaving the logic
//! embedded in the main expression type inference module.

use super::super::DeclarationEmitter;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn format_object_member_type_text(
        name: &str,
        type_text: &str,
        depth: u32,
    ) -> String {
        if !type_text.contains('\n') {
            return format!("{name}: {type_text}");
        }

        let _ = depth;
        format!("{name}: {type_text}")
    }

    pub(in crate::declaration_emitter) fn format_object_member_jsdoc_text(jsdoc: &str) -> String {
        let trimmed = jsdoc.trim();
        if !trimmed.contains('\n') {
            return format!("/** {trimmed} */");
        }

        let mut result = String::from("/**");
        for line in trimmed.lines() {
            result.push('\n');
            if line.trim().is_empty() {
                result.push_str(" *");
            } else {
                result.push_str(" * ");
                result.push_str(line.trim());
            }
        }
        result.push_str("\n */");
        result
    }

    pub(in crate::declaration_emitter) fn add_returned_object_member_comments_to_type_text(
        &self,
        initializer: NodeIndex,
        type_text: &str,
    ) -> String {
        if self.remove_comments || !type_text.contains("{\n") {
            return type_text.to_string();
        }
        let Some(object_idx) =
            self.function_initializer_unique_returned_object_literal(initializer)
        else {
            return type_text.to_string();
        };
        self.add_object_literal_member_comments_to_type_text(object_idx, type_text)
    }

    pub(in crate::declaration_emitter) fn add_initializer_object_member_comments_to_type_text(
        &self,
        initializer: NodeIndex,
        type_text: &str,
    ) -> String {
        if self.remove_comments || !type_text.contains("{\n") {
            return type_text.to_string();
        }
        let Some(object_idx) = self.initializer_object_literal_expression(initializer) else {
            return type_text.to_string();
        };
        self.add_object_literal_member_comments_to_type_text(object_idx, type_text)
    }

    fn add_object_literal_member_comments_to_type_text(
        &self,
        object_idx: NodeIndex,
        type_text: &str,
    ) -> String {
        let Some(object_node) = self.arena.get(object_idx) else {
            return type_text.to_string();
        };
        let Some(object) = self.arena.get_literal_expr(object_node) else {
            return type_text.to_string();
        };

        let mut commented_members = Vec::new();
        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
                continue;
            };
            let Some(name_text) = self.object_literal_member_name_text(name_idx) else {
                continue;
            };
            let comments = self.leading_jsdoc_comment_chain_for_pos(member_node.pos);
            if !comments.is_empty() {
                commented_members.push((name_text, comments));
            }
        }
        if commented_members.is_empty() {
            return type_text.to_string();
        }

        let mut lines: Vec<String> = type_text.lines().map(str::to_string).collect();
        for (name_text, comments) in commented_members.into_iter().rev() {
            let Some(line_idx) = lines.iter().position(|line| {
                let trimmed = line.trim_start();
                Self::object_literal_property_name_prefixes(&name_text)
                    .into_iter()
                    .any(|prefix| {
                        trimmed.starts_with(&prefix)
                            || trimmed.starts_with(&format!("readonly {prefix}"))
                    })
                    || Self::object_literal_method_line_matches_name(trimmed, &name_text)
            }) else {
                continue;
            };
            if line_idx > 0 && lines[line_idx - 1].trim_start().starts_with("/**") {
                continue;
            }
            let indent_len = lines[line_idx].len() - lines[line_idx].trim_start().len();
            let indent = " ".repeat(indent_len);
            let mut comment_lines = Vec::new();
            for jsdoc in comments {
                for line in Self::format_object_member_jsdoc_text(&jsdoc).lines() {
                    comment_lines.push(format!("{indent}{line}"));
                }
            }
            lines.splice(line_idx..line_idx, comment_lines);
        }

        if type_text.ends_with('\n') {
            format!("{}\n", lines.join("\n"))
        } else {
            lines.join("\n")
        }
    }

    fn initializer_object_literal_expression(&self, initializer: NodeIndex) -> Option<NodeIndex> {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);
        let init_node = self.arena.get(initializer)?;
        match init_node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => Some(initializer),
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                let assertion = self.arena.get_type_assertion(init_node)?;
                self.initializer_object_literal_expression(assertion.expression)
            }
            _ => None,
        }
    }

    fn function_initializer_unique_returned_object_literal(
        &self,
        initializer: NodeIndex,
    ) -> Option<NodeIndex> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.arena.get_function(init_node)?;
        let body_node = self.arena.get(func.body)?;
        if body_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return Some(func.body);
        }
        let block = self.arena.get_block(body_node)?;
        let mut returned = None;
        for &stmt_idx in &block.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.arena.get_return_statement(stmt_node)?;
            if !ret.expression.is_some() {
                continue;
            }
            let expr = self.skip_parenthesized_non_null_and_comma(ret.expression);
            let expr_node = self.arena.get(expr)?;
            if expr_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return None;
            }
            if returned.replace(expr).is_some() {
                return None;
            }
        }
        returned
    }

    pub(in crate::declaration_emitter) fn format_object_member_entry(
        member_indent: &str,
        member_text: &str,
    ) -> String {
        let mut lines = member_text.lines();
        let first = lines.next().unwrap_or(member_text);
        let mut result = String::new();
        result.push_str(member_indent);
        result.push_str(first);
        for line in lines {
            result.push('\n');
            result.push_str(line);
        }
        if !result.trim_start().starts_with("/**") && !result.trim_end().ends_with(';') {
            result.push(';');
        }
        result
    }

    pub(in crate::declaration_emitter) fn object_literal_member_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        self.resolved_computed_property_name_text(name_idx)
            .or_else(|| self.infer_property_name_text(name_idx))
    }

    pub(in crate::declaration_emitter) fn resolved_computed_property_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }

        let computed = self.arena.get_computed_property(name_node)?;
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(computed.expression);
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                if let Some(interner) = self.type_interner
                    && let Some(type_id) = self.get_node_type_or_names(&[expr_idx])
                    && let Some(literal) = tsz_solver::visitor::literal_value(interner, type_id)
                {
                    return Some(Self::format_property_name_literal_value(&literal, interner));
                }
                // Fallback: an enum member access (e.g. `[E.A]`) is a valid
                // property-name source even when the type cache hasn't
                // produced a `Literal` form for it. Detecting it via the
                // binder lets the caller keep method/getter syntax instead
                // of degrading to `[E.A]: () => T`.
                self.enum_member_access_name_text(expr_idx)
            }
            _ => None,
        }
    }

    /// Returns an enum member's escaped name for method-like dts syntax such as `[E.A]() {}`.
    pub(in crate::declaration_emitter) fn enum_member_access_name_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let binder = self.binder?;
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let symbol = binder.symbols.get(sym_id)?;
        if symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0 {
            return None;
        }
        Some(symbol.escaped_name.clone())
    }

    pub(in crate::declaration_emitter) fn format_property_name_literal_value(
        literal: &tsz_solver::types::LiteralValue,
        interner: &tsz_solver::TypeInterner,
    ) -> String {
        match literal {
            tsz_solver::types::LiteralValue::String(atom) => {
                Self::format_property_name_literal_text(&interner.resolve_atom(*atom))
            }
            tsz_solver::types::LiteralValue::Number(n) => Self::format_js_number(n.0),
            tsz_solver::types::LiteralValue::Boolean(b) => b.to_string(),
            tsz_solver::types::LiteralValue::BigInt(atom) => {
                format!("{}n", interner.resolve_atom(*atom))
            }
        }
    }

    pub(in crate::declaration_emitter) fn format_property_name_literal_text(text: &str) -> String {
        if Self::is_unquoted_property_name(text) {
            text.to_string()
        } else {
            format!("\"{}\"", super::escape_string_for_double_quote(text))
        }
    }

    pub(in crate::declaration_emitter) fn is_unquoted_property_name(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };

        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return false;
        }

        chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    pub(in crate::declaration_emitter) fn preferred_object_member_initializer_type_text(
        &self,
        initializer: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let type_id = self.get_node_type_or_names(&[initializer]);
        if let Some(typeof_text) = self.typeof_prefix_for_value_entity(initializer, true, type_id) {
            return Some(typeof_text);
        }
        if let Some(enum_type_text) = self.enum_member_widened_type_text(initializer) {
            return Some(enum_type_text);
        }
        if self
            .arena
            .get(initializer)
            .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
            && let Some(type_text) =
                self.reference_declared_source_type_annotation_text(initializer)
        {
            return Some(type_text);
        }
        self.preferred_expression_type_text(initializer)
            .or_else(|| self.infer_fallback_type_text_at(initializer, depth))
    }

    pub(in crate::declaration_emitter) fn object_literal_declared_shorthand_type_text(
        &self,
        initializer: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(init_node)?;
        let mut has_declared_shorthand = false;

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT {
                return None;
            }
            let Some(shorthand) = self.arena.get_shorthand_property(member_node) else {
                return None;
            };
            if shorthand.object_assignment_initializer != NodeIndex::NONE {
                return None;
            }
            if self
                .reference_declared_source_type_annotation_text(shorthand.name)
                .is_some()
            {
                has_declared_shorthand = true;
            } else {
                return None;
            }
        }

        has_declared_shorthand
            .then(|| self.infer_object_literal_type_text_at(initializer, depth))
            .flatten()
    }

    pub(in crate::declaration_emitter) fn enum_member_widened_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let binder = self.binder?;

        let member_sym_id = self.value_reference_symbol(expr_idx)?;
        let member_symbol = binder.symbols.get(member_sym_id)?;
        if !member_symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            return None;
        }

        let enum_expr = self.skip_parenthesized_non_null_and_comma(access.expression);
        let enum_sym_id = self.value_reference_symbol(enum_expr)?;
        let enum_symbol = binder.symbols.get(enum_sym_id)?;
        if !enum_symbol.has_any_flags(symbol_flags::ENUM)
            || enum_symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
        {
            return None;
        }

        self.nameable_constructor_expression_text(enum_expr)
    }

    pub(in crate::declaration_emitter) fn infer_property_name_text(
        &self,
        node_id: NodeIndex,
    ) -> Option<String> {
        let node = self.arena.get(node_id)?;
        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(node)?;
            let expr_idx = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(computed.expression);
            let expr_node = self.arena.get(expr_idx)?;
            match expr_node.kind {
                k if k == SyntaxKind::StringLiteral as u16 => {
                    let literal = self.arena.get_literal(expr_node)?;
                    let quote = self.original_quote_char(expr_node);
                    return Some(format!("{}{}{}", quote, literal.text, quote));
                }
                k if k == SyntaxKind::NumericLiteral as u16 => {
                    let literal = self.arena.get_literal(expr_node)?;
                    return Some(Self::normalize_numeric_literal(literal.text.as_ref()));
                }
                k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                    let unary = self.arena.get_unary_expr(expr_node)?;
                    let operand_idx = self
                        .arena
                        .skip_parenthesized_and_assertions_and_comma(unary.operand);
                    let operand_node = self.arena.get(operand_idx)?;
                    if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                        return None;
                    }
                    let literal = self.arena.get_literal(operand_node)?;
                    let normalized = Self::normalize_numeric_literal(literal.text.as_ref());
                    return match unary.operator {
                        k if k == SyntaxKind::MinusToken as u16 => Some(format!("[-{normalized}]")),
                        k if k == SyntaxKind::PlusToken as u16 => Some(normalized),
                        _ => None,
                    };
                }
                k if k == SyntaxKind::Identifier as u16
                    || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
                {
                    // Use the COMPUTED_PROPERTY_NAME node's source slice.
                    // Trim at `]` because node.end may include trailing property punctuation.
                    if let Some(mut s) = self.get_source_slice(node.pos, node.end) {
                        // Find the last `]` and truncate after it
                        if let Some(bracket_pos) = s.rfind(']') {
                            s.truncate(bracket_pos + 1);
                        } else {
                            // No brackets — trim trailing punctuation
                            while s.ends_with(':') || s.ends_with('(') {
                                s.pop();
                                s = s.trim_end().to_string();
                            }
                        }
                        if !s.is_empty() {
                            return Some(s);
                        }
                    }
                    return None;
                }
                _ => return None,
            }
        }
        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(ident.escaped_text.clone());
        }
        if let Some(literal) = self.arena.get_literal(node) {
            let quote = self.original_quote_char(node);
            return Some(format!("{}{}{}", quote, literal.text, quote));
        }
        self.get_source_slice(node.pos, node.end)
    }
}
