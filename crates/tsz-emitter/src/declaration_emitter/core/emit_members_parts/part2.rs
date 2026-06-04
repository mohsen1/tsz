impl<'a> DeclarationEmitter<'a> {
    fn type_text_top_level_union_members(type_text: &str) -> Vec<&str> {
        let mut members = Vec::new();
        let mut start = 0usize;
        let mut paren_depth = 0u32;
        let mut bracket_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut angle_depth = 0u32;

        for (idx, ch) in type_text.char_indices() {
            match ch {
                '(' => paren_depth = paren_depth.saturating_add(1),
                ')' => paren_depth = paren_depth.saturating_sub(1),
                '[' => bracket_depth = bracket_depth.saturating_add(1),
                ']' => bracket_depth = bracket_depth.saturating_sub(1),
                '{' => brace_depth = brace_depth.saturating_add(1),
                '}' => brace_depth = brace_depth.saturating_sub(1),
                '<' => angle_depth = angle_depth.saturating_add(1),
                '>' => angle_depth = angle_depth.saturating_sub(1),
                '|' if paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0
                    && angle_depth == 0 =>
                {
                    members.push(type_text[start..idx].trim());
                    start = idx + ch.len_utf8();
                }
                _ => {}
            }
        }

        members.push(type_text[start..].trim());
        members
    }

    fn trim_wrapping_parens(type_text: &str) -> &str {
        let mut text = type_text.trim();
        loop {
            let Some(stripped) = text
                .strip_prefix('(')
                .and_then(|inner| inner.strip_suffix(')'))
            else {
                return text;
            };
            text = stripped.trim();
        }
    }

    pub(in crate::declaration_emitter) fn js_accessor_backing_field_type_text(
        &self,
        accessor_idx: NodeIndex,
    ) -> Option<String> {
        if !self.source_is_js_file {
            return None;
        }

        let key_text = self.accessor_this_element_key_text(accessor_idx)?;
        let parent_idx = self.arena.get_extended(accessor_idx)?.parent;
        let parent_node = self.arena.get(parent_idx)?;
        let class = self.arena.get_class(parent_node)?;

        class.members.nodes.iter().copied().find_map(|member_idx| {
            let member_node = self.arena.get(member_idx)?;
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                return None;
            }
            if self.class_computed_property_key_text(member_idx).as_deref()
                != Some(key_text.as_str())
            {
                return None;
            }
            self.jsdoc_type_text_for_node(member_idx)
        })
    }

    pub(in crate::declaration_emitter) fn accessor_this_element_key_text(
        &self,
        accessor_idx: NodeIndex,
    ) -> Option<String> {
        let accessor_node = self.arena.get(accessor_idx)?;
        let accessor = self.arena.get_accessor(accessor_node)?;
        let body_idx = accessor.body.into_option()?;
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let first_param_name = accessor.parameters.nodes.first().and_then(|&param_idx| {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            self.get_identifier_text(param.name)
        });

        for &stmt_idx in &block.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                k if k == syntax_kind_ext::RETURN_STATEMENT => {
                    let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                        continue;
                    };
                    if let Some(key_text) = self.this_element_access_key_text(ret.expression) {
                        return Some(key_text);
                    }
                }
                k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                    let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                        continue;
                    };
                    let expr_idx = self
                        .arena
                        .skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
                    let Some(expr_node) = self.arena.get(expr_idx) else {
                        continue;
                    };
                    if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                        continue;
                    }
                    let Some(binary) = self.arena.get_binary_expr(expr_node) else {
                        continue;
                    };
                    if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                        continue;
                    }
                    if let Some(param_name) = first_param_name.as_deref() {
                        let rhs_idx = self
                            .arena
                            .skip_parenthesized_and_assertions_and_comma(binary.right);
                        if self.get_identifier_text(rhs_idx).as_deref() != Some(param_name) {
                            continue;
                        }
                    }
                    if let Some(key_text) = self.this_element_access_key_text(binary.left) {
                        return Some(key_text);
                    }
                }
                _ => {}
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn class_computed_property_key_text(
        &self,
        member_idx: NodeIndex,
    ) -> Option<String> {
        let name_idx = self.get_member_name_idx(member_idx)?;
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.arena.get_computed_property(name_node)?;
        let key_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(computed.expression);
        let key_node = self.arena.get(key_idx)?;
        self.get_source_slice(key_node.pos, key_node.end)
            .map(|text| text.trim().to_string())
    }

    fn this_element_access_key_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(expr_node)?;
        let receiver_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.expression);
        let receiver_node = self.arena.get(receiver_idx)?;
        if receiver_node.kind != SyntaxKind::ThisKeyword as u16 {
            return None;
        }
        let key_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.name_or_argument);
        let key_node = self.arena.get(key_idx)?;
        self.get_source_slice(key_node.pos, key_node.end)
            .map(|text| text.trim().to_string())
    }

    /// Recover the type-predicate annotation text of a paired getter for a
    /// setter that has no annotation of its own. tsc symmetrises the pair
    /// by writing the same predicate (`x is File`) on both accessors in
    /// the emitted .d.ts, even though the runtime type of the setter
    /// parameter is `boolean`.
    fn paired_getter_type_predicate_text(
        &self,
        accessor_idx: NodeIndex,
        setter_params: &tsz_parser::parser::NodeList,
    ) -> Option<String> {
        let first_param_idx = *setter_params.nodes.first()?;
        let param_node = self.arena.get(first_param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        if param.type_annotation.is_some() {
            return None;
        }

        let parent_idx = self.arena.get_extended(accessor_idx)?.parent;
        let parent_node = self.arena.get(parent_idx)?;
        let member_nodes = if let Some(class_decl) = self.arena.get_class(parent_node) {
            class_decl.members.nodes.clone()
        } else if let Some(interface) = self.arena.get_interface(parent_node) {
            interface.members.nodes.clone()
        } else {
            let literal = self.arena.get_literal_expr(parent_node)?;
            literal.elements.nodes.clone()
        };

        let setter_node = self.arena.get(accessor_idx)?;
        let setter_accessor = self.arena.get_accessor(setter_node)?;
        let setter_name_text = self
            .arena
            .get(setter_accessor.name)
            .and_then(|name_node| self.get_source_slice(name_node.pos, name_node.end))?;

        for member_idx in member_nodes {
            if member_idx == accessor_idx {
                continue;
            }
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::GET_ACCESSOR {
                continue;
            }
            let Some(getter) = self.arena.get_accessor(member_node) else {
                continue;
            };
            let getter_name_text = self
                .arena
                .get(getter.name)
                .and_then(|name_node| self.get_source_slice(name_node.pos, name_node.end));
            if getter_name_text.as_deref() != Some(setter_name_text.as_str()) {
                continue;
            }
            let annotation_node = self.arena.get(getter.type_annotation)?;
            if annotation_node.kind != syntax_kind_ext::TYPE_PREDICATE {
                return None;
            }
            // The annotation node's end span may include the `{` that
            // opens the body of the getter; trim trailing whitespace and
            // any leftover open brace so the emitted setter parameter
            // type matches the predicate alone.
            return self
                .get_source_slice(annotation_node.pos, annotation_node.end)
                .map(|s| {
                    s.trim_end_matches(|c: char| c.is_whitespace() || c == '{')
                        .to_string()
                });
        }

        None
    }

    fn emit_setter_parameters_with_type_text(
        &mut self,
        params: &tsz_parser::parser::NodeList,
        type_text: &str,
    ) {
        let mut first = true;
        for &param_idx in &params.nodes {
            if !first {
                self.write(", ");
            }
            first = false;

            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            if param.dot_dot_dot_token {
                self.write("...");
            }
            self.emit_node(param.name);
            if param.question_token {
                self.write("?");
            }
            self.write(": ");
            self.write(type_text);
        }
    }

    fn js_setter_param_declared_type_text(
        &self,
        params: &tsz_parser::parser::NodeList,
    ) -> Option<String> {
        if !self.source_is_js_file {
            return None;
        }

        let param_idx = *params.nodes.first()?;
        let decl = self.jsdoc_param_decl_for_parameter(param_idx, 0)?;
        let mut type_text = decl.type_text;
        if decl.optional && !Self::type_text_has_undefined_branch(&type_text) {
            type_text.push_str(" | undefined");
        }
        Some(type_text)
    }

    pub(in crate::declaration_emitter) fn emit_index_signature(&mut self, sig_idx: NodeIndex) {
        let Some(sig_node) = self.arena.get(sig_idx) else {
            return;
        };
        let Some(sig) = self.arena.get_index_signature(sig_node) else {
            return;
        };

        self.write_indent();

        // Modifiers
        self.emit_member_modifiers(&sig.modifiers);

        self.write("[");
        if let Some(text) = self.recovered_legacy_index_signature_parameters(sig_node, sig) {
            self.write(&text);
        } else {
            self.emit_parameters(&sig.parameters);
        }
        self.write("]");

        if sig.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(sig.type_annotation);
        } else if !self.source_is_declaration_file {
            self.write(": any");
        }

        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn recovered_legacy_index_signature_parameters(
        &self,
        sig_node: &Node,
        sig: &IndexSignatureData,
    ) -> Option<String> {
        let source = self.source_file_text.as_ref()?;
        let pos = sig
            .parameters
            .nodes
            .first()
            .and_then(|idx| self.arena.get(*idx))
            .map_or(sig_node.pos as usize, |node| node.pos as usize);
        let line_start = source[..pos].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = source[pos..]
            .find('\n')
            .map_or(source.len(), |idx| pos + idx);
        let line = source.get(line_start..line_end)?;
        let start = Self::index_signature_open_bracket_before_pos(line, pos - line_start)?;
        let end = line[start + 1..].find(']')? + start + 1;
        let inner = line[start + 1..end].trim();
        (inner.contains(',') && !inner.contains('\n')).then(|| inner.to_string())
    }

    pub(in crate::declaration_emitter) fn emit_index_signature_parameters(
        &mut self,
        params: &NodeList,
    ) {
        let mut first = true;
        for &param_idx in &params.nodes {
            if !first {
                self.write(", ");
            }
            first = false;

            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            let comment_pos = self
                .arena
                .get(param.name)
                .map_or(param_node.pos, |name_node| name_node.pos);
            self.emit_inline_parameter_comment(comment_pos);
            self.emit_member_modifiers(&param.modifiers);
            if param.dot_dot_dot_token {
                self.write("...");
            }
            if let Some(name) = self.recovered_index_signature_parameter_name(param_node) {
                self.write(&name);
            } else if let Some(name) = self.get_identifier_text(param.name) {
                self.write(&name);
            } else {
                self.emit_node(param.name);
            }
            if param.question_token {
                self.write("?");
            }
            if param.type_annotation.is_some() {
                self.write(": ");
                self.emit_type(param.type_annotation);
            }
        }
    }

    fn recovered_index_signature_parameter_name(&self, param_node: &Node) -> Option<String> {
        let name = self
            .arena
            .get_parameter(param_node)
            .and_then(|param| self.get_identifier_text(param.name))?;
        if !name.contains(',') && !name.contains('[') {
            return None;
        }
        let source = self.source_file_text.as_ref()?;
        let pos = param_node.pos as usize;
        let line_start = source[..pos].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = source[pos..]
            .find('\n')
            .map_or(source.len(), |idx| pos + idx);
        let line = source.get(line_start..line_end)?;
        let open = Self::index_signature_open_bracket_before_pos(line, pos - line_start)?;
        let after_open = line.get(open + 1..)?;
        let colon = after_open.find(':')?;
        let candidate = after_open.get(..colon)?.trim();
        (!candidate.is_empty()
            && candidate
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
        .then(|| candidate.to_string())
    }

    fn index_signature_open_bracket_before_pos(line: &str, pos_in_line: usize) -> Option<usize> {
        line[..pos_in_line.min(line.len())]
            .char_indices()
            .rev()
            .find_map(|(idx, ch)| (ch == '[').then_some(idx))
    }
}
