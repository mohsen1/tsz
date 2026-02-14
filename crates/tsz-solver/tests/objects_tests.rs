use super::*;
use crate::def::DefId;
use crate::intern::TypeInterner;

// Mock resolver for testing
struct MockResolver;

impl TypeResolver for MockResolver {
    fn resolve_lazy(&self, _def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn symbol_to_def_id(&self, _symbol: SymbolRef) -> Option<DefId> {
        None
    }

    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn get_type_params(&self, _symbol: SymbolRef) -> Option<Vec<crate::types::TypeParamInfo>> {
        None
    }

    fn get_lazy_type_params(&self, _def_id: DefId) -> Option<Vec<crate::types::TypeParamInfo>> {
        None
    }

    fn def_to_symbol_id(&self, _def_id: DefId) -> Option<tsz_binder::SymbolId> {
        None
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

    assert!(matches!(
        result,
        PropertyCollectionResult::Properties { .. }
    ));
    if let PropertyCollectionResult::Properties { properties, .. } = result {
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].name, interner.intern_string("x"));
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

    assert!(matches!(
        result,
        PropertyCollectionResult::Properties { .. }
    ));
    if let PropertyCollectionResult::Properties { properties, .. } = result {
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
}

#[test]
fn test_collect_properties_any_commutative() {
    let interner = TypeInterner::new();
    let resolver = MockResolver;

    // Create object { x: number }
    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
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

#[test]
fn test_collect_properties_conflicting_property_types() {
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

    // Create object { x: number }
    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
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

    assert!(matches!(
        result,
        PropertyCollectionResult::Properties { .. }
    ));
    if let PropertyCollectionResult::Properties { properties, .. } = result {
        assert_eq!(properties.len(), 1);
        // The property type should be the intersection of string & number
        // This should be a never type or some other representation of impossible intersection
        assert_eq!(properties[0].name, interner.intern_string("x"));
    }
}

#[test]
fn test_collect_properties_optionality_merging() {
    let interner = TypeInterner::new();
    let resolver = MockResolver;

    // Create object { x?: string }
    let obj1 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    // Create object { x: number }
    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
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

    assert!(matches!(
        result,
        PropertyCollectionResult::Properties { .. }
    ));
    if let PropertyCollectionResult::Properties { properties, .. } = result {
        assert_eq!(properties.len(), 1);
        // Required wins (optional && required = required)
        assert!(!properties[0].optional);
    }
}

#[test]
fn test_collect_properties_readonly_cumulative() {
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

    // Create object { readonly x: string }
    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    // Create intersection obj1 & obj2
    let intersection = interner.intersection2(obj1, obj2);

    let result = collect_properties(intersection, &interner, &resolver);

    assert!(matches!(
        result,
        PropertyCollectionResult::Properties { .. }
    ));
    if let PropertyCollectionResult::Properties { properties, .. } = result {
        assert_eq!(properties.len(), 1);
        // Readonly is cumulative (false || true = true)
        assert!(properties[0].readonly);
    }
}

#[test]
fn test_collect_properties_nested_intersections() {
    let interner = TypeInterner::new();
    let resolver = MockResolver;

    // Create objects
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

    let obj3 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("z"),
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    // Create nested intersections: (obj1 & obj2) & obj3
    let inner = interner.intersection2(obj1, obj2);
    let nested = interner.intersection2(inner, obj3);

    let result = collect_properties(nested, &interner, &resolver);

    assert!(matches!(
        result,
        PropertyCollectionResult::Properties { .. }
    ));
    if let PropertyCollectionResult::Properties { properties, .. } = result {
        // Should have all three properties from the nested intersection
        assert_eq!(properties.len(), 3);
    }
}
