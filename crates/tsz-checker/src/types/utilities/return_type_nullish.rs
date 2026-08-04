//! Non-strict (`strictNullChecks: false`) `null`/`undefined` widening for a
//! block-bodied function's inferred return type.
//!
//! Split out of `return_type.rs`, which sits at the checker's 2000-line
//! boundary. The expression-bodied seam lives there
//! (`maybe_widen_return_contribution`); this is the block-bodied twin, called
//! per return contribution from `collect_return_types_in_statement`.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Apply the non-strict `null`/`undefined` → `any` widening to a fresh
    /// return contribution, but only when every nullish leaf the widener would
    /// touch actually originates from a *widening* nullish source.
    ///
    /// tsc gives the `null` keyword and the global `undefined` the widening
    /// flavour (`nullWideningType` / `undefinedWideningType`) and propagates it
    /// through array/object-literal construction, so `getWidenedType` maps those
    /// leaves to `any` when `strictNullChecks` is off:
    /// `function f() { return [undefined]; }` infers `any[]`. A leaf that is
    /// merely *typed* `undefined` carries no widening flavour — with
    /// `declare var q: undefined`, `function f() { return [q]; }` keeps
    /// `undefined[]`. tsz has no per-type widening flag, so the provenance is
    /// recovered from the return expression's own syntax; anything the walk
    /// cannot account for keeps the unwidened contribution.
    pub(crate) fn widen_nullish_return_contribution(
        &mut self,
        expr_idx: NodeIndex,
        type_id: TypeId,
    ) -> TypeId {
        if self.ctx.strict_null_checks() {
            return type_id;
        }
        let widened =
            crate::query_boundaries::widening::widen_nullish_to_any_deep(self.ctx.types, type_id);
        if widened == type_id {
            return type_id;
        }
        if self.return_contribution_nullish_leaves_are_widening(expr_idx, 0) {
            widened
        } else {
            type_id
        }
    }

    /// Whether every nullish leaf reachable through a return expression's fresh
    /// literal structure comes from a widening nullish source. See
    /// [`Self::widen_nullish_return_contribution`] for the rule this recovers.
    fn return_contribution_nullish_leaves_are_widening(
        &mut self,
        expr_idx: NodeIndex,
        depth: u8,
    ) -> bool {
        const MAX_NULLISH_PROVENANCE_DEPTH: u8 = 8;
        if depth > MAX_NULLISH_PROVENANCE_DEPTH {
            return false;
        }
        let expr_idx = self.unwrap_parenthesized_expression(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        let kind = node.kind;

        // Fresh literal structure: the widening flavour of a leaf propagates to
        // the array/object literal built around it, so recurse into the members
        // instead of judging the composite node itself.
        if kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let Some(array) = self.ctx.arena.get_literal_expr(node) else {
                return false;
            };
            let elements: Vec<NodeIndex> = array.elements.nodes.clone();
            return elements.into_iter().all(|element| {
                self.return_contribution_nullish_leaves_are_widening(element, depth + 1)
            });
        }
        if kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let Some(object) = self.ctx.arena.get_literal_expr(node) else {
                return false;
            };
            let elements: Vec<NodeIndex> = object.elements.nodes.clone();
            for element_idx in elements {
                let Some(element) = self.ctx.arena.get(element_idx) else {
                    return false;
                };
                // Only a plain `name: value` member exposes its own value
                // expression; spreads, shorthands, methods and accessors are
                // judged by the leaf rule below on the member node itself, which
                // rejects anything carrying a nullish leaf.
                let member_value = if element.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                    self.ctx
                        .arena
                        .get_property_assignment(element)
                        .map(|prop| prop.initializer)
                        .unwrap_or(element_idx)
                } else {
                    element_idx
                };
                if !self.return_contribution_nullish_leaves_are_widening(member_value, depth + 1) {
                    return false;
                }
            }
            return true;
        }

        // Leaf rule: a leaf whose checked type has no nullish leaf for the
        // widener to touch is always fine; one that does must be a widening
        // source (the `null` keyword or the global `undefined`).
        let Some(&leaf_type) = self.ctx.node_types.get(&expr_idx.0) else {
            return false;
        };
        let widened =
            crate::query_boundaries::widening::widen_nullish_to_any_deep(self.ctx.types, leaf_type);
        if widened == leaf_type {
            return true;
        }
        if kind == SyntaxKind::NullKeyword as u16 || kind == SyntaxKind::UndefinedKeyword as u16 {
            return true;
        }
        crate::flow_domain::control_flow::narrowing_helpers::is_global_undefined_identifier(
            self.ctx.arena,
            self.ctx.binder,
            expr_idx,
        )
    }
}
