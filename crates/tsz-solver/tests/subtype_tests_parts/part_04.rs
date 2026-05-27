#[test]
fn test_mapped_type_key_remap_optional_readonly_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

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
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: Some(MappedModifier::Add),
    });

    let optional_readonly_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
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
    let required_readonly_b =
        interner.object(vec![PropertyInfo::readonly(prop_b.name, TypeId::NUMBER)]);
    let optional_mutable_b = interner.object(vec![PropertyInfo::opt(prop_b.name, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, optional_readonly_b));
    assert!(!checker.is_subtype_of(mapped, required_readonly_b));
    // TypeScript allows readonly → mutable property assignment
    assert!(checker.is_subtype_of(mapped, optional_mutable_b));
}

#[test]
fn test_mapped_type_key_remap_optional_readonly_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    };
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

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
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: Some(MappedModifier::Remove),
    });

    let required_mutable_b = interner.object(vec![PropertyInfo::new(prop_b.name, TypeId::NUMBER)]);
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let required_mutable_b_with_undef =
        interner.object(vec![PropertyInfo::new(prop_b.name, number_or_undefined)]);
    let optional_mutable_b = interner.object(vec![PropertyInfo::opt(prop_b.name, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, required_mutable_b));
    assert!(checker.is_subtype_of(mapped, required_mutable_b_with_undef));
    assert!(checker.is_subtype_of(mapped, optional_mutable_b));
    assert!(!checker.is_subtype_of(optional_mutable_b, mapped));
}

#[test]
fn test_mapped_type_key_remap_readonly_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

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
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    });

    let readonly_b = interner.object(vec![PropertyInfo::readonly(prop_b.name, TypeId::NUMBER)]);
    let mutable_b = interner.object(vec![PropertyInfo::new(prop_b.name, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, readonly_b));
    // TypeScript allows readonly → mutable property assignment
    assert!(checker.is_subtype_of(mapped, mutable_b));
}

#[test]
fn test_mapped_type_key_remap_readonly_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo::new(interner.intern_string("a"), TypeId::STRING);
    let prop_b = PropertyInfo::readonly(interner.intern_string("b"), TypeId::NUMBER);
    let obj = interner.object(vec![prop_a, prop_b.clone()]);

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
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    });

    let mutable_b = interner.object(vec![PropertyInfo::new(prop_b.name, TypeId::NUMBER)]);
    let readonly_b = interner.object(vec![PropertyInfo::readonly(prop_b.name, TypeId::NUMBER)]);

    assert!(checker.is_subtype_of(mapped, mutable_b));
    assert!(checker.is_subtype_of(mapped, readonly_b));
    // TypeScript allows readonly → mutable property assignment
    assert!(checker.is_subtype_of(readonly_b, mapped));
}

#[test]
fn test_mapped_type_key_remap_all_never_empty_object() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let empty_object = interner.object(Vec::new());

    assert!(checker.is_subtype_of(mapped, empty_object));
    assert!(checker.is_subtype_of(empty_object, mapped));
}

// =============================================================================
// Variance in Generic Positions
// =============================================================================

