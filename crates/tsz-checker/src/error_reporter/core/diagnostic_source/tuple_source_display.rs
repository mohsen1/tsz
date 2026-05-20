use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn array_literal_tuple_source_type_display(
        &mut self,
        expr_idx: NodeIndex,
        source_type: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let target = self.evaluate_type_for_assignability(target);
        let is_plain_tuple = crate::query_boundaries::common::is_tuple_type(self.ctx.types, target);
        if !is_plain_tuple
            && crate::query_boundaries::common::union_members(self.ctx.types, target).is_none()
        {
            return None;
        }

        let literal = self.ctx.arena.get_literal_expr(node)?;
        if literal.elements.nodes.is_empty() {
            if is_plain_tuple {
                return self.tuple_structural_source_display(source_type, target);
            }
            return None;
        }

        let target_elements = if is_plain_tuple {
            crate::query_boundaries::common::tuple_elements(self.ctx.types, target)
        } else {
            Some(self.tuple_elements_for_union_tuple_target(target)?)
        };
        let literal_len = literal.elements.nodes.len();
        let target_rest_layout = target_elements.as_ref().and_then(|elements| {
            elements
                .iter()
                .position(|element| element.rest)
                .map(|rest_index| {
                    let suffix_len = elements.len().saturating_sub(rest_index + 1);
                    let suffix_start_in_source = literal_len.saturating_sub(suffix_len);
                    (rest_index, suffix_len, suffix_start_in_source)
                })
        });
        let mut parts = Vec::with_capacity(literal.elements.nodes.len());
        for (element_position, element_idx) in literal.elements.nodes.iter().copied().enumerate() {
            let element_node = self.ctx.arena.get(element_idx)?;
            if element_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                return None;
            }
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                parts.push("undefined".to_string());
                continue;
            }
            if let Some(display) = self.array_literal_tuple_element_source_display(
                element_idx,
                element_position,
                &target_elements,
                target_rest_layout,
            ) {
                parts.push(display);
                continue;
            }

            let element_type = self.get_type_of_node(element_idx);
            let display_type = self.widen_type_for_display(element_type);
            parts.push(self.format_type_for_assignability_message(display_type));
        }

        Some(format!("[{}]", parts.join(", ")))
    }

    fn array_literal_tuple_element_source_display(
        &mut self,
        element_idx: NodeIndex,
        element_position: usize,
        target_elements: &Option<Vec<tsz_solver::TupleElement>>,
        target_rest_layout: Option<(usize, usize, usize)>,
    ) -> Option<String> {
        let display_element_idx = self.ctx.arena.skip_parenthesized(element_idx);
        let display_element_node = self.ctx.arena.get(display_element_idx)?;
        let target_element = self.target_tuple_element_for_literal_position(
            target_elements,
            element_position,
            target_rest_layout,
        );

        // Boolean literals are preserved for boolean-like tuple positions,
        // including optional elements and fixed suffix elements after a
        // variadic rest. Other primitive tuple targets use the widened
        // fallback.
        if target_element
            .is_some_and(|element| self.tuple_target_element_preserves_boolean_literal(element))
            || self.tuple_boolean_rest_before_position_preserves_literal(
                target_elements,
                element_position,
            )
        {
            if display_element_node.kind == SyntaxKind::TrueKeyword as u16 {
                return Some("true".to_string());
            }
            if display_element_node.kind == SyntaxKind::FalseKeyword as u16 {
                return Some("false".to_string());
            }
        }

        if target_element.is_some_and(|element| self.type_includes_literal_type(element.type_id)) {
            return self.literal_expression_display(element_idx);
        }
        None
    }

    fn target_tuple_element_for_literal_position<'b>(
        &self,
        target_elements: &'b Option<Vec<tsz_solver::TupleElement>>,
        element_position: usize,
        target_rest_layout: Option<(usize, usize, usize)>,
    ) -> Option<&'b tsz_solver::TupleElement> {
        let elements = target_elements.as_ref()?;
        if let Some((rest_index, suffix_len, suffix_start_in_source)) = target_rest_layout {
            if element_position < rest_index {
                return elements.get(element_position);
            }
            if suffix_len > 0 && element_position >= suffix_start_in_source {
                let suffix_offset = element_position.saturating_sub(suffix_start_in_source);
                return elements.get(rest_index + 1 + suffix_offset);
            }
            return elements.get(rest_index);
        }

        elements.get(element_position)
    }

    fn tuple_target_element_preserves_boolean_literal(
        &self,
        element: &tsz_solver::TupleElement,
    ) -> bool {
        self.type_preserves_boolean_literal(element.type_id)
            || (element.rest
                && crate::query_boundaries::common::array_element_type(
                    self.ctx.types,
                    element.type_id,
                )
                .is_some_and(|element_type| self.type_preserves_boolean_literal(element_type)))
    }

    fn tuple_boolean_rest_before_position_preserves_literal(
        &self,
        target_elements: &Option<Vec<tsz_solver::TupleElement>>,
        element_position: usize,
    ) -> bool {
        target_elements.as_ref().is_some_and(|elements| {
            elements.iter().enumerate().any(|(index, element)| {
                element.rest
                    && index <= element_position
                    && self.tuple_target_element_preserves_boolean_literal(element)
            })
        })
    }

    fn type_includes_literal_type(&self, type_id: TypeId) -> bool {
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            let has_true = members.contains(&TypeId::BOOLEAN_TRUE);
            let has_false = members.contains(&TypeId::BOOLEAN_FALSE);
            if has_true
                && has_false
                && members.iter().all(|member| {
                    matches!(
                        *member,
                        TypeId::BOOLEAN_TRUE
                            | TypeId::BOOLEAN_FALSE
                            | TypeId::UNDEFINED
                            | TypeId::NULL
                    )
                })
            {
                return false;
            }
            return members
                .iter()
                .any(|&member| self.type_includes_literal_type(member));
        }
        crate::query_boundaries::common::is_literal_type(self.ctx.types, type_id)
    }

    fn type_preserves_boolean_literal(&self, type_id: TypeId) -> bool {
        type_id == TypeId::BOOLEAN
            || crate::query_boundaries::common::union_members(self.ctx.types, type_id)
                .is_some_and(|members| members.contains(&TypeId::BOOLEAN))
            || self.type_includes_literal_type(type_id)
    }

    /// Build a synthetic `Vec<TupleElement>` for a union-of-tuples target where
    /// each position's `type_id` is the union of the corresponding element type
    /// from every member tuple.
    ///
    /// Returns `None` when the target is not a union, any member is not a plain
    /// fixed-length tuple (rest elements are rejected), or the members have
    /// different arities. The caller falls through to the widened source display
    /// in those cases.
    fn tuple_elements_for_union_tuple_target(
        &self,
        target: TypeId,
    ) -> Option<Vec<tsz_solver::TupleElement>> {
        let members = crate::query_boundaries::common::union_members(self.ctx.types, target)?;
        let mut per_member: Vec<Vec<tsz_solver::TupleElement>> = Vec::with_capacity(members.len());
        let mut arity = 0usize;
        for &member in &members {
            let resolved = crate::query_boundaries::common::evaluate_type(self.ctx.types, member);
            let elements =
                crate::query_boundaries::common::tuple_elements(self.ctx.types, resolved)?;
            if elements.iter().any(|e| e.rest) {
                return None;
            }
            let len = elements.len();
            if per_member.is_empty() {
                if len == 0 {
                    return None;
                }
                arity = len;
            } else if len != arity {
                return None;
            }
            per_member.push(elements);
        }
        let factory = self.ctx.types.factory();
        // Precompute optional flags to avoid an O(members) scan per position.
        let optional_at: Vec<bool> = (0..arity)
            .map(|pos| per_member.iter().any(|e| e[pos].optional))
            .collect();
        // Reuse one allocation across positions to avoid a fresh Vec per iteration.
        let mut type_ids_buf: Vec<TypeId> = Vec::with_capacity(per_member.len());
        let mut result = Vec::with_capacity(arity);
        for position in 0..arity {
            type_ids_buf.clear();
            type_ids_buf.extend(per_member.iter().map(|elems| elems[position].type_id));
            let union_type = if type_ids_buf.len() == 1 {
                type_ids_buf[0]
            } else {
                factory.union_from_slice(&type_ids_buf)
            };
            result.push(tsz_solver::TupleElement {
                type_id: union_type,
                name: None,
                optional: optional_at[position],
                rest: false,
            });
        }
        Some(result)
    }
}
