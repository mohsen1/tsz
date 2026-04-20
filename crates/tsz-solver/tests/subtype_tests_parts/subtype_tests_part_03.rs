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
#[test]
fn test_interface_merge_global_augmentation() {
    // Simulating global augmentation:
    // interface Window { myProp: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let document = interner.intern_string("document");
    let my_prop = interner.intern_string("myProp");

    // Original Window
    let window_original = interner.object(vec![PropertyInfo::new(document, TypeId::STRING)]);

    // Augmented Window
    let window_augmented = interner.object(vec![
        PropertyInfo::new(document, TypeId::STRING),
        PropertyInfo::new(my_prop, TypeId::STRING),
    ]);

    // Augmented is subtype of original
    assert!(checker.is_subtype_of(window_augmented, window_original));
}
#[test]
fn test_interface_merge_namespace_merge() {
    // interface + namespace merge (modeled as object with call signature + properties)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop = interner.intern_string("prop");

    // Interface part
    let interface_part = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);

    // Another object with same structure
    let same_structure = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);

    // Same structure - mutual subtypes
    assert!(checker.is_subtype_of(interface_part, same_structure));
    assert!(checker.is_subtype_of(same_structure, interface_part));
}
#[test]
fn test_interface_merge_multiple_files() {
    // Simulating interface merged from multiple files
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let file1_prop = interner.intern_string("fromFile1");
    let file2_prop = interner.intern_string("fromFile2");

    // What file1 sees
    let file1_view = interner.object(vec![PropertyInfo::new(file1_prop, TypeId::STRING)]);

    // Fully merged
    let merged = interner.object(vec![
        PropertyInfo::new(file1_prop, TypeId::STRING),
        PropertyInfo::new(file2_prop, TypeId::NUMBER),
    ]);

    // Merged is subtype of partial view
    assert!(checker.is_subtype_of(merged, file1_view));
}
#[test]
fn test_interface_merge_empty_interface() {
    // interface A {}
    // interface A { prop: string }
    // Merged: { prop: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop = interner.intern_string("prop");

    let empty = interner.object(vec![]);

    let with_prop = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);

    // Both subtype of empty
    assert!(checker.is_subtype_of(with_prop, empty));
    assert!(checker.is_subtype_of(empty, empty));
}

// =============================================================================
// INTERFACE VS TYPE ALIAS COMPATIBILITY TESTS
// =============================================================================
#[test]
fn test_interface_vs_type_alias_same_structure() {
    // interface I { a: string }
    // type T = { a: string }
    // Both should be compatible
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");

    // Interface
    let interface_i = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    // Type alias (same structure)
    let type_t = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    // Mutual subtypes
    assert!(checker.is_subtype_of(interface_i, type_t));
    assert!(checker.is_subtype_of(type_t, interface_i));
}
#[test]
fn test_interface_vs_type_alias_with_methods() {
    // interface I { method(): void }
    // type T = { method(): void }
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

    let interface_i = interner.object(vec![PropertyInfo::method(method_name, void_method)]);

    let type_t = interner.object(vec![PropertyInfo::method(method_name, void_method)]);

    // Mutual subtypes
    assert!(checker.is_subtype_of(interface_i, type_t));
    assert!(checker.is_subtype_of(type_t, interface_i));
}
#[test]
fn test_interface_vs_intersection_type() {
    // interface I { a: string; b: number }
    // type T = { a: string } & { b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let interface_i = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let obj_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_prop, TypeId::NUMBER)]);

    let type_intersection = interner.intersection(vec![obj_a, obj_b]);

    // Interface should be subtype of intersection (has all properties)
    assert!(checker.is_subtype_of(interface_i, type_intersection));
}
#[test]
fn test_interface_vs_type_alias_optional() {
    // interface I { value?: string }
    // type T = { value?: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");

    let interface_i = interner.object(vec![PropertyInfo::opt(value, TypeId::STRING)]);

    let type_t = interner.object(vec![PropertyInfo::opt(value, TypeId::STRING)]);

    // Mutual subtypes
    assert!(checker.is_subtype_of(interface_i, type_t));
    assert!(checker.is_subtype_of(type_t, interface_i));
}
#[test]
fn test_interface_vs_type_alias_readonly() {
    // interface I { readonly value: string }
    // type T = { readonly value: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");

    let interface_i = interner.object(vec![PropertyInfo::readonly(value, TypeId::STRING)]);

    let type_t = interner.object(vec![PropertyInfo::readonly(value, TypeId::STRING)]);

    // Mutual subtypes
    assert!(checker.is_subtype_of(interface_i, type_t));
    assert!(checker.is_subtype_of(type_t, interface_i));
}
#[test]
fn test_interface_vs_type_alias_index_signature() {
    // interface I { [key: string]: number }
    // type T = { [key: string]: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let interface_i = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let type_t = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Same structure
    assert!(checker.is_subtype_of(interface_i, type_t));
    assert!(checker.is_subtype_of(type_t, interface_i));
}
#[test]
fn test_interface_extends_type_alias() {
    // type Base = { a: string }
    // interface Derived extends Base { b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let type_base = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let interface_derived = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    // Interface extends type alias
    assert!(checker.is_subtype_of(interface_derived, type_base));
}
#[test]
fn test_type_alias_intersection_with_interface() {
    // interface I { a: string }
    // type T = I & { b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let interface_i = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let extra = interner.object(vec![PropertyInfo::new(b_prop, TypeId::NUMBER)]);

    let type_t = interner.intersection(vec![interface_i, extra]);

    // T is subtype of I (intersection contains interface)
    assert!(checker.is_subtype_of(type_t, interface_i));
}

