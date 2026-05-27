#[test]
fn test_template_literal_null_undefined() {
    // `${null}` and `${undefined}` - special types in template
    // TypeScript expands these to string literals "null" and "undefined"
    let interner = TypeInterner::new();

    let template_null = interner.template_literal(vec![TemplateSpan::Type(TypeId::NULL)]);
    let template_undefined = interner.template_literal(vec![TemplateSpan::Type(TypeId::UNDEFINED)]);

    // Both should expand to string literals
    match interner.lookup(template_null) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            let s = interner.resolve_atom_ref(atom);
            assert_eq!(s.as_ref(), "null", "Expected 'null' string literal");
        }
        _ => panic!(
            "Expected string literal 'null' for `${{null}}`, got {:?}",
            interner.lookup(template_null)
        ),
    }
    match interner.lookup(template_undefined) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            let s = interner.resolve_atom_ref(atom);
            assert_eq!(
                s.as_ref(),
                "undefined",
                "Expected 'undefined' string literal"
            );
        }
        _ => panic!(
            "Expected string literal 'undefined' for `${{undefined}}`, got {:?}",
            interner.lookup(template_undefined)
        ),
    }
}

#[test]
fn test_template_literal_subtype_of_string() {
    // `foo_${T}` should extend string when T is string
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // Template literal types extend string
    let mut checker = SubtypeChecker::new(&interner);
    let extends = checker.is_subtype_of(template, TypeId::STRING);
    // Should be true - all template literal types are subtypes of string
    assert!(extends);
}

#[test]
fn test_template_literal_specific_extends_pattern() {
    // "foo_bar" extends `foo_${string}`
    let interner = TypeInterner::new();

    let literal = interner.literal_string("foo_bar");
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("foo_")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    let extends = checker.is_subtype_of(literal, pattern);
    // "foo_bar" should extend `foo_${string}`
    assert!(extends);
}

// =============================================================================
// KEYOF EDGE CASES - INTERSECTION AND UNION
// =============================================================================

#[test]
fn test_keyof_intersection_with_never() {
    // keyof (T & never) should be never (never absorbs in intersection)
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj, TypeId::NEVER]);
    let result = evaluate_keyof(&interner, intersection);

    // Intersection with never is never, so keyof never is PropertyKey.
    assert_eq!(
        result,
        interner.union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL)
    );
}