#[test]
fn test_generic_function_constraint_directionality() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_function_types = true;

    let t = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::OBJECT),
        default: None,
        is_const: false,
    };
    let t_id = interner.intern(TypeData::TypeParameter(t));

    let t1 = TypeParamInfo {
        name: interner.intern_string("T1"),
        constraint: Some(t_id),
        default: None,
        is_const: false,
    };
    let t1_id = interner.intern(TypeData::TypeParameter(t1));

    let u = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(t_id),
        default: None,
        is_const: false,
    };
    let u_id = interner.intern(TypeData::TypeParameter(u));

    let v = TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: Some(t1_id),
        default: None,
        is_const: false,
    };
    let v_id = interner.intern(TypeData::TypeParameter(v));

    let fn_t = interner.function(FunctionShape {
        type_params: vec![u],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: u_id,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_id,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_t1 = interner.function(FunctionShape {
        type_params: vec![v],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: v_id,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: v_id,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // fn_t: <U extends T>(x: U) => U   (broader constraint: U extends T)
    // fn_t1: <V extends T1>(x: V) => V (narrower constraint: V extends T1, T1 extends T)
    //
    // Alpha-rename check uses target_to_source: targetConstraint ≤ sourceConstraint.
    // fn_t ≤ fn_t1: target=fn_t1, source=fn_t → targetConstraint(T1) ≤ sourceConstraint(T)
    //   T1 ≤ T → true (T1 extends T) → alpha-rename succeeds → subtype ✓
    assert!(checker.is_subtype_of(fn_t, fn_t1));
    // fn_t1 ≤ fn_t: target=fn_t, source=fn_t1 → targetConstraint(T) ≤ sourceConstraint(T1)
    //   T ≤ T1 → false (T doesn't extend T1) → alpha-rename fails
    //   → falls through to erasure/inference which may or may not succeed
    // This direction is NOT guaranteed to succeed with alpha-rename.
    // (The fallback erasure path handles it via constraint erasure + inference.)
}

#[test]
fn test_generic_covariant_return_position() {
    // Producer<T> = { get(): T } - T is in covariant position
    // Producer<string> <: Producer<string | number> (covariant)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let get_name = interner.intern_string("get");

    let get_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let get_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let producer_string = interner.object(vec![PropertyInfo {
        name: get_name,
        type_id: get_string,
        write_type: get_string,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let producer_union = interner.object(vec![PropertyInfo {
        name: get_name,
        type_id: get_union,
        write_type: get_union,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Covariant: Producer<string> <: Producer<string | number>
    assert!(checker.is_subtype_of(producer_string, producer_union));
    // Not the reverse
    assert!(!checker.is_subtype_of(producer_union, producer_string));
}

#[test]
fn test_generic_contravariant_param_position() {
    // Consumer<T> = { accept(x: T): void } - T is in contravariant position
    // Consumer<string | number> <: Consumer<string> (contravariant)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let accept_name = interner.intern_string("accept");

    let accept_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let accept_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let consumer_string = interner.object(vec![PropertyInfo::readonly(accept_name, accept_string)]);

    let consumer_union = interner.object(vec![PropertyInfo::readonly(accept_name, accept_union)]);

    // Contravariant: Consumer<string | number> <: Consumer<string>
    assert!(checker.is_subtype_of(consumer_union, consumer_string));
    // Not the reverse
    assert!(!checker.is_subtype_of(consumer_string, consumer_union));
}

#[test]
fn test_generic_mixed_variance_positions() {
    // Transform<T, U> = { process(input: T): U }
    // T is contravariant (param), U is covariant (return)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let process_name = interner.intern_string("process");
    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // process(input: string | number): string
    let process_wide_in_narrow_out = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("input")),
            type_id: wide_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // process(input: string): string | number
    let process_narrow_in_wide_out = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("input")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: wide_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let transform_a = interner.object(vec![PropertyInfo {
        name: process_name,
        type_id: process_wide_in_narrow_out,
        write_type: process_wide_in_narrow_out,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    let transform_b = interner.object(vec![PropertyInfo {
        name: process_name,
        type_id: process_narrow_in_wide_out,
        write_type: process_narrow_in_wide_out,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
    }]);

    // Transform with wider input and narrower output is subtype
    // (contravariant input, covariant output)
    assert!(checker.is_subtype_of(transform_a, transform_b));
    assert!(!checker.is_subtype_of(transform_b, transform_a));
}

// =============================================================================
// Bivariant Method Parameters
// =============================================================================

#[test]
fn test_method_bivariant_wider_param() {
    // Methods are bivariant in their parameters (TypeScript legacy behavior)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("handler");
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let method_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let method_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
            type_id: wide_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_narrow_method = interner.object(vec![PropertyInfo::method(method_name, method_narrow)]);

    let obj_wide_method = interner.object(vec![PropertyInfo::method(method_name, method_wide)]);

    // Methods are bivariant - both directions should work
    assert!(checker.is_subtype_of(obj_narrow_method, obj_wide_method));
    assert!(checker.is_subtype_of(obj_wide_method, obj_narrow_method));
}

#[test]
fn test_method_bivariant_callback_param() {
    // Method with callback parameter - bivariant behavior
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("on");

    let callback_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("data")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let callback_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("data")),
            type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let method_with_narrow_cb = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: callback_narrow,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let method_with_wide_cb = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: callback_wide,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_narrow_cb = interner.object(vec![PropertyInfo::method(
        method_name,
        method_with_narrow_cb,
    )]);

    let obj_wide_cb = interner.object(vec![PropertyInfo::method(method_name, method_with_wide_cb)]);

    // Bivariant methods allow both directions
    assert!(checker.is_subtype_of(obj_narrow_cb, obj_wide_cb));
    assert!(checker.is_subtype_of(obj_wide_cb, obj_narrow_cb));
}

#[test]
fn test_function_property_contravariant_not_bivariant() {
    // Function properties (not methods) should be contravariant in strict mode
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_name = interner.intern_string("handler");
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let fn_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
            type_id: wide_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // is_method: false - these are function properties, not methods
    let obj_narrow_fn = interner.object(vec![PropertyInfo::new(prop_name, fn_narrow)]);

    let obj_wide_fn = interner.object(vec![PropertyInfo::new(prop_name, fn_wide)]);

    // Function properties are contravariant in strict mode
    // wide param <: narrow param target (can accept string when expecting string|number)
    assert!(checker.is_subtype_of(obj_wide_fn, obj_narrow_fn));
    // Not bivariant - narrow param !<: wide param target
    assert!(!checker.is_subtype_of(obj_narrow_fn, obj_wide_fn));
}

// =============================================================================
// Invariant Mutable Property Types
// =============================================================================

#[test]
fn test_mutable_property_invariant_same_type() {
    // Mutable properties with same type should be compatible
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_name = interner.intern_string("value");

    let obj_string = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);

    let obj_string_2 = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);

    // Same mutable property types are compatible
    assert!(checker.is_subtype_of(obj_string, obj_string_2));
    assert!(checker.is_subtype_of(obj_string_2, obj_string));
}

#[test]
fn test_mutable_property_invariant_different_types() {
    // tsc uses covariant (not invariant) checking for mutable properties.
    // {value: string} IS assignable to {value: string | number} (covariant)
    // {value: string | number} is NOT assignable to {value: string} (narrowing)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_name = interner.intern_string("value");
    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_narrow = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);

    let obj_wide = interner.object(vec![PropertyInfo::new(prop_name, wide_type)]);

    // Narrow -> wide: OK (covariant property checking)
    assert!(checker.is_subtype_of(obj_narrow, obj_wide));
    // Wide -> narrow: NOT OK (string|number is not assignable to string)
    assert!(!checker.is_subtype_of(obj_wide, obj_narrow));
}

#[test]
fn test_mutable_property_split_accessor_wider_write() {
    // Property with split accessor: read narrow, write wide
    // This is safe and should be covariant-like
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_name = interner.intern_string("value");
    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_split = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: TypeId::STRING, // read type
        write_type: wide_type,   // write type (wider)
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

    let obj_normal = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);

    // Split accessor with wider write is a subtype (can write more, reads same)
    assert!(checker.is_subtype_of(obj_split, obj_normal));
    // Normal cannot substitute for split (narrower write type)
    assert!(!checker.is_subtype_of(obj_normal, obj_split));
}

#[test]
fn test_readonly_property_covariant() {
    // Readonly properties should be covariant (no writes)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_name = interner.intern_string("value");
    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_narrow_readonly =
        interner.object(vec![PropertyInfo::readonly(prop_name, TypeId::STRING)]);

    let obj_wide_readonly = interner.object(vec![PropertyInfo::readonly(prop_name, wide_type)]);

    // Readonly is covariant - narrow <: wide
    assert!(checker.is_subtype_of(obj_narrow_readonly, obj_wide_readonly));
    // Not the reverse
    assert!(!checker.is_subtype_of(obj_wide_readonly, obj_narrow_readonly));
}

#[test]
fn test_mutable_array_element_invariant() {
    // Arrays are covariant in TypeScript (unsound but intentional)
    // This test documents that behavior
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let wide_array = interner.array(interner.union(vec![TypeId::STRING, TypeId::NUMBER]));

    // TypeScript arrays are covariant (allows unsound mutations)
    assert!(checker.is_subtype_of(string_array, wide_array));
    // Not the reverse
    assert!(!checker.is_subtype_of(wide_array, string_array));
}

// =============================================================================
// Intersection Type Tests
// =============================================================================

#[test]
fn test_intersection_flattening_nested() {
    // (A & B) & C should be equivalent to A & B & C
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let obj_c = interner.object(vec![PropertyInfo::new(c_name, TypeId::BOOLEAN)]);

    // Nested: (A & B) & C
    let ab = interner.intersection(vec![obj_a, obj_b]);
    let nested = interner.intersection(vec![ab, obj_c]);

    // Flat: A & B & C
    let flat = interner.intersection(vec![obj_a, obj_b, obj_c]);

    // Both should be subtypes of each other (equivalent)
    assert!(checker.is_subtype_of(nested, flat));
    assert!(checker.is_subtype_of(flat, nested));
}

#[test]
fn test_intersection_flattening_single_element() {
    // A & (single element) should be equivalent to just A
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    // Single element intersection
    let single = interner.intersection(vec![obj_a]);

    // Should be equivalent to the element itself
    assert!(checker.is_subtype_of(single, obj_a));
    assert!(checker.is_subtype_of(obj_a, single));
}

#[test]
fn test_intersection_flattening_duplicates() {
    // A & A should be equivalent to A
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let duplicated = interner.intersection(vec![obj_a, obj_a]);

    // Should be equivalent to original
    assert!(checker.is_subtype_of(duplicated, obj_a));
    assert!(checker.is_subtype_of(obj_a, duplicated));
}

#[test]
fn test_intersection_with_never_is_never() {
    // A & never = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let with_never = interner.intersection(vec![obj_a, TypeId::NEVER]);

    // A & never should be subtype of never (i.e., is never)
    assert!(checker.is_subtype_of(with_never, TypeId::NEVER));
    // never is subtype of everything
    assert!(checker.is_subtype_of(TypeId::NEVER, with_never));
}

#[test]
fn test_intersection_never_absorbs_all() {
    // string & number & boolean & never = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let multi_with_never = interner.intersection(vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::NEVER,
    ]);

    assert!(checker.is_subtype_of(multi_with_never, TypeId::NEVER));
}

#[test]
fn test_intersection_never_at_any_position() {
    // never at beginning, middle, end should all reduce to never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let at_start = interner.intersection(vec![TypeId::NEVER, TypeId::STRING, TypeId::NUMBER]);
    let at_middle = interner.intersection(vec![TypeId::STRING, TypeId::NEVER, TypeId::NUMBER]);
    let at_end = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER, TypeId::NEVER]);

    assert!(checker.is_subtype_of(at_start, TypeId::NEVER));
    assert!(checker.is_subtype_of(at_middle, TypeId::NEVER));
    assert!(checker.is_subtype_of(at_end, TypeId::NEVER));
}

#[test]
fn test_object_intersection_merges_properties() {
    // { a: string } & { b: number } <: { a: string, b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    let merged = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    // Intersection should be subtype of merged object
    assert!(checker.is_subtype_of(intersection, merged));
    // Merged object should also be subtype of intersection
    assert!(checker.is_subtype_of(merged, intersection));
}

#[test]
fn test_object_intersection_same_property_narrowing() {
    // { x: string | number } & { x: string } = { x: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_wide = interner.object(vec![PropertyInfo::new(x_name, wide_type)]);

    let obj_narrow = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![obj_wide, obj_narrow]);

    // Intersection should be subtype of narrow (narrowed to string)
    assert!(checker.is_subtype_of(intersection, obj_narrow));
}

#[test]
fn test_object_intersection_three_objects() {
    // { a: string } & { b: number } & { c: boolean }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let obj_c = interner.object(vec![PropertyInfo::new(c_name, TypeId::BOOLEAN)]);

    let intersection = interner.intersection(vec![obj_a, obj_b, obj_c]);

    // Should be subtype of each individual object
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_b));
    assert!(checker.is_subtype_of(intersection, obj_c));
}

