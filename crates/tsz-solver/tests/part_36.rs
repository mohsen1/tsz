use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
/// Null and undefined handling
#[test]
fn test_null_undefined_extends() {
    let interner = TypeInterner::new();

    // null extends null
    let cond_null = ConditionalType {
        check_type: TypeId::NULL,
        extends_type: TypeId::NULL,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_null),
        interner.literal_boolean(true)
    );

    // undefined extends undefined
    let cond_undef = ConditionalType {
        check_type: TypeId::UNDEFINED,
        extends_type: TypeId::UNDEFINED,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_undef),
        interner.literal_boolean(true)
    );

    // null doesn't extend undefined
    let cond_null_undef = ConditionalType {
        check_type: TypeId::NULL,
        extends_type: TypeId::UNDEFINED,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_null_undef),
        interner.literal_boolean(false)
    );
}

/// Void and undefined relationship
#[test]
fn test_void_undefined_relationship() {
    let interner = TypeInterner::new();

    // undefined extends void
    let cond = ConditionalType {
        check_type: TypeId::UNDEFINED,
        extends_type: TypeId::VOID,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    assert_eq!(result, interner.literal_boolean(true));
}

/// Never is bottom type
#[test]
fn test_never_bottom_type() {
    let interner = TypeInterner::new();

    // never extends any type
    let cond_string = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::STRING,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_string),
        interner.literal_boolean(true)
    );

    let cond_number = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::NUMBER,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_number),
        interner.literal_boolean(true)
    );
}

/// Any and unknown are top types
#[test]
fn test_any_unknown_top_types() {
    let interner = TypeInterner::new();

    // string extends any
    let cond_any = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::ANY,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_any),
        interner.literal_boolean(true)
    );

    // string extends unknown
    let cond_unknown = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::UNKNOWN,
        true_type: interner.literal_boolean(true),
        false_type: interner.literal_boolean(false),
        is_distributive: false,
    };
    assert_eq!(
        evaluate_conditional(&interner, &cond_unknown),
        interner.literal_boolean(true)
    );
}

// ============================================================================
// const assertion (as const) tests
// The as const assertion creates readonly types with literal inference
// ============================================================================

#[test]
fn test_const_object_literal_readonly_properties() {
    // const x = { a: 1, b: "hello" } as const
    // -> { readonly a: 1, readonly b: "hello" }
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let hello = interner.literal_string("hello");

    // Object with readonly properties and literal types
    let const_obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: one,
            write_type: one,
            optional: false,
            readonly: true, // as const makes properties readonly
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo::readonly(interner.intern_string("b"), hello),
    ]);

    // Verify the object was created
    match interner.lookup(const_obj) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 2);
            // All properties should be readonly
            for prop in &shape.properties {
                assert!(prop.readonly);
            }
        }
        other => panic!("Expected Object type, got {other:?}"),
    }
}

#[test]
fn test_const_object_literal_nested() {
    // const x = { outer: { inner: 42 } } as const
    // -> { readonly outer: { readonly inner: 42 } }
    let interner = TypeInterner::new();

    let forty_two = interner.literal_number(42.0);

    // Inner object with readonly property
    let inner = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("inner"),
        forty_two,
    )]);

    // Outer object with readonly property pointing to inner
    let outer = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("outer"),
        inner,
    )]);

    match interner.lookup(outer) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert!(shape.properties[0].readonly);
            // The inner type should also be an object
            let inner_type = shape.properties[0].type_id;
            match interner.lookup(inner_type) {
                Some(TypeData::Object(inner_shape_id)) => {
                    let inner_shape = interner.object_shape(inner_shape_id);
                    assert!(inner_shape.properties[0].readonly);
                }
                other => panic!("Expected inner Object, got {other:?}"),
            }
        }
        other => panic!("Expected Object type, got {other:?}"),
    }
}

#[test]
fn test_const_object_literal_vs_mutable() {
    use crate::SubtypeChecker;

    // const x = { a: 1 } as const  ->  { readonly a: 1 }
    // let y = { a: 1 }             ->  { a: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);

    // as const version (readonly, literal type)
    let const_obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("a"),
        one,
    )]);

    // Same object but with widened type (still readonly for comparison)
    let widened_readonly = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    // Literal type is subtype of base type (when readonly matches)
    // { readonly a: 1 } is subtype of { readonly a: number }
    assert!(checker.is_subtype_of(const_obj, widened_readonly));

    // But not the other way around - number is not subtype of 1
    assert!(!checker.is_subtype_of(widened_readonly, const_obj));
}

