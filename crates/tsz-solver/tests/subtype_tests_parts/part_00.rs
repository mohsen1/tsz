#[test]
fn test_intrinsic_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Same type
    assert!(checker.is_subtype_of(TypeId::STRING, TypeId::STRING));
    assert!(checker.is_subtype_of(TypeId::NUMBER, TypeId::NUMBER));

    // Different intrinsics
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::NUMBER));

    // Any relations
    assert!(checker.is_subtype_of(TypeId::ANY, TypeId::STRING));
    assert!(checker.is_subtype_of(TypeId::STRING, TypeId::ANY));

    // Unknown relations
    assert!(checker.is_subtype_of(TypeId::STRING, TypeId::UNKNOWN));
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, TypeId::STRING));

    // Never relations
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::STRING));
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::NEVER));
}

#[test]
fn subtype_checker_cache_statistics_account_for_eval_entries() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty = checker.cache_statistics();
    assert_eq!(empty.eval_entries, 0);
    assert_eq!(empty.estimated_size_bytes(), 0);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(checker.evaluate_type(union), union);
    let populated = checker.cache_statistics();
    assert_eq!(populated.eval_entries, 1);
    assert!(
        populated.estimated_size_bytes() > empty.estimated_size_bytes(),
        "populated subtype eval cache should report nonzero estimated residency"
    );

    assert_eq!(checker.evaluate_type(union), union);
    let repeated = checker.cache_statistics();
    assert_eq!(repeated.eval_entries, populated.eval_entries);
    assert_eq!(
        repeated.estimated_size_bytes(),
        populated.estimated_size_bytes()
    );

    checker.reset();
    assert_eq!(checker.cache_statistics().eval_entries, 0);
}

#[test]
fn test_any_top_bottom_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // tsc rule: `if (s & TypeFlags.Any) return !(t & TypeFlags.Never)`
    // any is NOT assignable to never, even in tsc's own assignability check.
    assert!(!checker.is_subtype_of(TypeId::ANY, TypeId::NEVER));
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::ANY));
}

#[test]
fn test_generic_remapped_mapped_type_does_not_expand_to_source_keys() {
    let interner = TypeInterner::new();
    let atom_a = interner.intern_string("a");
    let atom_b = interner.intern_string("b");

    let model = interner.object(vec![
        PropertyInfo {
            name: atom_a,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
        PropertyInfo {
            name: atom_b,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 1,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(interner.keyof(model)),
        default: None,
        is_const: false,
    };
    let key_type = interner.type_param(key_param);
    let suffix_param = TypeParamInfo {
        name: interner.intern_string("Suffix"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let suffix_type = interner.type_param(suffix_param);
    let remapped_name = interner.template_literal(vec![
        crate::types::TemplateSpan::Type(key_type),
        crate::types::TemplateSpan::Type(suffix_type),
    ]);
    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: interner.keyof(model),
        name_type: Some(remapped_name),
        template: interner.index_access(model, key_type),
        readonly_modifier: None,
        optional_modifier: None,
    });

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(model, mapped),
        "generic remapped mapped types must not expand to the original source keys"
    );
}

#[test]
fn test_legacy_null_undefined_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    checker.strict_null_checks = false;

    assert!(checker.is_subtype_of(TypeId::NULL, TypeId::STRING));
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::STRING));
}

#[test]
fn test_error_type_permissive_subtyping() {
    // ERROR types are assignable to/from everything (like `any` in tsc).
    // This prevents cascading diagnostics when type resolution fails.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // ERROR is a subtype of concrete types (like `any`)
    assert!(checker.is_subtype_of(TypeId::ERROR, TypeId::STRING));
    // Concrete types are subtypes of ERROR (like `any`)
    assert!(checker.is_subtype_of(TypeId::STRING, TypeId::ERROR));
    // ERROR is a subtype of itself (reflexive)
    assert!(checker.is_subtype_of(TypeId::ERROR, TypeId::ERROR));
}

#[test]
fn test_error_type_acts_like_any() {
    // ERROR acts like `any` — assignable to/from all types.
    // This matches tsc behavior where errorType silences cascading errors.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    // ERROR is a subtype of object types (like `any`)
    assert!(checker.is_subtype_of(TypeId::ERROR, TypeId::OBJECT));
    // Tuples are subtypes of ERROR (like `any`)
    assert!(checker.is_subtype_of(tuple, TypeId::ERROR));
}