#[test]
fn test_keyof_union_with_any() {
    // keyof (T | any) - any absorbs the union
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let union = interner.union(vec![obj, TypeId::ANY]);
    let result = evaluate_keyof(&interner, union);

    // Union with any is any, keyof any is string | number | symbol
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_keyof_intersection_with_any() {
    // keyof (T & any) - any in intersection
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj, TypeId::ANY]);
    let result = evaluate_keyof(&interner, intersection);

    // Should produce keys from both
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_keyof_union_with_unknown() {
    // keyof (T | unknown) - unknown absorbs in union
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let union = interner.union(vec![obj, TypeId::UNKNOWN]);
    let result = evaluate_keyof(&interner, union);

    // keyof unknown is never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_keyof_four_way_intersection() {
    // keyof (A & B & C & D) = keyof A | keyof B | keyof C | keyof D
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let obj_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::BOOLEAN,
    )]);

    let obj_d = interner.object(vec![PropertyInfo::new(
        interner.intern_string("d"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b, obj_c, obj_d]);
    let result = evaluate_keyof(&interner, intersection);

    // Should produce "a" | "b" | "c" | "d"
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let lit_d = interner.literal_string("d");
    let expected = interner.union(vec![lit_a, lit_b, lit_c, lit_d]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_four_way_union() {
    // keyof (A | B | C | D) = only common keys
    let interner = TypeInterner::new();

    let common_key = interner.intern_string("common");

    let obj_a = interner.object(vec![
        PropertyInfo::new(common_key, TypeId::STRING),
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
    ]);

    let obj_b = interner.object(vec![
        PropertyInfo::new(common_key, TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let obj_c = interner.object(vec![PropertyInfo::new(common_key, TypeId::STRING)]);

    let obj_d = interner.object(vec![
        PropertyInfo::new(common_key, TypeId::STRING),
        PropertyInfo::new(interner.intern_string("d"), TypeId::BOOLEAN),
    ]);

    let union = interner.union(vec![obj_a, obj_b, obj_c, obj_d]);
    let result = evaluate_keyof(&interner, union);

    // Only "common" is present in all
    let expected = interner.literal_string("common");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_mixed_intersection_union() {
    // keyof ((A & B) | C) - nested combination
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("common"), TypeId::STRING),
    ]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let obj_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("common"),
        TypeId::STRING,
    )]);

    let a_and_b = interner.intersection(vec![obj_a, obj_b]);
    let union = interner.union(vec![a_and_b, obj_c]);
    let result = evaluate_keyof(&interner, union);

    // Common keys between (A & B) and C = "common"
    let expected = interner.literal_string("common");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_intersection_both_index_signatures() {
    // keyof ({ [k: string]: T } & { [k: number]: U }) = string | number
    let interner = TypeInterner::new();

    let string_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let number_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let intersection = interner.intersection(vec![string_indexed, number_indexed]);
    let result = evaluate_keyof(&interner, intersection);

    // Should be string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_union_index_and_literal() {
    // keyof ({ [k: string]: T } | { a: U }) - intersection of keys
    let interner = TypeInterner::new();

    let string_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let literal_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let union = interner.union(vec![string_indexed, literal_obj]);
    let result = evaluate_keyof(&interner, union);

    // "a" is subtype of string, so "a" is the common key
    let expected = interner.literal_string("a");
    assert_eq!(result, expected);
}

#[test]
fn test_keyof_intersection_with_callable() {
    // keyof (T & { (): void }) - object with call signature
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    let intersection = interner.intersection(vec![obj, callable]);
    let result = evaluate_keyof(&interner, intersection);

    // Should at least include "a" from the object
    let lit_a = interner.literal_string("a");
    let mut checker = SubtypeChecker::new(&interner);
    assert!(checker.is_subtype_of(lit_a, result));
}

#[test]
fn test_keyof_intersection_with_array() {
    // keyof ({ a: T } & string[]) - object intersected with array
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let arr = interner.array(TypeId::STRING);
    let intersection = interner.intersection(vec![obj, arr]);
    let result = evaluate_keyof(&interner, intersection);

    // Should include array keys (number index) plus "a" plus array methods
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_keyof_empty_intersection() {
    // keyof (A & B) where A and B have disjoint primitive types
    // This is different from object intersection - primitive intersection is never
    let interner = TypeInterner::new();

    // string & number = never
    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    let result = evaluate_keyof(&interner, intersection);

    // Intersection of disjoint primitives is never, so keyof never is PropertyKey.
    assert_eq!(
        result,
        interner.union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL)
    );
}

#[test]
fn test_keyof_empty_union() {
    // keyof never = string | number | symbol
    let interner = TypeInterner::new();

    let result = evaluate_keyof(&interner, TypeId::NEVER);
    assert_eq!(
        result,
        interner.union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL)
    );
}

#[test]
fn test_keyof_nested_keyof() {
    // keyof keyof T - nested keyof application
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let keyof_obj = evaluate_keyof(&interner, obj);
    // keyof_obj = "a" | "b"

    // Now keyof (keyof obj) = keyof ("a" | "b") = keyof string (apparent members)
    let keyof_keyof = evaluate_keyof(&interner, keyof_obj);

    // String literal unions extend string, so keyof should give string apparent members
    assert!(keyof_keyof != TypeId::ERROR);
}

// ==================== Callable-parameter inference regression tests ====================

#[test]
fn test_callable_param_infer_union_of_signatures() {
    // T extends ((x: infer P) => any) ? P : never
    // where T = ((x: string) => void) | ((x: number) => void)
    // Result should be string | number (extracting param from both signatures)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");
    let t_param = test_type_param_from_name(&interner, t_name);
    let infer_p = test_infer_param_from_name(&interner, p_name);

    // Pattern: (x: infer P) => any
    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_p)],
        return_type: TypeId::ANY,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Create union of two function signatures: ((x: string) => void) | ((x: number) => void)
    let fn_string = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });
    let fn_number = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });
    let fn_union = interner.union(vec![fn_string, fn_number]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, fn_union);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Distributive: (string) | (number) = string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_callable_param_infer_overloaded_callable() {
    // T extends { (x: infer P): any } ? P : never
    // where T = { (x: string): void; (x: number): void }
    // For overloaded callables, TypeScript uses the last signature's param
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");
    let t_param = test_type_param_from_name(&interner, t_name);
    let infer_p = test_infer_param_from_name(&interner, p_name);

    // Pattern: { (x: infer P): any }
    let pattern_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(infer_p)],
            return_type: TypeId::ANY,
            type_predicate: None,
            this_type: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_callable,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    // Create overloaded callable with two call signatures
    let overloaded = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                params: vec![ParamInfo::unnamed(TypeId::STRING)],
                return_type: TypeId::VOID,
                type_predicate: None,
                this_type: None,
                type_params: Vec::new(),
                is_method: false,
            },
            CallSignature {
                params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
                return_type: TypeId::BOOLEAN,
                type_predicate: None,
                this_type: None,
                type_params: Vec::new(),
                is_method: false,
            },
        ],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, overloaded);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Current behavior: callable matching doesn't yet extract from overloads
    // This returns never because Callable vs Callable matching with infer patterns
    // is not fully implemented for extracting from last signature.
    // TODO: Implement proper overload signature extraction for infer patterns
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_callable_param_infer_mixed_union() {
    // T extends ((x: infer P) => any) ? P : never
    // where T = ((x: string) => void) | number
    // Result: string (number doesn't match the pattern so it goes to never branch)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");
    let t_param = test_type_param_from_name(&interner, t_name);
    let infer_p = test_infer_param_from_name(&interner, p_name);

    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_p)],
        return_type: TypeId::ANY,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    let fn_string = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });
    let mixed_union = interner.union(vec![fn_string, TypeId::NUMBER]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, mixed_union);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // string (from fn) | never (from number) = string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_callable_return_and_param_infer_separately() {
    // T extends ((x: infer P) => infer R) ? [P, R] : never
    // where T = (x: string) => number
    // Result: [string, number] represented as a tuple
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let p_name = interner.intern_string("P");
    let r_name = interner.intern_string("R");
    let t_param = test_type_param_from_name(&interner, t_name);
    let infer_p = test_infer_param_from_name(&interner, p_name);
    let infer_r = test_infer_param_from_name(&interner, r_name);

    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_p)],
        return_type: infer_r,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    // True type: tuple [P, R]
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: infer_p,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: infer_r,
            optional: false,
            rest: false,
            name: None,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: tuple_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    let source_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
        return_type: TypeId::NUMBER,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
            name: None,
        },
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_callable_multiple_params_infer() {
    // T extends ((a: infer A, b: infer B) => any) ? [A, B] : never
    // where T = (a: string, b: number) => void
    // Result: [string, number]
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let t_param = test_type_param_from_name(&interner, t_name);
    let infer_a = test_infer_param_from_name(&interner, a_name);
    let infer_b = test_infer_param_from_name(&interner, b_name);

    let pattern_fn = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(infer_a), ParamInfo::unnamed(infer_b)],
        return_type: TypeId::ANY,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: infer_a,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: infer_b,
            optional: false,
            rest: false,
            name: None,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: tuple_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    let source_fn = interner.function(FunctionShape {
        params: vec![
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::NUMBER),
        ],
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
            name: None,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
            name: None,
        },
    ]);
    assert_eq!(result, expected);
}

