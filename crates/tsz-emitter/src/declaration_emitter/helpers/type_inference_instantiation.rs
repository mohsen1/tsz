//! Instantiation-expression and short-circuit fallback type text helpers.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

#[derive(Clone)]
struct ShortCircuitTypePart {
    text: String,
    source_order: Option<u32>,
}

impl<'a> DeclarationEmitter<'a> {
    pub(super) fn short_circuit_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        self.short_circuit_expression_type_parts(expr_idx, 0)
            .map(Self::format_short_circuit_type_parts)
    }

    fn short_circuit_expression_type_parts(
        &self,
        expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<Vec<ShortCircuitTypePart>> {
        if depth > 8 {
            return None;
        }
        let expr_node = self.arena.get(expr_idx)?;
        let binary = self.arena.get_binary_expr(expr_node)?;
        let operator = binary.operator_token;
        if operator != SyntaxKind::BarBarToken as u16
            && operator != SyntaxKind::QuestionQuestionToken as u16
        {
            return None;
        }

        let mut left_parts = self.short_circuit_operand_type_parts(binary.left, depth + 1)?;
        let right_parts = self.short_circuit_operand_type_parts(binary.right, depth + 1)?;

        if operator == SyntaxKind::BarBarToken as u16 {
            left_parts.retain(|part| !Self::short_circuit_or_excludes_left_type(&part.text));
        } else {
            left_parts.retain(|part| !Self::short_circuit_nullish_excludes_left_type(&part.text));
        }

        let mut parts = left_parts;
        parts.extend(right_parts);
        Self::dedupe_and_sort_short_circuit_type_parts(&mut parts);
        if parts.is_empty() {
            return None;
        }
        Some(parts)
    }

    fn short_circuit_operand_type_parts(
        &self,
        expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<Vec<ShortCircuitTypePart>> {
        if depth > 8 {
            return None;
        }
        let expr_idx = self.skip_parenthesized_expression_via_parent_node(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(parts) = self.short_circuit_expression_type_parts(expr_idx, depth + 1)
        {
            return Some(parts);
        }
        if expr_node.kind == SyntaxKind::Identifier as u16
            && let Some(initializer) = self.short_circuit_reference_initializer(expr_idx)
            && let Some(parts) = self.short_circuit_expression_type_parts(initializer, depth + 1)
        {
            return Some(parts);
        }

        let text = self.short_circuit_operand_type_text(expr_idx)?;
        let mut parts = Self::split_top_level_union_type_parts(&text)
            .into_iter()
            .map(|text| ShortCircuitTypePart {
                text,
                source_order: self.short_circuit_source_order(expr_idx),
            })
            .collect::<Vec<_>>();
        Self::dedupe_and_sort_short_circuit_type_parts(&mut parts);
        Some(parts)
    }

    fn short_circuit_reference_initializer(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            let var_decl = self.arena.get_variable_declaration(decl_node)?;
            if var_decl.initializer.is_some()
                && self
                    .arena
                    .get(var_decl.initializer)
                    .is_some_and(|node| node.kind == syntax_kind_ext::BINARY_EXPRESSION)
            {
                return Some(var_decl.initializer);
            }
        }
        None
    }

    fn short_circuit_source_order(&self, expr_idx: NodeIndex) -> Option<u32> {
        if self
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
            && let Some(sym_id) = self.value_reference_symbol(expr_idx)
            && let Some(symbol) = self.binder.and_then(|binder| binder.symbols.get(sym_id))
            && let Some(decl_idx) = symbol.declarations.first().copied()
            && let Some(decl_node) = self.arena.get(decl_idx)
        {
            return Some(decl_node.pos);
        }
        self.arena.get(expr_idx).map(|node| node.pos)
    }

    fn short_circuit_or_excludes_left_type(type_text: &str) -> bool {
        let trimmed = type_text.trim();
        Self::short_circuit_nullish_excludes_left_type(trimmed)
            || trimmed == "false"
            || trimmed == "0"
            || trimmed == "-0"
            || trimmed == "0n"
            || trimmed == "\"\""
            || trimmed == "''"
    }

    fn short_circuit_nullish_excludes_left_type(type_text: &str) -> bool {
        matches!(type_text.trim(), "null" | "undefined" | "void")
    }

    fn dedupe_and_sort_short_circuit_type_parts(parts: &mut Vec<ShortCircuitTypePart>) {
        let mut deduped: Vec<ShortCircuitTypePart> = Vec::new();
        for part in parts.drain(..) {
            if let Some(existing) = deduped
                .iter_mut()
                .find(|existing| existing.text == part.text)
            {
                if existing.source_order.is_none()
                    || part
                        .source_order
                        .is_some_and(|order| existing.source_order.is_none_or(|old| order < old))
                {
                    existing.source_order = part.source_order;
                }
            } else {
                deduped.push(part);
            }
        }
        deduped.sort_by(
            |left, right| match (left.source_order, right.source_order) {
                (Some(left_order), Some(right_order)) => left_order.cmp(&right_order),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            },
        );
        *parts = deduped;
    }

    fn format_short_circuit_type_parts(parts: Vec<ShortCircuitTypePart>) -> String {
        if parts.len() == 1 {
            return parts[0].text.clone();
        }
        parts
            .into_iter()
            .map(|part| Self::parenthesize_type_text_in_union_position(&part.text))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn short_circuit_operand_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        self.short_circuit_operand_type_text_at(expr_idx, 0)
    }

    fn short_circuit_operand_type_text_at(
        &self,
        expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        if depth > 8 {
            return None;
        }
        let expr_idx = self.skip_parenthesized_expression_via_parent_node(expr_idx)?;
        self.preferred_expression_type_text(expr_idx)
            .or_else(|| self.infer_fallback_type_text_at(expr_idx, 0))
            .or_else(|| {
                let expr_idx = self.skip_parenthesized_expression_via_parent_node(expr_idx)?;
                self.preferred_expression_type_text(expr_idx)
                    .or_else(|| self.infer_fallback_type_text_at(expr_idx, 0))
            })
    }

    fn skip_parenthesized_expression_via_parent_node(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = expr_idx;
        loop {
            let node = self.arena.get(current)?;
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return Some(current);
            }
            current = self.arena.get_parenthesized(node)?.expression;
        }
    }

    pub(super) fn instantiation_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let expr = self.arena.get_expr_type_args(expr_node)?;
        let type_args = self.type_argument_list_source_text(expr.type_arguments.as_ref());
        let [type_arg] = type_args.as_slice() else {
            return None;
        };
        let base_text = self
            .preferred_expression_type_text(expr.expression)
            .or_else(|| self.reference_declared_type_annotation_text(expr.expression))
            .or_else(|| {
                self.get_node_type_or_names(&[expr.expression])
                    .map(|type_id| self.print_type_id(type_id))
            })?;
        Self::instantiate_type_text_with_single_type_arg(&base_text, type_arg)
    }

    fn instantiate_type_text_with_single_type_arg(
        type_text: &str,
        type_arg: &str,
    ) -> Option<String> {
        let trimmed = type_text.trim();
        if trimmed.starts_with('{') {
            return Self::instantiate_object_type_text_with_single_type_arg(trimmed, type_arg);
        }

        let parts = Self::split_top_level_union_type_parts(trimmed);
        if parts.len() > 1 {
            let instantiated_parts: Vec<String> = parts
                .iter()
                .map(|part| {
                    Self::instantiate_generic_function_type_text(part, type_arg)
                        .unwrap_or_else(|| part.to_string())
                })
                .map(|part| Self::parenthesize_type_text_in_union_position(&part))
                .collect();
            return Some(instantiated_parts.join(" | "));
        }

        Self::instantiate_generic_function_type_text(trimmed, type_arg)
    }

    fn instantiate_object_type_text_with_single_type_arg(
        type_text: &str,
        type_arg: &str,
    ) -> Option<String> {
        let mut changed = false;
        let mut removed_non_generic_call = false;
        let lines: Vec<String> = type_text
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim_start();
                if trimmed.starts_with('<')
                    && (trimmed.contains("():") || trimmed.contains("=>"))
                    && let Some(instantiated) =
                        Self::instantiate_generic_function_type_text(trimmed, type_arg)
                {
                    changed = true;
                    let indent = &line[..line.len() - trimmed.len()];
                    return Some(format!("{indent}{instantiated}"));
                }
                if !changed && (trimmed.starts_with("():") || trimmed.starts_with("new (")) {
                    removed_non_generic_call = true;
                    return None;
                }
                Some(line.to_string())
            })
            .collect();

        if changed || removed_non_generic_call {
            Some(lines.join("\n"))
        } else {
            None
        }
    }

    fn instantiate_generic_function_type_text(type_text: &str, type_arg: &str) -> Option<String> {
        let trimmed = type_text.trim();
        let (prefix, body, suffix) = if trimmed.starts_with('(') && trimmed.ends_with(')') {
            ("(", &trimmed[1..trimmed.len() - 1], ")")
        } else {
            ("", trimmed, "")
        };
        let generic_start = body.find('<')?;
        let after_start = generic_start + 1;
        let generic_end = body[after_start..].find('>')? + after_start;
        let type_param = body[after_start..generic_end].trim();
        if !Self::is_simple_identifier_text(type_param) {
            return None;
        }

        let mut instantiated = String::new();
        instantiated.push_str(&body[..generic_start]);
        instantiated.push_str(&body[generic_end + 1..]);
        instantiated = Self::replace_whole_words_in_text(
            &instantiated,
            &[(type_param.to_string(), type_arg.to_string())],
        );
        Some(format!("{prefix}{instantiated}{suffix}"))
    }
}
