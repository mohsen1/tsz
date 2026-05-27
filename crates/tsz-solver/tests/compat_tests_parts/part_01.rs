#[test]
fn test_object_keyword_accepts_non_primitives() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("name");
    let obj = interner.object(vec![PropertyInfo {
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
    assert!(checker.is_assignable(obj, TypeId::OBJECT));

    let array = interner.array(TypeId::NUMBER);
    assert!(checker.is_assignable(array, TypeId::OBJECT));

    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert!(checker.is_assignable(tuple, TypeId::OBJECT));

    let func = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert!(checker.is_assignable(func, TypeId::OBJECT));
}

#[test]
fn test_object_keyword_rejects_primitives() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    assert!(!checker.is_assignable(TypeId::STRING, TypeId::OBJECT));
    assert!(!checker.is_assignable(TypeId::NUMBER, TypeId::OBJECT));
}

#[test]
fn test_object_interface_accepts_primitives() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let object_interface = make_object_interface(&interner);
    assert!(checker.is_assignable(TypeId::STRING, object_interface));
    assert!(checker.is_assignable(TypeId::NUMBER, object_interface));
    assert!(checker.is_assignable(TypeId::BOOLEAN, object_interface));
    assert!(checker.is_assignable(TypeId::SYMBOL, object_interface));

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);
    assert!(checker.is_assignable(template, object_interface));
}

#[test]
fn test_object_trifecta_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let empty_object = interner.object(Vec::new());
    let object_interface = make_object_interface(&interner);

    assert!(checker.is_assignable(TypeId::STRING, empty_object));
    assert!(checker.is_assignable(TypeId::STRING, object_interface));
    assert!(!checker.is_assignable(TypeId::STRING, TypeId::OBJECT));
}

#[test]
fn test_split_accessor_allows_wider_setter_in_source() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: interner.union2(TypeId::STRING, TypeId::NUMBER),
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

    let target = interner.object(vec![PropertyInfo {
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

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_split_accessor_rejects_wider_setter_in_target() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
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

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: interner.union2(TypeId::STRING, TypeId::NUMBER),
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

    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_function_type_accepts_callables() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let function_top = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let function = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    assert!(checker.is_assignable(function, function_top));

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(TypeId::STRING)],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });
    assert!(checker.is_assignable(callable, function_top));
}

#[test]
fn test_function_type_rejects_non_callables() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let function_top = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let name = interner.intern_string("name");
    let obj = interner.object(vec![PropertyInfo {
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
    assert!(!checker.is_assignable(obj, function_top));
}

#[test]
fn test_function_type_not_assignable_to_specific_callable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let function_top = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let specific_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    assert!(!checker.is_assignable(function_top, specific_callable));
}

#[test]
fn test_tuple_array_assignability_tuple_to_array() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

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
    let elem_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let array = interner.array(elem_union);

    assert!(checker.is_assignable(tuple, array));
}

#[test]
fn test_tuple_array_assignability_tuple_to_array_rejects() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

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
    let array = interner.array(TypeId::STRING);

    assert!(!checker.is_assignable(tuple, array));
}

#[test]
fn test_tuple_array_assignability_array_to_tuple_rejects() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let array = interner.array(TypeId::STRING);
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    assert!(!checker.is_assignable(array, tuple));
}

#[test]
fn test_tuple_array_assignability_empty_array_to_optional_tuple() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let never_array = interner.array(TypeId::NEVER);
    let optional_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: true,
        rest: false,
    }]);
    let empty_tuple = interner.tuple(Vec::new());
    let rest_tuple = interner.tuple(vec![TupleElement {
        type_id: interner.array(TypeId::STRING),
        name: None,
        optional: false,
        rest: true,
    }]);

    assert!(checker.is_assignable(never_array, empty_tuple));
    assert!(checker.is_assignable(never_array, optional_tuple));
    assert!(checker.is_assignable(never_array, rest_tuple));
}

