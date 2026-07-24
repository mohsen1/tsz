use super::*;
use crate::def::DefId;
use crate::intern::TypeInterner;
use crate::types::{ConditionalType, MappedType, TypeParamInfo, TypeParamOrigin};
use std::cell::Cell;

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

    fn resolve_well_known_symbol_name(&self, name: &str) -> Option<SymbolRef> {
        (name == "[Symbol.iterator]").then_some(SymbolRef(8_001))
    }
}

struct CountingResolver {
    def_id: DefId,
    body: TypeId,
    lazy_resolutions: Cell<usize>,
}

impl CountingResolver {
    const fn new(def_id: DefId, body: TypeId) -> Self {
        Self {
            def_id,
            body,
            lazy_resolutions: Cell::new(0),
        }
    }

    fn lazy_resolutions(&self) -> usize {
        self.lazy_resolutions.get()
    }
}

impl TypeResolver for CountingResolver {
    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        if def_id == self.def_id {
            self.lazy_resolutions
                .set(self.lazy_resolutions.get().saturating_add(1));
            Some(self.body)
        } else {
            None
        }
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

fn test_property(interner: &TypeInterner, name: &str, type_id: TypeId) -> PropertyInfo {
    PropertyInfo {
        name: interner.intern_string(name),
        type_id,
        write_type: type_id,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }
}

fn deferred_identity_remap(interner: &TypeInterner, binder: &str, constraint: TypeId) -> TypeId {
    let type_param = TypeParamInfo {
        name: interner.intern_string(binder),
        constraint: Some(constraint),
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    };
    let key = interner.type_param(type_param);
    let fallback = interner.type_param(TypeParamInfo {
        name: interner.intern_string(&format!("{binder}Fallback")),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    });
    let name_type = interner.conditional(ConditionalType {
        check_type: key,
        extends_type: key,
        true_type: key,
        false_type: fallback,
        is_distributive: false,
    });
    interner.mapped(MappedType {
        type_param,
        constraint,
        name_type: Some(name_type),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    })
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
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
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
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }]);

    // Create object { y: number }
    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("y"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
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
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
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
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }]);

    // Create object { x: number }
    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
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
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }]);

    // Create object { x: number }
    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
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
fn test_collect_properties_readonly_mutable_wins() {
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
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }]);

    // Create object { readonly x: string }
    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
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
        // Writable wins in intersections (false && true = false)
        // tsc: { x: string } & { readonly x: string } → writable x
        assert!(!properties[0].readonly);
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
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }]);

    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("y"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }]);

    let obj3 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("z"),
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
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