#[test]
fn test_object_intersection_with_optional_property() {
    // { a: string } & { b?: number } should have required a and optional b
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b_optional = interner.object(vec![PropertyInfo::opt(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b_optional]);

    // Should be subtype of required a
    assert!(checker.is_subtype_of(intersection, obj_a));
    // Should be subtype of optional b
    assert!(checker.is_subtype_of(intersection, obj_b_optional));
}

#[test]
fn test_intersection_subtype_of_each_member() {
    // A & B should be subtype of A and subtype of B
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    // A & B <: A
    assert!(checker.is_subtype_of(intersection, obj_a));
    // A & B <: B
    assert!(checker.is_subtype_of(intersection, obj_b));
    // A !<: A & B (missing b property)
    assert!(!checker.is_subtype_of(obj_a, intersection));
    // B !<: A & B (missing a property)
    assert!(!checker.is_subtype_of(obj_b, intersection));
}

// =============================================================================
// Literal Type Tests
// =============================================================================

#[test]
fn test_string_literal_narrows_to_union() {
    // "a" <: "a" | "b" | "c"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let c = interner.literal_string("c");

    let union = interner.union(vec![a, b, c]);

    // Each literal is subtype of the union
    assert!(checker.is_subtype_of(a, union));
    assert!(checker.is_subtype_of(b, union));
    assert!(checker.is_subtype_of(c, union));

    // Union is not subtype of individual literal
    assert!(!checker.is_subtype_of(union, a));
    assert!(!checker.is_subtype_of(union, b));
    assert!(!checker.is_subtype_of(union, c));
}

#[test]
fn test_string_literal_not_subtype_of_different_literal() {
    // "hello" is not subtype of "world"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    assert!(!checker.is_subtype_of(hello, world));
    assert!(!checker.is_subtype_of(world, hello));
}

#[test]
fn test_string_literal_subtype_of_string() {
    // Any string literal is subtype of string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let empty = interner.literal_string("");
    let special = interner.literal_string("!@#$%^&*()");

    assert!(checker.is_subtype_of(hello, TypeId::STRING));
    assert!(checker.is_subtype_of(empty, TypeId::STRING));
    assert!(checker.is_subtype_of(special, TypeId::STRING));

    // string is not subtype of literal
    assert!(!checker.is_subtype_of(TypeId::STRING, hello));
}

#[test]
fn test_string_literal_union_subtype_of_string() {
    // "a" | "b" <: string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let union = interner.union(vec![a, b]);

    assert!(checker.is_subtype_of(union, TypeId::STRING));
    assert!(!checker.is_subtype_of(TypeId::STRING, union));
}

#[test]
fn test_numeric_literal_types() {
    // 1 <: number, 1 === 1, 1 !== 2
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let zero = interner.literal_number(0.0);
    let negative = interner.literal_number(-42.0);
    const APPROX_FLOAT: f64 = 3.15;
    let float = interner.literal_number(APPROX_FLOAT);

    // Same literal is subtype of itself
    assert!(checker.is_subtype_of(one, one));
    assert!(checker.is_subtype_of(two, two));

    // Different literals are not subtypes of each other
    assert!(!checker.is_subtype_of(one, two));
    assert!(!checker.is_subtype_of(two, one));

    // All numeric literals are subtypes of number
    assert!(checker.is_subtype_of(one, TypeId::NUMBER));
    assert!(checker.is_subtype_of(zero, TypeId::NUMBER));
    assert!(checker.is_subtype_of(negative, TypeId::NUMBER));
    assert!(checker.is_subtype_of(float, TypeId::NUMBER));

    // number is not subtype of numeric literal
    assert!(!checker.is_subtype_of(TypeId::NUMBER, one));
}

#[test]
fn test_numeric_literal_union() {
    // 1 | 2 | 3 <: number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    let union = interner.union(vec![one, two, three]);

    // Union of numeric literals is subtype of number
    assert!(checker.is_subtype_of(union, TypeId::NUMBER));

    // Each literal is subtype of the union
    assert!(checker.is_subtype_of(one, union));
    assert!(checker.is_subtype_of(two, union));
    assert!(checker.is_subtype_of(three, union));

    // number is not subtype of the union
    assert!(!checker.is_subtype_of(TypeId::NUMBER, union));
}

#[test]
fn test_numeric_literal_special_values() {
    // Test special numeric values
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let zero = interner.literal_number(0.0);
    let neg_zero = interner.literal_number(-0.0);

    // Both are subtypes of number
    assert!(checker.is_subtype_of(zero, TypeId::NUMBER));
    assert!(checker.is_subtype_of(neg_zero, TypeId::NUMBER));
}

#[test]
fn test_template_literal_pattern_prefix() {
    // `prefix${string}` matches "prefix-anything"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Template: `prefix-${string}`
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    // Template is subtype of string
    assert!(checker.is_subtype_of(template, TypeId::STRING));

    // String literal matching the pattern
    let matching = interner.literal_string("prefix-hello");
    assert!(checker.is_subtype_of(matching, TypeId::STRING));

    // Literal "prefix-hello" should be subtype of the template pattern
    assert!(checker.is_subtype_of(matching, template));
}

#[test]
fn test_template_literal_pattern_suffix() {
    // `${string}-suffix` pattern
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Template: `${string}-suffix`
    let template = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("-suffix")),
    ]);

    // Template is subtype of string
    assert!(checker.is_subtype_of(template, TypeId::STRING));

    // Matching literal
    let matching = interner.literal_string("hello-suffix");
    assert!(checker.is_subtype_of(matching, template));

    // Non-matching literal should NOT be subtype
    let not_matching = interner.literal_string("hello-other");
    assert!(!checker.is_subtype_of(not_matching, template));
}

