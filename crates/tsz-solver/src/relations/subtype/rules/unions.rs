//! Union and intersection type subtype checking.
//!
//! This module handles subtyping for TypeScript's composite types:
//! - Union types (A | B | C) - source must be subtype of at least one member
//! - Intersection types (A & B & C) - source must be subtype of all members
//! - Distributivity rules between unions and intersections
//! - Type parameter compatibility in union/intersection contexts

use crate::construction::TypeDatabase;
use crate::type_queries::data::get_object_shape_id;
use crate::types::{
    MappedModifier, MappedTypeId, ObjectShapeId, PropertyInfo, TupleElement, TypeId, TypeParamInfo,
};
use crate::visitor::enum_components;
use crate::visitor::{
    application_id, array_element_type, index_access_parts, is_identity_comparable_type,
    is_literal_type, keyof_inner_type, lazy_def_id, mapped_type_id, readonly_inner_type,
    tuple_list_id, type_param_info, union_list_id,
};
use tsz_common::interner::Atom;

use super::super::{SubtypeChecker, SubtypeFailureReason, SubtypeResult, TypeResolver};

/// Maximum number of discriminant value combinations before giving up.
/// This matches TypeScript's limit to prevent exponential blowup.
const MAX_DISCRIMINANT_COMBINATIONS: usize = 25;

/// Maximum source properties considered for discriminated-union splitting.
const MAX_PROPERTIES_FOR_DISCRIMINATED: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscriminatedObjectSizeState {
    Continue,
    TooManyProperties,
}

