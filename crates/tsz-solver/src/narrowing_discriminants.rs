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

use crate::narrowing::{DiscriminantInfo, NarrowingContext, union_or_single_preserve};
use crate::operations_property::{PropertyAccessEvaluator, PropertyAccessResult};
use crate::subtype::is_subtype_of;
use crate::type_queries::{
    LiteralValueKind, UnionMembersKind, classify_for_literal_value, classify_for_union_members,
};
use crate::types::{PropertyLookup, TypeId};
use crate::visitor::{
    intersection_list_id, is_literal_type_db, object_shape_id, object_with_index_shape_id,
    union_list_id,
};
use rustc_hash::FxHashSet;
use tracing::{Level, span, trace};
use tsz_common::interner::Atom;

impl<'a> NarrowingContext<'a> {
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
        let mut member_props: Vec<Vec<(Atom, TypeId)>> = Vec::new();

        for &member in members.iter() {
            if let Some(shape_id) = object_shape_id(self.db, member) {
                let shape = self.db.object_shape(shape_id);
                let props_vec: Vec<(Atom, TypeId)> = shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id))
                    .collect();

                // Track all property names
                for (name, _) in &props_vec {
                    if !all_properties.contains(name) {
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
            let mut seen_literals: Vec<TypeId> = Vec::new();

            for (i, props) in member_props.iter().enumerate() {
                // Find this property in the member
                let prop_type = props
                    .iter()
                    .find(|(name, _)| name == prop_name)
                    .map(|(_, ty)| *ty);

                match prop_type {
                    Some(ty) => {
                        // Must be a literal type
                        if is_literal_type_db(self.db, ty) {
                            // Must be unique among members
                            if seen_literals.contains(&ty) {
                                is_discriminant = false;
                                break;
                            }
                            seen_literals.push(ty);
                            variants.push((ty, members[i]));
                        } else {
                            is_discriminant = false;
                            break;
                        }
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

            // Use resolve_property_access for proper optional property handling
            // This correctly handles properties that are optional (prop?: type)
            let prop_name_arc = self.db.resolve_atom_ref(prop_name);
            let prop_name_str = prop_name_arc.as_ref();
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
            return cached;
        }

        // Cache the resolved property type so hot paths avoid an extra resolve pass.
        let result = self
            .get_top_level_property_type_fast_uncached(type_id, property)
            .map(|prop_type| self.resolve_type(prop_type));
        self.cache.property_cache.borrow_mut().insert(key, result);
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
                return self.db.intersection(vec![type_id, narrowed_constraint]);
            }
        }

        if is_true_branch {
            self.narrow_by_discriminant(type_id, prop_path, literal_type)
        } else {
            self.narrow_by_excluding_discriminant(type_id, prop_path, literal_type)
        }
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

        // CRITICAL: Resolve Lazy types before checking for union members
        // This ensures type aliases are resolved to their actual union types
        let resolved_type = self.resolve_type(union_type);

        trace!(
            "narrow_by_discriminant: union_type={}, resolved_type={}, property_path={:?}, literal_value={}",
            union_type.0, resolved_type.0, property_path, literal_value.0
        );

        // CRITICAL FIX: Use classify_for_union_members instead of union_list_id
        // This correctly handles intersections containing unions, nested unions, etc.
        let single_member_storage: Vec<TypeId>;
        let members: &[TypeId] = match classify_for_union_members(self.db, resolved_type) {
            UnionMembersKind::Union(members_list) => {
                // Convert Vec to slice for iteration
                single_member_storage = members_list.into_iter().collect::<Vec<_>>();
                &single_member_storage
            }
            UnionMembersKind::NotUnion => {
                // Not a union at all - treat as single member
                single_member_storage = vec![resolved_type];
                &single_member_storage
            }
        };

        trace!("narrow_by_discriminant: members={:?}", members);

        trace!(
            "Checking {} member(s) for discriminant match",
            members.len()
        );

        trace!(
            "Narrowing union with {} members by discriminant property",
            members.len()
        );

        if property_path.len() == 1
            && let Some(fast_result) = self.fast_narrow_top_level_discriminant(
                union_type,
                members,
                property_path[0],
                literal_value,
                true,
            )
        {
            return fast_result;
        }

        let mut matching: Vec<TypeId> = Vec::new();
        let property_evaluator = match self.resolver {
            Some(resolver) => PropertyAccessEvaluator::with_resolver(self.db, resolver),
            None => PropertyAccessEvaluator::new(self.db),
        };

        for &member in members {
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
                .map(|members_id| self.db.type_list(members_id).to_vec());

            // Helper function to check if a type has a matching property at the path
            let check_member_for_property = |check_type_id: TypeId| -> bool {
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
                        return false;
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

                matches
            };

            // Check for property match
            let has_property_match = if let Some(ref intersection) = intersection_members {
                // For Intersection: at least one member must have the property
                intersection.iter().any(|&m| check_member_for_property(m))
            } else {
                // For non-Intersection: check the single member
                check_member_for_property(resolved_member)
            };

            if has_property_match {
                matching.push(member);
            }
        }

        // Return result based on matches

        if matching.is_empty() {
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

        // CRITICAL: Resolve Lazy types before checking for union members
        // This ensures type aliases are resolved to their actual union types
        let resolved_type = self.resolve_type(union_type);

        // CRITICAL FIX: Use classify_for_union_members instead of union_list_id
        // This correctly handles intersections containing unions, nested unions, etc.
        // Consistent with narrow_by_discriminant.
        let single_member_storage: Vec<TypeId>;
        let members: &[TypeId] = match classify_for_union_members(self.db, resolved_type) {
            UnionMembersKind::Union(members_list) => {
                single_member_storage = members_list.into_iter().collect::<Vec<_>>();
                &single_member_storage
            }
            UnionMembersKind::NotUnion => {
                single_member_storage = vec![resolved_type];
                &single_member_storage
            }
        };

        trace!(
            "Excluding discriminant value {} from union with {} members",
            excluded_value.0,
            members.len()
        );

        if property_path.len() == 1
            && let Some(fast_result) = self.fast_narrow_top_level_discriminant(
                union_type,
                members,
                property_path[0],
                excluded_value,
                false,
            )
        {
            return fast_result;
        }

        let mut remaining: Vec<TypeId> = Vec::new();
        let property_evaluator = match self.resolver {
            Some(resolver) => PropertyAccessEvaluator::with_resolver(self.db, resolver),
            None => PropertyAccessEvaluator::new(self.db),
        };

        for &member in members {
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
                .map(|members_id| self.db.type_list(members_id).to_vec());

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
                // CRITICAL: For Intersection exclusion, use ALL not ANY
                // If ANY intersection member has the excluded property value,
                // the ENTIRE intersection must be excluded.
                // Example: { kind: "A" } & { data: string } with x.kind !== "A"
                //   -> { kind: "A" } has "A" (excluded) -> exclude entire intersection
                intersection.iter().all(|&m| should_keep_member(m))
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

        let resolved_type = self.resolve_type(union_type);

        let single_member_storage: Vec<TypeId>;
        let members: &[TypeId] = match classify_for_union_members(self.db, resolved_type) {
            UnionMembersKind::Union(members_list) => {
                single_member_storage = members_list.into_iter().collect::<Vec<_>>();
                &single_member_storage
            }
            UnionMembersKind::NotUnion => {
                single_member_storage = vec![resolved_type];
                &single_member_storage
            }
        };

        // Put excluded values into a HashSet for O(1) lookup
        let excluded_set: FxHashSet<TypeId> = excluded_values.iter().copied().collect();

        let mut remaining: Vec<TypeId> = Vec::new();
        let property_evaluator = match self.resolver {
            Some(resolver) => PropertyAccessEvaluator::with_resolver(self.db, resolver),
            None => PropertyAccessEvaluator::new(self.db),
        };

        for &member in members {
            if member.is_any_or_unknown() {
                remaining.push(member);
                continue;
            }

            let resolved_member = self.resolve_type(member);
            let intersection_members = intersection_list_id(self.db, resolved_member)
                .map(|members_id| self.db.type_list(members_id).to_vec());

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