#[test]
fn test_apparent_string_members_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let length = interner.intern_string("length");
    let to_upper = interner.intern_string("toUpperCase");
    let to_upper_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![
        PropertyInfo::new(length, TypeId::NUMBER),
        PropertyInfo::method(to_upper, to_upper_type),
    ]);

    assert!(checker.is_assignable(TypeId::STRING, target));

    let literal = interner.literal_string("hello");
    assert!(checker.is_assignable(literal, target));
}

#[test]
fn test_apparent_string_members_include_substr_and_locale_compare() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let locale_compare = interner.intern_string("localeCompare");
    let substr = interner.intern_string("substr");
    let locale_compare_type = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("that")),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let substr_type = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("start")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("length")),
                type_id: TypeId::ANY,
                optional: true,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![
        PropertyInfo::method(locale_compare, locale_compare_type),
        PropertyInfo::method(substr, substr_type),
    ]);

    assert!(checker.is_assignable(TypeId::STRING, target));
}

#[test]
fn test_apparent_string_members_include_legacy_and_unicode() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let normalize = interner.intern_string("normalize");
    let is_well_formed = interner.intern_string("isWellFormed");
    let fontcolor = interner.intern_string("fontcolor");

    let normalize_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let is_well_formed_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fontcolor_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![
        PropertyInfo::method(normalize, normalize_type),
        PropertyInfo::method(is_well_formed, is_well_formed_type),
        PropertyInfo::method(fontcolor, fontcolor_type),
    ]);

    assert!(checker.is_assignable(TypeId::STRING, target));
}

#[test]
fn test_apparent_string_members_reject_mismatch() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let length = interner.intern_string("length");
    let target = interner.object(vec![PropertyInfo::new(length, TypeId::STRING)]);

    assert!(!checker.is_assignable(TypeId::STRING, target));
}

#[test]
fn test_apparent_string_members_exclude_at() {
    // `at` is es2022. The bootstrap apparent type for `string` must not
    // include it, so that property access falls through to the
    // checker's TS2550 ("change your target library") suggestion when
    // the loaded lib predates es2022.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let at = interner.intern_string("at");
    let at_type = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("index")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let target = interner.object(vec![PropertyInfo::method(at, at_type)]);

    assert!(!checker.is_assignable(TypeId::STRING, target));
    let literal = interner.literal_string("hello");
    assert!(!checker.is_assignable(literal, target));
}

#[test]
fn test_apparent_number_method_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let to_fixed = interner.intern_string("toFixed");
    let to_fixed_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![PropertyInfo::method(to_fixed, to_fixed_type)]);

    assert!(checker.is_assignable(TypeId::NUMBER, target));
}

#[test]
fn test_apparent_number_method_not_assignable_to_number() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let to_fixed = interner.intern_string("toFixed");
    let to_fixed_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![PropertyInfo::method(to_fixed, to_fixed_type)]);

    assert!(!checker.is_assignable(target, TypeId::NUMBER));
}

#[test]
fn test_apparent_number_member_rejects_mismatch() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

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
    let mismatch = interner.object(vec![PropertyInfo::method(to_fixed, method(TypeId::NUMBER))]);

    assert!(!checker.is_assignable(TypeId::NUMBER, mismatch));
}

#[test]
fn test_number_interface_boxing_assignability() {
    let interner = TypeInterner::new();
    let def_id = DefId(1);
    let number_interface = interner.lazy(def_id);

    let to_fixed = interner.intern_string("toFixed");
    let to_fixed_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_object = interner.object(vec![PropertyInfo::method(to_fixed, to_fixed_type)]);

    let mut env = TypeEnvironment::new();
    env.insert_def(def_id, number_object);

    let mut checker = CompatChecker::with_resolver(&interner, &env);
    assert!(checker.is_assignable(TypeId::NUMBER, number_interface));
    assert!(!checker.is_assignable(number_interface, TypeId::NUMBER));
}

#[test]
fn test_apparent_boolean_members_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let to_string = interner.intern_string("toString");
    let to_string_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![PropertyInfo::method(to_string, to_string_type)]);

    assert!(checker.is_assignable(TypeId::BOOLEAN, target));
}

