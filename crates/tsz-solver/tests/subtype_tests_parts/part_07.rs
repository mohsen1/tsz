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
fn test_tuple_required_undefined_union_to_optional() {
    // A required element typed as `T | undefined` should satisfy an optional `T` slot.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let str_or_undef = interner.union2(TypeId::STRING, TypeId::UNDEFINED);
    let tuple_required_union = interner.tuple(vec![TupleElement {
        type_id: str_or_undef,
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

    assert!(checker.is_subtype_of(tuple_required_union, tuple_optional));
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

#[test]
fn test_tuple_labeled_rest() {
    // [x: string, ...rest: number[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let rest_name = interner.intern_string("rest");
    let number_array = interner.array(TypeId::NUMBER);

    let labeled_rest = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(x_name),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: Some(rest_name),
            optional: false,
            rest: true,
        },
    ]);

    let labeled_two = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(x_name),
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

    // Fixed elements match labeled rest
    assert!(checker.is_subtype_of(labeled_two, labeled_rest));
}

#[test]
fn test_tuple_labeled_covariant() {
    // [x: "hello"] <: [x: string]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let hello = interner.literal_string("hello");

    let literal_labeled = interner.tuple(vec![TupleElement {
        type_id: hello,
        name: Some(x_name),
        optional: false,
        rest: false,
    }]);

    let string_labeled = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: Some(x_name),
        optional: false,
        rest: false,
    }]);

    // Literal labeled is subtype of string labeled
    assert!(checker.is_subtype_of(literal_labeled, string_labeled));
}

#[test]
fn test_tuple_labeled_mixed() {
    // [x: string, number, y: boolean] - mixed labeled/unlabeled
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");

    let mixed = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(x_name),
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
            name: Some(y_name),
            optional: false,
            rest: false,
        },
    ]);

    let all_unlabeled = interner.tuple(vec![
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

    // Mixed and unlabeled should be equivalent
    assert!(checker.is_subtype_of(mixed, all_unlabeled));
    assert!(checker.is_subtype_of(all_unlabeled, mixed));
}

// =============================================================================
// CLASS INHERITANCE HIERARCHY TESTS
// =============================================================================

#[test]
fn test_class_inheritance_derived_extends_base() {
    // class Base { base: string }
    // class Derived extends Base { derived: number }
    // Derived <: Base
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let base_prop = interner.intern_string("base");
    let derived_prop = interner.intern_string("derived");

    let base = interner.object(vec![PropertyInfo::new(base_prop, TypeId::STRING)]);

    let derived = interner.object(vec![
        PropertyInfo::new(base_prop, TypeId::STRING),
        PropertyInfo::new(derived_prop, TypeId::NUMBER),
    ]);

    // Derived is subtype of Base (has all base properties)
    assert!(checker.is_subtype_of(derived, base));
    // Base is not subtype of Derived (missing derived property)
    assert!(!checker.is_subtype_of(base, derived));
}

#[test]
fn test_class_inheritance_multi_level() {
    // class A { a: string }
    // class B extends A { b: number }
    // class C extends B { c: boolean }
    // C <: B <: A
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");
    let c_prop = interner.intern_string("c");

    let class_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let class_b = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let class_c = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
        PropertyInfo::new(c_prop, TypeId::BOOLEAN),
    ]);

    // Transitive inheritance
    assert!(checker.is_subtype_of(class_c, class_b));
    assert!(checker.is_subtype_of(class_b, class_a));
    assert!(checker.is_subtype_of(class_c, class_a));

    // Not the reverse
    assert!(!checker.is_subtype_of(class_a, class_b));
    assert!(!checker.is_subtype_of(class_b, class_c));
}

#[test]
fn test_class_inheritance_method_override() {
    // class Base { method(): string }
    // class Derived extends Base { method(): "hello" }
    // Derived <: Base (covariant return)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("method");
    let hello = interner.literal_string("hello");

    let base_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let derived_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: hello,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let base = interner.object(vec![PropertyInfo::method(method_name, base_method)]);

    let derived = interner.object(vec![PropertyInfo::method(method_name, derived_method)]);

    // Derived with narrower return type is subtype
    assert!(checker.is_subtype_of(derived, base));
}

