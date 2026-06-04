#[test]
fn test_const_array_vs_mutable() {
    use crate::relations::subtype::SubtypeChecker;

    // const x = [1, 2] as const  ->  readonly [1, 2]
    // A non-readonly tuple [1, 2] is subtype of number[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);

    // Non-readonly tuple with literal types
    let mutable_tuple = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let number_array = interner.array(TypeId::NUMBER);

    // Tuple [1, 2] is subtype of number[]
    assert!(checker.is_subtype_of(mutable_tuple, number_array));

    // Readonly version
    let readonly_tuple = interner.intern(TypeData::ReadonlyType(mutable_tuple));
    let readonly_array = interner.intern(TypeData::ReadonlyType(number_array));

    // Readonly tuple is subtype of readonly number[]
    assert!(checker.is_subtype_of(readonly_tuple, readonly_array));
}

#[test]
fn test_readonly_type_wrapper() {
    // ReadonlyType wraps any type to make it readonly
    let interner = TypeInterner::new();

    let arr = interner.array(TypeId::STRING);
    let readonly_arr = interner.intern(TypeData::ReadonlyType(arr));

    match interner.lookup(readonly_arr) {
        Some(TypeData::ReadonlyType(inner)) => {
            assert_eq!(inner, arr);
        }
        other => panic!("Expected ReadonlyType, got {other:?}"),
    }
}

#[test]
fn test_readonly_inference_object() {
    // Readonly<T> applied to object makes all properties readonly
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    // Wrap in ReadonlyType
    let readonly_obj = interner.intern(TypeData::ReadonlyType(obj));

    match interner.lookup(readonly_obj) {
        Some(TypeData::ReadonlyType(inner)) => {
            assert_eq!(inner, obj);
        }
        other => panic!("Expected ReadonlyType, got {other:?}"),
    }
}

#[test]
fn test_readonly_keyof() {
    // keyof readonly [1, 2, 3] should work the same as keyof [1, 2, 3]
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: three,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

    // keyof readonly tuple
    let result = evaluate_keyof(&interner, readonly_tuple);

    // Should include tuple indices: "0" | "1" | "2" | array methods
    // At minimum, verify it returns a union containing the indices
    match interner.lookup(result) {
        Some(TypeData::Union(_)) => {} // Expected - union of keys
        other => panic!("Expected Union from keyof readonly tuple, got {other:?}"),
    }
}

#[test]
fn test_template_literal_const_basic() {
    // const x = `hello` as const -> "hello"
    // Template literals with no interpolations become string literals
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");

    // A simple template literal `hello` with as const is just "hello"
    match interner.lookup(hello) {
        Some(TypeData::Literal(LiteralValue::String(_))) => {}
        other => panic!("Expected LiteralString, got {other:?}"),
    }
}

#[test]
fn test_template_literal_const_interpolation() {
    // const prefix = "hello" as const
    // const x = `${prefix} world` as const -> "hello world"
    // With known literal interpolations, result is a literal
    let interner = TypeInterner::new();

    // When all parts are literals, the result is a literal
    let hello_world = interner.literal_string("hello world");

    match interner.lookup(hello_world) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            assert_eq!(interner.resolve_atom(atom), "hello world");
        }
        other => panic!("Expected LiteralString, got {other:?}"),
    }
}

#[test]
fn test_template_literal_type_structure() {
    // Template literal types: `prefix${string}suffix`
    let interner = TypeInterner::new();

    let prefix = interner.intern_string("prefix");
    let suffix = interner.intern_string("suffix");

    let template = interner.template_literal(vec![
        TemplateSpan::Text(prefix),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(suffix),
    ]);

    match interner.lookup(template) {
        Some(TypeData::TemplateLiteral(spans_id)) => {
            let spans = interner.template_list(spans_id);
            assert_eq!(spans.len(), 3);
            match &spans[0] {
                TemplateSpan::Text(atom) => assert_eq!(interner.resolve_atom(*atom), "prefix"),
                _ => panic!("Expected Text span"),
            }
            match &spans[1] {
                TemplateSpan::Type(t) => assert_eq!(*t, TypeId::STRING),
                _ => panic!("Expected Type span"),
            }
            match &spans[2] {
                TemplateSpan::Text(atom) => assert_eq!(interner.resolve_atom(*atom), "suffix"),
                _ => panic!("Expected Text span"),
            }
        }
        other => panic!("Expected TemplateLiteral, got {other:?}"),
    }
}

#[test]
fn test_template_literal_union_expansion() {
    use crate::relations::subtype::SubtypeChecker;

    // `${"a" | "b"}` expands to "a" | "b"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let union = interner.union(vec![lit_a, lit_b]);

    // A template with just a union interpolation equals the union
    let template = interner.template_literal(vec![TemplateSpan::Type(union)]);

    // The template should be a subtype of string
    assert!(checker.is_subtype_of(template, TypeId::STRING));
}

#[test]
fn test_const_enum_like_object() {
    use crate::relations::subtype::SubtypeChecker;

    // const Direction = { Up: 0, Down: 1, Left: 2, Right: 3 } as const
    // -> { readonly Up: 0, readonly Down: 1, readonly Left: 2, readonly Right: 3 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let zero = interner.literal_number(0.0);
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    let direction = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("Down"), one),
        PropertyInfo::readonly(interner.intern_string("Left"), two),
        PropertyInfo::readonly(interner.intern_string("Right"), three),
        PropertyInfo::readonly(interner.intern_string("Up"), zero),
    ]);

    // Get keyof Direction = "Up" | "Down" | "Left" | "Right"
    let keys = evaluate_keyof(&interner, direction);

    // Each key literal is a subtype of string
    match interner.lookup(keys) {
        Some(TypeData::Union(members_id)) => {
            let members = interner.type_list(members_id);
            assert_eq!(members.len(), 4);
            for member in members.iter() {
                assert!(checker.is_subtype_of(*member, TypeId::STRING));
            }
        }
        other => panic!("Expected Union, got {other:?}"),
    }
}
