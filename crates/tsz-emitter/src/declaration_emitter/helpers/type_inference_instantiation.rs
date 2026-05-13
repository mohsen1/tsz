//! Instantiation-expression and short-circuit fallback type text helpers.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::types::TypeId;

#[derive(Clone)]
struct ShortCircuitOrTypeText {
    text: String,
    widened_from_inferred_literal: bool,
}

impl<'a> DeclarationEmitter<'a> {
    pub(super) fn short_circuit_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let binary = self.arena.get_binary_expr(expr_node)?;

        if binary.operator_token == SyntaxKind::BarBarToken as u16
            || binary.operator_token == SyntaxKind::QuestionQuestionToken as u16
        {
            if self.expression_type_is_never_for_decl_emit(binary.right)
                && let Some(left_text) = self
                    .short_circuit_operand_type_text(binary.left)
                    .filter(|text| text != "any" && text != "unknown")
                    .or_else(|| {
                        self.enclosing_parameter_type_annotation_text_for_identifier(binary.left)
                    })
                    .or_else(|| self.reference_declared_type_annotation_text(binary.left))
                && let Some(non_undefined) = Self::remove_undefined_from_union_text(&left_text)
                && Self::is_simple_identifier_text(&non_undefined)
            {
                return Some(format!("NonNullable<{non_undefined}>"));
            }

            if let (Some(left_text), Some(right_text)) = (
                self.short_circuit_operand_type_text(binary.left),
                self.short_circuit_operand_type_text(binary.right),
            ) {
                if Self::remove_undefined_from_union_text(&left_text)
                    .as_deref()
                    .is_some_and(|left_without_undefined| {
                        Self::type_texts_match_ignoring_redundant_parens(
                            left_without_undefined,
                            &right_text,
                        )
                    })
                {
                    return Some(right_text);
                }
            }
        }

        if binary.operator_token == SyntaxKind::BarBarToken as u16 {
            if let Some(type_text) = self.short_circuit_or_literal_type_text(expr_idx) {
                return Some(type_text.text);
            }

            if !self.expression_is_always_truthy_for_decl_emit(binary.left) {
                return None;
            }

            return self
                .preferred_expression_type_text(binary.left)
                .or_else(|| {
                    self.get_node_type_or_names(&[binary.left])
                        .map(|type_id| self.print_type_id(type_id))
                });
        }

