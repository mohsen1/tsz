#[test]
fn string_interning_dedup() {
    let i = TypeInterner::new();
    let a = i.intern_string("hello");
    let b = i.intern_string("hello");
    let c = i.intern_string("world");
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn string_resolve_round_trip() {
    let i = TypeInterner::new();
    let atom = i.intern_string("test_value");
    assert_eq!(i.resolve_atom(atom), "test_value");
}

#[test]
fn all_intrinsics_exist() {
    let i = TypeInterner::new();
    let intrinsics = [
        TypeId::ANY,
        TypeId::UNKNOWN,
        TypeId::NEVER,
        TypeId::VOID,
        TypeId::UNDEFINED,
        TypeId::NULL,
        TypeId::BOOLEAN,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::BIGINT,
        TypeId::SYMBOL,
        TypeId::OBJECT,
        TypeId::BOOLEAN_TRUE,
        TypeId::BOOLEAN_FALSE,
        TypeId::FUNCTION,
    ];
    for id in intrinsics {
        assert!(i.lookup(id).is_some(), "Intrinsic {id:?} must exist");
    }
}

#[test]
fn intrinsic_lookup_round_trip() {
    let i = TypeInterner::new();
    assert_eq!(
        i.lookup(TypeId::STRING),
        Some(TypeData::Intrinsic(IntrinsicKind::String))
    );
    assert_eq!(
        i.lookup(TypeId::NUMBER),
        Some(TypeData::Intrinsic(IntrinsicKind::Number))
    );
    assert_eq!(
        i.lookup(TypeId::BOOLEAN),
        Some(TypeData::Intrinsic(IntrinsicKind::Boolean))
    );
    assert_eq!(
        i.lookup(TypeId::VOID),
        Some(TypeData::Intrinsic(IntrinsicKind::Void))
    );
    assert_eq!(
        i.lookup(TypeId::NULL),
        Some(TypeData::Intrinsic(IntrinsicKind::Null))
    );
    assert_eq!(
        i.lookup(TypeId::UNDEFINED),
        Some(TypeData::Intrinsic(IntrinsicKind::Undefined))
    );
    assert_eq!(
        i.lookup(TypeId::NEVER),
        Some(TypeData::Intrinsic(IntrinsicKind::Never))
    );
    assert_eq!(
        i.lookup(TypeId::ANY),
        Some(TypeData::Intrinsic(IntrinsicKind::Any))
    );
    assert_eq!(
        i.lookup(TypeId::UNKNOWN),
        Some(TypeData::Intrinsic(IntrinsicKind::Unknown))
    );
}

#[test]
fn type_id_stability_across_constructions() {
    let i = TypeInterner::new();
    // Construct a complex type, then construct it again
    let obj = i.object(vec![
        PropertyInfo::new(i.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(i.intern_string("y"), TypeId::STRING),
    ]);
    let arr = i.array(obj);
    let union = i.union(vec![arr, TypeId::NULL]);

    // Do it all again
    let obj2 = i.object(vec![
        PropertyInfo::new(i.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(i.intern_string("y"), TypeId::STRING),
    ]);
    let arr2 = i.array(obj2);
    let union2 = i.union(vec![arr2, TypeId::NULL]);

    assert_eq!(union, union2, "Same construction must yield same TypeId");
}

#[test]
fn large_union_many_literals() {
    let i = TypeInterner::new();
    let members: Vec<TypeId> = (0..100).map(|n| i.literal_number(n as f64)).collect();
    let u = i.union(members);
    match i.lookup(u) {
        Some(TypeData::Union(list_id)) => {
            let members = i.type_list(list_id);
            assert_eq!(members.len(), 100);
        }
        _ => panic!("Expected union with 100 members"),
    }
}

#[test]
fn large_union_with_primitive_absorbs_all() {
    let i = TypeInterner::new();
    let mut members: Vec<TypeId> = (0..50).map(|n| i.literal_number(n as f64)).collect();
    members.push(TypeId::NUMBER);
    let u = i.union(members);
    assert_eq!(u, TypeId::NUMBER, "50 number literals + number => number");
}

#[test]
fn intersection_functions_merge_to_callable() {
    let i = TypeInterner::new();
    let f1 = i.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let f2 = i.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let inter = i.intersection(vec![f1, f2]);
    match i.lookup(inter) {
        Some(TypeData::Callable(cid)) => {
            let shape = i.callable_shape(cid);
            assert_eq!(
                shape.call_signatures.len(),
                2,
                "Two functions should merge into callable with 2 sigs"
            );
        }
        _ => panic!("Expected callable type from function intersection"),
    }
}

#[test]
fn bigint_hex_normalized() {
    let i = TypeInterner::new();
    let a = i.literal_bigint("0xFF");
    let b = i.literal_bigint("255");
    assert_eq!(a, b, "0xFF and 255 should normalize to same bigint");
}