#[test]
fn test_class_inheritance_same_structure() {
    // Two classes with identical structure are structurally equivalent
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop = interner.intern_string("value");

    let class1 = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);

    let class2 = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);

    // Structurally identical
    assert!(checker.is_subtype_of(class1, class2));
    assert!(checker.is_subtype_of(class2, class1));
}

#[test]
fn test_class_inheritance_property_type_mismatch() {
    // class Base { value: string }
    // class Other { value: number }
    // Neither is subtype of the other
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop = interner.intern_string("value");

    let class1 = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);

    let class2 = interner.object(vec![PropertyInfo::new(prop, TypeId::NUMBER)]);

    // Property types don't match
    assert!(!checker.is_subtype_of(class1, class2));
    assert!(!checker.is_subtype_of(class2, class1));
}

#[test]
fn test_class_inheritance_with_constructor() {
    // class with constructor modeled as object with properties
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let name_prop = interner.intern_string("name");
    let age_prop = interner.intern_string("age");

    let person = interner.object(vec![
        PropertyInfo::new(name_prop, TypeId::STRING),
        PropertyInfo::new(age_prop, TypeId::NUMBER),
    ]);

    let employee = interner.object(vec![
        PropertyInfo::new(name_prop, TypeId::STRING),
        PropertyInfo::new(age_prop, TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("employeeId"), TypeId::STRING),
    ]);

    // Employee extends Person structurally
    assert!(checker.is_subtype_of(employee, person));
    assert!(!checker.is_subtype_of(person, employee));
}

#[test]
fn test_class_inheritance_diamond() {
    // Diamond inheritance: D extends B, C which both extend A
    // D should be subtype of A
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");
    let c_prop = interner.intern_string("c");
    let d_prop = interner.intern_string("d");

    let class_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    // D has all properties from the diamond
    let class_d = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
        PropertyInfo::new(c_prop, TypeId::BOOLEAN),
        PropertyInfo::new(d_prop, TypeId::STRING),
    ]);

    // D is subtype of A (has all A properties)
    assert!(checker.is_subtype_of(class_d, class_a));
}

// =============================================================================
// IMPLEMENTS CLAUSE CHECKING TESTS
// =============================================================================

#[test]
fn test_implements_simple_interface() {
    // interface IGreeter { greet(): string }
    // class Greeter implements IGreeter { greet() { return "hello"; } }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let greet = interner.intern_string("greet");

    let greet_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let interface = interner.object(vec![PropertyInfo::method(greet, greet_method)]);

    // Class has additional property
    let class_impl = interner.object(vec![
        PropertyInfo::method(greet, greet_method),
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
    ]);

    // Class implements interface
    assert!(checker.is_subtype_of(class_impl, interface));
}

#[test]
fn test_implements_multiple_interfaces() {
    // interface A { a(): void }
    // interface B { b(): void }
    // class C implements A, B
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_method_name = interner.intern_string("a");
    let b_method_name = interner.intern_string("b");

    let void_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let interface_a = interner.object(vec![PropertyInfo::method(a_method_name, void_method)]);

    let interface_b = interner.object(vec![PropertyInfo::method(b_method_name, void_method)]);

    let class_c = interner.object(vec![
        PropertyInfo::method(a_method_name, void_method),
        PropertyInfo::method(b_method_name, void_method),
    ]);

    // Class implements both interfaces
    assert!(checker.is_subtype_of(class_c, interface_a));
    assert!(checker.is_subtype_of(class_c, interface_b));
}

#[test]
fn test_implements_missing_method() {
    // interface I { required(): void }
    // class C {} - missing required method
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let required = interner.intern_string("required");

    let void_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let interface = interner.object(vec![PropertyInfo::method(required, void_method)]);

    // Empty class
    let class_c = interner.object(vec![]);

    // Class does not implement interface
    assert!(!checker.is_subtype_of(class_c, interface));
}