        None
    }

    fn short_circuit_or_literal_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<ShortCircuitOrTypeText> {
        let expr_idx = self.skip_parenthesized_expression_via_parent_node(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::BarBarToken as u16 {
            return None;
        }

        let left = self.short_circuit_or_literal_operand_type_text(binary.left)?;
        let right = self.short_circuit_or_literal_operand_type_text(binary.right)?;
        Self::combine_short_circuit_or_literal_types(left, right)
    }

    fn short_circuit_or_literal_operand_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<ShortCircuitOrTypeText> {
        let expr_idx = self.skip_parenthesized_expression_via_parent_node(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;

        if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && self
                .arena
                .get_binary_expr(expr_node)
                .is_some_and(|binary| binary.operator_token == SyntaxKind::BarBarToken as u16)
        {
            return self.short_circuit_or_literal_type_text(expr_idx);
        }

        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        if let Some(type_text) = self.reference_declared_type_annotation_text(expr_idx) {
            return Self::short_circuit_or_literal_operand_from_text(&type_text, false);
        }

        if let Some(initializer) = self.unannotated_const_literal_reference_initializer(expr_idx) {
            let initializer = self.skip_parenthesized_expression_via_parent_node(initializer)?;
            if let Some(type_text) = self.short_circuit_or_literal_type_text(initializer) {
                return Some(type_text);
            }
            if let Some(type_text) =
                self.short_circuit_widened_literal_initializer_type_text(initializer)
            {
                return Self::short_circuit_or_literal_operand_from_text(&type_text, true);
            }
        }

        None
    }

    fn unannotated_const_literal_reference_initializer(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            let Some(var_decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if self.arena.is_const_variable_declaration(decl_idx)
                && var_decl.type_annotation.is_none()
                && var_decl.initializer.is_some()
            {
                return Some(var_decl.initializer);
            }
        }

        None
    }

    fn short_circuit_widened_literal_initializer_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_expression_via_parent_node(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                Some("string".to_string())
            }
            k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
            k if k == SyntaxKind::BigIntLiteral as u16 => Some("bigint".to_string()),
            k if k == SyntaxKind::TrueKeyword as u16 || k == SyntaxKind::FalseKeyword as u16 => {
                Some("boolean".to_string())
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self.is_negative_literal(expr_node) =>
            {
                let unary = self.arena.get_unary_expr(expr_node)?;
                let operand = self.arena.get(unary.operand)?;
                match operand.kind {
                    k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
                    k if k == SyntaxKind::BigIntLiteral as u16 => Some("bigint".to_string()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn short_circuit_or_literal_operand_from_text(
        type_text: &str,
        widened_from_inferred_literal: bool,
    ) -> Option<ShortCircuitOrTypeText> {
        let parts = Self::split_top_level_union_type_parts(type_text);
        if parts
            .iter()
            .all(|part| Self::short_circuit_literal_part_kind(part).is_some())
        {
            return Some(ShortCircuitOrTypeText {
                text: parts.join(" | "),
                widened_from_inferred_literal,
            });
        }
        None
    }

    fn combine_short_circuit_or_literal_types(
        left: ShortCircuitOrTypeText,
        right: ShortCircuitOrTypeText,
    ) -> Option<ShortCircuitOrTypeText> {
        let left_parts = Self::split_top_level_union_type_parts(&left.text);
        let right_parts = Self::split_top_level_union_type_parts(&right.text);
        let mut parts = Vec::new();
        parts.extend(left_parts);
        parts.extend(right_parts);

        if parts
            .iter()
            .any(|part| Self::short_circuit_literal_part_kind(part).is_none())
        {
            return None;
        }

        let should_widen = left.widened_from_inferred_literal
            || right.widened_from_inferred_literal
            || parts
                .iter()
                .any(|part| Self::short_circuit_literal_part_is_wide(part));

        if should_widen {
            let mut widened_parts = Vec::new();
            for kind in ["string", "number", "boolean", "bigint"] {
                if parts
                    .iter()
                    .any(|part| Self::short_circuit_literal_part_kind(part) == Some(kind))
                {
                    widened_parts.push(kind.to_string());
                }
            }
            return Some(ShortCircuitOrTypeText {
                text: widened_parts.join(" | "),
                widened_from_inferred_literal: true,
            });
        }

        let mut literal_parts = Vec::new();
        for part in parts {
            if !literal_parts.contains(&part) {
                literal_parts.push(part);
            }
        }
        Self::sort_primitive_name_string_literals(&mut literal_parts);

        Some(ShortCircuitOrTypeText {
            text: literal_parts.join(" | "),
            widened_from_inferred_literal: false,
        })
    }

    fn short_circuit_literal_part_kind(part: &str) -> Option<&'static str> {
        let part = part.trim();
        match part {
            "string" => return Some("string"),
            "number" => return Some("number"),
            "boolean" | "true" | "false" => return Some("boolean"),
            "bigint" => return Some("bigint"),
            _ => {}
        }

        if (part.starts_with('"') && part.ends_with('"'))
            || (part.starts_with('\'') && part.ends_with('\''))
        {
            return Some("string");
        }

        if part
            .strip_prefix('-')
            .unwrap_or(part)
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.')
            && part.chars().any(|ch| ch.is_ascii_digit())
        {
            return Some("number");
        }

        if part.ends_with('n')
            && part[..part.len() - 1]
                .strip_prefix('-')
                .unwrap_or(&part[..part.len() - 1])
                .chars()
                .all(|ch| ch.is_ascii_digit())
        {
            return Some("bigint");
        }

        None
    }

    fn short_circuit_literal_part_is_wide(part: &str) -> bool {
        matches!(part.trim(), "string" | "number" | "boolean" | "bigint")
    }

    fn sort_primitive_name_string_literals(parts: &mut [String]) {
        fn primitive_name_rank(part: &str) -> Option<usize> {
            match part.trim() {
                r#""string""# | "'string'" => Some(0),
                r#""number""# | "'number'" => Some(1),
                r#""boolean""# | "'boolean'" => Some(2),
                _ => None,
            }
        }

        if parts.iter().all(|part| primitive_name_rank(part).is_some()) {
            parts.sort_by_key(|part| primitive_name_rank(part).unwrap_or(usize::MAX));
        }
    }

    pub(in crate::declaration_emitter) fn expression_type_is_never_for_decl_emit(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        self.get_node_type_or_names(&[expr_idx])
            .is_some_and(|type_id| type_id == TypeId::NEVER)
    }

    fn short_circuit_operand_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
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

    pub(in crate::declaration_emitter) fn remove_undefined_from_union_text(
        type_text: &str,
    ) -> Option<String> {
        let parts = Self::split_top_level_union_type_parts(type_text);
        if parts.len() <= 1 || !parts.iter().any(|part| part == "undefined") {
            return None;
        }
        let remaining: Vec<String> = parts
            .into_iter()
            .filter(|part| part != "undefined")
            .map(|part| Self::parenthesize_type_text_in_union_position(&part))
            .collect();
        if let [single] = remaining.as_slice() {
            return Some(Self::strip_redundant_function_wrapper_parens(single).to_string());
        }
        Some(remaining.join(" | "))
    }

    fn type_texts_match_ignoring_redundant_parens(left: &str, right: &str) -> bool {
        Self::strip_redundant_function_wrapper_parens(left)
            == Self::strip_redundant_function_wrapper_parens(right)
    }

    fn strip_redundant_function_wrapper_parens(type_text: &str) -> &str {
        let trimmed = type_text.trim();
        if trimmed.starts_with("((") && trimmed.ends_with(')') && trimmed.contains("=>") {
            &trimmed[1..trimmed.len() - 1]
        } else {
            trimmed
        }
    }
}
