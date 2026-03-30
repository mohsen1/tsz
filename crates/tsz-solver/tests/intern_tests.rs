use super::*;
use crate::intern::PROPERTY_MAP_THRESHOLD;
use crate::relations::freshness::{is_fresh_object_type, widen_freshness};
use tsz_binder::SymbolId;

#[test]
fn test_interner_intrinsics() {
    let interner = TypeInterner::new();

    // Intrinsics should be pre-registered
    assert!(interner.lookup(TypeId::STRING).is_some());
    assert!(interner.lookup(TypeId::NUMBER).is_some());
    assert!(interner.lookup(TypeId::ANY).is_some());
}

#[test]
fn test_interner_deduplication() {
    let interner = TypeInterner::new();

    // Same structure should get same TypeId
    let id1 = interner.literal_string("hello");
    let id2 = interner.literal_string("hello");
    let id3 = interner.literal_string("world");

    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
}

#[test]
fn test_interner_fresh_object_distinct_from_non_fresh() {
    let interner = TypeInterner::new();
    let prop = PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER);

    let fresh = interner.object_fresh(vec![prop.clone()]);
    let non_fresh = interner.object(vec![prop]);

    assert_ne!(fresh, non_fresh);
    assert!(is_fresh_object_type(&interner, fresh));
    assert!(!is_fresh_object_type(&interner, non_fresh));
    assert_eq!(widen_freshness(&interner, fresh), non_fresh);
}

#[test]
fn test_interner_bigint_literal() {
    let interner = TypeInterner::new();

    let id = interner.literal_bigint("123");
    let key = interner
        .lookup(id)
        .expect("bigint literal should be interned");

    match key {
        TypeData::Literal(LiteralValue::BigInt(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "123");
        }
        _ => panic!("Expected bigint literal, got {key:?}"),
    }
}

#[test]
fn test_interner_union_normalization() {
    let interner = TypeInterner::new();

    // Union with single member should return that member
    let single = interner.union(vec![TypeId::STRING]);
    assert_eq!(single, TypeId::STRING);

    // Union with `any` should be `any`
    let with_any = interner.union(vec![TypeId::STRING, TypeId::ANY]);
    assert_eq!(with_any, TypeId::ANY);

    // Union with `never` should exclude `never`
    let with_never = interner.union(vec![TypeId::STRING, TypeId::NEVER]);
    assert_eq!(with_never, TypeId::STRING);

    // Empty union is `never`
    let empty = interner.union(vec![]);
    assert_eq!(empty, TypeId::NEVER);

    // Union with `error` should be `error`
    let with_error = interner.union(vec![TypeId::STRING, TypeId::ERROR]);
    assert_eq!(with_error, TypeId::ERROR);
}

#[test]
fn test_interner_union_unknown_dominates() {
    let interner = TypeInterner::new();

    let with_unknown = interner.union(vec![TypeId::STRING, TypeId::UNKNOWN]);
    assert_eq!(with_unknown, TypeId::UNKNOWN);

    let only_unknown = interner.union(vec![TypeId::UNKNOWN]);
    assert_eq!(only_unknown, TypeId::UNKNOWN);
}

#[test]
fn test_interner_union_any_beats_unknown() {
    let interner = TypeInterner::new();

    let any_and_unknown = interner.union(vec![TypeId::ANY, TypeId::UNKNOWN]);
    assert_eq!(any_and_unknown, TypeId::ANY);
}