#[test]
fn test_implements_optional_method() {
    // interface I { optional?(): void }
    // class C {} - OK, optional is optional
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let optional = interner.intern_string("optional");

    let void_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let interface = interner.object(vec![PropertyInfo {
        name: optional,
        type_id: void_method,
        write_type: void_method,
        optional: true,
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

    // Empty class
    let class_c = interner.object(vec![]);

    // Class implements interface (optional method not required)
    assert!(checker.is_subtype_of(class_c, interface));
}

#[test]
fn test_implements_wrong_signature() {
    // interface I { method(x: string): void }
    // class C { method(x: number): void } - wrong signature
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("method");

    let interface_method = interner.function(FunctionShape {
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

    let class_method = interner.function(FunctionShape {
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

    let interface = interner.object(vec![PropertyInfo::method(method_name, interface_method)]);

    let class_c = interner.object(vec![PropertyInfo::method(method_name, class_method)]);

    // Class does not implement interface (param type mismatch)
    assert!(!checker.is_subtype_of(class_c, interface));
}

#[test]
fn test_implements_interface_extends_interface() {
    // interface A { a: string }
    // interface B extends A { b: number }
    // class C implements B
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let interface_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let interface_b = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let class_c = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    // Class implements both interfaces
    assert!(checker.is_subtype_of(class_c, interface_a));
    assert!(checker.is_subtype_of(class_c, interface_b));
}

#[test]
fn test_implements_property_with_getter() {
    // interface I { readonly value: string }
    // class C { get value(): string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");

    let interface = interner.object(vec![PropertyInfo::readonly(value, TypeId::STRING)]);

    let class_c = interner.object(vec![PropertyInfo::readonly(value, TypeId::STRING)]);

    // Class implements readonly property
    assert!(checker.is_subtype_of(class_c, interface));
}

// =============================================================================
// ABSTRACT CLASS HANDLING TESTS
// =============================================================================

#[test]
fn test_abstract_class_with_abstract_method() {
    // abstract class Base { abstract method(): void }
    // class Derived extends Base { method() {} }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("method");

    let void_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Abstract base class structure
    let abstract_base = interner.object(vec![PropertyInfo::method(method_name, void_method)]);

    // Concrete derived class
    let derived = interner.object(vec![PropertyInfo::method(method_name, void_method)]);

    // Derived is subtype of abstract base
    assert!(checker.is_subtype_of(derived, abstract_base));
}

#[test]
fn test_abstract_class_with_concrete_method() {
    // abstract class Base { concrete(): string { return ""; } abstract abs(): void }
    // class Derived extends Base { abs() {} }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let concrete_name = interner.intern_string("concrete");
    let abstract_name = interner.intern_string("abs");

    let string_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let void_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let abstract_base = interner.object(vec![
        PropertyInfo::method(concrete_name, string_method),
        PropertyInfo::method(abstract_name, void_method),
    ]);

    let derived = interner.object(vec![
        PropertyInfo::method(concrete_name, string_method),
        PropertyInfo::method(abstract_name, void_method),
    ]);

    // Derived is subtype of abstract base
    assert!(checker.is_subtype_of(derived, abstract_base));
}

#[test]
fn test_abstract_class_to_abstract_class() {
    // abstract class A { abstract a(): void }
    // abstract class B extends A { abstract b(): void }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_method = interner.intern_string("a");
    let b_method = interner.intern_string("b");

    let void_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let abstract_a = interner.object(vec![PropertyInfo::method(a_method, void_method)]);

    let abstract_b = interner.object(vec![
        PropertyInfo::method(a_method, void_method),
        PropertyInfo::method(b_method, void_method),
    ]);

    // B extends A
    assert!(checker.is_subtype_of(abstract_b, abstract_a));
    assert!(!checker.is_subtype_of(abstract_a, abstract_b));
}

#[test]
fn test_abstract_class_with_property() {
    // abstract class Base { abstract value: string }
    // class Derived extends Base { value = "hello" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");

    let abstract_base = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);

    let hello = interner.literal_string("hello");
    let derived = interner.object(vec![PropertyInfo::new(value, hello)]);

    // Derived with literal type is subtype
    assert!(checker.is_subtype_of(derived, abstract_base));
}

#[test]
fn test_abstract_class_generic_method() {
    // abstract class Base<T> { abstract process(x: T): T }
    // Modeled as concrete instantiation
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let process = interner.intern_string("process");

    // Instantiated with string
    let string_process = interner.function(FunctionShape {
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
        is_constructor: false,
        is_method: false,
    });

    // Instantiated with number
    let number_process = interner.function(FunctionShape {
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
        is_constructor: false,
        is_method: false,
    });

    let base_string = interner.object(vec![PropertyInfo::method(process, string_process)]);

    let base_number = interner.object(vec![PropertyInfo::method(process, number_process)]);

    // Different instantiations are not subtypes
    assert!(!checker.is_subtype_of(base_string, base_number));
    assert!(!checker.is_subtype_of(base_number, base_string));
}

#[test]
fn test_abstract_class_missing_implementation() {
    // abstract class Base { abstract method(): void; concrete(): string }
    // class Incomplete { concrete(): string } - missing method
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("method");
    let concrete_name = interner.intern_string("concrete");

    let void_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let string_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let abstract_base = interner.object(vec![
        PropertyInfo::method(method_name, void_method),
        PropertyInfo::method(concrete_name, string_method),
    ]);

    // Incomplete - missing abstract method
    let incomplete = interner.object(vec![PropertyInfo::method(concrete_name, string_method)]);

    // Incomplete is not subtype (missing method)
    assert!(!checker.is_subtype_of(incomplete, abstract_base));
}

#[test]
fn test_abstract_class_protected_member() {
    // abstract class Base { protected value: string }
    // Modeled as regular property (protected is access control, not type)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");

    let base = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);

    let derived = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);

    // Structurally equivalent
    assert!(checker.is_subtype_of(derived, base));
    assert!(checker.is_subtype_of(base, derived));
}

// =============================================================================
// PRIVATE MEMBER CHECKING TESTS
// =============================================================================

#[test]
fn test_private_member_brand_pattern() {
    // class A { private __brand_a: void }
    // class B { private __brand_b: void }
    // Even with same structure, different brands make them incompatible
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand_a = interner.intern_string("__brand_a");
    let brand_b = interner.intern_string("__brand_b");
    let value = interner.intern_string("value");

    let class_a = interner.object(vec![
        PropertyInfo::new(brand_a, TypeId::VOID),
        PropertyInfo::new(value, TypeId::STRING),
    ]);

    let class_b = interner.object(vec![
        PropertyInfo::new(brand_b, TypeId::VOID),
        PropertyInfo::new(value, TypeId::STRING),
    ]);

    // Different brands - not subtypes
    assert!(!checker.is_subtype_of(class_a, class_b));
    assert!(!checker.is_subtype_of(class_b, class_a));
}

#[test]
fn test_private_member_same_brand() {
    // Same brand property makes classes equivalent
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand = interner.intern_string("__brand");
    let value = interner.intern_string("value");

    let class1 = interner.object(vec![
        PropertyInfo::new(brand, TypeId::VOID),
        PropertyInfo::new(value, TypeId::STRING),
    ]);

    let class2 = interner.object(vec![
        PropertyInfo::new(brand, TypeId::VOID),
        PropertyInfo::new(value, TypeId::STRING),
    ]);

    // Same brand - subtypes of each other
    assert!(checker.is_subtype_of(class1, class2));
    assert!(checker.is_subtype_of(class2, class1));
}

#[test]
fn test_private_member_derived_inherits_brand() {
    // class Base { private __brand: void }
    // class Derived extends Base { extra: number }
    // Derived has the brand too
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand = interner.intern_string("__brand");
    let value = interner.intern_string("value");
    let extra = interner.intern_string("extra");

    let base = interner.object(vec![
        PropertyInfo::new(brand, TypeId::VOID),
        PropertyInfo::new(value, TypeId::STRING),
    ]);

    let derived = interner.object(vec![
        PropertyInfo::new(brand, TypeId::VOID),
        PropertyInfo::new(value, TypeId::STRING),
        PropertyInfo::new(extra, TypeId::NUMBER),
    ]);

    // Derived is subtype of Base (has brand)
    assert!(checker.is_subtype_of(derived, base));
    // Base is not subtype of Derived (missing extra)
    assert!(!checker.is_subtype_of(base, derived));
}

#[test]
fn test_private_member_missing_brand() {
    // class A { private __brand: void; value: string }
    // Plain object { value: string } - no brand
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand = interner.intern_string("__brand");
    let value = interner.intern_string("value");

    let class_a = interner.object(vec![
        PropertyInfo::new(brand, TypeId::VOID),
        PropertyInfo::new(value, TypeId::STRING),
    ]);

    let plain_object = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);

    // Class is subtype of plain (has all plain properties)
    assert!(checker.is_subtype_of(class_a, plain_object));
    // Plain is not subtype of class (missing brand)
    assert!(!checker.is_subtype_of(plain_object, class_a));
}

#[test]
fn test_private_member_unique_symbol_brand() {
    // Using literal types as brands (simulating unique symbol)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand = interner.intern_string("__brand");
    let value = interner.intern_string("value");

    let brand_a_type = interner.literal_string("brand_a");
    let brand_b_type = interner.literal_string("brand_b");

    let class_a = interner.object(vec![
        PropertyInfo::new(brand, brand_a_type),
        PropertyInfo::new(value, TypeId::STRING),
    ]);

    let class_b = interner.object(vec![
        PropertyInfo::new(brand, brand_b_type),
        PropertyInfo::new(value, TypeId::STRING),
    ]);

    // Different brand values - not subtypes
    assert!(!checker.is_subtype_of(class_a, class_b));
    assert!(!checker.is_subtype_of(class_b, class_a));
}

#[test]
fn test_private_member_readonly_brand() {
    // readonly brand still works for nominal-like typing
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand = interner.intern_string("__brand");
    let value = interner.intern_string("value");

    let class_readonly = interner.object(vec![
        PropertyInfo::readonly(brand, TypeId::VOID),
        PropertyInfo::new(value, TypeId::STRING),
    ]);

    let class_writable = interner.object(vec![
        PropertyInfo::new(brand, TypeId::VOID),
        PropertyInfo::new(value, TypeId::STRING),
    ]);

    // Writable is subtype of readonly (can narrow to readonly)
    assert!(checker.is_subtype_of(class_writable, class_readonly));
}

#[test]
fn test_private_multiple_brands() {
    // Class with multiple brand properties
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand1 = interner.intern_string("__brand1");
    let brand2 = interner.intern_string("__brand2");
    let value = interner.intern_string("value");

    let class_both = interner.object(vec![
        PropertyInfo::new(brand1, TypeId::VOID),
        PropertyInfo::new(brand2, TypeId::VOID),
        PropertyInfo::new(value, TypeId::STRING),
    ]);

    let class_one = interner.object(vec![
        PropertyInfo::new(brand1, TypeId::VOID),
        PropertyInfo::new(value, TypeId::STRING),
    ]);

    // Class with both brands is subtype of class with one
    assert!(checker.is_subtype_of(class_both, class_one));
    // Not the reverse
    assert!(!checker.is_subtype_of(class_one, class_both));
}

#[test]
fn test_private_member_method_brand() {
    // Using a method as part of the class identity
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand_method = interner.intern_string("__isFoo");

    let true_return = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let class_foo = interner.object(vec![PropertyInfo::method(brand_method, true_return)]);

    let class_bar = interner.object(vec![]);

    // Foo has the brand method, Bar doesn't
    assert!(!checker.is_subtype_of(class_bar, class_foo));
    // Foo is subtype of empty
    assert!(checker.is_subtype_of(class_foo, class_bar));
}

// =============================================================================
// INTERFACE EXTENSION HIERARCHY TESTS
// =============================================================================

#[test]
fn test_interface_extends_single() {
    // interface A { a: string }
    // interface B extends A { b: number }
    // B <: A
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let interface_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let interface_b = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    // B extends A
    assert!(checker.is_subtype_of(interface_b, interface_a));
    assert!(!checker.is_subtype_of(interface_a, interface_b));
}

#[test]
fn test_interface_extends_chain() {
    // interface A { a: string }
    // interface B extends A { b: number }
    // interface C extends B { c: boolean }
    // C <: B <: A (transitive)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");
    let c_prop = interner.intern_string("c");

    let interface_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let interface_b = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let interface_c = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
        PropertyInfo::new(c_prop, TypeId::BOOLEAN),
    ]);

    // Transitive chain
    assert!(checker.is_subtype_of(interface_c, interface_b));
    assert!(checker.is_subtype_of(interface_b, interface_a));
    assert!(checker.is_subtype_of(interface_c, interface_a));
}

#[test]
fn test_interface_extends_with_method() {
    // interface A { method(): void }
    // interface B extends A { other(): string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("method");
    let other_name = interner.intern_string("other");

    let void_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let string_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let interface_a = interner.object(vec![PropertyInfo::method(method_name, void_method)]);

    let interface_b = interner.object(vec![
        PropertyInfo::method(method_name, void_method),
        PropertyInfo::method(other_name, string_method),
    ]);

    assert!(checker.is_subtype_of(interface_b, interface_a));
}

#[test]
fn test_interface_extends_override_method() {
    // interface A { method(): string }
    // interface B extends A { method(): "hello" } // narrower return
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("method");
    let hello = interner.literal_string("hello");

    let string_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let hello_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: hello,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let interface_a = interner.object(vec![PropertyInfo::method(method_name, string_method)]);

    let interface_b = interner.object(vec![PropertyInfo::method(method_name, hello_method)]);

    // B with narrower return is subtype of A
    assert!(checker.is_subtype_of(interface_b, interface_a));
}

#[test]
fn test_interface_extends_property_override() {
    // interface A { value: string | number }
    // interface B extends A { value: string } // narrower type
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let interface_a = interner.object(vec![PropertyInfo::new(value, string_or_number)]);

    let interface_b = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);

    // B with narrower property type is subtype of A
    assert!(checker.is_subtype_of(interface_b, interface_a));
}