#[test]
fn test_literal_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // Literal to same literal
    assert!(checker.is_subtype_of(hello, hello));

    // Literal to different literal
    assert!(!checker.is_subtype_of(hello, world));

    // Literal to intrinsic
    assert!(checker.is_subtype_of(hello, TypeId::STRING));
    assert!(!checker.is_subtype_of(hello, TypeId::NUMBER));
}

#[test]
fn test_synthetic_promise_base_is_covariant_in_inner_type() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one_name = interner.intern_string("one");
    let two_name = interner.intern_string("two");

    let source_tuple = interner.tuple(vec![
        TupleElement {
            type_id: interner.literal_number(1.0),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.literal_string("two"),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let target_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(one_name),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(two_name),
            optional: false,
            rest: false,
        },
    ]);

    let source_promise = interner.application(TypeId::PROMISE_BASE, vec![source_tuple]);
    let target_promise = interner.application(TypeId::PROMISE_BASE, vec![target_tuple]);

    assert!(checker.is_subtype_of(source_promise, target_promise));
    assert!(!checker.is_subtype_of(target_promise, source_promise));
}

#[test]
fn test_template_literal_subtyping_to_string() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let red = interner.literal_string("red");
    let blue = interner.literal_string("blue");
    let colors = interner.union(vec![red, blue]);
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("color-")),
        TemplateSpan::Type(colors),
    ]);

    assert!(checker.is_subtype_of(template, TypeId::STRING));
    assert!(!checker.is_subtype_of(TypeId::STRING, template));
}

#[test]
fn test_template_literal_apparent_member_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let red = interner.literal_string("red");
    let blue = interner.literal_string("blue");
    let colors = interner.union(vec![red, blue]);
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("color-")),
        TemplateSpan::Type(colors),
    ]);

    let method = |return_type| {
        interner.function(FunctionShape {
            params: Vec::new(),
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let to_upper = interner.intern_string("toUpperCase");
    let target = interner.object(vec![PropertyInfo::method(to_upper, method(TypeId::STRING))]);
    let mismatch = interner.object(vec![PropertyInfo::method(to_upper, method(TypeId::NUMBER))]);

    assert!(checker.is_subtype_of(template, target));
    assert!(!checker.is_subtype_of(template, mismatch));
}

#[test]
fn test_template_literal_number_index_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let red = interner.literal_string("red");
    let blue = interner.literal_string("blue");
    let colors = interner.union(vec![red, blue]);
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("color-")),
        TemplateSpan::Type(colors),
    ]);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });
    let mismatch = interner.object_with_index(ObjectShape {
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

    assert!(checker.is_subtype_of(template, target));
    assert!(!checker.is_subtype_of(template, mismatch));
}

#[test]
fn test_apparent_number_member_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_any = interner.array(TypeId::ANY);
    let method = |return_type| {
        interner.function(FunctionShape {
            params: vec![ParamInfo {
                name: None,
                type_id: rest_any,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let to_fixed = interner.intern_string("toFixed");
    let target = interner.object(vec![PropertyInfo::method(to_fixed, method(TypeId::STRING))]);

    let mismatch = interner.object(vec![PropertyInfo::method(to_fixed, method(TypeId::NUMBER))]);

    assert!(checker.is_subtype_of(TypeId::NUMBER, target));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, mismatch));
}

#[test]
fn test_apparent_string_member_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method = |return_type| {
        interner.function(FunctionShape {
            params: Vec::new(),
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let to_upper = interner.intern_string("toUpperCase");
    let target = interner.object(vec![PropertyInfo::method(to_upper, method(TypeId::STRING))]);
    let mismatch = interner.object(vec![PropertyInfo::method(to_upper, method(TypeId::NUMBER))]);

    assert!(checker.is_subtype_of(TypeId::STRING, target));
    assert!(!checker.is_subtype_of(TypeId::STRING, mismatch));
}

#[test]
fn test_generic_function_mapped_apparent_constraint_not_erased_by_alpha_rename() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    });

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    let t_key = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(interner.keyof(t_type)),
        default: None,
        is_const: false,
    };
    let t_key_type = interner.type_param(t_key);
    let foo_param_type = interner.mapped(MappedType {
        type_param: t_key,
        constraint: interner.keyof(t_type),
        name_type: None,
        template: interner.index_access(t_type, t_key_type),
        readonly_modifier: None,
        optional_modifier: None,
    });
    let foo = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("target")),
            type_id: foo_param_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![t_param],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(interner.array(TypeId::STRING)),
        default: None,
        is_const: false,
    };
    let u_type = interner.type_param(u_param);
    let u_key = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(interner.keyof(u_type)),
        default: None,
        is_const: false,
    };
    let u_key_type = interner.type_param(u_key);
    let bar_param_type = interner.mapped(MappedType {
        type_param: u_key,
        constraint: interner.keyof(u_type),
        name_type: None,
        template: interner.index_access(obj, u_key_type),
        readonly_modifier: None,
        optional_modifier: None,
    });
    let bar = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("source")),
            type_id: bar_param_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![u_param],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(
        !checker.is_subtype_of(foo, bar),
        "target constraint must remain visible so mapped apparent members stay incompatible"
    );
}

