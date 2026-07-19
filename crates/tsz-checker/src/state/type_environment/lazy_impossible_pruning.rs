//! Speculative pruning of provably-impossible object-union members.
//!
//! Split out of `lazy.rs` (it shares that module's `CheckerState` impl and
//! `evaluate_type_with_resolution` entry point). These helpers remove union
//! members whose evaluated shape is uninhabitable because an intersection has
//! conflicting unit-literal discriminants, so a relation never has to reason
//! about a member that can hold no value. A required `never` property alone does
//! not make an object impossible: TypeScript preserves those members in unions.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    pub(crate) fn prune_impossible_object_union_members_with_env(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        // Guard against infinite mutual recursion: evaluate → prune → evaluate members → prune.
        // Pruning calls evaluate_type_with_resolution on each union member, which can resolve
        // to new unions that get pruned again. Since pruning is a speculative optimization
        // (removing provably-impossible union members), skipping nested calls is always safe.
        if self.ctx.pruning_union_members {
            return type_id;
        }
        self.ctx.pruning_union_members = true;
        let result = self.prune_impossible_object_union_members_inner(type_id);
        self.ctx.pruning_union_members = false;
        result
    }

    fn prune_impossible_object_union_members_inner(&mut self, type_id: TypeId) -> TypeId {
        let Some(members) =
            crate::query_boundaries::state::checking::union_members(self.ctx.types, type_id)
        else {
            return type_id;
        };
        let total_members = members.len();

        let retained: Vec<_> = members
            .into_iter()
            .filter(|&member| {
                !self.intersection_has_impossible_literal_discriminants_with_env(member)
            })
            .collect();

        match retained.len() {
            0 => TypeId::NEVER,
            len if len == total_members => type_id,
            1 => retained[0],
            _ => self.ctx.types.union_preserve_members(retained),
        }
    }

    fn intersection_has_impossible_literal_discriminants_with_env(
        &mut self,
        type_id: TypeId,
    ) -> bool {
        // Concrete object intersections are eagerly merged for O(1) member
        // lookup. Inspect their retained structural origin so a required
        // `never` produced by conflicting discriminants is pruned without
        // treating an authored required-`never` property as impossible.
        let evaluated = self.evaluate_type_with_resolution(type_id);
        let type_id = self
            .ctx
            .types
            .get_merged_intersection_origin(evaluated)
            .or_else(|| self.ctx.types.get_merged_intersection_origin(type_id))
            .unwrap_or(evaluated);
        let Some(members) =
            crate::query_boundaries::state::checking::intersection_members(self.ctx.types, type_id)
        else {
            return false;
        };

        let mut discriminants: rustc_hash::FxHashMap<tsz_common::Atom, Vec<TypeId>> =
            rustc_hash::FxHashMap::default();

        for member in members {
            let evaluated_member = self.evaluate_type_with_resolution(member);
            let Some(shape) = crate::query_boundaries::state::checking::object_shape(
                self.ctx.types,
                evaluated_member,
            ) else {
                continue;
            };

            for prop in &shape.properties {
                let prop_type = self.evaluate_type_with_resolution(prop.type_id);
                if !crate::query_boundaries::state::checking::is_unit_type(
                    self.ctx.types,
                    prop_type,
                ) {
                    continue;
                }

                let seen = discriminants.entry(prop.name).or_default();
                if seen.iter().any(|&other| {
                    !self.diagnostic_subtype_outcome(prop_type, other).related
                        && !self.diagnostic_subtype_outcome(other, prop_type).related
                }) {
                    return true;
                }
                if !seen.contains(&prop_type) {
                    seen.push(prop_type);
                }
            }
        }

        false
    }
}