#[test]
fn test_template_literal_pattern_with_union() {
    // `color-${"red" | "blue"}` = "color-red" | "color-blue"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let red = interner.literal_string("red");
    let blue = interner.literal_string("blue");
    let colors = interner.union(vec![red, blue]);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("color-")),
        TemplateSpan::Type(colors),
    ]);

    // Template is subtype of string
    assert!(checker.is_subtype_of(template, TypeId::STRING));

    // Matching literals
    let color_red = interner.literal_string("color-red");
    let color_blue = interner.literal_string("color-blue");

    assert!(checker.is_subtype_of(color_red, template));
    assert!(checker.is_subtype_of(color_blue, template));

    // Non-matching literal
    let color_green = interner.literal_string("color-green");
    assert!(!checker.is_subtype_of(color_green, template));
}

#[test]
fn test_template_literal_pattern_multiple_parts() {
    // `${string}-${number}` pattern
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let template = interner.template_literal(vec![
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(TypeId::NUMBER),
    ]);

    // Template is subtype of string
    assert!(checker.is_subtype_of(template, TypeId::STRING));

    // Matching literal
    let matching = interner.literal_string("hello-42");
    assert!(checker.is_subtype_of(matching, template));
}

#[test]
fn test_template_literal_empty_parts() {
    // Template with just string interpolation `${string}`
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let template = interner.template_literal(vec![TemplateSpan::Type(TypeId::STRING)]);

    // Should be equivalent to string
    assert!(checker.is_subtype_of(template, TypeId::STRING));

    // Any string literal should match
    let hello = interner.literal_string("hello");
    assert!(checker.is_subtype_of(hello, template));
}

