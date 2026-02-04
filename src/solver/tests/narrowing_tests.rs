use super::*;

// =============================================================================
// Discriminant Detection Tests
// =============================================================================

#[test]
fn test_find_discriminants_basic() {
    let interner = TypeInterner::new();
    let type_name = interner.intern_string("type");

    // type Action = { type: "add" } | { type: "remove" }
    let type_add = interner.literal_string("add");
    let type_remove = interner.literal_string("remove");

    let member1 = interner.object(vec![PropertyInfo {
        name: type_name,
        type_id: type_add,
        write_type: type_add,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let member2 = interner.object(vec![PropertyInfo {
        name: type_name,
        type_id: type_remove,
        write_type: type_remove,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let union = interner.union(vec![member1, member2]);

    let discriminants = find_discriminants(&interner, union);

    assert_eq!(discriminants.len(), 1);
    assert_eq!(discriminants[0].property_name, type_name);
    assert_eq!(discriminants[0].variants.len(), 2);
}

#[test]
fn test_find_discriminants_multiple_props() {
    let interner = TypeInterner::new();
    let kind_name = interner.intern_string("kind");
    let type_name = interner.intern_string("type");

    // type Action = { kind: "a", type: 1 } | { kind: "b", type: 2 }
    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");
    let type_1 = interner.literal_number(1.0);
    let type_2 = interner.literal_number(2.0);

    let member1 = interner.object(vec![
        PropertyInfo {
            name: kind_name,
            type_id: kind_a,
            write_type: kind_a,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: type_name,
            type_id: type_1,
            write_type: type_1,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);
    let member2 = interner.object(vec![
        PropertyInfo {
            name: kind_name,
            type_id: kind_b,
            write_type: kind_b,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: type_name,
            type_id: type_2,
            write_type: type_2,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let union = interner.union(vec![member1, member2]);

    let discriminants = find_discriminants(&interner, union);

    // Both "kind" and "type" are discriminants
    assert_eq!(discriminants.len(), 2);
}

#[test]
fn test_find_discriminants_non_literal() {
    let interner = TypeInterner::new();
    let type_name = interner.intern_string("type");

    // type T = { type: string } | { type: string }
    // Not a discriminated union - type is not literal
    let member1 = interner.object(vec![PropertyInfo {
        name: type_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let member2 = interner.object(vec![PropertyInfo {
        name: type_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let union = interner.union(vec![member1, member2]);

    let discriminants = find_discriminants(&interner, union);

    // No discriminants - not literal types
    assert_eq!(discriminants.len(), 0);
}

#[test]
fn test_find_discriminants_missing_property() {
    let interner = TypeInterner::new();
    let type_name = interner.intern_string("type");
    let kind_name = interner.intern_string("kind");

    // type T = { type: "a" } | { kind: "b" }
    // Not a discriminated union - no common property
    let type_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");

    let member1 = interner.object(vec![PropertyInfo {
        name: type_name,
        type_id: type_a,
        write_type: type_a,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let member2 = interner.object(vec![PropertyInfo {
        name: kind_name,
        type_id: kind_b,
        write_type: kind_b,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let union = interner.union(vec![member1, member2]);

    let discriminants = find_discriminants(&interner, union);

    assert_eq!(discriminants.len(), 0);
}

// =============================================================================
// Nullish Helper Tests
// =============================================================================

#[test]
fn test_nullish_helpers_basic() {
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);

    assert!(is_nullish_type(&interner, TypeId::NULL));
    assert!(is_nullish_type(&interner, TypeId::UNDEFINED));
    assert!(!is_nullish_type(&interner, TypeId::STRING));
    assert!(can_be_nullish(&interner, union));
    assert!(type_contains_undefined(&interner, union));

    let removed = remove_nullish(&interner, union);
    assert_eq!(removed, TypeId::STRING);
}

#[test]
fn test_split_nullish_type() {
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);

    let (non_null, cause) = split_nullish_type(&interner, union);
    assert_eq!(non_null, Some(TypeId::STRING));
    let cause = cause.expect("expected nullish cause");
    assert!(is_nullish_type(&interner, cause));
    assert!(type_contains_undefined(&interner, cause));
}

#[test]
fn test_definitely_nullish_union() {
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);
    assert!(is_definitely_nullish(&interner, union));
}

// =============================================================================
// Narrowing by Discriminant Tests
// =============================================================================

#[test]
fn test_narrow_by_discriminant() {
    let interner = TypeInterner::new();
    let type_name = interner.intern_string("type");

    // type Action = { type: "add", value: number } | { type: "remove", id: string }
    let type_add = interner.literal_string("add");
    let type_remove = interner.literal_string("remove");

    let member_add = interner.object(vec![
        PropertyInfo {
            name: type_name,
            type_id: type_add,
            write_type: type_add,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);
    let member_remove = interner.object(vec![
        PropertyInfo {
            name: type_name,
            type_id: type_remove,
            write_type: type_remove,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("id"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let union = interner.union(vec![member_add, member_remove]);

    // Narrow to "add" variant
    let narrowed = narrow_by_discriminant(&interner, union, type_name, type_add);
    assert_eq!(narrowed, member_add);

    // Narrow to "remove" variant
    let narrowed = narrow_by_discriminant(&interner, union, type_name, type_remove);
    assert_eq!(narrowed, member_remove);
}

#[test]
fn test_narrow_by_discriminant_no_match() {
    let interner = TypeInterner::new();
    let type_name = interner.intern_string("type");

    let type_add = interner.literal_string("add");
    let type_unknown = interner.literal_string("unknown");

    let member = interner.object(vec![PropertyInfo {
        name: type_name,
        type_id: type_add,
        write_type: type_add,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let union = interner.union(vec![member]);

    // Narrow to non-existent variant - returns original
    let narrowed = narrow_by_discriminant(&interner, union, type_name, type_unknown);
    assert_eq!(narrowed, union);
}

#[test]
fn test_narrow_excluding_discriminant() {
    let interner = TypeInterner::new();
    let type_name = interner.intern_string("type");

    // type Action = { type: "a" } | { type: "b" } | { type: "c" }
    let type_a = interner.literal_string("a");
    let type_b = interner.literal_string("b");
    let type_c = interner.literal_string("c");

    let member_a = interner.object(vec![PropertyInfo {
        name: type_name,
        type_id: type_a,
        write_type: type_a,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let member_b = interner.object(vec![PropertyInfo {
        name: type_name,
        type_id: type_b,
        write_type: type_b,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let member_c = interner.object(vec![PropertyInfo {
        name: type_name,
        type_id: type_c,
        write_type: type_c,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let union = interner.union(vec![member_a, member_b, member_c]);

    let ctx = NarrowingContext::new(&interner);

    // Exclude "a" - should get "b" | "c"
    let narrowed = ctx.narrow_by_excluding_discriminant(union, type_name, type_a);
    let expected = interner.union(vec![member_b, member_c]);
    assert_eq!(narrowed, expected);
}

// =============================================================================
// Typeof Narrowing Tests
// =============================================================================

#[test]
fn test_narrow_by_typeof_string() {
    let interner = TypeInterner::new();

    // string | number narrowed by typeof "string" -> string
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrowed = narrow_by_typeof(&interner, union, "string");
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn test_narrow_by_typeof_number() {
    let interner = TypeInterner::new();

    // string | number narrowed by typeof "number" -> number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrowed = narrow_by_typeof(&interner, union, "number");
    assert_eq!(narrowed, TypeId::NUMBER);
}

#[test]
fn test_narrow_by_typeof_no_match() {
    let interner = TypeInterner::new();

    // string narrowed by typeof "number" -> never
    let narrowed = narrow_by_typeof(&interner, TypeId::STRING, "number");
    assert_eq!(narrowed, TypeId::NEVER);
}

#[test]
fn test_narrow_by_typeof_literal() {
    let interner = TypeInterner::new();

    // "hello" | 42 narrowed by typeof "string" -> "hello"
    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);
    let union = interner.union(vec![hello, forty_two]);

    let narrowed = narrow_by_typeof(&interner, union, "string");
    assert_eq!(narrowed, hello);
}

#[test]
fn test_narrow_by_typeof_template_literal() {
    let interner = TypeInterner::new();

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);
    let union = interner.union(vec![template, TypeId::NUMBER]);

    let narrowed = narrow_by_typeof(&interner, union, "string");
    assert_eq!(narrowed, template);
}

#[test]
fn test_narrow_by_typeof_any() {
    let interner = TypeInterner::new();

    let narrowed = narrow_by_typeof(&interner, TypeId::ANY, "string");
    assert_eq!(narrowed, TypeId::ANY);
}

#[test]
fn test_narrow_by_typeof_unknown_string() {
    let interner = TypeInterner::new();

    let narrowed = narrow_by_typeof(&interner, TypeId::UNKNOWN, "string");
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn test_narrow_by_typeof_unknown_object() {
    let interner = TypeInterner::new();

    let narrowed = narrow_by_typeof(&interner, TypeId::UNKNOWN, "object");
    let expected = interner.union(vec![TypeId::OBJECT, TypeId::NULL]);
    assert_eq!(narrowed, expected);
}

#[test]
fn test_narrow_by_typeof_unknown_function() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let narrowed = narrow_by_typeof(&interner, TypeId::UNKNOWN, "function");
    assert_eq!(narrowed, ctx.function_type());
}

#[test]
fn test_narrow_by_typeof_object_function() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let narrowed = narrow_by_typeof(&interner, TypeId::OBJECT, "function");
    assert_eq!(narrowed, ctx.function_type());
}

#[test]
fn test_narrow_by_typeof_empty_object_function() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let empty_object = interner.object(vec![]);
    let narrowed = narrow_by_typeof(&interner, empty_object, "function");
    assert_eq!(narrowed, ctx.function_type());
}

#[test]
fn test_narrow_by_typeof_negation_function() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let union = interner.union(vec![func, obj]);

    let narrowed = ctx.narrow_excluding_function(union);
    assert_eq!(narrowed, obj);
}

#[test]
fn test_narrow_by_typeof_negation_function_branded_intersection() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let brand = interner.object(vec![PropertyInfo {
        name: interner.intern_string("__brand"),
        type_id: interner.literal_string("Tagged"),
        write_type: interner.literal_string("Tagged"),
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let branded = interner.intersection(vec![func, brand]);
    let union = interner.union(vec![branded, TypeId::NUMBER]);

    let narrowed = ctx.narrow_excluding_function(union);
    assert_eq!(narrowed, TypeId::NUMBER);
}

#[test]
fn test_narrow_by_typeof_negation_function_type_param_with_union_constraint() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let constraint = interner.union(vec![func, TypeId::STRING]);
    let param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));
    let union = interner.union(vec![param, TypeId::BOOLEAN]);

    let narrowed = ctx.narrow_excluding_function(union);
    let expected_param = interner.intersection(vec![param, TypeId::STRING]);
    let expected = interner.union(vec![expected_param, TypeId::BOOLEAN]);
    assert_eq!(narrowed, expected);
}

#[test]
fn test_narrow_by_typeof_negation_function_type_param_to_never() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: Some(func),
        default: None,
        is_const: false,
    }));

    let narrowed = ctx.narrow_excluding_function(param);
    assert_eq!(narrowed, TypeId::NEVER);
}

#[test]
fn test_narrow_by_typeof_type_param_with_union_constraint() {
    let interner = TypeInterner::new();
    let constraint = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));
    let union = interner.union(vec![param, TypeId::BOOLEAN]);

    let narrowed = narrow_by_typeof(&interner, union, "string");
    let expected = interner.intersection(vec![param, TypeId::STRING]);
    assert_eq!(narrowed, expected);
}

#[test]
fn test_narrow_by_typeof_function_type_param_with_union_constraint() {
    let interner = TypeInterner::new();

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let constraint = interner.union(vec![func, TypeId::STRING]);
    let param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));
    let union = interner.union(vec![param, TypeId::BOOLEAN]);

    let narrowed = narrow_by_typeof(&interner, union, "function");
    let expected = interner.intersection(vec![param, func]);
    assert_eq!(narrowed, expected);
}

#[test]
fn test_narrow_by_typeof_function_type_param_with_non_function_constraint() {
    let interner = TypeInterner::new();
    let param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    let narrowed = narrow_by_typeof(&interner, param, "function");
    assert_eq!(narrowed, TypeId::NEVER);
}

#[test]
fn test_narrow_by_typeof_function_unconstrained_type_param() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);
    let param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: None,
        is_const: false,
        default: None,
        
    }));

    let narrowed = narrow_by_typeof(&interner, param, "function");
    let expected = interner.intersection(vec![param, ctx.function_type()]);
    assert_eq!(narrowed, expected);
}

#[test]
fn test_narrow_by_typeof_type_param_with_non_overlapping_constraint() {
    let interner = TypeInterner::new();
    let param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    let narrowed = narrow_by_typeof(&interner, param, "string");
    assert_eq!(narrowed, TypeId::NEVER);
}

#[test]
fn test_narrow_by_typeof_unconstrained_type_param() {
    let interner = TypeInterner::new();
    let param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: None,
        is_const: false,
        default: None,
        
    }));

    let narrowed = narrow_by_typeof(&interner, param, "string");
    let expected = interner.intersection(vec![param, TypeId::STRING]);
    assert_eq!(narrowed, expected);
}

#[test]
fn test_narrow_by_typeof_branded_string_intersection() {
    let interner = TypeInterner::new();

    let brand = interner.object(vec![PropertyInfo {
        name: interner.intern_string("__brand"),
        type_id: interner.literal_string("UserId"),
        write_type: interner.literal_string("UserId"),
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let branded = interner.intersection(vec![TypeId::STRING, brand]);
    let union = interner.union(vec![branded, TypeId::NUMBER]);

    let narrowed = narrow_by_typeof(&interner, union, "string");
    assert_eq!(narrowed, branded);
}

#[test]
fn test_narrow_by_typeof_branded_function_intersection() {
    let interner = TypeInterner::new();

    let brand = interner.object(vec![PropertyInfo {
        name: interner.intern_string("__brand"),
        type_id: interner.literal_string("Tagged"),
        write_type: interner.literal_string("Tagged"),
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let branded = interner.intersection(vec![func, brand]);
    let union = interner.union(vec![branded, TypeId::NUMBER]);

    let narrowed = narrow_by_typeof(&interner, union, "function");
    assert_eq!(narrowed, branded);
}

#[test]
fn test_narrow_by_typeof_object_excludes_branded_function_intersection() {
    let interner = TypeInterner::new();

    let brand = interner.object(vec![PropertyInfo {
        name: interner.intern_string("__brand"),
        type_id: interner.literal_string("Tagged"),
        write_type: interner.literal_string("Tagged"),
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let branded = interner.intersection(vec![func, brand]);
    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let union = interner.union(vec![branded, obj]);

    let narrowed = narrow_by_typeof(&interner, union, "object");
    assert_eq!(narrowed, obj);
}

#[test]
fn test_narrow_by_typeof_object_with_object_literal() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let union = interner.union(vec![obj, TypeId::NUMBER]);

    let narrowed = narrow_by_typeof(&interner, union, "object");
    assert_eq!(narrowed, obj);
}

#[test]
fn test_narrow_by_typeof_object_excludes_function() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let union = interner.union(vec![obj, func]);

    let narrowed = narrow_by_typeof(&interner, union, "object");
    assert_eq!(narrowed, obj);
}

#[test]
fn test_narrow_by_typeof_function_includes_callable() {
    let interner = TypeInterner::new();

    let sig = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_method: false,
    };
    let callable = interner.callable(CallableShape {
        symbol: None,
        call_signatures: vec![sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });
    let union = interner.union(vec![callable, TypeId::NUMBER]);

    let narrowed = narrow_by_typeof(&interner, union, "function");
    assert_eq!(narrowed, callable);
}

// =============================================================================
// General Narrowing Tests
// =============================================================================

#[test]
fn test_narrow_to_type() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | number | boolean narrowed to string -> string
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    let narrowed = ctx.narrow_to_type(union, TypeId::STRING);
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn test_narrow_excluding_type() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | number | boolean excluding string -> number | boolean
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    let narrowed = ctx.narrow_excluding_type(union, TypeId::STRING);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(narrowed, expected);
}

#[test]
fn test_narrow_excluding_type_param_with_union_constraint() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let constraint = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));
    let union = interner.union(vec![param, TypeId::BOOLEAN]);

    let narrowed = ctx.narrow_excluding_type(union, TypeId::STRING);
    let expected_param = interner.intersection(vec![param, TypeId::NUMBER]);
    let expected = interner.union(vec![expected_param, TypeId::BOOLEAN]);
    assert_eq!(narrowed, expected);
}

#[test]
fn test_narrow_excluding_type_param_with_non_overlapping_constraint() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    let narrowed = ctx.narrow_excluding_type(param, TypeId::STRING);
    assert_eq!(narrowed, param);
}

#[test]
fn test_narrow_excluding_type_param_to_never() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    let param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        is_const: false,
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let narrowed = ctx.narrow_excluding_type(param, TypeId::STRING);
    assert_eq!(narrowed, TypeId::NEVER);
}

#[test]
fn test_narrow_to_never() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string excluding string -> never
    let narrowed = ctx.narrow_excluding_type(TypeId::STRING, TypeId::STRING);
    assert_eq!(narrowed, TypeId::NEVER);
}

#[test]
fn test_narrow_single_member_union() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | number excluding string -> number (not a union)
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrowed = ctx.narrow_excluding_type(union, TypeId::STRING);
    assert_eq!(narrowed, TypeId::NUMBER);
}

// =============================================================================
// Type Predicate Structure Tests
// =============================================================================
// These tests verify TypePredicate structures are correctly created.
// Actual narrowing with type predicates happens at the checker level.

#[test]
fn test_type_predicate_basic_structure() {
    use super::TypePredicate;
    use super::TypePredicateTarget;

    let interner = TypeInterner::new();
    let x_name = interner.intern_string("x");

    // x is string
    let predicate = TypePredicate {
        asserts: false,
        target: TypePredicateTarget::Identifier(x_name),
        type_id: Some(TypeId::STRING),
    };

    assert!(!predicate.asserts);
    assert_eq!(predicate.target, TypePredicateTarget::Identifier(x_name));
    assert_eq!(predicate.type_id, Some(TypeId::STRING));
}

#[test]
fn test_type_predicate_asserts_structure() {
    use super::TypePredicate;
    use super::TypePredicateTarget;

    let interner = TypeInterner::new();
    let x_name = interner.intern_string("x");

    // asserts x is string
    let predicate = TypePredicate {
        asserts: true,
        target: TypePredicateTarget::Identifier(x_name),
        type_id: Some(TypeId::STRING),
    };

    assert!(predicate.asserts);
    assert_eq!(predicate.target, TypePredicateTarget::Identifier(x_name));
    assert_eq!(predicate.type_id, Some(TypeId::STRING));
}

#[test]
fn test_type_predicate_this_target() {
    use super::TypePredicate;
    use super::TypePredicateTarget;

    let interner = TypeInterner::new();

    // Create an object type for the predicate
    let foo_name = interner.intern_string("foo");
    let foo_type = interner.object(vec![PropertyInfo {
        name: foo_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // this is Foo
    let predicate = TypePredicate {
        asserts: false,
        target: TypePredicateTarget::This,
        type_id: Some(foo_type),
    };

    assert!(!predicate.asserts);
    assert_eq!(predicate.target, TypePredicateTarget::This);
    assert_eq!(predicate.type_id, Some(foo_type));
}

#[test]
fn test_type_predicate_asserts_without_type() {
    use super::TypePredicate;
    use super::TypePredicateTarget;

    let interner = TypeInterner::new();
    let x_name = interner.intern_string("x");

    // asserts x (no type - just assertion that x is truthy)
    let predicate = TypePredicate {
        asserts: true,
        target: TypePredicateTarget::Identifier(x_name),
        type_id: None,
    };

    assert!(predicate.asserts);
    assert_eq!(predicate.target, TypePredicateTarget::Identifier(x_name));
    assert_eq!(predicate.type_id, None);
}

#[test]
fn test_function_shape_with_type_predicate() {
    use super::{FunctionShape, ParamInfo, TypePredicate, TypePredicateTarget};

    let interner = TypeInterner::new();
    let x_name = interner.intern_string("x");

    // function isString(x: any): x is string
    let shape = FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(x_name),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(x_name),
            type_id: Some(TypeId::STRING),
        }),
        is_constructor: false,
        is_method: false,
    };

    assert!(shape.type_predicate.is_some());
    let pred = shape.type_predicate.unwrap();
    assert!(!pred.asserts);
    assert_eq!(pred.type_id, Some(TypeId::STRING));
}

#[test]
fn test_call_signature_with_type_predicate() {
    use super::{CallSignature, ParamInfo, TypePredicate, TypePredicateTarget};

    let interner = TypeInterner::new();
    let x_name = interner.intern_string("x");

    // Overload: (x: any): x is number
    let sig = CallSignature {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(x_name),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(x_name),
            type_id: Some(TypeId::NUMBER),
        }),
        is_method: false,
    };

    assert!(sig.type_predicate.is_some());
    let pred = sig.type_predicate.unwrap();
    assert_eq!(pred.type_id, Some(TypeId::NUMBER));
}

#[test]
fn test_narrow_to_type_simulates_type_predicate_narrowing() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // Simulating what happens after a type predicate check:
    // if (isString(x)) { /* x is narrowed to string here */ }

    // Start with x: string | number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // After type predicate `x is string` returns true:
    // Narrow to string (the predicate type)
    let narrowed = ctx.narrow_to_type(union, TypeId::STRING);

    // Should be narrowed to string
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn test_narrow_excluding_type_simulates_type_predicate_false_branch() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // Simulating the else branch after a type predicate check:
    // if (isString(x)) { ... } else { /* x is NOT string here */ }

    // Start with x: string | number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // After type predicate `x is string` returns false:
    // Narrow by excluding string
    let narrowed = ctx.narrow_excluding_type(union, TypeId::STRING);

    // Should be narrowed to number
    assert_eq!(narrowed, TypeId::NUMBER);
}

#[test]
fn test_narrow_to_interface_type() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // Simulating interface narrowing:
    // interface Cat { meow(): void }
    // interface Dog { bark(): void }
    // function isCat(x: Cat | Dog): x is Cat

    let meow_name = interner.intern_string("meow");
    let bark_name = interner.intern_string("bark");

    let cat_type = interner.object(vec![PropertyInfo {
        name: meow_name,
        type_id: TypeId::VOID,
        write_type: TypeId::VOID,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let dog_type = interner.object(vec![PropertyInfo {
        name: bark_name,
        type_id: TypeId::VOID,
        write_type: TypeId::VOID,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let union = interner.union(vec![cat_type, dog_type]);

    // After type predicate `x is Cat` returns true:
    let narrowed = ctx.narrow_to_type(union, cat_type);

    // Should be narrowed to Cat
    assert_eq!(narrowed, cat_type);
}

// =============================================================================
// TypeGuard and narrow_type() Tests
// =============================================================================

use crate::solver::narrowing::{NarrowingContext, TypeGuard};

#[test]
fn test_type_guard_typeof_string() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // typeof x === "string"
    let guard = TypeGuard::Typeof("string".to_string());
    let narrowed = ctx.narrow_type(union, &guard, true);

    // Should narrow to string
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn test_type_guard_typeof_string_negated() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // typeof x !== "string" (sense=false)
    let guard = TypeGuard::Typeof("string".to_string());
    let narrowed = ctx.narrow_type(union, &guard, false);

    // Should narrow to number (exclude string)
    assert_eq!(narrowed, TypeId::NUMBER);
}

#[test]
fn test_type_guard_literal_equality() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // "foo" | "bar"
    let foo = interner.literal_string("foo");
    let bar = interner.literal_string("bar");
    let union = interner.union(vec![foo, bar]);

    // x === "foo"
    let guard = TypeGuard::LiteralEquality(foo);
    let narrowed = ctx.narrow_type(union, &guard, true);

    // Should narrow to "foo"
    assert_eq!(narrowed, foo);
}

#[test]
fn test_type_guard_literal_equality_negated() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // "foo" | "bar"
    let foo = interner.literal_string("foo");
    let bar = interner.literal_string("bar");
    let union = interner.union(vec![foo, bar]);

    // x !== "foo" (sense=false)
    let guard = TypeGuard::LiteralEquality(foo);
    let narrowed = ctx.narrow_type(union, &guard, false);

    // Should narrow to "bar" (exclude "foo")
    assert_eq!(narrowed, bar);
}

#[test]
fn test_type_guard_nullish_equality() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | null
    let union = interner.union(vec![TypeId::STRING, TypeId::NULL]);

    // x == null
    let guard = TypeGuard::NullishEquality;
    let narrowed = ctx.narrow_type(union, &guard, true);

    // Should narrow to null | undefined
    let nullish = interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);
    assert_eq!(narrowed, nullish);
}

#[test]
fn test_type_guard_nullish_equality_negated() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | null
    let union = interner.union(vec![TypeId::STRING, TypeId::NULL]);

    // x != null (sense=false)
    let guard = TypeGuard::NullishEquality;
    let narrowed = ctx.narrow_type(union, &guard, false);

    // Should narrow to string (exclude null and undefined)
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn test_type_guard_discriminant() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);
    let kind_name = interner.intern_string("kind");

    // { kind: "a" } | { kind: "b" }
    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");

    let member1 = interner.object(vec![PropertyInfo {
        name: kind_name,
        type_id: kind_a,
        write_type: kind_a,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let member2 = interner.object(vec![PropertyInfo {
        name: kind_name,
        type_id: kind_b,
        write_type: kind_b,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let union = interner.union(vec![member1, member2]);

    // x.kind === "a"
    let guard = TypeGuard::Discriminant {
        property_name: kind_name,
        value_type: kind_a,
    };
    let narrowed = ctx.narrow_type(union, &guard, true);

    // Should narrow to { kind: "a" }
    assert_eq!(narrowed, member1);
}

#[test]
fn test_type_guard_discriminant_negated() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);
    let kind_name = interner.intern_string("kind");

    // { kind: "a" } | { kind: "b" }
    let kind_a = interner.literal_string("a");
    let kind_b = interner.literal_string("b");

    let member1 = interner.object(vec![PropertyInfo {
        name: kind_name,
        type_id: kind_a,
        write_type: kind_a,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let member2 = interner.object(vec![PropertyInfo {
        name: kind_name,
        type_id: kind_b,
        write_type: kind_b,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let union = interner.union(vec![member1, member2]);

    // x.kind !== "a" (sense=false)
    let guard = TypeGuard::Discriminant {
        property_name: kind_name,
        value_type: kind_a,
    };
    let narrowed = ctx.narrow_type(union, &guard, false);

    // Should narrow to { kind: "b" }
    assert_eq!(narrowed, member2);
}

#[test]
fn test_type_guard_truthy() {
    let interner = TypeInterner::new();
    let ctx = NarrowingContext::new(&interner);

    // string | null
    let union = interner.union(vec![TypeId::STRING, TypeId::NULL]);

    // if (x) { ... }  (truthy check)
    let guard = TypeGuard::Truthy;
    let narrowed = ctx.narrow_type(union, &guard, true);

    // Should narrow to string (exclude null and undefined)
    assert_eq!(narrowed, TypeId::STRING);
}
