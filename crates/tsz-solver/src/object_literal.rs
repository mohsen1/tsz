//! Object literal type construction.
//!
//! This module provides a builder for constructing object types from
//! property information, with support for:
//! - Property collection and merging
//! - Spread operators
//! - Contextual typing

use crate::TypeDatabase;
use crate::contextual::ContextualTypeContext;
use crate::types::{ObjectFlags, PropertyInfo, TypeId, TypeKey};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

/// Builder for constructing object literal types.
///
/// This is a solver component that handles the pure type construction
/// aspects of object literals, separate from AST traversal and error reporting.
pub struct ObjectLiteralBuilder<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> ObjectLiteralBuilder<'a> {
    /// Create a new object literal builder.
    pub fn new(db: &'a dyn TypeDatabase) -> Self {
        ObjectLiteralBuilder { db }
    }

    /// Build object type from properties.
    ///
    /// This creates a fresh object type with the given properties.
    /// The properties are sorted by name for consistent hashing.
    pub fn build_object_type(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.db.object_fresh(properties)
    }

    /// Build object type with index signature.
    ///
    /// Creates an object type with both properties and optional
    /// string/number index signatures.
    pub fn build_object_with_index(
        &self,
        properties: Vec<PropertyInfo>,
        string_index: Option<crate::types::IndexSignature>,
        number_index: Option<crate::types::IndexSignature>,
    ) -> TypeId {
        use crate::types::ObjectShape;
        self.db.object_with_index(ObjectShape {
            flags: ObjectFlags::FRESH_LITERAL,
            properties,
            string_index,
            number_index,
            symbol: None,
        })
    }

    /// Merge spread properties into base properties.
    ///
    /// Given a base set of properties and a spread type, extracts all properties
    /// from the spread type and merges them into the base (later properties override).
    ///
    /// Example:
    /// ```typescript
    /// const base = { x: 1 };
    /// const spread = { y: 2, x: 3 };
    /// // Result: { x: 3, y: 2 }
    /// ```
    pub fn merge_spread(
        &self,
        base_properties: Vec<PropertyInfo>,
        spread_type: TypeId,
    ) -> Vec<PropertyInfo> {
        let mut merged: FxHashMap<Atom, PropertyInfo> =
            base_properties.into_iter().map(|p| (p.name, p)).collect();

        // Extract properties from spread type
        for prop in self.extract_properties(spread_type) {
            merged.insert(prop.name, prop);
        }

        merged.into_values().collect()
    }

    /// Apply contextual typing to property types.
    ///
    /// When an object literal has a contextual type, each property value
    /// should be narrowed by the corresponding property type from the context.
    ///
    /// Example:
    /// ```typescript
    /// type Point = { x: number; y: number };
    /// const p: Point = { x: 1, y: '2' };  // Error: '2' is not assignable to number
    /// ```
    pub fn apply_contextual_types(
        &self,
        properties: Vec<PropertyInfo>,
        contextual: TypeId,
    ) -> Vec<PropertyInfo> {
        let ctx = ContextualTypeContext::with_expected(self.db, contextual);

        properties
            .into_iter()
            .map(|prop| {
                let prop_name = self.db.resolve_atom_ref(prop.name);
                let contextual_prop_type = ctx.get_property_type(&prop_name);

                if let Some(ctx_type) = contextual_prop_type {
                    PropertyInfo {
                        type_id: self.apply_contextual_type(prop.type_id, ctx_type),
                        ..prop
                    }
                } else {
                    prop
                }
            })
            .collect()
    }

    /// Collect all properties for object spread and spread-mutation paths.
    ///
    /// This is the solver-side public entrypoint used by query APIs for object
    /// spread property extraction, including `CheckerState::get_type_of_object_literal`.
    pub fn collect_spread_properties(&self, spread_type: TypeId) -> Vec<PropertyInfo> {
        self.extract_properties(spread_type)
    }

    /// Extract all properties from a type (for spread operations).
    ///
    /// This handles:
    /// - Object types
    /// - Callable types (function properties)
    /// - Intersection types (merge all properties)
    ///
    /// Returns an empty vec for types that don't have properties.
    fn extract_properties(&self, type_id: TypeId) -> Vec<PropertyInfo> {
        let Some(key) = self.db.lookup(type_id) else {
            return Vec::new();
        };

        match key {
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.to_vec()
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.db.callable_shape(shape_id);
                shape.properties.to_vec()
            }
            TypeKey::Intersection(list_id) => {
                let members = self.db.type_list(list_id);
                let mut merged: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();

                for &member in members.iter() {
                    for prop in self.extract_properties(member) {
                        merged.insert(prop.name, prop);
                    }
                }

                merged.into_values().collect()
            }
            _ => Vec::new(),
        }
    }

    /// Apply contextual type to a value type.
    ///
    /// Uses bidirectional type inference to narrow the value type
    /// based on the expected contextual type.
    fn apply_contextual_type(&self, value_type: TypeId, ctx_type: TypeId) -> TypeId {
        // If the value type is 'any' or the contextual type is 'any', no narrowing occurs
        if value_type == TypeId::ANY || ctx_type == TypeId::ANY {
            return value_type;
        }

        // If the value type already satisfies the contextual type, use the contextual type
        // This is the key insight from bidirectional typing
        //
        // FIX: Use CompatChecker (The Lawyer) instead of SubtypeChecker (The Judge)
        // to ensure TypeScript's assignability rules are applied during contextual typing.
        // This is critical for freshness checks (excess properties) and other TS rules.
        use crate::compat::CompatChecker;
        let mut checker = CompatChecker::new(self.db);

        if checker.is_assignable(value_type, ctx_type) {
            ctx_type
        } else {
            value_type
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_literal_tests.rs"]
mod tests;
