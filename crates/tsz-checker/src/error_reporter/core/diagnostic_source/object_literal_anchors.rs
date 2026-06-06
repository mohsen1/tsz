use crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn object_literal_initializer_anchor_for_type(
        &mut self,
        object_idx: NodeIndex,
        source_type: TypeId,
    ) -> Option<(u32, u32)> {
        let mut current = self.ctx.arena.skip_parenthesized_and_assertions(object_idx);
        let mut guard = 0;

        loop {
            guard += 1;
            if guard > 32 {
                return None;
            }

            let node = self.ctx.arena.get(current)?;

            let direct_initializer =
                if let Some(prop) = self.ctx.arena.get_property_assignment(node) {
                    Some(prop.initializer)
                } else {
                    self.ctx
                        .arena
                        .get_shorthand_property(node)
                        .map(|prop| prop.name)
                };

            if let Some(initializer_idx) = direct_initializer {
                if let Some(anchor) =
                    self.resolve_diagnostic_anchor(initializer_idx, DiagnosticAnchorKind::Exact)
                {
                    return Some((anchor.start, anchor.length));
                }

                let (pos, end) = self.get_node_span(initializer_idx)?;
                return Some(self.normalized_anchor_span(
                    initializer_idx,
                    pos,
                    end.saturating_sub(pos),
                ));
            }

            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                let literal = self.ctx.arena.get_literal_expr(node)?;
                let source_display = self.format_type_for_assignability_message(
                    self.widen_type_for_display(source_type),
                );

                for child_idx in literal.elements.nodes.iter().copied() {
                    let Some(child) = self.ctx.arena.get(child_idx) else {
                        continue;
                    };

                    let candidate_idx =
                        if let Some(prop) = self.ctx.arena.get_property_assignment(child) {
                            prop.initializer
                        } else if let Some(prop) = self.ctx.arena.get_shorthand_property(child) {
                            prop.name
                        } else {
                            continue;
                        };

                    let candidate_type = self.get_type_of_node(candidate_idx);
                    if matches!(candidate_type, TypeId::ERROR | TypeId::UNKNOWN) {
                        continue;
                    }

                    let candidate_display = self.format_type_for_assignability_message(
                        self.widen_type_for_display(candidate_type),
                    );
                    if candidate_type != source_type && candidate_display != source_display {
                        continue;
                    }

                    if let Some(anchor) =
                        self.resolve_diagnostic_anchor(candidate_idx, DiagnosticAnchorKind::Exact)
                    {
                        return Some((anchor.start, anchor.length));
                    }

                    let (pos, end) = self.get_node_span(candidate_idx)?;
                    return Some(self.normalized_anchor_span(
                        candidate_idx,
                        pos,
                        end.saturating_sub(pos),
                    ));
                }

                return None;
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = self.ctx.arena.skip_parenthesized_and_assertions(ext.parent);
        }
    }
}