impl DiscriminatedObjectSizeState {
    const fn for_property_count(property_count: usize) -> Self {
        if property_count > MAX_PROPERTIES_FOR_DISCRIMINATED {
            Self::TooManyProperties
        } else {
            Self::Continue
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscriminantCombinationState {
    Continue,
    LimitExceeded,
}

impl DiscriminantCombinationState {
    const fn for_count(count: usize) -> Self {
        if count > MAX_DISCRIMINANT_COMBINATIONS {
            Self::LimitExceeded
        } else {
            Self::Continue
        }
    }
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check if a type parameter is a subtype of a target type.
    ///
    /// Handles both type parameter vs type parameter and type parameter vs concrete type.
    /// Implements TypeScript's soundness rules for type parameter compatibility.
    ///
    /// ## TypeScript Soundness Rules:
    /// - Same type parameter (shared declaration identity, or a legacy shared
    ///   name not proven distinct) → reflexive (always compatible)
    /// - Different type parameters → check constraint transitivity
    /// - Type parameter vs concrete → constraint must be subtype of concrete
    /// - Unconstrained type parameter → acts like `unknown` (top type)
    pub(crate) fn check_type_parameter_subtype(
        &mut self,
        s_info: &TypeParamInfo,
        target: TypeId,
    ) -> SubtypeResult {
        // Type parameter vs type parameter
        if let Some(t_info) = type_param_info(self.interner, target) {
            // A shared *name* is only a proxy for legacy unstamped parameters.
            // Declaration-stamped parameters carry an authoritative binder
            // identity, so an inner generic and a captured outer parameter with
            // the same spelling must remain unrelated.
            // When two legacy parameters carry constraints that are provably incompatible
            // (neither assignable to the other), the parameters are definitely
            // distinct — `tsc` treats them as unrelated and reports the failure
            // (TS2719, "two different types with this name exist"). Related
            // but non-identical constraints are not enough evidence: without a
            // declaration handle they may still be the same parameter viewed
            // through a primitive/object or alias relation. Only the reflexive
            // same-parameter case may short-circuit to `True`; the distinct
            // case must fall through to constraint transitivity so a genuine
            // mismatch is reported instead of silently accepted.
            if s_info.is_same_binder(t_info)
                && !self.same_named_type_params_are_distinct(s_info, &t_info)
            {
                return SubtypeResult::True;
            }
            if let Some(s_constraint) = s_info.constraint {
                if s_constraint == target {
                    return SubtypeResult::True;
                }
                if self.check_subtype(s_constraint, target).is_true() {
                    return SubtypeResult::True;
                }
            }
            return SubtypeResult::False;
        }

        // Type parameter vs concrete type
        if let Some(constraint) = s_info.constraint {
            let result = self.check_subtype(constraint, target);
            if result.is_true() {
                return result;
            }
        } else {
            // Unconstrained type parameter: use unknown as base constraint.
            let result = self.check_subtype(TypeId::UNKNOWN, target);
            if result.is_true() {
                return result;
            }
        }

        // Homomorphic mapped type target check:
        // T is assignable to { [K in keyof T]+?: T[K] } (Partial<T>)
        // T is assignable to { readonly [K in keyof T]: T[K] } (Readonly<T>)
        // T is assignable to { [K in keyof T]: T[K] } (identity mapped type)
        // T is assignable to { [P in keyof T]: T[keyof T] } (widened template)
        //
        // This implements tsc's typeRelatedToMappedType: when the target is a
        // generic homomorphic mapped type whose source is the same type parameter
        // (or a supertype), and the mapped type doesn't remove optionality,
        // the source type parameter is assignable.
        if let Some(mapped_id) = mapped_type_id(self.interner, target)
            && self.is_assignable_to_homomorphic_mapped(*s_info, mapped_id)
        {
            return SubtypeResult::True;
        }

        // Also handle Application targets that resolve to mapped types.
        // e.g., MyMap<U> where type MyMap<T> = { [P in keyof T]: T[keyof T] }
        // The Application expands to a Mapped type which we can then check.
        if let Some(app_id) = application_id(self.interner, target)
            && let Some(expanded) = self.try_expand_application_type(target, app_id)
            && let Some(mapped_id) = mapped_type_id(self.interner, expanded)
            && self.is_assignable_to_homomorphic_mapped(*s_info, mapped_id)
        {
            return SubtypeResult::True;
        }

        // Variadic tuple identity: T is assignable to [...T] (and readonly [...T])
        // when T is a type parameter. tsc treats [...T] as structurally equivalent to T.
        // This handles: T <: [...T], T <: readonly [...T]
        {
            // Unwrap readonly wrapper if present
            let target_is_readonly = readonly_inner_type(self.interner, target).is_some();
            let inner_target = readonly_inner_type(self.interner, target).unwrap_or(target);
            if let Some(t_list) = tuple_list_id(self.interner, inner_target) {
                let t_elems = self.interner.tuple_list(t_list);
                if t_elems.len() == 1
                    && t_elems[0].rest
                    && type_param_info(self.interner, t_elems[0].type_id)
                        .is_some_and(|inner_info| s_info.is_same_binder(inner_info))
                    && self.type_param_constraint_allows_spread_identity(
                        s_info.constraint,
                        target_is_readonly,
                    )
                {
                    return SubtypeResult::True;
                }
            }
        }

        SubtypeResult::False
    }

    /// Decide whether two same-named type parameters are *provably distinct*
    /// declarations rather than the same parameter seen twice.
    ///
    /// A pair of authoritative declaration origins provides authoritative
    /// declaration identity. For legacy unstamped parameters, the conservative
    /// signal that two same-named parameters are genuinely different is that
    /// both carry constraints which are mutually non-assignable: the same
    /// parameter always presents the same constraint (interning to the same
    /// `TypeId`, or — when the constraint is reached
    /// through different-but-related representations — at least a one-way
    /// assignable one), so a constraint pair with no assignable direction can
    /// only come from two different declarations. Returning `true` here
    /// suppresses the name-based reflexive shortcut so the relation falls
    /// through to constraint transitivity and a real mismatch is reported,
    /// mirroring `tsc`'s handling of distinct identically-named type parameters.
    ///
    /// Unconstrained (or one-sided-unconstrained) same-named legacy pairs return
    /// `false`: they intern to one `TypeId` and never reach this path, or cannot
    /// be told apart without a declaration stamp, so the historical reflexive
    /// behaviour is preserved.
    fn same_named_type_params_are_distinct(
        &mut self,
        s_info: &TypeParamInfo,
        t_info: &TypeParamInfo,
    ) -> bool {
        // A declaration stamp is authoritative identity. Distinct owners stay
        // unrelated even when their surface spelling, constraint, and default
        // are byte-for-byte identical (for example a method-level JSDoc
        // `@template T` shadowing a class-level `@template T`). Generic
        // signature alpha-renaming registers an explicit equivalence before
        // reaching this rule, so declaration identity does not reject valid
        // `<T>(x: T) => T` / `<U>(x: U) => U` comparisons.
        if s_info.origin.is_decl_scoped() && t_info.origin.is_decl_scoped() {
            return s_info.origin != t_info.origin;
        }

        let (Some(s_constraint), Some(t_constraint)) = (s_info.constraint, t_info.constraint)
        else {
            return false;
        };
        if s_constraint == t_constraint {
            return false;
        }
        let source_extends_target = self.check_subtype(s_constraint, t_constraint).is_true();
        let target_extends_source = self.check_subtype(t_constraint, s_constraint).is_true();
        !source_extends_target && !target_extends_source
    }

    fn type_param_constraint_allows_spread_identity(
        &self,
        constraint: Option<TypeId>,
        target_is_readonly: bool,
    ) -> bool {
        let Some(constraint) = constraint else {
            return false;
        };
        let constraint_is_readonly = readonly_inner_type(self.interner, constraint).is_some();
        let array_like = if let Some(inner) = readonly_inner_type(self.interner, constraint) {
            array_element_type(self.interner, inner).is_some()
                || tuple_list_id(self.interner, inner).is_some()
        } else {
            array_element_type(self.interner, constraint).is_some()
                || tuple_list_id(self.interner, constraint).is_some()
        };

        array_like && (target_is_readonly || !constraint_is_readonly)
    }

    /// Check if a type (identified by name and optional constraint) is assignable
    /// to a homomorphic mapped type.
    ///
    /// A type T is assignable to `{ [K in keyof S]: S[K] }` (with optional modifiers)
    /// when T is related to S and the mapped type doesn't remove optionality (-?).
    ///
    /// This covers:
    /// - `T <: Partial<T>` (adds optional) — YES, T satisfies optional requirements
    /// - `T <: Readonly<T>` (adds readonly) — YES, readonly doesn't affect assignment
    /// - `T <: { [K in keyof T]: T[K] }` (identity) — YES, identity preserves shape
    /// - `U extends T => U <: Partial<T>` (constraint-based)
    ///
    /// Does NOT cover:
    /// - `T <: Required<T>` — NO, T may have optional properties that Required demands
    fn is_assignable_to_homomorphic_mapped(
        &mut self,
        source: TypeParamInfo,
        mapped_id: MappedTypeId,
    ) -> bool {
        let mapped = self.interner.mapped_type(mapped_id);

        // If there's an as-clause, it must be a filtering conditional
        // (produces only P or never) for this optimization to apply.
        if let Some(name_type) = mapped.name_type
            && !super::generics::is_filtering_name_type(self.interner, name_type, &mapped)
        {
            return false;
        }

        // Mapped types that REMOVE optionality (-?) like Required<T> are NARROWER
        // than the source type parameter. T may have optional properties that
        // Required<T> demands be present, so T → Required<T> fails.
        if mapped.optional_modifier == Some(MappedModifier::Remove) {
            return false;
        }

        // Determine the "picked-from" object `S` and ensure the mapped type's key
        // set is within `keyof S`. Two shapes are accepted:
        //   * the constraint is exactly `keyof S` (the classic homomorphic case), or
        //   * the constraint is a *subset* of `keyof S` and the template is the
        //     identity `S[P]` — the `Pick<S, K>` shape `{ [P in K]: S[P] }`, where
        //     every demanded key `P ∈ K` is a key of `S`, so a `T` carrying `S`'s
        //     shape supplies each demanded property with a matching type. tsc
        //     accepts `T <: Pick<T, SomeKeys<T>>` for exactly this reason.
        // `is_identity_template` is set true when the constraint-source is derived
        // from an identity `S[P]` template, so the general `S[K] <: Template` check
        // below can be skipped.
        let (constraint_source, is_identity_template) =
            match keyof_inner_type(self.interner, mapped.constraint) {
                Some(source) => {
                    // A homomorphic *identity* mapped source (`keyof Id<S>`,
                    // where `Id<S> = { [P in keyof S]: S[P] }`) is interchangeable
                    // with `S` itself, because `keyof Id<S> = keyof S` and
                    // `Id<S>[K] = S[K]`. Peeling it collapses nested
                    // `Prettify<Prettify<T>>` / `Id<Id<T>>` wrappers to the
                    // single-level case so a source type parameter `T` is
                    // recognised as assignable.
                    let source = self.peel_homomorphic_identity_mapped_source(source);
                    // Classic case: detect an identity `S[P]` template so the
                    // general check can be skipped. The template's indexed object
                    // is peeled the same way so a template written over a nested
                    // identity wrapper (`Id<S>[P]`) still matches the peeled
                    // source `S`.
                    let identity = match index_access_parts(self.interner, mapped.template) {
                        Some((template_obj, template_idx)) => {
                            let name_matches = type_param_info(self.interner, template_idx)
                                .is_some_and(|idx_param| {
                                    mapped.type_param.is_same_binder(idx_param)
                                });
                            name_matches
                                && self.peel_homomorphic_identity_mapped_source(template_obj)
                                    == source
                        }
                        None => false,
                    };
                    (source, identity)
                }
                None => {
                    // Subset (`Pick`) shape: the template must be the identity
                    // indexed access `S[P]` with `P` the iteration parameter, and
                    // the picked key set must be a subset of `keyof S` (`keyof S`
                    // covers the constraint). Matching this shape proves the
                    // template is identity, so `is_identity_template` is true.
                    let Some((template_obj, template_idx)) =
                        index_access_parts(self.interner, mapped.template)
                    else {
                        return false;
                    };
                    let Some(idx_param) = type_param_info(self.interner, template_idx) else {
                        return false;
                    };
                    if !mapped.type_param.is_same_binder(idx_param) {
                        return false;
                    }
                    let full_key_set = self.interner.keyof(template_obj);
                    if !self.mapped_key_constraint_covers(full_key_set, mapped.constraint) {
                        return false;
                    }
                    // Peel a nested identity wrapper here too, mirroring the
                    // classic-`keyof S` arm above, so `Pick<Id<S>, K>` recognises
                    // the underlying source.
                    (
                        self.peel_homomorphic_identity_mapped_source(template_obj),
                        true,
                    )
                }
            };

        if !is_identity_template {
            // General case: construct S[K] (source value type at key K) and check
            // if S[K] <: Template. K is the iteration parameter with constraint keyof(S).
            //
            // This handles templates like T[keyof T], T[P] | undefined, etc.
            // The visit_index_access subtype rule handles S[I] <: T[J] by checking
            // S <: T AND I <: J, and type parameter subtype checking handles
            // K <: keyof S via K's constraint.
            let k_type_id = self.interner.type_param(TypeParamInfo {
                name: mapped.type_param.name,
                constraint: Some(mapped.constraint),
                default: None,
                is_const: false,
                origin: mapped.type_param.origin,
            });
            let source_value_type = self.interner.index_access(constraint_source, k_type_id);
            if !self
                .check_subtype(source_value_type, mapped.template)
                .is_true()
            {
                return false;
            }
        }

        // Source type parameter must be related to the mapped type's source:
        // - Same name: T <: { [K in keyof T]: T[K] } (direct match)
        // - Constraint-based: U extends T => U <: Partial<T>
        if let Some(source_param) = type_param_info(self.interner, constraint_source)
            && source.is_same_binder(source_param)
        {
            return true;
        }

        // Check if source constraint is assignable to the mapped type source
        if let Some(constraint) = source.constraint {
            return self.check_subtype(constraint, constraint_source).is_true();
        }

        false
    }

    /// Check subtype with optional method bivariance.
    ///
    /// When `allow_bivariant` is true, temporarily disables strict function types
    /// to allow bivariant parameter checking. This is used for method compatibility
    /// where TypeScript allows bivariance even in strict mode.
    ///
    /// ## Variance Modes:
    /// - **Contravariant (strict)**: `target <: source` - Function parameters in strict mode
    /// - **Bivariant (legacy)**: `target <: source OR source <: target` - Methods, legacy functions
    ///
    /// ## Example:
    /// ```typescript
    /// // Bivariant methods allow unsound but convenient assignments
    /// interface Animal { name: string; }
    /// interface Dog extends Animal { bark(): void; }
    /// class AnimalKeeper {
    ///   feed(animal: Animal) { ... }  // Contravariant parameter
    /// }
    /// class DogKeeper {
    ///   feed(dog: Dog) { ... }  // More specific
    /// }
    /// // DogKeeper.feed is assignable to AnimalKeeper.feed (bivariant)
    /// ```
    pub(crate) fn check_subtype_with_method_variance(
        &mut self,
        source: TypeId,
        target: TypeId,
        allow_bivariant: bool,
    ) -> SubtypeResult {
        // In identity mode (TS2403), never use method bivariance.
        // tsc's isTypeIdenticalTo uses the identity relation which is strictly
        // bidirectional structural equality without any bivariance.
        if allow_bivariant && !self.identity_cycle_check && !self.disable_method_bivariance {
            // Method bivariance: temporarily disable strict_function_types
            // so check_parameter_compatibility uses bivariant parameter checks.
            // This only affects parameter variance, NOT return type variance.
            let prev = self.strict_function_types;
            self.method_bivariance_strict_stack.push(prev);
            self.strict_function_types = false;
            let result = self.check_subtype(source, target);
            self.strict_function_types = prev;
            self.method_bivariance_strict_stack.pop();
            return result;
        }
        self.check_subtype(source, target)
    }

    /// Explain failure with method bivariance rules.
    pub(crate) fn explain_failure_with_method_variance(
        &mut self,
        source: TypeId,
        target: TypeId,
        allow_bivariant: bool,
    ) -> Option<SubtypeFailureReason> {
        if allow_bivariant && !self.identity_cycle_check && !self.disable_method_bivariance {
            let prev = self.strict_function_types;
            self.strict_function_types = false;
            let result = self.explain_failure(source, target);
            self.strict_function_types = prev;
            return result;
        }
        self.explain_failure(source, target)
    }

    /// Check if source is related to a discriminated union type.
    ///
    /// Implements TypeScript's `typeRelatedToDiscriminatedType` algorithm.
    /// When a source object has properties that act as discriminants for the
    /// target union, we split the check: for each possible discriminant value
    /// in the source, check if a narrowed source is assignable to a matching
    /// target member.
    pub(crate) fn type_related_to_discriminated_type(
        &mut self,
        source: TypeId,
        target_members: &[TypeId],
    ) -> SubtypeResult {
        // Get source object shape — must be an object type
        let source_shape_id = match get_object_shape_id(self.interner, source) {
            Some(id) => id,
            None => return SubtypeResult::False,
        };
        let source_shape = self.interner.object_shape(source_shape_id);

        // Performance guard: skip discriminated union narrowing for large object types.
        // DOM interfaces like HTMLElement have hundreds of properties; creating narrowed
        // copies (clone + sort + hash + intern) for each discriminant combination is
        // prohibitively expensive and never matches real discriminated union patterns.
        match DiscriminatedObjectSizeState::for_property_count(source_shape.properties.len()) {
            DiscriminatedObjectSizeState::Continue => {}
            DiscriminatedObjectSizeState::TooManyProperties => return SubtypeResult::False,
        }

        let target_members_for_discriminants: Vec<TypeId> = target_members
            .iter()
            .map(|&member| {
                let evaluated = self.evaluate_type(member);
                if get_object_shape_id(self.interner, evaluated).is_some() {
                    evaluated
                } else {
                    member
                }
            })
            .collect();

        // Find discriminant properties in the source that discriminate target
        let disc_props = find_discriminant_properties(
            self.interner,
            self.resolver,
            &source_shape.properties,
            &target_members_for_discriminants,
        );
        if disc_props.is_empty() {
            return SubtypeResult::False;
        }

        // For each discriminant property, collect source values and matching targets.
        // Start with all target members, then intersect across discriminants.
        let mut candidate_targets: Option<Vec<bool>> = None;

        for &(prop_name, source_prop_type) in &disc_props {
            let source_values =
                get_discriminant_values(self.interner, self.resolver, source_prop_type);
            match DiscriminantCombinationState::for_count(source_values.len()) {
                DiscriminantCombinationState::Continue => {}
                DiscriminantCombinationState::LimitExceeded => return SubtypeResult::False,
            }

            // For this discriminant, track which target members are reachable
            let mut reachable = vec![false; target_members.len()];

            for &value in &source_values {
                let mut value_has_match = false;
                for (i, &target_member) in target_members_for_discriminants.iter().enumerate() {
                    let t_prop =
                        get_property_type_of_object(self.interner, target_member, prop_name);
                    match t_prop {
                        Some(t_prop_type) if self.check_subtype(value, t_prop_type).is_true() => {
                            reachable[i] = true;
                            value_has_match = true;
                        }
                        None => {
                            // Target member doesn't have this discriminant property.
                            // It's reachable for any discriminant value since the
                            // absence means it doesn't discriminate on this property.
                            reachable[i] = true;
                            value_has_match = true;
                        }
                        _ => {}
                    }
                }
                if !value_has_match {
                    return SubtypeResult::False;
                }
            }

            // Intersect with previous discriminant results
            match &mut candidate_targets {
                Some(prev) => {
                    for (p, r) in prev.iter_mut().zip(reachable.iter()) {
                        *p = *p && *r;
                    }
                }
                None => candidate_targets = Some(reachable),
            }
        }

        let candidates = match candidate_targets {
            Some(c) => c,
            None => return SubtypeResult::False,
        };

        // Verify: for each combination of discriminant values across ALL
        // discriminant properties, narrow the source by all of them and check
        // that the fully-narrowed source is assignable to at least one matching
        // target member. This is critical for cases like:
        //   source: { kind: "a"|"b", value: number|undefined }
        //   target: { kind: "a"|"b", value: number } | { kind: "a", value: undefined } | ...
        // Narrowing by only `kind` leaves `value` too wide; we must narrow both.
        let disc_values: Vec<smallvec::SmallVec<[TypeId; 4]>> = disc_props
            .iter()
            .map(|&(_, source_prop_type)| {
                get_discriminant_values(self.interner, self.resolver, source_prop_type)
            })
            .collect();

        // Check total combinations don't exceed limit
        let total_combinations: usize = disc_values.iter().map(|v| v.len()).product();
        match DiscriminantCombinationState::for_count(total_combinations) {
            DiscriminantCombinationState::Continue => {}
            DiscriminantCombinationState::LimitExceeded => return SubtypeResult::False,
        }

        // Iterate over all combinations using index-based enumeration
        let mut combo_indices = vec![0usize; disc_values.len()];
        loop {
            // Build the narrowed source by applying ALL discriminant narrowings
            let narrowed = narrow_object_properties(
                self.interner,
                source_shape_id,
                &disc_props,
                &disc_values,
                &combo_indices,
            );

            let mut found = false;
            for (i, &target_member) in target_members.iter().enumerate() {
                if !candidates[i] {
                    continue;
                }
                let normalized_target = target_members_for_discriminants[i];
                if self.check_subtype(narrowed, target_member).is_true()
                    || (normalized_target != target_member
                        && self.check_subtype(narrowed, normalized_target).is_true())
                {
                    found = true;
                    break;
                }
            }
            if !found {
                return SubtypeResult::False;
            }

            // Advance to next combination (odometer-style)
            let mut carry = true;
            for d in (0..disc_values.len()).rev() {
                if carry {
                    combo_indices[d] += 1;
                    if combo_indices[d] >= disc_values[d].len() {
                        combo_indices[d] = 0;
                    } else {
                        carry = false;
                    }
                }
            }
            if carry {
                break; // All combinations exhausted
            }
        }

        SubtypeResult::True
    }

    /// Tuple variant of `type_related_to_discriminated_type`.
    ///
    /// TypeScript accepts a tuple whose element contains a finite literal union
    /// when each literal arm is accepted by a matching target tuple union member:
    /// `["a" | "b", 1]` is assignable to `["a", number] | ["b", number]`.
    pub(crate) fn type_related_to_discriminated_tuple_type(
        &mut self,
        source: TypeId,
        target_members: &[TypeId],
    ) -> SubtypeResult {
        let Some(source_list_id) = tuple_list_id(self.interner, source) else {
            return SubtypeResult::False;
        };
        let source_elems = self.interner.tuple_list(source_list_id);
        if source_elems.is_empty() || source_elems.iter().any(|elem| elem.rest) {
            return SubtypeResult::False;
        }

        let mut target_tuples = Vec::with_capacity(target_members.len());
        for &member in target_members {
            let Some(list_id) = tuple_list_id(self.interner, member) else {
                return SubtypeResult::False;
            };
            let elems = self.interner.tuple_list(list_id);
            if elems.len() != source_elems.len() || elems.iter().any(|elem| elem.rest) {
                return SubtypeResult::False;
            }
            target_tuples.push(elems);
        }

        let mut disc_positions = Vec::new();
        let mut disc_values = Vec::new();
        for (index, source_elem) in source_elems.iter().enumerate() {
            let values = get_discriminant_values(self.interner, self.resolver, source_elem.type_id);
            if values.len() <= 1 {
                continue;
            }

            let mut has_unit = false;
            let mut seen_types = Vec::new();
            for target_tuple in &target_tuples {
                let target_type = target_tuple[index].type_id;
                if !seen_types.contains(&target_type) {
                    seen_types.push(target_type);
                }
                for &constituent in &get_type_constituents(self.interner, target_type) {
                    if is_identity_comparable_type(self.interner, constituent)
                        || is_literal_type(self.interner, constituent)
                    {
                        has_unit = true;
                    }
                }
            }

            if has_unit && seen_types.len() > 1 {
                disc_positions.push(index);
                disc_values.push(values);
            }
        }

        if disc_positions.is_empty() {
            return SubtypeResult::False;
        }

        let total_combinations: usize = disc_values.iter().map(|values| values.len()).product();
        match DiscriminantCombinationState::for_count(total_combinations) {
            DiscriminantCombinationState::Continue => {}
            DiscriminantCombinationState::LimitExceeded => return SubtypeResult::False,
        }

        let mut combo_indices = vec![0usize; disc_values.len()];
        loop {
            let narrowed = narrow_tuple_elements(
                self.interner,
                &source_elems,
                &disc_positions,
                &disc_values,
                &combo_indices,
            );

            let mut found = false;
            for &target_member in target_members {
                if self.check_subtype(narrowed, target_member).is_true() {
                    found = true;
                    break;
                }
            }
            if !found {
                return SubtypeResult::False;
            }

            let mut carry = true;
            for d in (0..disc_values.len()).rev() {
                if carry {
                    combo_indices[d] += 1;
                    if combo_indices[d] >= disc_values[d].len() {
                        combo_indices[d] = 0;
                    } else {
                        carry = false;
                    }
                }
            }
            if carry {
                break;
            }
        }

        SubtypeResult::True
    }
}

// ── Helper functions for discriminated union checking ──

/// Get the constituents of a type. If it's a union, return all members.
/// Otherwise return a singleton. Uses `SmallVec` to avoid heap allocation
/// for the common singleton case.
fn get_type_constituents(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> smallvec::SmallVec<[TypeId; 4]> {
    if type_id.is_intrinsic() {
        return smallvec::smallvec![type_id];
    }
    if let Some(list_id) = union_list_id(db, type_id) {
        let members = db.type_list(list_id);
        members.iter().copied().collect()
    } else {
        smallvec::smallvec![type_id]
    }
}

/// Get discriminant values from a source property type.
///
/// This expands `boolean` to `true | false` to enable discriminated union matching,
/// since TypeScript treats `boolean` as equivalent to `true | false` for this purpose.
/// Type parameters are resolved to their constraints before extracting values, so that
/// `T extends "a" | "b"` yields `["a", "b"]` rather than `[T]`. Without this, objects
/// like `{ k: T }` would fail the per-value discriminant check against `{ k: "a" } | { k: "b" }`.
/// A whole-enum type yields its nominal member types (`Mode` → `[Mode.A, Mode.B]`),
/// matching tsc's model of an enum type as the union of its member types.
/// Union constituents are expanded recursively (e.g. `T | E.A`, `Mode | undefined`).
fn get_discriminant_values<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> smallvec::SmallVec<[TypeId; 4]> {
    // Special case: boolean is equivalent to true | false for discriminated union matching
    if type_id == TypeId::BOOLEAN {
        return smallvec::smallvec![TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN_FALSE];
    }

    // Resolve `Lazy(DefId)` first: a source property typed by a semantic
    // reference (e.g. `m: Mode` stored as `Lazy(Mode)`) is opaque to the
    // enum/union extractors below and would otherwise surface as one opaque
    // "value" that matches no member-typed arm (#17643).
    if let Some(def_id) = lazy_def_id(db, type_id)
        && let Some(resolved) = resolver.resolve_lazy(def_id, db)
        && resolved != type_id
    {
        return get_discriminant_values(db, resolver, resolved);
    }

    // Resolve type parameters to their constraints for discriminant matching.
    // e.g., T extends "a" | "b" → use "a" | "b" as discriminant values.
    if let Some(info) = type_param_info(db, type_id)
        && let Some(constraint) = info.constraint
    {
        return get_discriminant_values(db, resolver, constraint);
    }

    // Expand a whole-enum type to one NOMINAL value per member literal.
    // tsc models an enum type as the union of its member types, so the
    // discriminant constituents of `{ m: Mode }` are `Mode.A | Mode.B` and
    // the narrowed source keeps matching member-typed arms like
    // `{ m: Mode.A }`. Each structural member literal is wrapped back into
    // the source enum's nominal domain (`Enum(parent, lit_i)`, the same
    // shape control-flow narrowing mints): bare literals would erase the
    // source enum's identity and wrongly match ANOTHER enum's member-typed
    // arms. An enum MEMBER type is already a single unit value.
    if let Some((def_id, structural_type)) = enum_components(db, type_id) {
        if resolver.get_enum_parent_def_id(def_id).is_some() {
            return smallvec::smallvec![type_id];
        }
        let literals = get_type_constituents(db, structural_type);
        if literals.len() <= 1 {
            return smallvec::smallvec![type_id];
        }
        return literals
            .iter()
            .map(|&lit| db.enum_type(def_id, lit))
            .collect();
    }

    let constituents = get_type_constituents(db, type_id);

    // Expand each union constituent recursively: type parameters resolve to
    // their constraint values, whole enums to their member types, and
    // `boolean` to `true | false` — tsc's union constituents are already
    // expanded at this granularity (e.g. `Mode | undefined` is
    // `Mode.A | Mode.B | undefined`). Constituents with no expansion
    // return themselves, so plain unions are unchanged.
    if constituents.len() > 1 {
        let mut result = smallvec::SmallVec::new();
        for &c in &constituents {
            result.extend(get_discriminant_values(db, resolver, c));
        }
        return result;
    }

    constituents
}

/// Get a property type from an object-like type by atom name.
/// For optional properties, includes `undefined` in the type.
fn get_property_type_of_object(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    prop_name: Atom,
) -> Option<TypeId> {
    let shape_id = get_object_shape_id(db, type_id)?;
    let shape = db.object_shape(shape_id);
    let prop = crate::utils::lookup_property(db, &shape.properties, Some(shape_id), prop_name)?;
    if prop.optional {
        // Optional properties accept undefined
        Some(db.union2(prop.type_id, TypeId::UNDEFINED))
    } else {
        Some(prop.type_id)
    }
}

/// Find properties in the source that discriminate the target union.
///
/// A discriminant property is one where:
/// - It exists in every target union member (as an object property)
/// - At least one target member has a unit/literal type for it
/// - The property types differ across members
fn find_discriminant_properties<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    source_props: &[PropertyInfo],
    target_members: &[TypeId],
) -> Vec<(Atom, TypeId)> {
    let mut result = Vec::new();

    for prop in source_props {
        if is_discriminant_for_union(db, resolver, prop.name, target_members) {
            result.push((prop.name, prop.type_id));
        }
    }

    result
}

/// Check if a property name is a discriminant for a target union.
///
/// Resolves `Lazy(DefId)` property types before checking identity-comparability,
/// since enum member types like `E.A` may still be stored as `Lazy(DefId)` in
/// object shapes even after top-level evaluation.
fn is_discriminant_for_union<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    prop_name: Atom,
    target_members: &[TypeId],
) -> bool {
    let mut has_unit = false;
    let mut seen_types: Vec<TypeId> = Vec::new();

    for &member in target_members {
        let shape_id = match get_object_shape_id(db, member) {
            Some(id) => id,
            None => return false, // All members must be object types
        };
        let shape = db.object_shape(shape_id);
        let prop =
            match crate::utils::lookup_property(db, &shape.properties, Some(shape_id), prop_name) {
                Some(p) => p,
                None => {
                    // Property missing — still valid if optional in source
                    // For discriminant purposes, treat missing as "undefined"
                    continue;
                }
            };

        let prop_type = prop.type_id;
        // A member's property type marks the property as discriminant-capable
        // only when the WHOLE type is unit-like: `boolean`, a single unit
        // type, or a union whose EVERY constituent is a unit type (tsc's
        // `isLiteralType`, feeding `CheckFlags.HasLiteralType`). A mixed
        // union like `string | undefined` must NOT qualify: crediting its one
        // unit constituent would let a wide source (`Output | undefined`
        // instantiated with a union) narrow per-constituent into different
        // same-base arms and wrongly accept (#17643).
        // Resolve `Lazy` constituents first — enum member property types may
        // still be `Lazy(DefId)` in the object shape after top-level union
        // evaluation.
        // `boolean` counts as unit-like wherever it appears: tsc models it as
        // the union `true | false`, so both a bare `boolean` property and a
        // `boolean | undefined` union satisfy the every-constituent rule.
        let whole_type_is_unit = get_type_constituents(db, prop_type)
            .iter()
            .all(|&constituent| {
                let resolved = if let Some(def_id) = lazy_def_id(db, constituent) {
                    resolver.resolve_lazy(def_id, db).unwrap_or(constituent)
                } else {
                    constituent
                };
                resolved == TypeId::BOOLEAN
                    || is_identity_comparable_type(db, resolved)
                    || is_literal_type(db, resolved)
            });
        if whole_type_is_unit {
            has_unit = true;
        }

        if !seen_types.contains(&prop_type) {
            seen_types.push(prop_type);
        }
    }

    // Must have at least one unit type and different types across members
    has_unit && seen_types.len() > 1
}

/// Create a new object type by narrowing MULTIPLE properties simultaneously.
///
/// Used for multi-discriminant union checking where the source must be narrowed
/// by all discriminant properties at once. `combo_indices[d]` selects which
/// value from `disc_values[d]` to use for discriminant property `disc_props[d]`.
fn narrow_object_properties(
    db: &dyn TypeDatabase,
    shape_id: ObjectShapeId,
    disc_props: &[(Atom, TypeId)],
    disc_values: &[smallvec::SmallVec<[TypeId; 4]>],
    combo_indices: &[usize],
) -> TypeId {
    let shape = db.object_shape(shape_id);
    let mut new_props: Vec<PropertyInfo> = shape.properties.to_vec();

    for (d, &(prop_name, _)) in disc_props.iter().enumerate() {
        let value = disc_values[d][combo_indices[d]];
        if let Ok(idx) = new_props.binary_search_by(|p| p.name.cmp(&prop_name)) {
            new_props[idx] = PropertyInfo {
                type_id: value,
                write_type: value,
                optional: false,
                ..new_props[idx].clone()
            };
        }
    }

    db.object(new_props)
}

fn narrow_tuple_elements(
    db: &dyn TypeDatabase,
    source_elems: &[TupleElement],
    disc_positions: &[usize],
    disc_values: &[smallvec::SmallVec<[TypeId; 4]>],
    combo_indices: &[usize],
) -> TypeId {
    let mut elements = source_elems.to_vec();
    for (d, &position) in disc_positions.iter().enumerate() {
        elements[position].type_id = disc_values[d][combo_indices[d]];
        elements[position].optional = false;
    }
    db.tuple(elements)
}

#[cfg(test)]
mod discriminant_guard_state_tests {
    use super::*;

    #[test]
    fn discriminated_object_size_state_names_exact_cap_and_overflow() {
        assert_eq!(
            DiscriminatedObjectSizeState::for_property_count(MAX_PROPERTIES_FOR_DISCRIMINATED),
            DiscriminatedObjectSizeState::Continue
        );
        assert_eq!(
            DiscriminatedObjectSizeState::for_property_count(MAX_PROPERTIES_FOR_DISCRIMINATED + 1),
            DiscriminatedObjectSizeState::TooManyProperties
        );
    }

    #[test]
    fn discriminant_combination_state_names_exact_cap_and_overflow() {
        assert_eq!(
            DiscriminantCombinationState::for_count(MAX_DISCRIMINANT_COMBINATIONS),
            DiscriminantCombinationState::Continue
        );
        assert_eq!(
            DiscriminantCombinationState::for_count(MAX_DISCRIMINANT_COMBINATIONS + 1),
            DiscriminantCombinationState::LimitExceeded
        );
    }
}