#[test]
fn test_boolean_literal_types() {
    // true and false literal types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Use literal_boolean to create true/false literal types
    let type_true = interner.literal_boolean(true);
    let type_false = interner.literal_boolean(false);

    // true and false literal types are subtypes of boolean
    assert!(checker.is_subtype_of(type_true, TypeId::BOOLEAN));
    assert!(checker.is_subtype_of(type_false, TypeId::BOOLEAN));

    // true and false are not subtypes of each other
    assert!(!checker.is_subtype_of(type_true, type_false));
    assert!(!checker.is_subtype_of(type_false, type_true));

    // boolean is not subtype of true or false
    assert!(!checker.is_subtype_of(TypeId::BOOLEAN, type_true));
    assert!(!checker.is_subtype_of(TypeId::BOOLEAN, type_false));
}

// =============================================================================
// Variance Tests - Covariant, Contravariant, Invariant, Bivariant
// =============================================================================

// -----------------------------------------------------------------------------
// Covariant Position (Return Types)
// -----------------------------------------------------------------------------

#[test]
fn test_covariant_return_type_subtype() {
    // () => string <: () => string | number
    // Return type is covariant: narrower return assignable to wider
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_return_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let fn_return_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: union,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Covariant: () => string <: () => string | number
    assert!(checker.is_subtype_of(fn_return_string, fn_return_union));
    // Not the reverse
    assert!(!checker.is_subtype_of(fn_return_union, fn_return_string));
}

