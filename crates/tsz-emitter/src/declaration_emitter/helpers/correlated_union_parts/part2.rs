impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn single_generic_type_argument_text(
        type_text: &str,
    ) -> Option<(&str, &str)> {
        let type_text = type_text.trim();
        let open = type_text.find('<')?;
        if !type_text.ends_with('>') {
            return None;
        }
        let wrapper = type_text[..open].trim();
        if wrapper.is_empty()
            || wrapper
                .chars()
                .any(|ch| !(ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric()))
        {
            return None;
        }
        let inner = &type_text[open + 1..type_text.len() - 1];
        let mut depth = 0usize;
        for ch in inner.chars() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth = depth.checked_sub(1)?;
                }
                ',' if depth == 0 => return None,
                _ => {}
            }
        }
        (depth == 0).then_some((wrapper, inner.trim()))
    }

    pub(super) fn type_param_constraint_text<'b>(
        type_param_constraints: &'b [(String, String)],
        type_param_name: &str,
    ) -> Option<&'b str> {
        type_param_constraints
            .iter()
            .find_map(|(name, constraint)| (name == type_param_name).then_some(constraint.as_str()))
    }

    pub(in crate::declaration_emitter) fn infer_single_alias_discriminant_substitution(
        &self,
        source_arena: &NodeArena,
        param_type_text: &str,
        arg_idx: NodeIndex,
        type_param_names: &[String],
    ) -> Option<(String, String)> {
        let (alias_name, param_name) =
            Self::single_type_parameter_alias_argument(param_type_text, type_param_names)?;
        let alias_type_node = self.find_type_alias_type_node_in_arena(source_arena, alias_name)?;
        let shape = self.correlated_alias_shape(source_arena, alias_type_node)?;
        let value_text = self.object_literal_property_literal_type_text(
            arg_idx,
            &shape.discriminant_property_name,
        )?;
        Some((param_name.to_string(), value_text))
    }

    pub(in crate::declaration_emitter) fn single_type_parameter_alias_argument<'b>(
        type_text: &'b str,
        type_param_names: &'b [String],
    ) -> Option<(&'b str, &'b str)> {
        let trimmed = type_text.trim();
        let open = trimmed.find('<')?;
        let alias_name = trimmed.get(..open)?.trim();
        if alias_name.is_empty()
            || !alias_name
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        {
            return None;
        }
        let inner = trimmed.get(open + 1..)?.trim().strip_suffix('>')?.trim();
        type_param_names
            .iter()
            .find(|name| name.as_str() == inner)
            .map(|name| (alias_name, name.as_str()))
    }

    pub(in crate::declaration_emitter) fn object_literal_property_literal_type_text(
        &self,
        object_idx: NodeIndex,
        property_name: &str,
    ) -> Option<String> {
        let object_idx = self.skip_parenthesized_expression(object_idx)?;
        let object_node = self.arena.get(object_idx)?;
        if object_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(object_node)?;
        for &member_idx in &object.elements.nodes {
            let member_node = self.arena.get(member_idx)?;
            let name_idx = self.object_literal_member_name_idx(member_node)?;
            if self.object_literal_member_name_text(name_idx)? != property_name {
                continue;
            }
            let initializer = self.object_literal_member_initializer(member_node)?;
            return self
                .const_literal_initializer_text(initializer)
                .or_else(|| self.infer_fallback_type_text_at(initializer, 0));
        }
        None
    }

    pub(in crate::declaration_emitter) fn call_argument_type_text_for_substitution(
        &self,
        arg_idx: NodeIndex,
        type_param_constraint: Option<&str>,
    ) -> Option<String> {
        if let Some(type_text) = self.lexical_parameter_declared_type_annotation_text(arg_idx) {
            return Some(type_text);
        }
        if let Some(type_text) = self.referenced_parameter_declared_type_annotation_text(arg_idx) {
            return Some(type_text);
        }
        if let Some(type_text) = self.reference_declared_source_type_annotation_text(arg_idx) {
            return Some(type_text);
        }
        if let Some(type_text) = self.reference_declared_type_annotation_text(arg_idx) {
            return Some(type_text);
        }

        if let Some(type_text) =
            self.contextual_function_argument_type_text(arg_idx, type_param_constraint)
        {
            return Some(type_text);
        }

        // Bare type-parameter inference widens literal arguments (`box(0)` ->
        // `Box<number>`, not `Box<0>`). Keep literal-preserving paths only for
        // explicit `as const`, local variable initializers that already carry
        // literal types, or primitive literals inferred into primitive-constrained
        // type parameters.
        self.as_const_assertion_type_text(arg_idx)
            .or_else(|| self.local_variable_initializer_type_text(arg_idx))
            .or_else(|| {
                type_param_constraint
                    .is_some_and(Self::constraint_preserves_primitive_literal)
                    .then(|| self.primitive_literal_argument_type_text(arg_idx))
                    .flatten()
            })
            .or_else(|| {
                self.preferred_expression_type_text(arg_idx)
                    .filter(|text| text != "any" && text != "unknown" && !text.contains("any"))
            })
            .or_else(|| self.infer_fallback_type_text_at(arg_idx, 0))
    }

    pub(super) fn lexical_parameter_declared_type_annotation_text(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        if arg_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let arg_name = self.get_identifier_text(arg_idx)?;

        let mut current = arg_idx;
        for _ in 0..32 {
            let Some(parent) = self.arena.parent_of(current) else {
                break;
            };
            current = parent;
            let Some(parent_node) = self.arena.get(current) else {
                continue;
            };
            let Some(func) = self.arena.get_function(parent_node) else {
                continue;
            };
            for &param_idx in &func.parameters.nodes {
                let Some(param_node) = self.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.arena.get_parameter(param_node) else {
                    continue;
                };
                if self.get_identifier_text(param.name).as_deref() != Some(arg_name.as_str())
                    || !param.type_annotation.is_some()
                {
                    continue;
                }
                let type_text = self
                    .emit_type_node_text(param.type_annotation)
                    .or_else(|| self.source_slice_from_arena(self.arena, param.type_annotation))?;
                let trimmed = type_text.trim_end();
                let trimmed = trimmed.strip_suffix(';').unwrap_or(trimmed).trim_end();
                let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
                return Some(trimmed.to_string());
            }
        }

        None
    }

    fn call_argument_type_texts_for_rest_substitution(
        &self,
        arg_idx: NodeIndex,
        type_param_constraint: Option<&str>,
    ) -> Option<Vec<String>> {
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        if arg_node.kind != syntax_kind_ext::SPREAD_ELEMENT {
            return self
                .call_argument_type_text_for_substitution(arg_idx, type_param_constraint)
                .map(|text| vec![text]);
        }

        let spread = self.arena.get_spread(arg_node)?;
        let spread_expr = self.skip_parenthesized_expression(spread.expression)?;
        let spread_type_text = self
            .get_node_type(spread_expr)
            .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
            .filter(|text| Self::tuple_type_text_elements(text).is_some())
            .or_else(|| self.reference_declared_type_annotation_text(spread_expr))
            .or_else(|| self.local_variable_initializer_type_text(spread_expr))
            .or_else(|| self.preferred_expression_type_text(spread_expr))
            .or_else(|| {
                self.get_node_type(spread_expr)
                    .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
            })
            .or_else(|| self.infer_fallback_type_text_at(spread_expr, 0))?;

        if let Some(elements) = Self::tuple_type_text_elements(&spread_type_text) {
            return Some(elements);
        }

        Some(vec![spread_type_text])
    }

    fn tuple_type_text_elements(type_text: &str) -> Option<Vec<String>> {
        let mut text = type_text.trim();
        if let Some(rest) = text.strip_prefix("readonly ") {
            text = rest.trim();
        }
        if !text.starts_with('[') || !text.ends_with(']') {
            return None;
        }
        let inner = text[1..text.len() - 1].trim();
        if inner.is_empty() {
            return Some(Vec::new());
        }
        Some(
            Self::split_top_level_commas(inner)
                .into_iter()
                .map(|part| {
                    let mut part = part.trim();
                    if let Some(rest) = part.strip_prefix("...") {
                        part = rest.trim();
                    }
                    if let Some(colon) = Self::find_top_level_byte(part, b':') {
                        part = part[colon + 1..].trim();
                    }
                    part.trim_end_matches('?').trim().to_string()
                })
                .collect(),
        )
    }

    fn contextual_function_argument_type_text(
        &self,
        arg_idx: NodeIndex,
        type_param_constraint: Option<&str>,
    ) -> Option<String> {
        let expected = type_param_constraint?.trim();
        let expected = expected.strip_suffix("[]").unwrap_or(expected).trim();
        let expected = expected
            .strip_prefix('(')
            .and_then(|text| text.strip_suffix(')'))
            .unwrap_or(expected)
            .trim();
        let expected_parts = Self::parse_function_type_text(expected)?;
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        if arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.arena.get_function(arg_node)?;
        let params = func
            .parameters
            .nodes
            .iter()
            .copied()
            .enumerate()
            .map(|(position, param_idx)| {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                let name = self.get_identifier_text(param.name)?;
                let type_text = self
                    .emit_type_node_text(param.type_annotation)
                    .or_else(|| {
                        expected_parts
                            .parameters
                            .get(position)
                            .map(|param| param.type_text.clone())
                    })
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("{name}: {type_text}"))
            })
            .collect::<Option<Vec<_>>>()?;
        let return_text = self
            .emit_type_node_text(func.type_annotation)
            .or_else(|| self.contextual_function_body_return_type_text(func, &expected_parts))
            .unwrap_or_else(|| expected_parts.return_type.clone());
        Some(format!("({}) => {return_text}", params.join(", ")))
    }

    fn contextual_function_body_return_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        expected_parts: &super::type_inference_function_text::FunctionTypeTextParts,
    ) -> Option<String> {
        let body = self.skip_parenthesized_expression(func.body)?;
        let body_text = self
            .source_file_text
            .as_deref()
            .and_then(|text| {
                let node = self.arena.get(body)?;
                let start = usize::try_from(node.pos).ok()?;
                let end = usize::try_from(node.end).ok()?;
                text.get(start..end)
            })
            .unwrap_or_default();
        if body_text.contains("\"\"") || body_text.contains("''") || body_text.contains('`') {
            return Some("string".to_string());
        }
        if expected_parts
            .parameters
            .iter()
            .any(|param| param.type_text.trim() == "number")
            && body_text.contains('+')
        {
            return Some("number".to_string());
        }
        self.infer_fallback_type_text_at(body, 0)
    }

    fn constraint_preserves_primitive_literal(constraint: &str) -> bool {
        Self::contains_whole_word_in_text(constraint, "string")
            || Self::contains_whole_word_in_text(constraint, "number")
            || Self::contains_whole_word_in_text(constraint, "boolean")
            || Self::contains_whole_word_in_text(constraint, "bigint")
    }

    pub(in crate::declaration_emitter) fn primitive_literal_argument_type_text(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let arg_idx = self.skip_parenthesized_expression(arg_idx)?;
        let arg_node = self.arena.get(arg_idx)?;
        (arg_node.kind == SyntaxKind::StringLiteral as u16
            || arg_node.kind == SyntaxKind::NumericLiteral as u16
            || arg_node.kind == SyntaxKind::BigIntLiteral as u16
            || arg_node.kind == SyntaxKind::TrueKeyword as u16
            || arg_node.kind == SyntaxKind::FalseKeyword as u16)
            .then(|| self.js_literal_type_text(arg_idx))
            .flatten()
    }

    pub(super) fn referenced_parameter_declared_type_annotation_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.value_reference_symbol(expr_idx)?;

        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let mut current = decl_idx;
            for _ in 0..12 {
                let node = source_arena.get(current)?;
                if let Some(param) = source_arena.get_parameter(node) {
                    let type_annotation = param.type_annotation;
                    if !type_annotation.is_some() {
                        return None;
                    }
                    let type_text = self
                        .emit_type_node_text_from_arena(source_arena, type_annotation)
                        .or_else(|| self.source_slice_from_arena(source_arena, type_annotation))?;
                    let trimmed = type_text.trim_end();
                    let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
                    return Some(trimmed.to_string());
                }
                let parent = source_arena.parent_of(current)?;
                if parent.is_none() {
                    break;
                }
                current = parent;
            }
            None
        })
    }
}
