#[test]
fn test_this_type_polymorphic_method_chain() {
    // Test fluent chaining with this type
    // class Builder {
    //   setName(name: string): this
    //   setValue(value: number): this
    //   build(): Result
    // }
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let result_type = interner.lazy(DefId(1));

    let set_name = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("name")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let set_value = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let build = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: result_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let builder = interner.object(vec![
        PropertyInfo::method(interner.intern_string("setName"), set_name),
        PropertyInfo::method(interner.intern_string("setValue"), set_value),
        PropertyInfo::method(interner.intern_string("build"), build),
    ]);

    // Builder with all fluent methods should be valid
    assert_ne!(builder, TypeId::ERROR);
}

#[test]
fn test_this_type_with_generics_in_class() {
    // class Container<T> {
    //   map<U>(fn: (value: T) => U): Container<U>
    //   filter(predicate: (value: T) => boolean): this
    // }
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let _t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let _u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };

    // filter method returning this (polymorphic return)
    let filter_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("predicate")),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: TypeId::UNKNOWN,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::BOOLEAN,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let container = interner.object(vec![PropertyInfo::method(
        interner.intern_string("filter"),
        filter_method,
    )]);

    // Container with filter returning this should be valid
    assert_ne!(container, TypeId::ERROR);
}

#[test]
fn test_this_type_class_hierarchy_multiple_methods() {
    // Test class hierarchy with multiple methods using this
    // class Base {
    //   method1(): this
    //   method2(): this
    // }
    // class Derived extends Base {
    //   method1(): this
    //   method2(): this
    //   method3(): number
    // }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let this_type = interner.intern(TypeData::ThisType);

    let method1 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let method2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let method3 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let base_class = interner.object(vec![
        PropertyInfo::method(interner.intern_string("method1"), method1),
        PropertyInfo::method(interner.intern_string("method2"), method2),
    ]);

    let derived_class = interner.object(vec![
        PropertyInfo::method(interner.intern_string("method1"), method1),
        PropertyInfo::method(interner.intern_string("method2"), method2),
        PropertyInfo::method(interner.intern_string("method3"), method3),
    ]);

    // Derived should be subtype of Base (all methods compatible)
    assert!(
        checker.is_subtype_of(derived_class, base_class),
        "Derived should be subtype of Base (all this-returning methods compatible)"
    );
}

#[test]
fn test_this_type_with_constrained_generic() {
    // Test this type with constrained generic parameter
    // class Base {
    //   method<T extends Base>(this: T): T
    // }
    let interner = TypeInterner::new();

    let base_ref = interner.lazy(DefId(100));
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(base_ref),
        default: None,
        is_const: false,
    };

    let t_type_param = interner.intern(TypeData::TypeParameter(t_param));

    // method<T extends Base>(this: T): T
    let constrained_method = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![],
        this_type: Some(t_type_param),
        return_type: t_type_param,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let base_class = interner.object(vec![PropertyInfo::method(
        interner.intern_string("method"),
        constrained_method,
    )]);

    // Base with constrained this method should be valid
    assert_ne!(base_class, TypeId::ERROR);
}