#[test]
fn test_apparent_bigint_members_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let value_of = interner.intern_string("valueOf");
    let value_of_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::BIGINT,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![PropertyInfo::method(value_of, value_of_type)]);

    assert!(checker.is_assignable(TypeId::BIGINT, target));
}

#[test]
fn test_apparent_symbol_members_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let description = interner.intern_string("description");
    let to_string = interner.intern_string("toString");
    let description_type = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    let to_string_type = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.object(vec![
        PropertyInfo::new(description, description_type),
        PropertyInfo::method(to_string, to_string_type),
    ]);

    assert!(checker.is_assignable(TypeId::SYMBOL, target));
}

#[test]
fn test_apparent_string_number_index_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

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

    assert!(checker.is_assignable(TypeId::STRING, target));
}

#[test]
fn test_apparent_string_rejects_string_index_signature() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let target = interner.object_with_index(ObjectShape {
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

    assert!(!checker.is_assignable(TypeId::STRING, target));
}

#[test]
fn test_optional_property_allows_undefined() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::UNDEFINED,
        write_type: TypeId::UNDEFINED,
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

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_optional_property_rejects_required_target() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
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
    let target = interner.object(vec![PropertyInfo {
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

    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_optional_property_rejects_string_index_signature() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
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

    let target = interner.object_with_index(ObjectShape {
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

    // In tsc, optional properties are compatible with index signatures that don't
    // include `undefined`. The optionality of a property does not contribute `undefined`
    // to the index signature check: `{ x?: number }` IS assignable to `{ [s: string]: number }`.
    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_template_literal_index_signature_tracks_excess_properties() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let allowed_suffix =
        interner.union2(interner.literal_string("A"), interner.literal_string("B"));
    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: interner.template_literal(vec![
                TemplateSpan::Text(interner.intern_string("prefix")),
                TemplateSpan::Type(allowed_suffix),
            ]),
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let good = interner.object_fresh(vec![PropertyInfo::new(
        interner.intern_string("prefixA"),
        TypeId::STRING,
    )]);
    let bad = interner.object_fresh(vec![PropertyInfo::new(
        interner.intern_string("prefixC"),
        TypeId::STRING,
    )]);

    assert!(checker.is_assignable(good, target));
    assert!(!checker.is_assignable(bad, target));
}

#[test]
fn test_exact_optional_property_rejects_undefined() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_exact_optional_property_types(true);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::UNDEFINED,
        write_type: TypeId::UNDEFINED,
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

    assert!(!checker.is_assignable(source, target));
}

#[test]
fn test_exact_optional_property_allows_string_index_signature() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_exact_optional_property_types(true);

    let name = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo {
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

    let target = interner.object_with_index(ObjectShape {
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

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_rest_any_callable_target_from_function() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let rest_any = interner.array(TypeId::ANY);
    let target = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: rest_any,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_rest_unknown_callable_target_from_callable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    checker.set_strict_function_types(true);

    let rest_unknown = interner.array(TypeId::UNKNOWN);
    let target = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo {
                name: None,
                type_id: rest_unknown,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let source = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            params: vec![ParamInfo::unnamed(TypeId::STRING)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            type_params: Vec::new(),
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_mapped_type_over_number_keys_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let constraint = interner.intern(TypeData::KeyOf(TypeId::NUMBER));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_fixed = interner.intern_string("toFixed");
    let expected = interner.object(vec![PropertyInfo::new(to_fixed, TypeId::BOOLEAN)]);
    let mismatch = interner.object(vec![PropertyInfo::new(to_fixed, TypeId::NUMBER)]);

    assert!(checker.is_assignable(mapped, expected));
    assert!(!checker.is_assignable(mapped, mismatch));
    assert!(!checker.is_assignable(expected, mapped));
}

#[test]
fn test_mapped_type_over_string_keys_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let constraint = interner.intern(TypeData::KeyOf(TypeId::STRING));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_upper = interner.intern_string("toUpperCase");
    let expected = interner.object(vec![PropertyInfo::new(to_upper, TypeId::BOOLEAN)]);
    let mismatch = interner.object(vec![PropertyInfo::new(to_upper, TypeId::NUMBER)]);

    assert!(checker.is_assignable(mapped, expected));
    assert!(!checker.is_assignable(mapped, mismatch));
    assert!(!checker.is_assignable(expected, mapped));
}

#[test]
fn test_mapped_type_over_boolean_keys_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let constraint = interner.intern(TypeData::KeyOf(TypeId::BOOLEAN));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_string = interner.intern_string("toString");
    let expected = interner.object(vec![PropertyInfo::new(to_string, TypeId::NUMBER)]);
    let mismatch = interner.object(vec![PropertyInfo::new(to_string, TypeId::STRING)]);

    assert!(checker.is_assignable(mapped, expected));
    assert!(!checker.is_assignable(mapped, mismatch));
    assert!(checker.is_assignable(expected, mapped));
}

#[test]
fn test_mapped_type_key_remap_filters_keys() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a.clone(), prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
        is_const: false,
    };
    let key_param_id = interner.intern(TypeData::TypeParameter(key_param));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeData::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let expected = interner.object(vec![prop_b]);
    let requires_a = interner.object(vec![prop_a]);

    assert!(checker.is_assignable(mapped, expected));
    assert!(checker.is_assignable(expected, mapped));
    assert!(!checker.is_assignable(mapped, requires_a));
}

#[test]
fn test_conditional_tuple_wrapper_no_distribution_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let tuple_check = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: false,
    }]);
    let tuple_extends = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    let conditional = interner.conditional(ConditionalType {
        check_type: tuple_check,
        extends_type: tuple_extends,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });

    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, string_or_number);

    let instantiated = instantiate_type(&interner, conditional, &subst);

    assert!(checker.is_assignable(instantiated, TypeId::BOOLEAN));
    assert!(!checker.is_assignable(instantiated, TypeId::NUMBER));
}

