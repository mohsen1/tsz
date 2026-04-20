use super::*;

// =================================================================
// Union formatting
// =================================================================

#[test]
fn format_union_two_members() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let union = db.union2(TypeId::STRING, TypeId::NUMBER);
    let result = fmt.format(union);
    assert!(result.contains("string"));
    assert!(result.contains("number"));
    assert!(result.contains(" | "));
}

#[test]
fn format_union_three_members() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let union = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let result = fmt.format(union);
    assert!(result.contains("string"));
    assert!(result.contains("number"));
    assert!(result.contains("boolean"));
    // Should have exactly 2 "|" separators
    assert_eq!(result.matches(" | ").count(), 2);
}

#[test]
fn format_union_with_literal_members() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let s1 = db.literal_string("a");
    let s2 = db.literal_string("b");
    let union = db.union2(s1, s2);
    let result = fmt.format(union);
    assert!(result.contains("\"a\""));
    assert!(result.contains("\"b\""));
    assert!(result.contains(" | "));
}

#[test]
fn format_union_named_construct_callable_without_parentheses() {
    let db = TypeInterner::new();
    let mut symbols = tsz_binder::SymbolArena::new();
    let sym_id = symbols.alloc(tsz_binder::symbol_flags::INTERFACE, "ConstructableA".into());

    let constructable = db.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: Some(sym_id),
        is_abstract: false,
    });

    let union = db.union2(constructable, TypeId::STRING);
    let mut fmt = TypeFormatter::with_symbols(&db, &symbols);
    let rendered = fmt.format(union);
    assert!(rendered.contains("ConstructableA"));
    assert!(rendered.contains("string"));
    assert!(!rendered.contains("(ConstructableA)"));
}

#[test]
fn format_large_union_truncation() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Create a union with more members than max_union_members (default: 10)
    let members: Vec<TypeId> = (0..15).map(|i| db.literal_number(i as f64)).collect();
    let union = db.union_preserve_members(members);
    let result = fmt.format(union);
    // Should truncate with "..."
    assert!(
        result.contains("..."),
        "Large union should be truncated, got: {result}"
    );
}

// =================================================================
// Intersection formatting
// =================================================================

#[test]
fn format_intersection_two_type_params() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let u = db.type_param(TypeParamInfo {
        name: db.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let inter = db.intersection2(t, u);
    let result = fmt.format(inter);
    assert!(result.contains("T"));
    assert!(result.contains("U"));
    assert!(result.contains(" & "));
}

#[test]
fn format_intersection_uses_display_properties_for_anonymous_object_member() {
    let db = TypeInterner::new();
    let foo_prop = db.intern_string("fooProp");
    let widened = PropertyInfo::new(foo_prop, TypeId::STRING);
    let display = PropertyInfo::new(foo_prop, db.literal_string("frizzlebizzle"));
    let fresh = db
        .factory()
        .object_fresh_with_display(vec![widened], vec![display]);
    let t = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });

    let intersection = db.intersection2(fresh, t);
    let mut fmt = TypeFormatter::new(&db).with_display_properties();
    let result = fmt.format(intersection);

    assert!(
        result.contains("{ fooProp: \"frizzlebizzle\"; }"),
        "Expected fresh-object display properties inside intersection, got: {result}"
    );
    assert!(result.contains(" & "));
}

#[test]
fn format_intersection_preserves_anonymous_objects() {
    // tsc's `typeToString` preserves the intersection form (`A & B`) for
    // IntersectionType values, even when every member is an anonymous object
    // literal type. A merged single-object display is only produced when the
    // type is already stored as a single object (e.g. via spread/apparent-type
    // computation). See intersectionsAndOptionalProperties.ts and
    // jsxEmptyExpressionNotCountedAsChild2.tsx for cases that depend on this.
    let db = TypeInterner::new();

    let a_prop = PropertyInfo::new(db.intern_string("a"), TypeId::NULL);
    let b_prop = PropertyInfo::new(db.intern_string("b"), TypeId::STRING);

    let obj_a = db.factory().object(vec![a_prop]);
    let obj_b = db.factory().object(vec![b_prop]);

    let intersection = db.intersection2(obj_a, obj_b);
    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(intersection);

    assert!(
        result.contains(" & "),
        "Intersection of anonymous objects should keep `&` display, got: {result}"
    );
    assert!(
        result.contains("a: null") && result.contains("b: string"),
        "Intersection display should contain both members' properties, got: {result}"
    );
}

#[test]
fn format_intersection_preserves_named_types() {
    // Intersections with named types (type params) should NOT be flattened
    let db = TypeInterner::new();

    let a_prop = PropertyInfo::new(db.intern_string("a"), TypeId::NULL);
    let obj_a = db.factory().object(vec![a_prop]);
    let t = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });

    let intersection = db.intersection2(obj_a, t);
    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(intersection);

    // Should preserve intersection form: `{ a: null; } & T`
    assert!(
        result.contains(" & "),
        "Intersection with type param should not be flattened, got: {result}"
    );
}

// =================================================================
// Object type formatting
// =================================================================

#[test]
fn format_empty_object() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let obj = db.object(vec![]);
    assert_eq!(fmt.format(obj), "{}");
}

