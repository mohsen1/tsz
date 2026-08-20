//! `tsc`'s `hasExcessProperties` per-property union check, shared between the
//! relation VERDICT (the Lawyer's excess gate in `relations/compat.rs`) and the
//! failure EXPLANATION (`explain_union_target.rs`).
//!
//! For a FRESH object-literal source against a union target, `tsc` does more
//! than reject unknown keys: after `isKnownProperty` passes, every source
//! property must also relate to the union of that property's types across the
//! discriminant-reduced arms (`getTypeOfPropertyInTypes`). This is how
//! `tsc` rejects `both({ p: 1, q: 8, box: 5 })` against
//! `{ p: 1; q: 4 } | { p: 2; q: 8 } | Box` — the literal structurally
//! satisfies `Box`, but `p: 1` reduces the arms to
//! `{ p: 1; q: 4 } | Box` and `q: 8` fails against `4`
//! (`Types of property 'q' are incompatible.`).
//!
//! Oracle-pinned semantics (`typescript@7.0.2 --strict`):
//! - The reduction (`discriminateTypeByDiscriminableItems`) keeps arms that
//!   LACK a discriminator key, drops declaring arms whose value does not
//!   relate, REVERTS a pass in which no arm positively matched, and abandons
//!   narrowing entirely when a pass empties the candidate set. This mirrors
//!   the checker-side matcher in `excess_property_tail.rs` (#17801).
//! - The checked union for a property collects the types of the arms that
//!   DECLARE it; an arm lacking the property contributes nothing (the nested
//!   line for the witness reads `Type '8' is not assignable to type '4'.`,
//!   not `'4 | undefined'`).
//! - A property no reduced arm declares is left to the excess-key check.
//!
//! An arm that declares no such property still contributes its applicable
//! index-signature type (`getTypeOfPropertyInTypes`' `getIndexTypeOfType`
//! fallback: the number index for a numeric-literal name, else the string
//! index) — without this, a `{ [key: string]: V }` arm stops widening the
//! checked union and structurally-fine literals are falsely rejected.
//! Known residual (documented, not modeled): the REDUCTION consults declared
//! properties only, mirroring the checker's landed matcher — an
//! index-signature-only arm never qualifies a discriminator.

use crate::def::resolver::TypeResolver;
use crate::relations::subtype::SubtypeChecker;
use crate::type_queries::flow::is_unit_type;
use crate::types::TypeId;
use crate::visitor::{object_shape_id, object_with_index_shape_id, union_list_id};
use tsz_common::interner::Atom;