#[test]
fn test_keyof_intersection_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    let keyof_a = interner.intern(TypeData::KeyOf(obj_a));
    let keyof_intersection = interner.intern(TypeData::KeyOf(intersection));

    assert!(checker.is_assignable(keyof_a, keyof_intersection));
    assert!(!checker.is_assignable(keyof_intersection, keyof_a));
}

#[test]
fn test_keyof_union_index_signature_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let string_index = interner.object_with_index(ObjectShape {
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
    let number_index = interner.object_with_index(ObjectShape {
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

    let union = interner.union(vec![string_index, number_index]);
    let keyof_union = interner.intern(TypeData::KeyOf(union));

    assert!(checker.is_assignable(keyof_union, TypeId::NUMBER));
    assert!(!checker.is_assignable(keyof_union, TypeId::STRING));
}

#[test]
fn test_keyof_union_intersection_only_shared_keys() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER);
    let prop_b = PropertyInfo::new(interner.intern_string("b"), TypeId::STRING);
    let prop_c = PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN);

    let obj_ab = interner.object(vec![prop_a.clone(), prop_b]);
    let obj_ac = interner.object(vec![prop_a, prop_c]);
    let union = interner.union(vec![obj_ab, obj_ac]);
    let keyof_union = interner.intern(TypeData::KeyOf(union));

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");

    assert!(checker.is_assignable(key_a, keyof_union));
    assert!(!checker.is_assignable(key_b, keyof_union));
    assert!(!checker.is_assignable(key_c, keyof_union));
}

#[test]
fn test_intersection_reduction_disjoint_discriminant_assignable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let kind = interner.intern_string("kind");
    let obj_a = interner.object(vec![PropertyInfo::new(kind, interner.literal_string("a"))]);
    let obj_b = interner.object(vec![PropertyInfo::new(kind, interner.literal_string("b"))]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    assert!(checker.is_assignable(intersection, TypeId::NEVER));
    assert!(checker.is_assignable(intersection, TypeId::STRING));
}

#[test]
fn test_intersection_reduction_disjoint_primitives() {
    let interner = TypeInterner::new();

    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);

    assert_eq!(intersection, TypeId::NEVER);
}