#[test]
fn test_interface_extends_optional_to_required() {
    // interface A { value?: string }
    // interface B extends A { value: string } // making it required
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");

    let interface_a = interner.object(vec![PropertyInfo::opt(value, TypeId::STRING)]);

    let interface_b = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);

    // Required is subtype of optional
    assert!(checker.is_subtype_of(interface_b, interface_a));
}

#[test]
fn test_interface_extends_readonly_property() {
    // interface A { readonly value: string }
    // interface B extends A { value: string } // can widen readonly
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");

    let interface_a = interner.object(vec![PropertyInfo::readonly(value, TypeId::STRING)]);

    let interface_b = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);

    // Writable is subtype of readonly
    assert!(checker.is_subtype_of(interface_b, interface_a));
}

// =============================================================================
// MULTIPLE INTERFACE IMPLEMENTS TESTS
// =============================================================================

#[test]
fn test_interface_extends_multiple() {
    // interface A { a: string }
    // interface B { b: number }
    // interface C extends A, B { c: boolean }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");
    let c_prop = interner.intern_string("c");

    let interface_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let interface_b = interner.object(vec![PropertyInfo::new(b_prop, TypeId::NUMBER)]);

    let interface_c = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
        PropertyInfo::new(c_prop, TypeId::BOOLEAN),
    ]);

    // C extends both A and B
    assert!(checker.is_subtype_of(interface_c, interface_a));
    assert!(checker.is_subtype_of(interface_c, interface_b));
}

