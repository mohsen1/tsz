//! Index-key validation for indexed accesses whose object base is a *deferred*
//! conditional type. Split out of `indexed_access.rs` to keep that file under
//! the per-file LOC ceiling; behavior is unchanged.

use crate::query_boundaries::indexed_access_key_space as key_space_query;
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
            crate::query_boundaries::conditional_constraints::conditional_branch_union_constraint(
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
        let keyof_constraint = self.indexed_access_keyof_with_env(base_constraint);
        // The key space itself must be concrete (not a deferred `keyof T`) for a
        // concrete-literal index validation to be meaningful.
        if crate::query_boundaries::common::is_keyof_type(self.ctx.types, keyof_constraint) {
            return false;
        }
        self.indexed_access_key_space_relation_outcome(index_type_for_check, keyof_constraint)
            .related
    }

    /// Apparent type of a *deferred* indexed access `B[K1]` whose base `B` is a
    /// deferred conditional (e.g. `Cond<T>['required']`). tsc resolves such a
    /// generic indexed access through `getConstraintOfIndexedAccessType`: it
    /// indexes `B`'s base constraint — the union of the conditional's branch
    /// results — with `K1`. That apparent type carries a concrete key space even
    /// while the access itself stays deferred, so a later access
    /// `B[K1][K2]` (or a spread `[...B[K1]]`) validates `K2`/array-likeness
    /// against it instead of failing as if `B[K1]` had no members.
    ///
    /// Returns the evaluated apparent type, or `None` when `object_type` is not
    /// such a deferred indexed access, the inner key is not a usable literal, the
    /// conditional has no resolvable branch-union constraint, or the apparent
    /// type cannot be reduced to a concrete (non-deferred) form.
    pub(super) fn deferred_indexed_access_conditional_apparent_type(
        &mut self,
        object_type: TypeId,
    ) -> Option<TypeId> {
        use crate::query_boundaries::common as q;
        use crate::query_boundaries::conditional_constraints as conditional_query;
        // Resolve a generic `Application` (e.g. an alias `Drop<T>`) whose body is
        // an indexed access into that indexed access first, so both the direct and
        // alias-wrapped forms are covered.
        let resolve_index_access = |this: &mut Self, ty: TypeId| -> Option<(TypeId, TypeId)> {
            if let Some(parts) = q::index_access_types(this.ctx.types, ty) {
                return Some(parts);
            }
            if q::is_generic_application(this.ctx.types, ty) {
                let expanded = this.evaluate_application_type(ty);
                if let Some(parts) = q::index_access_types(this.ctx.types, expanded) {
                    return Some(parts);
                }
            }
            None
        };
        let (inner_base, inner_index) = resolve_index_access(self, object_type)?;

        // The inner base must resolve to a deferred conditional; otherwise the
        // existing concrete/type-parameter paths already own validation.
        let resolve_conditional = |this: &mut Self, ty: TypeId| -> Option<TypeId> {
            if q::is_conditional_type(this.ctx.types, ty) {
                return Some(ty);
            }
            if q::is_generic_application(this.ctx.types, ty) {
                let expanded = this.evaluate_application_type(ty);
                if q::is_conditional_type(this.ctx.types, expanded) {
                    return Some(expanded);
                }
            }
            None
        };
        let conditional = match resolve_conditional(self, inner_base) {
            Some(c) => c,
            None => {
                let evaluated_base = self.evaluate_type_with_env(inner_base);
                resolve_conditional(self, evaluated_base)?
            }
        };

        let base_constraint =
            conditional_query::conditional_branch_union_constraint(self.ctx.types, conditional)?;
        // The branch-union must have resolved to a concrete shape; a constraint
        // that is itself still deferred has no computable apparent type.
        if q::is_conditional_type(self.ctx.types, base_constraint)
            || q::is_index_access_type(self.ctx.types, base_constraint)
        {
            return None;
        }

        // The inner key drives `apparent = base_constraint[inner_index]`. Evaluate
        // it; an apparent type that is still deferred (e.g. the inner key was
        // itself generic) gives no concrete key space, so bail.
        let apparent = self.evaluate_type_with_env(key_space_query::indexed_access_type(
            self.ctx.types,
            base_constraint,
            inner_index,
        ));
        if apparent == TypeId::ERROR
            || apparent == TypeId::ANY
            || q::is_index_access_type(self.ctx.types, apparent)
            || q::is_conditional_type(self.ctx.types, apparent)
        {
            return None;
        }
        Some(apparent)
    }

    /// Index-key validation for `B[K1][K2]` where `B[K1]` is a deferred indexed
    /// access into a deferred conditional. Validates the outer literal key `K2`
    /// against the key space of the apparent type of `B[K1]` (see
    /// [`Self::deferred_indexed_access_conditional_apparent_type`]). tsc accepts
    /// `Cond<T>['required']['length']` (length ∈ keyof of the tuple apparent
    /// type) but still rejects `Cond<T>['required']['nope']`.
    pub(super) fn deferred_indexed_access_conditional_key_is_valid(
        &mut self,
        object_type: TypeId,
        object_type_for_check: TypeId,
        index_type_for_check: TypeId,
    ) -> bool {
        let Some(apparent) = self
            .deferred_indexed_access_conditional_apparent_type(object_type_for_check)
            .or_else(|| self.deferred_indexed_access_conditional_apparent_type(object_type))
        else {
            return false;
        };
        let keyof_apparent = self.indexed_access_keyof_with_env(apparent);
        if crate::query_boundaries::common::is_keyof_type(self.ctx.types, keyof_apparent) {
            return false;
        }
        self.indexed_access_key_space_relation_outcome(index_type_for_check, keyof_apparent)
            .related
    }
}