#[test]
fn test_unique_symbol_nominal_assignability() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let sym_a = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeData::UniqueSymbol(SymbolRef(2)));

    assert!(checker.is_assignable(sym_a, TypeId::SYMBOL));
    assert!(!checker.is_assignable(TypeId::SYMBOL, sym_a));
    assert!(checker.is_assignable(sym_a, sym_a));
    assert!(!checker.is_assignable(sym_a, sym_b));
}

#[test]
fn test_template_literal_expansion_limit_widens_to_string() {
    let interner = TypeInterner::new();

    let count = crate::intern::TEMPLATE_LITERAL_EXPANSION_LIMIT + 1;
    let mut members = Vec::with_capacity(count);
    for idx in 0..count {
        let literal = interner.literal_string(&format!("k{idx}"));
        members.push(literal);
    }
    let union = interner.union(members);
    let template = interner.template_literal(vec![TemplateSpan::Type(union)]);

    assert_eq!(template, TypeId::STRING);
}

// =============================================================================
// Weak Type Detection - Comprehensive Tests (Catalog Rule #13)
// =============================================================================

#[test]
fn test_weak_type_all_optional_properties_detection() {
    // Verifies that types with ALL optional properties are detected as weak
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    // Target with all optional properties - weak type
    let weak_target = interner.object(vec![
        PropertyInfo::opt(a, TypeId::STRING),
        PropertyInfo::opt(b, TypeId::NUMBER),
    ]);

    // Source with no overlapping properties - should be rejected
    let c = interner.intern_string("c");
    let source = interner.object(vec![PropertyInfo::new(c, TypeId::BOOLEAN)]);

    assert!(!checker.is_assignable(source, weak_target));
    // The weak type violation is detected internally and causes the assignability to fail
    // We can verify this by checking the failure reason
    assert!(matches!(
        checker.explain_failure(source, weak_target),
        Some(SubtypeFailureReason::NoCommonProperties { .. })
    ));
}

#[test]
fn test_weak_type_with_index_signature_not_weak() {
    // Types with index signatures are NOT weak, even with all optional properties
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");

    // Target with optional property + index signature - NOT weak
    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::opt(a, TypeId::STRING)],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Source with no overlapping properties - should be accepted due to index signature
    let b = interner.intern_string("b");
    let source = interner.object(vec![PropertyInfo::new(b, TypeId::NUMBER)]);

    assert!(checker.is_assignable(source, target));
}

#[test]
fn test_weak_type_empty_source_accepted() {
    // Empty objects are assignable to weak types
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");

    let weak_target = interner.object(vec![PropertyInfo::opt(a, TypeId::STRING)]);

    let empty_source = interner.object(Vec::new());

    // Empty source should be accepted (no conflicting properties)
    assert!(checker.is_assignable(empty_source, weak_target));
}

#[test]
fn test_intersection_with_primitive_not_weak_type() {
    // Reproduces the instantiateContextualTypes.ts false positive.
    // `string & { attachPayloadTypeHack?: P & never }` is NOT a weak type
    // because `string` is not a weak type. In tsc, isWeakType() for an
    // intersection requires ALL members to be weak.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let prop_name = interner.intern_string("attachPayloadTypeHack");

    // Create { attachPayloadTypeHack?: never } — a weak object type
    let weak_obj = interner.object(vec![PropertyInfo::opt(prop_name, TypeId::NEVER)]);

    // Create string & { attachPayloadTypeHack?: never } — NOT weak because string is not weak
    let intersection = interner.intersection2(TypeId::STRING, weak_obj);

    // A string literal should be assignable to this intersection (string part matches)
    let source = interner.literal_string("NON_VOID_ACTION");

    // This should NOT produce a NoCommonProperties failure.
    // Before the fix, the weak type check incorrectly classified this
    // intersection as weak because it only looked at the object member.
    let failure = checker.explain_failure(source, intersection);
    assert!(
        !matches!(
            failure,
            Some(SubtypeFailureReason::NoCommonProperties { .. })
        ),
        "string & {{ weak_object }} should NOT be treated as a weak type; got: {failure:?}"
    );
}