#[test]
fn test_covariant_return_type_literal() {
    // () => "hello" <: () => string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let fn_return_literal = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: hello,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Covariant: () => "hello" <: () => string
    assert!(checker.is_subtype_of(fn_return_literal, fn_return_string));
    // Not the reverse
    assert!(!checker.is_subtype_of(fn_return_string, fn_return_literal));
}

#[test]
fn test_covariant_return_type_object() {
    // () => { a: string, b: number } <: () => { a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let obj_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let fn_return_ab = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_ab,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_a = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_a,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Covariant: more properties in return is subtype of fewer
    assert!(checker.is_subtype_of(fn_return_ab, fn_return_a));
    assert!(!checker.is_subtype_of(fn_return_a, fn_return_ab));
}

#[test]
fn test_covariant_return_type_array() {
    // () => string[] <: () => (string | number)[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union);

    let fn_return_string_arr = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: string_array,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_union_arr = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: union_array,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Covariant: narrower array type in return
    assert!(checker.is_subtype_of(fn_return_string_arr, fn_return_union_arr));
    assert!(!checker.is_subtype_of(fn_return_union_arr, fn_return_string_arr));
}

#[test]
fn test_covariant_return_never() {
    // () => never <: () => string
    // never is bottom type, subtype of everything
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_return_never = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NEVER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // never is subtype of any return type
    assert!(checker.is_subtype_of(fn_return_never, fn_return_string));
    // string is not subtype of never
    assert!(!checker.is_subtype_of(fn_return_string, fn_return_never));
}

