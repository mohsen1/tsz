//! Comprehensive tests for class type operations.
//!
//! These tests verify TypeScript's class type behavior:
//! - Class instance types
//! - Class static types
//! - Constructor types
//! - Class inheritance
//! - Method types

use super::*;
use crate::intern::TypeInterner;
use crate::subtype::SubtypeChecker;
use crate::types::{FunctionShape, ParamInfo, PropertyInfo, TypeData};

// =============================================================================
// Basic Class Instance Tests
// =============================================================================

#[test]
fn test_class_instance_with_properties() {
    let interner = TypeInterner::new();

    let instance_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("value"), TypeId::NUMBER),
    ]);

    if let Some(TypeData::Object(_)) = interner.lookup(instance_type) {
        // Good - class instance type created
    } else {
        panic!("Expected object type for class instance");
    }
}

#[test]
fn test_class_instance_with_method() {
    let interner = TypeInterner::new();

    let method_type = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let instance_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("getValue"),
        method_type,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(instance_type) {
        // Good - class instance with method
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Constructor Type Tests
// =============================================================================

#[test]
fn test_constructor_type() {
    let interner = TypeInterner::new();

    let constructor_type = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("name")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING, // Instance type would be more complex
        type_params: vec![],
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    if let Some(TypeData::Function(shape_id)) = interner.lookup(constructor_type) {
        let shape = interner.function_shape(shape_id);
        assert!(shape.is_constructor);
    } else {
        panic!("Expected function type");
    }
}

#[test]
fn test_constructor_with_no_params() {
    let interner = TypeInterner::new();

    let constructor_type = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![],
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    if let Some(TypeData::Function(shape_id)) = interner.lookup(constructor_type) {
        let shape = interner.function_shape(shape_id);
        assert!(shape.is_constructor);
        assert_eq!(shape.params.len(), 0);
    } else {
        panic!("Expected function type");
    }
}

// =============================================================================
// Class Property Types
// =============================================================================

#[test]
fn test_class_with_readonly_property() {
    let interner = TypeInterner::new();

    let mut prop = PropertyInfo::new(interner.intern_string("id"), TypeId::NUMBER);
    prop.readonly = true;

    let instance_type = interner.object(vec![prop]);

    if let Some(TypeData::Object(_)) = interner.lookup(instance_type) {
        // Good - class with readonly property
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_class_with_optional_property() {
    let interner = TypeInterner::new();

    let mut prop = PropertyInfo::new(interner.intern_string("middleName"), TypeId::STRING);
    prop.optional = true;

    let instance_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("firstName"), TypeId::STRING),
        prop,
    ]);

    if let Some(TypeData::Object(_)) = interner.lookup(instance_type) {
        // Good - class with optional property
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_class_with_private_property() {
    let interner = TypeInterner::new();

    let mut prop = PropertyInfo::new(interner.intern_string("internal"), TypeId::NUMBER);
    prop.visibility = crate::types::Visibility::Private;

    let instance_type = interner.object(vec![prop]);

    if let Some(TypeData::Object(_)) = interner.lookup(instance_type) {
        // Good - class with private property
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_class_with_protected_property() {
    let interner = TypeInterner::new();

    let mut prop = PropertyInfo::new(interner.intern_string("protected"), TypeId::NUMBER);
    prop.visibility = crate::types::Visibility::Protected;

    let instance_type = interner.object(vec![prop]);

    if let Some(TypeData::Object(_)) = interner.lookup(instance_type) {
        // Good - class with protected property
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Class Inheritance Tests
// =============================================================================

#[test]
fn test_class_extends_another() {
    let interner = TypeInterner::new();

    // Base class
    let base_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("baseMethod"),
        TypeId::STRING,
    )]);

    // Derived class adds more properties
    let derived_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("baseMethod"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("derivedMethod"), TypeId::NUMBER),
    ]);

    // Derived should be subtype of base
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(derived_type, base_type),
        "Derived class should be subtype of base"
    );
}

#[test]
fn test_class_with_overridden_method() {
    let interner = TypeInterner::new();

    // Base method returns string
    let base_method = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // Derived method returns more specific string literal
    let hello = interner.literal_string("hello");
    let derived_method = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: hello,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // Derived method should be subtype of base (return type covariance)
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(derived_method, base_method),
        "Overridden method with narrower return should be subtype"
    );
}

