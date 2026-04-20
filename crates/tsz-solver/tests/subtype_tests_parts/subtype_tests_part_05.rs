#[test]
fn test_constructor_contravariant_parameters() {
    // new (x: Base) => T <: new (x: Derived) => T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("result"),
        TypeId::BOOLEAN,
    )]);

    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let ctor_wide_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: string_or_number,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let ctor_narrow_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Constructor with wider param type is subtype (contravariance)
    assert!(checker.is_subtype_of(ctor_wide_param, ctor_narrow_param));
}
#[test]
fn test_constructor_optional_parameter() {
    // new (x?: string) => T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![]);

    let ctor_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let ctor_required = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Optional param constructor is wider (accepts more call patterns)
    assert!(checker.is_subtype_of(ctor_optional, ctor_required));
}
#[test]
fn test_constructor_rest_parameter() {
    // new (...args: string[]) => T
    let interner = TypeInterner::new();

    let instance = interner.object(vec![]);
    let string_array = interner.array(TypeId::STRING);

    let ctor_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(ctor_rest != TypeId::ERROR);
}
#[test]
fn test_constructor_overload_signatures() {
    // interface C { new (): A; new (x: string): B }
    let interner = TypeInterner::new();

    let instance_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let instance_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let overloaded_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: instance_a,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: instance_b,
                type_predicate: None,
                is_method: false,
            },
        ],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(overloaded_ctor != TypeId::ERROR);
}
#[test]
fn test_constructor_generic_type_param() {
    // new <T>() => T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let generic_ctor = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(generic_ctor != TypeId::ERROR);
}
#[test]
fn test_constructor_generic_with_constraint() {
    // new <T extends object>() => T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::OBJECT),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let constrained_ctor = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(constrained_ctor != TypeId::ERROR);
}
#[test]
fn test_constructor_abstract_pattern() {
    // abstract new () => T (abstract constructor)
    // Represented as a construct signature that can't be directly called
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    // Abstract constructor (conceptually - just a construct signature)
    let abstract_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Concrete constructor
    let concrete_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Both should be valid
    assert!(abstract_ctor != TypeId::ERROR);
    assert!(concrete_ctor != TypeId::ERROR);
}
#[test]
fn test_constructor_with_static_properties() {
    // Constructor function with static members
    let interner = TypeInterner::new();

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);

    let ctor_with_static = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![PropertyInfo {
            name: interner.intern_string("create"),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: instance,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            write_type: TypeId::NEVER,
            optional: false,
            readonly: true,
            is_method: true,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    assert!(ctor_with_static != TypeId::ERROR);
}
#[test]
fn test_constructor_instance_type_extraction() {
    // InstanceType<typeof C> pattern
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // The return type of the constructor IS the instance type
    // This simulates what InstanceType<> would extract
    assert!(ctor != TypeId::ERROR);
    assert!(checker.is_subtype_of(instance, instance));
}
#[test]
fn test_constructor_parameters_extraction() {
    // ConstructorParameters<typeof C> pattern
    let interner = TypeInterner::new();

    let instance = interner.object(vec![]);

    let ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("age")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Constructor parameters would be [string, number]
    let params_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(interner.intern_string("name")),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(interner.intern_string("age")),
            optional: false,
            rest: false,
        },
    ]);

    assert!(ctor != TypeId::ERROR);
    assert!(params_tuple != TypeId::ERROR);
}
#[test]
fn test_constructor_reflexive() {
    // C <: C
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(checker.is_subtype_of(ctor, ctor));
}
#[test]
fn test_constructor_never_return() {
    // new () => never (throws)
    let interner = TypeInterner::new();

    let throwing_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NEVER,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(throwing_ctor != TypeId::ERROR);
}
#[test]
fn test_constructor_any_return() {
    // new () => any
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let ctor_any = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let ctor_specific = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // any return is assignable to/from specific (any is bivariant)
    assert!(checker.is_subtype_of(ctor_any, ctor_specific));
}
#[test]
fn test_constructor_multiple_construct_signatures_subtype() {
    // Subtyping between callables with construct signatures
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let single_sig = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let double_sig = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: instance,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: instance,
                type_predicate: None,
                is_method: false,
            },
        ],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Double signature is more specific (has additional overload)
    // The single signature should match one of the overloads
    assert!(checker.is_subtype_of(single_sig, double_sig));
}
#[test]
fn test_constructor_with_this_type() {
    // new (this: Window) => T
    let interner = TypeInterner::new();

    let window_type = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("document"),
        TypeId::OBJECT,
    )]);

    let instance = interner.object(vec![]);

    let ctor_with_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(window_type),
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(ctor_with_this != TypeId::ERROR);
}
#[test]
fn test_constructor_empty_vs_nonempty() {
    // new () => {} vs new () => { x: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty_instance = interner.object(vec![]);
    let nonempty_instance = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let ctor_empty = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: empty_instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let ctor_nonempty = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: nonempty_instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // nonempty is subtype of empty (structural typing)
    assert!(checker.is_subtype_of(ctor_nonempty, ctor_empty));
    // empty is NOT subtype of nonempty
    assert!(!checker.is_subtype_of(ctor_empty, ctor_nonempty));
}

