//! Union and intersection type subtype checking.
//!
//! This module handles subtyping for TypeScript's composite types:
//! - Union types (A | B | C) - source must be subtype of at least one member
//! - Intersection types (A & B & C) - source must be subtype of all members
//! - Distributivity rules between unions and intersections
//! - Type parameter compatibility in union/intersection contexts

use crate::TypeDatabase;
use crate::types::{ObjectShapeId, PropertyInfo, TypeId, TypeParamInfo};
use crate::visitor::{
    is_literal_type, is_unit_type, object_shape_id, object_with_index_shape_id, type_param_info,
    union_list_id,
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
            return self.check_subtype(constraint, target);
        }

        // Unconstrained type parameter: use {} (empty object) as base constraint.
        // In TypeScript, an unconstrained type parameter's base constraint is {},
        // meaning it can be assigned to any object type with only optional properties.
        let empty_object = self.interner.object(Vec::new());
        self.check_subtype(empty_object, target)
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
        if !allow_bivariant {
            return self.check_subtype(source, target);
        }

        // If we're already in bivariant mode, don't nest - just check normally
        // This prevents infinite recursion when methods contain other methods
        if !self.strict_function_types && self.allow_bivariant_param_count {
            return self.check_subtype(source, target);
        }

        let prev = self.strict_function_types;
        let prev_param_count = self.allow_bivariant_param_count;
        self.strict_function_types = false;
        self.allow_bivariant_param_count = true;
        let result = self.check_subtype(source, target);
        self.allow_bivariant_param_count = prev_param_count;
        self.strict_function_types = prev;
        result
    }

    /// Explain failure with method bivariance rules.
    pub(crate) fn explain_failure_with_method_variance(
        &mut self,
        source: TypeId,
        target: TypeId,
        allow_bivariant: bool,
    ) -> Option<SubtypeFailureReason> {
        if !allow_bivariant {
            return self.explain_failure(source, target);
        }

        // If we're already in bivariant mode, don't nest - just check normally
        // This prevents infinite recursion when methods contain other methods
        if !self.strict_function_types && self.allow_bivariant_param_count {
            return self.explain_failure(source, target);
        }

        let prev = self.strict_function_types;
        let prev_param_count = self.allow_bivariant_param_count;
        self.strict_function_types = false;
        self.allow_bivariant_param_count = true;
        let result = self.explain_failure(source, target);
        self.allow_bivariant_param_count = prev_param_count;
        self.strict_function_types = prev;
        result
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
        let source_shape_id = match get_object_shape(self.interner, source) {
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
            let source_values = get_type_constituents(self.interner, source_prop_type);
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
        let source_values = get_type_constituents(self.interner, disc_source_type);

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

/// Get the object shape id for a type (handles both Object and `ObjectWithIndex`).
fn get_object_shape(db: &dyn TypeDatabase, type_id: TypeId) -> Option<ObjectShapeId> {
    object_shape_id(db, type_id).or_else(|| object_with_index_shape_id(db, type_id))
}

/// Get the constituents of a type. If it's a union, return all members.
/// Otherwise return a singleton slice.
fn get_type_constituents(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<TypeId> {
    if let Some(list_id) = union_list_id(db, type_id) {
        db.type_list(list_id).to_vec()
    } else {
        vec![type_id]
    }
}

/// Get a property type from an object-like type by atom name.
/// For optional properties, includes `undefined` in the type.
fn get_property_type_of_object(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    prop_name: Atom,
) -> Option<TypeId> {
    let shape_id = get_object_shape(db, type_id)?;
    let shape = db.object_shape(shape_id);
    let prop = shape
        .properties
        .binary_search_by(|p| p.name.cmp(&prop_name))
        .ok()
        .map(|idx| &shape.properties[idx])?;
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
        let shape_id = match get_object_shape(db, member) {
            Some(id) => id,
            None => return false, // All members must be object types
        };
        let shape = db.object_shape(shape_id);
        let prop = match shape
            .properties
            .binary_search_by(|p| p.name.cmp(&prop_name))
            .ok()
        {
            Some(idx) => &shape.properties[idx],
            None => {
                // Property missing — still valid if optional in source
                // For discriminant purposes, treat missing as "undefined"
                continue;
            }
        };

        let prop_type = prop.type_id;
        // Check if any constituent is a unit type
        for &constituent in &get_type_constituents(db, prop_type) {
            if is_unit_type(db, constituent) || is_literal_type(db, constituent) {
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
            ..new_props[idx].clone()
        };
    }

    db.object(new_props)
}