#[test]
fn test_apparent_string_length_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let length = interner.intern_string("length");
    let target = interner.object(vec![PropertyInfo::new(length, TypeId::NUMBER)]);
    let mismatch = interner.object(vec![PropertyInfo::new(length, TypeId::STRING)]);

    assert!(checker.is_subtype_of(TypeId::STRING, target));
    assert!(!checker.is_subtype_of(TypeId::STRING, mismatch));
}

#[test]
fn test_apparent_string_number_index_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });
    let mismatch = interner.object_with_index(ObjectShape {
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

    assert!(checker.is_subtype_of(TypeId::STRING, target));
    assert!(!checker.is_subtype_of(TypeId::STRING, mismatch));
}

#[test]
fn test_apparent_boolean_member_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method = |return_type| {
        interner.function(FunctionShape {
            params: Vec::new(),
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let value_of = interner.intern_string("valueOf");
    let target = interner.object(vec![PropertyInfo::method(
        value_of,
        method(TypeId::BOOLEAN),
    )]);
    let mismatch = interner.object(vec![PropertyInfo::method(value_of, method(TypeId::NUMBER))]);

    assert!(checker.is_subtype_of(TypeId::BOOLEAN, target));
    assert!(!checker.is_subtype_of(TypeId::BOOLEAN, mismatch));
}

#[test]
fn test_apparent_symbol_member_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let description = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    let name = interner.intern_string("description");

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: description,
        write_type: description,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    assert!(checker.is_subtype_of(TypeId::SYMBOL, target));
    assert!(!checker.is_subtype_of(TypeId::SYMBOL, mismatch));
}

#[test]
fn test_apparent_bigint_member_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method = |return_type| {
        interner.function(FunctionShape {
            params: Vec::new(),
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let value_of = interner.intern_string("valueOf");
    let target = interner.object(vec![PropertyInfo::method(value_of, method(TypeId::BIGINT))]);
    let mismatch = interner.object(vec![PropertyInfo::method(value_of, method(TypeId::NUMBER))]);

    assert!(checker.is_subtype_of(TypeId::BIGINT, target));
    assert!(!checker.is_subtype_of(TypeId::BIGINT, mismatch));
}

#[test]
fn test_apparent_object_member_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method = |return_type| {
        interner.function(FunctionShape {
            params: Vec::new(),
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let has_own = interner.intern_string("hasOwnProperty");
    let target = interner.object(vec![PropertyInfo::method(has_own, method(TypeId::BOOLEAN))]);
    let mismatch = interner.object(vec![PropertyInfo::method(has_own, method(TypeId::STRING))]);

    assert!(checker.is_subtype_of(TypeId::NUMBER, target));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, mismatch));
}

#[test]
fn test_object_trifecta_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let array = interner.array(TypeId::STRING);
    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::BOOLEAN,
        name: None,
        optional: false,
        rest: false,
    }]);
    let func = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let empty_object = interner.object(Vec::new());

    assert!(checker.is_subtype_of(obj, TypeId::OBJECT));
    assert!(checker.is_subtype_of(array, TypeId::OBJECT));
    assert!(checker.is_subtype_of(tuple, TypeId::OBJECT));
    assert!(checker.is_subtype_of(func, TypeId::OBJECT));
    assert!(checker.is_subtype_of(TypeId::STRING, empty_object));
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, TypeId::OBJECT));
}

#[test]
fn test_object_trifecta_object_interface_accepts_primitives() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let to_string = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let object_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toString"),
        to_string,
    )]);

    let def_id = DefId(1);
    env.insert_def(def_id, object_interface);
    let object_ref = interner.lazy(def_id);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    let empty_object = interner.object(Vec::new());

    assert!(checker.is_subtype_of(TypeId::STRING, object_ref));
    assert!(checker.is_subtype_of(TypeId::NUMBER, object_ref));
    assert!(checker.is_subtype_of(TypeId::STRING, empty_object));
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::OBJECT));
}