#[test]
fn test_interface_extends_multiple_with_overlap() {
    // interface A { shared: string; a: number }
    // interface B { shared: string; b: boolean }
    // interface C extends A, B {} // shared property from both
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let shared = interner.intern_string("shared");
    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let interface_a = interner.object(vec![
        PropertyInfo::new(shared, TypeId::STRING),
        PropertyInfo::new(a_prop, TypeId::NUMBER),
    ]);

    let interface_b = interner.object(vec![
        PropertyInfo::new(shared, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::BOOLEAN),
    ]);

    let interface_c = interner.object(vec![
        PropertyInfo::new(shared, TypeId::STRING),
        PropertyInfo::new(a_prop, TypeId::NUMBER),
        PropertyInfo::new(b_prop, TypeId::BOOLEAN),
    ]);

    // C extends both
    assert!(checker.is_subtype_of(interface_c, interface_a));
    assert!(checker.is_subtype_of(interface_c, interface_b));
}

#[test]
fn test_interface_extends_multiple_methods() {
    // interface Readable { read(): string }
    // interface Writable { write(s: string): void }
    // interface ReadWritable extends Readable, Writable {}
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let read = interner.intern_string("read");
    let write = interner.intern_string("write");

    let read_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let write_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("s")),
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

    let readable = interner.object(vec![PropertyInfo::method(read, read_method)]);

    let writable = interner.object(vec![PropertyInfo::method(write, write_method)]);

    let read_writable = interner.object(vec![
        PropertyInfo::method(read, read_method),
        PropertyInfo::method(write, write_method),
    ]);

    assert!(checker.is_subtype_of(read_writable, readable));
    assert!(checker.is_subtype_of(read_writable, writable));
}

