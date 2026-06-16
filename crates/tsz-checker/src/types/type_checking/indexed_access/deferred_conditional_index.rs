//! Index-key validation for indexed accesses whose object base is a *deferred*
//! conditional type. Split out of `indexed_access.rs` to keep that file under
//! the per-file LOC ceiling; behavior is unchanged.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// When the indexed-access object base is a *deferred* conditional, validate
    /// the index key against the conditional's base constraint — the union of
    /// its two branch results (tsc's `getBaseConstraintOfType` of a conditional).
    ///
    /// tsc keeps `C<T>['x']` deferred (so a concrete `C<string>['x']` still
    /// resolves to the selected branch), but validates `'x'` against
    /// `keyof ({ x: 1; y: 2 } | { x: 3; y: 4 })` = `'x' | 'y'`. The key is valid
    /// iff it lies in *every* branch's key space (the union's keyof is the
    /// intersection of member keys), so `C<T>['z']` and a key present in only
    /// one branch still correctly produce `TS2536`.
    ///
    /// A generic `Application` whose body is a conditional (e.g. `C<T>`) is
    /// expanded to that conditional first. Returns `true` when the key validates
    /// and `TS2536` should be suppressed.
    pub(super) fn deferred_conditional_index_key_is_valid(
        &mut self,
        object_type: TypeId,
        object_type_for_check: TypeId,
        index_type_for_check: TypeId,
    ) -> bool {
        let resolve_conditional = |this: &mut Self, ty: TypeId| -> Option<TypeId> {
            if crate::query_boundaries::common::is_conditional_type(this.ctx.types, ty) {
                return Some(ty);
            }
            if crate::query_boundaries::common::is_generic_application(this.ctx.types, ty) {
                let expanded = this.evaluate_application_type(ty);
                if crate::query_boundaries::common::is_conditional_type(this.ctx.types, expanded) {
                    return Some(expanded);
                }
            }
            None
        };

        let Some(conditional) = resolve_conditional(self, object_type_for_check)
            .or_else(|| resolve_conditional(self, object_type))
        else {
            return false;
        };

        let Some(base_constraint) =
            crate::query_boundaries::common::conditional_branch_union_constraint(
                self.ctx.types,
                conditional,
            )
        else {
            return false;
        };
        // Only trust the branch union when its *key space* resolved concretely:
        // a base constraint that is still a conditional / indexed access has no
        // computable key set. Generic *value* types are fine — an object whose
        // values are `infer` variables (e.g. `{ yield: Y; return: R }`) still has
        // a concrete key set `'yield' | 'return'`, so the key validates.
        if crate::query_boundaries::common::is_conditional_type(self.ctx.types, base_constraint)
            || crate::query_boundaries::common::is_index_access_type(
                self.ctx.types,
                base_constraint,
            )
        {
            return false;
        }
        let keyof_constraint = self.ctx.types.evaluate_keyof(base_constraint);
        // The key space itself must be concrete (not a deferred `keyof T`) for a
        // concrete-literal index validation to be meaningful.
        if crate::query_boundaries::common::is_keyof_type(self.ctx.types, keyof_constraint) {
            return false;
        }
        self.indexed_access_key_space_relation_outcome(index_type_for_check, keyof_constraint)
            .related
    }
}
