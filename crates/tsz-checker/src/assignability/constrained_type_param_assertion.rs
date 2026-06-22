use crate::state::CheckerState;
use tsz_common::{Atom, Visibility};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Returns true when `target` is a constrained type parameter `T extends C`
    /// and `source`'s required members structurally fit `C`, matching `tsc`'s
    /// deferred assertion overlap rule for `source as T`.
    pub(crate) fn assertion_source_fits_constrained_type_param(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(info) = crate::query_boundaries::common::type_param_info(self.ctx.types, target)
        else {
            return false;
        };
        let Some(raw_constraint) = info.constraint else {
            return false;
        };
        if raw_constraint == TypeId::ANY || raw_constraint == TypeId::UNKNOWN {
            return false;
        }
        if crate::query_boundaries::assignability::contains_type_parameters(
            self.ctx.types,
            raw_constraint,
        ) {
            return false;
        }

        let resolved_source = self.evaluate_type_with_resolution(source);
        let resolved_constraint = self.evaluate_type_with_resolution(raw_constraint);

        let source_props =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, resolved_source)
                .map(|shape| shape.properties.to_vec())
                .unwrap_or_default();
        if source_props.is_empty() {
            // Empty-object and non-object cases stay on the solver overlap path.
            return false;
        }
        if source_props
            .iter()
            .any(|p| p.visibility != Visibility::Public)
        {
            return false;
        }

        let mut saw_required = false;
        for prop in source_props.iter().filter(|p| !p.optional) {
            saw_required = true;
            if !self.constrained_type_param_constraint_provides_member(
                resolved_constraint,
                prop.name,
                prop.type_id,
                0,
            ) {
                return false;
            }
        }
        saw_required
    }

    /// Assertion overlap when `source` (or `target`) is a *deferred conditional*
    /// or an indexed access whose object is one. tsc compares against the
    /// conditional's base constraint — the union of its branch results — so
    /// resolve that base constraint *with the resolver* (the solver-only
    /// comparability path cannot expand `keyof`/index access without it) and
    /// retry the overlap check.
    ///
    /// Covers `Box<T>[keyof Box<T>] as string` (tanstack-router `Matches.ts:96`)
    /// where `Box<T> = T extends string ? { a: T } : { a: string }`: the base
    /// constraint indexed by `keyof` is `T | string`, a string-domain type that
    /// overlaps `string`.
    pub(crate) fn deferred_conditional_assertion_overlaps(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if let Some(resolved) = self.deferred_conditional_assertion_constraint(source)
            && resolved != source
            && self.deferred_conditional_constraint_overlaps(resolved, target)
        {
            return true;
        }
        if let Some(resolved) = self.deferred_conditional_assertion_constraint(target)
            && resolved != target
            && self.deferred_conditional_constraint_overlaps(source, resolved)
        {
            return true;
        }
        false
    }

    /// Overlap check against a resolved deferred-conditional base constraint.
    ///
    /// When the resolved constraint still contains a free type parameter (e.g.
    /// `T | string`), tsc treats the assertion permissively — the type parameter
    /// could instantiate to overlap the other side — so report overlap. This
    /// mirrors the permissive `generic_indexed_assertion_source` path for an
    /// indexed access that surfaces only after alias expansion. Otherwise fall
    /// back to the structural comparability check.
    fn deferred_conditional_constraint_overlaps(&mut self, source: TypeId, target: TypeId) -> bool {
        if crate::query_boundaries::common::contains_free_type_parameters(self.ctx.types, source)
            || crate::query_boundaries::common::contains_free_type_parameters(
                self.ctx.types,
                target,
            )
        {
            return true;
        }
        crate::query_boundaries::common::types_are_comparable_for_assertion(
            self.ctx.types,
            source,
            target,
        )
    }

    /// Resolve a deferred conditional (or `Obj[Key]` over one) to its base
    /// constraint using the resolver-backed evaluator, so a deferred `keyof`
    /// index collapses to a concrete key space.
    fn deferred_conditional_assertion_constraint(&mut self, type_id: TypeId) -> Option<TypeId> {
        let evaluated = self.evaluate_type_with_resolution(type_id);

        if let Some(constraint) =
            crate::query_boundaries::conditional_constraints::conditional_branch_union_constraint(
                self.ctx.types,
                evaluated,
            )
        {
            return Some(self.evaluate_type_with_resolution(constraint));
        }

        let (object_type, key_type) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, evaluated)?;
        let object_constraint =
            crate::query_boundaries::conditional_constraints::conditional_branch_union_constraint(
                self.ctx.types,
                self.evaluate_type_with_resolution(object_type),
            )?;
        // Resolve the index key against the branch-union's key space. A deferred
        // `keyof Box<T>` index has no concrete keys until the conditional
        // resolves; over the branch-union it becomes `keyof ({ a: T } | { a: string })`
        // = `'a'`, so the access yields the value domain `T | string`. A concrete
        // (already-resolved) key indexes the union directly.
        let resolved_key = if crate::query_boundaries::common::is_keyof_type(
            self.ctx.types,
            self.evaluate_type_with_resolution(key_type),
        ) {
            self.ctx.types.evaluate_keyof(object_constraint)
        } else {
            key_type
        };
        let indexed = self
            .ctx
            .types
            .factory()
            .index_access(object_constraint, resolved_key);
        let resolved = self.evaluate_type_with_env(indexed);
        if resolved != evaluated && resolved != type_id && resolved != TypeId::ERROR {
            Some(resolved)
        } else {
            None
        }
    }

    fn constrained_type_param_constraint_provides_member(
        &mut self,
        constraint: TypeId,
        name: Atom,
        source_prop_type: TypeId,
        depth: u32,
    ) -> bool {
        if depth > 10 {
            return false;
        }
        if constraint == TypeId::ANY || constraint == TypeId::UNKNOWN {
            return true;
        }

        let constraint = self.evaluate_type_with_resolution(constraint);
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, constraint)
        {
            return members.iter().any(|&member| {
                self.constrained_type_param_constraint_provides_member(
                    member,
                    name,
                    source_prop_type,
                    depth + 1,
                )
            });
        }
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, constraint)
        {
            return members.iter().any(|&member| {
                self.constrained_type_param_constraint_provides_member(
                    member,
                    name,
                    source_prop_type,
                    depth + 1,
                )
            });
        }
        if let Some(prop) = crate::query_boundaries::common::find_property_in_object(
            self.ctx.types,
            constraint,
            name,
        ) {
            return crate::query_boundaries::common::types_are_comparable_for_assertion(
                self.ctx.types,
                source_prop_type,
                prop.type_id,
            );
        }
        if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, constraint)
            && let Some(idx) = shape.string_index
        {
            return crate::query_boundaries::common::types_are_comparable_for_assertion(
                self.ctx.types,
                source_prop_type,
                idx.value_type,
            );
        }
        false
    }
}
