//! Property collection and merging for intersection types.
//!
//! This module provides utilities for collecting properties from intersection types
//! while handling Lazy/Ref resolution and avoiding infinite recursion.

use crate::solver::subtype::TypeResolver;
use crate::solver::types::*;
use rustc_hash::FxHashSet;

// Import TypeDatabase trait
use crate::solver::db::TypeDatabase;

/// Result of property collection from intersection types.
///
/// This enum represents the possible outcomes when collecting properties
/// from an intersection type for subtyping checks.
#[derive(Debug)]
pub enum PropertyCollectionResult {
    /// The intersection contains `any`, making all properties effectively `any`.
    Any,
    /// The type is not an object type (e.g., primitive, function, etc.).
    NonObject,
    /// The type has object properties with optional index signatures.
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
/// A `PropertyCollectionResult` indicating whether the type is `Any`, a non-object type,
/// or has object properties with optional index signatures.
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
        string_index: None,
        number_index: None,
        seen: FxHashSet::default(),
        has_any: false,
    };
    collector.collect(type_id);

    // If we encountered Any, the entire result is Any
    if collector.has_any {
        return PropertyCollectionResult::Any;
    }

    // If no properties were collected, this is a non-object type
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

/// Helper function to resolve Lazy and Ref types
fn resolve_type<R>(type_id: TypeId, interner: &dyn TypeDatabase, resolver: &R) -> TypeId
where
    R: TypeResolver,
{
    use crate::solver::visitor::{lazy_def_id, ref_symbol};

    // Handle DefId-based Lazy types (new API)
    if let Some(def_id) = lazy_def_id(interner, type_id) {
        return resolver.resolve_lazy(def_id, interner).unwrap_or(type_id);
    }

    // Handle legacy SymbolRef-based types (old API)
    if let Some(symbol) = ref_symbol(interner, type_id) {
        if let Some(def_id) = resolver.symbol_to_def_id(symbol) {
            resolver.resolve_lazy(def_id, interner).unwrap_or(type_id)
        } else {
            #[allow(deprecated)]
            resolver.resolve_ref(symbol, interner).unwrap_or(type_id)
        }
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
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    /// Prevent infinite recursion for circular intersections like: type T = { a: number } & T
    seen: FxHashSet<TypeId>,
    /// Tracks if Any was encountered in the intersection
    has_any: bool,
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
            Some(TypeKey::Intersection(members_id)) => {
                // Recursively collect from all intersection members
                for &member in self.interner.type_list(members_id).iter() {
                    self.collect(member);
                }
            }
            Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                self.merge_shape(&shape);
            }
            // Any type in intersection makes everything Any
            Some(TypeKey::Intrinsic(IntrinsicKind::Any)) => {
                // Mark that we have an Any in the intersection
                self.has_any = true;
                // Clear all collected properties and indices
                self.properties.clear();
                self.string_index = None;
                self.number_index = None;
            }
            // Never in intersection makes the whole thing Never
            // This is handled by the caller, not here
            _ => {
                // Not an object or intersection - ignore
            }
        }
    }

    fn merge_shape(&mut self, shape: &ObjectShape) {
        // Merge properties
        for prop in &shape.properties {
            if let Some(existing) = self.properties.iter_mut().find(|p| p.name == prop.name) {
                // TS Rule: Intersect types (using raw to avoid recursion)
                existing.type_id = self
                    .interner
                    .intersect_types_raw2(existing.type_id, prop.type_id);
                existing.write_type = self
                    .interner
                    .intersect_types_raw2(existing.write_type, prop.write_type);
                // TS Rule: Optional if ALL are optional (required wins)
                existing.optional = existing.optional && prop.optional;
                // TS Rule: Readonly if ANY is readonly (readonly is cumulative)
                existing.readonly = existing.readonly || prop.readonly;
            } else {
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
                // Readonly if ANY is readonly
                existing.readonly = existing.readonly || idx.readonly;
            } else {
                self.string_index = Some(idx.clone());
            }
        }

        // Merge number index signature
        if let Some(ref idx) = shape.number_index {
            if let Some(existing) = &mut self.number_index {
                // Intersect value types
                existing.value_type = self
                    .interner
                    .intersect_types_raw2(existing.value_type, idx.value_type);
                // Readonly if ANY is readonly
                existing.readonly = existing.readonly || idx.readonly;
            } else {
                self.number_index = Some(idx.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::intern::TypeInterner;
    use crate::solver::types::SymbolRef;

    // Mock resolver for testing
    struct MockResolver;

    impl TypeResolver for MockResolver {
        fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            None // No resolution in mock
        }
    }

    #[test]
    fn test_collect_properties_single_object() {
        let interner = TypeInterner::new();
        let resolver = MockResolver;

        // Create a simple object type { x: number }
        let props = vec![PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }];

        let obj_type = interner.object(props);

        let result = collect_properties(obj_type, &interner, &resolver);

        match result {
            PropertyCollectionResult::Properties { properties, .. } => {
                assert_eq!(properties.len(), 1);
                assert_eq!(properties[0].name, interner.intern_string("x"));
            }
            _ => panic!("Expected Properties result, got {:?}", result),
        }
    }

    #[test]
    fn test_collect_properties_intersection() {
        let interner = TypeInterner::new();
        let resolver = MockResolver;

        // Create object { x: string }
        let obj1 = interner.object(vec![PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        // Create object { y: number }
        let obj2 = interner.object(vec![PropertyInfo {
            name: interner.intern_string("y"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        // Create intersection obj1 & obj2
        let intersection = interner.intersection2(obj1, obj2);

        let result = collect_properties(intersection, &interner, &resolver);

        match result {
            PropertyCollectionResult::Properties { properties, .. } => {
                assert_eq!(properties.len(), 2);
                assert!(
                    properties
                        .iter()
                        .any(|p| p.name == interner.intern_string("x"))
                );
                assert!(
                    properties
                        .iter()
                        .any(|p| p.name == interner.intern_string("y"))
                );
            }
            _ => panic!("Expected Properties result, got {:?}", result),
        }
    }
}