#[test]
fn test_intersection_of_all_weak_types_is_still_weak() {
    // When ALL members of an intersection are weak types, the intersection IS weak.
    // e.g., `{ a?: number } & { b?: string }` is still a weak type.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");

    let weak1 = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let weak2 = interner.object(vec![PropertyInfo::opt(b, TypeId::STRING)]);

    let intersection = interner.intersection2(weak1, weak2);

    // Source with no overlapping properties should be rejected (weak type violation)
    let source = interner.object(vec![PropertyInfo::new(c, TypeId::BOOLEAN)]);
    assert!(!checker.is_assignable(source, intersection));
    assert!(matches!(
        checker.explain_failure(source, intersection),
        Some(SubtypeFailureReason::NoCommonProperties { .. })
    ));
}

#[test]
fn test_intersection_weak_type_source_matching_second_member_not_violation() {
    // Source has a property that matches the SECOND weak intersection member.
    // The weak-type check must consider all members' properties, not just the first.
    // `{ b: boolean } <: { a?: number } & { b?: string }` — b is in the second member,
    // so source shares a property name with the intersection. Not a weak-type violation.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let weak1 = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let weak2 = interner.object(vec![PropertyInfo::opt(b, TypeId::STRING)]);

    let intersection = interner.intersection2(weak1, weak2);

    // Source with property `b` matches weak2 — not a NoCommonProperties violation.
    // (There may be a type-mismatch for b: boolean vs b?: string, but that is
    // a different diagnostic, not TS2559.)
    let source_b = interner.object(vec![PropertyInfo::new(b, TypeId::BOOLEAN)]);
    assert!(
        !matches!(
            checker.explain_failure(source_b, intersection),
            Some(SubtypeFailureReason::NoCommonProperties { .. })
        ),
        "Source with property in second member must not trigger NoCommonProperties"
    );
}

#[test]
fn test_intersection_weak_type_three_members_no_common() {
    // Three-member weak intersection: source has no property in any member.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");
    let c = interner.intern_string("c");
    let z = interner.intern_string("z");

    let w1 = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    let w2 = interner.object(vec![PropertyInfo::opt(b, TypeId::STRING)]);
    let w3 = interner.object(vec![PropertyInfo::opt(c, TypeId::BOOLEAN)]);

    let intersection = interner.intersection(vec![w1, w2, w3]);

    let source = interner.object(vec![PropertyInfo::new(z, TypeId::NUMBER)]);
    assert!(!checker.is_assignable(source, intersection));
    assert!(matches!(
        checker.explain_failure(source, intersection),
        Some(SubtypeFailureReason::NoCommonProperties { .. })
    ));
}

#[test]
fn test_intersection_with_non_weak_member_not_weak_intersection() {
    // An intersection where at least one member is NOT weak is not a weak intersection.
    // `string & { a?: number }` is not weak because `string` is not weak.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let z = interner.intern_string("z");

    let weak = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);
    // Use a required-property object to make the intersection non-weak.
    let non_weak = interner.object(vec![PropertyInfo::new(z, TypeId::STRING)]);

    let intersection = interner.intersection2(weak, non_weak);

    let source_unrelated = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    // Non-weak intersection should not trigger NoCommonProperties.
    assert!(
        !matches!(
            checker.explain_failure(source_unrelated, intersection),
            Some(SubtypeFailureReason::NoCommonProperties { .. })
        ),
        "Non-weak intersection must not trigger NoCommonProperties"
    );
}

#[test]
fn test_weak_union_with_all_weak_members() {
    // Weak union: union of only weak types
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let weak_a = interner.object(vec![PropertyInfo::opt(a, TypeId::STRING)]);
    let weak_b = interner.object(vec![PropertyInfo::opt(b, TypeId::NUMBER)]);

    let weak_union = interner.union(vec![weak_a, weak_b]);

    let c = interner.intern_string("c");
    let source = interner.object(vec![PropertyInfo::new(c, TypeId::BOOLEAN)]);

    // Source with no overlap should be rejected
    assert!(!checker.is_assignable(source, weak_union));
}