// =============================================================================
// Class with Generics Tests
// =============================================================================

#[test]
fn test_generic_class_instantiation() {
    let interner = TypeInterner::new();

    // Generic class instance<T> with property value: T
    let type_param_info = crate::types::TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let type_param = interner.intern(TypeData::TypeParameter(type_param_info));

    let _instance_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        type_param,
    )]);

    // Good - generic class instance type created
}

#[test]
fn test_class_with_multiple_type_params() {
    let interner = TypeInterner::new();

    // Class<K, V> with properties key: K, value: V
    let k_info = crate::types::TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let v_info = crate::types::TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let k_param = interner.intern(TypeData::TypeParameter(k_info));
    let v_param = interner.intern(TypeData::TypeParameter(v_info));

    let _instance_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("key"), k_param),
        PropertyInfo::new(interner.intern_string("value"), v_param),
    ]);

    // Good - class with multiple type params created
}

// =============================================================================
// Class Subtype Tests
// =============================================================================

#[test]
fn test_same_class_is_subtype_of_itself() {
    let interner = TypeInterner::new();

    let class_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(class_type, class_type),
        "Class should be subtype of itself"
    );
}

#[test]
fn test_class_not_subtype_of_unrelated_class() {
    let interner = TypeInterner::new();

    let class_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let class_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::STRING,
    )]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(class_a, class_b),
        "Unrelated classes should not be subtypes"
    );
}

#[test]
fn test_class_assignable_to_object() {
    let interner = TypeInterner::new();

    let class_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let empty_object = interner.object(vec![]);

    // Class with properties is a subtype of object
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(class_type, empty_object),
        "Class instance should be subtype of object"
    );
}

// =============================================================================
// Class with Accessors Tests
// =============================================================================

#[test]
fn test_class_with_getter() {
    let interner = TypeInterner::new();

    // Getter is a method that returns a type
    let getter_type = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let instance_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("count"),
        getter_type,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(instance_type) {
        // Good - class with getter
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn test_class_with_setter() {
    let interner = TypeInterner::new();

    // Setter is a method that takes a parameter
    let setter_type = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let instance_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("count"),
        setter_type,
    )]);

    if let Some(TypeData::Object(_)) = interner.lookup(instance_type) {
        // Good - class with setter
    } else {
        panic!("Expected object type");
    }
}

// =============================================================================
// Class with Static Members Tests
// =============================================================================

#[test]
fn test_static_method_type() {
    let interner = TypeInterner::new();

    // Static method doesn't have this_type
    let static_method = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false, // Static methods are not instance methods
    });

    let _static_side = interner.object(vec![PropertyInfo::new(
        interner.intern_string("create"),
        static_method,
    )]);

    // Good - static method type created
}

// =============================================================================
// Class Identity Tests
// =============================================================================

#[test]
fn test_class_identity_stability() {
    let interner = TypeInterner::new();

    let props = vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )];

    let class1 = interner.object(props.clone());
    let class2 = interner.object(props);

    assert_eq!(
        class1, class2,
        "Same class construction should produce same TypeId"
    );
}

// =============================================================================
// Class with Index Signature Tests
// =============================================================================

#[test]
fn test_class_with_string_index() {
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(crate::types::ObjectShape {
        symbol: None,
        flags: crate::types::ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("known"),
            TypeId::STRING,
        )],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    // object_with_index returns ObjectWithIndex variant
    if let Some(TypeData::ObjectWithIndex(_)) = interner.lookup(obj) {
        // Good - class with string index
    } else {
        panic!("Expected object with index type");
    }
}

// =============================================================================
// Class with any/never Tests
// =============================================================================

#[test]
fn test_class_assignable_to_any() {
    let interner = TypeInterner::new();

    let class_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(class_type, TypeId::ANY),
        "Class should be subtype of any"
    );
}

#[test]
fn test_never_assignable_to_class() {
    let interner = TypeInterner::new();

    let class_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(TypeId::NEVER, class_type),
        "never should be subtype of class"
    );
}

// =============================================================================
// Abstract Class Tests (structural representation)
// =============================================================================

#[test]
fn test_abstract_class_with_abstract_method() {
    let interner = TypeInterner::new();

    // Abstract method signature
    let abstract_method = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let _abstract_class = interner.object(vec![PropertyInfo::new(
        interner.intern_string("abstractMethod"),
        abstract_method,
    )]);

    // Good - abstract class representation created
}
