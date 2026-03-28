//! Property collection and merging for intersection types.
//!
//! This module provides utilities for collecting properties from intersection types
//! while handling Lazy/Ref resolution and avoiding infinite recursion.

use crate::relations::subtype::TypeResolver;
#[cfg(test)]
use crate::types::*;
use crate::types::{
    IndexSignature, IntrinsicKind, ObjectShape, PropertyInfo, TypeData, TypeId, TypeListId,
    Visibility,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

// Import TypeDatabase trait
use crate::caches::db::TypeDatabase;

/// Merge two visibility levels, returning the more restrictive one.
///
/// Ordering: Private > Protected > Public
const fn merge_visibility(a: Visibility, b: Visibility) -> Visibility {
    match (a, b) {
        (Visibility::Private, _) | (_, Visibility::Private) => Visibility::Private,
        (Visibility::Protected, _) | (_, Visibility::Protected) => Visibility::Protected,
        (Visibility::Public, Visibility::Public) => Visibility::Public,
    }
}

/// Result of collecting properties from an intersection type.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyCollectionResult {
    /// The intersection contains `any`, making the entire type `any`
    Any,
    /// The intersection contains only non-object types (never, unknown, primitives, etc.)
    NonObject,
    /// The intersection contains object properties
    Properties {
        properties: Vec<PropertyInfo>,
        string_index: Option<IndexSignature>,
        number_index: Option<IndexSignature>,
    },
}

/// Collect properties from an intersection type, recursively merging all members.
///
/// This function handles:
/// - Recursive traversal of intersection members
/// - Lazy/Ref type resolution
/// - Property type intersection (using raw intersection to avoid recursion)
/// - Optionality merging (required wins)
/// - Readonly merging (readonly is cumulative)
/// - Index signature merging
///
/// # Arguments
/// * `type_id` - The type to collect properties from (may be an intersection)
/// * `interner` - The type interner for type operations
/// * `resolver` - Type resolver for handling Lazy/Ref types
///
/// # Returns
/// A `PropertyCollectionResult` indicating whether the result is `Any`, non-object,
/// or contains actual properties.
///
/// # Important
/// - Call signatures are NOT collected (this is for properties only)
/// - Mapped types are NOT handled (input should be pre-lowered/evaluated)
/// - `any & T` always returns `Any` (commutative)
pub fn collect_properties<R>(
    type_id: TypeId,
    interner: &dyn TypeDatabase,
    resolver: &R,
) -> PropertyCollectionResult
where
    R: TypeResolver,
{
    let mut collector = PropertyCollector {
        interner,
        resolver,
        properties: Vec::new(),
        prop_index: FxHashMap::default(),
        string_index: None,
        number_index: None,
        seen: FxHashSet::default(),
        found_any: false,
    };
    collector.collect(type_id);

    // If we encountered Any at any point, the result is Any (commutative)
    if collector.found_any {
        return PropertyCollectionResult::Any;
    }

    // If no properties were collected, return NonObject
    if collector.properties.is_empty()
        && collector.string_index.is_none()
        && collector.number_index.is_none()
    {
        return PropertyCollectionResult::NonObject;
    }

    // Sort properties by name to maintain interner invariants
    collector.properties.sort_by_key(|p| p.name.0);

    PropertyCollectionResult::Properties {
        properties: collector.properties,
        string_index: collector.string_index,
        number_index: collector.number_index,
    }
}

/// Helper function to resolve Lazy types via DefId
fn resolve_type<R>(type_id: TypeId, interner: &dyn TypeDatabase, resolver: &R) -> TypeId
where
    R: TypeResolver,
{
    use crate::visitor::lazy_def_id;

    if let Some(def_id) = lazy_def_id(interner, type_id) {
        resolver.resolve_lazy(def_id, interner).unwrap_or(type_id)
    } else {
        type_id
    }
}

