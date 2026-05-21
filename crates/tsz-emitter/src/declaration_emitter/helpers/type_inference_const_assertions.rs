//! Const assertion and asserted-type declaration inference helpers.
//!
//! These routines recover useful declaration type text from `as const`,
//! angle-bracket const assertions, asserted aliases, and const-asserted
//! array/object/template literals.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn rewrite_const_assertion_object_index_value_union(
        &self,
        initializer: NodeIndex,
        type_text: &str,
    ) -> Option<String> {
        let object_expr_idx = self
            .const_assertion_expression(initializer)
            .unwrap_or(initializer);
        self.arena
            .get(object_expr_idx)
            .filter(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)?;
        let source_union =
            self.source_ordered_object_literal_index_value_union_text(object_expr_idx)?;
        let mut lines = type_text.lines().map(str::to_string).collect::<Vec<_>>();
        Self::rewrite_broad_index_signature_value_union(&mut lines, &source_union);
        let rewritten = lines.join("\n");
        (rewritten != type_text).then_some(rewritten)
    }

    pub(in crate::declaration_emitter) fn strip_synthetic_anonymous_object_members(
        type_text: &str,
    ) -> String {
        if let Some(unwrapped) = Self::unwrap_synthetic_anonymous_object_type(type_text) {
            return unwrapped;
        }
        type_text.to_string()
    }

    fn unwrap_synthetic_anonymous_object_type(type_text: &str) -> Option<String> {
        let trimmed = type_text.trim();
        let inner = trimmed.strip_prefix('{')?.trim_start();
        let member = inner.strip_prefix(':')?.trim();
        let member = if member.ends_with('}') {
            let without_outer = member.strip_suffix('}').unwrap_or(member).trim_end();
            if without_outer.ends_with(';') {
                without_outer
            } else {
                member
            }
        } else {
            member
        };
        let member = member.strip_suffix(';').unwrap_or(member).trim();
        if member.is_empty() {
            return None;
        }
        if member.starts_with('{') {
            if let Some(unwrapped) = Self::unwrap_synthetic_anonymous_object_type(member) {
                return Some(unwrapped);
            }
        }
        Some(member.to_string())
    }

    pub(in crate::declaration_emitter) fn explicit_asserted_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let mut current = expr_idx;

        for _ in 0..100 {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node)
            {
                current = unary.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::CommaToken as u16
            {
                current = binary.right;
                continue;
            }

            if node.kind == syntax_kind_ext::SATISFIES_EXPRESSION {
                return None;
            }

            let assertion = self.arena.get_type_assertion(node)?;
            let asserted_type = self.arena.get(assertion.type_node)?;
            if asserted_type.kind == SyntaxKind::ConstKeyword as u16 {
                return None;
            }
            if let Some(alias_text) =
                self.local_asserted_type_alias_text(current, assertion.type_node)
            {
                return Some(alias_text);
            }
            return self.emit_type_node_text_normalized(assertion.type_node);
        }

        None
    }

    pub(in crate::declaration_emitter) fn declaration_type_is_uninformative(
        &self,
        candidates: &[NodeIndex],
    ) -> bool {
        self.get_node_type_or_names(candidates)
            .is_none_or(|type_id| {
                type_id == tsz_solver::types::TypeId::ANY
                    || type_id == tsz_solver::types::TypeId::ERROR
                    || type_id == tsz_solver::types::TypeId::UNKNOWN
            })
    }

    pub(in crate::declaration_emitter) fn as_const_assertion_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::AS_EXPRESSION {
            return None;
        }
        let assertion = self.arena.get_type_assertion(expr_node)?;
        if !self.type_assertion_is_const(assertion.type_node) {
            return None;
        }

        self.const_asserted_expression_type_text(assertion.expression, self.indent_level)
    }

    pub(in crate::declaration_emitter) fn angle_bracket_const_assertion_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::TYPE_ASSERTION {
            return None;
        }

        let assertion = self.arena.get_type_assertion(expr_node)?;
        if !self.type_assertion_is_const(assertion.type_node) {
            return None;
        }

        self.const_asserted_expression_type_text(assertion.expression, self.indent_level)
    }

    pub(in crate::declaration_emitter) fn as_const_single_spread_array_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::AS_EXPRESSION {
            return None;
        }
        let assertion = self.arena.get_type_assertion(expr_node)?;
        if !self.type_assertion_is_const(assertion.type_node) {
            return None;
        }

        let array_node = self.arena.get(assertion.expression)?;
        if array_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let array = self.arena.get_literal_expr(array_node)?;
        let [element_idx] = array.elements.nodes.as_slice() else {
            return None;
        };
        let element_node = self.arena.get(*element_idx)?;
        if element_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
            return None;
        }
        let spread = self.arena.get_spread(element_node)?;
        let spread_expr_node = self.arena.get(spread.expression)?;
        if spread_expr_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }

        let spread_type = self
            .get_node_type_or_names(&[spread.expression])
            .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
            .or_else(|| {
                self.infer_fallback_type_text_at(spread.expression, self.indent_level + 1)
            })?;
        let inner = spread_type
            .strip_prefix("readonly ")
            .unwrap_or(&spread_type)
            .strip_suffix("[]")?;
        (!inner.contains('|') && !inner.contains('&') && !inner.starts_with('['))
            .then(|| format!("readonly {inner}[]"))
    }

    pub(in crate::declaration_emitter) fn type_assertion_is_const(
        &self,
        type_node_idx: NodeIndex,
    ) -> bool {
        self.arena
            .get(type_node_idx)
            .is_some_and(|asserted_type| asserted_type.kind == SyntaxKind::ConstKeyword as u16)
            || self
                .get_identifier_text(type_node_idx)
                .or_else(|| self.emit_type_node_text(type_node_idx))
                .as_deref()
                == Some("const")
    }

    pub(in crate::declaration_emitter) fn const_assertion_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::AS_EXPRESSION
            && expr_node.kind != syntax_kind_ext::TYPE_ASSERTION
        {
            return None;
        }
        let assertion = self.arena.get_type_assertion(expr_node)?;
        self.type_assertion_is_const(assertion.type_node)
            .then_some(assertion.expression)
    }

    pub(in crate::declaration_emitter) fn const_asserted_expression_type_text(
        &self,
        expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.arena.get_parenthesized(expr_node).and_then(|paren| {
                    self.const_asserted_expression_type_text(paren.expression, depth)
                })
            }
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || (k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    && self.is_negative_literal(expr_node)) =>
            {
                self.const_literal_initializer_text(expr_idx)
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                self.const_asserted_template_expression_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.const_asserted_array_literal_type_text(expr_idx, depth)
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.const_asserted_object_literal_type_text(expr_idx, depth)
            }
            _ => self
                .get_node_type_or_names(&[expr_idx])
                .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
                .or_else(|| self.infer_fallback_type_text_at(expr_idx, depth)),
        }
    }

    fn const_asserted_template_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let template = self.arena.get_template_expr(expr_node)?;
        let mut parts = Vec::with_capacity(template.template_spans.nodes.len());
        let mut text = self.template_literal_part_text(template.head, true)?;
        for &span_idx in &template.template_spans.nodes {
            let span_node = self.arena.get(span_idx)?;
            let span = self.arena.get_template_span(span_node)?;
            let expr_type = self
                .template_expression_placeholder_type_text(span.expression)
                .or_else(|| {
                    self.get_node_type_or_names(&[span.expression])
                        .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
                })
                .or_else(|| self.infer_fallback_type_text_at(span.expression, 0))?;
            parts.push(expr_type);
            text.push_str("${");
            text.push_str(parts.last()?);
            text.push('}');
            text.push_str(&self.template_literal_part_text(span.literal, false)?);
        }
        Some(format!("`{text}`"))
    }

    fn template_expression_placeholder_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let ident = self.get_identifier_text(expr_idx)?;
        let mut current = expr_idx;
        for _ in 0..32 {
            let parent_idx = self.arena.parent_of(current)?;
            let parent_node = self.arena.get(parent_idx)?;
            if let Some(func) = self.arena.get_function(parent_node) {
                for &param_idx in &func.parameters.nodes {
                    let param_node = self.arena.get(param_idx)?;
                    let param = self.arena.get_parameter(param_node)?;
                    if self.get_identifier_text(param.name).as_deref() != Some(ident.as_str()) {
                        continue;
                    }
                    let type_text = self
                        .source_slice_from_arena(self.arena, param.type_annotation)
                        .or_else(|| {
                            self.type_annotation_text_from_arena_node(
                                self.arena,
                                param.type_annotation,
                            )
                        })?;
                    return Some(type_text.trim().to_string());
                }
                return None;
            }
            current = parent_idx;
        }
        None
    }

    fn template_literal_part_text(&self, idx: NodeIndex, is_head: bool) -> Option<String> {
        let node = self.arena.get(idx)?;
        let raw = self.get_source_slice_no_semi(node.pos, node.end)?;
        let raw = raw.as_str();
        if is_head {
            raw.strip_prefix('`')
                .and_then(|text| text.strip_suffix("${"))
                .or_else(|| {
                    raw.strip_prefix('`')
                        .and_then(|text| text.strip_suffix('`'))
                })
                .map(str::to_string)
        } else {
            raw.strip_prefix('}')
                .and_then(|text| text.strip_suffix("${"))
                .or_else(|| {
                    raw.strip_prefix('}')
                        .and_then(|text| text.strip_suffix('`'))
                })
                .map(str::to_string)
        }
    }

    fn const_asserted_array_literal_type_text(
        &self,
        expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let array = self.arena.get_literal_expr(expr_node)?;
        let mut parts = Vec::with_capacity(array.elements.nodes.len());

        for &element_idx in &array.elements.nodes {
            let element_node = self.arena.get(element_idx)?;
            if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                let spread = self.arena.get_spread(element_node)?;
                let spread_type = self
                    .get_node_type_or_names(&[spread.expression])
                    .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
                    .or_else(|| self.infer_fallback_type_text_at(spread.expression, depth + 1))
                    .unwrap_or_else(|| "any[]".to_string());
                parts.push(format!("...{spread_type}"));
                continue;
            }

            let element_depth = if element_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                depth
            } else {
                depth + 1
            };
            parts.push(
                self.const_asserted_expression_type_text(element_idx, element_depth)
                    .unwrap_or_else(|| "any".to_string()),
            );
        }

        Some(format!("readonly [{}]", parts.join(", ")))
    }

    fn const_asserted_object_literal_type_text(
        &self,
        expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let object = self.arena.get_literal_expr(expr_node)?;
        let mut members = Vec::new();

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT {
                return None;
            }

            let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
                continue;
            };
            let name = self.object_literal_member_name_text(name_idx)?;
            if name.is_empty() || name == ":" {
                continue;
            }

            if let Some(method) = self.arena.get_method_decl(member_node) {
                let type_text = self
                    .function_expression_type_text_from_ast(member_idx)
                    .or_else(|| {
                        self.get_node_type_or_names(&[member_idx])
                            .map(|type_id| self.print_type_id(type_id))
                    })
                    .unwrap_or_else(|| {
                        if method.parameters.nodes.is_empty() {
                            "() => void".to_string()
                        } else {
                            "any".to_string()
                        }
                    });
                if !self.remove_comments {
                    for jsdoc in self.leading_jsdoc_comment_chain_for_pos(member_node.pos) {
                        members.push(Self::format_object_member_jsdoc_text(&jsdoc));
                    }
                }
                members.push(format!("readonly {name}: {type_text};"));
                continue;
            }

            let Some(initializer) = self.object_literal_member_initializer(member_node) else {
                continue;
            };
            let type_text = self
                .const_asserted_expression_type_text(initializer, depth + 1)
                .unwrap_or_else(|| "any".to_string());
            if !self.remove_comments {
                for jsdoc in self.leading_jsdoc_comment_chain_for_pos(member_node.pos) {
                    members.push(Self::format_object_member_jsdoc_text(&jsdoc));
                }
            }
            members.push(format!("readonly {name}: {type_text};"));
        }

        if members.is_empty() {
            return Some("{}".to_string());
        }

        let member_indent = "    ".repeat((depth + 1) as usize);
        let closing_indent = "    ".repeat(depth as usize);
        let lines = members
            .into_iter()
            .map(|member| format!("{member_indent}{member}"))
            .collect::<Vec<_>>();
        Some(format!("{{\n{}\n{closing_indent}}}", lines.join("\n")))
    }

    pub(in crate::declaration_emitter) fn local_asserted_type_alias_text(
        &self,
        assertion_expr_idx: NodeIndex,
        type_node_idx: NodeIndex,
    ) -> Option<String> {
        let name = self.simple_type_reference_name_text(type_node_idx)?;
        let alias_decl_idx =
            self.find_enclosing_block_type_alias_declaration(assertion_expr_idx, &name)?;
        let alias_node = self.arena.get(alias_decl_idx)?;
        let alias = self.arena.get_type_alias(alias_node)?;
        let mut alias_text = self.emit_type_node_text_normalized(alias.type_node)?;
        let substitutions = self
            .type_alias_application_substitutions(alias.type_parameters.as_ref(), type_node_idx);
        if !substitutions.is_empty() {
            alias_text = Self::replace_whole_words_in_text(&alias_text, &substitutions);
        }
        if alias_text.contains("typeof ") {
            return Some(alias_text);
        }

        if Self::type_text_contains_mapped_type_literal(&alias_text) {
            return Some(
                self.expand_enclosing_block_type_aliases_in_text(
                    assertion_expr_idx,
                    &alias_text,
                    &name,
                )
                .unwrap_or(alias_text),
            );
        }

        alias_text
            .trim_start()
            .starts_with('{')
            .then(|| Self::normalize_local_type_literal_accessor_text(&alias_text))
    }

    pub(in crate::declaration_emitter) fn type_text_contains_mapped_type_literal(
        text: &str,
    ) -> bool {
        text.contains("[") && text.contains(" in ") && text.contains("]:")
    }

    pub(in crate::declaration_emitter) fn expand_enclosing_block_type_aliases_in_text(
        &self,
        from_idx: NodeIndex,
        type_text: &str,
        excluded_name: &str,
    ) -> Option<String> {
        let aliases = self.enclosing_block_type_alias_replacements(from_idx, excluded_name)?;
        Some(Self::replace_whole_words_in_text(type_text, &aliases))
    }

    pub(in crate::declaration_emitter) fn enclosing_block_type_alias_replacements(
        &self,
        from_idx: NodeIndex,
        excluded_name: &str,
    ) -> Option<Vec<(String, String)>> {
        let mut current_idx = from_idx;
        while let Some(ext) = self.arena.get_extended(current_idx) {
            let parent_idx = ext.parent;
            if !parent_idx.is_some() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::BLOCK {
                let block = self.arena.get_block(parent_node)?;
                let replacements = block
                    .statements
                    .nodes
                    .iter()
                    .copied()
                    .filter_map(|stmt_idx| {
                        let stmt_node = self.arena.get(stmt_idx)?;
                        if stmt_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                            return None;
                        }
                        let alias = self.arena.get_type_alias(stmt_node)?;
                        let name = self.get_identifier_text(alias.name)?;
                        if name == excluded_name {
                            return None;
                        }
                        let alias_text = self.emit_type_node_text_normalized(alias.type_node)?;
                        Some((name, alias_text))
                    })
                    .collect();
                return Some(replacements);
            }
            current_idx = parent_idx;
        }
        None
    }
}
