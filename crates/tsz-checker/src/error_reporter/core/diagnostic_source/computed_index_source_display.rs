//! Computed-index and literal target source display helpers.

use super::literal_widening_helpers::literal_display_appropriate_for_undefined_null_target;
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Merge every wide (non-entity-name) `string`/`number` computed key in
    /// an object literal into ONE synthesized `[x: kind]: V` index-signature
    /// clause, matching `tsc`'s `checkObjectLiteral` display for a fresh
    /// literal type it cannot re-spell member-by-member.
    ///
    /// Requires every element to be a computed key of the SAME kind — a
    /// literal-spelled name, a different kind, or a non-computed member all
    /// bail to the natural per-member display in the caller. Bails too when
    /// EVERY key in the group is an entity-name reference (a plain
    /// identifier or dotted `a.b.c` chain): `tsc` can re-spell those from
    /// their own syntax and shows each individually instead, unmerged and
    /// with no value-type union (see
    /// `object_literal_source_display.rs`'s per-member loop). Only ONE
    /// non-entity-name member in the group is enough to fold every sibling —
    /// entity-named or not — into the single merged clause (oracle-verified
    /// against `typescript@7.0.2`, #16721).
    pub(in crate::error_reporter) fn computed_index_signature_object_literal_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: Option<TypeId>,
    ) -> Option<String> {
        let target = target?;
        let shape = crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)?;
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let literal = self.ctx.arena.get_literal_expr(node)?;
        let mut computed_key_kind = None;
        let mut computed_value_types = Vec::new();
        let mut any_non_entity_name = false;

        for child_idx in literal.elements.nodes.iter().copied() {
            let child = self.ctx.arena.get(child_idx)?;
            let prop = self.ctx.arena.get_property_assignment(child)?;
            let name_node = self.ctx.arena.get(prop.name)?;
            if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                return None;
            }
            let computed = self.ctx.arena.get_computed_property(name_node)?;
            let raw_key_type = self.get_type_of_node(computed.expression);
            let key_type = self.widen_type_for_display(raw_key_type);
            let key_kind = if key_type == TypeId::STRING {
                "string"
            } else if key_type == TypeId::NUMBER {
                "number"
            } else {
                return None;
            };
            if computed_key_kind.is_some_and(|existing| existing != key_kind) {
                return None;
            }
            computed_key_kind = Some(key_kind);
            if !self.computed_key_is_entity_name_reference(prop.name) {
                any_non_entity_name = true;
            }

            let value_type = self.get_type_of_node(prop.initializer);
            if value_type == TypeId::ERROR {
                return None;
            }
            computed_value_types.push(self.widen_type_for_display(value_type));
        }

        if !any_non_entity_name {
            return None;
        }
        let key_kind = computed_key_kind?;
        if computed_value_types.is_empty()
            || !((key_kind == "string" && shape.string_index.is_some())
                || (key_kind == "number" && shape.number_index.is_some()))
        {
            return None;
        }

        let value_type = crate::query_boundaries::diagnostics::source_display_union_type(
            self.ctx.types,
            computed_value_types,
        );
        let value_display = self.format_type_for_assignability_message(value_type);
        Some(format!("{{ [x: {key_kind}]: {value_display}; }}"))
    }

    pub(in crate::error_reporter) fn literal_assignment_source_display_for_target(
        &mut self,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        if self.in_arithmetic_compound_assignment_context(anchor_idx)
            || !crate::query_boundaries::common::is_template_literal_type(self.ctx.types, target)
        {
            return None;
        }
        let expr_idx = self
            .assignment_source_expression(anchor_idx)
            .or_else(|| self.direct_diagnostic_source_expression(anchor_idx))?;
        let display = self.literal_expression_display(expr_idx)?;
        literal_display_appropriate_for_undefined_null_target(self.ctx.types, target, &display)
            .then_some(display)
    }
}
