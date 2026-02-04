//! Object literal type construction.
//!
//! This module provides a builder for constructing object types from
//! property information, with support for:
//! - Property collection and merging
//! - Spread operators
//! - Contextual typing

use crate::interner::Atom;
use crate::solver::TypeDatabase;
use crate::solver::contextual::ContextualTypeContext;
use crate::solver::types::{ObjectFlags, PropertyInfo, TypeId, TypeKey};
use rustc_hash::FxHashMap;

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
        string_index: Option<crate::solver::types::IndexSignature>,
        number_index: Option<crate::solver::types::IndexSignature>,
    ) -> TypeId {
        use crate::solver::types::ObjectShape;
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
        use crate::solver::compat::CompatChecker;
        let mut checker = CompatChecker::new(self.db);

        if checker.is_assignable(value_type, ctx_type) {
            ctx_type
        } else {
            value_type
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::TypeInterner;
    use crate::solver::types::Visibility;

    #[test]
    fn test_build_object_type() {
        let db = TypeInterner::new();
        let builder = ObjectLiteralBuilder::new(&db);

        let properties = vec![PropertyInfo {
            name: db.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }];

        let obj_type = builder.build_object_type(properties);

        let key = db.lookup(obj_type).unwrap();
        assert!(matches!(key, TypeKey::Object(_)));
    }

    #[test]
    fn test_merge_spread() {
        let db = TypeInterner::new();
        let builder = ObjectLiteralBuilder::new(&db);

        // Create base object { x: number }
        let base_props = vec![PropertyInfo {
            name: db.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }];

        // Create spread object { y: string, x: boolean }
        let spread_props = vec![
            PropertyInfo {
                name: db.intern_string("y"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            },
            PropertyInfo {
                name: db.intern_string("x"),
                type_id: TypeId::BOOLEAN,
                write_type: TypeId::BOOLEAN,
                optional: false,
                readonly: false,
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            },
        ];
        let spread_type = db.object(spread_props);

        // Merge: { x: boolean, y: string } (x is overridden)
        let merged = builder.merge_spread(base_props, spread_type);

        assert_eq!(merged.len(), 2);

        let x_prop = merged
            .iter()
            .find(|p| db.resolve_atom_ref(p.name) == "x".into())
            .unwrap();
        assert_eq!(x_prop.type_id, TypeId::BOOLEAN);

        let y_prop = merged
            .iter()
            .find(|p| db.resolve_atom_ref(p.name) == "y".into())
            .unwrap();
        assert_eq!(y_prop.type_id, TypeId::STRING);
    }

    #[test]
    fn test_apply_contextual_types() {
        let db = TypeInterner::new();
        let builder = ObjectLiteralBuilder::new(&db);

        // Create contextual type { x: number }
        let ctx_type = db.object(vec![PropertyInfo {
            name: db.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        // Create properties { x: 1 } (where 1 is a literal number type)
        // For simplicity, we'll just use NUMBER type
        let properties = vec![PropertyInfo {
            name: db.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }];

        let contextualized = builder.apply_contextual_types(properties, ctx_type);

        assert_eq!(contextualized.len(), 1);
        assert_eq!(contextualized[0].type_id, TypeId::NUMBER);
    }

    #[test]
    fn test_extract_properties_from_intersection() {
        let db = TypeInterner::new();
        let builder = ObjectLiteralBuilder::new(&db);

        // Create intersection of { x: number } & { y: string }
        let type1 = db.object(vec![PropertyInfo {
            name: db.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        let type2 = db.object(vec![PropertyInfo {
            name: db.intern_string("y"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        let intersection = db.intersection2(type1, type2);

        let props = builder.extract_properties(intersection);

        assert_eq!(props.len(), 2);
        assert!(
            props
                .iter()
                .any(|p| db.resolve_atom_ref(p.name) == "x".into())
        );
        assert!(
            props
                .iter()
                .any(|p| db.resolve_atom_ref(p.name) == "y".into())
        );
    }
}
