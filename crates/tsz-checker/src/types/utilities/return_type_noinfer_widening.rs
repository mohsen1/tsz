//! `NoInfer<T>` callback-return literal-widening display support (#17501).
//!
//! Split out of `return_type.rs`, which sits at the checker's 2000-line
//! boundary. A fresh literal returned from a callback whose contextual return is
//! `NoInfer<T>` over an *inferred* type parameter is widened by tsc's
//! `getReturnTypeFromBody` while `T` is unfixed (`isLiteralOfContextualType` is
//! false for a bare type variable) and the widened type is displayed. tsz marks
//! such bodies during the unfixed inference phase
//! (`return_contribution_is_widenable`, in `return_type.rs`) and reproduces the
//! widen for the return-mismatch elaboration here.

use crate::query_boundaries::function_returns as return_type_queries;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Record `body_idx` for widened-source return-mismatch elaboration when its
    /// contextual return `ctx_type` is a `NoInfer<X>` whose wrapped `X` still
    /// contains a free type parameter — the deferred-inference phase where tsc
    /// widens the fresh return literal (`isLiteralOfContextualType` is false for a
    /// bare type variable). A no-op otherwise; in particular an explicit type
    /// argument never reaches this unfixed phase, so an explicit `NoInfer<"foo">`
    /// callback is never recorded and keeps its literal. See
    /// [`CheckerContext::noinfer_generic_return_bodies`](crate::context::CheckerContext).
    pub(crate) fn mark_noinfer_generic_return_body_if_applicable(
        &mut self,
        body_idx: NodeIndex,
        ctx_type: TypeId,
    ) {
        if body_idx.is_none() {
            return;
        }
        let is_deferred_inference_phase =
            crate::query_boundaries::common::no_infer_inner_type(self.ctx.types, ctx_type)
                .is_some_and(|inner| {
                    return_type_queries::contains_free_type_parameters(self.ctx.types, inner)
                });
        if is_deferred_inference_phase {
            self.ctx.noinfer_generic_return_bodies.insert(body_idx);
        }
    }

    /// The widened *display* source type for an expression-body callback return
    /// recorded in [`CheckerContext::noinfer_generic_return_bodies`](crate::context::CheckerContext)
    /// (a `NoInfer<T>` contextual return whose `T` was inferred, not explicit), or
    /// `None` when the recorded-body widen does not apply.
    ///
    /// The return-mismatch elaboration otherwise renders the raw literal spelling
    /// read back from the body's AST node, so it routes through the
    /// widened-source-display path with this type instead. Returns `None` —
    /// keeping the literal — for a body that was never recorded (an explicit
    /// `NoInfer<"foo">`, a concrete-literal contextual return), a `const`
    /// assertion, or a `preserve_literal_types` context; the assignability
    /// decision is unaffected either way.
    pub(crate) fn noinfer_generic_return_body_widened_display(
        &mut self,
        body_idx: NodeIndex,
        type_id: TypeId,
    ) -> Option<TypeId> {
        if body_idx.is_none()
            || !self.ctx.noinfer_generic_return_bodies.contains(&body_idx)
            || self.ctx.preserve_literal_types
            || self.return_expression_is_const_assertion(body_idx)
            || !self.is_fresh_literal_expression(body_idx)
        {
            return None;
        }
        let widened = self.widen_return_contribution_preserving_const(body_idx, type_id);
        Some(self.widen_enum_member_type(widened))
    }
}