#[test]
fn format_object_single_property() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let obj = db.object(vec![PropertyInfo::new(
        db.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert_eq!(fmt.format(obj), "{ x: number; }");
}

#[test]
fn format_object_multiple_properties() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let obj = db.object(vec![
        PropertyInfo::new(db.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(db.intern_string("y"), TypeId::STRING),
    ]);
    let result = fmt.format(obj);
    assert!(result.contains("x: number"));
    assert!(result.contains("y: string"));
}

#[test]
fn format_object_readonly_property() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mut prop = PropertyInfo::new(db.intern_string("x"), TypeId::NUMBER);
    prop.readonly = true;
    let obj = db.object(vec![prop]);
    let result = fmt.format(obj);
    assert!(
        result.contains("readonly x: number"),
        "Expected 'readonly x: number', got: {result}"
    );
}

#[test]
fn format_object_many_properties_truncated() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // tsc starts truncating large object displays (roughly 22+ members),
    // preserving a long head and the tail property.
    let props: Vec<PropertyInfo> = (1..=24)
        .map(|i| PropertyInfo::new(db.intern_string(&format!("p{i}")), TypeId::NUMBER))
        .collect();
    let obj = db.object(props);
    let result = fmt.format(obj);
    assert!(
        result.contains("... 6 more ..."),
        "Expected omitted-count marker for large object, got: {result}"
    );
    assert!(
        result.contains("p24: number"),
        "Expected tail property preservation in truncated object display, got: {result}"
    );
}

#[test]
fn format_object_hides_duplicate_internal_default_alias() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let shared = TypeId::NUMBER;
    let obj = db.object(vec![
        PropertyInfo::new(db.intern_string("default"), shared),
        PropertyInfo::new(db.intern_string("_default"), shared),
        PropertyInfo::new(db.intern_string("value"), TypeId::STRING),
    ]);
    let result = fmt.format(obj);

    assert!(
        result.contains("default: number"),
        "Expected real default export to remain visible, got: {result}"
    );
    assert!(
        !result.contains("_default"),
        "Expected duplicate internal `_default` alias to be hidden, got: {result}"
    );
    assert!(
        result.contains("value: string"),
        "Expected unrelated properties to remain visible, got: {result}"
    );
}

#[test]
fn format_object_keeps_distinct_internal_default_alias() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let obj = db.object(vec![
        PropertyInfo::new(db.intern_string("default"), TypeId::NUMBER),
        PropertyInfo::new(db.intern_string("_default"), TypeId::STRING),
    ]);
    let result = fmt.format(obj);

    assert!(
        result.contains("_default: string"),
        "Expected `_default` to remain when it is not a duplicate of `default`, got: {result}"
    );
}

#[test]
fn format_object_with_string_index_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let shape = crate::types::ObjectShape {
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);
    assert!(
        result.contains("[x: string]: number"),
        "Expected string index signature with default param name 'x', got: {result}"
    );
}

#[test]
fn format_object_with_index_hides_duplicate_internal_default_alias() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let shape = crate::types::ObjectShape {
        properties: vec![
            PropertyInfo::new(db.intern_string("default"), TypeId::NUMBER),
            PropertyInfo::new(db.intern_string("_default"), TypeId::NUMBER),
        ],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);

    assert!(
        result.contains("[x: string]: number"),
        "Expected index signature to remain visible, got: {result}"
    );
    assert!(
        result.contains("default: number"),
        "Expected real default export to remain visible, got: {result}"
    );
    assert!(
        !result.contains("_default"),
        "Expected duplicate internal `_default` alias to be hidden in object-with-index display, got: {result}"
    );
}

#[test]
fn format_object_with_number_index_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let shape = crate::types::ObjectShape {
        properties: vec![],
        string_index: None,
        number_index: Some(crate::types::IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);
    assert!(
        result.contains("[x: number]: string"),
        "Expected number index signature with default param name 'x', got: {result}"
    );
}

#[test]
fn format_object_with_readonly_number_index_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let shape = crate::types::ObjectShape {
        properties: vec![],
        string_index: None,
        number_index: Some(crate::types::IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: true,
            param_name: None,
        }),
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);
    assert!(
        result.contains("readonly [x: number]: string"),
        "Expected readonly number index signature, got: {result}"
    );
}

#[test]
fn format_object_with_readonly_string_index_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let shape = crate::types::ObjectShape {
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);
    assert!(
        result.contains("readonly [x: string]: number"),
        "Expected readonly string index signature, got: {result}"
    );
}

#[test]
fn format_object_with_index_many_properties_truncated() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mut props: Vec<PropertyInfo> = (1..=20)
        .map(|i| PropertyInfo::new(db.intern_string(&format!("p{i}")), TypeId::NUMBER))
        .collect();
    let mut tail = PropertyInfo::new(
        db.intern_string("[Symbol.unscopables]"),
        db.object(vec![PropertyInfo::new(
            db.intern_string("a"),
            TypeId::NUMBER,
        )]),
    );
    tail.readonly = true;
    props.push(tail);

    let shape = crate::types::ObjectShape {
        properties: props,
        string_index: None,
        number_index: Some(crate::types::IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        symbol: None,
        flags: Default::default(),
    };
    let obj = db.object_with_index(shape);
    let result = fmt.format(obj);
    assert!(
        result.contains("... 4 more ..."),
        "Expected omitted-count marker for indexed object truncation, got: {result}"
    );
    assert!(
        result.contains("readonly [Symbol.unscopables]:"),
        "Expected tail symbol property preservation in indexed-object truncation, got: {result}"
    );
}
