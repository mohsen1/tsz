//! Widening-provenance gate for the mutable-binding non-strict nullish widen
//! (#16384 leg B).
//!
//! tsc's non-strict nullish widening (`nullWideningType`/`undefinedWideningType`
//! reaching `getWidenedType`) is a property of the *expression*, not the type:
//! only the bare `null`/`undefined` keyword, or an identifier resolving to the
//! global `undefined`, carries the widening flavour. A value that merely has
//! type `undefined`/`null` through a declared source (`declare var q: undefined`)
//! does not. tsz carries no per-type widening flag, so
//! [`CheckerState::widen_initializer_type_for_mutable_binding`] must recover the
//! provenance from the initializer's own fresh array/object-literal syntax
//! before calling the solver's purely type-shape-driven `widen_nullish_to_any_deep`.
//!
//! This mirrors the reusable gate the return-contribution seam needs for the
//! same rule (`return_contribution_nullish_leaves_are_widening`, tracked
//! separately in #16383/#16384 leg A); the two seams walk different AST shapes
//! (a return expression vs. a mutable-binding initializer) so are not merged
//! into one function, but the underlying rule — and the "fail closed on
//! anything the walk cannot account for" policy — is identical.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Whether every `null`/`undefined` leaf that
    /// [`crate::query_boundaries::widening::widen_nullish_to_any_deep`] would
    /// touch inside `expr`'s fresh array/object-literal structure is a genuine
    /// widening source, so the deep nullish-to-`any` widen is safe to apply.
    ///
    /// Recurses into nested fresh array/object literals (`[[undefined]]`,
    /// `{ p: [undefined] }`). Anything the walk cannot account for — a spread
    /// element, a shorthand/method/accessor object member, a computed key, or
    /// a leaf whose checked type was not cached — fails closed (`false`), so a
    /// case this gate cannot prove stays with tsz's prior (non-widening)
    /// behaviour rather than risk over-widening a declared value.
    pub(crate) fn initializer_nullish_leaves_are_widening(&self, expr: NodeIndex) -> bool {
        self.initializer_nullish_leaves_are_widening_inner(expr, 0)
    }

    /// Whether a *call argument* expression is itself a fresh array/object
    /// literal whose nullish leaves are all widening sources (#16384 leg A).
    ///
    /// tsc's `getInferredType` ends in `getWidenedType`, so a widening-flavoured
    /// argument propagates the flavour into the inferred type argument:
    /// `declare function id[ T ](x: T): T; var v = id([undefined]);` infers
    /// `any[]`, not `undefined[]`. The candidate seam is the only place this can
    /// be recovered — the variable-declaration seam cannot, because a call
    /// expression is never a fresh literal.
    ///
    /// Unlike [`CheckerState::is_fresh_literal_expression`], this deliberately
    /// does **not** follow identifiers to a fresh-by-reference initializer, and
    /// does not accept a bare literal token. The widening-provenance walk is
    /// only meaningful over literal syntax written at the argument position:
    /// `declare var qa: undefined[]; id(qa)` must keep `undefined[]`, and an
    /// identifier's own checked type carries no flavour to recover. The bare
    /// `undefined`/`null` argument (`id(undefined)` → `any`) already resolves
    /// correctly through the scalar path and is left alone here.
    pub(crate) fn fresh_literal_argument_nullish_leaves_are_widening(
        &self,
        expr: NodeIndex,
    ) -> bool {
        let expr = self.ctx.arena.skip_parenthesized(expr);
        let Some(node) = self.ctx.arena.get(expr) else {
            return false;
        };
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            && node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            return false;
        }
        self.initializer_nullish_leaves_are_widening(expr)
    }

    fn initializer_nullish_leaves_are_widening_inner(&self, expr: NodeIndex, depth: u8) -> bool {
        // Cycle / runaway-recursion guard, mirroring `is_fresh_literal_expression`.
        const MAX_DEPTH: u8 = 16;
        if depth > MAX_DEPTH {
            return false;
        }

        let expr = self.ctx.arena.skip_parenthesized(expr);
        let Some(node) = self.ctx.arena.get(expr) else {
            return false;
        };

        if node.kind == SyntaxKind::NullKeyword as u16
            || node.kind == SyntaxKind::UndefinedKeyword as u16
        {
            return true;
        }
        if node.kind == SyntaxKind::Identifier as u16
            && crate::control_flow::narrowing_helpers::is_global_undefined_identifier(
                self.ctx.arena,
                self.ctx.binder,
                expr,
            )
        {
            return true;
        }
        // A spread element's source is opaque to this walk (its elements are
        // not individually visible here) — fail closed rather than assume.
        if node.kind == syntax_kind_ext::SPREAD_ELEMENT {
            return false;
        }

        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let Some(array) = self.ctx.arena.get_literal_expr(node) else {
                return false;
            };
            return array
                .elements
                .nodes
                .iter()
                .all(|&elem| self.initializer_nullish_leaves_are_widening_inner(elem, depth + 1));
        }

        if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let Some(obj) = self.ctx.arena.get_literal_expr(node) else {
                return false;
            };
            return obj.elements.nodes.iter().all(|&elem| {
                let Some(elem_node) = self.ctx.arena.get(elem) else {
                    return false;
                };
                // Shorthand/method/accessor/spread members are opaque here —
                // only a plain `name: value` assignment is walked.
                let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                    return false;
                };
                self.initializer_nullish_leaves_are_widening_inner(prop.initializer, depth + 1)
            });
        }

        // Any other leaf expression (identifier, call, property access, ...):
        // it only needs to be a widening source when its own checked type is
        // actually nullish — a non-nullish leaf never reaches the widener.
        !matches!(
            self.ctx.node_types.get(&expr.0).copied(),
            Some(t) if t == TypeId::UNDEFINED || t == TypeId::NULL
        )
    }
}
