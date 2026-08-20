//! Eager evaluation of a monomorphic meta return type, keeping display
//! provenance anchored on the declared-return application.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Eagerly evaluate a monomorphic `Application`/conditional call return
    /// (the `finalize_call_return_like_success` fast path that avoids nested
    /// return chains), then re-anchor the evaluated result's display alias.
    ///
    /// The evaluated structural result's display alias is first-writer-wins;
    /// an inference-internal application interned during the same call's
    /// return-context merge scan can claim it first, repainting diagnostic
    /// heads with the forwarded base (`PairRow<...>`) where `tsc` renders the
    /// declared-return alias application (`FlipRow<...>`). The re-anchoring is
    /// gated on the existing claim being that application's own forwarded
    /// view, so an unrelated first writer is never repainted.
    pub(super) fn eagerly_evaluate_meta_return_type(
        &mut self,
        return_type: TypeId,
        is_monomorphic_application: bool,
    ) -> TypeId {
        let evaluated = if is_monomorphic_application
            && self.return_application_uses_opaque_object_base(return_type)
        {
            self.evaluate_application_type_for_property_access(return_type)
        } else {
            self.evaluate_type_with_env(return_type)
        };
        if is_monomorphic_application {
            crate::query_boundaries::assignability_alias_display::
                repoint_evaluated_call_return_display_alias(
                    self.ctx.types,
                    &self.ctx.definition_store,
                    evaluated,
                    return_type,
                );
        }
        evaluated
    }
}