// ============================================================================
// This type tests (this in classes, fluent interfaces)
// ============================================================================
#[test]
fn test_this_type_basic() {
    // Basic polymorphic this type
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    // this type should be valid
    assert!(this_type != TypeId::ERROR);
    assert!(this_type != TypeId::NEVER);
}
#[test]
fn test_this_type_in_method_return() {
    // Method returning this for fluent interface
    // method(): this
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    let fluent_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo::method(
        interner.intern_string("setName"),
        fluent_method,
    )]);

    assert!(obj != TypeId::ERROR);
}
#[test]
fn test_this_type_fluent_builder() {
    // Builder pattern with multiple fluent methods
    // { setName(name: string): this, setValue(value: number): this, build(): Result }
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let result_type = interner.lazy(DefId(100));

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
        is_method: false,
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
        is_method: false,
    });

    let build = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: result_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let builder = interner.object(vec![
        PropertyInfo::method(interner.intern_string("setName"), set_name),
        PropertyInfo::method(interner.intern_string("setValue"), set_value),
        PropertyInfo::method(interner.intern_string("build"), build),
    ]);

    assert!(builder != TypeId::ERROR);
}
#[test]
fn test_this_type_with_explicit_this_parameter() {
    // Method with explicit this parameter
    // method(this: MyClass): void
    let interner = TypeInterner::new();

    let my_class = interner.lazy(DefId(1));

    let method_with_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(my_class),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(method_with_this != TypeId::ERROR);
}
#[test]
fn test_this_type_with_this_constraint() {
    // Method with constrained this
    // method<T extends MyClass>(this: T): T
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(interner.lazy(DefId(1))),
        default: None,
        is_const: false,
    }));

    let constrained_method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(interner.lazy(DefId(1))),
            default: None,
            is_const: false,
        }],
        params: vec![],
        this_type: Some(t_param),
        return_type: t_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(constrained_method != TypeId::ERROR);
}
#[test]
fn test_this_type_in_callback() {
    // Callback with this type
    // callback: (this: Context) => void
    let interner = TypeInterner::new();

    let context_type = interner.lazy(DefId(1));

    let callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(context_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("onClick"),
        callback,
    )]);

    assert!(obj != TypeId::ERROR);
}
#[test]
fn test_this_type_subtype_check() {
    // this type subtype relationships
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let this_type = interner.intern(TypeData::ThisType);

    // this is subtype of unknown
    assert!(checker.is_subtype_of(this_type, TypeId::UNKNOWN));

    // this is not subtype of never (unless it IS never)
    // this should not be subtype of specific types without context
}
#[test]
fn test_this_type_in_class_method() {
    // Class with method returning this
    // class Chainable { chain(): this }
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    let chain_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let chainable = interner.object(vec![PropertyInfo::method(
        interner.intern_string("chain"),
        chain_method,
    )]);

    assert!(chainable != TypeId::ERROR);
}
#[test]
fn test_this_type_with_generic_method() {
    // Generic method with this return
    // method<T>(value: T): this
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let t_ref = interner.lazy(DefId(50));

    let generic_fluent = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: t_ref,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(generic_fluent != TypeId::ERROR);
}
#[test]
fn test_this_type_with_property_access() {
    // Object with property of type this
    // { self: this }
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("self"),
        this_type,
    )]);

    assert!(obj != TypeId::ERROR);
}
#[test]
fn test_this_type_array() {
    // Array of this type
    // this[]
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let this_array = interner.array(this_type);

    assert!(this_array != TypeId::ERROR);
}
#[test]
fn test_this_type_in_union() {
    // this | null
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let nullable_this = interner.union(vec![this_type, TypeId::NULL]);

    assert!(nullable_this != TypeId::ERROR);
}
#[test]
fn test_this_type_in_intersection() {
    // this & HasId
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let has_id = interner.object(vec![PropertyInfo::new(
        interner.intern_string("id"),
        TypeId::STRING,
    )]);

    let this_with_id = interner.intersection(vec![this_type, has_id]);

    assert!(this_with_id != TypeId::ERROR);
}
#[test]
fn test_this_type_clone_method() {
    // clone(): this pattern
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    let clone_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cloneable = interner.object(vec![PropertyInfo::method(
        interner.intern_string("clone"),
        clone_method,
    )]);

    assert!(cloneable != TypeId::ERROR);
}
#[test]
fn test_this_type_with_optional_chaining() {
    // Method returning this | undefined for optional operation
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let optional_this = interner.union(vec![this_type, TypeId::UNDEFINED]);

    let optional_chain = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: optional_this,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(optional_chain != TypeId::ERROR);
}
#[test]
fn test_this_type_with_promise() {
    // Async method returning Promise<this>
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let promise_this = interner.application(interner.lazy(DefId(100)), vec![this_type]);

    let async_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: promise_this,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(async_method != TypeId::ERROR);
}
#[test]
fn test_this_type_in_tuple() {
    // [this, number] tuple
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let tuple_with_this = interner.tuple(vec![
        TupleElement {
            type_id: this_type,
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

    assert!(tuple_with_this != TypeId::ERROR);
}
#[test]
fn test_this_type_map_method() {
    // map<U>(fn: (value: this) => U): U
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let u_ref = interner.lazy(DefId(50));

    let mapper_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: this_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_ref,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let map_method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("fn")),
            type_id: mapper_fn,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_ref,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(map_method != TypeId::ERROR);
}
#[test]
fn test_this_type_with_readonly() {
    // Readonly<this>
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    // Simulated Readonly<this> as application
    let readonly_this = interner.application(interner.lazy(DefId(100)), vec![this_type]);

    assert!(readonly_this != TypeId::ERROR);
}
#[test]
fn test_this_type_partial() {
    // Partial<this>
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    let partial_this = interner.application(interner.lazy(DefId(101)), vec![this_type]);

    assert!(partial_this != TypeId::ERROR);
}
#[test]
fn test_this_type_with_keyof() {
    // keyof this
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let keyof_this = interner.intern(TypeData::KeyOf(this_type));

    assert!(keyof_this != TypeId::ERROR);
}
#[test]
fn test_this_type_indexed_access() {
    // this[K] indexed access
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let k_ref = interner.lazy(DefId(50));

    let indexed = interner.intern(TypeData::IndexAccess(this_type, k_ref));

    assert!(indexed != TypeId::ERROR);
}
#[test]
fn test_this_type_with_extends() {
    // this extends SomeInterface ? A : B
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let some_interface = interner.lazy(DefId(1));

    let cond = ConditionalType {
        check_type: this_type,
        extends_type: some_interface,
        true_type: TypeId::STRING,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    };

    let conditional = interner.conditional(cond);

    assert!(conditional != TypeId::ERROR);
}
#[test]
fn test_this_type_method_decorator_pattern() {
    // Decorator that preserves this type
    // <T extends (...args: any[]) => any>(method: T): T
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    // Method that takes this as explicit parameter
    let decorated = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(this_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(decorated != TypeId::ERROR);
}
#[test]
fn test_this_type_static_vs_instance() {
    // Static method doesn't use this, instance method does
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    // Static method - no this
    let static_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Instance method - returns this
    let instance_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let class_type = interner.object(vec![
        PropertyInfo::method(interner.intern_string("staticMethod"), static_method),
        PropertyInfo::method(interner.intern_string("instanceMethod"), instance_method),
    ]);

    assert!(class_type != TypeId::ERROR);
}
#[test]
fn test_this_type_with_getter_setter() {
    // Getter returns this, setter takes value
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    // Simulating getter: get prop(): this
    let getter = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("current"),
        type_id: getter,
        write_type: this_type,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert!(obj != TypeId::ERROR);
}
#[test]
fn test_this_type_with_rest_params() {
    // method(...args: Parameters<this["method"]>): this
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    // Simplified: method with rest params returning this
    let rest_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: interner.array(TypeId::ANY),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(rest_method != TypeId::ERROR);
}
#[test]
fn test_this_type_comparison() {
    // Two this types should be equal
    let interner = TypeInterner::new();

    let this1 = interner.intern(TypeData::ThisType);
    let this2 = interner.intern(TypeData::ThisType);

    // Same interned type
    assert_eq!(this1, this2);
}
#[test]
fn test_this_type_with_method_overload() {
    // Overloaded methods all returning this
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    let overload1 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let overload2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Union of overloads
    let overloaded = interner.intersection(vec![overload1, overload2]);

    assert!(overloaded != TypeId::ERROR);
}
#[test]
fn test_this_type_event_emitter_pattern() {
    // on(event: string, handler: Function): this
    // off(event: string, handler: Function): this
    // emit(event: string, ...args: any[]): this
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);

    let on_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("event")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("handler")),
                type_id: TypeId::FUNCTION,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let off_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("event")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("handler")),
                type_id: TypeId::FUNCTION,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let emit_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("event")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: interner.array(TypeId::ANY),
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let emitter = interner.object(vec![
        PropertyInfo::method(interner.intern_string("on"), on_method),
        PropertyInfo::method(interner.intern_string("off"), off_method),
        PropertyInfo::method(interner.intern_string("emit"), emit_method),
    ]);

    assert!(emitter != TypeId::ERROR);
}
#[test]
fn test_this_type_query_builder() {
    // Query builder with chainable methods
    // where(condition: string): this
    // orderBy(field: string): this
    // limit(n: number): this
    // execute(): Promise<Result[]>
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeData::ThisType);
    let result_array = interner.array(interner.lazy(DefId(100)));
    let promise_results = interner.application(interner.lazy(DefId(101)), vec![result_array]);

    let where_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("condition")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let order_by_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("field")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let limit_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("n")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let execute_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: promise_results,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let query_builder = interner.object(vec![
        PropertyInfo::method(interner.intern_string("where"), where_method),
        PropertyInfo::method(interner.intern_string("orderBy"), order_by_method),
        PropertyInfo::method(interner.intern_string("limit"), limit_method),
        PropertyInfo::method(interner.intern_string("execute"), execute_method),
    ]);

    assert!(query_builder != TypeId::ERROR);
}