#[test]
fn test_object_trifecta_nullish_rejection() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let to_string = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let object_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toString"),
        to_string,
    )]);
    let def_id = DefId(99);
    env.insert_def(def_id, object_interface);
    let object_ref = interner.lazy(def_id);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    let empty_object = interner.object(Vec::new());

    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::NULL, empty_object));
    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, empty_object));
    assert!(!checker.is_subtype_of(TypeId::NULL, object_ref));
    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, object_ref));
}

#[test]
fn test_primitive_boxing_assignability() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let to_fixed = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toFixed"),
        to_fixed,
    )]);

    let def_id = DefId(2);
    env.insert_def(def_id, number_interface);
    let number_ref = interner.lazy(def_id);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(checker.is_subtype_of(TypeId::NUMBER, number_ref));
    assert!(!checker.is_subtype_of(number_ref, TypeId::NUMBER));
}

#[test]
fn test_primitive_boxing_bigint_assignability() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let to_string = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let bigint_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toString"),
        to_string,
    )]);

    let def_id = DefId(3);
    env.insert_def(def_id, bigint_interface);
    let bigint_ref = interner.lazy(def_id);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(checker.is_subtype_of(TypeId::BIGINT, bigint_ref));
    assert!(!checker.is_subtype_of(bigint_ref, TypeId::BIGINT));
}

#[test]
fn test_primitive_boxing_boolean_assignability() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let to_string = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let boolean_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toString"),
        to_string,
    )]);

    let def_id = DefId(4);
    env.insert_def(def_id, boolean_interface);
    let boolean_ref = interner.lazy(def_id);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(checker.is_subtype_of(TypeId::BOOLEAN, boolean_ref));
    assert!(!checker.is_subtype_of(boolean_ref, TypeId::BOOLEAN));
}

#[test]
fn test_primitive_boxing_string_assignability() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let to_upper = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let string_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toUpperCase"),
        to_upper,
    )]);

    let def_id = DefId(5);
    env.insert_def(def_id, string_interface);
    let string_ref = interner.lazy(def_id);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(checker.is_subtype_of(TypeId::STRING, string_ref));
    assert!(!checker.is_subtype_of(string_ref, TypeId::STRING));
}

#[test]
fn test_primitive_boxing_symbol_assignability() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let description = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    let symbol_interface = interner.object(vec![PropertyInfo::new(
        interner.intern_string("description"),
        description,
    )]);

    let def_id = DefId(6);
    env.insert_def(def_id, symbol_interface);
    let symbol_ref = interner.lazy(def_id);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(checker.is_subtype_of(TypeId::SYMBOL, symbol_ref));
    assert!(!checker.is_subtype_of(symbol_ref, TypeId::SYMBOL));
}

/// Regression test: primitive → object must be rejected even when boxed wrappers
/// are registered. Previously, `is_target_boxed_type` had a structural fallback
/// that checked `Number_interface <: object` (unidirectional). Since Number IS an
/// object type, this returned true — incorrectly treating `object` as the Number
/// boxed wrapper. The fix requires bidirectional subtyping (structural equivalence).
#[test]
fn test_primitive_not_subtype_of_object_with_boxed_wrappers_registered() {
    let interner = TypeInterner::new();

    // Create Number boxed wrapper interface
    let to_fixed = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toFixed"),
        to_fixed,
    )]);

    // Create String boxed wrapper interface
    let to_upper = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let string_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toUpperCase"),
        to_upper,
    )]);

    // Create Boolean boxed wrapper interface
    let to_string = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let boolean_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toString"),
        to_string,
    )]);

    // Register boxed wrappers on the interner (simulating what the checker does)
    interner.set_boxed_type(IntrinsicKind::Number, number_interface);
    interner.set_boxed_type(IntrinsicKind::String, string_interface);
    interner.set_boxed_type(IntrinsicKind::Boolean, boolean_interface);

    let mut checker = SubtypeChecker::new(&interner);

    // Primitives → object must FAIL (object = non-primitive keyword)
    assert!(!checker.is_subtype_of(TypeId::NUMBER, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::BOOLEAN, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::BIGINT, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::SYMBOL, TypeId::OBJECT));

    // Primitives → their boxed wrapper must SUCCEED
    assert!(checker.is_subtype_of(TypeId::NUMBER, number_interface));
    assert!(checker.is_subtype_of(TypeId::STRING, string_interface));
    assert!(checker.is_subtype_of(TypeId::BOOLEAN, boolean_interface));

    // Object types → object must SUCCEED
    let plain_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(checker.is_subtype_of(plain_obj, TypeId::OBJECT));
    assert!(checker.is_subtype_of(number_interface, TypeId::OBJECT));

    // Nullish → object must FAIL
    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, TypeId::OBJECT));
}

