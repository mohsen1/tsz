//! Return type recovery helpers for returned local function initializers.

use super::super::DeclarationEmitter;
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn source_function_initializer_return_type_text(
        &self,
        outer_func: Option<&tsz_parser::parser::node::FunctionData>,
        inner_idx: NodeIndex,
        inner_func: &tsz_parser::parser::node::FunctionData,
        inner_type_param_renames: &[(String, String)],
    ) -> Option<String> {
        if inner_func.type_annotation.is_some() {
            let type_text = self
                .preferred_annotation_name_text(inner_func.type_annotation)
                .or_else(|| self.emit_type_node_text(inner_func.type_annotation))?;
            return Some(Self::rename_type_text_identifiers(
                &type_text,
                inner_type_param_renames,
            ));
        }
        if self.source_is_js_file
            && let Some(type_text) =
                self.jsdoc_returned_function_return_type_text(inner_idx, inner_func)
        {
            return Some(Self::rename_type_text_identifiers(
                &type_text,
                inner_type_param_renames,
            ));
        }
        if inner_func.body.is_none() {
            return None;
        }

        let return_text = if self.body_returns_void(inner_func.body) {
            "void".to_string()
        } else {
            let outer_func = outer_func?;
            let return_expr = self
                .function_body_unique_return_expression(inner_func.body)
                .filter(|idx| idx.is_some())
                .unwrap_or(inner_func.body);
            let return_expr = self
                .const_asserted_expression(return_expr)
                .unwrap_or(return_expr);
            let return_node = self.arena.get(return_expr)?;
            if let Some(type_text) = self.returned_call_expression_type_text_from_outer_parameter(
                outer_func,
                return_expr,
                inner_type_param_renames,
            ) {
                type_text
            } else if return_node.kind == SyntaxKind::Identifier as u16 {
                self.function_scope_identifier_type_text(
                    outer_func,
                    inner_func,
                    return_expr,
                    inner_type_param_renames,
                )?
            } else if return_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                self.source_function_return_object_literal_type_text(
                    outer_func,
                    inner_func,
                    inner_type_param_renames,
                    return_expr,
                )?
            } else {
                if return_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                    return None;
                }
                let array = self.arena.get_literal_expr(return_node)?;
                let elements = array
                    .elements
                    .nodes
                    .iter()
                    .copied()
                    .map(|elem_idx| {
                        self.function_scope_identifier_type_text(
                            outer_func,
                            inner_func,
                            elem_idx,
                            inner_type_param_renames,
                        )
                    })
                    .collect::<Option<Vec<_>>>()?;
                format!("readonly [{}]", elements.join(", "))
            }
        };

        if inner_func.is_async
            || self
                .arena
                .has_modifier(&inner_func.modifiers, SyntaxKind::AsyncKeyword)
        {
            Some(format!("Promise<{return_text}>"))
        } else {
            Some(return_text)
        }
    }

    fn source_function_return_object_literal_type_text(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        inner_func: &tsz_parser::parser::node::FunctionData,
        inner_type_param_renames: &[(String, String)],
        object_idx: NodeIndex,
    ) -> Option<String> {
        let object_node = self.arena.get(object_idx)?;
        let object = self.arena.get_literal_expr(object_node)?;
        let mut names_in_scope = outer_func
            .type_parameters
            .as_ref()
            .map(|type_params| self.collect_type_param_names(type_params))
            .unwrap_or_default();
        for (_, renamed) in inner_type_param_renames {
            if !names_in_scope.contains(renamed) {
                names_in_scope.push(renamed.clone());
            }
        }

        let mut members = Vec::new();
        for &member_idx in &object.elements.nodes {
            let member_node = self.arena.get(member_idx)?;
            let name_idx = self.object_literal_member_name_idx(member_node)?;
            let name_text = self.object_literal_member_name_text(name_idx)?;
            if name_text.is_empty() || name_text == ":" {
                return None;
            }
            let value_idx = if let Some(data) = self.arena.get_shorthand_property(member_node) {
                data.name
            } else {
                self.arena.get_property_assignment(member_node)?.initializer
            };

            let type_text = self
                .function_declaration_identifier_type_text(
                    value_idx,
                    Some(inner_func),
                    &names_in_scope,
                    inner_type_param_renames,
                )
                .or_else(|| {
                    self.preferred_object_member_initializer_type_text(value_idx, 1)
                        .map(|text| {
                            Self::rename_type_text_identifiers(&text, inner_type_param_renames)
                        })
                })?;
            members.push(Self::format_object_member_type_text(
                &name_text, &type_text, 1,
            ));
        }

        let member_indent = "        ";
        let closing_indent = "    ";
        let formatted_members = members
            .iter()
            .map(|member| Self::format_object_member_entry(member_indent, member))
            .collect::<Vec<_>>()
            .join("\n");
        Some(format!("{{\n{formatted_members}\n{closing_indent}}}"))
    }

    fn returned_call_expression_type_text_from_outer_parameter(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        return_expr: NodeIndex,
        inner_type_param_renames: &[(String, String)],
    ) -> Option<String> {
        let return_node = self.arena.get(return_expr)?;
        if return_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(return_node)?;
        let callee_idx = self.skip_parenthesized_expression(call.expression)?;
        let callee_node = self.arena.get(callee_idx)?;
        if callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(member_type) =
                self.property_access_declared_type_annotation_text(callee_idx)
                && let Some(parts) = Self::parse_function_type_text(&member_type)
            {
                if parts.return_type.trim() == "unknown" {
                    return self.outer_parameter_property_function_return_type_text(
                        outer_func,
                        callee_idx,
                        inner_type_param_renames,
                    );
                }
                return Some(Self::rename_type_text_identifiers(
                    &parts.return_type,
                    inner_type_param_renames,
                ));
            }
            return self.outer_parameter_property_function_return_type_text(
                outer_func,
                callee_idx,
                inner_type_param_renames,
            );
        }
        if callee_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let parameter_type = self.function_parameter_type_text(outer_func, callee_idx)?;
        let parts = Self::parse_function_type_text(&parameter_type)?;
        Some(Self::rename_type_text_identifiers(
            &parts.return_type,
            inner_type_param_renames,
        ))
    }

    fn outer_parameter_property_function_return_type_text(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        callee_idx: NodeIndex,
        inner_type_param_renames: &[(String, String)],
    ) -> Option<String> {
        let access_node = self.arena.get(callee_idx)?;
        let access = self.arena.get_access_expr(access_node)?;
        let expression_idx = self.skip_parenthesized_expression(access.expression)?;
        let property_name =
            self.property_name_text_from_arena(self.arena, access.name_or_argument)?;
        let parameter_type = self.function_parameter_type_text(outer_func, expression_idx)?;
        let (target_name, target_args) = Self::parse_type_reference_text(&parameter_type)?;
        let source_file = self
            .current_source_file_idx
            .and_then(|idx| self.arena.get(idx))
            .and_then(|node| self.arena.get_source_file(node))?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(interface) = self.arena.get_interface(stmt_node) else {
                continue;
            };
            if self.get_identifier_text(interface.name).as_deref() != Some(&target_name) {
                continue;
            }

            let type_param_names = interface
                .type_parameters
                .as_ref()
                .map(|type_params| self.collect_type_param_names(type_params))
                .unwrap_or_default();
            if type_param_names.len() != target_args.len() {
                continue;
            }
            let substitutions: Vec<(String, String)> = type_param_names
                .into_iter()
                .zip(target_args.iter().cloned())
                .collect();

            for &member_idx in &interface.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                let Some(signature) = self.arena.get_signature(member_node) else {
                    continue;
                };
                if self
                    .property_name_text_from_arena(self.arena, signature.name)
                    .as_deref()
                    != Some(&property_name)
                {
                    continue;
                }
                let member_type = self.emit_type_node_text(signature.type_annotation)?;
                let parts = Self::parse_function_type_text(&member_type)?;
                let substituted =
                    Self::rename_type_text_identifiers(&parts.return_type, &substitutions);
                return Some(Self::rename_type_text_identifiers(
                    &substituted,
                    inner_type_param_renames,
                ));
            }
        }

        None
    }

    fn parse_type_reference_text(type_text: &str) -> Option<(String, Vec<String>)> {
        let trimmed = type_text.trim();
        let Some(lt_idx) = trimmed.find('<') else {
            return Self::is_simple_type_reference_name(trimmed)
                .then(|| (trimmed.to_string(), Vec::new()));
        };
        let name = trimmed.get(..lt_idx)?.trim();
        if !Self::is_simple_type_reference_name(name) || !trimmed.ends_with('>') {
            return None;
        }
        let args_text = trimmed.get(lt_idx + 1..trimmed.len() - 1)?;
        let args = Self::split_top_level_commas(args_text)
            .into_iter()
            .map(str::trim)
            .filter(|arg| !arg.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        Some((name.to_string(), args))
    }

    fn is_simple_type_reference_name(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if first != '_' && first != '$' && !first.is_ascii_alphabetic() {
            return false;
        }
        chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    fn function_body_unique_return_expression(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        let body_node = self.arena.get(body_idx)?;
        if body_node.kind != syntax_kind_ext::BLOCK {
            return Some(body_idx);
        }
        let block = self.arena.get_block(body_node)?;
        let mut result = None;
        for stmt_idx in block.statements.nodes.iter().copied() {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.arena.get_return_statement(stmt_node)?;
            if !ret.expression.is_some() || result.replace(ret.expression).is_some() {
                return None;
            }
        }
        result
    }

    fn jsdoc_returned_function_return_type_text(
        &self,
        inner_idx: NodeIndex,
        inner_func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let jsdoc = self.returned_function_expression_jsdoc(inner_idx, inner_func)?;
        if let Some(type_text) = Self::parse_jsdoc_type_text(&jsdoc)
            && let Some(parts) = Self::parse_function_type_text(&type_text)
        {
            return Some(parts.return_type);
        }
        Self::parse_jsdoc_return_type_text(&jsdoc)
    }

    pub(in crate::declaration_emitter) fn returned_function_expression_jsdoc(
        &self,
        inner_idx: NodeIndex,
        inner_func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        if let Some(jsdoc) = self.function_like_jsdoc_for_node(inner_idx) {
            return Some(jsdoc);
        }
        if inner_func.body.is_some()
            && let Some(jsdoc) = self.function_like_jsdoc_for_node(inner_func.body)
        {
            return Some(jsdoc);
        }
        if let Some(return_idx) = self.return_statement_ancestor(inner_idx)
            && let Some(return_node) = self.arena.get(return_idx)
            && let Some(jsdoc) = self.leading_jsdoc_comment_for_pos(return_node.pos)
        {
            return Some(jsdoc);
        }
        inner_func
            .parameters
            .nodes
            .first()
            .and_then(|param_idx| self.function_like_jsdoc_for_node(*param_idx))
    }

    fn return_statement_ancestor(&self, from_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = from_idx;
        for _ in 0..8 {
            let parent_idx = self.arena.parent_of(current)?;
            let parent_node = self.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::RETURN_STATEMENT {
                return Some(parent_idx);
            }
            current = parent_idx;
        }
        None
    }

    fn leading_jsdoc_comment_for_pos(&self, pos: u32) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let bytes = text.as_bytes();
        let mut actual_start = pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }

        self.all_comments
            .iter()
            .filter(|comment| comment.end as usize <= actual_start)
            .rev()
            .find_map(|comment| {
                let between = text.get(comment.end as usize..actual_start)?;
                if !between
                    .bytes()
                    .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
                {
                    return None;
                }
                is_jsdoc_comment(comment, text).then(|| get_jsdoc_content(comment, text))
            })
    }

    fn const_asserted_expression(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::AS_EXPRESSION
            && expr_node.kind != syntax_kind_ext::TYPE_ASSERTION
        {
            return None;
        }
        let assertion = self.arena.get_type_assertion(expr_node)?;
        if self
            .arena
            .get(assertion.type_node)
            .is_some_and(|node| node.kind == SyntaxKind::ConstKeyword as u16)
        {
            return Some(assertion.expression);
        }
        let type_name = self
            .get_identifier_text(assertion.type_node)
            .or_else(|| self.emit_type_node_text(assertion.type_node))?;
        (type_name == "const").then_some(assertion.expression)
    }

    fn function_scope_identifier_type_text(
        &self,
        outer_func: &tsz_parser::parser::node::FunctionData,
        inner_func: &tsz_parser::parser::node::FunctionData,
        expr_idx: NodeIndex,
        inner_type_param_renames: &[(String, String)],
    ) -> Option<String> {
        let name = self.get_identifier_text(expr_idx)?;
        if let Some(type_text) = self.function_parameter_annotation_text(inner_func, &name) {
            return Some(Self::rename_type_text_identifiers(
                &type_text,
                inner_type_param_renames,
            ));
        }
        if let Some(type_text) = self.function_parameter_annotation_text(outer_func, &name) {
            return Some(type_text);
        }
        self.get_node_type_or_names(&[expr_idx])
            .map(|type_id| self.print_type_id(type_id))
    }

    fn function_parameter_annotation_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        name: &str,
    ) -> Option<String> {
        for param_idx in func.parameters.nodes.iter().copied() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            if self.identifier_text_or_source(param.name).as_deref() != Some(name) {
                continue;
            }
            return self
                .preferred_annotation_name_text(param.type_annotation)
                .or_else(|| self.emit_type_node_text(param.type_annotation));
        }
        None
    }

    pub(in crate::declaration_emitter) fn local_type_alias_annotation_text(
        &self,
        from_idx: NodeIndex,
        name: &str,
    ) -> Option<String> {
        if let Some(from_node) = self.arena.get(from_idx)
            && from_node.kind == syntax_kind_ext::BLOCK
            && let Some(block) = self.arena.get_block(from_node)
            && let Some(type_text) =
                self.local_type_alias_annotation_text_in_statements(&block.statements, name)
        {
            return Some(type_text);
        }

        let mut current_idx = from_idx;
        while let Some(parent_idx) = self.arena.parent_of(current_idx) {
            let Some(parent_node) = self.arena.get(parent_idx) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::BLOCK
                && let Some(block) = self.arena.get_block(parent_node)
                && let Some(type_text) =
                    self.local_type_alias_annotation_text_in_statements(&block.statements, name)
            {
                return Some(type_text);
            }
            current_idx = parent_idx;
        }
        None
    }

    fn local_type_alias_annotation_text_in_statements(
        &self,
        statements: &NodeList,
        name: &str,
    ) -> Option<String> {
        for stmt_idx in statements.nodes.iter().copied() {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                continue;
            }
            let Some(alias) = self.arena.get_type_alias(stmt_node) else {
                continue;
            };
            if self.get_identifier_text(alias.name).as_deref() == Some(name) {
                return self
                    .local_type_annotation_text(alias.type_node)
                    .or_else(|| {
                        self.preferred_annotation_name_text(alias.type_node)
                            .or_else(|| self.emit_type_node_text(alias.type_node))
                    });
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn renamed_type_param_name(
        name: &str,
        renames: &[(String, String)],
    ) -> String {
        renames
            .iter()
            .find_map(|(from, to)| (from == name).then(|| to.clone()))
            .unwrap_or_else(|| name.to_string())
    }

    pub(in crate::declaration_emitter) fn identifier_text_or_source(
        &self,
        idx: NodeIndex,
    ) -> Option<String> {
        self.get_identifier_text(idx).or_else(|| {
            let node = self.arena.get(idx)?;
            (node.kind == SyntaxKind::Identifier as u16)
                .then(|| self.get_source_slice_no_semi(node.pos, node.end))?
        })
    }

    pub(in crate::declaration_emitter) fn rename_type_text_identifiers(
        text: &str,
        renames: &[(String, String)],
    ) -> String {
        if renames.is_empty() {
            return text.to_string();
        }

        let mut result = String::with_capacity(text.len());
        let mut ident_start = None;
        for (idx, ch) in text.char_indices() {
            if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                ident_start.get_or_insert(idx);
                continue;
            }
            if let Some(start) = ident_start.take() {
                let ident = &text[start..idx];
                result.push_str(&Self::renamed_type_param_name(ident, renames));
            }
            result.push(ch);
        }
        if let Some(start) = ident_start {
            let ident = &text[start..];
            result.push_str(&Self::renamed_type_param_name(ident, renames));
        }
        result
    }
}