// ============================================================================
// Readonly property tests (readonly modifiers, Readonly<T>)
// ============================================================================
#[test]
fn test_readonly_property_basic() {
    // { readonly x: string }
    let interner = TypeInterner::new();

    let readonly_obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    assert!(readonly_obj != TypeId::ERROR);
}
#[test]
fn test_readonly_vs_mutable_property() {
    // { readonly x: string } vs { x: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let readonly_obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    let mutable_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    // Mutable is subtype of readonly (can assign mutable to readonly)
    assert!(checker.is_subtype_of(mutable_obj, readonly_obj));

    // TypeScript allows readonly property → mutable property assignment
    assert!(checker.is_subtype_of(readonly_obj, mutable_obj));
}
#[test]
fn test_readonly_array_basic() {
    // readonly string[]
    let interner = TypeInterner::new();

    let readonly_array = interner.readonly_array(TypeId::STRING);

    assert!(readonly_array != TypeId::ERROR);
}
#[test]
fn test_readonly_array_vs_mutable() {
    // readonly string[] vs string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let readonly_array = interner.readonly_array(TypeId::STRING);
    let mutable_array = interner.array(TypeId::STRING);

    // Mutable array is subtype of readonly array
    assert!(checker.is_subtype_of(mutable_array, readonly_array));

    // Readonly array is NOT subtype of mutable array
    assert!(!checker.is_subtype_of(readonly_array, mutable_array));
}
#[test]
fn test_readonly_tuple_basic() {
    // readonly [string, number]
    let interner = TypeInterner::new();

    let readonly_tuple = interner.readonly_tuple(vec![
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

    assert!(readonly_tuple != TypeId::ERROR);
}
#[test]
fn test_readonly_tuple_vs_mutable() {
    // readonly [string, number] vs [string, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let readonly_tuple = interner.readonly_tuple(vec![
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
    let mutable_tuple = interner.tuple(vec![
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

    // Mutable tuple is subtype of readonly tuple
    assert!(checker.is_subtype_of(mutable_tuple, readonly_tuple));

    // Readonly tuple is NOT subtype of mutable tuple
    assert!(!checker.is_subtype_of(readonly_tuple, mutable_tuple));
}
#[test]
fn test_readonly_multiple_properties() {
    // { readonly a: string, readonly b: number }
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    assert!(obj != TypeId::ERROR);
}
#[test]
fn test_readonly_mixed_with_mutable() {
    // { readonly a: string, b: number }
    let interner = TypeInterner::new();

    let mixed = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    assert!(mixed != TypeId::ERROR);
}
#[test]
fn test_readonly_index_signature() {
    // { readonly [key: string]: number }
    let interner = TypeInterner::new();

    let readonly_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(readonly_index != TypeId::ERROR);
}
#[test]
fn test_readonly_index_vs_mutable() {
    // { readonly [key: string]: number } vs { [key: string]: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let readonly_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: None,
    });

    let mutable_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Mutable index is subtype of readonly index
    assert!(checker.is_subtype_of(mutable_index, readonly_index));

    // Per tsc behavior, readonly on index signatures does NOT affect assignability.
    // A readonly index signature IS assignable to a mutable index signature.
    assert!(checker.is_subtype_of(readonly_index, mutable_index));
}
#[test]
fn test_readonly_optional_property() {
    // { readonly x?: string }
    let interner = TypeInterner::new();

    let readonly_optional = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: true,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert!(readonly_optional != TypeId::ERROR);
}
#[test]
fn test_readonly_nested_object() {
    // { readonly data: { readonly inner: string } }
    let interner = TypeInterner::new();

    let inner = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("inner"),
        TypeId::STRING,
    )]);

    let outer = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("data"),
        inner,
    )]);

    assert!(outer != TypeId::ERROR);
}
#[test]
fn test_readonly_with_union_property() {
    // { readonly x: string | number }
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        union,
    )]);

    assert!(obj != TypeId::ERROR);
}
#[test]
fn test_readonly_with_array_property() {
    // { readonly items: string[] }
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("items"),
        string_array,
    )]);

    assert!(obj != TypeId::ERROR);
}
#[test]
fn test_readonly_deep_with_array() {
    // { readonly items: readonly string[] }
    let interner = TypeInterner::new();

    let readonly_array = interner.readonly_array(TypeId::STRING);

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("items"),
        readonly_array,
    )]);

    assert!(obj != TypeId::ERROR);
}
#[test]
fn test_readonly_with_function_property() {
    // { readonly callback: () => void }
    let interner = TypeInterner::new();

    let callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("callback"),
        callback,
    )]);

    assert!(obj != TypeId::ERROR);
}
#[test]
fn test_readonly_method_is_always_readonly() {
    // Methods are inherently readonly
    let interner = TypeInterner::new();

    let method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("getValue"),
        type_id: method,
        write_type: method,
        optional: false,
        readonly: false, // Methods can be defined non-readonly
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert!(obj != TypeId::ERROR);
}
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
// OVERLOAD RESOLUTION TESTS
// ============================================================================
// Tests for function overloads, generic overloads, and overload subtyping
#[test]
fn test_overload_basic_two_signatures() {
    // interface Overloaded {
    //   (x: string): number;
    //   (x: number): string;
    // }
    let interner = TypeInterner::new();

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
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
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}
#[test]
fn test_overload_by_argument_count() {
    // interface ByCount {
    //   (): void;
    //   (x: number): number;
    //   (x: number, y: number): number;
    // }
    let interner = TypeInterner::new();

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("x")),
                        type_id: TypeId::NUMBER,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("y")),
                        type_id: TypeId::NUMBER,
                        optional: false,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}