#[test]
fn test_weak_union_with_non_weak_member_not_weak() {
    // Union with at least one non-weak member is not a weak union
    // Normal union typing applies: source must match at least one member
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let a = interner.intern_string("a");
    let b = interner.intern_string("b");

    let weak_type = interner.object(vec![PropertyInfo::opt(a, TypeId::STRING)]);
    let non_weak_type = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false, // Required property - NOT weak
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

    let union = interner.union(vec![weak_type, non_weak_type]);

    // Source that matches the non-weak member
    let source_matching_non_weak = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER, // Matches the required property
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

    // Should be accepted since source matches the non-weak member
    assert!(
        checker.is_assignable(source_matching_non_weak, union),
        "Source matching non-weak member should be assignable to union"
    );

    // Source that doesn't match any member should be rejected
    let c = interner.intern_string("c");
    let source_no_match = interner.object(vec![PropertyInfo::new(c, TypeId::BOOLEAN)]);

    // Should be rejected since source doesn't match any union member
    assert!(
        !checker.is_assignable(source_no_match, union),
        "Source not matching any union member should not be assignable"
    );
}

#[test]
fn test_global_object_type_exempt_from_weak_type_check() {
    // The global Object type (with its standard properties like constructor,
    // toString, hasOwnProperty, etc.) should be exempt from weak type checks.
    // This matches TypeScript behavior: Object is treated like {} for weak type
    // purposes. See TypeScript PR #16047.
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Create Object-like type with the standard 7 properties
    let constructor = interner.intern_string("constructor");
    let to_string = interner.intern_string("toString");
    let to_locale_string = interner.intern_string("toLocaleString");
    let value_of = interner.intern_string("valueOf");
    let has_own_property = interner.intern_string("hasOwnProperty");
    let is_prototype_of = interner.intern_string("isPrototypeOf");
    let property_is_enumerable = interner.intern_string("propertyIsEnumerable");

    let object_type = interner.object(vec![
        PropertyInfo::new(constructor, TypeId::ANY),
        PropertyInfo::new(to_string, TypeId::ANY),
        PropertyInfo::new(to_locale_string, TypeId::ANY),
        PropertyInfo::new(value_of, TypeId::ANY),
        PropertyInfo::new(has_own_property, TypeId::ANY),
        PropertyInfo::new(is_prototype_of, TypeId::ANY),
        PropertyInfo::new(property_is_enumerable, TypeId::ANY),
    ]);

    // Weak target (all optional properties, no overlap with Object)
    let wings = interner.intern_string("wings");
    let legs = interner.intern_string("legs");
    let weak_target = interner.object(vec![
        PropertyInfo::opt(wings, TypeId::BOOLEAN),
        PropertyInfo::opt(legs, TypeId::NUMBER),
    ]);

    // Object should be assignable to weak type (exempt from weak type check)
    assert!(
        checker.is_assignable(object_type, weak_target),
        "Global Object type should be exempt from weak type check"
    );

    // But a non-Object source with no overlap should still be rejected
    let name = interner.intern_string("name");
    let non_object = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    assert!(
        !checker.is_assignable(non_object, weak_target),
        "Non-Object source with no common properties should be rejected"
    );
}

// =============================================================================
// exact_optional_property_types Tests (Catalog Rule #14)
// =============================================================================

#[test]
fn test_exact_optional_property_types_distinguishes_undefined_from_missing() {
    // With exact_optional_property_types=true, optional properties distinguish
    // between "missing" and "undefined"
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    checker.set_exact_optional_property_types(true);

    let x = interner.intern_string("x");

    // { x?: number }
    let optional_number = interner.object(vec![PropertyInfo::opt(x, TypeId::NUMBER)]);

    // { x: number | undefined }
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let _explicit_undefined = interner.object(vec![PropertyInfo::new(x, number_or_undefined)]);

    // With exact mode, these are NOT the same
    // { x?: number } is NOT assignable to { x: number | undefined }
    assert!(
        !checker.is_assignable(optional_number, _explicit_undefined),
        "Optional property should not be assignable to explicit undefined union in exact mode"
    );
    // { x: number | undefined } is NOT assignable to { x?: number }
    assert!(
        !checker.is_assignable(_explicit_undefined, optional_number),
        "Explicit undefined union should not be assignable to optional property in exact mode"
    );
}