#[test]
fn test_interner_union_dedups_and_flattens() {
    let interner = TypeInterner::new();

    let nested = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let flattened = interner.union(vec![TypeId::STRING, nested, TypeId::STRING]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(flattened, expected);
}

#[test]
fn test_interner_intersection_normalization() {
    let interner = TypeInterner::new();

    // Intersection with single member should return that member
    let single = interner.intersection(vec![TypeId::STRING]);
    assert_eq!(single, TypeId::STRING);

    // Intersection with `never` should be `never`
    let with_never = interner.intersection(vec![TypeId::STRING, TypeId::NEVER]);
    assert_eq!(with_never, TypeId::NEVER);

    // Empty intersection is `unknown`
    let empty = interner.intersection(vec![]);
    assert_eq!(empty, TypeId::UNKNOWN);

    // Intersection with `any` should be `any`
    let with_any = interner.intersection(vec![TypeId::STRING, TypeId::ANY]);
    assert_eq!(with_any, TypeId::ANY);

    // Intersection with `error` should be `error`
    let with_error = interner.intersection(vec![TypeId::STRING, TypeId::ERROR]);
    assert_eq!(with_error, TypeId::ERROR);
}

#[test]
fn test_interner_intersection_unknown_identity() {
    let interner = TypeInterner::new();

    let with_unknown = interner.intersection(vec![TypeId::STRING, TypeId::UNKNOWN]);
    assert_eq!(with_unknown, TypeId::STRING);

    let only_unknown = interner.intersection(vec![TypeId::UNKNOWN]);
    assert_eq!(only_unknown, TypeId::UNKNOWN);
}

#[test]
fn test_interner_intersection_any_over_unknown() {
    let interner = TypeInterner::new();

    let any_and_unknown = interner.intersection(vec![TypeId::ANY, TypeId::UNKNOWN]);
    assert_eq!(any_and_unknown, TypeId::ANY);
}

#[test]
fn test_interner_intersection_flattens_and_dedups() {
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let inner = interner.intersection(vec![obj_a, obj_b]);
    let outer = interner.intersection(vec![inner, obj_a]);
    let dup = interner.intersection(vec![obj_b, obj_a, obj_a]);

    assert_eq!(outer, inner);
    assert_eq!(dup, inner);
}

#[test]
fn test_interner_intersection_disjoint_primitives() {
    let interner = TypeInterner::new();

    let disjoint = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(disjoint, TypeId::NEVER);

    let literal = interner.literal_string("a");
    let disjoint_literal = interner.intersection(vec![literal, TypeId::BOOLEAN]);
    assert_eq!(disjoint_literal, TypeId::NEVER);
}

#[test]
fn test_interner_intersection_disjoint_object_literals() {
    let interner = TypeInterner::new();

    let kind = interner.intern_string("kind");
    let obj_a = interner.object(vec![PropertyInfo::new(kind, interner.literal_string("a"))]);
    let obj_b = interner.object(vec![PropertyInfo::new(kind, interner.literal_string("b"))]);

    let disjoint = interner.intersection(vec![obj_a, obj_b]);
    assert_eq!(disjoint, TypeId::NEVER);
}

#[test]
fn test_interner_intersection_disjoint_object_literal_union() {
    let interner = TypeInterner::new();

    let kind = interner.intern_string("kind");
    let union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);
    let obj_union = interner.object(vec![PropertyInfo::new(kind, union)]);
    let obj_c = interner.object(vec![PropertyInfo::new(kind, interner.literal_string("c"))]);

    let disjoint = interner.intersection(vec![obj_union, obj_c]);
    assert_eq!(disjoint, TypeId::NEVER);
}

#[test]
fn test_interner_intersection_callable_vs_object_disjoint_property() {
    // { (x: string): number, a: "" } & { a: number } should reduce to never
    // because property `a` has type "" & number which is never (cross-domain literal).
    // This matches tsc's discriminant-based intersection reduction.
    let interner = TypeInterner::new();

    let a_name = interner.intern_string("a");
    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![PropertyInfo::new(a_name, interner.literal_string(""))],
        ..Default::default()
    });

    let obj = interner.object(vec![PropertyInfo::new(a_name, TypeId::NUMBER)]);

    let result = interner.intersection(vec![callable, obj]);
    assert_eq!(
        result,
        TypeId::NEVER,
        "Callable with literal property intersected with object of incompatible primitive class should be never"
    );
}

#[test]
fn test_interner_intersection_callable_vs_object_compatible_property() {
    // { (x: string): number, a: string } & { a: string } should NOT reduce to never
    // because property `a: string & string = string` is compatible.
    let interner = TypeInterner::new();

    let a_name = interner.intern_string("a");
    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![PropertyInfo::new(a_name, TypeId::STRING)],
        ..Default::default()
    });

    let obj = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let result = interner.intersection(vec![callable, obj]);
    assert_ne!(
        result,
        TypeId::NEVER,
        "Callable with compatible property should not reduce to never"
    );
}

#[test]
fn test_interner_intersection_object_intrinsic_with_primitive_is_never() {
    let interner = TypeInterner::new();

    // object & string = never (object excludes all primitives)
    assert_eq!(
        interner.intersection(vec![TypeId::OBJECT, TypeId::STRING]),
        TypeId::NEVER
    );

    // object & number = never
    assert_eq!(
        interner.intersection(vec![TypeId::OBJECT, TypeId::NUMBER]),
        TypeId::NEVER
    );

    // object & boolean = never
    assert_eq!(
        interner.intersection(vec![TypeId::OBJECT, TypeId::BOOLEAN]),
        TypeId::NEVER
    );

    // object & null = never
    assert_eq!(
        interner.intersection(vec![TypeId::OBJECT, TypeId::NULL]),
        TypeId::NEVER
    );

    // object & undefined = never
    assert_eq!(
        interner.intersection(vec![TypeId::OBJECT, TypeId::UNDEFINED]),
        TypeId::NEVER
    );

    // object & "hello" (string literal) = never
    let hello = interner.literal_string("hello");
    assert_eq!(
        interner.intersection(vec![TypeId::OBJECT, hello]),
        TypeId::NEVER
    );

    // But: object & { foo: string } should NOT be never (structural object is compatible)
    let foo = interner.intern_string("foo");
    let obj = interner.object(vec![PropertyInfo::new(foo, TypeId::STRING)]);
    assert_ne!(
        interner.intersection(vec![TypeId::OBJECT, obj]),
        TypeId::NEVER
    );

    // And: {} & string should NOT be never (branded types allowed)
    let empty_obj = interner.object(vec![]);
    assert_ne!(
        interner.intersection(vec![empty_obj, TypeId::STRING]),
        TypeId::NEVER
    );
}