// =============================================================================
// NEVER AS BOTTOM TYPE TESTS
// =============================================================================
#[test]
fn test_never_is_bottom_type_for_primitives() {
    // never is subtype of all primitive types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // never <: string
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::STRING));
    // never <: number
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::NUMBER));
    // never <: boolean
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::BOOLEAN));
    // never <: symbol
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::SYMBOL));
    // never <: bigint
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::BIGINT));

    // But primitives are NOT subtypes of never
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::NEVER));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, TypeId::NEVER));
    assert!(!checker.is_subtype_of(TypeId::BOOLEAN, TypeId::NEVER));
}
#[test]
fn test_never_is_bottom_type_for_object_types() {
    // never is subtype of object types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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
    }]);

    // never <: { name: string }
    assert!(checker.is_subtype_of(TypeId::NEVER, obj));
    // { name: string } is NOT subtype of never
    assert!(!checker.is_subtype_of(obj, TypeId::NEVER));
}
#[test]
fn test_never_is_bottom_type_for_function_types() {
    // never is subtype of function types
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

    // never <: (x: string) => number
    assert!(checker.is_subtype_of(TypeId::NEVER, fn_type));
    // (x: string) => number is NOT subtype of never
    assert!(!checker.is_subtype_of(fn_type, TypeId::NEVER));
}
#[test]
fn test_never_is_bottom_type_for_tuple_types() {
    // never is subtype of tuple types
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

    // never <: [string, number]
    assert!(checker.is_subtype_of(TypeId::NEVER, tuple));
    // [string, number] is NOT subtype of never
    assert!(!checker.is_subtype_of(tuple, TypeId::NEVER));
}
#[test]
fn test_never_is_bottom_type_for_union_types() {
    // never is subtype of union types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // never <: string | number
    assert!(checker.is_subtype_of(TypeId::NEVER, union));
    // string | number is NOT subtype of never
    assert!(!checker.is_subtype_of(union, TypeId::NEVER));
}

// =============================================================================
// UNKNOWN AS TOP TYPE TESTS
// =============================================================================
#[test]
fn test_unknown_is_top_type_for_primitives() {
    // All primitive types are subtypes of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // string <: unknown
    assert!(checker.is_subtype_of(TypeId::STRING, TypeId::UNKNOWN));
    // number <: unknown
    assert!(checker.is_subtype_of(TypeId::NUMBER, TypeId::UNKNOWN));
    // boolean <: unknown
    assert!(checker.is_subtype_of(TypeId::BOOLEAN, TypeId::UNKNOWN));
    // symbol <: unknown
    assert!(checker.is_subtype_of(TypeId::SYMBOL, TypeId::UNKNOWN));
    // bigint <: unknown
    assert!(checker.is_subtype_of(TypeId::BIGINT, TypeId::UNKNOWN));

    // But unknown is NOT subtype of primitives
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, TypeId::STRING));
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, TypeId::NUMBER));
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, TypeId::BOOLEAN));
}
#[test]
fn test_unknown_is_top_type_for_object_types() {
    // Object types are subtypes of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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
    }]);

    // { name: string } <: unknown
    assert!(checker.is_subtype_of(obj, TypeId::UNKNOWN));
    // unknown is NOT subtype of { name: string }
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, obj));
}
#[test]
fn test_unknown_is_top_type_for_function_types() {
    // Function types are subtypes of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_type = interner.function(FunctionShape {
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
        is_constructor: false,
        is_method: false,
    });

    // (x: number) => string <: unknown
    assert!(checker.is_subtype_of(fn_type, TypeId::UNKNOWN));
    // unknown is NOT subtype of (x: number) => string
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, fn_type));
}
#[test]
fn test_unknown_is_top_type_for_tuple_types() {
    // Tuple types are subtypes of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::BOOLEAN,
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

    // [boolean, string] <: unknown
    assert!(checker.is_subtype_of(tuple, TypeId::UNKNOWN));
    // unknown is NOT subtype of [boolean, string]
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, tuple));
}
#[test]
fn test_unknown_is_top_type_for_never() {
    // never is subtype of unknown (bottom <: top)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // never <: unknown
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::UNKNOWN));
    // unknown is NOT subtype of never
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, TypeId::NEVER));
}

// =============================================================================
// UNION WITH NEVER SIMPLIFICATION TESTS
// =============================================================================
#[test]
fn test_union_never_with_primitive_simplifies() {
    // T | never simplifies to T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // string | never should behave like string
    let union_with_never = interner.union(vec![TypeId::STRING, TypeId::NEVER]);

    // string | never <: string (via simplification)
    assert!(checker.is_subtype_of(union_with_never, TypeId::STRING));
    // string <: string | never
    assert!(checker.is_subtype_of(TypeId::STRING, union_with_never));
}
#[test]
fn test_union_never_with_multiple_types_simplifies() {
    // (A | B | never) should behave like (A | B)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_with_never = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::NEVER]);
    let union_without_never = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // (string | number | never) <: (string | number)
    assert!(checker.is_subtype_of(union_with_never, union_without_never));
    // (string | number) <: (string | number | never)
    assert!(checker.is_subtype_of(union_without_never, union_with_never));
}
#[test]
fn test_union_never_with_object_simplifies() {
    // { x: T } | never should behave like { x: T }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::NUMBER)]);

    let union_with_never = interner.union(vec![obj, TypeId::NEVER]);

    // { x: number } | never <: { x: number }
    assert!(checker.is_subtype_of(union_with_never, obj));
    // { x: number } <: { x: number } | never
    assert!(checker.is_subtype_of(obj, union_with_never));
}
#[test]
fn test_union_only_never_remains_never() {
    // never | never should still be never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_of_nevers = interner.union(vec![TypeId::NEVER, TypeId::NEVER]);

    // never | never <: never
    assert!(checker.is_subtype_of(union_of_nevers, TypeId::NEVER));
    // never <: never | never
    assert!(checker.is_subtype_of(TypeId::NEVER, union_of_nevers));
}
#[test]
fn test_union_never_first_position_simplifies() {
    // never | T should behave like T (never in first position)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_never_first = interner.union(vec![TypeId::NEVER, TypeId::BOOLEAN]);

    // never | boolean <: boolean
    assert!(checker.is_subtype_of(union_never_first, TypeId::BOOLEAN));
    // boolean <: never | boolean
    assert!(checker.is_subtype_of(TypeId::BOOLEAN, union_never_first));
}

