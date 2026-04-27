//! Union and intersection type subtype checking.
//!
//! This module handles subtyping for TypeScript's composite types:
//! - Union types (A | B | C) - source must be subtype of at least one member
//! - Intersection types (A & B & C) - source must be subtype of all members
//! - Distributivity rules between unions and intersections
//! - Type parameter compatibility in union/intersection contexts

use crate::TypeDatabase;
use crate::type_queries::data::get_object_shape_id;
use crate::types::{
    MappedModifier, MappedTypeId, ObjectShapeId, PropertyInfo, TypeId, TypeParamInfo,
};
use crate::visitor::enum_components;
use crate::visitor::{
    application_id, index_access_parts, is_identity_comparable_type, is_literal_type,
    keyof_inner_type, lazy_def_id, mapped_type_id, readonly_inner_type, tuple_list_id,
    type_param_info, union_list_id,
};
use tsz_common::interner::Atom;

use super::super::{SubtypeChecker, SubtypeFailureReason, SubtypeResult, TypeResolver};

/// Maximum number of discriminant value combinations before giving up.
/// This matches TypeScript's limit to prevent exponential blowup.
const MAX_DISCRIMINANT_COMBINATIONS: usize = 25;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check if a type parameter is a subtype of a target type.
    ///
    /// Handles both type parameter vs type parameter and type parameter vs concrete type.
    /// Implements TypeScript's soundness rules for type parameter compatibility.
    ///
    /// ## TypeScript Soundness Rules:
    /// - Same type parameter by name → reflexive (always compatible)
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
            if s_info.name == t_info.name {
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
            && self.is_assignable_to_homomorphic_mapped(s_info.name, s_info.constraint, mapped_id)
        {
            return SubtypeResult::True;
        }

        // Also handle Application targets that resolve to mapped types.
        // e.g., MyMap<U> where type MyMap<T> = { [P in keyof T]: T[keyof T] }
        // The Application expands to a Mapped type which we can then check.
        if let Some(app_id) = application_id(self.interner, target)
            && let Some(expanded) = self.try_expand_application(app_id)
            && let Some(mapped_id) = mapped_type_id(self.interner, expanded)
            && self.is_assignable_to_homomorphic_mapped(s_info.name, s_info.constraint, mapped_id)
        {
            return SubtypeResult::True;
        }

        // Variadic tuple identity: T is assignable to [...T] (and readonly [...T])
        // when T is a type parameter. tsc treats [...T] as structurally equivalent to T.
        // This handles: T <: [...T], T <: readonly [...T]
        {
            // Unwrap readonly wrapper if present
            let inner_target = readonly_inner_type(self.interner, target).unwrap_or(target);
            if let Some(t_list) = tuple_list_id(self.interner, inner_target) {
                let t_elems = self.interner.tuple_list(t_list);
                if t_elems.len() == 1
                    && t_elems[0].rest
                    && type_param_info(self.interner, t_elems[0].type_id)
                        .is_some_and(|inner_info| inner_info.name == s_info.name)
                {
                    return SubtypeResult::True;
                }
            }
        }

        SubtypeResult::False
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
        source_name: Atom,
        source_constraint: Option<TypeId>,
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

        // Constraint must be keyof(S) for some S
        let Some(constraint_source) = keyof_inner_type(self.interner, mapped.constraint) else {
            return false;
        };

        // Fast path: Template is exactly S[K] where K is the iteration parameter
        let is_identity_template = if let Some((template_obj, template_idx)) =
            index_access_parts(self.interner, mapped.template)
        {
            if let Some(idx_param) = type_param_info(self.interner, template_idx) {
                idx_param.name == mapped.type_param.name && template_obj == constraint_source
            } else {
                false
            }
        } else {
            false
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
                variance: crate::TypeParamVariance::None,
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
            && source_param.name == source_name
        {
            return true;
        }

        // Check if source constraint is assignable to the mapped type source
        if let Some(constraint) = source_constraint {
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
        if allow_bivariant && !self.identity_cycle_check {
            // Method bivariance: temporarily disable strict_function_types
            // so check_parameter_compatibility uses bivariant parameter checks.
            // This only affects parameter variance, NOT return type variance.
            let prev = self.strict_function_types;
            self.strict_function_types = false;
            let result = self.check_subtype(source, target);
            self.strict_function_types = prev;
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
        if allow_bivariant {
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
        const MAX_PROPERTIES_FOR_DISCRIMINATED: usize = 50;
        if source_shape.properties.len() > MAX_PROPERTIES_FOR_DISCRIMINATED {
            return SubtypeResult::False;
        }

        // Find discriminant properties in the source that discriminate target
        let disc_props = find_discriminant_properties(
            self.interner,
            self.resolver,
            &source_shape.properties,
            target_members,
        );
        if disc_props.is_empty() {
            return SubtypeResult::False;
        }

        // For each discriminant property, collect source values and matching targets.
        // Start with all target members, then intersect across discriminants.
        let mut candidate_targets: Option<Vec<bool>> = None;

        for &(prop_name, source_prop_type) in &disc_props {
            let source_values = get_discriminant_values(self.interner, source_prop_type);
            if source_values.len() > MAX_DISCRIMINANT_COMBINATIONS {
                return SubtypeResult::False;
            }

            // For this discriminant, track which target members are reachable
            let mut reachable = vec![false; target_members.len()];

            for &value in &source_values {
                let mut value_has_match = false;
                for (i, &target_member) in target_members.iter().enumerate() {
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
            .map(|&(_, source_prop_type)| get_discriminant_values(self.interner, source_prop_type))
            .collect();

        // Check total combinations don't exceed limit
        let total_combinations: usize = disc_values.iter().map(|v| v.len()).product();
        if total_combinations > MAX_DISCRIMINANT_COMBINATIONS {
            return SubtypeResult::False;
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
                if self.check_subtype(narrowed, target_member).is_true() {
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
}

// ── Helper functions for discriminated union checking ──

/// Get the constituents of a type. If it's a union, return all members.
/// Otherwise return a singleton. Uses `SmallVec` to avoid heap allocation
/// for the common singleton case.
fn get_type_constituents(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> smallvec::SmallVec<[TypeId; 4]> {
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
/// Unions containing type parameters (e.g., `T | E.A`) are flattened by resolving each
/// type parameter member to its constraint values.
fn get_discriminant_values(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> smallvec::SmallVec<[TypeId; 4]> {
    // Special case: boolean is equivalent to true | false for discriminated union matching
    if type_id == TypeId::BOOLEAN {
        return smallvec::smallvec![TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN_FALSE];
    }

    // Resolve type parameters to their constraints for discriminant matching.
    // e.g., T extends "a" | "b" → use "a" | "b" as discriminant values.
    if let Some(info) = type_param_info(db, type_id)
        && let Some(constraint) = info.constraint
    {
        return get_discriminant_values(db, constraint);
    }

    // Expand enum types to their structural member values.
    // e.g., enum E { A, B } → Enum(def, 0 | 1) → [0, 1]
    // This allows discriminated union matching against E.A | E.B targets,
    // since 0 <: Enum(E.A_def, 0) succeeds via structural enum subtyping.
    if let Some((_def_id, structural_type)) = enum_components(db, type_id) {
        return get_discriminant_values(db, structural_type);
    }

    let constituents = get_type_constituents(db, type_id);

    // Expand unions containing type parameters by resolving each member.
    // e.g., T | E.A where T extends E → expand T to its constraint values.
    if constituents.len() > 1
        && constituents
            .iter()
            .any(|c| type_param_info(db, *c).is_some())
    {
        let mut result = smallvec::SmallVec::new();
        for &c in &constituents {
            result.extend(get_discriminant_values(db, c));
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
        // Check if any constituent is a unit type.
        // Resolve Lazy types first — enum member property types may still
        // be Lazy(DefId) in the object shape after top-level union evaluation.
        for &constituent in &get_type_constituents(db, prop_type) {
            let resolved = if let Some(def_id) = lazy_def_id(db, constituent) {
                resolver.resolve_lazy(def_id, db).unwrap_or(constituent)
            } else {
                constituent
            };
            if is_identity_comparable_type(db, resolved) || is_literal_type(db, resolved) {
                has_unit = true;
            }
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