// =============================================================================
// Mapped Type Edge Cases - Homomorphic Modifiers & Key Remapping
// =============================================================================
// These tests cover advanced mapped type scenarios including homomorphic
// modifier preservation, complex key remapping, and edge cases.

#[test]
fn test_mapped_type_homomorphic_preserves_optional() {
    // Homomorphic: { [K in keyof T]: T[K] } preserves optional from source
    let interner = TypeInterner::new();

    // Source type with optional property
    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("required"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("optional"), TypeId::NUMBER),
    ]);

    let keyof_source = interner.intern(TypeData::KeyOf(source));

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_source,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_homomorphic_preserves_readonly() {
    // Homomorphic: { [K in keyof T]: T[K] } preserves readonly from source
    let interner = TypeInterner::new();

    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("mutable"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("immutable"), TypeId::NUMBER),
    ]);

    let keyof_source = interner.intern(TypeData::KeyOf(source));

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_source,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_key_remap_to_getter_setter() {
    // Key remapping: { [K in keyof T as `get${Capitalize<K>}`]: () => T[K] }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    // Simulate key remapping with template literal
    let get_x = interner.literal_string("getX");
    let get_y = interner.literal_string("getY");
    let remapped_keys = interner.union(vec![get_x, get_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: Some(remapped_keys),
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_key_remap_filter_by_type() {
    // Filter keys: { [K in keyof T as T[K] extends string ? K : never]: T[K] }
    let interner = TypeInterner::new();

    let key_name = interner.literal_string("name");
    let key_age = interner.literal_string("age");
    let keys = interner.union(vec![key_name, key_age]);

    // Only "name" passes filter (string type), "age" becomes never
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: Some(key_name),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_nested_mapped() {
    // Nested: { [K in keyof T]: { [J in keyof T[K]]: boolean } }
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let outer_keys = interner.union(vec![key_a, key_b]);

    let inner_template = TypeId::BOOLEAN;

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: outer_keys,
        name_type: None,
        template: inner_template,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_with_conditional_template() {
    // Conditional template: { [K in keyof T]: T[K] extends string ? number : boolean }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    };
    let cond_template = interner.conditional(cond);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: cond_template,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_union_key_constraint() {
    // Keys from union of object types
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let union = interner.union(vec![obj_a, obj_b]);
    let keyof_union = interner.intern(TypeData::KeyOf(union));

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_union,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_intersection_source() {
    // Keys from intersection: keyof (A & B)
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    let keyof_intersection = interner.intern(TypeData::KeyOf(intersection));

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keyof_intersection,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_key_remap_exclude_pattern() {
    // Exclude pattern: { [K in keyof T as Exclude<K, "internal">]: T[K] }
    let interner = TypeInterner::new();

    let key_public = interner.literal_string("public");
    let key_internal = interner.literal_string("internal");
    let keys = interner.union(vec![key_public, key_internal]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: Some(key_public),
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_deep_readonly() {
    // DeepReadonly: { readonly [K in keyof T]: DeepReadonly<T[K]> }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::OBJECT,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_mapped_type_pick_pattern() {
    // Pick<T, K>: { [P in K]: T[P] }
    let interner = TypeInterner::new();

    let key_a = interner.literal_string("a");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("P"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: key_a,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_record_pattern() {
    // Record<K, T>: { [P in K]: T }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let key_z = interner.literal_string("z");
    let keys = interner.union(vec![key_x, key_y, key_z]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("P"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("z"), TypeId::NUMBER),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_mutable_pattern() {
    // Mutable<T>: { -readonly [K in keyof T]: T[K] }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_required_pattern() {
    // Required<T>: { [K in keyof T]-?: T[K] }
    let interner = TypeInterner::new();

    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_empty_keys() {
    // Mapped type over never (empty key set)
    let interner = TypeInterner::new();

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::NEVER,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    let expected = interner.object(vec![]);
    assert_eq!(result, expected);
}

#[test]
fn test_mapped_type_single_literal_key() {
    // Single literal key: { [K in "only"]: number }
    let interner = TypeInterner::new();

    let key = interner.literal_string("only");

    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: key,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);

    let expected = interner.object(vec![PropertyInfo::new(
        interner.intern_string("only"),
        TypeId::NUMBER,
    )]);
    assert_eq!(result, expected);
}

// ==================== Function return inference edge case tests ====================

#[test]
fn test_infer_return_void_vs_undefined() {
    // T extends () => infer R ? R : never
    // where T = () => void
    // Result should be void (not undefined)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");
    let t_param = test_type_param_from_name(&interner, t_name);
    let infer_r = test_infer_param_from_name(&interner, r_name);

    let pattern_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: infer_r,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    let source_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: TypeId::VOID,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::VOID);
}

#[test]
fn test_infer_return_promise_like() {
    // T extends () => infer R ? R : never
    // where T = () => Promise<string>
    // Result should be Promise<string> (as an object type)
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");
    let t_param = test_type_param_from_name(&interner, t_name);
    let infer_r = test_infer_param_from_name(&interner, r_name);

    let pattern_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: infer_r,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    // Create a simple Promise-like object { then(cb: (v: string) => void): void }
    let then_name = interner.intern_string("then");
    let promise_string = interner.object(vec![PropertyInfo {
        name: then_name,
        type_id: TypeId::ANY, // Simplified, normally this would be a function
        write_type: TypeId::ANY,
        optional: false,
        readonly: false,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let source_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: promise_string,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, promise_string);
}

#[test]
fn test_infer_return_union() {
    // T extends () => infer R ? R : never
    // where T = () => (string | number)
    // Result should be string | number
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");
    let t_param = test_type_param_from_name(&interner, t_name);
    let infer_r = test_infer_param_from_name(&interner, r_name);

    let pattern_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: infer_r,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    let union_return = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let source_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: union_return,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Result should be string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_infer_return_never() {
    // T extends () => infer R ? R : unknown
    // where T = () => never
    // Result should be never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");
    let t_param = test_type_param_from_name(&interner, t_name);
    let infer_r = test_infer_param_from_name(&interner, r_name);

    let pattern_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: infer_r,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: pattern_fn,
        true_type: infer_r,
        false_type: TypeId::UNKNOWN,
        is_distributive: false,
    };

    let cond_type = interner.conditional(cond);

    let source_fn = interner.function(FunctionShape {
        params: Vec::new(),
        return_type: TypeId::NEVER,
        type_predicate: None,
        this_type: None,
        type_params: Vec::new(),
        is_constructor: false,
        is_method: false,
    });

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, source_fn);

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    assert_eq!(result, TypeId::NEVER);
}

// =============================================================================
// CONDITIONAL TYPE DISTRIBUTION STRESS TESTS
// =============================================================================

#[test]
fn test_distribution_over_large_union() {
    // T extends string ? "yes" : "no" where T = "a" | "b" | "c" | "d" | "e"
    // Distributes to: ("a" extends string ? "yes" : "no") | ... | ("e" extends string ? "yes" : "no")
    // = "yes" | "yes" | "yes" | "yes" | "yes" = "yes"
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let lit_d = interner.literal_string("d");
    let lit_e = interner.literal_string("e");
    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let large_union = interner.union(vec![lit_a, lit_b, lit_c, lit_d, lit_e]);

    let cond = ConditionalType {
        check_type: large_union,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // All literals extend string, so result should be "yes"
    assert_eq!(result, lit_yes);
}

#[test]
fn test_distribution_over_mixed_union() {
    // T extends string ? T : never where T = string | number | "literal"
    // Distributes: (string extends string ? string : never) | (number extends string ? number : never) | ("literal" extends string ? "literal" : never)
    // = string | never | "literal" = string (since "literal" <: string)
    let interner = TypeInterner::new();

    let lit_val = interner.literal_string("literal");
    let mixed_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, lit_val]);

    let cond = ConditionalType {
        check_type: mixed_union,
        extends_type: TypeId::STRING,
        true_type: mixed_union, // T in true branch
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Result should be string | "literal" = string (or union containing string parts)
    assert!(result != TypeId::ERROR);
    assert!(result != TypeId::NEVER);
}

#[test]
fn test_distribution_over_union_all_false() {
    // T extends string ? "yes" : "no" where T = number | boolean | symbol
    // Distributes: (number extends string ? "yes" : "no") | (boolean extends string ? "yes" : "no") | (symbol extends string ? "yes" : "no")
    // = "no" | "no" | "no" = "no"
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let non_string_union = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN, TypeId::SYMBOL]);

    let cond = ConditionalType {
        check_type: non_string_union,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // All members don't extend string, so result should be "no"
    assert_eq!(result, lit_no);
}

#[test]
fn test_distribution_with_never_check_type() {
    // never extends T ? "yes" : "no"
    // never distributes to empty union, result is never
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: TypeId::NEVER,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // never distributes to empty union = never
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_distribution_with_any_check_type() {
    // any extends string ? "yes" : "no"
    // any distributes specially, result is "yes" | "no"
    let interner = TypeInterner::new();

    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: TypeId::ANY,
        extends_type: TypeId::STRING,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);
    // any distributes to both branches
    let expected = interner.union(vec![lit_yes, lit_no]);
    assert!(result == expected || result == lit_yes || result == lit_no);
}

#[test]
fn test_distribution_nested_conditional() {
    // T extends string ? (T extends "a" ? 1 : 2) : 3
    // where T = "a" | "b" | number
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);

    let check_union = interner.union(vec![lit_a, lit_b, TypeId::NUMBER]);

    // Inner conditional for true branch
    let inner_cond = ConditionalType {
        check_type: check_union,
        extends_type: lit_a,
        true_type: lit_1,
        false_type: lit_2,
        is_distributive: true,
    };
    let inner_result = interner.conditional(inner_cond);

    let outer_cond = ConditionalType {
        check_type: check_union,
        extends_type: TypeId::STRING,
        true_type: inner_result,
        false_type: lit_3,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &outer_cond);
    // "a" -> string -> inner: "a" extends "a" -> 1
    // "b" -> string -> inner: "b" extends "a" -> 2
    // number -> not string -> 3
    // Result: 1 | 2 | 3
    assert!(result != TypeId::ERROR);
}