#[test]
fn test_interner_intersection_optional_object_literals_not_reduced() {
    let interner = TypeInterner::new();

    let kind = interner.intern_string("kind");
    let obj_a = interner.object(vec![PropertyInfo::opt(kind, interner.literal_string("a"))]);
    let obj_b = interner.object(vec![PropertyInfo::opt(kind, interner.literal_string("b"))]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    assert_ne!(intersection, TypeId::NEVER);
}

#[test]
fn test_interner_object_sorting() {
    let interner = TypeInterner::new();

    // Properties in different order should produce same TypeId
    let props1 = vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ];
    let props2 = vec![
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
    ];

    let id1 = interner.object(props1);
    let id2 = interner.object(props2);

    assert_eq!(id1, id2);
}

#[test]
fn test_interner_object_property_lookup_cache() {
    let interner = TypeInterner::new();

    let mut props = Vec::with_capacity(PROPERTY_MAP_THRESHOLD + 2);
    for i in 0..(PROPERTY_MAP_THRESHOLD + 2) {
        let name = format!("prop{i}");
        props.push(PropertyInfo::new(
            interner.intern_string(&name),
            TypeId::NUMBER,
        ));
    }

    let obj = interner.object(props);
    let shape_id = match interner.lookup(obj) {
        Some(TypeData::Object(shape_id)) => shape_id,
        other => panic!("expected object type, got {other:?}"),
    };

    let target_name = format!("prop{}", PROPERTY_MAP_THRESHOLD / 2);
    let target_atom = interner.intern_string(&target_name);
    match interner.object_property_index(shape_id, target_atom) {
        PropertyLookup::Found(idx) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties[idx].name, target_atom);
        }
        other => panic!("expected cached lookup, got {other:?}"),
    }

    let missing = interner.intern_string("missing");
    assert_eq!(
        interner.object_property_index(shape_id, missing),
        PropertyLookup::NotFound
    );

    let small = interner.object(vec![PropertyInfo::new(
        interner.intern_string("only"),
        TypeId::STRING,
    )]);
    let small_shape_id = match interner.lookup(small) {
        Some(TypeData::Object(shape_id)) => shape_id,
        other => panic!("expected object type, got {other:?}"),
    };
    assert_eq!(
        interner.object_property_index(small_shape_id, interner.intern_string("only")),
        PropertyLookup::Uncached
    );
}

#[test]
fn test_interner_application_deduplication() {
    let interner = TypeInterner::new();

    let base = interner.lazy(DefId(1));
    let app1 = interner.application(base, vec![TypeId::STRING]);
    let app2 = interner.application(base, vec![TypeId::STRING]);
    let app3 = interner.application(base, vec![TypeId::NUMBER]);

    assert_eq!(app1, app2);
    assert_ne!(app1, app3);
}

#[test]
fn test_tuple_list_interning_deduplication() {
    use std::sync::Arc;

    let interner = TypeInterner::new();
    let elements = vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ];

    let tuple_a = interner.tuple(elements.clone());
    let tuple_b = interner.tuple(elements);

    let Some(TypeData::Tuple(list_a)) = interner.lookup(tuple_a) else {
        panic!("Expected tuple type");
    };
    let Some(TypeData::Tuple(list_b)) = interner.lookup(tuple_b) else {
        panic!("Expected tuple type");
    };

    assert_eq!(list_a, list_b);
    let elems_a = interner.tuple_list(list_a);
    let elems_b = interner.tuple_list(list_b);
    assert!(Arc::ptr_eq(&elems_a, &elems_b));
    assert_eq!(elems_a.len(), 2);
}

#[test]
fn test_template_literal_list_interning_deduplication() {
    use std::sync::Arc;

    let interner = TypeInterner::new();
    let spans = vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ];

    let template_a = interner.template_literal(spans.clone());
    let template_b = interner.template_literal(spans);

    let Some(TypeData::TemplateLiteral(list_a)) = interner.lookup(template_a) else {
        panic!("Expected template literal type");
    };
    let Some(TypeData::TemplateLiteral(list_b)) = interner.lookup(template_b) else {
        panic!("Expected template literal type");
    };

    assert_eq!(list_a, list_b);
    let spans_a = interner.template_list(list_a);
    let spans_b = interner.template_list(list_b);
    assert!(Arc::ptr_eq(&spans_a, &spans_b));
    assert_eq!(spans_a.len(), 3);
}