// =============================================================================
// INTERSECTION WITH UNKNOWN SIMPLIFICATION TESTS
// =============================================================================
#[test]
fn test_intersection_unknown_with_primitive_simplifies() {
    // T & unknown simplifies to T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::UNKNOWN]);

    // string & unknown <: string
    assert!(checker.is_subtype_of(intersection, TypeId::STRING));
    // string <: string & unknown
    assert!(checker.is_subtype_of(TypeId::STRING, intersection));
}
#[test]
fn test_intersection_unknown_with_object_simplifies() {
    // { x: T } & unknown should behave like { x: T }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_name, TypeId::STRING)]);

    let intersection = interner.intersection(vec![obj, TypeId::UNKNOWN]);

    // { x: string } & unknown <: { x: string }
    assert!(checker.is_subtype_of(intersection, obj));
    // { x: string } <: { x: string } & unknown
    assert!(checker.is_subtype_of(obj, intersection));
}
#[test]
fn test_intersection_unknown_with_function_simplifies() {
    // ((x: T) => U) & unknown should behave like (x: T) => U
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
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let intersection = interner.intersection(vec![fn_type, TypeId::UNKNOWN]);

    // ((x: string) => boolean) & unknown <: (x: string) => boolean
    assert!(checker.is_subtype_of(intersection, fn_type));
    // (x: string) => boolean <: ((x: string) => boolean) & unknown
    assert!(checker.is_subtype_of(fn_type, intersection));
}
#[test]
fn test_intersection_unknown_first_position_simplifies() {
    // unknown & T should behave like T (unknown in first position)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let intersection = interner.intersection(vec![TypeId::UNKNOWN, TypeId::NUMBER]);

    // unknown & number <: number
    assert!(checker.is_subtype_of(intersection, TypeId::NUMBER));
    // number <: unknown & number
    assert!(checker.is_subtype_of(TypeId::NUMBER, intersection));
}
#[test]
fn test_intersection_multiple_unknowns_simplifies() {
    // unknown & unknown & T should behave like T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let intersection =
        interner.intersection(vec![TypeId::UNKNOWN, TypeId::STRING, TypeId::UNKNOWN]);

    // unknown & string & unknown <: string
    assert!(checker.is_subtype_of(intersection, TypeId::STRING));
    // string <: unknown & string & unknown
    assert!(checker.is_subtype_of(TypeId::STRING, intersection));
}

// =============================================================================
// NUMERIC ENUM ASSIGNABILITY TESTS
// =============================================================================
#[test]
fn test_numeric_enum_member_to_number() {
    // enum E { A = 0, B = 1 }
    // E.A (literal 0) is subtype of number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(0.0);
    let enum_b = interner.literal_number(1.0);

    // Numeric enum members are subtypes of number
    assert!(checker.is_subtype_of(enum_a, TypeId::NUMBER));
    assert!(checker.is_subtype_of(enum_b, TypeId::NUMBER));
}
#[test]
fn test_numeric_enum_union() {
    // enum E { A = 0, B = 1, C = 2 }
    // E is union of 0 | 1 | 2
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(0.0);
    let enum_b = interner.literal_number(1.0);
    let enum_c = interner.literal_number(2.0);

    let enum_type = interner.union(vec![enum_a, enum_b, enum_c]);

    // Enum type is subtype of number
    assert!(checker.is_subtype_of(enum_type, TypeId::NUMBER));

    // Individual members are subtypes of enum type
    assert!(checker.is_subtype_of(enum_a, enum_type));
    assert!(checker.is_subtype_of(enum_b, enum_type));
    assert!(checker.is_subtype_of(enum_c, enum_type));
}
#[test]
fn test_numeric_enum_same_values_equal() {
    // enum E1 { A = 0 }
    // enum E2 { A = 0 }
    // Same literal values are equal structurally
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let e1_a = interner.literal_number(0.0);
    let e2_a = interner.literal_number(0.0);

    // Same literal values are equal
    assert!(checker.is_subtype_of(e1_a, e2_a));
    assert!(checker.is_subtype_of(e2_a, e1_a));
}
#[test]
fn test_numeric_enum_computed_values() {
    // enum E { A = 1, B = 2, C = A + B } // C = 3
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(1.0);
    let enum_b = interner.literal_number(2.0);
    let enum_c = interner.literal_number(3.0);

    let enum_type = interner.union(vec![enum_a, enum_b, enum_c]);

    // All computed values are part of enum
    assert!(checker.is_subtype_of(enum_c, enum_type));
    assert!(checker.is_subtype_of(enum_type, TypeId::NUMBER));
}
#[test]
fn test_numeric_enum_negative_values() {
    // enum E { A = -1, B = 0, C = 1 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(-1.0);
    let enum_b = interner.literal_number(0.0);
    let enum_c = interner.literal_number(1.0);

    let enum_type = interner.union(vec![enum_a, enum_b, enum_c]);

    // Negative values work correctly
    assert!(checker.is_subtype_of(enum_a, TypeId::NUMBER));
    assert!(checker.is_subtype_of(enum_a, enum_type));
}
#[test]
fn test_number_not_subtype_of_numeric_enum() {
    // number is not subtype of enum (enum is more specific)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(0.0);
    let enum_b = interner.literal_number(1.0);
    let enum_type = interner.union(vec![enum_a, enum_b]);

    // number is not subtype of specific enum union
    assert!(!checker.is_subtype_of(TypeId::NUMBER, enum_type));
}
#[test]
fn test_numeric_enum_single_member() {
    // enum E { Only = 42 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let only = interner.literal_number(42.0);

    // Single member enum
    assert!(checker.is_subtype_of(only, TypeId::NUMBER));

    // Other number literals are not the enum value
    let other = interner.literal_number(43.0);
    assert!(!checker.is_subtype_of(other, only));
}

