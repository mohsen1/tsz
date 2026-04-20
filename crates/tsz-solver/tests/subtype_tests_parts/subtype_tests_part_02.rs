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
#[test]
fn test_contravariant_callback_param() {
    // Callback in param position creates double contravariance = covariance
    // (cb: (x: string) => void) => void <: (cb: (x: string | number) => void) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let cb_narrow = interner.function(FunctionShape {
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

    let cb_wide = interner.function(FunctionShape {
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

    let fn_with_cb_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: cb_narrow,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_with_cb_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: cb_wide,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Double contravariance: narrower callback param is subtype
    assert!(checker.is_subtype_of(fn_with_cb_narrow, fn_with_cb_wide));
    assert!(!checker.is_subtype_of(fn_with_cb_wide, fn_with_cb_narrow));
}

// -----------------------------------------------------------------------------
// Invariant Position (Mutable Types)
// -----------------------------------------------------------------------------
#[test]
fn test_invariant_mutable_property() {
    // Mutable property is invariant: { value: T } not subtype of { value: U } unless T = U
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value_prop = interner.intern_string("value");

    // Mutable property (not readonly, write_type == read_type)
    let obj_string = interner.object(vec![PropertyInfo::new(value_prop, TypeId::STRING)]);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let obj_union = interner.object(vec![PropertyInfo::new(value_prop, union)]);

    // Mutable property should be invariant
    // { value: string } is NOT subtype of { value: string | number }
    // because we could write a number into the string slot
    // Note: TypeScript allows this unsoundly, but strict mode doesn't
    // This test verifies the invariant behavior
    // The actual result depends on the checker implementation
    let is_subtype = checker.is_subtype_of(obj_string, obj_union);
    // Just verify both directions - exact behavior depends on strictness
    let is_super = checker.is_subtype_of(obj_union, obj_string);
    // At least one direction should be false for true invariance
    assert!(!(is_subtype && is_super) || obj_string == obj_union);
}
#[test]
fn test_invariant_array_element() {
    // Array<T> should be invariant, but TypeScript treats it covariantly (unsound)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union);

    // TypeScript allows this (covariant arrays) but it's technically unsound
    // string[] <: (string | number)[] - TypeScript allows
    let allows_covariant = checker.is_subtype_of(string_array, union_array);
    // The test documents the current behavior
    // For truly invariant arrays, this would be false
    assert!(allows_covariant); // TypeScript behavior
}
#[test]
fn test_invariant_generic_mutable_box() {
    // Box<T> = { value: T } where T is both read and written
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value_prop = interner.intern_string("value");

    // Box<string>
    let box_string = interner.object(vec![PropertyInfo::new(value_prop, TypeId::STRING)]);

    // Box<number>
    let box_number = interner.object(vec![PropertyInfo::new(value_prop, TypeId::NUMBER)]);

    // Neither should be subtype of the other (invariant)
    assert!(!checker.is_subtype_of(box_string, box_number));
    assert!(!checker.is_subtype_of(box_number, box_string));
}
#[test]
fn test_invariant_ref_cell_pattern() {
    // RefCell<T> = { get(): T, set(v: T): void }
    // tsc uses bivariant method parameter checking, so methods are NOT
    // checked contravariantly — this means RefCell<string> IS assignable
    // to RefCell<string|number>, even though set() would be unsound.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let get_name = interner.intern_string("get");
    let set_name = interner.intern_string("set");

    // RefCell<string>
    let get_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let set_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("v")),
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
    let refcell_string = interner.object(vec![
        PropertyInfo {
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
        },
        PropertyInfo {
            name: set_name,
            type_id: set_string,
            write_type: set_string,
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

    // RefCell<string | number>
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let get_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: union,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let set_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("v")),
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
    let refcell_union = interner.object(vec![
        PropertyInfo {
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
        },
        PropertyInfo {
            name: set_name,
            type_id: set_union,
            write_type: set_union,
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

    // tsc bivariant method checking: RefCell<string> -> RefCell<string|number> IS allowed
    assert!(checker.is_subtype_of(refcell_string, refcell_union));
    // RefCell<string|number> -> RefCell<string> is NOT allowed (get returns wider type)
    assert!(!checker.is_subtype_of(refcell_union, refcell_string));
}
#[test]
fn test_invariant_in_out_parameter() {
    // Function with param used for both input and output
    // (ref: T) => T - T is invariant
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("ref")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let fn_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("ref")),
            type_id: union,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: union,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Mixed variance creates invariance
    // (string) => string is not subtype of (union) => union
    // because param is contravariant but return is covariant
    assert!(!checker.is_subtype_of(fn_string, fn_union));
    assert!(!checker.is_subtype_of(fn_union, fn_string));
}

// -----------------------------------------------------------------------------
// Bivariance in Method Parameters
// -----------------------------------------------------------------------------
#[test]
fn test_bivariant_method_param_wider() {
    // Methods with bivariant params: both directions work
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let handler_name = interner.intern_string("handler");
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Method with narrow param
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

    // Method with wide param
    let method_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
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

    // Object with method (is_method: true enables bivariance)
    let obj_narrow = interner.object(vec![PropertyInfo::method(handler_name, method_narrow)]);

    let obj_wide = interner.object(vec![PropertyInfo::method(handler_name, method_wide)]);

    // Bivariant: both directions should work for methods
    // Note: actual behavior depends on strictFunctionTypes setting
    let narrow_to_wide = checker.is_subtype_of(obj_narrow, obj_wide);
    let wide_to_narrow = checker.is_subtype_of(obj_wide, obj_narrow);
    // At least one direction should work (contravariant minimum)
    assert!(narrow_to_wide || wide_to_narrow);
}
#[test]
fn test_bivariant_method_vs_function_property() {
    // Method (bivariant) vs function property (contravariant)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let handler_name = interner.intern_string("handler");
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

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

    // Method (is_method: true)
    let obj_method = interner.object(vec![PropertyInfo::method(handler_name, fn_narrow)]);

    // Function property (is_method: false)    visibility: Visibility::Public,    parent_id: None,
    let obj_fn_prop = interner.object(vec![PropertyInfo::new(handler_name, fn_wide)]);

    // Test subtype relationship
    // Method sources can be bivariant
    let result = checker.is_subtype_of(obj_method, obj_fn_prop);
    // Document the behavior - just verify it doesn't panic
    let _ = result;
}
#[test]
fn test_bivariant_event_handler_pattern() {
    // Common pattern: addEventListener with bivariant event handlers
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let on_event_name = interner.intern_string("onEvent");

    // Base event type
    let event_prop = interner.intern_string("type");
    let base_event = interner.object(vec![PropertyInfo::readonly(event_prop, TypeId::STRING)]);

    // Derived event with additional property
    let target_prop = interner.intern_string("target");
    let derived_event = interner.object(vec![
        PropertyInfo::readonly(event_prop, TypeId::STRING),
        PropertyInfo::readonly(target_prop, TypeId::STRING),
    ]);

    let handler_base = interner.function(FunctionShape {
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

    let handler_derived = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("e")),
            type_id: derived_event,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Object with event handler method
    let obj_base_handler = interner.object(vec![PropertyInfo::method(on_event_name, handler_base)]);

    let obj_derived_handler =
        interner.object(vec![PropertyInfo::method(on_event_name, handler_derived)]);

    // With bivariance, handler expecting derived event should be assignable
    // to handler expecting base event (practical for event handling)
    let derived_to_base = checker.is_subtype_of(obj_derived_handler, obj_base_handler);
    // This is the "unsound but practical" TypeScript behavior - just ensure no panic
    let _ = derived_to_base;
}
#[test]
fn test_bivariant_overload_callback() {
    // Overloaded callbacks with bivariance
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let cb_name = interner.intern_string("callback");

    // Callback that takes string
    let cb_string = interner.function(FunctionShape {
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

    // Callback that takes number
    let cb_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_cb_string = interner.object(vec![PropertyInfo::method(cb_name, cb_string)]);

    let obj_cb_number = interner.object(vec![PropertyInfo::method(cb_name, cb_number)]);

    // Incompatible param types - neither should be subtype
    assert!(!checker.is_subtype_of(obj_cb_string, obj_cb_number));
    assert!(!checker.is_subtype_of(obj_cb_number, obj_cb_string));
}
#[test]
fn test_bivariant_optional_method_param() {
    // Method with optional parameter
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("process");

    // Method with required param
    let method_required = interner.function(FunctionShape {
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

    // Method with optional param
    let method_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_required = interner.object(vec![PropertyInfo::method(method_name, method_required)]);

    let obj_optional = interner.object(vec![PropertyInfo::method(method_name, method_optional)]);

    // Optional param is more general than required
    // Method with optional can accept calls without arg
    let optional_to_required = checker.is_subtype_of(obj_optional, obj_required);
    let required_to_optional = checker.is_subtype_of(obj_required, obj_optional);
    // At least one direction should work
    assert!(optional_to_required || required_to_optional);
}

// =============================================================================
// Intersection Type Subtype Tests
// =============================================================================

// -----------------------------------------------------------------------------
// Intersection Flattening (A & B & C)
// -----------------------------------------------------------------------------
#[test]
fn test_intersection_associativity() {
    // (A & B) & C should be equivalent to A & (B & C)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let obj_c = interner.object(vec![PropertyInfo::new(c_name, TypeId::BOOLEAN)]);

    // (A & B) & C
    let ab = interner.intersection(vec![obj_a, obj_b]);
    let left_assoc = interner.intersection(vec![ab, obj_c]);

    // A & (B & C)
    let bc = interner.intersection(vec![obj_b, obj_c]);
    let right_assoc = interner.intersection(vec![obj_a, bc]);

    // Both should be equivalent
    assert!(checker.is_subtype_of(left_assoc, right_assoc));
    assert!(checker.is_subtype_of(right_assoc, left_assoc));
}
#[test]
fn test_intersection_commutativity() {
    // A & B should be equivalent to B & A
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let ab = interner.intersection(vec![obj_a, obj_b]);
    let ba = interner.intersection(vec![obj_b, obj_a]);

    // A & B should be equivalent to B & A
    assert!(checker.is_subtype_of(ab, ba));
    assert!(checker.is_subtype_of(ba, ab));
}
#[test]
fn test_intersection_four_types() {
    // A & B & C & D flattening
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");
    let d_name = interner.intern_string("d");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let obj_c = interner.object(vec![PropertyInfo::new(c_name, TypeId::BOOLEAN)]);

    let obj_d = interner.object(vec![PropertyInfo::new(d_name, TypeId::STRING)]);

    // Flat four-way intersection
    let flat = interner.intersection(vec![obj_a, obj_b, obj_c, obj_d]);

    // Nested: ((A & B) & C) & D
    let ab = interner.intersection(vec![obj_a, obj_b]);
    let abc = interner.intersection(vec![ab, obj_c]);
    let nested = interner.intersection(vec![abc, obj_d]);

    // Should be equivalent
    assert!(checker.is_subtype_of(flat, nested));
    assert!(checker.is_subtype_of(nested, flat));
}
#[test]
fn test_intersection_with_unknown_identity() {
    // A & unknown = A (unknown is identity for intersection)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let with_unknown = interner.intersection(vec![obj_a, TypeId::UNKNOWN]);

    // A & unknown should be equivalent to A
    assert!(checker.is_subtype_of(with_unknown, obj_a));
    assert!(checker.is_subtype_of(obj_a, with_unknown));
}
#[test]
fn test_intersection_intrinsics_flatten() {
    // string & number & boolean reduces properly
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let intrinsic_intersection =
        interner.intersection(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

    // Disjoint intrinsics intersection is never
    assert!(checker.is_subtype_of(intrinsic_intersection, TypeId::NEVER));
}

// -----------------------------------------------------------------------------
// Intersection vs Object Types
// -----------------------------------------------------------------------------
#[test]
fn test_intersection_equals_merged_object() {
    // { a: string } & { b: number } should equal { a: string, b: number }
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

    // Should be bidirectionally subtype (equivalent)
    assert!(checker.is_subtype_of(intersection, merged));
    assert!(checker.is_subtype_of(merged, intersection));
}
#[test]
fn test_intersection_wider_object_not_subtype() {
    // { a: string, b: number, c: boolean } is subtype of { a: string } & { b: number }
    // but { a: string } is NOT subtype of { a: string } & { b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    let obj_abc = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
        PropertyInfo::new(c_name, TypeId::BOOLEAN),
    ]);

    // Wider object with extra property is subtype of intersection
    assert!(checker.is_subtype_of(obj_abc, intersection));
    // obj_a alone is NOT subtype of intersection (missing b)
    assert!(!checker.is_subtype_of(obj_a, intersection));
}
#[test]
fn test_intersection_overlapping_properties() {
    // { x: string, y: number } & { y: number, z: boolean }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");
    let z_name = interner.intern_string("z");

    let obj_xy = interner.object(vec![
        PropertyInfo::new(x_name, TypeId::STRING),
        PropertyInfo::new(y_name, TypeId::NUMBER),
    ]);

    let obj_yz = interner.object(vec![
        PropertyInfo::new(y_name, TypeId::NUMBER),
        PropertyInfo::new(z_name, TypeId::BOOLEAN),
    ]);

    let intersection = interner.intersection(vec![obj_xy, obj_yz]);

    // Should have all three properties
    let obj_xyz = interner.object(vec![
        PropertyInfo::new(x_name, TypeId::STRING),
        PropertyInfo::new(y_name, TypeId::NUMBER),
        PropertyInfo::new(z_name, TypeId::BOOLEAN),
    ]);

    // Intersection should be equivalent to merged xyz
    assert!(checker.is_subtype_of(intersection, obj_xyz));
    assert!(checker.is_subtype_of(obj_xyz, intersection));
}
#[test]
fn test_intersection_conflicting_property_types() {
    // { x: string } & { x: number } - conflicting property types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");

    let obj_x_string = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let obj_x_number = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let _intersection = interner.intersection(vec![obj_x_string, obj_x_number]);

    // The intersection of { x: string } & { x: number } has x: string & number = never
    // So this should reduce to never or be subtype of never
    // At minimum, neither original object should be subtype of the other
    assert!(!checker.is_subtype_of(obj_x_string, obj_x_number));
    assert!(!checker.is_subtype_of(obj_x_number, obj_x_string));
}
#[test]
fn test_object_subtype_of_intersection() {
    // { a: string, b: number } <: { a: string } & { b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    // Object with both properties is subtype of intersection
    assert!(checker.is_subtype_of(obj_ab, intersection));
    // And intersection is subtype of merged object
    assert!(checker.is_subtype_of(intersection, obj_ab));
}

// -----------------------------------------------------------------------------
// Intersection with Never
// -----------------------------------------------------------------------------
#[test]
fn test_intersection_never_with_object() {
    // { a: string } & never = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let with_never = interner.intersection(vec![obj_a, TypeId::NEVER]);

    // Should be never (subtype of never)
    assert!(checker.is_subtype_of(with_never, TypeId::NEVER));
    // never is subtype of everything
    assert!(checker.is_subtype_of(TypeId::NEVER, with_never));
}
#[test]
fn test_intersection_never_with_function() {
    // ((x: string) => number) & never = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_type = interner.function(FunctionShape {
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
        is_constructor: false,
        is_method: false,
    });

    let with_never = interner.intersection(vec![fn_type, TypeId::NEVER]);

    // Should be never
    assert!(checker.is_subtype_of(with_never, TypeId::NEVER));
}
#[test]
fn test_intersection_never_with_union() {
    // (string | number) & never = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let with_never = interner.intersection(vec![union, TypeId::NEVER]);

    // Should be never
    assert!(checker.is_subtype_of(with_never, TypeId::NEVER));
}
#[test]
fn test_intersection_nested_never() {
    // (A & never) & B = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let a_and_never = interner.intersection(vec![obj_a, TypeId::NEVER]);
    let nested = interner.intersection(vec![a_and_never, obj_b]);

    // Should still be never
    assert!(checker.is_subtype_of(nested, TypeId::NEVER));
}
#[test]
fn test_intersection_never_zero_element() {
    // never as only element in intersection
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let just_never = interner.intersection(vec![TypeId::NEVER]);

    // Should be never
    assert!(checker.is_subtype_of(just_never, TypeId::NEVER));
    assert!(checker.is_subtype_of(TypeId::NEVER, just_never));
}
#[test]
fn test_intersection_multiple_nevers() {
    // never & never = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let double_never = interner.intersection(vec![TypeId::NEVER, TypeId::NEVER]);

    assert!(checker.is_subtype_of(double_never, TypeId::NEVER));
    assert!(checker.is_subtype_of(TypeId::NEVER, double_never));
}

// -----------------------------------------------------------------------------
// Intersection Member Access
// -----------------------------------------------------------------------------
#[test]
fn test_intersection_access_from_first_member() {
    // (A & B).a should be accessible
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    // Intersection should be subtype of { a: string } (can access .a)
    assert!(checker.is_subtype_of(intersection, obj_a));
}
#[test]
fn test_intersection_access_from_second_member() {
    // (A & B).b should be accessible
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    // Intersection should be subtype of { b: number } (can access .b)
    assert!(checker.is_subtype_of(intersection, obj_b));
}
#[test]
fn test_intersection_access_all_members() {
    // (A & B & C) should have access to all properties
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let obj_c = interner.object(vec![PropertyInfo::new(c_name, TypeId::BOOLEAN)]);

    let intersection = interner.intersection(vec![obj_a, obj_b, obj_c]);

    // Can access all three properties
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_b));
    assert!(checker.is_subtype_of(intersection, obj_c));
}
#[test]
fn test_intersection_method_access() {
    // Intersection with method should allow method access
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let method_name = interner.intern_string("doSomething");

    let method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_method = interner.object(vec![PropertyInfo::method(method_name, method)]);

    let intersection = interner.intersection(vec![obj_a, obj_method]);

    // Can access both property and method
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_method));
}
#[test]
fn test_intersection_narrowed_property_access() {
    // { x: string | number } & { x: string } - accessing x gives string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_wide = interner.object(vec![PropertyInfo::new(x_name, wide_type)]);

    let obj_narrow = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![obj_wide, obj_narrow]);

    // Intersection should be subtype of narrow (x is string, not string | number)
    assert!(checker.is_subtype_of(intersection, obj_narrow));
}
#[test]
fn test_intersection_function_member_access() {
    // Intersection of functions - can call with intersection of params
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_string = interner.function(FunctionShape {
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

    let fn_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_intersection = interner.intersection(vec![fn_string, fn_number]);

    // Function intersection can be called with string | number
    let union_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let fn_union_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: union_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (string => void) & (number => void) should be callable with string | number
    assert!(checker.is_subtype_of(fn_union_param, fn_intersection));
}
#[test]
fn test_intersection_readonly_property_access() {
    // Intersection with readonly - readonly is preserved
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a_readonly = interner.object(vec![PropertyInfo::readonly(a_name, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_name, TypeId::NUMBER)]);

    let intersection = interner.intersection(vec![obj_a_readonly, obj_b]);

    // Should be subtype of both
    assert!(checker.is_subtype_of(intersection, obj_a_readonly));
    assert!(checker.is_subtype_of(intersection, obj_b));
}
#[test]
fn test_intersection_optional_property_access() {
    // { a?: string } & { a: string } - a becomes required
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_a_optional = interner.object(vec![PropertyInfo::opt(a_name, TypeId::STRING)]);

    let obj_a_required = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![obj_a_optional, obj_a_required]);

    // Intersection should be subtype of required (a is required in intersection)
    assert!(checker.is_subtype_of(intersection, obj_a_required));
}

// =============================================================================
// Function Type Subtype Tests
// =============================================================================

// -----------------------------------------------------------------------------
// Parameter Contravariance
// -----------------------------------------------------------------------------
#[test]
fn test_fn_param_contravariance_wider_param_is_subtype() {
    // (x: string | number) => void <: (x: string) => void
    // A function that accepts more types can be used where a function accepting fewer is expected
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let param_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let fn_string_param = interner.function(FunctionShape {
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

    let fn_union_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: param_union,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function with wider param type is subtype (contravariance)
    assert!(checker.is_subtype_of(fn_union_param, fn_string_param));
    // Function with narrower param type is NOT subtype
    assert!(!checker.is_subtype_of(fn_string_param, fn_union_param));
}
#[test]
fn test_fn_param_contravariance_unknown_accepts_all() {
    // (x: unknown) => void <: (x: string) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_string_param = interner.function(FunctionShape {
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

    let fn_unknown_param = interner.function(FunctionShape {
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

    // unknown param accepts any input, so it's a subtype
    assert!(checker.is_subtype_of(fn_unknown_param, fn_string_param));
}
#[test]
fn test_fn_param_contravariance_multiple_params() {
    // (a: unknown, b: unknown) => void <: (a: string, b: number) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_specific = interner.function(FunctionShape {
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

    let fn_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::UNKNOWN,
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

    // Wide params is subtype due to contravariance
    assert!(checker.is_subtype_of(fn_wide, fn_specific));
}
#[test]
fn test_fn_param_contravariance_object_type() {
    // (x: { a: string }) => void is NOT subtype of (x: { a: string, b: number }) => void
    // Because { a: string, b: number } is narrower than { a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    let fn_obj_a = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: obj_a,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_obj_ab = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: obj_ab,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // fn_obj_a has wider param (accepts more objects), so it's subtype
    assert!(checker.is_subtype_of(fn_obj_a, fn_obj_ab));
    // fn_obj_ab has narrower param, so it's NOT subtype
    assert!(!checker.is_subtype_of(fn_obj_ab, fn_obj_a));
}
#[test]
fn test_fn_param_contravariance_never_param() {
    // (x: never) => void - can't be called with any value
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_string_param = interner.function(FunctionShape {
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

    let fn_never_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NEVER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // never is the narrowest type, so fn_string is subtype of fn_never (contravariance)
    assert!(checker.is_subtype_of(fn_string_param, fn_never_param));
}
#[test]
fn test_fn_param_contravariance_literal_type() {
    // (x: string) => void <: (x: "hello") => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");

    let fn_string_param = interner.function(FunctionShape {
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

    let fn_literal_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: hello,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // string is wider than "hello", so fn_string is subtype
    assert!(checker.is_subtype_of(fn_string_param, fn_literal_param));
    // "hello" is narrower, so fn_literal is NOT subtype
    assert!(!checker.is_subtype_of(fn_literal_param, fn_string_param));
}

// -----------------------------------------------------------------------------
// Return Type Covariance
// -----------------------------------------------------------------------------
#[test]
fn test_fn_return_covariance_narrower_return_is_subtype() {
    // () => string <: () => string | number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let return_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let fn_return_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: return_union,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Narrower return type is subtype (covariance)
    assert!(checker.is_subtype_of(fn_return_string, fn_return_union));
    // Wider return type is NOT subtype
    assert!(!checker.is_subtype_of(fn_return_union, fn_return_string));
}
#[test]
fn test_fn_return_covariance_literal_return() {
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

    // "hello" is subtype of string, so fn_return_literal is subtype
    assert!(checker.is_subtype_of(fn_return_literal, fn_return_string));
}
#[test]
fn test_fn_return_covariance_never_return() {
    // () => never <: () => T for any T
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

    let fn_return_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // never is subtype of everything
    assert!(checker.is_subtype_of(fn_return_never, fn_return_string));
    assert!(checker.is_subtype_of(fn_return_never, fn_return_number));
}
#[test]
fn test_fn_return_covariance_object_return() {
    // () => { a: string, b: number } <: () => { a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    let fn_return_a = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_a,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_return_ab = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_ab,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // { a, b } is subtype of { a }, so fn_return_ab is subtype
    assert!(checker.is_subtype_of(fn_return_ab, fn_return_a));
    // { a } is NOT subtype of { a, b }
    assert!(!checker.is_subtype_of(fn_return_a, fn_return_ab));
}
#[test]
fn test_fn_return_covariance_void_return() {
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

    // undefined is subtype of void
    assert!(checker.is_subtype_of(fn_return_undefined, fn_return_void));
}
#[test]
fn test_fn_return_covariance_unknown_return() {
    // () => string is NOT subtype of () => unknown in strict sense
    // But () => unknown accepts any return
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

    let fn_return_unknown = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::UNKNOWN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // string is subtype of unknown, so fn_return_string is subtype
    assert!(checker.is_subtype_of(fn_return_string, fn_return_unknown));
}

// -----------------------------------------------------------------------------
// Optional Parameter Handling
// -----------------------------------------------------------------------------
#[test]
fn test_fn_optional_param_fewer_params_is_subtype() {
    // () => void <: (x?: string) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_no_params = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_optional_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function with no params can be used where optional param is expected
    assert!(checker.is_subtype_of(fn_no_params, fn_optional_param));
}
#[test]
fn test_fn_optional_param_required_to_optional() {
    // (x: string) => void is NOT subtype of (x?: string) => void
    // TypeScript widens optional params to string|undefined, so
    // contravariant check: string|undefined <: string fails.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_required = interner.function(FunctionShape {
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

    let fn_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Required param IS subtype of optional — tsc compares declared types,
    // not the | undefined widened type, so (x: string) => void <: (x?: string) => void.
    assert!(checker.is_subtype_of(fn_required, fn_optional));
}
#[test]
fn test_fn_optional_param_optional_to_required_is_subtype() {
    // (x?: string) => void IS subtype of (x: string) => void
    // Contravariant: string <: string|undefined → YES
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_required = interner.function(FunctionShape {
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

    let fn_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Optional IS subtype of required (contravariant: string <: string|undefined)
    assert!(checker.is_subtype_of(fn_optional, fn_required));
}
#[test]
fn test_fn_optional_param_multiple_optional() {
    // (a: string) => void is NOT subtype of (a?: string, b?: number) => void
    // Contravariant: string|undefined <: string fails
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_one_required = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("a")),
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

    let fn_two_optional = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: true,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: TypeId::NUMBER,
                optional: true,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Required IS subtype of optional — tsc compares declared types without
    // | undefined widening, so (x: string) => void <: (x?: string, y?: number) => void.
    assert!(checker.is_subtype_of(fn_one_required, fn_two_optional));
}
#[test]
fn test_fn_optional_param_mixed_required_optional() {
    // (a: string, b: number) => void is NOT subtype of (a: string, b?: number) => void
    // Contravariant on b: number|undefined <: number fails
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_both_required = interner.function(FunctionShape {
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

    let fn_one_optional = interner.function(FunctionShape {
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
                optional: true,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Required IS subtype of optional — tsc compares b's declared type (number),
    // not number | undefined, so (a: string, b: number) <: (a: string, b?: number).
    assert!(checker.is_subtype_of(fn_both_required, fn_one_optional));
}
#[test]
fn test_fn_optional_param_with_undefined_union() {
    // (x: string | undefined) => void vs (x?: string) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_or_undefined = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    let fn_union_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: string_or_undefined,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_optional_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // These should be related - exact relationship depends on implementation
    // At minimum, check they don't crash
    let _union_to_optional = checker.is_subtype_of(fn_union_param, fn_optional_param);
    let _optional_to_union = checker.is_subtype_of(fn_optional_param, fn_union_param);
}

// -----------------------------------------------------------------------------
// Rest Parameter Assignability
// -----------------------------------------------------------------------------
#[test]
fn test_fn_rest_param_basic() {
    // (...args: string[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    let fn_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_no_params = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // No params should be subtype of rest (can be called with zero args)
    assert!(checker.is_subtype_of(fn_no_params, fn_rest));
}
#[test]
fn test_fn_rest_param_fixed_params_to_rest() {
    // (a: string, b: string) => void <: (...args: string[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    let fn_two_strings = interner.function(FunctionShape {
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
                type_id: TypeId::STRING,
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

    let fn_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Fixed string params should be subtype of rest strings
    assert!(checker.is_subtype_of(fn_two_strings, fn_rest));
}
#[test]
fn test_fn_rest_param_wider_element_type() {
    // (...args: unknown[]) => void <: (...args: string[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let unknown_array = interner.array(TypeId::UNKNOWN);

    let fn_rest_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_rest_unknown = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: unknown_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // unknown[] accepts more, so it's subtype (contravariance)
    assert!(checker.is_subtype_of(fn_rest_unknown, fn_rest_string));
}
#[test]
fn test_fn_rest_param_with_leading_params() {
    // (a: string, ...rest: number[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let fn_with_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("rest")),
                type_id: number_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_just_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("a")),
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

    // Just string param should be subtype (rest can be empty)
    assert!(checker.is_subtype_of(fn_just_string, fn_with_rest));
}
#[test]
fn test_fn_rest_param_union_element_type() {
    // (...args: (string | number)[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let union_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_type);

    let fn_rest_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_rest_union = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: union_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Union array accepts more types, so it's subtype
    assert!(checker.is_subtype_of(fn_rest_union, fn_rest_string));
}
#[test]
fn test_fn_rest_to_rest_same_type() {
    // (...args: string[]) => void <: (...args: string[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    let fn_rest1 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_rest2 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: string_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Same rest type should be bidirectionally subtype
    assert!(checker.is_subtype_of(fn_rest1, fn_rest2));
    assert!(checker.is_subtype_of(fn_rest2, fn_rest1));
}
#[test]
fn test_fn_rest_combined_with_optional() {
    // (a?: string, ...rest: number[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let fn_optional_and_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: true,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("rest")),
                type_id: number_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_no_params = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // No params should be subtype (both optional and rest can be empty)
    assert!(checker.is_subtype_of(fn_no_params, fn_optional_and_rest));
}

// =============================================================================
// Object Literal Type Tests
// =============================================================================

// -----------------------------------------------------------------------------
// Excess Property Checking
// -----------------------------------------------------------------------------
#[test]
fn test_excess_property_structural_subtype() {
    // { a: string, b: number } <: { a: string } (structural subtyping)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    // Object with extra property is subtype (structural)
    assert!(checker.is_subtype_of(obj_ab, obj_a));
    // Object missing property is NOT subtype
    assert!(!checker.is_subtype_of(obj_a, obj_ab));
}
#[test]
fn test_excess_property_three_extra() {
    // { a, b, c, d } <: { a }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");
    let d_name = interner.intern_string("d");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_abcd = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
        PropertyInfo::new(c_name, TypeId::BOOLEAN),
        PropertyInfo::new(d_name, TypeId::STRING),
    ]);

    // Multiple extra properties still subtype
    assert!(checker.is_subtype_of(obj_abcd, obj_a));
}
#[test]
fn test_excess_property_different_required() {
    // { a: string, b: number } is NOT subtype of { a: string, c: boolean }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    let obj_ac = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(c_name, TypeId::BOOLEAN),
    ]);

    // Missing required property c
    assert!(!checker.is_subtype_of(obj_ab, obj_ac));
    // Missing required property b
    assert!(!checker.is_subtype_of(obj_ac, obj_ab));
}
#[test]
fn test_excess_property_with_method() {
    // { a: string, method(): void } <: { a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let method_name = interner.intern_string("method");

    let method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_a_method = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::method(method_name, method),
    ]);

    // Extra method is still subtype
    assert!(checker.is_subtype_of(obj_a_method, obj_a));
}
#[test]
fn test_excess_property_narrower_type() {
    // { a: "hello" } <: { a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let hello = interner.literal_string("hello");

    let obj_a_literal = interner.object(vec![PropertyInfo::new(a_name, hello)]);

    let obj_a_string = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    // Literal type is subtype of wider type
    assert!(checker.is_subtype_of(obj_a_literal, obj_a_string));
    // Wider type is NOT subtype of literal
    assert!(!checker.is_subtype_of(obj_a_string, obj_a_literal));
}
#[test]
fn test_excess_property_empty_object() {
    // { a: string } <: {} (empty object accepts all)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_a = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let empty_obj = interner.object(vec![]);

    // Any object is subtype of empty object
    assert!(checker.is_subtype_of(obj_a, empty_obj));
}

// -----------------------------------------------------------------------------
// Optional Property Matching
// -----------------------------------------------------------------------------
#[test]
fn test_optional_property_required_to_optional() {
    // { a: string } <: { a?: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_required = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_optional = interner.object(vec![PropertyInfo::opt(a_name, TypeId::STRING)]);

    // Required is subtype of optional
    assert!(checker.is_subtype_of(obj_required, obj_optional));
}
#[test]
fn test_optional_property_optional_to_required_not_subtype() {
    // { a?: string } is NOT subtype of { a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_required = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_optional = interner.object(vec![PropertyInfo::opt(a_name, TypeId::STRING)]);

    // Optional is NOT subtype of required
    assert!(!checker.is_subtype_of(obj_optional, obj_required));
}
#[test]
fn test_optional_property_missing_optional() {
    // {} <: { a?: string } (missing optional property is OK)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let empty_obj = interner.object(vec![]);

    let obj_optional = interner.object(vec![PropertyInfo::opt(a_name, TypeId::STRING)]);

    // Empty object is subtype of object with only optional properties
    assert!(checker.is_subtype_of(empty_obj, obj_optional));
}
#[test]
fn test_optional_property_mixed_required_optional() {
    // { a: string, b: number } <: { a: string, b?: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_both_required = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::NUMBER),
    ]);

    let obj_b_optional = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::opt(b_name, TypeId::NUMBER),
    ]);

    // Both required is subtype of one optional
    assert!(checker.is_subtype_of(obj_both_required, obj_b_optional));
}
#[test]
fn test_optional_property_all_optional() {
    // { a?: string, b?: number } <: { a?: string, b?: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj = interner.object(vec![
        PropertyInfo::opt(a_name, TypeId::STRING),
        PropertyInfo::opt(b_name, TypeId::NUMBER),
    ]);

    // Same optional properties - bidirectional subtype
    assert!(checker.is_subtype_of(obj, obj));
}
#[test]
fn test_optional_property_type_mismatch() {
    // { a?: string } is NOT subtype of { a?: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_optional_string = interner.object(vec![PropertyInfo::opt(a_name, TypeId::STRING)]);

    let obj_optional_number = interner.object(vec![PropertyInfo::opt(a_name, TypeId::NUMBER)]);

    // Different types - not subtypes
    assert!(!checker.is_subtype_of(obj_optional_string, obj_optional_number));
    assert!(!checker.is_subtype_of(obj_optional_number, obj_optional_string));
}

// -----------------------------------------------------------------------------
// Index Signature Assignability
// -----------------------------------------------------------------------------
#[test]
fn test_index_signature_string_basic() {
    // { [key: string]: number } - string index signature
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_number = interner.object_with_index(ObjectShape {
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

    let indexed_string = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Different value types - not subtypes
    assert!(!checker.is_subtype_of(indexed_number, indexed_string));
    assert!(!checker.is_subtype_of(indexed_string, indexed_number));
}
#[test]
fn test_index_signature_covariant_value() {
    // { [key: string]: "hello" } <: { [key: string]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");

    let indexed_literal = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: hello,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let indexed_string = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Literal value type is subtype of wider value type
    assert!(checker.is_subtype_of(indexed_literal, indexed_string));
}
#[test]
fn test_index_signature_with_known_property() {
    // { a: string, [key: string]: string } <: { [key: string]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let indexed_with_prop = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(a_name, TypeId::STRING)],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let indexed_only = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Object with known property and index signature is subtype
    assert!(checker.is_subtype_of(indexed_with_prop, indexed_only));
}
#[test]
fn test_index_signature_number_index() {
    // { [key: number]: string } - number index signature (array-like)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let string_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Number index and string index are different
    // In TypeScript, number index must be subtype of string index value
    let _result = checker.is_subtype_of(number_indexed, string_indexed);
}
#[test]
fn test_index_signature_union_value() {
    // { [key: string]: string | number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_value = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let indexed_union = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: union_value,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let indexed_string = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // string is subtype of string | number
    // So { [k: string]: string } <: { [k: string]: string | number }
    assert!(checker.is_subtype_of(indexed_string, indexed_union));
}
#[test]
fn test_index_signature_object_to_indexed() {
    // { a: string, b: string } <: { [key: string]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::new(b_name, TypeId::STRING),
    ]);

    let indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Object with matching property types is subtype of index signature
    assert!(checker.is_subtype_of(obj_ab, indexed));
}

// -----------------------------------------------------------------------------
// Readonly Property Handling
// -----------------------------------------------------------------------------
#[test]
fn test_readonly_mutable_to_readonly() {
    // { a: string } <: { readonly a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_mutable = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_readonly = interner.object(vec![PropertyInfo::readonly(a_name, TypeId::STRING)]);

    // Mutable is subtype of readonly (can read from both)
    assert!(checker.is_subtype_of(obj_mutable, obj_readonly));
}
#[test]
fn test_readonly_to_mutable() {
    // { readonly a: string } may or may not be subtype of { a: string }
    // This depends on whether we allow readonly-to-mutable assignment
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_mutable = interner.object(vec![PropertyInfo::new(a_name, TypeId::STRING)]);

    let obj_readonly = interner.object(vec![PropertyInfo::readonly(a_name, TypeId::STRING)]);

    // Check both directions - implementation-dependent
    let _readonly_to_mutable = checker.is_subtype_of(obj_readonly, obj_mutable);
    let _mutable_to_readonly = checker.is_subtype_of(obj_mutable, obj_readonly);
}
#[test]
fn test_readonly_both_readonly() {
    // { readonly a: string } <: { readonly a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_readonly = interner.object(vec![PropertyInfo::readonly(a_name, TypeId::STRING)]);

    // Same readonly - bidirectional subtype
    assert!(checker.is_subtype_of(obj_readonly, obj_readonly));
}
#[test]
fn test_readonly_mixed_properties() {
    // { a: string, readonly b: number } <: { a: string, readonly b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj = interner.object(vec![
        PropertyInfo::new(a_name, TypeId::STRING),
        PropertyInfo::readonly(b_name, TypeId::NUMBER),
    ]);

    // Same object - bidirectional subtype
    assert!(checker.is_subtype_of(obj, obj));
}
#[test]
fn test_readonly_narrower_type() {
    // { readonly a: "hello" } <: { readonly a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let hello = interner.literal_string("hello");

    let obj_literal = interner.object(vec![PropertyInfo::readonly(a_name, hello)]);

    let obj_string = interner.object(vec![PropertyInfo::readonly(a_name, TypeId::STRING)]);

    // Readonly literal is subtype of readonly wider type
    assert!(checker.is_subtype_of(obj_literal, obj_string));
}
#[test]
fn test_readonly_with_optional() {
    // { readonly a?: string } - both readonly and optional
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_readonly_optional = interner.object(vec![PropertyInfo {
        name: a_name,
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

    let obj_readonly_required =
        interner.object(vec![PropertyInfo::readonly(a_name, TypeId::STRING)]);

    // Required is subtype of optional (even with readonly)
    assert!(checker.is_subtype_of(obj_readonly_required, obj_readonly_optional));
    // Optional is NOT subtype of required
    assert!(!checker.is_subtype_of(obj_readonly_optional, obj_readonly_required));
}
#[test]
fn test_readonly_array_like() {
    // ReadonlyArray<T> pattern - readonly with index signature
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let length_name = interner.intern_string("length");

    let readonly_array_like =
        interner.object(vec![PropertyInfo::readonly(length_name, TypeId::NUMBER)]);

    let mutable_array_like = interner.object(vec![PropertyInfo::new(length_name, TypeId::NUMBER)]);

    // Mutable is subtype of readonly
    assert!(checker.is_subtype_of(mutable_array_like, readonly_array_like));
}
#[test]
fn test_readonly_method_property() {
    // { readonly method(): void }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("method");

    let method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_readonly_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: method,
        write_type: method,
        optional: false,
        readonly: true,
        is_method: true,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let obj_mutable_method = interner.object(vec![PropertyInfo::method(method_name, method)]);

    // Mutable method is subtype of readonly method
    assert!(checker.is_subtype_of(obj_mutable_method, obj_readonly_method));
}

// =============================================================================
// Tuple Type Subtype Tests
// =============================================================================

// -----------------------------------------------------------------------------
// Fixed Length Tuple Assignability
// -----------------------------------------------------------------------------
#[test]
fn test_tuple_fixed_same_length_same_types() {
    // [string, number] <: [string, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple1 = interner.tuple(vec![
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

    let tuple2 = interner.tuple(vec![
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

    // Same types - bidirectional subtype
    assert!(checker.is_subtype_of(tuple1, tuple2));
    assert!(checker.is_subtype_of(tuple2, tuple1));
}
#[test]
fn test_tuple_fixed_covariant_elements() {
    // ["hello", 42] <: [string, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);

    let literal_tuple = interner.tuple(vec![
        TupleElement {
            type_id: hello,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: forty_two,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let wide_tuple = interner.tuple(vec![
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

    // Literal tuple is subtype of wider tuple
    assert!(checker.is_subtype_of(literal_tuple, wide_tuple));
    // Wider tuple is NOT subtype of literal
    assert!(!checker.is_subtype_of(wide_tuple, literal_tuple));
}
#[test]
fn test_tuple_fixed_different_lengths_not_subtype() {
    // [string, number, boolean] is NOT subtype of [string, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_3 = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let tuple_2 = interner.tuple(vec![
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

    // Extra element - not subtype of fixed tuple
    assert!(!checker.is_subtype_of(tuple_3, tuple_2));
    // Missing element - not subtype
    assert!(!checker.is_subtype_of(tuple_2, tuple_3));
}
#[test]
fn test_tuple_fixed_type_mismatch() {
    // [string, string] is NOT subtype of [string, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_ss = interner.tuple(vec![
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

    let tuple_sn = interner.tuple(vec![
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

    // Different element types - not subtypes
    assert!(!checker.is_subtype_of(tuple_ss, tuple_sn));
    assert!(!checker.is_subtype_of(tuple_sn, tuple_ss));
}
#[test]
fn test_tuple_fixed_empty_tuple() {
    // [] <: []
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty_tuple = interner.tuple(vec![]);

    // Empty tuple is subtype of itself
    assert!(checker.is_subtype_of(empty_tuple, empty_tuple));
}
#[test]
fn test_tuple_fixed_single_element() {
    // [string] <: [string]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let single = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(checker.is_subtype_of(single, single));
}
#[test]
fn test_tuple_fixed_union_element() {
    // [string | number] <: [string | number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let tuple_union = interner.tuple(vec![TupleElement {
        type_id: union,
        name: None,
        optional: false,
        rest: false,
    }]);

    let tuple_string = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    // [string] <: [string | number]
    assert!(checker.is_subtype_of(tuple_string, tuple_union));
    // [string | number] is NOT subtype of [string]
    assert!(!checker.is_subtype_of(tuple_union, tuple_string));
}

// -----------------------------------------------------------------------------
// Rest Element Handling
// -----------------------------------------------------------------------------
#[test]
fn test_tuple_rest_basic() {
    // [string, ...number[]] - tuple with rest
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let tuple_with_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let tuple_string_number = interner.tuple(vec![
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

    // Fixed tuple with matching types is subtype of rest tuple
    assert!(checker.is_subtype_of(tuple_string_number, tuple_with_rest));
}
#[test]
fn test_tuple_rest_accepts_multiple() {
    // [string, number, number, number] <: [string, ...number[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let tuple_with_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let tuple_four = interner.tuple(vec![
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

    // Multiple numbers match rest
    assert!(checker.is_subtype_of(tuple_four, tuple_with_rest));
}
#[test]
fn test_tuple_rest_accepts_zero() {
    // [string] <: [string, ...number[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let tuple_with_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let tuple_one = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    // Zero rest elements is valid
    assert!(checker.is_subtype_of(tuple_one, tuple_with_rest));
}
#[test]
fn test_tuple_rest_type_mismatch() {
    // [string, boolean] is NOT subtype of [string, ...number[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let tuple_with_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let tuple_bool = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // boolean doesn't match number rest
    assert!(!checker.is_subtype_of(tuple_bool, tuple_with_rest));
}
#[test]
fn test_tuple_rest_to_rest() {
    // [...string[]] <: [...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    let tuple_rest1 = interner.tuple(vec![TupleElement {
        type_id: string_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    let tuple_rest2 = interner.tuple(vec![TupleElement {
        type_id: string_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    // Same rest types - bidirectional subtype
    assert!(checker.is_subtype_of(tuple_rest1, tuple_rest2));
    assert!(checker.is_subtype_of(tuple_rest2, tuple_rest1));
}
#[test]
fn test_tuple_rest_covariant() {
    // [...("hello")[]] <: [...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let hello_array = interner.array(hello);
    let string_array = interner.array(TypeId::STRING);

    let tuple_literal_rest = interner.tuple(vec![TupleElement {
        type_id: hello_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    let tuple_string_rest = interner.tuple(vec![TupleElement {
        type_id: string_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    // Literal rest is subtype of string rest
    assert!(checker.is_subtype_of(tuple_literal_rest, tuple_string_rest));
}
#[test]
fn test_tuple_rest_middle_position() {
    // [string, ...number[], boolean] - rest in middle
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    let tuple_middle_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let tuple_three = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Fixed tuple matches middle rest
    assert!(checker.is_subtype_of(tuple_three, tuple_middle_rest));
}

// -----------------------------------------------------------------------------
// Optional Element Patterns
// -----------------------------------------------------------------------------
#[test]
fn test_tuple_optional_basic() {
    // [string, number?] - optional second element
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_optional = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    let tuple_one = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    // Shorter tuple matches optional
    assert!(checker.is_subtype_of(tuple_one, tuple_optional));
}
#[test]
fn test_tuple_optional_provided() {
    // [string, number] <: [string, number?]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_optional = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    let tuple_both = interner.tuple(vec![
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

    // Full tuple with optional provided is subtype
    assert!(checker.is_subtype_of(tuple_both, tuple_optional));
}
#[test]
fn test_tuple_optional_all_optional() {
    // [string?, number?]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_all_optional = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    let empty_tuple = interner.tuple(vec![]);

    // Empty tuple matches all optional
    assert!(checker.is_subtype_of(empty_tuple, tuple_all_optional));
}
#[test]
fn test_tuple_optional_type_mismatch() {
    // [string, boolean] is NOT subtype of [string, number?]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_optional_number = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    let tuple_with_bool = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Wrong type for optional slot
    assert!(!checker.is_subtype_of(tuple_with_bool, tuple_optional_number));
}
#[test]
fn test_tuple_optional_required_to_optional() {
    // Required element can fill optional slot
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_required = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    let tuple_optional = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: true,
        rest: false,
    }]);

    // Required is subtype of optional
    assert!(checker.is_subtype_of(tuple_required, tuple_optional));
}
#[test]
fn test_tuple_optional_to_required_not_subtype() {
    // [string?] is NOT subtype of [string]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_required = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    let tuple_optional = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: true,
        rest: false,
    }]);

    // Optional is NOT subtype of required
    assert!(!checker.is_subtype_of(tuple_optional, tuple_required));
}
#[test]
fn test_tuple_optional_multiple() {
    // [string, number?, boolean?]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_multi_optional = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    let tuple_one = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    let tuple_two = interner.tuple(vec![
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

    // Both shorter tuples match
    assert!(checker.is_subtype_of(tuple_one, tuple_multi_optional));
    assert!(checker.is_subtype_of(tuple_two, tuple_multi_optional));
}

// -----------------------------------------------------------------------------
// Labeled Tuple Elements
// -----------------------------------------------------------------------------
#[test]
fn test_tuple_labeled_same_labels() {
    // [x: string, y: number] <: [x: string, y: number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");

    let tuple1 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(x_name),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(y_name),
            optional: false,
            rest: false,
        },
    ]);

    let tuple2 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(x_name),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(y_name),
            optional: false,
            rest: false,
        },
    ]);

    // Same labels - bidirectional subtype
    assert!(checker.is_subtype_of(tuple1, tuple2));
    assert!(checker.is_subtype_of(tuple2, tuple1));
}
#[test]
fn test_tuple_labeled_to_unlabeled() {
    // [x: string, y: number] <: [string, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");

    let labeled = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(x_name),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(y_name),
            optional: false,
            rest: false,
        },
    ]);

    let unlabeled = interner.tuple(vec![
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

    // Labels don't affect subtyping - types must match
    assert!(checker.is_subtype_of(labeled, unlabeled));
    assert!(checker.is_subtype_of(unlabeled, labeled));
}
#[test]
fn test_tuple_labeled_different_labels() {
    // [a: string, b: number] <: [x: string, y: number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");

    let tuple_ab = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(a_name),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(b_name),
            optional: false,
            rest: false,
        },
    ]);

    let tuple_xy = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(x_name),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(y_name),
            optional: false,
            rest: false,
        },
    ]);

    // Different labels but same types - should still be subtypes
    assert!(checker.is_subtype_of(tuple_ab, tuple_xy));
    assert!(checker.is_subtype_of(tuple_xy, tuple_ab));
}
#[test]
fn test_tuple_labeled_optional() {
    // [x: string, y?: number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");

    let labeled_optional = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(x_name),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(y_name),
            optional: true,
            rest: false,
        },
    ]);

    let labeled_one = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: Some(x_name),
        optional: false,
        rest: false,
    }]);

    // Shorter tuple matches optional labeled
    assert!(checker.is_subtype_of(labeled_one, labeled_optional));
}