#[test]
fn test_intersection_visibility_merging() {
    let interner = TypeInterner::new();

    // Create object { x: number } with private visibility
    let obj_private = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Private,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Create object { x: string } with public visibility
    let obj_public = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    // Intersection should merge visibility (Private > Public = Private)
    let intersection = interner.intersection2(obj_private, obj_public);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(intersection) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 1);
        assert_eq!(shape.properties[0].visibility, Visibility::Private);
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_intersection_disjoint_literals() {
    let interner = TypeInterner::new();

    // Test: 1 & 2 should be NEVER (disjoint number literals)
    let lit1 = interner.literal_number(1.0);
    let lit2 = interner.literal_number(2.0);
    let intersection = interner.intersection2(lit1, lit2);

    assert_eq!(intersection, TypeId::NEVER);
}

#[test]
fn test_intersection_object_merging() {
    let interner = TypeInterner::new();

    // Test: { a: 1 } & { b: 2 } should merge to { a: 1, b: 2 }
    let obj1 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let obj2 = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection = interner.intersection2(obj1, obj2);

    if let Some(TypeData::Object(shape_id)) = interner.lookup(intersection) {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);
        let prop_names: Vec<_> = shape.properties.iter().map(|p| p.name.0).collect();
        let atom_a = interner.intern_string("a").0;
        let atom_b = interner.intern_string("b").0;
        assert!(prop_names.contains(&atom_a));
        assert!(prop_names.contains(&atom_b));
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_intersection_disjoint_property_types() {
    let interner = TypeInterner::new();

    // Test: { a: 1 } & { a: 2 } should reduce to NEVER (disjoint property types)
    let lit1 = interner.literal_number(1.0);
    let lit2 = interner.literal_number(2.0);

    let obj1 = interner.object(vec![PropertyInfo::new(interner.intern_string("a"), lit1)]);

    let obj2 = interner.object(vec![PropertyInfo::new(interner.intern_string("a"), lit2)]);

    let intersection = interner.intersection2(obj1, obj2);

    // Objects with disjoint property types should reduce to NEVER
    // This is detected in intersection_has_disjoint_primitives
    assert_eq!(intersection, TypeId::NEVER);
}

#[test]
fn test_visibility_interning_distinct_shape_ids() {
    let interner = TypeInterner::new();

    // Create two objects with identical structure but different visibility
    let obj_public = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let obj_private = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Private,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // These should have different TypeIds because visibility differs
    assert_ne!(
        obj_public, obj_private,
        "Objects with different visibility should have different TypeIds"
    );

    // They should also have different ObjectShapeIds
    let shape_public = match interner.lookup(obj_public) {
        Some(TypeData::Object(shape_id)) => shape_id,
        other => panic!("Expected object type, got {other:?}"),
    };

    let shape_private = match interner.lookup(obj_private) {
        Some(TypeData::Object(shape_id)) => shape_id,
        other => panic!("Expected object type, got {other:?}"),
    };

    assert_ne!(
        shape_public, shape_private,
        "Objects with different visibility should have different ObjectShapeIds"
    );
}

#[test]
fn test_parent_id_interning_distinct_shape_ids() {
    let interner = TypeInterner::new();

    // Create two objects with identical structure but different parent_id
    // This tests nominal property identity (different declaring classes)
    let obj_class1 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: Some(SymbolId(1)),
        declaration_order: 0,
        is_string_named: false,
    }]);

    let obj_class2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: Some(SymbolId(2)),
        declaration_order: 0,
        is_string_named: false,
    }]);

    // These should have different TypeIds because parent_id differs
    assert_ne!(
        obj_class1, obj_class2,
        "Objects with different parent_id should have different TypeIds"
    );

    // They should also have different ObjectShapeIds
    let shape_class1 = match interner.lookup(obj_class1) {
        Some(TypeData::Object(shape_id)) => shape_id,
        other => panic!("Expected object type, got {other:?}"),
    };

    let shape_class2 = match interner.lookup(obj_class2) {
        Some(TypeData::Object(shape_id)) => shape_id,
        other => panic!("Expected object type, got {other:?}"),
    };

    assert_ne!(
        shape_class1, shape_class2,
        "Objects with different parent_id should have different ObjectShapeIds"
    );
}

// Task #42: Test union order independence (canonicalization)
#[test]
fn test_union_order_independence() {
    let interner = TypeInterner::new();

    // Create two literal types
    let type_a = interner.literal_string("a");
    let type_b = interner.literal_string("b");

    // Create union A | B
    let union_ab = interner.union(vec![type_a, type_b]);

    // Create union B | A (reverse order)
    let union_ba = interner.union(vec![type_b, type_a]);

    // They should have the same TypeId (order independence)
    assert_eq!(
        union_ab, union_ba,
        "Unions should be order-independent: A | B == B | A"
    );

    // Also test with more members
    let type_c = interner.literal_string("c");
    let union_abc = interner.union(vec![type_a, type_b, type_c]);
    let union_cba = interner.union(vec![type_c, type_b, type_a]);

    assert_eq!(
        union_abc, union_cba,
        "Unions with 3+ members should be order-independent"
    );
}