// =============================================================================
// STRING ENUM ASSIGNABILITY TESTS
// =============================================================================
#[test]
fn test_string_enum_member_to_string() {
    // enum E { A = "a", B = "b" }
    // E.A (literal "a") is subtype of string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_string("a");
    let enum_b = interner.literal_string("b");

    // String enum members are subtypes of string
    assert!(checker.is_subtype_of(enum_a, TypeId::STRING));
    assert!(checker.is_subtype_of(enum_b, TypeId::STRING));
}
#[test]
fn test_string_enum_union() {
    // enum Direction { Up = "UP", Down = "DOWN", Left = "LEFT", Right = "RIGHT" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let up = interner.literal_string("UP");
    let down = interner.literal_string("DOWN");
    let left = interner.literal_string("LEFT");
    let right = interner.literal_string("RIGHT");

    let direction = interner.union(vec![up, down, left, right]);

    // Enum type is subtype of string
    assert!(checker.is_subtype_of(direction, TypeId::STRING));

    // Individual members are subtypes of enum type
    assert!(checker.is_subtype_of(up, direction));
    assert!(checker.is_subtype_of(down, direction));
}
#[test]
fn test_string_not_subtype_of_string_enum() {
    // string is not subtype of string enum
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let enum_type = interner.union(vec![a, b]);

    // string is not subtype of specific string enum
    assert!(!checker.is_subtype_of(TypeId::STRING, enum_type));
}
#[test]
fn test_string_enum_non_member_literal() {
    // Non-member string literal is not subtype of enum
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let enum_type = interner.union(vec![a, b]);

    let c = interner.literal_string("c");

    // "c" is not a member of the enum
    assert!(!checker.is_subtype_of(c, enum_type));
}
#[test]
fn test_string_enum_case_sensitive() {
    // String enums are case-sensitive
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let upper = interner.literal_string("UP");
    let lower = interner.literal_string("up");

    // Different cases are different values
    assert!(!checker.is_subtype_of(upper, lower));
    assert!(!checker.is_subtype_of(lower, upper));
}
#[test]
fn test_string_enum_empty_string() {
    // enum E { Empty = "" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty = interner.literal_string("");

    assert!(checker.is_subtype_of(empty, TypeId::STRING));
}
#[test]
fn test_string_enum_with_special_chars() {
    // enum E { Special = "hello-world_123" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let special = interner.literal_string("hello-world_123");

    assert!(checker.is_subtype_of(special, TypeId::STRING));
}

// =============================================================================
// CONST ENUM HANDLING TESTS
// =============================================================================
#[test]
fn test_const_enum_numeric_values() {
    // const enum E { A = 0, B = 1, C = 2 }
    // Const enums are inlined - same as regular numeric enum for type checking
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_number(0.0);
    let b = interner.literal_number(1.0);
    let c = interner.literal_number(2.0);

    let const_enum = interner.union(vec![a, b, c]);

    // Same behavior as regular enum
    assert!(checker.is_subtype_of(const_enum, TypeId::NUMBER));
    assert!(checker.is_subtype_of(a, const_enum));
}
#[test]
fn test_const_enum_string_values() {
    // const enum E { A = "a", B = "b" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");

    let const_enum = interner.union(vec![a, b]);

    assert!(checker.is_subtype_of(const_enum, TypeId::STRING));
    assert!(checker.is_subtype_of(a, const_enum));
}
#[test]
fn test_const_enum_computed_member() {
    // const enum E { A = 1 << 0, B = 1 << 1, C = 1 << 2 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a = interner.literal_number(1.0); // 1 << 0
    let b = interner.literal_number(2.0); // 1 << 1
    let c = interner.literal_number(4.0); // 1 << 2

    let flags_enum = interner.union(vec![a, b, c]);

    assert!(checker.is_subtype_of(flags_enum, TypeId::NUMBER));
}
#[test]
fn test_const_enum_single_value() {
    // const enum E { Only = 42 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let only = interner.literal_number(42.0);

    // Single value const enum
    assert!(checker.is_subtype_of(only, TypeId::NUMBER));
}
#[test]
fn test_const_enum_mixed_types() {
    // Testing union behavior for hypothetical mixed enum
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let num = interner.literal_number(0.0);
    let str = interner.literal_string("b");

    let mixed = interner.union(vec![num, str]);

    // Mixed enum is subtype of string | number
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(checker.is_subtype_of(mixed, string_or_number));

    // But not just string or just number
    assert!(!checker.is_subtype_of(mixed, TypeId::STRING));
    assert!(!checker.is_subtype_of(mixed, TypeId::NUMBER));
}
#[test]
fn test_const_enum_preserves_literal_types() {
    // Const enum values should preserve their literal types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let val = interner.literal_number(42.0);
    let other = interner.literal_number(42.0);

    // Same literal values are equal
    assert!(checker.is_subtype_of(val, other));
    assert!(checker.is_subtype_of(other, val));
}
#[test]
fn test_const_enum_bitwise_flags() {
    // const enum Flags { None = 0, Read = 1, Write = 2, Execute = 4, All = 7 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let none = interner.literal_number(0.0);
    let read = interner.literal_number(1.0);
    let write = interner.literal_number(2.0);
    let execute = interner.literal_number(4.0);
    let all = interner.literal_number(7.0);

    let flags = interner.union(vec![none, read, write, execute, all]);

    assert!(checker.is_subtype_of(flags, TypeId::NUMBER));
    assert!(checker.is_subtype_of(all, flags));
}

