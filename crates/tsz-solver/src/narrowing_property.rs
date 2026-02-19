//! Property-based type narrowing.
//!
//! This module contains narrowing methods for:
//! - `in` operator narrowing (property presence check)
//! - Property type lookup for narrowing
//! - Object-like type detection for instanceof support

use crate::narrowing::NarrowingContext;
use crate::types::{ObjectShapeId, PropertyInfo, TypeId, Visibility};
use crate::visitor::{
    intersection_list_id, object_shape_id, object_with_index_shape_id, type_param_info,
    union_list_id,
};
use tracing::{Level, span, trace};
use tsz_common::interner::Atom;

impl<'a> NarrowingContext<'a> {
    /// Check if a type is object-like (has object structure)
    ///
    /// This is used to determine if two types can form an intersection
    /// for instanceof narrowing when they're not directly assignable.
    pub(crate) fn are_object_like(&self, type_id: TypeId) -> bool {
        use crate::types::TypeData;

        match self.db.lookup(type_id) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Function(_)
                | TypeData::Callable(_),
            ) => true,

            // Interface and class types (which are object-like)
            Some(TypeData::Application(_)) => {
                // Check if the application type has construct signatures or object structure
                use crate::type_queries_extended::InstanceTypeKind;
                use crate::type_queries_extended::classify_for_instance_type;

                matches!(
                    classify_for_instance_type(self.db, type_id),
                    InstanceTypeKind::Callable(_) | InstanceTypeKind::Function(_)
                )
            }

            // Type parameters - check their constraint
            Some(TypeData::TypeParameter(info)) => {
                // For instanceof, generics with object constraints are treated as object-like
                // This allows intersection narrowing for cases like: T & MyClass
                info.constraint.is_none_or(|c| self.are_object_like(c))
            }

            // Intersection of object types
            Some(TypeData::Intersection(members)) => {
                let members = self.db.type_list(members);
                members.iter().any(|&member| self.are_object_like(member))
            }

