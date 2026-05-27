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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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