// =============================================================================
// ENUM MEMBER ACCESS TESTS
// =============================================================================
#[test]
fn test_enum_member_access_numeric() {
    // enum E { A = 0, B = 1 }
    // typeof E.A is literal type 0
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let e_a = interner.literal_number(0.0);
    let e_b = interner.literal_number(1.0);

    // E.A is distinct from E.B
    assert!(!checker.is_subtype_of(e_a, e_b));
    assert!(!checker.is_subtype_of(e_b, e_a));

    // But both are numbers
    assert!(checker.is_subtype_of(e_a, TypeId::NUMBER));
    assert!(checker.is_subtype_of(e_b, TypeId::NUMBER));
}
#[test]
fn test_literal_enum_members_with_same_def_id_are_distinct_subtypes() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_def = DefId(42);
    let enum_a = interner.intern(TypeData::Enum(enum_def, interner.literal_number(0.0)));
    let enum_b = interner.intern(TypeData::Enum(enum_def, interner.literal_number(1.0)));

    assert!(checker.is_subtype_of(enum_a, enum_a));
    assert!(checker.is_subtype_of(enum_b, enum_b));
    assert!(!checker.is_subtype_of(enum_a, enum_b));
    assert!(!checker.is_subtype_of(enum_b, enum_a));
}
#[test]
fn test_enum_member_access_string() {
    // enum E { A = "a", B = "b" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let e_a = interner.literal_string("a");
    let e_b = interner.literal_string("b");

    // E.A is distinct from E.B
    assert!(!checker.is_subtype_of(e_a, e_b));

    // Both are strings
    assert!(checker.is_subtype_of(e_a, TypeId::STRING));
    assert!(checker.is_subtype_of(e_b, TypeId::STRING));
}
#[test]
fn test_enum_member_in_object_property() {
    // interface I { status: Status.Active }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let status_prop = interner.intern_string("status");
    let active = interner.literal_string("ACTIVE");
    let inactive = interner.literal_string("INACTIVE");

    let interface_active = interner.object(vec![PropertyInfo::new(status_prop, active)]);

    let obj_active = interner.object(vec![PropertyInfo::new(status_prop, active)]);

    let obj_inactive = interner.object(vec![PropertyInfo::new(status_prop, inactive)]);

    // Object with matching status is subtype
    assert!(checker.is_subtype_of(obj_active, interface_active));

    // Object with different status is not
    assert!(!checker.is_subtype_of(obj_inactive, interface_active));
}
#[test]
fn test_enum_member_union_in_property() {
    // interface I { status: Status.Active | Status.Pending }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let status_prop = interner.intern_string("status");
    let active = interner.literal_string("ACTIVE");
    let pending = interner.literal_string("PENDING");
    let completed = interner.literal_string("COMPLETED");

    let active_or_pending = interner.union(vec![active, pending]);

    let interface_type = interner.object(vec![PropertyInfo::new(status_prop, active_or_pending)]);

    let obj_active = interner.object(vec![PropertyInfo::new(status_prop, active)]);

    let obj_completed = interner.object(vec![PropertyInfo::new(status_prop, completed)]);

    // Active matches union
    assert!(checker.is_subtype_of(obj_active, interface_type));

    // Completed does not match union
    assert!(!checker.is_subtype_of(obj_completed, interface_type));
}
#[test]
fn test_enum_member_as_function_param() {
    // function f(status: Status.Active): void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let active = interner.literal_string("ACTIVE");
    let inactive = interner.literal_string("INACTIVE");

    let fn_active_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("status")),
            type_id: active,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_inactive_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("status")),
            type_id: inactive,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Functions with different enum member params are not subtypes
    assert!(!checker.is_subtype_of(fn_active_param, fn_inactive_param));
}
#[test]
fn test_enum_member_as_return_type() {
    // function f(): Status.Active
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let active = interner.literal_string("ACTIVE");

    let fn_returns_active = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: active,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_returns_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function returning enum member is subtype of function returning string
    assert!(checker.is_subtype_of(fn_returns_active, fn_returns_string));
}
#[test]
fn test_enum_member_narrowing() {
    // Testing narrowing: if status === Status.Active, type is Status.Active
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let active = interner.literal_string("ACTIVE");
    let inactive = interner.literal_string("INACTIVE");
    let pending = interner.literal_string("PENDING");

    let status_enum = interner.union(vec![active, inactive, pending]);

    // After narrowing, active is subtype of the full enum
    assert!(checker.is_subtype_of(active, status_enum));

    // And the narrowed type is more specific
    assert!(!checker.is_subtype_of(status_enum, active));
}
#[test]
fn test_enum_reverse_mapping_numeric() {
    // Numeric enums have reverse mappings: E[0] === "A"
    // This is runtime behavior, but the type would be the key type
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // The reverse mapped value is a string (the enum key name)
    let key_name = interner.literal_string("A");

    assert!(checker.is_subtype_of(key_name, TypeId::STRING));
}
#[test]
fn test_enum_reverse_mapping_multiple_keys() {
    // enum E { A = 0, B = 1, C = 2 }
    // E[0] === "A", E[1] === "B", E[2] === "C"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("A");
    let key_b = interner.literal_string("B");
    let key_c = interner.literal_string("C");

    // All reverse mapped keys are strings
    let key_union = interner.union(vec![key_a, key_b, key_c]);

    assert!(checker.is_subtype_of(key_union, TypeId::STRING));
    assert!(checker.is_subtype_of(key_a, key_union));
}
#[test]
fn test_string_enum_no_reverse_mapping() {
    // String enums do NOT have reverse mappings
    // enum E { A = "a" } - E["a"] is undefined, not "A"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_value = interner.literal_string("a");
    let enum_key = interner.literal_string("A");

    // The key and value are distinct types
    assert!(!checker.is_subtype_of(enum_value, enum_key));
    assert!(!checker.is_subtype_of(enum_key, enum_value));
}
#[test]
fn test_heterogeneous_enum_mixed_types() {
    // enum E { A = 0, B = "b", C = 1 }
    // Heterogeneous enum: mix of string and number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(0.0);
    let enum_b = interner.literal_string("b");
    let enum_c = interner.literal_number(1.0);

    let enum_type = interner.union(vec![enum_a, enum_b, enum_c]);

    // Each member is subtype of enum
    assert!(checker.is_subtype_of(enum_a, enum_type));
    assert!(checker.is_subtype_of(enum_b, enum_type));
    assert!(checker.is_subtype_of(enum_c, enum_type));

    // Enum is subtype of string | number
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(checker.is_subtype_of(enum_type, string_or_number));

    // But not just string or just number
    assert!(!checker.is_subtype_of(enum_type, TypeId::STRING));
    assert!(!checker.is_subtype_of(enum_type, TypeId::NUMBER));
}
#[test]
fn test_const_enum_inlined_literal() {
    // const enum E { A = 1, B = 2 }
    // At type level, behaves like literals
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let const_a = interner.literal_number(1.0);
    let _const_b = interner.literal_number(2.0);

    // Const enum members maintain literal types
    assert!(checker.is_subtype_of(const_a, TypeId::NUMBER));

    // And are compatible with same literal
    let same_literal = interner.literal_number(1.0);
    assert!(checker.is_subtype_of(const_a, same_literal));
    assert!(checker.is_subtype_of(same_literal, const_a));
}
#[test]
fn test_const_enum_string_inlined() {
    // const enum E { A = "a", B = "b" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let const_a = interner.literal_string("a");
    let const_b = interner.literal_string("b");
    let const_enum = interner.union(vec![const_a, const_b]);

    // Inlined const enum values are literal types
    assert!(checker.is_subtype_of(const_a, TypeId::STRING));
    assert!(checker.is_subtype_of(const_enum, TypeId::STRING));
}
#[test]
fn test_enum_cross_compatibility_same_shape() {
    // enum E1 { A = 0, B = 1 }
    // enum E2 { X = 0, Y = 1 }
    // Structurally equivalent but nominally different
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let e1_a = interner.literal_number(0.0);
    let e1_b = interner.literal_number(1.0);
    let e1_type = interner.union(vec![e1_a, e1_b]);

    let e2_x = interner.literal_number(0.0);
    let e2_y = interner.literal_number(1.0);
    let e2_type = interner.union(vec![e2_x, e2_y]);

    // Same structure = compatible in structural type system
    assert!(checker.is_subtype_of(e1_type, e2_type));
    assert!(checker.is_subtype_of(e2_type, e1_type));

    // Individual members also compatible
    assert!(checker.is_subtype_of(e1_a, e2_x));
}
#[test]
fn test_enum_partial_overlap() {
    // enum E1 { A = 0, B = 1, C = 2 }
    // enum E2 { X = 0, Y = 1 }
    // E2 is subset of E1
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let e1_a = interner.literal_number(0.0);
    let e1_b = interner.literal_number(1.0);
    let e1_c = interner.literal_number(2.0);
    let e1_type = interner.union(vec![e1_a, e1_b, e1_c]);

    let e2_x = interner.literal_number(0.0);
    let e2_y = interner.literal_number(1.0);
    let e2_type = interner.union(vec![e2_x, e2_y]);

    // E2 <: E1 (E2 is subset)
    assert!(checker.is_subtype_of(e2_type, e1_type));

    // E1 </: E2 (E1 has extra member)
    assert!(!checker.is_subtype_of(e1_type, e2_type));
}
#[test]
fn test_enum_with_auto_increment() {
    // enum E { A, B, C } // A = 0, B = 1, C = 2 (auto-incremented)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(0.0);
    let enum_b = interner.literal_number(1.0);
    let enum_c = interner.literal_number(2.0);

    // Auto-incremented values form sequential literals
    let enum_type = interner.union(vec![enum_a, enum_b, enum_c]);

    assert!(checker.is_subtype_of(enum_a, enum_type));
    assert!(checker.is_subtype_of(enum_type, TypeId::NUMBER));
}
#[test]
fn test_enum_with_explicit_and_auto() {
    // enum E { A = 10, B, C } // A = 10, B = 11, C = 12
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(10.0);
    let enum_b = interner.literal_number(11.0);
    let enum_c = interner.literal_number(12.0);

    let enum_type = interner.union(vec![enum_a, enum_b, enum_c]);

    // All are part of enum
    assert!(checker.is_subtype_of(enum_a, enum_type));
    assert!(checker.is_subtype_of(enum_b, enum_type));
    assert!(checker.is_subtype_of(enum_c, enum_type));
}
#[test]
fn test_enum_member_in_conditional() {
    // Using enum member as conditional type extends target
    // E.A extends number ? true : false
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(0.0);

    // Enum member extends number
    assert!(checker.is_subtype_of(enum_a, TypeId::NUMBER));

    // Enum member extends same literal
    let literal_zero = interner.literal_number(0.0);
    assert!(checker.is_subtype_of(enum_a, literal_zero));
}
#[test]
fn test_const_enum_as_type_parameter_constraint() {
    // type OnlyZeroOrOne<T extends 0 | 1> = T
    // Can use const enum values as constraints
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_0 = interner.literal_number(0.0);
    let lit_1 = interner.literal_number(1.0);
    let constraint = interner.union(vec![lit_0, lit_1]);

    let lit_2 = interner.literal_number(2.0);

    // 0 and 1 satisfy constraint
    assert!(checker.is_subtype_of(lit_0, constraint));
    assert!(checker.is_subtype_of(lit_1, constraint));

    // 2 does not satisfy constraint
    assert!(!checker.is_subtype_of(lit_2, constraint));
}
#[test]
fn test_enum_keyof() {
    // keyof typeof E for numeric enum
    // enum E { A = 0, B = 1 } -> keyof typeof E = "A" | "B"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("A");
    let key_b = interner.literal_string("B");
    let keyof_enum = interner.union(vec![key_a, key_b]);

    // Keys are strings
    assert!(checker.is_subtype_of(keyof_enum, TypeId::STRING));

    // Individual keys are part of keyof
    assert!(checker.is_subtype_of(key_a, keyof_enum));
}
#[test]
fn test_enum_value_type() {
    // typeof E[keyof typeof E] for enum E { A = 0, B = 1 }
    // = 0 | 1
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let val_a = interner.literal_number(0.0);
    let val_b = interner.literal_number(1.0);
    let value_type = interner.union(vec![val_a, val_b]);

    // Value type is union of literals
    assert!(checker.is_subtype_of(value_type, TypeId::NUMBER));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, value_type));
}
#[test]
fn test_enum_with_bigint_like_value() {
    // enum E { BIG = 9007199254740991 } // MAX_SAFE_INTEGER
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let big_val = interner.literal_number(9007199254740991.0);

    // Large numbers still work
    assert!(checker.is_subtype_of(big_val, TypeId::NUMBER));
}
#[test]
fn test_enum_preserves_literal_identity() {
    // enum E { A = 1 }
    // const x: 1 = E.A; // Should be assignable
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let enum_a = interner.literal_number(1.0);
    let literal_one = interner.literal_number(1.0);

    // Enum member is same as literal
    assert!(checker.is_subtype_of(enum_a, literal_one));
    assert!(checker.is_subtype_of(literal_one, enum_a));
}
#[test]
fn test_string_enum_unicode() {
    // enum E { EMOJI = "🎉", SYMBOL = "→" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let emoji = interner.literal_string("🎉");
    let symbol = interner.literal_string("→");
    let enum_type = interner.union(vec![emoji, symbol]);

    // Unicode strings work
    assert!(checker.is_subtype_of(emoji, TypeId::STRING));
    assert!(checker.is_subtype_of(symbol, enum_type));
}
#[test]
fn test_enum_in_mapped_type_context() {
    // { [K in E]: K } where E = "a" | "b"
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    // Result object has properties "a" and "b"
    let result = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), lit_a),
        PropertyInfo::new(interner.intern_string("b"), lit_b),
    ]);

    assert!(result != TypeId::ERROR);
}