#[test]
fn test_interface_diamond_extends() {
    // interface A { a: string }
    // interface B extends A { b: number }
    // interface C extends A { c: boolean }
    // interface D extends B, C {} // diamond
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");
    let c_prop = interner.intern_string("c");

    let interface_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let interface_b = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let interface_c = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(c_prop, TypeId::BOOLEAN),
    ]);

    let interface_d = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
        PropertyInfo::new(c_prop, TypeId::BOOLEAN),
    ]);

    // D extends all in diamond
    assert!(checker.is_subtype_of(interface_d, interface_a));
    assert!(checker.is_subtype_of(interface_d, interface_b));
    assert!(checker.is_subtype_of(interface_d, interface_c));
}

#[test]
fn test_interface_implements_partial() {
    // Object missing some properties from interface
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let interface_ab = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let partial = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    // Partial does not implement full interface
    assert!(!checker.is_subtype_of(partial, interface_ab));
}

#[test]
fn test_interface_implements_extra_properties() {
    // Object with extra properties still implements interface
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let extra_prop = interner.intern_string("extra");

    let interface_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let with_extra = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(extra_prop, TypeId::NUMBER),
    ]);

    // Object with extra properties implements interface
    assert!(checker.is_subtype_of(with_extra, interface_a));
}

#[test]
fn test_interface_implements_wrong_type() {
    // Object with wrong property type doesn't implement interface
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");

    let interface_string = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);

    let has_number = interner.object(vec![PropertyInfo::new(value, TypeId::NUMBER)]);

    // Wrong property type
    assert!(!checker.is_subtype_of(has_number, interface_string));
}