#[test]
fn test_overload_subtype_more_signatures_to_fewer() {
    // More overloads is subtype of fewer (if matching signatures exist)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Two signatures: (string) => number, (number) => string
    let more_overloads = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
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
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // One signature: (string) => number
    let fewer_overloads = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // More overloads should be subtype of fewer (can be used anywhere fewer is expected)
    assert!(checker.is_subtype_of(more_overloads, fewer_overloads));
}
#[test]
fn test_overload_subtype_fewer_not_subtype_of_more() {
    // Fewer overloads is NOT subtype of more (missing capability)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Two signatures
    let more_overloads = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
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
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // One signature only
    let fewer_overloads = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Fewer cannot substitute for more - missing the (number) => string overload
    assert!(!checker.is_subtype_of(fewer_overloads, more_overloads));
}
#[test]
fn test_overload_generic_identity() {
    // interface GenericOverload {
    //   <T>(x: T): T;
    //   (x: string): string;
    // }
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("T"),
                    constraint: None,
                    default: None,
                    is_const: false,
                }],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: t_param,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: t_param,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}
#[test]
fn test_overload_generic_with_constraint() {
    // interface ConstrainedOverload {
    //   <T extends string>(x: T): T;
    //   <T extends number>(x: T): T;
    // }
    let interner = TypeInterner::new();

    let t_string = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    let t_number = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
    }));

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("T"),
                    constraint: Some(TypeId::STRING),
                    default: None,
                    is_const: false,
                }],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: t_string,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: t_string,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("T"),
                    constraint: Some(TypeId::NUMBER),
                    default: None,
                    is_const: false,
                }],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: t_number,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: t_number,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}
