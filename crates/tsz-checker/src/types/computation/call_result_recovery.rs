//! Recovery-return helpers for failed call resolution.
//!
//! Thin checker-side adapters over the `query_boundaries::checkers::call`
//! recovery walks, split out of `call_result.rs` to keep that file under the
//! architecture LOC ceiling. Behavior is unchanged — these forward to the same
//! query-boundary entry points.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Recovery return type for a generic call that failed the argument-count
    /// check, with the signature's own type parameters resolved to their
    /// `default → constraint → unknown` fallback (matching tsc). Falls back to
    /// the plain recovery if the default-resolving walk finds no signature.
    pub(crate) fn stable_call_recovery_return_type_with_default_type_args(
        &self,
        callee_type: TypeId,
    ) -> Option<TypeId> {
        crate::query_boundaries::checkers::call::stable_call_recovery_return_type_with_default_type_args(
            self.ctx.types,
            callee_type,
        )
    }
}
