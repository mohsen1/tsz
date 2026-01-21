use super::*;
use crate::parser::NodeArena;
use crate::parser::NodeIndex;
use crate::solver::intern::PROPERTY_MAP_THRESHOLD;

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let obj_a = interner.object(vec![PropertyInfo {
        name: kind,
        type_id: interner.literal_string("a"),
        write_type: interner.literal_string("a"),
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let obj_b = interner.object(vec![PropertyInfo {
        name: kind,
        type_id: interner.literal_string("b"),
        write_type: interner.literal_string("b"),
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let obj_union = interner.object(vec![PropertyInfo {
        name: kind,
        type_id: union,
        write_type: union,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let obj_c = interner.object(vec![PropertyInfo {
        name: kind,
        type_id: interner.literal_string("c"),
        write_type: interner.literal_string("c"),
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let disjoint = interner.intersection(vec![obj_union, obj_c]);
    assert_eq!(disjoint, TypeId::NEVER);
}

#[test]
fn test_interner_intersection_optional_object_literals_not_reduced() {
    let interner = TypeInterner::new();

    let kind = interner.intern_string("kind");
    let obj_a = interner.object(vec![PropertyInfo {
        name: kind,
        type_id: interner.literal_string("a"),
        write_type: interner.literal_string("a"),
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let obj_b = interner.object(vec![PropertyInfo {
        name: kind,
        type_id: interner.literal_string("b"),
        write_type: interner.literal_string("b"),
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    assert_ne!(intersection, TypeId::NEVER);
}

#[test]
fn test_interner_object_sorting() {
    let interner = TypeInterner::new();
    use std::sync::Arc;

    // Properties in different order should produce same TypeId
    let props1 = vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ];
    let props2 = vec![
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        props.push(PropertyInfo {
            name: interner.intern_string(&name),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        });
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

    let small = interner.object(vec![PropertyInfo {
        name: interner.intern_string("only"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
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

    let base = interner.reference(SymbolRef(1));
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