#[test]
fn test_overload_with_rest_parameter() {
    // interface WithRest {
    //   (x: number): number;
    //   (...args: number[]): number;
    // }
    let interner = TypeInterner::new();

    let number_array = interner.array(TypeId::NUMBER);

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("args")),
                    type_id: number_array,
                    optional: false,
                    rest: true,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}
#[test]
fn test_overload_with_optional_parameters() {
    // interface WithOptional {
    //   (x: string): string;
    //   (x: string, y?: number): string;
    // }
    let interner = TypeInterner::new();

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("x")),
                        type_id: TypeId::STRING,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("y")),
                        type_id: TypeId::NUMBER,
                        optional: true,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}
#[test]
fn test_overload_mixed_call_and_construct() {
    // interface MixedCallable {
    //   (x: string): string;
    //   new (x: number): object;
    // }
    let interner = TypeInterner::new();

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::OBJECT,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}
#[test]
fn test_overload_return_type_union() {
    // interface UnionReturn {
    //   (x: "a"): number;
    //   (x: "b"): string;
    //   (x: string): number | string;
    // }
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let num_or_string = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: lit_a,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: lit_b,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: num_or_string,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}
#[test]
fn test_overload_subtype_signature_order_matters() {
    // Overload signature order should be preserved for resolution
    let interner = TypeInterner::new();

    let lit_a = interner.literal_string("a");

    // Order: specific first, then general
    let specific_first = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: lit_a,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Order: general first, then specific
    let general_first = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: lit_a,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // These should be different types due to signature order
    assert!(specific_first != general_first);
}
#[test]
fn test_overload_generic_multiple_type_params() {
    // interface MultiGeneric {
    //   <T, U>(x: T, y: U): [T, U];
    //   <T>(x: T): T;
    // }
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let tuple_t_u = interner.tuple(vec![
        TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: u_param,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![
                    TypeParamInfo {
                        name: interner.intern_string("T"),
                        constraint: None,
                        default: None,
                        is_const: false,
                    },
                    TypeParamInfo {
                        name: interner.intern_string("U"),
                        constraint: None,
                        default: None,
                        is_const: false,
                    },
                ],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("x")),
                        type_id: t_param,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("y")),
                        type_id: u_param,
                        optional: false,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: tuple_t_u,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("T"),
                    constraint: None,
                    default: None,
                    is_const: false,
                }],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: t_param,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: t_param,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}
