//! Property collection and merging for intersection types.
//!
//! This module provides utilities for collecting properties from intersection types
//! while handling Lazy/Ref resolution and avoiding infinite recursion.

use crate::solver::subtype::TypeResolver;
use crate::solver::types::*;
use rustc_hash::FxHashSet;

// Import TypeDatabase trait
use crate::solver::db::TypeDatabase;

/// Merge two visibility levels, returning the more restrictive one.
///
/// Ordering: Private > Protected > Public
fn merge_visibility(a: Visibility, b: Visibility) -> Visibility {
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
            // Any type in intersection makes everything Any (commutative)
            Some(TypeKey::Intrinsic(IntrinsicKind::Any)) => {
                self.found_any = true;
            }
            // Never in intersection makes the whole thing Never
            // This is handled by the caller, not here
            _ => {
                // Not an object or intersection - ignore (call signatures, primitives, etc.)
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
                // Merge visibility: use the more restrictive one (private > protected > public)
                existing.visibility = merge_visibility(existing.visibility, prop.visibility);
                // is_method: if one is a method, treat as property (more general)
                existing.is_method = existing.is_method && prop.is_method;
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

    // Mock resolver for testing
    struct MockResolver;

    impl TypeResolver for MockResolver {
        fn resolve_lazy(&self, _def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            None
        }

        fn symbol_to_def_id(&self, _symbol: SymbolRef) -> Option<DefId> {
            None
        }

        #[allow(deprecated)]
        fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
            None
        }

        fn get_type_params(
            &self,
            _symbol: SymbolRef,
        ) -> Option<Vec<crate::solver::types::TypeParamInfo>> {
            None
        }

        fn get_lazy_type_params(
            &self,
            _def_id: DefId,
        ) -> Option<Vec<crate::solver::types::TypeParamInfo>> {
            None
        }

        fn get_symbol_id(&self, _def_id: DefId) -> Option<crate::binder::SymbolId> {
            None
        }
    }

    #[test]
    fn test_collect_properties_single_object() {
        let interner = TypeInterner::new();
        let resolver = MockResolver;

        // Create a simple object type { x: number }
        let props = vec![PropertyInfo {
            name: Atom::from("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
        }];

        let obj_type = interner.object(props);

        let result = collect_properties(obj_type, &interner, &resolver);

        assert!(matches!(
            result,
            PropertyCollectionResult::Properties { .. }
        ));
        if let PropertyCollectionResult::Properties { properties, .. } = result {
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0].name, Atom::from("x"));
        }
    }

    #[test]
    fn test_collect_properties_intersection() {
        let interner = TypeInterner::new();
        let resolver = MockResolver;

        // Create object { x: string }
        let obj1 = interner.object(vec![PropertyInfo {
            name: Atom::from("x"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
        }]);

        // Create object { y: number }
        let obj2 = interner.object(vec![PropertyInfo {
            name: Atom::from("y"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
        }]);

        // Create intersection obj1 & obj2
        let intersection = interner.intersection2(obj1, obj2);

        let result = collect_properties(intersection, &interner, &resolver);

        assert!(matches!(
            result,
            PropertyCollectionResult::Properties { .. }
        ));
        if let PropertyCollectionResult::Properties { properties, .. } = result {
            assert_eq!(properties.len(), 2);
            assert!(properties.iter().any(|p| p.name == Atom::from("x")));
            assert!(properties.iter().any(|p| p.name == Atom::from("y")));
        }
    }

    #[test]
    fn test_collect_properties_any_commutative() {
        let interner = TypeInterner::new();
        let resolver = MockResolver;

        // Create object { x: number }
        let obj = interner.object(vec![PropertyInfo {
            name: Atom::from("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
        }]);

        // Test: obj & any
        let intersection1 = interner.intersection2(obj, TypeId::ANY);
        let result1 = collect_properties(intersection1, &interner, &resolver);
        assert_eq!(result1, PropertyCollectionResult::Any);

        // Test: any & obj (reverse order)
        let intersection2 = interner.intersection2(TypeId::ANY, obj);
        let result2 = collect_properties(intersection2, &interner, &resolver);
        assert_eq!(result2, PropertyCollectionResult::Any);
    }
}