// Task #42: Test intersection order independence
#[test]
fn test_intersection_order_independence() {
    let interner = TypeInterner::new();

    // Create two literal types (non-callable for simplicity)
    let type_a = interner.literal_string("a");
    let type_b = interner.literal_string("b");

    // Create intersection A & B
    let inter_ab = interner.intersection(vec![type_a, type_b]);

    // Create intersection B & A (reverse order)
    let inter_ba = interner.intersection(vec![type_b, type_a]);

    // They should have the same TypeId (order independence for non-callables)
    assert_eq!(
        inter_ab, inter_ba,
        "Intersections should be order-independent: A & B == B & A"
    );
}

// Task #42: Test union redundancy elimination
#[test]
fn test_union_redundancy_elimination() {
    let interner = TypeInterner::new();

    let type_a = interner.literal_string("a");

    // A | A should simplify to A
    let union_aa = interner.union(vec![type_a, type_a]);

    assert_eq!(union_aa, type_a, "Union of A | A should simplify to A");
}

// Task #42: Test intersection redundancy elimination
#[test]
fn test_intersection_redundancy_elimination() {
    let interner = TypeInterner::new();

    let type_a = interner.literal_string("a");

    // A & A should simplify to A
    let inter_aa = interner.intersection(vec![type_a, type_a]);

    assert_eq!(
        inter_aa, type_a,
        "Intersection of A & A should simplify to A"
    );
}

// Task #43: Test partial object merging in mixed intersections
#[test]
fn test_partial_object_merging_in_intersection() {
    let interner = TypeInterner::new();

    // Create two object types
    let obj1 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::NEVER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NEVER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Create a primitive type
    let prim = TypeId::BOOLEAN;

    // Intersection: { a: string } & { b: number } & boolean
    // Expected: Merged object { a: string; b: number } & boolean
    let inter1 = interner.intersection(vec![obj1, obj2, prim]);
    let inter2 = interner.intersection(vec![obj2, obj1, prim]); // Different order

    // Order independence should still hold
    assert_eq!(
        inter1, inter2,
        "Partial object merging should be order-independent"
    );

    // The result should be an intersection of merged object and boolean
    if let Some(TypeData::Intersection(members)) = interner.lookup(inter1) {
        let member_list = interner.type_list(members);
        assert_eq!(
            member_list.len(),
            2,
            "Result should have 2 members: merged object + boolean"
        );
    } else {
        panic!("Expected intersection type");
    }
}

// Task #43: Test partial callable merging in mixed intersections
#[test]
fn test_partial_callable_merging_in_intersection() {
    let interner = TypeInterner::new();

    // Create two function types
    let func1 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let func2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Create a primitive type
    let prim = TypeId::BOOLEAN;

    // Intersection: (x: string) => void & (x: number) => void & boolean
    // Expected: Merged callable with 2 overloads & boolean
    let inter = interner.intersection(vec![func1, func2, prim]);

    // The result should be an intersection of merged callable and boolean
    if let Some(TypeData::Intersection(members)) = interner.lookup(inter) {
        let member_list = interner.type_list(members);
        assert_eq!(
            member_list.len(),
            2,
            "Result should have 2 members: merged callable + boolean"
        );
    } else {
        panic!("Expected intersection type");
    }

    // NOTE: Callable order IS significant in TypeScript, so different input
    // orders produce different results (different overload orders).
    // We do NOT test order independence for callables.
}

// Task #43: Test partial merging with both objects and callables
#[test]
fn test_partial_object_and_callable_merging() {
    let interner = TypeInterner::new();

    // Create object type
    let obj1 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::NEVER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NEVER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Create callable types
    let func1 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let func2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Intersection: { a: string } & { b: number } & (x: string) => void & (x: number) => void
    // Expected: Merged object { a: string; b: number } & Merged callable (with 2 overloads)
    let inter = interner.intersection(vec![obj1, obj2, func1, func2]);

    // The result should be an intersection with 2 members: merged object + merged callable
    if let Some(TypeData::Intersection(members)) = interner.lookup(inter) {
        let member_list = interner.type_list(members);
        assert_eq!(
            member_list.len(),
            2,
            "Result should have 2 members: merged object + merged callable"
        );

        // First member should be the merged object
        if let Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_)) =
            interner.lookup(member_list[0])
        {
            // OK
        } else {
            panic!("First member should be an object");
        }

        // Second member should be the merged callable
        if let Some(TypeData::Callable(shape_id)) = interner.lookup(member_list[1]) {
            // OK - verify it has 2 call signatures
            let callable = interner.callable_shape(shape_id);
            assert_eq!(
                callable.call_signatures.len(),
                2,
                "Merged callable should have 2 call signatures"
            );
        } else {
            panic!("Second member should be a callable");
        }
    } else {
        panic!("Expected intersection type");
    }

    // NOTE: Object order independence should be tested separately
}

// Task #47: Template Literal Canonicalization Tests

