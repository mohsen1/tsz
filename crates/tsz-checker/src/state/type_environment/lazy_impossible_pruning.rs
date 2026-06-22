//! Speculative pruning of provably-impossible object-union members.
//!
//! Split out of `lazy.rs` (it shares that module's `CheckerState` impl and
//! `evaluate_type_with_resolution` entry point). These helpers remove union
//! members whose evaluated shape is uninhabitable — an intersection with
//! conflicting unit-literal discriminants, or an object with a required
//! property that is itself an impossible unit intersection — so a relation
//! never has to reason about a member that can hold no value.

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
                    && !self.object_member_has_impossible_required_property_with_env(member)
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
                if !crate::query_boundaries::state::checking::is_unit_type(
                    self.ctx.types,
                    prop.type_id,
                ) {
                    continue;
                }

                let seen = discriminants.entry(prop.name).or_default();
                if seen.iter().any(|&other| {
                    !self.diagnostic_subtype_outcome(prop.type_id, other).related
                        && !self.diagnostic_subtype_outcome(other, prop.type_id).related
                }) {
                    return true;
                }
                if !seen.contains(&prop.type_id) {
                    seen.push(prop.type_id);
                }
            }
        }

        false
    }

    fn object_member_has_impossible_required_property_with_env(&mut self, type_id: TypeId) -> bool {
        let evaluated_type = self.evaluate_type_with_resolution(type_id);
        let Some(shape) =
            crate::query_boundaries::state::checking::object_shape(self.ctx.types, evaluated_type)
        else {
            return false;
        };

        shape.properties.iter().any(|prop| {
            !prop.optional && self.type_is_impossible_unit_intersection_with_env(prop.type_id)
        })
    }

    fn type_is_impossible_unit_intersection_with_env(&mut self, type_id: TypeId) -> bool {
        let evaluated = self.evaluate_type_with_resolution(type_id);
        if evaluated == TypeId::NEVER {
            return true;
        }

        let Some(members) = crate::query_boundaries::state::checking::intersection_members(
            self.ctx.types,
            evaluated,
        ) else {
            return false;
        };

        let mut units = Vec::new();
        for member in members {
            let evaluated_member = self.evaluate_type_with_resolution(member);
            if !crate::query_boundaries::state::checking::is_unit_type(
                self.ctx.types,
                evaluated_member,
            ) {
                continue;
            }

            if units.iter().any(|&other| {
                !self
                    .diagnostic_subtype_outcome(evaluated_member, other)
                    .related
                    && !self
                        .diagnostic_subtype_outcome(other, evaluated_member)
                        .related
            }) {
                return true;
            }

            if !units.contains(&evaluated_member) {
                units.push(evaluated_member);
            }
        }

        false
    }
}
