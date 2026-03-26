//! Discriminant-based narrowing for discriminated unions.
//!
//! Handles narrowing of types based on discriminant property checks.
//! For example, `action.type === "add"` narrows `Action` to
//! `{ type: "add", value: number }`.
//!
//! Key functions:
//! - `find_discriminants`: Identifies discriminant properties in unions
//! - `narrow_by_discriminant`: Narrows to matching union members
//! - `narrow_by_excluding_discriminant`: Excludes matching union members

use std::sync::Arc;

use crate::TypeData;
use rustc_hash::FxHashMap;

use super::{DiscriminantInfo, NarrowingContext, union_or_single_preserve};
use crate::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};
use crate::relations::subtype::is_subtype_of;
use crate::type_queries::{
    LiteralValueKind, UnionMembersKind, classify_for_literal_value, classify_for_union_members,
    get_tuple_elements,
};
use crate::types::{PropertyLookup, TypeId};
use crate::visitor::{
    intersection_list_id, is_literal_type_through_type_constraints, object_shape_id,
    object_with_index_shape_id, union_list_id,
};
use rustc_hash::FxHashSet;
use tracing::{Level, span, trace};
use tsz_common::interner::Atom;

impl<'a> NarrowingContext<'a> {
    /// Resolve a type into its union members and build a property evaluator.
    ///
    /// This is the shared setup for all discriminant/property-based narrowing:
    /// 1. Resolves Lazy types
    /// 2. Classifies into union members (or wraps a non-union as a single-element list)
    /// 3. Creates a `PropertyAccessEvaluator` respecting any resolver override
    fn resolve_members_and_evaluator(
        &self,
        type_id: TypeId,
    ) -> (TypeId, Vec<TypeId>, PropertyAccessEvaluator<'_>) {
        let resolved = self.resolve_type(type_id);
        let members = match classify_for_union_members(self.db, resolved) {
            UnionMembersKind::Union(list) => list.into_iter().collect::<Vec<_>>(),
            UnionMembersKind::NotUnion => vec![resolved],
        };
        let evaluator = match self.resolver {
            Some(resolver) => PropertyAccessEvaluator::with_resolver(self.db, resolver),
            None => PropertyAccessEvaluator::new(self.db),
        };
        (resolved, members, evaluator)
    }

