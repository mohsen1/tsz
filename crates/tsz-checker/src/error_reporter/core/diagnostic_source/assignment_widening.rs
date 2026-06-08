//! Assignment source widening helpers for TS2322-family diagnostics.

use super::literal_widening_helpers::target_accepts_literal_primitive_kind;
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Whether to suppress the AST-literal short-circuit for an
    /// object-literal-property elaboration when the property elaboration has
    /// already widened the source (e.g. `1` -> `number`). Mirrors tsc's
    /// `getWidenedLiteralLikeTypeForContextualType`: keep the literal display
    /// when the source's primitive kind appears as a literal kind somewhere in
    /// the target, otherwise widen.
    pub(in crate::error_reporter) fn property_elaboration_widening_required_for_display(
        &self,
        expr_idx: NodeIndex,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !self.is_property_assignment_initializer(expr_idx) {
            return false;
        }
        // Only fire when the caller passed in a non-literal primitive source
        // (i.e. the property elaboration already widened the literal). For
        // direct `let x: 1 = "abc"` style mismatches the source is still the
        // literal type, so this guard short-circuits.
        if !crate::query_boundaries::common::is_primitive_type(self.ctx.types, source) {
            return false;
        }
        if crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some() {
            return false;
        }
        let primitive_kind = source;
        !target_accepts_literal_primitive_kind(self.ctx.types, target, primitive_kind)
    }

    pub(in crate::error_reporter) fn array_elaboration_widening_required_for_display(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        use crate::query_boundaries::common;

        let source_primitive = if let Some(value) = common::literal_value(self.ctx.types, source) {
            value.primitive_type_id()
        } else if matches!(
            source,
            TypeId::STRING | TypeId::NUMBER | TypeId::BIGINT | TypeId::BOOLEAN
        ) {
            source
        } else {
            return false;
        };
        let target = common::evaluate_type(self.ctx.types, target);
        if target == TypeId::UNDEFINED || target == TypeId::NULL {
            return source_primitive != TypeId::BOOLEAN;
        }

        !target_accepts_literal_primitive_kind(self.ctx.types, target, source_primitive)
    }

    pub(in crate::error_reporter) fn array_literal_element_source_widening_required_for_display(
        &self,
        anchor_idx: NodeIndex,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !self.array_elaboration_widening_required_for_display(source, target) {
            return false;
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        self.ctx
            .arena
            .parent_of(expr_idx)
            .and_then(|parent_idx| self.ctx.arena.get(parent_idx))
            .is_some_and(|parent| parent.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
    }

    pub(in crate::error_reporter) fn is_object_rest_assignment_target_anchor(
        &self,
        anchor_idx: NodeIndex,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        let mut current = expr_idx;
        let mut saw_spread_wrapper = false;
        let mut object_idx = None;

        while let Some(parent_idx) = self.ctx.arena.parent_of(current) {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                || parent_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                saw_spread_wrapper = true;
                current = parent_idx;
                continue;
            }
            if parent_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION && saw_spread_wrapper
            {
                object_idx = Some(parent_idx);
                break;
            }
            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                || parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                || parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
            {
                break;
            }
            current = parent_idx;
        }
        let Some(object_idx) = object_idx else {
            return false;
        };

        self.assignment_target_expression(anchor_idx)
            .is_some_and(|target_idx| {
                self.ctx.arena.skip_parenthesized_and_assertions(target_idx) == object_idx
            })
    }
}
