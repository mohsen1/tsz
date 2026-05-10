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
        if !crate::query_boundaries::common::is_tuple_type(self.ctx.types, target) {
            return None;
        }

        let literal = self.ctx.arena.get_literal_expr(node)?;
        if literal.elements.nodes.is_empty() {
            return self.tuple_structural_source_display(source_type, target);
        }

        let target_elements =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, target);
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
    ) -> Option<String> {
        let display_element_idx = self.ctx.arena.skip_parenthesized(element_idx);
        let display_element_node = self.ctx.arena.get(display_element_idx)?;
        // In tuple source displays, tsc keeps boolean literals even when
        // contextual typing has widened the element type to `boolean`.
        if display_element_node.kind == SyntaxKind::TrueKeyword as u16 {
            return Some("true".to_string());
        }
        if display_element_node.kind == SyntaxKind::FalseKeyword as u16 {
            return Some("false".to_string());
        }

        if target_elements
            .as_ref()
            .and_then(|elements| elements.get(element_position))
            .is_some_and(|element| self.type_includes_literal_type(element.type_id))
        {
            return self.literal_expression_display(element_idx);
        }
        None
    }

    fn type_includes_literal_type(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::is_literal_type(self.ctx.types, type_id)
            || crate::query_boundaries::common::union_members(self.ctx.types, type_id).is_some_and(
                |members| {
                    members.iter().any(|member| {
                        crate::query_boundaries::common::is_literal_type(self.ctx.types, *member)
                    })
                },
            )
    }
}