/// Property collector for intersection types.
///
/// Recursively walks intersection members and collects all properties,
/// merging properties with the same name using intersection types.
struct PropertyCollector<'a, R> {
    interner: &'a dyn TypeDatabase,
    resolver: &'a R,
    properties: Vec<PropertyInfo>,
    /// Maps property name (Atom) to index in `properties` for O(1) lookup during merge
    prop_index: FxHashMap<Atom, usize>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    /// Prevent infinite recursion for circular intersections like: type T = { a: number } & T
    seen: FxHashSet<TypeId>,
    /// Track if we encountered Any (makes the whole result Any, commutative)
    found_any: bool,
}

impl<'a, R: TypeResolver> PropertyCollector<'a, R> {
    fn collect(&mut self, type_id: TypeId) {
        // Prevent infinite recursion
        if !self.seen.insert(type_id) {
            return;
        }

        // 1. Resolve Lazy/Ref
        let resolved = resolve_type(type_id, self.interner, self.resolver);

        // 2. Handle different type variants
        match self.interner.lookup(resolved) {
            Some(TypeData::Intersection(members_id)) => {
                // Recursively collect from all intersection members
                for &member in self.interner.type_list(members_id).iter() {
                    self.collect(member);
                }
            }
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                self.merge_shape(&shape);
            }
            // Any type in intersection makes everything Any (commutative)
            Some(TypeData::Intrinsic(IntrinsicKind::Any)) => {
                self.found_any = true;
            }
            // Type parameter: collect properties from its constraint
            Some(TypeData::TypeParameter(info)) => {
                if let Some(constraint) = info.constraint {
                    self.collect(constraint);
                }
            }
            // Conditional type: collect properties from its default constraint.
            // For Extract-like patterns (T extends U ? T : never), the constraint
            // is T & U, so we get U's properties. For general patterns, the
            // constraint is true_type | false_type (union of both branches).
            // This matches tsc's getApparentType → getBaseConstraintOfType →
            // getConstraintOfConditionalType for conditional types.
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner.conditional_type(cond_id);
                let constraint = if cond.true_type == cond.check_type {
                    // Extract-like: T extends U ? T : never → T & U
                    self.interner
                        .intersection2(cond.check_type, cond.extends_type)
                } else {
                    // General: union of both branches
                    self.interner.union2(cond.true_type, cond.false_type)
                };
                self.collect(constraint);
            }
            // Union: collect common properties (present in ALL members)
            Some(TypeData::Union(members_id)) => {
                self.collect_union_common(members_id);
            }
            // Never in intersection makes the whole thing Never
            // This is handled by the caller, not here
            _ => {
                // Not an object or intersection - ignore (call signatures, primitives, etc.)
            }
        }
    }

    /// Collect common properties from all union members.
    /// Only properties present in ALL members are included.
    /// Property types become the union of the individual types.
    fn collect_union_common(&mut self, members_id: TypeListId) {
        let member_list = self.interner.type_list(members_id);
        if member_list.is_empty() {
            return;
        }

        // Collect properties from each union member using sub-collectors
        let mut member_props: Vec<PropertyCollectionResult> = Vec::new();
        for &member in member_list.iter() {
            let result = collect_properties(member, self.interner, self.resolver);
            member_props.push(result);
        }

        // If any member is Any, the whole union is Any
        if member_props
            .iter()
            .any(|r| matches!(r, PropertyCollectionResult::Any))
        {
            self.found_any = true;
            return;
        }

        // Collect property names present in ALL members
        // Start with first member's property names, intersect with rest
        let first = match &member_props[0] {
            PropertyCollectionResult::Properties { properties, .. } => properties,
            _ => return, // First member has no properties
        };

        // For each property in the first member, check if it's in all others
        for prop in first {
            let mut present_in_all = true;
            let mut type_ids = vec![prop.type_id];
            let mut all_optional = prop.optional;
            let mut any_readonly = prop.readonly;

            for member_result in member_props.iter().skip(1) {
                match member_result {
                    PropertyCollectionResult::Properties { properties, .. } => {
                        if let Some(other_prop) = PropertyInfo::find_in_slice(properties, prop.name)
                        {
                            type_ids.push(other_prop.type_id);
                            all_optional = all_optional && other_prop.optional;
                            any_readonly = any_readonly || other_prop.readonly;
                        } else {
                            present_in_all = false;
                            break;
                        }
                    }
                    _ => {
                        present_in_all = false;
                        break;
                    }
                }
            }

            if present_in_all {
                // Create union type for the property
                let union_type = if type_ids.len() == 1 {
                    type_ids[0]
                } else {
                    self.interner.union(type_ids)
                };

                // Merge into our properties
                if let Some(&idx) = self.prop_index.get(&prop.name) {
                    let existing = &mut self.properties[idx];
                    existing.type_id = self
                        .interner
                        .intersect_types_raw2(existing.type_id, union_type);
                    existing.optional = existing.optional && all_optional;
                    existing.readonly = existing.readonly || any_readonly;
                } else {
                    let new_idx = self.properties.len();
                    self.prop_index.insert(prop.name, new_idx);
                    self.properties.push(PropertyInfo {
                        name: prop.name,
                        type_id: union_type,
                        write_type: union_type,
                        optional: all_optional,
                        readonly: any_readonly,
                        visibility: prop.visibility,
                        is_method: prop.is_method,
                        is_class_prototype: prop.is_class_prototype,
                        parent_id: prop.parent_id,
                        declaration_order: 0,
                    });
                }
            }
        }
    }

    fn merge_shape(&mut self, shape: &ObjectShape) {
        // Merge properties using HashMap index for O(1) lookup
        for prop in &shape.properties {
            if let Some(&idx) = self.prop_index.get(&prop.name) {
                let existing = &mut self.properties[idx];
                // TS Rule: Intersect types (using raw to avoid recursion)
                existing.type_id = self
                    .interner
                    .intersect_types_raw2(existing.type_id, prop.type_id);
                // TS Rule: Optional if ALL are optional (required wins)
                existing.optional = existing.optional && prop.optional;
                // TS Rule: Readonly only if ALL are readonly (writable wins)
                // { readonly a: number } & { a: number } = { a: number }
                existing.readonly = existing.readonly && prop.readonly;
                // Write type: if writable, use read type to avoid NONE sentinels
                // from readonly members (NONE & number = "error & number").
                if !existing.readonly {
                    existing.write_type = existing.type_id;
                } else {
                    existing.write_type = self
                        .interner
                        .intersect_types_raw2(existing.write_type, prop.write_type);
                }
                // Merge visibility: use the more restrictive one (private > protected > public)
                existing.visibility = merge_visibility(existing.visibility, prop.visibility);
                // is_method: if one is a method, treat as property (more general)
                existing.is_method = existing.is_method && prop.is_method;
            } else {
                let new_idx = self.properties.len();
                self.prop_index.insert(prop.name, new_idx);
                self.properties.push(prop.clone());
            }
        }

        // Merge string index signature
        if let Some(ref idx) = shape.string_index {
            if let Some(existing) = &mut self.string_index {
                // Intersect value types
                existing.value_type = self
                    .interner
                    .intersect_types_raw2(existing.value_type, idx.value_type);
                // Readonly only if ALL are readonly
                existing.readonly = existing.readonly && idx.readonly;
            } else {
                self.string_index = Some(*idx);
            }
        }

        // Merge number index signature
        if let Some(ref idx) = shape.number_index {
            if let Some(existing) = &mut self.number_index {
                // Intersect value types
                existing.value_type = self
                    .interner
                    .intersect_types_raw2(existing.value_type, idx.value_type);
                // Readonly only if ALL are readonly
                existing.readonly = existing.readonly && idx.readonly;
            } else {
                self.number_index = Some(*idx);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/objects_tests.rs"]
mod tests;