// =============================================================================
// INTERFACE MERGE BEHAVIOR TESTS
// =============================================================================

#[test]
fn test_interface_merge_same_properties() {
    // interface A { a: string }
    // interface A { b: number } // declaration merging
    // Merged: { a: string; b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    // First declaration
    let interface_a1 = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    // Merged interface (both declarations)
    let interface_merged = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    // Merged is subtype of first declaration
    assert!(checker.is_subtype_of(interface_merged, interface_a1));
    // But not the reverse
    assert!(!checker.is_subtype_of(interface_a1, interface_merged));
}

#[test]
fn test_interface_merge_method_overloads() {
    // interface A { method(x: string): void }
    // interface A { method(x: number): void }
    // Merged should have both overloads
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("method");

    let string_method = interner.function(FunctionShape {
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

    let number_method = interner.function(FunctionShape {
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

    let interface_string = interner.object(vec![PropertyInfo::method(method_name, string_method)]);

    let interface_number = interner.object(vec![PropertyInfo::method(method_name, number_method)]);

    // Different signatures - not subtypes of each other
    assert!(!checker.is_subtype_of(interface_string, interface_number));
    assert!(!checker.is_subtype_of(interface_number, interface_string));
}

#[test]
fn test_interface_merge_compatible_properties() {
    // interface A { value: string | number }
    // interface A { value: string } // narrower - compatible in merge context
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let interface_wide = interner.object(vec![PropertyInfo::new(value, string_or_number)]);

    let interface_narrow = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);

    // Narrow is subtype of wide
    assert!(checker.is_subtype_of(interface_narrow, interface_wide));
}