#[test]
fn test_weak_type_detection_requires_overlap() {
    // Note: Weak type checking is now handled by CompatChecker, not SubtypeChecker.
    // SubtypeChecker's enforce_weak_types flag is no longer enforced to avoid
    // double-checking which caused false positives (TS2322).
    // See compat_tests::test_weak_type_rejects_no_common_properties for the
    // authoritative test of weak type behavior.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    // enforce_weak_types is ignored - weak checking is done by CompatChecker

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let weak_target = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);

    let no_overlap = interner.object(vec![PropertyInfo::new(b, TypeId::NUMBER)]);

    let overlap = interner.object(vec![PropertyInfo::new(a, TypeId::NUMBER)]);

    // SubtypeChecker no longer rejects based on weak type rules
    // (that's handled by CompatChecker to avoid double-checking)
    assert!(checker.is_subtype_of(no_overlap, weak_target));
    assert!(checker.is_subtype_of(overlap, weak_target));
}

#[test]
fn test_weak_type_detection_empty_object_allowed() {
    // Empty objects should be assignable to weak types (per TypeScript behavior)
    // Only objects with non-overlapping properties should fail
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    // Note: enforce_weak_types was removed - weak checking is done by CompatChecker

    let a = interner.intern_string("a");

    let weak_target = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);

    let empty_object = interner.object(vec![]);

    // Empty object should be assignable to weak type
    assert!(checker.is_subtype_of(empty_object, weak_target));
}

#[test]
fn test_weak_type_detection_multiple_optional_properties() {
    // Note: Weak type checking is now handled by CompatChecker, not SubtypeChecker.
    // See compat_tests::test_weak_type_all_optional_properties_detection for the
    // authoritative test of this behavior.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    // enforce_weak_types is ignored - weak checking is done by CompatChecker

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");

    let weak_target = interner.object(vec![
        PropertyInfo::opt(a, TypeId::NUMBER),
        PropertyInfo::opt(b, TypeId::STRING),
    ]);

    // SubtypeChecker no longer rejects based on weak type rules
    let no_overlap = interner.object(vec![PropertyInfo::new(c, TypeId::BOOLEAN)]);
    // SubtypeChecker passes this (CompatChecker would reject it)
    assert!(checker.is_subtype_of(no_overlap, weak_target));

    // Partial overlap (shares 'a' property) - should pass
    let partial_overlap = interner.object(vec![PropertyInfo::new(a, TypeId::NUMBER)]);
    assert!(checker.is_subtype_of(partial_overlap, weak_target));
}

#[test]
fn test_weak_type_detection_not_weak_if_has_required() {
    // Types with at least one required property are NOT weak
    // Note: enforce_weak_types was removed - weak checking is done by CompatChecker
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    // Not weak - has a required property
    let not_weak_target = interner.object(vec![
        PropertyInfo::opt(a, TypeId::NUMBER),
        PropertyInfo {
            name: b,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false, // Required!
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
        },
    ]);

    let c = interner.intern_string("c");
    let unrelated_source = interner.object(vec![PropertyInfo::new(c, TypeId::BOOLEAN)]);

    // Should pass because target is NOT weak (has a required property)
    // Even though properties don't overlap, structural typing applies
    assert!(!checker.is_subtype_of(unrelated_source, not_weak_target));
}

#[test]
fn test_split_accessor_variance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let name = interner.intern_string("x");
    let wide_write = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let wide_accessor = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: wide_write,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let narrow_accessor = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    assert!(checker.is_subtype_of(wide_accessor, narrow_accessor));
    assert!(!checker.is_subtype_of(narrow_accessor, wide_accessor));
}

#[test]
fn test_exact_optional_property_types_toggle() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let name = interner.intern_string("x");
    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::UNDEFINED,
        write_type: TypeId::UNDEFINED,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    assert!(checker.is_subtype_of(source, target));

    checker.exact_optional_property_types = true;
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_unique_symbol_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_a = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeData::UniqueSymbol(SymbolRef(2)));

    assert!(checker.is_subtype_of(sym_a, sym_a));
    assert!(!checker.is_subtype_of(sym_a, sym_b));
    assert!(checker.is_subtype_of(sym_a, TypeId::SYMBOL));
    assert!(!checker.is_subtype_of(TypeId::SYMBOL, sym_a));
}