#[test]
fn test_const_array_literal_tuple() {
    // const x = [1, 2, 3] as const
    // -> readonly [1, 2, 3]
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    // Create tuple with literal elements
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

    // Wrap in ReadonlyType for as const
    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

    match interner.lookup(readonly_tuple) {
        Some(TypeData::ReadonlyType(inner)) => {
            assert_eq!(inner, tuple);
            // Verify inner is a tuple
            match interner.lookup(inner) {
                Some(TypeData::Tuple(list_id)) => {
                    let elements = interner.tuple_list(list_id);
                    assert_eq!(elements.len(), 3);
                }
                other => panic!("Expected Tuple, got {other:?}"),
            }
        }
        other => panic!("Expected ReadonlyType, got {other:?}"),
    }
}

#[test]
fn test_const_array_mixed_types() {
    // const x = [1, "two", true] as const
    // -> readonly [1, "two", true]
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let two_str = interner.literal_string("two");
    let lit_true = interner.literal_boolean(true);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two_str,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: lit_true,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

    match interner.lookup(readonly_tuple) {
        Some(TypeData::ReadonlyType(inner)) => match interner.lookup(inner) {
            Some(TypeData::Tuple(list_id)) => {
                let elements = interner.tuple_list(list_id);
                assert_eq!(elements.len(), 3);
                assert_eq!(elements[0].type_id, one);
                assert_eq!(elements[1].type_id, two_str);
                assert_eq!(elements[2].type_id, lit_true);
            }
            other => panic!("Expected Tuple, got {other:?}"),
        },
        other => panic!("Expected ReadonlyType, got {other:?}"),
    }
}

#[test]
fn test_const_array_nested() {
    // const x = [[1, 2], [3, 4]] as const
    // -> readonly [readonly [1, 2], readonly [3, 4]]
    let interner = TypeInterner::new();

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);
    let four = interner.literal_number(4.0);

    let inner1 = interner.tuple(vec![
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
    let inner1_readonly = interner.intern(TypeData::ReadonlyType(inner1));

    let inner2 = interner.tuple(vec![
        TupleElement {
            type_id: three,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: four,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let inner2_readonly = interner.intern(TypeData::ReadonlyType(inner2));

    let outer = interner.tuple(vec![
        TupleElement {
            type_id: inner1_readonly,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: inner2_readonly,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let outer_readonly = interner.intern(TypeData::ReadonlyType(outer));

    match interner.lookup(outer_readonly) {
        Some(TypeData::ReadonlyType(inner)) => {
            match interner.lookup(inner) {
                Some(TypeData::Tuple(list_id)) => {
                    let elements = interner.tuple_list(list_id);
                    assert_eq!(elements.len(), 2);
                    // Each element should be ReadonlyType
                    for elem in elements.iter() {
                        match interner.lookup(elem.type_id) {
                            Some(TypeData::ReadonlyType(_)) => {}
                            other => panic!("Expected nested ReadonlyType, got {other:?}"),
                        }
                    }
                }
                other => panic!("Expected Tuple, got {other:?}"),
            }
        }
        other => panic!("Expected ReadonlyType, got {other:?}"),
    }
}

#[test]
fn test_const_array_vs_mutable() {
    use crate::SubtypeChecker;

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
    use crate::SubtypeChecker;

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
    use crate::SubtypeChecker;

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

// ============================================================================
// Omit<T, K> and Pick<T, K> Utility Type Tests
// ============================================================================

/// Basic Pick<T, K> - picks specific keys from an object type
/// Pick<{ a: number, b: string, c: boolean }, "a" | "b"> = { a: number, b: string }
#[test]
fn test_pick_basic() {
    let interner = TypeInterner::new();

    // Original type: { a: number, b: string, c: boolean }
    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");
    let key_c = interner.intern_string("c");

    let original = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_b, TypeId::STRING),
        PropertyInfo::new(key_c, TypeId::BOOLEAN),
    ]);

    // Keys to pick: "a" | "b"
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let pick_keys = interner.union(vec![lit_a, lit_b]);

    // Pick<T, K> = { [P in K]: T[P] }
    let key_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(pick_keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    // Template: T[P] - index access
    let index_access = interner.intern(TypeData::IndexAccess(original, key_param_id));

    let mapped = MappedType {
        type_param: key_param,
        constraint: pick_keys,
        name_type: None,
        template: index_access,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    // Expected: { a: number, b: string }
    let expected = interner.object(vec![
        PropertyInfo::new(key_a, TypeId::NUMBER),
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    assert_eq!(result, expected);
}

