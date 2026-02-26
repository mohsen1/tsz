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
use crate::visitor::{
    index_access_parts, is_identity_comparable_type, is_literal_type, keyof_inner_type,
    mapped_type_id, type_param_info, union_list_id,
};
use tsz_common::interner::Atom;

use super::super::{SubtypeChecker, SubtypeFailureReason, SubtypeResult, TypeResolver};

/// Maximum number of discriminant value combinations before giving up.
/// This matches TypeScript's limit to prevent exponential blowup.
const MAX_DISCRIMINANT_COMBINATIONS: usize = 25;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check if two types are equivalent (mutually subtypes).
    ///
    /// Type equivalence means bidirectional subtyping:
    /// `A ≡ B` iff `A <: B` AND `B <: A`
    ///
    /// ## Examples:
    /// - `string` ≡ `string` ✅ (reflexive)
    /// - `A | B` ≡ `B | A` ✅ (union commutes)
    /// - `T & U` ≡ `U & T` ✅ (intersection commutes)
    ///
    /// Note: For most type checking, unidirectional subtyping (`<:`) is used.
    /// Equivalence (`≡`) is primarily for type parameter constraints and exact matching.
    pub(crate) fn types_equivalent(&mut self, left: TypeId, right: TypeId) -> bool {
        self.check_subtype(left, right).is_true() && self.check_subtype(right, left).is_true()
    }

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
            && !super::generics::is_filtering_name_type(self.interner, name_type, &mapped) {
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

        // Template must be S[K] where K is the iteration parameter (homomorphic form)
        let Some((template_obj, template_idx)) = index_access_parts(self.interner, mapped.template)
        else {
            return false;
        };
        let Some(idx_param) = type_param_info(self.interner, template_idx) else {
            return false;
        };
        if idx_param.name != mapped.type_param.name {
            return false;
        }

        // Template object must match constraint source (e.g., T[K] where constraint is keyof T)
        if template_obj != constraint_source {
            return false;
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
        if allow_bivariant {
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

        // Find discriminant properties in the source that discriminate target
        let disc_props =
            find_discriminant_properties(self.interner, &source_shape.properties, target_members);
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
                    if let Some(t_prop_type) = t_prop
                        && self.check_subtype(value, t_prop_type).is_true()
                    {
                        reachable[i] = true;
                        value_has_match = true;
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

        // Verify: for each discriminant value combination, the narrowed source
        // must be assignable to at least one matching target member.
        // Use the first discriminant for per-value checking.
        let (disc_name, disc_source_type) = disc_props[0];
        let source_values = get_discriminant_values(self.interner, disc_source_type);

        for &value in &source_values {
            let narrowed = narrow_object_property(self.interner, source_shape_id, disc_name, value);
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
        }

        SubtypeResult::True
    }
}

// ── Helper functions for discriminated union checking ──

/// Get the constituents of a type. If it's a union, return all members.
/// Otherwise return a singleton slice.
fn get_type_constituents(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<TypeId> {
    if let Some(list_id) = union_list_id(db, type_id) {
        db.type_list(list_id).to_vec()
    } else {
        vec![type_id]
    }
}

/// Get discriminant values from a source property type.
///
/// This expands `boolean` to `true | false` to enable discriminated union matching,
/// since TypeScript treats `boolean` as equivalent to `true | false` for this purpose.
/// For other types, returns constituents as-is.
fn get_discriminant_values(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<TypeId> {
    // Special case: boolean is equivalent to true | false for discriminated union matching
    if type_id == TypeId::BOOLEAN {
        return vec![TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN_FALSE];
    }

    get_type_constituents(db, type_id)
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
fn find_discriminant_properties(
    db: &dyn TypeDatabase,
    source_props: &[PropertyInfo],
    target_members: &[TypeId],
) -> Vec<(Atom, TypeId)> {
    let mut result = Vec::new();

    for prop in source_props {
        if is_discriminant_for_union(db, prop.name, target_members) {
            result.push((prop.name, prop.type_id));
        }
    }

    result
}

/// Check if a property name is a discriminant for a target union.
fn is_discriminant_for_union(
    db: &dyn TypeDatabase,
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
        // Check if any constituent is a unit type
        for &constituent in &get_type_constituents(db, prop_type) {
            if is_identity_comparable_type(db, constituent) || is_literal_type(db, constituent) {
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

/// Create a new object type by narrowing one property to a specific type.
fn narrow_object_property(
    db: &dyn TypeDatabase,
    shape_id: ObjectShapeId,
    prop_name: Atom,
    narrowed_type: TypeId,
) -> TypeId {
    let shape = db.object_shape(shape_id);
    let mut new_props: Vec<PropertyInfo> = shape.properties.to_vec();

    if let Ok(idx) = new_props.binary_search_by(|p| p.name.cmp(&prop_name)) {
        new_props[idx] = PropertyInfo {
            type_id: narrowed_type,
            write_type: narrowed_type,
            ..new_props[idx].clone()
        };
    }

    db.object(new_props)
}