#[test]
fn test_overload_reflexivity() {
    // Same overloaded callable should be subtype of itself
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
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
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(checker.is_subtype_of(callable, callable));
}
#[test]
fn test_overload_covariant_return_types() {
    // Overload with more specific return type should be subtype
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_hello = interner.literal_string("hello");

    // Returns literal "hello"
    let specific_return = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: lit_hello,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Returns string
    let general_return = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
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
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // More specific return is subtype (covariance)
    assert!(checker.is_subtype_of(specific_return, general_return));
    assert!(!checker.is_subtype_of(general_return, specific_return));
}
#[test]
fn test_overload_contravariant_parameters() {
    // Overload with less specific parameter should be subtype
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_hello = interner.literal_string("hello");

    // Accepts any string
    let general_param = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
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
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Accepts only "hello"
    let specific_param = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: lit_hello,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // More general param is subtype (contravariance)
    assert!(checker.is_subtype_of(general_param, specific_param));
    assert!(!checker.is_subtype_of(specific_param, general_param));
}
#[test]
fn test_overload_construct_signature_subtyping() {
    // Constructor overload subtyping
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_with_x = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let obj_with_xy = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);

    // Returns {x, y}
    let specific_constructor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: obj_with_xy,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Returns {x}
    let general_constructor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: obj_with_x,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // More specific instance type is subtype
    assert!(checker.is_subtype_of(specific_constructor, general_constructor));
}
#[test]
fn test_overload_with_this_type() {
    // interface WithThis {
    //   (this: Window, x: string): void;
    //   (this: Document, x: number): void;
    // }
    let interner = TypeInterner::new();

    let window_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("location"),
        TypeId::STRING,
    )]);

    let document_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("body"),
        TypeId::OBJECT,
    )]);

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: Some(window_type),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: Some(document_type),
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}
#[test]
fn test_overload_empty_callable() {
    // Empty callable (no call or construct signatures)
    let interner = TypeInterner::new();

    let empty_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    assert!(empty_callable != TypeId::ERROR);
}
#[test]
fn test_overload_with_properties() {
    // interface CallableWithProps {
    //   (x: string): number;
    //   name: string;
    //   version: number;
    // }
    let interner = TypeInterner::new();

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![
            PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("version"), TypeId::NUMBER),
        ],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}
#[test]
fn test_overload_generic_default_type() {
    // interface WithDefault {
    //   <T = string>(x: T): T;
    // }
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: Some(TypeId::STRING),
        is_const: false,
    }));

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![TypeParamInfo {
                name: interner.intern_string("T"),
                constraint: None,
                default: Some(TypeId::STRING),
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_param,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(callable != TypeId::ERROR);
}
#[test]
fn test_overload_array_methods_pattern() {
    // Array-like overloads pattern:
    // interface ArrayLike<T> {
    //   map<U>(fn: (x: T) => U): U[];
    //   filter(fn: (x: T) => boolean): T[];
    //   reduce<U>(fn: (acc: U, x: T) => U, init: U): U;
    // }
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // (x: T) => U
    let map_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (x: T) => boolean
    let filter_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (acc: U, x: T) => U
    let reduce_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("acc")),
                type_id: u_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: u_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let u_array = interner.array(u_param);
    let t_array = interner.array(t_param);

    // map<U>(fn: (x: T) => U): U[]
    let map_method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("fn")),
            type_id: map_callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_array,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // filter(fn: (x: T) => boolean): T[]
    let filter_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("fn")),
            type_id: filter_callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_array,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // reduce<U>(fn: (acc: U, x: T) => U, init: U): U
    let reduce_method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("fn")),
                type_id: reduce_callback,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("init")),
                type_id: u_param,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: u_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let array_like = interner.object(vec![
        PropertyInfo::method(interner.intern_string("map"), map_method),
        PropertyInfo::method(interner.intern_string("filter"), filter_method),
        PropertyInfo::method(interner.intern_string("reduce"), reduce_method),
    ]);

    assert!(array_like != TypeId::ERROR);
}
#[test]
fn test_overload_event_handler_pattern() {
    // DOM-style event handler overloads:
    // interface EventTarget {
    //   addEventListener(type: "click", listener: (e: MouseEvent) => void): void;
    //   addEventListener(type: "keydown", listener: (e: KeyboardEvent) => void): void;
    //   addEventListener(type: string, listener: (e: Event) => void): void;
    // }
    let interner = TypeInterner::new();

    let lit_click = interner.literal_string("click");
    let lit_keydown = interner.literal_string("keydown");

    let mouse_event = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("type"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("clientX"), TypeId::NUMBER),
        PropertyInfo::readonly(interner.intern_string("clientY"), TypeId::NUMBER),
    ]);

    let keyboard_event = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("type"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("key"), TypeId::STRING),
        PropertyInfo::readonly(interner.intern_string("code"), TypeId::STRING),
    ]);

    let base_event = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("type"),
        TypeId::STRING,
    )]);

    // (e: MouseEvent) => void
    let mouse_listener = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
            type_id: mouse_event,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (e: KeyboardEvent) => void
    let keyboard_listener = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
            type_id: keyboard_event,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (e: Event) => void
    let base_listener = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
            type_id: base_event,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let add_event_listener = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("type")),
                        type_id: lit_click,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("listener")),
                        type_id: mouse_listener,
                        optional: false,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("type")),
                        type_id: lit_keydown,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("listener")),
                        type_id: keyboard_listener,
                        optional: false,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("type")),
                        type_id: TypeId::STRING,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("listener")),
                        type_id: base_listener,
                        optional: false,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let event_target = interner.object(vec![PropertyInfo::method(
        interner.intern_string("addEventListener"),
        add_event_listener,
    )]);

    assert!(event_target != TypeId::ERROR);
}
#[test]
fn test_overload_promise_then_pattern() {
    // Promise.then overloads:
    // interface Promise<T> {
    //   then<U>(onFulfilled: (value: T) => U): Promise<U>;
    //   then<U>(onFulfilled: (value: T) => Promise<U>): Promise<U>;
    //   then<U, V>(onFulfilled: (value: T) => U, onRejected: (reason: any) => V): Promise<U | V>;
    // }
    let interner = TypeInterner::new();

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    let v_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: None,
        default: None,
        is_const: false,
    }));

    // (value: T) => U
    let on_fulfilled_sync = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (reason: any) => V
    let on_rejected = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("reason")),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: v_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let u_or_v = interner.union(vec![u_param, v_param]);

    let then_method = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            // then<U>(onFulfilled: (value: T) => U): Promise<U>
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("U"),
                    constraint: None,
                    default: None,
                    is_const: false,
                }],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("onFulfilled")),
                    type_id: on_fulfilled_sync,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                // Would be Promise<U> but simplified here
                return_type: u_param,
                type_predicate: None,
                is_method: false,
            },
            // then<U, V>(onFulfilled, onRejected): Promise<U | V>
            CallSignature {
                type_params: vec![
                    TypeParamInfo {
                        name: interner.intern_string("U"),
                        constraint: None,
                        default: None,
                        is_const: false,
                    },
                    TypeParamInfo {
                        name: interner.intern_string("V"),
                        constraint: None,
                        default: None,
                        is_const: false,
                    },
                ],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("onFulfilled")),
                        type_id: on_fulfilled_sync,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("onRejected")),
                        type_id: on_rejected,
                        optional: false,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: u_or_v,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(then_method != TypeId::ERROR);
}
#[test]
fn test_overload_constructor_overloads() {
    // interface DateConstructor {
    //   new (): Date;
    //   new (value: number): Date;
    //   new (value: string): Date;
    //   new (year: number, month: number, date?: number): Date;
    // }
    let interner = TypeInterner::new();

    let date_instance = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("getTime"),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            write_type: TypeId::NEVER,
            optional: false,
            readonly: true,
            is_method: true,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: interner.intern_string("toISOString"),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            write_type: TypeId::NEVER,
            optional: false,
            readonly: true,
            is_method: true,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ]);

    let date_constructor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![
            // new (): Date
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: date_instance,
                type_predicate: None,
                is_method: false,
            },
            // new (value: number): Date
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: date_instance,
                type_predicate: None,
                is_method: false,
            },
            // new (value: string): Date
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: date_instance,
                type_predicate: None,
                is_method: false,
            },
            // new (year: number, month: number, date?: number): Date
            CallSignature {
                type_params: vec![],
                params: vec![
                    ParamInfo {
                        name: Some(interner.intern_string("year")),
                        type_id: TypeId::NUMBER,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("month")),
                        type_id: TypeId::NUMBER,
                        optional: false,
                        rest: false,
                    },
                    ParamInfo {
                        name: Some(interner.intern_string("date")),
                        type_id: TypeId::NUMBER,
                        optional: true,
                        rest: false,
                    },
                ],
                this_type: None,
                return_type: date_instance,
                type_predicate: None,
                is_method: false,
            },
        ],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    assert!(date_constructor != TypeId::ERROR);
}