#[test]
fn test_template_never_absorption() {
    let interner = TypeInterner::new();

    // `` `${never}` `` should be never
    let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::NEVER)]);
    assert_eq!(
        template,
        TypeId::NEVER,
        "Template with never should be never"
    );

    // `` `a${never}b` `` should be never
    let template2 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(TypeId::NEVER),
        TemplateSpan::Text(interner.intern_string("b")),
    ]);
    assert_eq!(
        template2,
        TypeId::NEVER,
        "Template with never anywhere should be never"
    );
}

#[test]
fn test_template_empty_string_removal() {
    let interner = TypeInterner::new();

    // `` `${""}` `` should simplify to empty string literal
    let empty_lit = interner.literal_string("");
    let template = interner.template_literal(vec![TemplateSpan::Type(empty_lit)]);

    // Should be a literal empty string, not a template with empty type span
    match interner.lookup(template) {
        Some(TypeData::Literal(LiteralValue::String(s))) => {
            let s = interner.resolve_atom_ref(s);
            assert!(s.is_empty(), "Should be empty string literal");
        }
        _ => panic!("Expected empty string literal"),
    }

    // `` `a${""}b` `` should become `ab` (text spans merged)
    let template2 = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("a")),
        TemplateSpan::Type(empty_lit),
        TemplateSpan::Text(interner.intern_string("b")),
    ]);

    // Should be a literal "ab", not a template
    match interner.lookup(template2) {
        Some(TypeData::Literal(LiteralValue::String(s))) => {
            let s = interner.resolve_atom_ref(s);
            assert_eq!(s.to_string(), "ab", "Should be merged 'ab' literal");
        }
        _ => panic!("Expected 'ab' string literal"),
    }
}

#[test]
fn test_template_unknown_widening() {
    let interner = TypeInterner::new();

    // `` `${unknown}` `` should widen to string
    let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::UNKNOWN)]);
    assert_eq!(
        template,
        TypeId::STRING,
        "Template with unknown should widen to string"
    );
}

#[test]
fn test_template_any_widening() {
    let interner = TypeInterner::new();

    // `` `${any}` `` should widen to string (any is infectious in templates)
    let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::ANY)]);
    assert_eq!(
        template,
        TypeId::STRING,
        "Template with any should widen to string"
    );
}

#[test]
fn test_empty_object_rule_intersection() {
    let interner = TypeInterner::new();

    // Task #48: Empty Object Rule for Intersections
    // Primitives should absorb empty objects in intersections

    // Case 1: string & empty_object → string
    let empty_obj = interner.object(vec![]);
    let string_and_empty = interner.intersection(vec![TypeId::STRING, empty_obj]);
    assert_eq!(
        string_and_empty,
        TypeId::STRING,
        "string & empty_object should normalize to string"
    );

    // Case 2: number & empty_object → number
    let number_and_empty = interner.intersection(vec![TypeId::NUMBER, empty_obj]);
    assert_eq!(
        number_and_empty,
        TypeId::NUMBER,
        "number & empty_object should normalize to number"
    );

    // Case 3: (string | null) & empty_object
    // Due to distributivity: (string | null) & {} → (string & {}) | (null & {})
    // string & {} → string
    // null & {} → never (null is disjoint from objects)
    // Result: string | never → string
    let string_or_null = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let union_and_empty = interner.intersection(vec![string_or_null, empty_obj]);

    // The result should be string (null is filtered out by the empty object constraint)
    assert_eq!(
        union_and_empty,
        TypeId::STRING,
        "(string | null) & empty_object should normalize to string"
    );

    // Case 4: string & empty_object & number → never (disjoint primitives still detected)
    let string_and_empty_and_number =
        interner.intersection(vec![TypeId::STRING, empty_obj, TypeId::NUMBER]);
    assert_eq!(
        string_and_empty_and_number,
        TypeId::NEVER,
        "string & empty_object & number should be never (disjoint primitives)"
    );
}

// =========================================================================
// Union member ordering tests
// =========================================================================

/// String literals in unions should be sorted by lexicographic content,
/// not by TypeId (interning order). This matches tsc's behavior where
/// String literal unions are canonical: the same set of members always
/// produces the same TypeId regardless of input order.
#[test]
fn test_union_string_literal_ordering() {
    let interner = TypeInterner::new();

    let type_d = interner.literal_string("D");
    let type_c = interner.literal_string("C");
    let type_b = interner.literal_string("B");
    let type_a = interner.literal_string("A");

    let union1 = interner.union(vec![type_d, type_c, type_b, type_a]);
    let union2 = interner.union(vec![type_a, type_b, type_c, type_d]);
    let union3 = interner.union(vec![type_b, type_d, type_a, type_c]);

    // All orderings should produce the same canonical union
    assert_eq!(
        union1, union2,
        "Unions with same members in different order should be identical"
    );
    assert_eq!(
        union1, union3,
        "Unions with same members in any order should be identical"
    );

    // Verify we get a 4-member union
    if let Some(TypeData::Union(list_id)) = interner.lookup(union1) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 4);
    } else {
        panic!("Expected Union type");
    }
}