impl<R: TypeResolver> SubtypeChecker<'_, R> {
    /// Expand a union target's written members into leaf arms: lazy aliases
    /// are resolved and nested unions (an alias arm like `U` in `U | Box`)
    /// are flattened, so discriminant reduction and per-property collection
    /// see the same constituents `tsc`'s `checkTypes` does. Non-union members
    /// keep their written spelling; per-use helpers resolve them again.
    pub(crate) fn flattened_union_arms(&mut self, members: &[TypeId]) -> Vec<TypeId> {
        let mut arms = Vec::with_capacity(members.len());
        let mut stack: Vec<TypeId> = members.iter().rev().copied().collect();
        let mut expanded_lists = Vec::new();
        while let Some(member) = stack.pop() {
            let resolved = self.resolve_lazy_type(member);
            match union_list_id(self.interner, resolved) {
                Some(list_id) if !expanded_lists.contains(&list_id) => {
                    expanded_lists.push(list_id);
                    let nested = self.interner.type_list(list_id).to_vec();
                    stack.extend(nested.iter().rev().copied());
                }
                _ => arms.push(member),
            }
        }
        arms
    }

    /// The source's properties in DECLARATION order (`tsc` iterates
    /// `getPropertiesOfType`, so the first-declared failing property is the
    /// reported witness). The interned shape stores name-sorted properties;
    /// declaration order is restored from `PropertyInfo::declaration_order`
    /// (0 = unrecorded, kept after the recorded ones in stored order —
    /// `sort_by_key` is stable).
    fn shape_properties_in_declaration_order(
        &self,
        shape_id: crate::types::ObjectShapeId,
    ) -> Vec<(Atom, TypeId)> {
        let mut properties: Vec<(u32, Atom, TypeId)> = self
            .interner
            .object_shape(shape_id)
            .properties
            .iter()
            .map(|prop| {
                let order = if prop.declaration_order == 0 {
                    u32::MAX
                } else {
                    prop.declaration_order
                };
                (order, prop.name, prop.type_id)
            })
            .collect();
        properties.sort_by_key(|&(order, _, _)| order);
        properties
            .into_iter()
            .map(|(_, name, type_id)| (name, type_id))
            .collect()
    }

    /// `tsc`'s `discriminateTypeByDiscriminableItems` as `hasExcessProperties`
    /// consumes it: narrow `arms` by every unit-valued source property that
    /// qualifies as a union discriminant. Returns `Some(reduced)` only when
    /// the fold genuinely narrowed the set; `None` means "no reduction" (the
    /// caller checks against every arm), including the abandon case where a
    /// pass emptied the candidate set.
    ///
    /// Pass semantics, mirroring the checker's oracle-verified matcher
    /// (`excess_property_tail.rs`, #17801):
    /// - a discriminator qualifies when at least one arm declares the key,
    ///   some declared type is a unit, and the declared types are non-uniform;
    /// - arms LACKING the key survive the pass untouched;
    /// - declaring arms survive iff the source value relates to their declared
    ///   type (a written `undefined` also matches an optional slot);
    /// - a pass in which no arm positively matched is REVERTED (an ordinary
    ///   property-type mismatch for the relation to report, not a
    ///   discriminant);
    /// - a pass that empties the candidate set abandons narrowing entirely.
    fn reduce_union_arms_by_source_discriminants(
        &mut self,
        source_props: &[(Atom, TypeId)],
        arms: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        let mut active: Vec<TypeId> = arms.to_vec();
        let mut did_narrow = false;

        for &(name, source_value) in source_props {
            if !is_unit_type(self.interner, source_value) {
                continue;
            }
            // Qualification is decided against the FULL arm list, not the
            // narrowed set: whether a key is a discriminant is a property of
            // the union shape.
            let mut declared: Vec<(TypeId, TypeId)> = Vec::new();
            for &arm in arms {
                if let Some(arm_value) = self.discriminant_property_type(arm, name) {
                    declared.push((arm, arm_value));
                }
            }
            if declared.is_empty() {
                continue;
            }
            let any_unit = declared
                .iter()
                .any(|&(_, value)| is_unit_type(self.interner, value));
            if !any_unit {
                continue;
            }
            let first_value = declared[0].1;
            let non_uniform = declared.iter().any(|&(_, value)| value != first_value);
            if !non_uniform {
                continue;
            }

            let mut any_positive_match = false;
            let mut candidate: Vec<TypeId> = Vec::with_capacity(active.len());
            for &arm in &active {
                match self.discriminant_property_type(arm, name) {
                    None => candidate.push(arm),
                    Some(arm_value) => {
                        let related = self.check_subtype(source_value, arm_value).is_true()
                            || (source_value == TypeId::UNDEFINED
                                && self.discriminant_property_is_optional(arm, name));
                        if related {
                            any_positive_match = true;
                            candidate.push(arm);
                        }
                    }
                }
            }
            if candidate.is_empty() {
                return None;
            }
            if !any_positive_match {
                continue;
            }
            if candidate.len() < active.len() {
                did_narrow = true;
            }
            active = candidate;
        }

        if did_narrow && active.len() < arms.len() {
            Some(active)
        } else {
            None
        }
    }

    /// The first source property (declaration order) whose type fails the
    /// per-property union check of `tsc`'s `hasExcessProperties`, as
    /// `(name, source_property_type, checked_union_type)`. `None` when the
    /// source is not a fresh object literal, no arm is checkable, or every
    /// property relates — the excess-key check and the structural walk own
    /// those verdicts.
    pub(crate) fn fresh_union_per_property_failure(
        &mut self,
        resolved_source: TypeId,
        members: &[TypeId],
    ) -> Option<(Atom, TypeId, TypeId)> {
        let shape_id = object_shape_id(self.interner, resolved_source)
            .or_else(|| object_with_index_shape_id(self.interner, resolved_source))?;
        if !self.interner.object_shape(shape_id).is_fresh_literal() {
            return None;
        }
        let arms = self.flattened_union_arms(members);
        let source_props = self.shape_properties_in_declaration_order(shape_id);
        let check_arms: Vec<TypeId> =
            match self.reduce_union_arms_by_source_discriminants(&source_props, &arms) {
                Some(reduced) => reduced,
                // No discriminant narrowing: `tsc` checks against every
                // constituent (`filterPrimitivesIfContainsNonPrimitive` filters
                // primitives only next to the `object` keyword type); arms
                // without object-like keys cannot declare a property, so
                // restricting to object-like arms is behavior-preserving.
                None => arms
                    .iter()
                    .copied()
                    .filter(|&arm| {
                        let resolved = self.apparent_type_for_keys(arm);
                        object_shape_id(self.interner, resolved).is_some()
                            || object_with_index_shape_id(self.interner, resolved).is_some()
                    })
                    .collect(),
            };
        if check_arms.is_empty() {
            return None;
        }
        for (name, source_prop) in source_props {
            // `getTypeOfPropertyInTypes` over the reduced arms: the checked
            // union collects the DECLARING arms' property types; an arm
            // lacking the property contributes nothing (oracle: the nested
            // line reads `'4'`, not `'4 | undefined'`). The read type of an
            // OPTIONAL slot carries `| undefined` under strictNullChecks, so
            // a written `undefined` satisfies it.
            let mut declared_types = Vec::with_capacity(check_arms.len());
            for &arm in &check_arms {
                let Some(prop_type) = self.discriminant_property_type(arm, name) else {
                    // `getTypeOfPropertyInTypes`' index fallback: an arm
                    // without the declared property still contributes its
                    // applicable index-signature type.
                    if let Some(index_type) = self.index_signature_property_contribution(arm, name)
                    {
                        declared_types.push(index_type);
                    }
                    continue;
                };
                let prop_type = if self.strict_null_checks
                    && prop_type != TypeId::UNDEFINED
                    && self.discriminant_property_is_optional(arm, name)
                {
                    self.interner.union(vec![prop_type, TypeId::UNDEFINED])
                } else {
                    prop_type
                };
                declared_types.push(prop_type);
            }
            // A property unknown to every checked arm belongs to the
            // excess-key check, not this fold.
            if declared_types.is_empty() {
                continue;
            }
            let target_prop = if let [single] = declared_types.as_slice() {
                *single
            } else {
                self.interner.union(declared_types)
            };
            if self.check_subtype(source_prop, target_prop).is_true() {
                continue;
            }
            return Some((name, source_prop, target_prop));
        }
        None
    }

    /// `getTypeOfPropertyInTypes`' index-signature fallback: the type an arm
    /// contributes for `name` through its index signatures when it declares
    /// no such property — the number index for a numeric-literal name, else
    /// the string index. Intersection members are consulted like
    /// [`Self::discriminant_property_type`] does for declared properties.
    fn index_signature_property_contribution(&mut self, arm: TypeId, name: Atom) -> Option<TypeId> {
        use crate::type_queries::data::get_intersection_members;

        let resolved = self.apparent_type_for_keys(arm);
        if let Some(contribution) = self.shape_index_contribution(resolved, name) {
            return Some(contribution);
        }
        if let Some(members) = get_intersection_members(self.interner, resolved) {
            for member in members {
                let resolved_member = self.apparent_type_for_keys(member);
                if let Some(contribution) = self.shape_index_contribution(resolved_member, name) {
                    return Some(contribution);
                }
            }
        }
        None
    }

    /// The index-signature value type `resolved` exposes for `name`, if any.
    fn shape_index_contribution(&self, resolved: TypeId, name: Atom) -> Option<TypeId> {
        let shape_id = object_with_index_shape_id(self.interner, resolved)?;
        let shape = self.interner.object_shape(shape_id);
        let name_str = self.interner.resolve_atom(name);
        if crate::utils::is_numeric_literal_name(name_str.as_ref())
            && let Some(number_index) = &shape.number_index
        {
            return Some(number_index.value_type);
        }
        shape.string_index.as_ref().map(|index| index.value_type)
    }
}