#[test]
fn test_collect_properties_deep_intersection_chain_is_iterative() {
    let interner = TypeInterner::new();
    let resolver = MockResolver;

    let mut ty = interner.object(vec![PropertyInfo {
        name: interner.intern_string("p0"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }]);

    for i in 1..4096 {
        let prop_name = interner.intern_string(&format!("p{i}"));
        let next = interner.object(vec![PropertyInfo {
            name: prop_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: i,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        }]);
        ty = interner.intersection2(ty, next);
    }

    let result = collect_properties(ty, &interner, &resolver);

    let PropertyCollectionResult::Properties { properties, .. } = result else {
        panic!("deep intersection should still collect object properties");
    };
    assert_eq!(properties.len(), 4096);
    assert!(
        properties
            .iter()
            .any(|prop| prop.name == interner.intern_string("p0")),
        "expected the deepest property to be preserved"
    );
    assert!(
        properties
            .iter()
            .any(|prop| prop.name == interner.intern_string("p4095")),
        "expected the outermost property to be preserved"
    );
}

#[test]
fn collect_properties_reuses_union_member_within_one_outer_collection() {
    let interner = TypeInterner::new();

    let shared_body = interner.object(vec![test_property(&interner, "shared", TypeId::STRING)]);
    let shared_lazy_def = DefId(7_001);
    let shared_lazy = interner.lazy(shared_lazy_def);
    let branch_a = interner.object(vec![test_property(&interner, "shared", TypeId::NUMBER)]);
    let branch_b = interner.object(vec![test_property(&interner, "shared", TypeId::BOOLEAN)]);
    let union_a = interner.union(vec![shared_lazy, branch_a]);
    let union_b = interner.union(vec![shared_lazy, branch_b]);
    let outer = interner.intersect_types_raw2(union_a, union_b);
    let resolver = CountingResolver::new(shared_lazy_def, shared_body);

    let result = collect_properties(outer, &interner, &resolver);

    let PropertyCollectionResult::Properties { properties, .. } = result else {
        panic!("outer union fan-out should collect common properties");
    };
    assert!(
        properties
            .iter()
            .any(|prop| prop.name == interner.intern_string("shared")),
        "expected the shared lazy member to contribute its common property"
    );
    assert_eq!(
        resolver.lazy_resolutions(),
        1,
        "one outer collection should reuse the completed shared union member"
    );

    let _ = collect_properties(outer, &interner, &resolver);
    assert_eq!(
        resolver.lazy_resolutions(),
        2,
        "operation-local memo must not leak across public collections"
    );
}

#[test]
fn finite_identity_mapped_materialization_keeps_normal_and_well_known_symbol_keys() {
    let interner = TypeInterner::new();
    let mut iterator = test_property(&interner, "[Symbol.iterator]", TypeId::STRING);
    iterator.is_symbol_named = true;
    let source = interner.object(vec![
        test_property(&interner, "size", TypeId::NUMBER),
        iterator,
    ]);
    let key_param = TypeParamInfo {
        name: interner.intern_string("Member"),
        constraint: Some(interner.keyof(source)),
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    };
    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: interner.keyof(source),
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let PropertyCollectionResult::Properties { properties, .. } =
        collect_properties(mapped, &interner, &MockResolver)
    else {
        panic!("finite identity map should materialize properties");
    };

    let size = properties
        .iter()
        .find(|property| property.name == interner.intern_string("size"))
        .expect("normal key must not be poisoned by the canonical symbol key");
    assert!(!size.is_symbol_named);
    assert_eq!(size.type_id, TypeId::BOOLEAN);
    let iterator = properties
        .iter()
        .find(|property| property.name == interner.intern_string("[Symbol.iterator]"))
        .expect("resolver-aware materialization must retain the symbol key");
    assert!(iterator.is_symbol_named);
    assert_eq!(iterator.type_id, TypeId::BOOLEAN);
}

#[test]
fn nested_finite_mapped_materialization_preserves_quoted_numeric_key_identity() {
    let interner = TypeInterner::new();
    let quoted_zero = interner.literal_string("0");
    let inner = deferred_identity_remap(&interner, "InnerKey", quoted_zero);
    let outer_param = TypeParamInfo {
        name: interner.intern_string("OuterKey"),
        constraint: Some(interner.keyof(inner)),
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    };
    let outer = interner.mapped(MappedType {
        type_param: outer_param,
        constraint: interner.keyof(inner),
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let PropertyCollectionResult::Properties { properties, .. } =
        collect_properties(outer, &interner, &MockResolver)
    else {
        panic!("nested finite map should materialize its quoted numeric key");
    };
    assert_eq!(properties.len(), 1);
    assert_eq!(properties[0].name, interner.intern_string("0"));
    assert!(
        properties[0].is_string_named,
        "quoted numeric key metadata must survive exact-key materialization"
    );
    assert!(!properties[0].is_symbol_named);
    assert_eq!(properties[0].type_id, TypeId::NUMBER);
}

#[test]
fn collect_properties_operation_memo_rejects_infer_conditionals() {
    let interner = TypeInterner::new();
    let infer_member = interner.infer(TypeParamInfo {
        name: interner.intern_string("Value"),
        constraint: None,
        default: None,
        is_const: false,
        origin: TypeParamOrigin::User,
    });
    let conditional = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: infer_member,
        true_type: TypeId::STRING,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let plain = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::STRING,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    assert!(!operation_memo_eligible(&interner, conditional));
    assert!(operation_memo_eligible(&interner, plain));
}