// =============================================================================
// TS2322 Detection Improvement Tests
// =============================================================================
#[test]
fn test_explain_failure_intrinsic_mismatch() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // string vs number should produce IntrinsicTypeMismatch
    let reason = checker.explain_failure(TypeId::STRING, TypeId::NUMBER);
    assert!(reason.is_some());
    match reason.unwrap() {
        SubtypeFailureReason::IntrinsicTypeMismatch {
            source_type,
            target_type,
        } => {
            assert_eq!(source_type, TypeId::STRING);
            assert_eq!(target_type, TypeId::NUMBER);
        }
        other => panic!("Expected IntrinsicTypeMismatch, got {other:?}"),
    }
}
#[test]
fn test_explain_failure_literal_mismatch() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // "hello" vs "world" should produce LiteralTypeMismatch
    let reason = checker.explain_failure(hello, world);
    assert!(reason.is_some());
    match reason.unwrap() {
        SubtypeFailureReason::LiteralTypeMismatch {
            source_type,
            target_type,
        } => {
            assert_eq!(source_type, hello);
            assert_eq!(target_type, world);
        }
        other => panic!("Expected LiteralTypeMismatch, got {other:?}"),
    }
}
#[test]
fn test_explain_failure_literal_to_incompatible_intrinsic() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");

    // "hello" vs number should produce LiteralTypeMismatch
    let reason = checker.explain_failure(hello, TypeId::NUMBER);
    assert!(reason.is_some());
    match reason.unwrap() {
        SubtypeFailureReason::LiteralTypeMismatch {
            source_type,
            target_type,
        } => {
            assert_eq!(source_type, hello);
            assert_eq!(target_type, TypeId::NUMBER);
        }
        other => panic!("Expected LiteralTypeMismatch, got {other:?}"),
    }
}
#[test]
fn test_explain_failure_error_type() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // ERROR type should produce ErrorType failure reason, not None
    let reason = checker.explain_failure(TypeId::ERROR, TypeId::NUMBER);
    assert!(
        reason.is_some(),
        "ERROR type should produce a failure reason"
    );
    match reason.unwrap() {
        SubtypeFailureReason::ErrorType {
            source_type,
            target_type,
        } => {
            assert_eq!(source_type, TypeId::ERROR);
            assert_eq!(target_type, TypeId::NUMBER);
        }
        other => panic!("Expected ErrorType, got {other:?}"),
    }
}
#[test]
fn test_literal_number_to_string_fails() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let forty_two = interner.literal_number(42.0);

    // 42 vs string should fail
    assert!(!checker.is_subtype_of(forty_two, TypeId::STRING));

    // And produce a proper failure reason
    let reason = checker.explain_failure(forty_two, TypeId::STRING);
    assert!(reason.is_some());
    match reason.unwrap() {
        SubtypeFailureReason::LiteralTypeMismatch { .. } => {}
        other => panic!("Expected LiteralTypeMismatch, got {other:?}"),
    }
}
#[test]
fn test_intrinsic_to_literal_fails() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");

    // string vs "hello" should fail (widening is not allowed)
    assert!(!checker.is_subtype_of(TypeId::STRING, hello));

    // And produce a proper failure reason
    let reason = checker.explain_failure(TypeId::STRING, hello);
    assert!(reason.is_some());
    match reason.unwrap() {
        SubtypeFailureReason::TypeMismatch { .. } => {}
        other => panic!("Expected TypeMismatch, got {other:?}"),
    }
}

