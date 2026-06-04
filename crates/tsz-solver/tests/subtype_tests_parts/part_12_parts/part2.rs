#[test]
fn test_readonly_with_literal_type() {
    // { readonly status: "active" | "inactive" }
    let interner = TypeInterner::new();

    let lit_active = interner.literal_string("active");
    let lit_inactive = interner.literal_string("inactive");
    let status_union = interner.union(vec![lit_active, lit_inactive]);

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("status"),
        status_union,
    )]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_with_number_index() {
    // { readonly [index: number]: string }
    let interner = TypeInterner::new();

    let readonly_number_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: true,
            param_name: None,
        }),
    });

    assert!(readonly_number_index != TypeId::ERROR);
}

#[test]
fn test_readonly_intersection() {
    // { readonly a: string } & { readonly b: number }
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    assert!(intersection != TypeId::ERROR);
}

#[test]
fn test_readonly_in_generic_context() {
    // Container<T> = { readonly value: T }
    let interner = TypeInterner::new();

    let t_ref = interner.lazy(DefId(50));

    let container = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("value"),
        t_ref,
    )]);

    assert!(container != TypeId::ERROR);
}

#[test]
fn test_readonly_preserves_subtype_covariance() {
    // { readonly x: "a" } is subtype of { readonly x: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_a = interner.literal_string("a");

    let readonly_literal = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        lit_a,
    )]);

    let readonly_string = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    // Literal is subtype of wider type (covariant)
    assert!(checker.is_subtype_of(readonly_literal, readonly_string));
}

#[test]
fn test_readonly_with_this_type() {
    // { readonly self: this }
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("self"),
        this_type,
    )]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_with_tuple_property() {
    // { readonly coords: [number, number] }
    let interner = TypeInterner::new();

    let coords = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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
    ]);

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("coords"),
        coords,
    )]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_with_readonly_tuple_property() {
    // { readonly coords: readonly [number, number] }
    let interner = TypeInterner::new();

    let readonly_coords = interner.readonly_tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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
    ]);

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("coords"),
        readonly_coords,
    )]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_mapped_type_pattern() {
    // Simulating Readonly<T> mapped type result
    // { readonly a: string, readonly b: number }
    let interner = TypeInterner::new();

    let readonly_all = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let mutable_all = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let mut checker = SubtypeChecker::new(&interner);

    // Mutable is subtype of readonly
    assert!(checker.is_subtype_of(mutable_all, readonly_all));
}

#[test]
fn test_readonly_class_instance_properties() {
    // Class instance: { readonly id: string, readonly createdAt: number }
    let interner = TypeInterner::new();

    let instance = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("id"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("createdAt"), TypeId::NUMBER),
    ]);

    assert!(instance != TypeId::ERROR);
}

#[test]
fn test_readonly_with_bigint() {
    // { readonly value: bigint }
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("value"),
        TypeId::BIGINT,
    )]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_with_symbol() {
    // { readonly sym: symbol }
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("sym"),
        TypeId::SYMBOL,
    )]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_with_null_union() {
    // { readonly value: string | null }
    let interner = TypeInterner::new();

    let nullable = interner.union(vec![TypeId::STRING, TypeId::NULL]);

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("value"),
        nullable,
    )]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_config_pattern() {
    // Config object: { readonly host: string, readonly port: number, readonly debug: boolean }
    let interner = TypeInterner::new();

    let config = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("host"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("port"), TypeId::NUMBER),
        PropertyInfo::readonly(interner.intern_string("debug"), TypeId::BOOLEAN),
    ]);

    assert!(config != TypeId::ERROR);
}
