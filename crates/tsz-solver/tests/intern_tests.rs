use super::*;
use crate::freshness::{is_fresh_object_type, widen_freshness};
use crate::intern::PROPERTY_MAP_THRESHOLD;
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
        TypeKey::Literal(LiteralValue::BigInt(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "123");
        }
        _ => panic!("Expected bigint literal, got {:?}", key),
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
        let name = format!("prop{}", i);
        props.push(PropertyInfo::new(
            interner.intern_string(&name),
            TypeId::NUMBER,
        ));
    }

    let obj = interner.object(props);
    let shape_id = match interner.lookup(obj) {
        Some(TypeKey::Object(shape_id)) => shape_id,
        other => panic!("expected object type, got {:?}", other),
    };

    let target_name = format!("prop{}", PROPERTY_MAP_THRESHOLD / 2);
    let target_atom = interner.intern_string(&target_name);
    match interner.object_property_index(shape_id, target_atom) {
        PropertyLookup::Found(idx) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties[idx].name, target_atom);
        }
        other => panic!("expected cached lookup, got {:?}", other),
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
        Some(TypeKey::Object(shape_id)) => shape_id,
        other => panic!("expected object type, got {:?}", other),
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

    let Some(TypeKey::Tuple(list_a)) = interner.lookup(tuple_a) else {
        panic!("Expected tuple type");
    };
    let Some(TypeKey::Tuple(list_b)) = interner.lookup(tuple_b) else {
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

    let Some(TypeKey::TemplateLiteral(list_a)) = interner.lookup(template_a) else {
        panic!("Expected template literal type");
    };
    let Some(TypeKey::TemplateLiteral(list_b)) = interner.lookup(template_b) else {
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
        visibility: Visibility::Private,
        parent_id: None,
    }]);

    // Create object { x: string } with public visibility
    let obj_public = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    // Intersection should merge visibility (Private > Public = Private)
    let intersection = interner.intersection2(obj_private, obj_public);

    if let Some(TypeKey::Object(shape_id)) = interner.lookup(intersection) {
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

    if let Some(TypeKey::Object(shape_id)) = interner.lookup(intersection) {
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
        visibility: Visibility::Private,
        parent_id: None,
    }]);

    // These should have different TypeIds because visibility differs
    assert_ne!(
        obj_public, obj_private,
        "Objects with different visibility should have different TypeIds"
    );

    // They should also have different ObjectShapeIds
    let shape_public = match interner.lookup(obj_public) {
        Some(TypeKey::Object(shape_id)) => shape_id,
        other => panic!("Expected object type, got {:?}", other),
    };

    let shape_private = match interner.lookup(obj_private) {
        Some(TypeKey::Object(shape_id)) => shape_id,
        other => panic!("Expected object type, got {:?}", other),
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
        visibility: Visibility::Public,
        parent_id: Some(SymbolId(1)),
    }]);

    let obj_class2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: Some(SymbolId(2)),
    }]);

    // These should have different TypeIds because parent_id differs
    assert_ne!(
        obj_class1, obj_class2,
        "Objects with different parent_id should have different TypeIds"
    );

    // They should also have different ObjectShapeIds
    let shape_class1 = match interner.lookup(obj_class1) {
        Some(TypeKey::Object(shape_id)) => shape_id,
        other => panic!("Expected object type, got {:?}", other),
    };

    let shape_class2 = match interner.lookup(obj_class2) {
        Some(TypeKey::Object(shape_id)) => shape_id,
        other => panic!("Expected object type, got {:?}", other),
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
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NEVER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
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
    if let Some(TypeKey::Intersection(members)) = interner.lookup(inter1) {
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
    if let Some(TypeKey::Intersection(members)) = interner.lookup(inter) {
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
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let obj2 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NEVER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
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
    if let Some(TypeKey::Intersection(members)) = interner.lookup(inter) {
        let member_list = interner.type_list(members);
        assert_eq!(
            member_list.len(),
            2,
            "Result should have 2 members: merged object + merged callable"
        );

        // First member should be the merged object
        if let Some(TypeKey::Object(_) | TypeKey::ObjectWithIndex(_)) =
            interner.lookup(member_list[0])
        {
            // OK
        } else {
            panic!("First member should be an object");
        }

        // Second member should be the merged callable
        if let Some(TypeKey::Callable(shape_id)) = interner.lookup(member_list[1]) {
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
        Some(TypeKey::Literal(LiteralValue::String(s))) => {
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
        Some(TypeKey::Literal(LiteralValue::String(s))) => {
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