// ============================================================================
// Explain failure: mapped type evaluation in the explain path
// These tests verify that mapped types are evaluated to concrete object types
// during explain_failure, enabling property-level diagnostics (TS2739/TS2741).
// ============================================================================
#[test]
fn test_explain_failure_mapped_type_target_missing_property() {
    // Simulates: Required<{ a?: string, b: number }> as target
    // with source { b: number } (missing 'a').
    // The mapped type (with -? modifier) should be evaluated to a concrete
    // object { a: string, b: number } so explain_failure can detect the
    // missing property and return MissingProperty instead of TypeMismatch.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    // Build the source object: { a?: string, b: number }
    let source_obj = interner.object(vec![
        PropertyInfo::opt(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    // Build Required<source_obj> as a mapped type: { [K in keyof T]-?: T[K] }
    let keyof_source = interner.keyof(source_obj);
    let k_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_param = interner.intern(TypeData::TypeParameter(k_param_info));
    let template = interner.index_access(source_obj, k_param);
    let required_target = interner.mapped(MappedType {
        type_param: k_param_info,
        constraint: keyof_source,
        name_type: None,
        template,
        optional_modifier: Some(MappedModifier::Remove),
        readonly_modifier: None,
    });

    // Source is missing property 'a': { b: number }
    let incomplete_source = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    // Verify the assignment fails
    assert!(
        !checker.is_subtype_of(incomplete_source, required_target),
        "{{b: number}} should not be assignable to Required<{{a?: string, b: number}}>"
    );

    // explain_failure should return MissingProperty (TS2741), not TypeMismatch (TS2322)
    let reason = checker.explain_failure(incomplete_source, required_target);
    assert!(reason.is_some(), "Should produce a failure reason");
    match reason.unwrap() {
        SubtypeFailureReason::MissingProperty { property_name, .. } => {
            assert_eq!(property_name, a_name, "Missing property should be 'a'");
        }
        SubtypeFailureReason::MissingProperties { .. } => {
            // Also acceptable — depends on how many properties are missing
        }
        other => panic!(
            "Expected MissingProperty or MissingProperties for mapped type target, got {other:?}"
        ),
    }
}
#[test]
fn test_explain_failure_mapped_type_source_evaluated() {
    // Verify that mapped type sources are also evaluated.
    // Source: Partial<{ a: string, b: number }> => { a?: string, b?: number }
    // Target: { a: string, b: number }
    // The source mapped type should evaluate to a concrete object so
    // explain_failure can detect the optional→required mismatch.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    // Build the concrete object { a: string, b: number }
    let concrete_obj = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    // Build Partial<concrete_obj> as a mapped type: { [K in keyof T]+?: T[K] }
    let keyof_obj = interner.keyof(concrete_obj);
    let k_param_info = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_param = interner.intern(TypeData::TypeParameter(k_param_info));
    let template = interner.index_access(concrete_obj, k_param);
    let partial_source = interner.mapped(MappedType {
        type_param: k_param_info,
        constraint: keyof_obj,
        name_type: None,
        template,
        optional_modifier: Some(MappedModifier::Add),
        readonly_modifier: None,
    });

    // Partial<T> → T should fail (properties may be missing)
    assert!(
        !checker.is_subtype_of(partial_source, concrete_obj),
        "Partial<{{a: string, b: number}}> should not be assignable to {{a: string, b: number}}"
    );

    // explain_failure should return a structured reason (not None)
    let reason = checker.explain_failure(partial_source, concrete_obj);
    assert!(
        reason.is_some(),
        "Partial<T> → T should produce a failure reason"
    );
    // The specific reason depends on how the solver handles optional→required mismatches.
    // The important thing is we get a structured reason, not a generic TypeMismatch from
    // failing to enumerate properties on an unevaluated mapped type.
}

// ============================================================================
// Tuple-to-Array Assignability Tests
// These tests document TypeScript behavior for assigning tuples to arrays
// ============================================================================

// --- Homogeneous Tuples to Arrays ---