#[test]
fn test_union_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Union member is subtype of union
    assert!(checker.is_subtype_of(TypeId::STRING, string_or_number));
    assert!(checker.is_subtype_of(TypeId::NUMBER, string_or_number));

    // Non-member is not subtype
    assert!(!checker.is_subtype_of(TypeId::BOOLEAN, string_or_number));

    // Union is subtype if all members are subtypes
    let just_string = interner.union(vec![TypeId::STRING]);
    assert!(checker.is_subtype_of(just_string, string_or_number));
}

#[test]
fn test_recursion_depth_limit_provisional_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    fn nest_array(interner: &TypeInterner, base: TypeId, depth: usize) -> TypeId {
        let mut ty = base;
        for _ in 0..depth {
            ty = interner.array(ty);
        }
        ty
    }

    let shallow_string = nest_array(&interner, TypeId::STRING, 10);
    let shallow_number = nest_array(&interner, TypeId::NUMBER, 10);
    assert!(!checker.is_subtype_of(shallow_string, shallow_number));

    let deep_string = nest_array(&interner, TypeId::STRING, 120);
    let deep_number = nest_array(&interner, TypeId::NUMBER, 120);
    // Deep recursion returns DepthExceeded when depth limit is hit.
    // Following tsc's semantics, DepthExceeded is treated as true (Ternary.Maybe).
    // This matches tsc's behavior where recursive depth overflow assumes types are
    // related, preventing false TS2344 errors on circular generic constraints.
    // The depth_exceeded flag is still set for TS2589 diagnostic emission.
    let result = checker.check_subtype(deep_string, deep_number);
    assert!(matches!(result, SubtypeResult::DepthExceeded));
    assert!(checker.guard.is_exceeded());
}

#[test]
fn test_no_unchecked_indexed_access_array_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let index_access = interner.intern(TypeData::IndexAccess(string_array, TypeId::NUMBER));

    assert!(checker.is_subtype_of(index_access, TypeId::STRING));

    checker.no_unchecked_indexed_access = true;
    assert!(!checker.is_subtype_of(index_access, TypeId::STRING));

    let string_or_undefined = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert!(checker.is_subtype_of(index_access, string_or_undefined));
}

#[test]
fn test_no_unchecked_indexed_access_tuple_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple = interner.tuple(vec![
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
    ]);
    let index_access = interner.intern(TypeData::IndexAccess(tuple, TypeId::NUMBER));
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert!(checker.is_subtype_of(index_access, string_or_number));

    checker.no_unchecked_indexed_access = true;
    assert!(!checker.is_subtype_of(index_access, string_or_number));

    let string_number_or_undefined =
        interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]);
    assert!(checker.is_subtype_of(index_access, string_number_or_undefined));
}

#[test]
fn test_index_access_fresh_equivalent_type_parameter_keys_are_related() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_atom = interner.intern_string("Key");
    let key_info = TypeParamInfo {
        name: key_atom,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let source_key = interner.fresh_type_param(key_info);
    let target_key = interner.fresh_type_param(key_info);
    let object = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let source = interner.index_access(object, source_key);
    let target = interner.index_access(object, target_key);

    assert!(
        checker.is_subtype_of(source, target),
        "fresh ids for the same declaration-scoped key should compare through the operand relation"
    );
}

#[test]
fn test_no_unchecked_object_index_signature_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let index_access = interner.intern(TypeData::IndexAccess(indexed, TypeId::NUMBER));

    assert!(checker.is_subtype_of(index_access, TypeId::NUMBER));

    checker.no_unchecked_indexed_access = true;

    assert!(!checker.is_subtype_of(index_access, TypeId::NUMBER));
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert!(checker.is_subtype_of(index_access, number_or_undefined));
}

#[test]
fn test_no_unchecked_indexed_access_string_index_signature() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let index_access = interner.intern(TypeData::IndexAccess(indexed, TypeId::STRING));

    assert!(checker.is_subtype_of(index_access, TypeId::NUMBER));

    checker.no_unchecked_indexed_access = true;

    assert!(!checker.is_subtype_of(index_access, TypeId::NUMBER));
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert!(checker.is_subtype_of(index_access, number_or_undefined));
}

#[test]
fn test_no_unchecked_indexed_access_union_index_signature() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let index_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let index_access = interner.intern(TypeData::IndexAccess(indexed, index_type));

    assert!(checker.is_subtype_of(index_access, TypeId::NUMBER));

    checker.no_unchecked_indexed_access = true;

    assert!(!checker.is_subtype_of(index_access, TypeId::NUMBER));
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert!(checker.is_subtype_of(index_access, number_or_undefined));
}