    /// Find discriminant properties in a union type.
    ///
    /// A discriminant property is one where:
    /// 1. All union members have the property
    /// 2. Each member has a unique literal type for that property
    pub fn find_discriminants(&self, union_type: TypeId) -> Vec<DiscriminantInfo> {
        let _span = span!(
            Level::TRACE,
            "find_discriminants",
            union_type = union_type.0
        )
        .entered();

        let members = match union_list_id(self.db, union_type) {
            Some(members_id) => self.db.type_list(members_id),
            None => return vec![],
        };

        if members.len() < 2 {
            trace!("Union has fewer than 2 members, skipping discriminant search");
            return vec![];
        }

        // Collect all property names from all members
        let mut all_properties: Vec<Atom> = Vec::new();
        let mut seen_properties: FxHashSet<Atom> = FxHashSet::default();
        let mut member_props: Vec<Vec<(Atom, TypeId)>> = Vec::new();

        for &member in members.iter() {
            if let Some(shape_id) = object_shape_id(self.db, member) {
                let shape = self.db.object_shape(shape_id);
                let props_vec: Vec<(Atom, TypeId)> = shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id))
                    .collect();

                // Track all property names (O(1) per insert with HashSet)
                for (name, _) in &props_vec {
                    if seen_properties.insert(*name) {
                        all_properties.push(*name);
                    }
                }
                member_props.push(props_vec);
            } else {
                // Non-object member - can't have discriminants
                return vec![];
            }
        }

        // Check each property to see if it's a valid discriminant
        let mut discriminants = Vec::new();

        for prop_name in &all_properties {
            let mut is_discriminant = true;
            let mut variants: Vec<(TypeId, TypeId)> = Vec::new();
            let mut seen_literals: FxHashSet<TypeId> = FxHashSet::default();

            for (i, props) in member_props.iter().enumerate() {
                // Find this property in the member
                let prop_type = props
                    .iter()
                    .find(|(name, _)| name == prop_name)
                    .map(|(_, ty)| *ty);

                match prop_type {
                    Some(ty) => {
                        // Evaluate the property type to resolve template literals
                        // (e.g., `${AnimalType.cat}` → "cat") and enum wrappers
                        // before checking if it's a literal discriminant.
                        let evaluated_ty = self.db.evaluate_type(ty);
                        let check_ty =
                            if is_literal_type_through_type_constraints(self.db, evaluated_ty) {
                                evaluated_ty
                            } else if is_literal_type_through_type_constraints(self.db, ty) {
                                ty
                            } else {
                                // Not a literal type even after evaluation
                                is_discriminant = false;
                                break;
                            };
                        // Must be unique among members (O(1) with HashSet)
                        if !seen_literals.insert(check_ty) {
                            is_discriminant = false;
                            break;
                        }
                        variants.push((check_ty, members[i]));
                    }
                    None => {
                        // Property doesn't exist in this member
                        is_discriminant = false;
                        break;
                    }
                }
            }

            if is_discriminant && !variants.is_empty() {
                discriminants.push(DiscriminantInfo {
                    property_name: *prop_name,
                    variants,
                });
            }
        }

        discriminants
    }

    /// Get the type of a property at a nested path within a type.
    ///
    /// # Examples
    /// - `get_type_at_path(type, ["payload"])` -> type of `payload` property
    /// - `get_type_at_path(type, ["payload", "type"])` -> type of `payload.type`
    ///
    /// Returns `None` if:
    /// - The type doesn't have the property at any level in the path
    /// - An intermediate type in the path is not an object type
    ///
    /// **NOTE**: Uses `resolve_property_access` which correctly handles optional properties.
    /// For optional properties that don't exist on a specific union member, returns
    /// `TypeId::UNDEFINED` to indicate the property could be undefined (not a definitive mismatch).
    fn get_type_at_path(
        &self,
        mut type_id: TypeId,
        path: &[Atom],
        evaluator: &PropertyAccessEvaluator<'_>,
    ) -> Option<TypeId> {
        for (i, &prop_name) in path.iter().enumerate() {
            // Handle ANY - any property access on any returns any
            if type_id == TypeId::ANY {
                return Some(TypeId::ANY);
            }

            // Resolve Lazy types
            type_id = self.resolve_type(type_id);

            // Handle Union - return union of property types from all members
            if let Some(members_id) = union_list_id(self.db, type_id) {
                let members = self.db.type_list(members_id);
                let remaining_path = &path[i..];
                let prop_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&member| self.get_type_at_path(member, remaining_path, evaluator))
                    .collect();

                if prop_types.is_empty() {
                    return None;
                } else if prop_types.len() == 1 {
                    return Some(prop_types[0]);
                }
                return Some(self.db.union(prop_types));
            }

            // Tuple discriminants like `switch (pair[0])` narrow the base tuple/union,
            // but the discriminant path is represented as string atoms (`"0"`, `"1"`).
            // PropertyAccessEvaluator does not treat tuple elements as named properties,
            // so handle that structural lookup directly here.
            let prop_name_arc = self.db.resolve_atom_ref(prop_name);
            let prop_name_str = prop_name_arc.as_ref();
            if let Ok(index) = prop_name_str.parse::<usize>()
                && let Some(elements) = get_tuple_elements(self.db, type_id)
            {
                if index < elements.len() {
                    type_id = elements[index].type_id;
                    continue;
                }

                if let Some(rest) = elements.iter().rev().find(|elem| elem.rest) {
                    type_id = rest.type_id;
                    continue;
                }
            }

            // Use resolve_property_access for proper optional property handling
            // This correctly handles properties that are optional (prop?: type)
            match evaluator.resolve_property_access(type_id, prop_name_str) {
                PropertyAccessResult::Success {
                    type_id: prop_type_id,
                    ..
                } => {
                    // Property found - use its type
                    // For optional properties, this already includes `undefined` in the union
                    type_id = prop_type_id;
                }
                PropertyAccessResult::PropertyNotFound { .. } => {
                    // Property truly doesn't exist on this type
                    // This union member doesn't have the discriminant property, so filter it out
                    return None;
                }
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    // CRITICAL FIX: For optional properties (prop?: type), we need to preserve
                    // both the property type AND undefined in the union.
                    // This ensures that is_subtype_of(circle, "circle" | undefined) works correctly.
                    if let Some(prop_ty) = property_type {
                        // Create union: property_type | undefined
                        type_id = self.db.union2(prop_ty, TypeId::UNDEFINED);
                    } else {
                        // No property type, just undefined
                        type_id = TypeId::UNDEFINED;
                    }
                }
                PropertyAccessResult::IsUnknown => {
                    return Some(TypeId::ANY);
                }
            }
        }

        Some(type_id)
    }

    /// Fast path for top-level property lookup on object members.
    ///
    /// This avoids `PropertyAccessEvaluator` for the common discriminant pattern
    /// `x.kind === "..."` where we only need a direct property read from object-like
    /// union members. Falls back to the general path for complex structures.
    fn get_top_level_property_type_fast(&self, type_id: TypeId, property: Atom) -> Option<TypeId> {
        let key = (type_id, property);
        if let Some(&cached) = self.cache.property_cache.borrow().get(&key) {
            // Don't trust a cached Lazy type — re-resolve in case the TypeEnvironment
            // has been populated since the cache entry was created.
            if let Some(prop_type) = cached {
                if !matches!(self.db.lookup(prop_type), Some(TypeData::Lazy(_))) {
                    return cached;
                }
                // Re-resolve the Lazy type
                let re_resolved = self.resolve_type(prop_type);
                if re_resolved != prop_type {
                    self.cache
                        .property_cache
                        .borrow_mut()
                        .insert(key, Some(re_resolved));
                    return Some(re_resolved);
                }
                return cached;
            }
            return cached;
        }

        // Cache the resolved property type so hot paths avoid an extra resolve pass.
        let result = self
            .get_top_level_property_type_fast_uncached(type_id, property)
            .map(|prop_type| self.resolve_type(prop_type));
        // Don't cache unresolved Lazy property types — the TypeEnvironment may be
        // populated later during checking, and we need to re-resolve on next access.
        let should_cache = match result {
            Some(prop_type) => !matches!(self.db.lookup(prop_type), Some(TypeData::Lazy(_))),
            None => true,
        };
        if should_cache {
            self.cache.property_cache.borrow_mut().insert(key, result);
        }
        result
    }

    fn get_top_level_property_type_fast_uncached(
        &self,
        mut type_id: TypeId,
        property: Atom,
    ) -> Option<TypeId> {
        type_id = self.resolve_type(type_id);

        // Keep this fast path conservative: intersections and complex wrappers
        // should use the full evaluator-based path for correctness.
        if intersection_list_id(self.db, type_id).is_some() {
            return None;
        }

        let shape_id = object_shape_id(self.db, type_id)
            .or_else(|| object_with_index_shape_id(self.db, type_id))?;
        let shape = self.db.object_shape(shape_id);

        let prop = match self.db.object_property_index(shape_id, property) {
            PropertyLookup::Found(idx) => shape.properties.get(idx),
            PropertyLookup::NotFound => None,
            PropertyLookup::Uncached => {
                // Properties are sorted by Atom id.
                shape
                    .properties
                    .binary_search_by_key(&property, |p| p.name)
                    .ok()
                    .and_then(|idx| shape.properties.get(idx))
            }
        }?;

        Some(if prop.optional {
            self.db.union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        })
    }

    /// Fast literal-only subtype check used by discriminant hot paths.
    ///
    /// Returns `None` when either side is non-literal (or not a string/number
    /// literal) so callers can fall back to the full subtype relation.
    #[inline]
    fn literal_subtype_fast(&self, source: TypeId, target: TypeId) -> Option<bool> {
        if source == target {
            return Some(true);
        }

        match (
            classify_for_literal_value(self.db, source),
            classify_for_literal_value(self.db, target),
        ) {
            (LiteralValueKind::String(a), LiteralValueKind::String(b)) => Some(a == b),
            (LiteralValueKind::Number(a), LiteralValueKind::Number(b)) => Some(a == b),
            (LiteralValueKind::String(_), LiteralValueKind::Number(_))
            | (LiteralValueKind::Number(_), LiteralValueKind::String(_)) => Some(false),
            _ => None,
        }
    }

    /// Fast narrowing for `x.<prop> === literal` / `!== literal` over union members.
    ///
    /// Returns `None` to request fallback to the general evaluator-based implementation
    /// when the structure is too complex for this direct path.
    fn fast_narrow_top_level_discriminant(
        &self,
        original_union_type: TypeId,
        members: &[TypeId],
        property: Atom,
        literal_value: TypeId,
        keep_matching: bool,
    ) -> Option<TypeId> {
        // PERF: Use discriminant index for O(1) lookup instead of O(N) member iteration.
        // Works for both true branch (keep matching) and false branch (exclude matching).
        if members.len() >= 8 {
            if keep_matching {
                if let Some(result) = self.fast_narrow_via_discriminant_index(
                    original_union_type,
                    members,
                    property,
                    literal_value,
                ) {
                    return Some(result);
                }
            } else if let Some(result) = self.fast_narrow_excluding_via_discriminant_index(
                original_union_type,
                members,
                property,
                literal_value,
            ) {
                return Some(result);
            }
        }

        let mut kept = Vec::with_capacity(members.len());

        for &member in members {
            if member.is_any_or_unknown() {
                kept.push(member);
                continue;
            }

            let prop_type = self.get_top_level_property_type_fast(member, property)?;
            let should_keep = if prop_type == literal_value {
                keep_matching
            } else if keep_matching {
                // true branch: keep members where literal <: property_type
                self.literal_subtype_fast(literal_value, prop_type)
                    .unwrap_or_else(|| is_subtype_of(self.db, literal_value, prop_type))
            } else {
                // false branch: exclude members where property_type <: excluded_literal
                !self
                    .literal_subtype_fast(prop_type, literal_value)
                    .unwrap_or_else(|| is_subtype_of(self.db, prop_type, literal_value))
            };

            if should_keep {
                kept.push(member);
            }
        }

        if keep_matching && kept.is_empty() {
            return Some(TypeId::NEVER);
        }
        if keep_matching && kept.len() == members.len() {
            return Some(original_union_type);
        }

        Some(union_or_single_preserve(self.db, kept))
    }

    /// O(1) discriminant narrowing using a cached index.
    /// Builds the index once per (union, property) pair, then returns matching members
    /// in O(1) for each literal value lookup.
    fn fast_narrow_via_discriminant_index(
        &self,
        original_union_type: TypeId,
        members: &[TypeId],
        property: Atom,
        literal_value: TypeId,
    ) -> Option<TypeId> {
        let cache_key = (original_union_type, property);

        // Check if index is already built
        let existing = self
            .cache
            .discriminant_index
            .borrow()
            .get(&cache_key)
            .cloned();

        let index = if let Some(idx) = existing {
            idx
        } else {
            // Build the discriminant index: literal_value → Vec<matching_members>
            let mut index_map: FxHashMap<TypeId, Vec<TypeId>> = FxHashMap::default();
            let mut any_unknown_members: Vec<TypeId> = Vec::new();
            let mut has_non_indexable = false;

            for &member in members {
                if member.is_any_or_unknown() {
                    any_unknown_members.push(member);
                    continue;
                }
                match self.get_top_level_property_type_fast(member, property) {
                    Some(prop_type) => {
                        index_map.entry(prop_type).or_default().push(member);
                    }
                    None => {
                        // Member doesn't have a simple property lookup — can't index
                        has_non_indexable = true;
                        break;
                    }
                }
            }

            if has_non_indexable {
                return None; // Fall back to linear scan
            }

            // Add any/unknown members to every bucket (they always match)
            if !any_unknown_members.is_empty() {
                for bucket in index_map.values_mut() {
                    bucket.extend_from_slice(&any_unknown_members);
                }
            }

            let index = Arc::new(index_map);
            self.cache
                .discriminant_index
                .borrow_mut()
                .insert(cache_key, Arc::clone(&index));
            index
        };

        // O(1) lookup
        match index.get(&literal_value) {
            Some(matching) if matching.is_empty() => Some(TypeId::NEVER),
            Some(matching) if matching.len() == 1 => Some(matching[0]),
            Some(matching) => Some(self.db.union(matching.clone())),
            None => {
                // No exact match — try subtype matching for non-literal discriminants
                // This handles cases like `type: string` matching `type: "specific"`.
                // Fall back to linear scan for these edge cases.
                None
            }
        }
    }

    /// O(1) discriminant narrowing for the false/excluding branch.
    /// Builds the index if needed, then returns all members NOT matching the literal.
    fn fast_narrow_excluding_via_discriminant_index(
        &self,
        original_union_type: TypeId,
        members: &[TypeId],
        property: Atom,
        excluded_literal: TypeId,
    ) -> Option<TypeId> {
        // Build/retrieve the index (same as fast_narrow_via_discriminant_index)
        let cache_key = (original_union_type, property);
        let existing = self
            .cache
            .discriminant_index
            .borrow()
            .get(&cache_key)
            .cloned();

        let index = if let Some(idx) = existing {
            idx
        } else {
            // Trigger index build via the positive path
            let _ = self.fast_narrow_via_discriminant_index(
                original_union_type,
                members,
                property,
                excluded_literal,
            );
            // Re-fetch from cache
            self.cache
                .discriminant_index
                .borrow()
                .get(&cache_key)
                .cloned()?
        };

        // Compute complement: all members from OTHER buckets
        let excluded_set = index.get(&excluded_literal);
        let mut kept = Vec::new();
        for (_, bucket_members) in index.iter() {
            for &m in bucket_members {
                if excluded_set.is_none_or(|excluded| !excluded.contains(&m)) && !kept.contains(&m)
                {
                    kept.push(m);
                }
            }
        }

        if kept.is_empty() {
            Some(TypeId::NEVER)
        } else if kept.len() == members.len() {
            Some(original_union_type)
        } else {
            Some(union_or_single_preserve(self.db, kept))
        }
    }

    /// Narrow a union type based on a discriminant property check.
    ///
    /// Example: `action.type === "add"` narrows `Action` to `{ type: "add", value: number }`
    ///
    /// Uses a filtering approach: checks each union member individually to see if
    /// the property could match the literal value. This is more flexible than the
    /// old `find_discriminants` approach which required ALL members to have the
    /// property with unique literal values.
    ///
    /// # Arguments
    /// Narrow a type by discriminant, handling type parameter constraints.
    ///
    /// If the type is a type parameter with a constraint, narrows the constraint
    /// and intersects with the type parameter when the constraint is affected.
    pub fn narrow_by_discriminant_for_type(
        &self,
        type_id: TypeId,
        prop_path: &[Atom],
        literal_type: TypeId,
        is_true_branch: bool,
    ) -> TypeId {
        use crate::type_queries::{
            TypeParameterConstraintKind, classify_for_type_parameter_constraint,
        };

        if let TypeParameterConstraintKind::TypeParameter {
            constraint: Some(constraint),
        } = classify_for_type_parameter_constraint(self.db, type_id)
            && constraint != type_id
        {
            let narrowed_constraint = if is_true_branch {
                self.narrow_by_discriminant(constraint, prop_path, literal_type)
            } else {
                self.narrow_by_excluding_discriminant(constraint, prop_path, literal_type)
            };
            if narrowed_constraint != constraint {
                return self.db.intersection2(type_id, narrowed_constraint);
            }
        }

        if is_true_branch {
            self.narrow_by_discriminant(type_id, prop_path, literal_type)
        } else {
            self.narrow_by_excluding_discriminant(type_id, prop_path, literal_type)
        }
    }

    /// Narrows a union type based on whether a property is truthy or falsy.
    ///
    /// This is used for conditionals like `if (x.prop)` when `x` is a union.
    pub fn narrow_by_property_truthiness(
        &self,
        union_type: TypeId,
        property_path: &[Atom],
        sense: bool,
    ) -> TypeId {
        use crate::type_queries::{
            TypeParameterConstraintKind, classify_for_type_parameter_constraint,
        };

        if let TypeParameterConstraintKind::TypeParameter {
            constraint: Some(constraint),
        } = classify_for_type_parameter_constraint(self.db, union_type)
            && constraint != union_type
        {
            let narrowed_constraint =
                self.narrow_by_property_truthiness(constraint, property_path, sense);
            if narrowed_constraint != constraint {
                return self.db.intersection2(union_type, narrowed_constraint);
            }
        }

        let _span = span!(
            Level::TRACE,
            "narrow_by_property_truthiness",
            union_type = union_type.0,
            property_path_len = property_path.len(),
            sense
        )
        .entered();

        let (_resolved, members, property_evaluator) =
            self.resolve_members_and_evaluator(union_type);

        let mut matching: Vec<TypeId> = Vec::new();

        for &member in &members {
            if member.is_any_or_unknown() {
                matching.push(member);
                continue;
            }

            let resolved_member = self.resolve_type(member);

            let intersection_members = intersection_list_id(self.db, resolved_member)
                .map(|members_id| self.db.type_list(members_id));

            let check_member_for_property = |check_type_id: TypeId| -> bool {
                let prop_type = match self.get_type_at_path(
                    check_type_id,
                    property_path,
                    &property_evaluator,
                ) {
                    Some(t) => t,
                    None => {
                        // Property doesn't exist -> undefined (falsy)
                        return !sense;
                    }
                };

                let resolved_prop_type = self.resolve_type(prop_type);

                // If it's the true branch, check if the property can be truthy
                // If it's the false branch, check if the property can be falsy
                if sense {
                    let narrowed = self.narrow_by_truthiness(resolved_prop_type);
                    narrowed != TypeId::NEVER
                } else {
                    let narrowed = self.narrow_to_falsy(resolved_prop_type);
                    narrowed != TypeId::NEVER
                }
            };

            let matches = if let Some(ref intersection) = intersection_members {
                intersection.iter().any(|&m| check_member_for_property(m))
            } else {
                check_member_for_property(resolved_member)
            };

            if matches {
                matching.push(member);
            }
        }

        if matching.is_empty() {
            return TypeId::NEVER;
        }

        if matching.len() == members.len() {
            return union_type;
        }

        union_or_single_preserve(self.db, matching)
    }

    /// - `union_type`: The union type to narrow
    /// - `property_path`: Path to the discriminant property (e.g., ["payload", "type"])
    /// - `literal_value`: The literal value to match
    pub fn narrow_by_discriminant(
        &self,
        union_type: TypeId,
        property_path: &[Atom],
        literal_value: TypeId,
    ) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_discriminant",
            union_type = union_type.0,
            property_path_len = property_path.len(),
            literal_value = literal_value.0
        )
        .entered();

        let (resolved_type, members, property_evaluator) =
            self.resolve_members_and_evaluator(union_type);

        trace!(
            "narrow_by_discriminant: union_type={}, resolved_type={}, property_path={:?}, literal_value={}",
            union_type.0, resolved_type.0, property_path, literal_value.0
        );

        trace!(
            "Narrowing union with {} members by discriminant property",
            members.len()
        );

        if property_path.len() == 1
            && let Some(fast_result) = self.fast_narrow_top_level_discriminant(
                union_type,
                &members,
                property_path[0],
                literal_value,
                true,
            )
        {
            return fast_result;
        }

        let mut matching: Vec<TypeId> = Vec::new();
        // Track whether any member actually has the discriminant property.
        // If no member has the property, discriminant narrowing is inapplicable
        // and we should return the original type instead of `never`.
        // This matches TSC behavior: `obj.nonExistent === value` does not
        // narrow `obj` to `never` when the property doesn't exist.
        let mut any_member_has_property = false;

        for &member in &members {
            // Special case: any and unknown always match
            if member.is_any_or_unknown() {
                trace!("Member {} is any/unknown, keeping in true branch", member.0);
                matching.push(member);
                continue;
            }

            // CRITICAL: Resolve Lazy types before checking for object shape
            // This ensures type aliases are resolved to their actual types
            let resolved_member = self.resolve_type(member);

            // Handle Intersection types: check all intersection members for the property
            let intersection_members = intersection_list_id(self.db, resolved_member)
                .map(|members_id| self.db.type_list(members_id));

            // Helper function to check if a type has a matching property at the path
            let check_member_for_property = |check_type_id: TypeId| -> Option<bool> {
                // Get the type at the property path
                let prop_type = match self.get_type_at_path(
                    check_type_id,
                    property_path,
                    &property_evaluator,
                ) {
                    Some(t) => t,
                    None => {
                        // Property doesn't exist on this member
                        trace!(
                            "Member {} does not have property path {:?}",
                            check_type_id.0, property_path
                        );
                        return None;
                    }
                };

                // CRITICAL: Resolve Lazy types in property type before comparison.
                // Property types like `E.A` may be stored as Lazy(DefId) references
                // that need to be resolved to their actual enum literal types.
                let resolved_prop_type = self.resolve_type(prop_type);

                // CRITICAL: Use is_subtype_of(literal_value, property_type)
                // NOT the reverse! This was the bug in the reverted commit.
                let matches = is_subtype_of(self.db, literal_value, resolved_prop_type);

                if matches {
                    trace!(
                        "Member {} has property path {:?} with type {}, literal {} matches",
                        check_type_id.0, property_path, prop_type.0, literal_value.0
                    );
                } else {
                    trace!(
                        "Member {} has property path {:?} with type {}, literal {} does not match",
                        check_type_id.0, property_path, prop_type.0, literal_value.0
                    );
                }

                Some(matches)
            };

            // Check for property match
            let has_property_match = if let Some(ref intersection) = intersection_members {
                // For Intersection: compute the effective property type by intersecting
                // property types from all members that have the property.
                // Example: X & { a: object } where X has a?: { aProp: string }
                //   X's `a` type = { aProp: string } | undefined
                //   { a: object }'s `a` type = object
                //   effective = ({ aProp: string } | undefined) & object = { aProp: string }
                //   So `a === undefined` does NOT match (correct).
                let prop_types: Vec<TypeId> = intersection
                    .iter()
                    .filter_map(|&m| {
                        self.get_type_at_path(m, property_path, &property_evaluator)
                            .map(|t| self.resolve_type(t))
                    })
                    .collect();
                if prop_types.is_empty() {
                    false
                } else {
                    any_member_has_property = true;
                    if prop_types.len() == 1 {
                        is_subtype_of(self.db, literal_value, prop_types[0])
                    } else {
                        let effective_type = self.db.intersection(prop_types);
                        is_subtype_of(self.db, literal_value, effective_type)
                    }
                }
            } else {
                // For non-Intersection: check the single member
                match check_member_for_property(resolved_member) {
                    Some(matches) => {
                        any_member_has_property = true;
                        matches
                    }
                    None => false,
                }
            };

            if has_property_match {
                matching.push(member);
            }
        }

        // Return result based on matches

        if matching.is_empty() {
            if !any_member_has_property {
                // No member has the discriminant property at all — this means the
                // comparison is against a non-existent property. TSC does not narrow
                // to `never` in this case; the type is left unchanged.
                trace!(
                    "No members have discriminant property {:?}, returning original type",
                    property_path
                );
                return union_type;
            }
            trace!("No members matched discriminant check, returning never");
            TypeId::NEVER
        } else if matching.len() == members.len() {
            trace!("All members matched, returning original");
            union_type
        } else if matching.len() == 1 {
            trace!("Narrowed to single member");
            matching[0]
        } else {
            trace!(
                "Narrowed to {} of {} members",
                matching.len(),
                members.len()
            );
            self.db.union(matching)
        }
    }

    /// Narrow a union type by excluding variants with a specific discriminant value.
    ///
    /// Example: `action.type !== "add"` narrows to `{ type: "remove", ... } | { type: "clear" }`
    ///
    /// Uses the inverse logic of `narrow_by_discriminant`: we exclude a member
    /// ONLY if its property is definitely and only the excluded value.
    ///
    /// For example:
    /// - prop is "a", exclude "a" -> exclude (property is always "a")
    /// - prop is "a" | "b", exclude "a" -> keep (could be "b")
    /// - prop doesn't exist -> keep (property doesn't match excluded value)
    ///
    /// # Arguments
    /// - `union_type`: The union type to narrow
    /// - `property_path`: Path to the discriminant property (e.g., ["payload", "type"])
    /// - `excluded_value`: The literal value to exclude
    pub fn narrow_by_excluding_discriminant(
        &self,
        union_type: TypeId,
        property_path: &[Atom],
        excluded_value: TypeId,
    ) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_excluding_discriminant",
            union_type = union_type.0,
            property_path_len = property_path.len(),
            excluded_value = excluded_value.0
        )
        .entered();

        let (_resolved, members, property_evaluator) =
            self.resolve_members_and_evaluator(union_type);

        trace!(
            "Excluding discriminant value {} from union with {} members",
            excluded_value.0,
            members.len()
        );

        if property_path.len() == 1
            && let Some(fast_result) = self.fast_narrow_top_level_discriminant(
                union_type,
                &members,
                property_path[0],
                excluded_value,
                false,
            )
        {
            return fast_result;
        }

        let mut remaining: Vec<TypeId> = Vec::new();

        for &member in &members {
            // Special case: any and unknown always kept (could have any property value)
            if member.is_any_or_unknown() {
                trace!(
                    "Member {} is any/unknown, keeping in false branch",
                    member.0
                );
                remaining.push(member);
                continue;
            }

            // CRITICAL: Resolve Lazy types before checking for object shape
            let resolved_member = self.resolve_type(member);

            // Handle Intersection types: check all intersection members for the property
            let intersection_members = intersection_list_id(self.db, resolved_member)
                .map(|members_id| self.db.type_list(members_id));

            // Helper function to check if a member should be excluded
            // Returns true if member should be KEPT (not excluded)
            let should_keep_member = |check_type_id: TypeId| -> bool {
                // Get the type at the property path
                let prop_type = match self.get_type_at_path(
                    check_type_id,
                    property_path,
                    &property_evaluator,
                ) {
                    Some(t) => t,
                    None => {
                        // Property doesn't exist - keep the member
                        trace!(
                            "Member {} does not have property path, keeping",
                            check_type_id.0
                        );
                        return true;
                    }
                };

                // CRITICAL: Resolve Lazy types in property type before comparison.
                let resolved_prop_type = self.resolve_type(prop_type);

                // Exclude member ONLY if property type is subtype of excluded value
                // This means the property is ALWAYS the excluded value
                // REVERSE of narrow_by_discriminant logic
                let should_exclude = is_subtype_of(self.db, resolved_prop_type, excluded_value);

                if should_exclude {
                    trace!(
                        "Member {} has property path type {} which is subtype of excluded {}, excluding",
                        check_type_id.0, prop_type.0, excluded_value.0
                    );
                    false // Member should be excluded
                } else {
                    trace!(
                        "Member {} has property path type {} which is not subtype of excluded {}, keeping",
                        check_type_id.0, prop_type.0, excluded_value.0
                    );
                    true // Member should be kept
                }
            };

            // Check if member should be kept
            let keep_member = if let Some(ref intersection) = intersection_members {
                // For Intersection: compute the effective property type by intersecting
                // property types from all members that have the property.
                // Then exclude only if the effective type is a subtype of the excluded value.
                // Example: X & { a: undefined } where X has a?: { aProp: string }
                //   X's `a` type = { aProp: string } | undefined
                //   { a: undefined }'s `a` type = undefined
                //   effective = ({ aProp: string } | undefined) & undefined = undefined
                //   undefined <: undefined → exclude (correct)
                let prop_types: Vec<TypeId> = intersection
                    .iter()
                    .filter_map(|&m| {
                        self.get_type_at_path(m, property_path, &property_evaluator)
                            .map(|t| self.resolve_type(t))
                    })
                    .collect();
                if prop_types.is_empty() {
                    true // no member has the property, keep
                } else if prop_types.len() == 1 {
                    !is_subtype_of(self.db, prop_types[0], excluded_value)
                } else {
                    let effective_type = self.db.intersection(prop_types);
                    !is_subtype_of(self.db, effective_type, excluded_value)
                }
            } else {
                // For non-Intersection: check the single member
                should_keep_member(resolved_member)
            };

            if keep_member {
                remaining.push(member);
            }
        }

        union_or_single_preserve(self.db, remaining)
    }

    /// Narrow a union type by excluding variants with any of the specified discriminant values.
    ///
    /// This is an optimized batch version of `narrow_by_excluding_discriminant` for switch statements.
    pub fn narrow_by_excluding_discriminant_values(
        &self,
        union_type: TypeId,
        property_path: &[Atom],
        excluded_values: &[TypeId],
    ) -> TypeId {
        if excluded_values.is_empty() {
            return union_type;
        }

        let _span = span!(
            Level::TRACE,
            "narrow_by_excluding_discriminant_values",
            union_type = union_type.0,
            property_path_len = property_path.len(),
            excluded_count = excluded_values.len()
        )
        .entered();

        let (_resolved, members, property_evaluator) =
            self.resolve_members_and_evaluator(union_type);

        // Put excluded values into a HashSet for O(1) lookup
        let excluded_set: FxHashSet<TypeId> = excluded_values.iter().copied().collect();

        let mut remaining: Vec<TypeId> = Vec::new();

        for &member in &members {
            if member.is_any_or_unknown() {
                remaining.push(member);
                continue;
            }

            let resolved_member = self.resolve_type(member);
            let intersection_members = intersection_list_id(self.db, resolved_member)
                .map(|members_id| self.db.type_list(members_id));

            // Helper to check if member should be kept
            let should_keep_member = |check_type_id: TypeId| -> bool {
                let prop_type = match self.get_type_at_path(
                    check_type_id,
                    property_path,
                    &property_evaluator,
                ) {
                    Some(t) => t,
                    None => return true, // Keep if property missing
                };

                let resolved_prop_type = self.resolve_type(prop_type);

                // Optimization: if property type is directly in excluded set (literal match)
                if excluded_set.contains(&resolved_prop_type) {
                    return false; // Exclude
                }

                // Subtype check for each excluded value
                for &excluded in excluded_values {
                    if is_subtype_of(self.db, resolved_prop_type, excluded) {
                        return false; // Exclude
                    }
                }
                true // Keep
            };

            let keep_member = if let Some(ref intersection) = intersection_members {
                intersection.iter().all(|&m| should_keep_member(m))
            } else {
                should_keep_member(resolved_member)
            };

            if keep_member {
                remaining.push(member);
            }
        }

        union_or_single_preserve(self.db, remaining)
    }
}
