//! Truncation and string-literal expansion helpers for declaration type inference.

use super::super::DeclarationEmitter;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn truncation_candidate_type_node(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = expr_idx;

        for _ in 0..100 {
            let node = self.arena.get(current)?;
            if let Some(assertion) = self.arena.get_type_assertion(node) {
                let asserted_type = self.arena.get(assertion.type_node)?;
                if asserted_type.kind == SyntaxKind::ConstKeyword as u16 {
                    return None;
                }
                return Some(assertion.type_node);
            }

            if node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                return None;
            }

            let access = self.arena.get_access_expr(node)?;
            let argument = self.arena.get(access.name_or_argument)?;
            let literal = self.arena.get_literal(argument)?;
            if argument.kind != SyntaxKind::NumericLiteral as u16 || literal.text != "0" {
                return None;
            }

            let array_node = self.arena.get(access.expression)?;
            let literal_expr = self.arena.get_literal_expr(array_node)?;
            if array_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || literal_expr.elements.nodes.len() != 1
            {
                return None;
            }

            current = literal_expr.elements.nodes[0];
        }

        None
    }

    pub(in crate::declaration_emitter) fn truncation_candidate_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let type_node = self.truncation_candidate_type_node(expr_idx)?;
        if let Some(type_id) = self.get_node_type_or_names(&[type_node]) {
            let printed = self.print_type_id(type_id);
            if printed != "any" {
                return Some(printed);
            }
        }
        self.emit_type_node_text(type_node)
    }

    pub(in crate::declaration_emitter) fn estimated_truncation_candidate_length(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<usize> {
        let type_node = self.truncation_candidate_type_node(expr_idx)?;
        self.estimate_serialized_type_length(type_node, &FxHashMap::default(), 0)
    }

    pub(in crate::declaration_emitter) fn estimate_serialized_type_length(
        &self,
        type_node: NodeIndex,
        substitutions: &FxHashMap<String, String>,
        depth: usize,
    ) -> Option<usize> {
        if depth > 32 {
            return None;
        }

        let node = self.arena.get(type_node)?;
        match node.kind {
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                let mapped = self.arena.get_mapped_type(node)?;
                let type_param = self.arena.get_type_parameter_at(mapped.type_parameter)?;
                let type_param_name = self.get_identifier_text(type_param.name)?;
                let constraint = if type_param.constraint != NodeIndex::NONE {
                    type_param.constraint
                } else {
                    return None;
                };
                let keys = self.expand_string_literals_from_type_node(
                    constraint,
                    substitutions,
                    depth + 1,
                )?;
                let mut total = 4usize;
                for key in keys {
                    let mut next = substitutions.clone();
                    next.insert(type_param_name.clone(), key.clone());
                    let value_len =
                        self.estimate_serialized_type_length(mapped.type_node, &next, depth + 1)?;
                    total = total
                        .saturating_add(self.serialized_property_name_length(&key))
                        .saturating_add(2)
                        .saturating_add(value_len)
                        .saturating_add(2);
                }
                Some(total)
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                let expansions = self.expand_string_literals_from_type_node(
                    type_node,
                    substitutions,
                    depth + 1,
                )?;
                let mut total = 0usize;
                for (idx, value) in expansions.iter().enumerate() {
                    if idx > 0 {
                        total = total.saturating_add(3);
                    }
                    total = total.saturating_add(value.len() + 2);
                }
                Some(total)
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = self.arena.get_type_ref(node)?;
                let name = self.type_reference_name_text(type_ref.type_name)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(value.len() + 2);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.estimate_serialized_type_length(alias_type, substitutions, depth + 1)
            }
            k if k == syntax_kind_ext::UNION_TYPE => {
                let composite = self.arena.get_composite_type(node)?;
                let mut total = 0usize;
                for (idx, child) in composite.types.nodes.iter().enumerate() {
                    if idx > 0 {
                        total = total.saturating_add(3);
                    }
                    total = total.saturating_add(self.estimate_serialized_type_length(
                        *child,
                        substitutions,
                        depth + 1,
                    )?);
                }
                Some(total)
            }
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                let literal = self.arena.get_literal_type(node)?;
                let literal_node = self.arena.get(literal.literal)?;
                match literal_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16 => {
                        Some(self.arena.get_literal(literal_node)?.text.len() + 2)
                    }
                    _ => None,
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self.get_identifier_text(type_node)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(value.len() + 2);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.estimate_serialized_type_length(alias_type, substitutions, depth + 1)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn expand_string_literals_from_type_node(
        &self,
        type_node: NodeIndex,
        substitutions: &FxHashMap<String, String>,
        depth: usize,
    ) -> Option<Vec<String>> {
        if depth > 32 {
            return None;
        }

        let node = self.arena.get(type_node)?;
        match node.kind {
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                let literal = self.arena.get_literal_type(node)?;
                let literal_node = self.arena.get(literal.literal)?;
                if literal_node.kind != SyntaxKind::StringLiteral as u16 {
                    return None;
                }
                Some(vec![self.arena.get_literal(literal_node)?.text.clone()])
            }
            k if k == syntax_kind_ext::UNION_TYPE => {
                let composite = self.arena.get_composite_type(node)?;
                let mut result = Vec::new();
                for child in &composite.types.nodes {
                    result.extend(self.expand_string_literals_from_type_node(
                        *child,
                        substitutions,
                        depth + 1,
                    )?);
                }
                Some(result)
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                let template = self.arena.get_template_literal_type(node)?;
                let head = self.arena.get(template.head)?;
                let head_text = self
                    .arena
                    .get_literal(head)
                    .map(|lit| lit.text.clone())
                    .unwrap_or_default();
                let mut results = vec![head_text];
                for span in &template.template_spans.nodes {
                    let data = self.arena.get_template_span_at(*span)?;
                    let expansions = self.expand_string_literals_from_type_node(
                        data.expression,
                        substitutions,
                        depth + 1,
                    )?;
                    let suffix = self
                        .arena
                        .get(data.literal)
                        .and_then(|literal| self.arena.get_literal(literal))
                        .map(|lit| lit.text.clone())
                        .unwrap_or_default();
                    let mut next =
                        Vec::with_capacity(results.len().saturating_mul(expansions.len()));
                    for prefix in &results {
                        for expansion in &expansions {
                            let mut combined = String::with_capacity(
                                prefix.len() + expansion.len() + suffix.len(),
                            );
                            combined.push_str(prefix);
                            combined.push_str(expansion);
                            combined.push_str(&suffix);
                            next.push(combined);
                        }
                    }
                    results = next;
                }
                Some(results)
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = self.arena.get_type_ref(node)?;
                let name = self.type_reference_name_text(type_ref.type_name)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(vec![value.clone()]);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.expand_string_literals_from_type_node(alias_type, substitutions, depth + 1)
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self.get_identifier_text(type_node)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(vec![value.clone()]);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.expand_string_literals_from_type_node(alias_type, substitutions, depth + 1)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn find_local_type_alias_type_node(
        &self,
        name: &str,
    ) -> Option<NodeIndex> {
        let binder = self.binder?;
        let symbol = binder
            .file_locals
            .get(name)
            .or_else(|| binder.current_scope.get(name))?;
        let declaration = binder.symbols.get(symbol)?.declarations.first().copied()?;
        let declaration_node = self.arena.get(declaration)?;
        self.arena
            .get_type_alias(declaration_node)
            .map(|alias| alias.type_node)
    }
}