// =============================================================================
// Index Signature Tests - String/Number Keys and Intersections
// =============================================================================
// These tests cover index signature behavior including string/number keys,
// intersection of index signatures, and edge cases.
#[test]
fn test_index_signature_string_to_string() {
    // { [key: string]: number } is subtype of { [key: string]: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object_with_index(ObjectShape {
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

    let obj_b = interner.object_with_index(ObjectShape {
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

    assert!(checker.is_subtype_of(obj_a, obj_b));
}
#[test]
fn test_index_signature_number_to_number() {
    // { [key: number]: string } is subtype of { [key: number]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object_with_index(ObjectShape {
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

    let obj_b = interner.object_with_index(ObjectShape {
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

    assert!(checker.is_subtype_of(obj_a, obj_b));
}
#[test]
fn test_index_signature_covariant_value_type() {
    // { [key: string]: "a" | "b" } is subtype of { [key: string]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let literal_union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);

    let obj_specific = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: literal_union,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let obj_general = interner.object_with_index(ObjectShape {
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

    assert!(checker.is_subtype_of(obj_specific, obj_general));
    assert!(!checker.is_subtype_of(obj_general, obj_specific));
}
#[test]
fn test_index_signature_both_string_and_number() {
    // { [key: string]: any, [key: number]: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_both = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let obj_string_only = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Object with both is subtype of object with just string
    assert!(checker.is_subtype_of(obj_both, obj_string_only));
}
#[test]
fn test_index_signature_number_subtype_of_string() {
    // Number index signature value must be subtype of string index signature value
    // { [key: string]: any, [key: number]: string } - string is subtype of any
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    // This should be valid - string is subtype of any
    assert!(obj != TypeId::ERROR);
}
#[test]
fn test_index_signature_intersection_combines() {
    // { [key: string]: A } & { [key: string]: B } = { [key: string]: A & B }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object_with_index(ObjectShape {
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

    let obj_b = interner.object_with_index(ObjectShape {
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

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    // Intersection should be assignable to either
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_b));
}
#[test]
fn test_index_signature_with_properties() {
    // { x: number, [key: string]: number | string }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let union_type = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("x"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: union_type,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Object has both property and index signature
    assert!(obj != TypeId::ERROR);
}
#[test]
fn test_index_signature_property_must_match_index() {
    // Property type must be subtype of index signature value type
    // { x: string, [key: string]: string } is valid
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let obj_valid = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("x"),
            TypeId::STRING,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(obj_valid != TypeId::ERROR);
}
#[test]
fn test_index_signature_readonly_to_mutable() {
    // { readonly [key: string]: T } is NOT subtype of { [key: string]: T }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_readonly = interner.object_with_index(ObjectShape {
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

    let obj_mutable = interner.object_with_index(ObjectShape {
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

    // Per tsc behavior, readonly on index signatures does NOT affect assignability.
    // A readonly index signature IS assignable to a mutable index signature.
    assert!(checker.is_subtype_of(obj_readonly, obj_mutable));
}
#[test]
fn test_index_signature_mutable_to_readonly() {
    // { [key: string]: T } is subtype of { readonly [key: string]: T }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_mutable = interner.object_with_index(ObjectShape {
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

    let obj_readonly = interner.object_with_index(ObjectShape {
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

    // Mutable is assignable to readonly (can read)
    assert!(checker.is_subtype_of(obj_mutable, obj_readonly));
}
#[test]
fn test_index_signature_union_value_subtyping() {
    // { [key: string]: A | B } - specific member is subtype of union
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_value = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj = interner.object_with_index(ObjectShape {
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

    let obj_string = interner.object_with_index(ObjectShape {
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

    // { [k: string]: string } is subtype of { [k: string]: string | number }
    assert!(checker.is_subtype_of(obj_string, obj));
}
#[test]
fn test_index_signature_intersection_value() {
    // { [key: string]: A & B }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let intersection_value = interner.intersection(vec![obj_a, obj_b]);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: intersection_value,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Object with intersection value type
    assert!(obj != TypeId::ERROR);
}
#[test]
fn test_index_signature_empty_object_to_indexed() {
    // {} is NOT subtype of { [key: string]: T } unless T allows undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty_obj = interner.object(vec![]);

    let indexed_obj = interner.object_with_index(ObjectShape {
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

    // Empty object may not be subtype of indexed object
    // This depends on strictness settings
    let result = checker.is_subtype_of(empty_obj, indexed_obj);
    // Just ensure it doesn't panic
    let _ = result;
}
#[test]
fn test_index_signature_object_with_extra_props() {
    // { a: number, b: string } is subtype of { [key: string]: number | string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_with_props = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);

    let union_value = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let indexed_obj = interner.object_with_index(ObjectShape {
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

    assert!(checker.is_subtype_of(obj_with_props, indexed_obj));
}
#[test]
fn test_index_signature_numeric_string_key() {
    // { "0": T, "1": T } should be compatible with { [key: number]: T }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_with_numeric_props = interner.object(vec![
        PropertyInfo::new(interner.intern_string("0"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("1"), TypeId::STRING),
    ]);

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

    // Numeric string properties should be compatible
    assert!(checker.is_subtype_of(obj_with_numeric_props, number_indexed));
}
#[test]
fn test_index_signature_any_value() {
    // { [key: string]: any } accepts anything
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_any = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let obj_with_props = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::BOOLEAN,
    )]);

    assert!(checker.is_subtype_of(obj_with_props, indexed_any));
}
#[test]
fn test_object_with_named_props_satisfies_number_index_any() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("one"),
        TypeId::NUMBER,
    )]);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(checker.is_subtype_of(source, target));
    assert_eq!(checker.explain_failure(source, target), None);
}
#[test]
fn test_string_is_not_subtype_of_string_index_any() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(!checker.is_subtype_of(TypeId::STRING, target));
    assert!(matches!(
        checker.explain_failure(TypeId::STRING, target),
        Some(SubtypeFailureReason::TypeMismatch { .. })
    ));
}
#[test]
fn test_boolean_is_not_subtype_of_number_index_any() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::ANY,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!checker.is_subtype_of(TypeId::BOOLEAN, target));
    assert!(matches!(
        checker.explain_failure(TypeId::BOOLEAN, target),
        Some(SubtypeFailureReason::TypeMismatch { .. })
    ));
}
#[test]
fn test_index_signature_unknown_value() {
    // { [key: string]: unknown } - safe unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_unknown = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::UNKNOWN,
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

    // { [k: string]: string } is subtype of { [k: string]: unknown }
    assert!(checker.is_subtype_of(indexed_string, indexed_unknown));
}
#[test]
fn test_index_signature_never_value() {
    // { [key: string]: never } - impossible to add properties
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_never = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NEVER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // Empty object might be subtype of { [k: string]: never }
    let empty_obj = interner.object(vec![]);
    let result = checker.is_subtype_of(empty_obj, indexed_never);
    // Just ensure it handles the case
    let _ = result;
}
#[test]
fn test_index_signature_function_value() {
    // { [key: string]: () => void }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let indexed_fn = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: fn_type,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(indexed_fn != TypeId::ERROR);
}
#[test]
fn test_index_signature_array_value() {
    // { [key: string]: T[] }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let array_type = interner.array(TypeId::NUMBER);

    let indexed_array = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: array_type,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(indexed_array != TypeId::ERROR);
}
#[test]
fn test_index_signature_tuple_value() {
    // { [key: number]: [string, number] }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let tuple_type = interner.tuple(vec![
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

    let indexed_tuple = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: tuple_type,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(indexed_tuple != TypeId::ERROR);
}
#[test]
fn test_index_signature_nested_object_value() {
    // { [key: string]: { x: number } }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let nested_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let indexed_nested = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: nested_obj,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    assert!(indexed_nested != TypeId::ERROR);
}
#[test]
fn test_index_signature_intersection_objects() {
    // { [key: string]: A } & { x: B }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let indexed_obj = interner.object_with_index(ObjectShape {
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

    let prop_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let intersection = interner.intersection(vec![indexed_obj, prop_obj]);

    // Intersection should have both index signature and property
    assert!(intersection != TypeId::ERROR);
}
#[test]
fn test_index_signature_literal_key_subset() {
    // { [key: "a" | "b"]: T } - template literal pattern index
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let _literal_keys = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);

    // This would be like a Pick pattern or mapped type result
    let obj_with_literal_props = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    assert!(obj_with_literal_props != TypeId::ERROR);
}

// =============================================================================
// COVARIANCE / CONTRAVARIANCE EDGE CASE TESTS
// =============================================================================
#[test]
fn test_variance_nested_function_contravariance() {
    // (f: (x: string) => void) => void  <:  (f: (x: string | number) => void) => void
    // The callback parameter is contravariant, so callbacks with wider params are subtypes
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Callback with narrow param
    let narrow_callback = interner.function(FunctionShape {
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

    // Callback with wide param
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
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

    // HOF taking narrow callback
    let hof_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: narrow_callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // HOF taking wide callback
    let hof_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: wide_callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // HOF with wide callback <: HOF with narrow callback (double contravariance = covariance)
    // In strict variance: hof_wide <: hof_narrow only
    // Current behavior: bivariant for callback parameters - both directions work
    assert!(!checker.is_subtype_of(hof_wide, hof_narrow));
    assert!(checker.is_subtype_of(hof_narrow, hof_wide));
}
#[test]
fn test_variance_callback_return_type() {
    // (f: () => string) => void  vs  (f: () => string | number) => void
    // Callback return is covariant within callback, but callback is contravariant
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Callback returning narrow type
    let narrow_returning = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Callback returning wide type
    let wide_return = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_returning = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: wide_return,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // HOF taking narrow-returning callback
    let hof_narrow_return = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: narrow_returning,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // HOF taking wide-returning callback
    let hof_wide_return = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: wide_returning,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // HOF with narrow-returning <: HOF with wide-returning (contravariant flip of covariant)
    // In strict variance: hof_narrow_return <: hof_wide_return only
    // Current behavior: bivariant for callback parameters - both directions work
    assert!(!checker.is_subtype_of(hof_narrow_return, hof_wide_return));
    assert!(checker.is_subtype_of(hof_wide_return, hof_narrow_return));
}
#[test]
fn test_variance_readonly_property_covariant() {
    // { readonly x: string } <: { readonly x: string | number }
    // Readonly properties are covariant (only read, never written)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrow_readonly = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    let wide_readonly = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        wide_type,
    )]);

    // Narrow readonly <: wide readonly (covariant)
    assert!(checker.is_subtype_of(narrow_readonly, wide_readonly));
}
#[test]
fn test_variance_mutable_property_invariant() {
    // { x: string } should not be subtype of { x: string | number } (invariant for mutable)
    // In TypeScript this is unsound - arrays are covariant even when mutable
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrow_mutable = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::STRING,
    )]);

    let wide_mutable = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        wide_type,
    )]);

    // TypeScript allows this (unsound covariance), so we match behavior
    assert!(checker.is_subtype_of(narrow_mutable, wide_mutable));
}
#[test]
fn test_variance_tuple_element_covariant() {
    // [string, number] <: [string | number, number | boolean]
    // Tuple elements are covariant for reading
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_first = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_second = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);

    let narrow_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            name: None,
            rest: false,
        },
    ]);

    let wide_tuple = interner.tuple(vec![
        TupleElement {
            type_id: wide_first,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: wide_second,
            optional: false,
            name: None,
            rest: false,
        },
    ]);

    // Narrow tuple <: wide tuple (covariant elements)
    assert!(checker.is_subtype_of(narrow_tuple, wide_tuple));
    assert!(!checker.is_subtype_of(wide_tuple, narrow_tuple));
}