/// Built-in types should maintain their fixed sort order regardless of
/// input order. null and undefined should sort last.
#[test]
fn test_union_builtin_ordering() {
    let interner = TypeInterner::new();

    // string | number should be consistent regardless of input order
    let sn = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let ns = interner.union2(TypeId::NUMBER, TypeId::STRING);
    assert_eq!(sn, ns, "string | number == number | string");

    // Verify string sorts before number (sort key 8 < 9)
    if let Some(TypeData::Union(list_id)) = interner.lookup(sn) {
        let members = interner.type_list(list_id);
        assert_eq!(
            members[0],
            TypeId::STRING,
            "string should sort before number"
        );
        assert_eq!(
            members[1],
            TypeId::NUMBER,
            "number should sort after string"
        );
    }

    // null and undefined should sort after primitives
    let with_null = interner.union(vec![TypeId::NULL, TypeId::STRING, TypeId::UNDEFINED]);
    if let Some(TypeData::Union(list_id)) = interner.lookup(with_null) {
        let members = interner.type_list(list_id);
        assert_eq!(members[0], TypeId::STRING, "string should be first");
        assert_eq!(
            members[1],
            TypeId::UNDEFINED,
            "undefined should be second-to-last"
        );
        assert_eq!(members[2], TypeId::NULL, "null should be last");
    }
}

/// Number literal unions are canonical: the same set of members always
/// produces the same TypeId regardless of input order.
#[test]
fn test_union_number_literal_ordering() {
    let interner = TypeInterner::new();

    let n3 = interner.literal_number(3.0);
    let n1 = interner.literal_number(1.0);
    let n2 = interner.literal_number(2.0);

    let union_mixed = interner.union(vec![n3, n1, n2]);
    let union_sorted = interner.union(vec![n1, n2, n3]);
    let union_rev = interner.union(vec![n2, n3, n1]);

    assert_eq!(
        union_mixed, union_sorted,
        "Number literal unions should be order-independent"
    );
    assert_eq!(
        union_mixed, union_rev,
        "Number literal unions should be order-independent (reversed)"
    );

    if let Some(TypeData::Union(list_id)) = interner.lookup(union_mixed) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 3);
    }
}

/// Application types (generic instantiations) in unions should sort by their base
/// type's DefId ordering, not by raw TypeId. This ensures that `I1<number> | I2<number>`
/// displays in source declaration order (I1 before I2) matching tsc behavior.
#[test]
fn test_union_application_types_sort_by_base_def_id() {
    use crate::def::DefId;

    let interner = TypeInterner::new();

    // Create two Lazy types with known DefIds — lower DefId = declared first in source
    let def_i1 = DefId(10); // I1 declared first
    let def_i2 = DefId(20); // I2 declared second
    let lazy_i1 = interner.lazy(def_i1);
    let lazy_i2 = interner.lazy(def_i2);

    // Create Application types: I1<number> and I2<number>
    let app_i1_num = interner.application(lazy_i1, vec![TypeId::NUMBER]);
    let app_i2_num = interner.application(lazy_i2, vec![TypeId::NUMBER]);

    // Create union in REVERSE order: I2<number> | I1<number>
    let union_reversed = interner.union(vec![app_i2_num, app_i1_num]);

    // The normalized union should sort I1<number> before I2<number> (lower DefId first)
    if let Some(TypeData::Union(list_id)) = interner.lookup(union_reversed) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 2, "Union should have 2 members");
        assert_eq!(
            members[0], app_i1_num,
            "I1<number> (DefId=10) should come first in the union"
        );
        assert_eq!(
            members[1], app_i2_num,
            "I2<number> (DefId=20) should come second in the union"
        );
    } else {
        panic!("Expected Union type");
    }

    // Union created in source order should produce the same TypeId
    let union_ordered = interner.union(vec![app_i1_num, app_i2_num]);
    assert_eq!(
        union_reversed, union_ordered,
        "Application union ordering should be deterministic regardless of input order"
    );
}

/// Application types with the same base but different args should sort by args.
#[test]
fn test_union_application_types_same_base_sort_by_args() {
    use crate::def::DefId;

    let interner = TypeInterner::new();

    let def_id = DefId(10);
    let lazy_base = interner.lazy(def_id);

    // Create Application types: Foo<number> and Foo<string>
    let app_num = interner.application(lazy_base, vec![TypeId::NUMBER]);
    let app_str = interner.application(lazy_base, vec![TypeId::STRING]);

    // Create union in both orders — should normalize to same result
    let union_a = interner.union(vec![app_num, app_str]);
    let union_b = interner.union(vec![app_str, app_num]);
    assert_eq!(
        union_a, union_b,
        "Same-base application unions should be order-independent"
    );

    // Verify it's a union (not collapsed)
    if let Some(TypeData::Union(list_id)) = interner.lookup(union_a) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 2, "Union should have 2 members");
    } else {
        panic!("Expected Union type");
    }
}