#[test]
fn test_covariant_return_void_undefined() {
    // () => undefined <: () => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_return_undefined = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::UNDEFINED,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // undefined <: void
    assert!(checker.is_subtype_of(fn_return_undefined, fn_return_void));
}

// -----------------------------------------------------------------------------
// Contravariant Position (Parameter Types)
// -----------------------------------------------------------------------------

#[test]
fn test_contravariant_param_wider_is_subtype() {
    // (x: string | number) => void <: (x: string) => void
    // Param type is contravariant: wider param is subtype
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let fn_param_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: union,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_param_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Contravariant: (string | number) => void <: (string) => void
    assert!(checker.is_subtype_of(fn_param_union, fn_param_string));
    // Not the reverse
    assert!(!checker.is_subtype_of(fn_param_string, fn_param_union));
}

#[test]
fn test_contravariant_param_base_class() {
    // (x: Base) => void <: (x: Derived) => void
    // Base is "wider" than Derived
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let base_prop = interner.intern_string("base");
    let derived_prop = interner.intern_string("derived");

    // Base has one property
    let base = interner.object(vec![PropertyInfo::new(base_prop, TypeId::STRING)]);

    // Derived extends Base with additional property
    let derived = interner.object(vec![
        PropertyInfo::new(base_prop, TypeId::STRING),
        PropertyInfo::new(derived_prop, TypeId::NUMBER),
    ]);

    let fn_param_base = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: base,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_param_derived = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: derived,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Contravariant: (Base) => void <: (Derived) => void
    assert!(checker.is_subtype_of(fn_param_base, fn_param_derived));
    // Not the reverse
    assert!(!checker.is_subtype_of(fn_param_derived, fn_param_base));
}

#[test]
fn test_contravariant_param_unknown() {
    // (x: unknown) => void <: (x: T) => void for any T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_param_unknown = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_param_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (unknown) => void is subtype of (string) => void
    assert!(checker.is_subtype_of(fn_param_unknown, fn_param_string));
}

#[test]
fn test_contravariant_multiple_params() {
    // (a: A', b: B') => void <: (a: A, b: B) => void when A <: A' and B <: B'
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Wider params
    let fn_wider = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: union,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Narrower params
    let fn_narrower = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Contravariant in all params
    assert!(checker.is_subtype_of(fn_wider, fn_narrower));
    assert!(!checker.is_subtype_of(fn_narrower, fn_wider));
}