#[test]
fn test_correlated_union_index_access_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let kind = interner.intern_string("kind");
    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");

    let obj_a = interner.object(vec![
        PropertyInfo::new(kind, interner.literal_string("a")),
        PropertyInfo::new(key_a, TypeId::NUMBER),
    ]);
    let obj_b = interner.object(vec![
        PropertyInfo::new(kind, interner.literal_string("b")),
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);
    let index_access = interner.intern(TypeData::IndexAccess(union_obj, key_union));
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    assert!(checker.is_subtype_of(index_access, expected));
    assert!(!checker.is_subtype_of(index_access, TypeId::NUMBER));
}

#[test]
fn test_object_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // { x: number }
    let obj_x = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    // { x: number, y: string }
    let obj_xy = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    // Object with more properties is subtype
    assert!(checker.is_subtype_of(obj_xy, obj_x));

    // Object with fewer properties is not subtype
    assert!(!checker.is_subtype_of(obj_x, obj_xy));
}

#[test]
fn test_readonly_property_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let name = interner.intern_string("x");
    let readonly_obj = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);
    let mutable_obj = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // TypeScript allows readonly property → mutable property assignment
    assert!(checker.is_subtype_of(readonly_obj, mutable_obj));
    assert!(checker.is_subtype_of(mutable_obj, readonly_obj));
}

#[test]
fn test_readonly_array_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let mutable_array = interner.array(TypeId::STRING);
    let readonly_array = interner.intern(TypeData::ReadonlyType(mutable_array));

    assert!(checker.is_subtype_of(mutable_array, readonly_array));
    assert!(!checker.is_subtype_of(readonly_array, mutable_array));
}

struct ReadonlyArrayDefResolver {
    def_id: DefId,
}

impl TypeResolver for ReadonlyArrayDefResolver {
    fn resolve_ref(
        &self,
        _symbol: SymbolRef,
        _interner: &dyn crate::construction::TypeDatabase,
    ) -> Option<TypeId> {
        None
    }

    fn is_builtin_readonly_array_def(&self, def_id: DefId) -> bool {
        def_id == self.def_id
    }
}

#[test]
fn test_readonly_array_application_matches_readonly_array_syntax() {
    let interner = TypeInterner::new();
    let resolver = ReadonlyArrayDefResolver { def_id: DefId(1) };
    let mut checker = SubtypeChecker::with_resolver(&interner, &resolver);

    let readonly_array_def = interner.lazy(DefId(1));
    let readonly_array_app = interner.application(readonly_array_def, vec![TypeId::STRING]);
    let readonly_string_array = interner.readonly_type(interner.array(TypeId::STRING));
    let readonly_number_array = interner.readonly_type(interner.array(TypeId::NUMBER));
    let app_or_null = interner.union(vec![readonly_array_app, TypeId::NULL]);
    let syntax_or_null = interner.union(vec![readonly_string_array, TypeId::NULL]);

    assert!(checker.is_subtype_of(readonly_array_app, readonly_string_array));
    assert!(checker.is_subtype_of(readonly_string_array, readonly_array_app));
    assert!(checker.is_subtype_of(app_or_null, syntax_or_null));
    assert!(!checker.is_subtype_of(readonly_array_app, readonly_number_array));

    let shadow_resolver = ReadonlyArrayDefResolver { def_id: DefId(99) };
    let mut shadow_checker = SubtypeChecker::with_resolver(&interner, &shadow_resolver);
    assert!(!shadow_checker.is_subtype_of(readonly_array_app, readonly_string_array));
}

#[test]
fn test_readonly_tuple_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

    assert!(checker.is_subtype_of(tuple, readonly_tuple));
    assert!(!checker.is_subtype_of(readonly_tuple, tuple));
}

#[test]
fn test_array_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);
    let any_array = interner.array(TypeId::ANY);

    // Same element type
    assert!(checker.is_subtype_of(string_array, string_array));

    // Different element type
    assert!(!checker.is_subtype_of(string_array, number_array));

    // Covariance with any
    assert!(checker.is_subtype_of(string_array, any_array));
}