#[test]
fn test_union_member_order_uses_allocation_order() {
    // Short string literals (1-2 chars) are sorted by content to match tsc's
    // lib.d.ts pre-allocation order. tsc pre-creates common short string
    // literals during lib processing in roughly alphabetical order.
    // Longer strings use allocation order (source encounter order).
    let interner = TypeInterner::new();

    // Create short string literals in a specific order (d, c, a)
    let lit_d = interner.literal_string("d");
    let lit_c = interner.literal_string("c");
    let lit_a = interner.literal_string("a");

    // Short strings should sort by content (alphabetical), matching tsc lib ordering
    let union_id = interner.union(vec![lit_a, lit_c, lit_d]);

    if let Some(TypeData::Union(list_id)) = interner.lookup(union_id) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 3);
        // Content order: a, c, d (alphabetical for short strings)
        assert_eq!(
            members[0], lit_a,
            "First member should be 'a' (alphabetically first)"
        );
        assert_eq!(
            members[1], lit_c,
            "Second member should be 'c' (alphabetically second)"
        );
        assert_eq!(
            members[2], lit_d,
            "Third member should be 'd' (alphabetically third)"
        );
    } else {
        panic!("Expected Union type");
    }

    // Longer strings should preserve allocation order (source encounter order)
    let lit_foo = interner.literal_string("foo");
    let lit_bar = interner.literal_string("bar");

    let union_id2 = interner.union(vec![lit_bar, lit_foo]);

    if let Some(TypeData::Union(list_id)) = interner.lookup(union_id2) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 2);
        // Allocation order: foo was interned first, then bar
        assert_eq!(
            members[0], lit_foo,
            "First member should be 'foo' (interned first)"
        );
        assert_eq!(
            members[1], lit_bar,
            "Second member should be 'bar' (interned second)"
        );
    } else {
        panic!("Expected Union type");
    }
}

#[test]
fn test_union_order_independent_of_input_order() {
    // Unions constructed with different input orders should normalize
    // to the same allocation-order-based result.
    let interner = TypeInterner::new();

    // Intern in order: x, y, z
    let x = interner.literal_string("x");
    let y = interner.literal_string("y");
    let z = interner.literal_string("z");

    let union1 = interner.union(vec![z, x, y]);
    let union2 = interner.union(vec![y, z, x]);
    let union3 = interner.union(vec![x, y, z]);

    assert_eq!(union1, union2, "Union should be order-independent");
    assert_eq!(union2, union3, "Union should be order-independent");
}

#[test]
fn test_estimated_size_bytes_is_nonzero_for_fresh_interner() {
    let interner = TypeInterner::new();
    let size = interner.estimated_size_bytes();
    assert!(
        size > 0,
        "estimated_size_bytes should be nonzero even for a fresh interner (struct overhead + intrinsics)"
    );
    // Must be at least the struct size itself
    assert!(
        size >= std::mem::size_of::<TypeInterner>(),
        "estimate ({size}) should be >= struct size ({})",
        std::mem::size_of::<TypeInterner>()
    );
}

#[test]
fn test_estimated_size_bytes_grows_with_interned_types() {
    let interner = TypeInterner::new();
    let baseline = interner.estimated_size_bytes();

    // Intern a bunch of types
    for i in 0..100 {
        interner.literal_string(&format!("prop_{i}"));
    }

    let after_types = interner.estimated_size_bytes();
    assert!(
        after_types > baseline,
        "Size should grow after interning types: baseline={baseline}, after={after_types}"
    );
}

#[test]
fn test_estimated_size_bytes_grows_with_object_shapes() {
    let interner = TypeInterner::new();
    let baseline = interner.estimated_size_bytes();

    // Intern object shapes (heavier than primitives)
    for i in 0..20 {
        let prop_name = interner.string_interner.intern(&format!("field_{i}"));
        let prop = PropertyInfo {
            name: prop_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            visibility: Visibility::Public,
            is_method: false,
            is_class_prototype: false,
            parent_id: None,
            declaration_order: i as u32,
            is_string_named: false,
        };
        interner.object(vec![prop]);
    }

    let after_objects = interner.estimated_size_bytes();
    assert!(
        after_objects > baseline,
        "Size should grow after interning objects: baseline={baseline}, after={after_objects}"
    );
}

#[test]
fn test_estimated_size_bytes_grows_with_functions() {
    let interner = TypeInterner::new();
    let baseline = interner.estimated_size_bytes();

    // Intern function shapes
    for i in 0..20 {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.string_interner.intern(&format!("p_{i}"))),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
    }

    let after_fns = interner.estimated_size_bytes();
    assert!(
        after_fns > baseline,
        "Size should grow after interning functions: baseline={baseline}, after={after_fns}"
    );
}