            _ => false,
        }
    }

    /// Narrow a type based on an `in` operator check.
    ///
    /// Example: `"a" in x` narrows `A | B` to include only types that have property `a`
    pub fn narrow_by_property_presence(
        &self,
        source_type: TypeId,
        property_name: Atom,
        present: bool,
    ) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_property_presence",
            source_type = source_type.0,
            ?property_name,
            present
        )
        .entered();

        // Handle special cases
        if source_type == TypeId::ANY {
            trace!("Source type is ANY, returning unchanged");
            return TypeId::ANY;
        }

        if source_type == TypeId::NEVER {
            trace!("Source type is NEVER, returning unchanged");
            return TypeId::NEVER;
        }

        if source_type == TypeId::UNKNOWN {
            if !present {
                // False branch: property is not present. Since unknown could be anything,
                // it remains unknown in the false branch.
                trace!("UNKNOWN in false branch for in operator, returning UNKNOWN");
                return TypeId::UNKNOWN;
            }

            // For unknown, narrow to object & { [prop]: unknown }
            // This matches TypeScript's behavior where `in` check on unknown
            // narrows to object type with the property
            let prop_type = TypeId::UNKNOWN;
            let required_prop = PropertyInfo {
                name: property_name,
                type_id: prop_type,
                write_type: prop_type,
                optional: false, // Property becomes required after `in` check
                readonly: false,
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            };
            let filter_obj = self.db.object(vec![required_prop]);
            let narrowed = self.db.intersection2(TypeId::OBJECT, filter_obj);
            trace!("Narrowing unknown to object & property = {}", narrowed.0);
            return narrowed;
        }

        // Handle type parameters: narrow the constraint and intersect if changed
        if let Some(type_param_info) = type_param_info(self.db, source_type) {
            if let Some(constraint) = type_param_info.constraint
                && constraint != source_type
            {
                let narrowed_constraint =
                    self.narrow_by_property_presence(constraint, property_name, present);
                if narrowed_constraint != constraint {
                    trace!(
                        "Type parameter constraint narrowed from {} to {}, creating intersection",
                        constraint.0, narrowed_constraint.0
                    );
                    return self.db.intersection2(source_type, narrowed_constraint);
                }
            }
            // Type parameter with no constraint or unchanged constraint
            trace!("Type parameter unchanged, returning source");
            return source_type;
        }

        // If source is a union, filter members based on property presence
        if let Some(members_id) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members_id);
            trace!(
                "Checking property {} in union with {} members",
                self.db.resolve_atom_ref(property_name),
                members.len()
            );

            let matching: Vec<TypeId> = members
                .iter()
                .map(|&member| {
                    // CRITICAL: Resolve Lazy types for each member
                    let resolved_member = self.resolve_type(member);

                    let has_property = self.type_has_property(resolved_member, property_name);
                    if present {
                        // Positive: "prop" in member
                        if has_property {
                            // Property exists: Keep the member as-is
                            // CRITICAL: For union narrowing, we don't modify the member type
                            // We just filter to keep only members that have the property
                            member
                        } else {
                            // Property not found: Exclude member (return NEVER)
                            // Per TypeScript: "prop in x" being true means x MUST have the property
                            // If x doesn't have it (and no index signature), narrow to never
                            TypeId::NEVER
                        }
                    } else {
                        // Negative: !("prop" in member)
                        // Exclude member ONLY if property is required
                        if self.is_property_required(resolved_member, property_name) {
                            return TypeId::NEVER;
                        }
                        // Keep member (no required property found, or property is optional)
                        member
                    }
                })
                .collect();

            // CRITICAL FIX: Filter out NEVER types before creating the union
            // When a union member doesn't have the required property, it becomes NEVER
            // and should be EXCLUDED from the result, not included in the union
            let matching_non_never: Vec<TypeId> = matching
                .into_iter()
                .filter(|&t| t != TypeId::NEVER)
                .collect();

            if matching_non_never.is_empty() {
                trace!("All members were NEVER, returning NEVER");
                return TypeId::NEVER;
            } else if matching_non_never.len() == 1 {
                trace!(
                    "Found single member after filtering, returning {}",
                    matching_non_never[0].0
                );
                return matching_non_never[0];
            }
            trace!("Created union with {} members", matching_non_never.len());
            return self.db.union(matching_non_never);
        }

        // For non-union types, check if the property exists
        // CRITICAL: Resolve Lazy types before checking
        let resolved_type = self.resolve_type(source_type);
        let has_property = self.type_has_property(resolved_type, property_name);

        if present {
            // Positive: "prop" in x
            if has_property {
                // Property exists: Promote to required
                let prop_type = self.get_property_type(resolved_type, property_name);
                let required_prop = PropertyInfo {
                    name: property_name,
                    type_id: prop_type.unwrap_or(TypeId::UNKNOWN),
                    write_type: prop_type.unwrap_or(TypeId::UNKNOWN),
                    optional: false,
                    readonly: false,
                    is_method: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                };
                let filter_obj = self.db.object(vec![required_prop]);
                self.db.intersection2(source_type, filter_obj)
            } else {
                // Property not found: Narrow to never
                // Per TypeScript: "prop in x" being true means x MUST have the property
                // If x doesn't have it (and no index signature), narrow to never
                TypeId::NEVER
            }
        } else {
            // Negative: !("prop" in x)
            // Exclude ONLY if property is required (not optional)
            if self.is_property_required(resolved_type, property_name) {
                return TypeId::NEVER;
            }
            // Keep source_type (no required property found, or property is optional)
            source_type
        }
    }

    /// Check if a type has a specific property.
    ///
    /// Returns true if the type has the property (required or optional),
    /// or has an index signature that would match the property.
    pub(crate) fn type_has_property(&self, type_id: TypeId, property_name: Atom) -> bool {
        self.get_property_type(type_id, property_name).is_some()
    }

    /// Check if a property exists and is required on a type.
    ///
    /// Returns true if the property is required (not optional).
    /// This is used for negative narrowing: `!("prop" in x)` should
    /// exclude types where `prop` is required.
    pub(crate) fn is_property_required(&self, type_id: TypeId, property_name: Atom) -> bool {
        let resolved_type = self.resolve_type(type_id);

        // Helper to check a specific shape
        let check_shape = |shape_id: ObjectShapeId| -> bool {
            let shape = self.db.object_shape(shape_id);
            if let Some(prop) = shape.properties.iter().find(|p| p.name == property_name) {
                return !prop.optional;
            }
            false
        };

        // Check standard object shape
        if let Some(shape_id) = object_shape_id(self.db, resolved_type)
            && check_shape(shape_id)
        {
            return true;
        }

        // Check object with index shape (CRITICAL for interfaces/classes)
        if let Some(shape_id) = object_with_index_shape_id(self.db, resolved_type)
            && check_shape(shape_id)
        {
            return true;
        }

        // Check intersection members
        // If ANY member requires it, the intersection requires it
        if let Some(members_id) = intersection_list_id(self.db, resolved_type) {
            let members = self.db.type_list(members_id);
            return members
                .iter()
                .any(|&m| self.is_property_required(m, property_name));
        }

        false
    }

    /// Get the type of a property if it exists.
    ///
    /// Returns Some(type) if the property exists, None otherwise.
    pub(crate) fn get_property_type(&self, type_id: TypeId, property_name: Atom) -> Option<TypeId> {
        // CRITICAL: Resolve Lazy types before checking for properties
        // This ensures type aliases are resolved to their actual types
        let resolved_type = self.resolve_type(type_id);

        // Check intersection types - property exists if ANY member has it
        if let Some(members_id) = intersection_list_id(self.db, resolved_type) {
            let members = self.db.type_list(members_id);
            // Return the type from the first member that has the property
            for &member in members.iter() {
                // Resolve each member in the intersection
                let resolved_member = self.resolve_type(member);
                if let Some(prop_type) = self.get_property_type(resolved_member, property_name) {
                    return Some(prop_type);
                }
            }
            return None;
        }

        // Check object shape
        if let Some(shape_id) = object_shape_id(self.db, resolved_type) {
            let shape = self.db.object_shape(shape_id);

            // Check if the property exists in the object's properties
            if let Some(prop) = shape.properties.iter().find(|p| p.name == property_name) {
                return Some(prop.type_id);
            }

            // Check index signatures
            // If the object has a string index signature, it has any string property
            if let Some(ref string_idx) = shape.string_index {
                // String index signature matches any string property
                return Some(string_idx.value_type);
            }

            // If the object has a number index signature and the property name is numeric
            if let Some(ref number_idx) = shape.number_index {
                let prop_str = self.db.resolve_atom_ref(property_name);
                if prop_str.chars().all(|c| c.is_ascii_digit()) {
                    return Some(number_idx.value_type);
                }
            }

            return None;
        }

        // Check object with index signature
        if let Some(shape_id) = object_with_index_shape_id(self.db, resolved_type) {
            let shape = self.db.object_shape(shape_id);

            // Check properties first
            if let Some(prop) = shape.properties.iter().find(|p| p.name == property_name) {
                return Some(prop.type_id);
            }

            // Check index signatures
            if let Some(ref string_idx) = shape.string_index {
                return Some(string_idx.value_type);
            }

            if let Some(ref number_idx) = shape.number_index {
                let prop_str = self.db.resolve_atom_ref(property_name);
                if prop_str.chars().all(|c| c.is_ascii_digit()) {
                    return Some(number_idx.value_type);
                }
            }

            return None;
        }

        // For other types (functions, classes, arrays, etc.), assume they don't have arbitrary properties
        // unless they have been handled above (object shapes, etc.)
        None
    }
}