#[test]
fn test_array_to_iterable_protocol_subtyping() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);
    let mut checker = SubtypeChecker::with_resolver(&interner, &cache).with_query_db(&cache);

    let array_length = interner.intern_string("length");
    let array_base = interner.object(vec![PropertyInfo::readonly(array_length, TypeId::NUMBER)]);
    interner.set_array_base_type(array_base, vec![]);

    let iterator_name = interner.intern_string("[Symbol.iterator]");
    let next_name = interner.intern_string("next");
    let value_name = interner.intern_string("value");
    let done_name = interner.intern_string("done");

    let iterator_result_type = |value_ty| {
        interner.object(vec![
            PropertyInfo::new(value_name, value_ty),
            PropertyInfo::readonly(done_name, TypeId::BOOLEAN),
        ])
    };

    let iterator_type = |value_ty| {
        let next = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: iterator_result_type(value_ty),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        interner.object(vec![PropertyInfo::method(next_name, next)])
    };

    let iterable_of = |value_ty| {
        let iter_fn = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: iterator_type(value_ty),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        interner.object(vec![PropertyInfo::method(iterator_name, iter_fn)])
    };

    let iterable_number = iterable_of(TypeId::NUMBER);
    let iterable_string = iterable_of(TypeId::STRING);
    let iterator_info =
        crate::operations::iterators::get_iterator_info(&cache, iterable_number, false)
            .expect("iterable target should expose iterable info");
    assert_eq!(iterator_info.yield_type, TypeId::NUMBER);
    let source = interner.array(TypeId::NUMBER);

    assert!(!checker.is_subtype_of(array_base, iterable_number));
    let interface_result = checker
        .check_array_interface_subtype(TypeId::NUMBER, iterable_number)
        .expect("array interface check should apply");
    assert!(interface_result.is_true());
    assert!(checker.is_subtype_of(source, iterable_number));
    assert!(!checker.is_subtype_of(source, iterable_string));
}

#[test]
fn test_array_covariant_mutable_unsoundness() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(string_or_number);

    assert!(checker.is_subtype_of(string_array, union_array));
    assert!(!checker.is_subtype_of(union_array, string_array));
}

#[test]
fn test_type_environment() {
    let _interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Register some types
    let sym1 = SymbolRef(1);
    let sym2 = SymbolRef(2);
    env.insert(sym1, TypeId::STRING);
    env.insert(sym2, TypeId::NUMBER);

    // Check retrieval
    assert_eq!(env.get(sym1), Some(TypeId::STRING));
    assert_eq!(env.get(sym2), Some(TypeId::NUMBER));
    assert_eq!(env.get(SymbolRef(999)), None);

    // Check contains
    assert!(env.contains(sym1));
    assert!(!env.contains(SymbolRef(999)));
}

#[test]
fn test_ref_resolution_with_environment() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Create a Ref type for symbol 1
    let ref_type = interner.lazy(DefId(1));

    // Without resolution, Ref to anything should fail (no noop resolution)
    let mut checker = SubtypeChecker::new(&interner);
    // Ref to intrinsic - can't resolve, so falls back to false
    assert!(!checker.is_subtype_of(ref_type, TypeId::STRING));

    // Add resolution: symbol 1 = string
    env.insert_def(DefId(1), TypeId::STRING);

    // With environment, Ref(1) resolves to string
    let mut checker_with_env = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker_with_env.is_subtype_of(ref_type, TypeId::STRING));
    assert!(!checker_with_env.is_subtype_of(ref_type, TypeId::NUMBER));
}

#[test]
fn test_reference_lazy_fallback_uses_symbol_to_def_mapping() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Register a real DefId and map a raw SymbolId back to it.
    let real_def = DefId(100);
    env.insert_def(real_def, TypeId::STRING);
    env.register_def_symbol_mapping(real_def, SymbolId(5));

    let raw_reference = interner.reference(SymbolRef(5));

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker.is_subtype_of(raw_reference, TypeId::STRING));
    assert!(!checker.is_subtype_of(raw_reference, TypeId::NUMBER));
}

#[test]
fn test_lazy_type_params_falls_back_from_symbol_based_lazy_ref() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let generic_def = DefId(200);
    env.insert_def_with_params(generic_def, TypeId::STRING, vec![t_param]);
    env.register_def_symbol_mapping(generic_def, SymbolId(42));

    let raw_lazy = env
        .get_lazy_type_params(DefId(42))
        .expect("fallback should resolve params");
    assert_eq!(raw_lazy.len(), 1);
    assert_eq!(raw_lazy[0], t_param);

    let symbol_reference = interner.reference(SymbolRef(42));
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker.is_subtype_of(symbol_reference, TypeId::STRING));
}

