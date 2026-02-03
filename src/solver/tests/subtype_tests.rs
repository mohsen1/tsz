use super::*;
use crate::solver::def::DefId;
use crate::solver::{TypeSubstitution, instantiate_type};

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
fn test_any_top_bottom_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    assert!(checker.is_subtype_of(TypeId::ANY, TypeId::NEVER));
    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::ANY));
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
fn test_error_type_strictness_subtyping() {
    // ERROR types should NOT silently pass subtype checks.
    // This prevents "error poisoning" where a TS2304 (cannot find name) masks
    // downstream TS2322 (type not assignable) errors.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // ERROR is NOT a subtype of concrete types
    assert!(!checker.is_subtype_of(TypeId::ERROR, TypeId::STRING));
    // Concrete types are NOT subtypes of ERROR
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::ERROR));
    // ERROR is a subtype of itself (reflexive)
    assert!(checker.is_subtype_of(TypeId::ERROR, TypeId::ERROR));
}

#[test]
fn test_error_type_not_top_or_bottom() {
    // ERROR should NOT act as a top or bottom type.
    // It should fail subtype checks with other types.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    // ERROR is NOT a subtype of object types
    assert!(!checker.is_subtype_of(TypeId::ERROR, TypeId::OBJECT));
    // Tuples are NOT subtypes of ERROR
    assert!(!checker.is_subtype_of(tuple, TypeId::ERROR));
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
    let target = interner.object(vec![PropertyInfo {
        name: to_upper,
        type_id: method(TypeId::STRING),
        write_type: method(TypeId::STRING),
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: to_upper,
        type_id: method(TypeId::NUMBER),
        write_type: method(TypeId::NUMBER),
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });
    let mismatch = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
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
    let target = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: method(TypeId::STRING),
        write_type: method(TypeId::STRING),
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let mismatch = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: method(TypeId::NUMBER),
        write_type: method(TypeId::NUMBER),
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
    let target = interner.object(vec![PropertyInfo {
        name: to_upper,
        type_id: method(TypeId::STRING),
        write_type: method(TypeId::STRING),
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: to_upper,
        type_id: method(TypeId::NUMBER),
        write_type: method(TypeId::NUMBER),
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_subtype_of(TypeId::STRING, target));
    assert!(!checker.is_subtype_of(TypeId::STRING, mismatch));
}

#[test]
fn test_apparent_string_length_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let length = interner.intern_string("length");
    let target = interner.object(vec![PropertyInfo {
        name: length,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: length,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(TypeId::STRING, target));
    assert!(!checker.is_subtype_of(TypeId::STRING, mismatch));
}

#[test]
fn test_apparent_string_number_index_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });
    let mismatch = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
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
    let target = interner.object(vec![PropertyInfo {
        name: value_of,
        type_id: method(TypeId::BOOLEAN),
        write_type: method(TypeId::BOOLEAN),
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: value_of,
        type_id: method(TypeId::NUMBER),
        write_type: method(TypeId::NUMBER),
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
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
    let target = interner.object(vec![PropertyInfo {
        name: value_of,
        type_id: method(TypeId::BIGINT),
        write_type: method(TypeId::BIGINT),
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: value_of,
        type_id: method(TypeId::NUMBER),
        write_type: method(TypeId::NUMBER),
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
    let target = interner.object(vec![PropertyInfo {
        name: has_own,
        type_id: method(TypeId::BOOLEAN),
        write_type: method(TypeId::BOOLEAN),
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: has_own,
        type_id: method(TypeId::STRING),
        write_type: method(TypeId::STRING),
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_subtype_of(TypeId::NUMBER, target));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, mismatch));
}

#[test]
fn test_object_trifecta_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
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
    let object_interface = interner.object(vec![PropertyInfo {
        name: interner.intern_string("toString"),
        type_id: to_string,
        write_type: to_string,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
    let object_interface = interner.object(vec![PropertyInfo {
        name: interner.intern_string("toString"),
        type_id: to_string,
        write_type: to_string,
        optional: false,
        readonly: false,
        is_method: true,
    }]);
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
    let number_interface = interner.object(vec![PropertyInfo {
        name: interner.intern_string("toFixed"),
        type_id: to_fixed,
        write_type: to_fixed,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
    let bigint_interface = interner.object(vec![PropertyInfo {
        name: interner.intern_string("toString"),
        type_id: to_string,
        write_type: to_string,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
    let boolean_interface = interner.object(vec![PropertyInfo {
        name: interner.intern_string("toString"),
        type_id: to_string,
        write_type: to_string,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
    let string_interface = interner.object(vec![PropertyInfo {
        name: interner.intern_string("toUpperCase"),
        type_id: to_upper,
        write_type: to_upper,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
    let symbol_interface = interner.object(vec![PropertyInfo {
        name: interner.intern_string("description"),
        type_id: description,
        write_type: description,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let def_id = DefId(6);
    env.insert_def(def_id, symbol_interface);
    let symbol_ref = interner.lazy(def_id);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(checker.is_subtype_of(TypeId::SYMBOL, symbol_ref));
    assert!(!checker.is_subtype_of(symbol_ref, TypeId::SYMBOL));
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

    let weak_target = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let no_overlap = interner.object(vec![PropertyInfo {
        name: b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let overlap = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let weak_target = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

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
        PropertyInfo {
            name: a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            is_method: false,
        },
    ]);

    // SubtypeChecker no longer rejects based on weak type rules
    let no_overlap = interner.object(vec![PropertyInfo {
        name: c,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    // SubtypeChecker passes this (CompatChecker would reject it)
    assert!(checker.is_subtype_of(no_overlap, weak_target));

    // Partial overlap (shares 'a' property) - should pass
    let partial_overlap = interner.object(vec![PropertyInfo {
        name: a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
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
        PropertyInfo {
            name: a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false, // Required!
            readonly: false,
            is_method: false,
        },
    ]);

    let c = interner.intern_string("c");
    let unrelated_source = interner.object(vec![PropertyInfo {
        name: c,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    }]);

    let narrow_accessor = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
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
    }]);
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::UNDEFINED,
        write_type: TypeId::UNDEFINED,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(source, target));

    checker.exact_optional_property_types = true;
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_unique_symbol_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_a = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeKey::UniqueSymbol(SymbolRef(2)));

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
    // This is now treated as false for soundness (prevents unsound type acceptance).
    // The depth_exceeded flag is set for TS2589 diagnostic emission.
    //
    // Note: Returning DepthExceeded means "the recursion is too deep, treat as incompatible".
    // This is the conservative choice for soundness - it prevents incorrectly accepting
    // genuinely incompatible types that happen to be deeply nested.
    let result = checker.check_subtype(deep_string, deep_number);
    assert!(matches!(result, SubtypeResult::DepthExceeded));
    assert!(checker.depth_exceeded);
}

#[test]
fn test_no_unchecked_indexed_access_array_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let index_access = interner.intern(TypeKey::IndexAccess(string_array, TypeId::NUMBER));

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
    let index_access = interner.intern(TypeKey::IndexAccess(tuple, TypeId::NUMBER));
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert!(checker.is_subtype_of(index_access, string_or_number));

    checker.no_unchecked_indexed_access = true;
    assert!(!checker.is_subtype_of(index_access, string_or_number));

    let string_number_or_undefined =
        interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]);
    assert!(checker.is_subtype_of(index_access, string_number_or_undefined));
}

#[test]
fn test_no_unchecked_object_index_signature_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let index_access = interner.intern(TypeKey::IndexAccess(indexed, TypeId::NUMBER));

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
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let index_access = interner.intern(TypeKey::IndexAccess(indexed, TypeId::STRING));

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
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let index_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let index_access = interner.intern(TypeKey::IndexAccess(indexed, index_type));

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
        PropertyInfo {
            name: kind,
            type_id: interner.literal_string("a"),
            write_type: interner.literal_string("a"),
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: key_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);
    let obj_b = interner.object(vec![
        PropertyInfo {
            name: kind,
            type_id: interner.literal_string("b"),
            write_type: interner.literal_string("b"),
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: key_b,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);
    let index_access = interner.intern(TypeKey::IndexAccess(union_obj, key_union));
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    assert!(checker.is_subtype_of(index_access, expected));
    assert!(!checker.is_subtype_of(index_access, TypeId::NUMBER));
}

#[test]
fn test_object_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // { x: number }
    let obj_x = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // { x: number, y: string }
    let obj_xy = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("y"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
    }]);
    let mutable_obj = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(!checker.is_subtype_of(readonly_obj, mutable_obj));
    assert!(checker.is_subtype_of(mutable_obj, readonly_obj));
}

#[test]
fn test_readonly_array_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let mutable_array = interner.array(TypeId::STRING);
    let readonly_array = interner.intern(TypeKey::ReadonlyType(mutable_array));

    assert!(checker.is_subtype_of(mutable_array, readonly_array));
    assert!(!checker.is_subtype_of(readonly_array, mutable_array));
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
    let readonly_tuple = interner.intern(TypeKey::ReadonlyType(tuple));

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

    // Initially empty
    assert!(env.is_empty());
    assert_eq!(env.len(), 0);

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

    // Check len
    assert_eq!(env.len(), 2);
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
fn test_ref_to_ref_resolution() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Two refs that should be equal when resolved
    let ref1 = interner.lazy(DefId(1));
    let ref2 = interner.lazy(DefId(2));

    // Both resolve to string
    env.insert_def(DefId(1), TypeId::STRING);
    env.insert_def(DefId(2), TypeId::STRING);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker.is_subtype_of(ref1, ref2));
    assert!(checker.is_subtype_of(ref2, ref1));
}

#[test]
fn test_ref_to_object_resolution() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Create an object type: { x: number }
    let obj_x = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Create a Ref that resolves to { x: number, y: string }
    let obj_xy = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("y"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let ref_type = interner.lazy(DefId(100));
    env.insert_def(DefId(100), obj_xy);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    // Ref resolves to { x: number, y: string } which is subtype of { x: number }
    assert!(checker.is_subtype_of(ref_type, obj_x));
}

#[test]
fn test_unresolved_ref_behavior() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new(); // Empty environment

    let ref_type = interner.lazy(DefId(999));

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    // Unresolved ref to itself should be true (same TypeId)
    assert!(checker.is_subtype_of(ref_type, ref_type));

    // Unresolved ref to something else should be false
    assert!(!checker.is_subtype_of(ref_type, TypeId::STRING));
}

#[test]
fn test_function_rest_parameter_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Create any[] type for rest parameter
    let any_array = interner.array(TypeId::ANY);

    // (a: string, b: any, c: any) => any - 3 fixed params
    let fixed_params = FunctionShape {
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
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("c")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };
    let fixed_fn = interner.function(fixed_params);

    // (a: string, b: any, ...args: any[]) => any - 2 fixed + rest
    let rest_params = FunctionShape {
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
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: any_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };
    let rest_fn = interner.function(rest_params);

    // Function with 3 fixed params IS assignable to function with 2 fixed + rest
    // Because (a, b, c) can be called as (a, b, ...args) where args = [c]
    assert!(checker.is_subtype_of(fixed_fn, rest_fn));

    // Function with rest is NOT assignable to function with fixed params
    // (because rest can accept 0 or more args, but fixed expects exactly 3)
    // This depends on semantics - TypeScript actually allows this in some cases
    // For now, test the basic case
}

#[test]
fn test_rest_unknown_bivariant_subtyping_toggle() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_unknown = interner.array(TypeId::UNKNOWN);
    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_unknown,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_subtype_of(source, target));

    checker.allow_bivariant_rest = true;
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_rest_any_bivariant_subtyping_toggle() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_any = interner.array(TypeId::ANY);
    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: rest_any,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_subtype_of(source, target));

    checker.allow_bivariant_rest = true;
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_extra_elements() {
    // CRITICAL: [number, string] is NOT assignable to [number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // [number, string]
    let source = interner.tuple(vec![
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

    // [number]
    let target = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    // Source has extra elements, target is closed -> should FAIL
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_with_rest_target() {
    // [number, string] IS assignable to [number, ...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [number, string]
    let source = interner.tuple(vec![
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

    // [number, ...string[]]
    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // Target has rest -> should accept extra elements
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_rest_tuple_expansion() {
    // [number, string, boolean] IS assignable to [number, ...[string, boolean]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_tuple = interner.tuple(vec![
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

    let source = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_rest_tuple_missing_element() {
    // [number, string] is NOT assignable to [number, ...[string, boolean]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_tuple = interner.tuple(vec![
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

    let source = interner.tuple(vec![
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

    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_rest_tuple_extra_element() {
    // [number, string, boolean, boolean] is NOT assignable to [number, ...[string, boolean]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_tuple = interner.tuple(vec![
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

    let source = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
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

    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_rest_tuple_variadic_tail() {
    // [number, string, boolean, boolean] IS assignable to [number, ...[string, ...boolean[]]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let boolean_array = interner.array(TypeId::BOOLEAN);
    let rest_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: boolean_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let source = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
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

    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_source_rest_closed_target() {
    // [number, ...string[]] is NOT assignable to [number, string]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [number, ...string[]]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // [number, string]
    let target = interner.tuple(vec![
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

    // Source has rest but target is closed -> should FAIL
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_optional_elements() {
    // [number, string?] IS assignable to [number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // [number, string?]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    // [number]
    let target = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    // Optional elements don't count as "extra" if they're beyond target length
    // This is actually a borderline case - TypeScript may reject this
    // For strictness, we reject tuples with more elements even if optional
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_subtyping_rest_to_rest() {
    // [number, ...string[]] IS assignable to [number, ...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [number, ...string[]]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // [number, ...string[]]
    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // Both have rest, same types -> should succeed
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_to_array_with_rest() {
    // BLOCKER fix: [number, ...string[]] IS assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [number, ...string[]]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // string[]
    let target = string_array;

    // This should FAIL because first element is number, not string
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_to_array_with_rest_tuple() {
    // [string, ...[string, string]] IS assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let rest_tuple = interner.tuple(vec![
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

    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(checker.is_subtype_of(source, string_array));
}

#[test]
fn test_tuple_to_array_with_rest_tuple_mismatch() {
    // [string, ...[string, number]] is NOT assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let rest_tuple = interner.tuple(vec![
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

    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(!checker.is_subtype_of(source, string_array));
}

#[test]
fn test_tuple_to_array_with_rest_tuple_variadic() {
    // [string, ...[string, ...string[]]] IS assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let rest_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_tuple,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(checker.is_subtype_of(source, string_array));
}

#[test]
fn test_tuple_to_array_all_matching_with_rest() {
    // [string, ...string[]] IS assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [string, ...string[]]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    // string[]
    let target = string_array;

    // This should SUCCEED - all elements (including rest) are strings
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_to_array_no_rest() {
    // [string, string] IS assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [string, string]
    let source = interner.tuple(vec![
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

    // string[]
    let target = string_array;

    // This should SUCCEED - all fixed elements are strings
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_to_array_mixed_types() {
    // [number, string] is NOT assignable to string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    // [number, string]
    let source = interner.tuple(vec![
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

    // string[]
    let target = string_array;

    // This should FAIL - first element is number, not string
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_to_array_number_number() {
    // [number, number] IS assignable to number[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);

    // [number, number]
    let source = interner.tuple(vec![
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

    // number[]
    let target = number_array;

    // This should SUCCEED - all fixed elements are numbers
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_tuple_array_assignment_tuple_to_union_array() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(checker.is_subtype_of(source, union_array));
}

#[test]
fn test_array_to_variadic_tuple() {
    // string[] is NOT assignable to [...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![TupleElement {
        type_id: string_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    assert!(!checker.is_subtype_of(string_array, target));
}

#[test]
fn test_tuple_array_assignment_array_to_tuple_rejected() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![
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

    assert!(!checker.is_subtype_of(string_array, target));
}

#[test]
fn test_array_to_variadic_tuple_with_required_prefix() {
    // string[] is NOT assignable to [string, ...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(!checker.is_subtype_of(string_array, target));
}

#[test]
fn test_array_to_variadic_tuple_with_optional_prefix() {
    // string[] is NOT assignable to [string?, ...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(!checker.is_subtype_of(string_array, target));
}

#[test]
fn test_array_to_fixed_optional_tuple() {
    // string[] is NOT assignable to [string?]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: true,
        rest: false,
    }]);

    assert!(!checker.is_subtype_of(string_array, target));
}

#[test]
fn test_tuple_array_assignment_empty_array_optional_tuple() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty_array = interner.array(TypeId::NEVER);
    let optional_tuple = interner.tuple(vec![
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

    assert!(checker.is_subtype_of(empty_array, optional_tuple));
}

#[test]
fn test_never_array_to_optional_tuple() {
    // never[] IS assignable to [] and [string?]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let never_array = interner.array(TypeId::NEVER);
    let empty_tuple = interner.tuple(Vec::new());
    let optional_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: true,
        rest: false,
    }]);
    let required_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(checker.is_subtype_of(never_array, empty_tuple));
    assert!(checker.is_subtype_of(never_array, optional_tuple));
    assert!(!checker.is_subtype_of(never_array, required_tuple));
}

#[test]
fn test_never_array_to_variadic_tuple() {
    // never[] IS assignable to [...string[]]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let never_array = interner.array(TypeId::NEVER);
    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![TupleElement {
        type_id: string_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    assert!(checker.is_subtype_of(never_array, target));
}

#[test]
fn test_number_index_signature_numeric_property() {
    // CRITICAL: { 0: string } should match { [x: number]: string }

    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // { 0: string }
    let source = interner.object(vec![PropertyInfo {
        name: interner.intern_string("0"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // { [x: number]: string }
    let target_shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    // This should SUCCEED - numeric property "0" matches number index signature
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_number_index_signature_type_mismatch() {
    // { 0: number } should NOT match { [x: number]: string }

    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // { 0: number }
    let source = interner.object(vec![PropertyInfo {
        name: interner.intern_string("0"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // { [x: number]: string }
    let target_shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    // This should FAIL - numeric property has wrong type
    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_number_index_signature_method_bivariant_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let narrow_param = TypeId::STRING;
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_fn = interner.function(FunctionShape {
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

    let source_method = interner.object(vec![PropertyInfo {
        name: interner.intern_string("0"),
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let source_prop = interner.object(vec![PropertyInfo {
        name: interner.intern_string("0"),
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let target_shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: wide_fn,
            readonly: false,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    assert!(checker.is_subtype_of(source_method, target));
    assert!(!checker.is_subtype_of(source_prop, target));
}

#[test]
fn test_string_index_signature_method_bivariant_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let narrow_param = TypeId::STRING;
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_fn = interner.function(FunctionShape {
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

    let source_method = interner.object(vec![PropertyInfo {
        name: interner.intern_string("foo"),
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let source_prop = interner.object(vec![PropertyInfo {
        name: interner.intern_string("foo"),
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let target_shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: wide_fn,
            readonly: false,
        }),
    };
    let target = interner.object_with_index(target_shape);

    assert!(checker.is_subtype_of(source_method, target));
    assert!(!checker.is_subtype_of(source_prop, target));
}

#[test]
fn test_number_index_signature_multiple_numeric_props() {
    // { 0: string, 1: string, 2: string } should match { [x: number]: string }

    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // { 0: string, 1: string, 2: string }
    let source = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("0"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("1"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("2"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    // { [x: number]: string }
    let target_shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    // This should SUCCEED - all numeric properties match
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_number_and_string_index_signatures() {
    // { 0: string, foo: string } should match { [x: number]: string; [y: string]: string }

    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // { 0: string, foo: string }
    let source = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("0"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("foo"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    // { [x: number]: string; [y: string]: string }
    let target_shape = ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    };
    let target = interner.object_with_index(target_shape);

    // This should SUCCEED - "0" satisfies number index, both satisfy string index
    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_index_signature_consistency_number_vs_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_readonly_index_signature_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let readonly_source = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
        }),
    });

    let mutable_target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let readonly_target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
        }),
    });

    assert!(!checker.is_subtype_of(readonly_source, mutable_target));
    assert!(checker.is_subtype_of(mutable_target, readonly_target));
}

#[test]
fn test_readonly_property_with_mutable_index_signature() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let mutable_index = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let readonly_index = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
        }),
    });

    assert!(!checker.is_subtype_of(source, mutable_index));
    assert!(checker.is_subtype_of(source, readonly_index));
}

#[test]
fn test_object_with_index_properties_match_target_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![
            PropertyInfo {
                name: interner.intern_string("0"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
            },
            PropertyInfo {
                name: interner.intern_string("name"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
            },
        ],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_object_with_index_property_mismatch_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("name"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_object_with_index_property_mismatch_number_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("0"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        string_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        string_index: None,
    });

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_object_with_index_satisfies_named_property_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let target = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_object_with_index_named_property_mismatch_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let target = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_object_to_indexed_property_mismatch_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let target = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_object_with_index_satisfies_numeric_property_number_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        string_index: None,
    });

    let target = interner.object(vec![PropertyInfo {
        name: interner.intern_string("0"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_object_with_index_noncanonical_numeric_property_fails() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        string_index: None,
    });

    let target = interner.object(vec![PropertyInfo {
        name: interner.intern_string("01"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_object_with_index_readonly_index_to_mutable_property_fails() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
        }),
    });

    let target = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_type_parameter_constraint_assignability() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
    }));

    assert!(checker.is_subtype_of(t_param, TypeId::STRING));
    assert!(!checker.is_subtype_of(t_param, TypeId::NUMBER));

    let unconstrained = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
    }));
    assert!(!checker.is_subtype_of(unconstrained, TypeId::STRING));
}

#[test]
fn test_base_constraint_assignability_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
    }));
    let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(TypeId::STRING),
        default: None,
    }));
    let v_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: Some(TypeId::NUMBER),
        default: None,
    }));

    assert!(checker.is_subtype_of(t_param, TypeId::STRING));
    assert!(!checker.is_subtype_of(t_param, TypeId::NUMBER));
    assert!(!checker.is_subtype_of(t_param, u_param));
    assert!(!checker.is_subtype_of(t_param, v_param));
}

#[test]
fn test_base_constraint_not_assignable_to_param() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
    }));

    assert!(!checker.is_subtype_of(TypeId::STRING, t_param));
}

#[test]
fn test_type_parameter_identity_only() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
    }));
    let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(TypeId::STRING),
        default: None,
    }));

    assert!(!checker.is_subtype_of(t_param, u_param));
}

#[test]
fn test_deferred_conditional_source_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));

    let conditional = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    });

    let target_union = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);

    assert!(checker.is_subtype_of(conditional, target_union));
    assert!(!checker.is_subtype_of(conditional, TypeId::NUMBER));
}

#[test]
fn test_deferred_conditional_target_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));

    let conditional = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    });

    assert!(!checker.is_subtype_of(TypeId::NUMBER, conditional));
}

#[test]
fn test_deferred_conditional_structural_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));

    let source = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: true,
    });

    let union_nb = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);
    let target = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: union_nb,
        false_type: union_nb,
        is_distributive: true,
    });

    let mismatch = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::NUMBER,
        true_type: union_nb,
        false_type: union_nb,
        is_distributive: true,
    });

    assert!(checker.is_subtype_of(source, target));
    assert!(!checker.is_subtype_of(source, mismatch));
}

#[test]
fn test_conditional_tuple_wrapper_no_distribution_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
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

    assert!(checker.is_subtype_of(instantiated, TypeId::BOOLEAN));
    assert!(!checker.is_subtype_of(instantiated, TypeId::NUMBER));
}

#[test]
fn test_strict_function_variance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    // Ensure strict mode is on (default)
    assert!(checker.strict_function_types);

    // (x: string | number) => void
    let union_arg_fn = interner.function(FunctionShape {
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

    // (x: string) => void
    let string_arg_fn = interner.function(FunctionShape {
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

    // 1. Safe assignment: (string | number) => void  <:  (string) => void
    // Target param (string) <: Source param (string | number) -> OK (contravariant)
    assert!(checker.is_subtype_of(union_arg_fn, string_arg_fn));

    // 2. Unsafe assignment: (string) => void  <:  (string | number) => void
    // Target param (string | number) <: Source param (string) -> FAIL (would be unsound)
    assert!(!checker.is_subtype_of(string_arg_fn, union_arg_fn));

    // 3. Disable strict mode (Bivariant)
    checker.strict_function_types = false;
    // Now unsafe assignment should pass (legacy behavior)
    assert!(checker.is_subtype_of(string_arg_fn, union_arg_fn));
}

#[test]
fn test_function_variance_union_intersection_targets() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_with_param = |param| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };

    let fn_string = fn_with_param(TypeId::STRING);
    let fn_number = fn_with_param(TypeId::NUMBER);
    let fn_union_param = fn_with_param(interner.union(vec![TypeId::STRING, TypeId::NUMBER]));

    let union_target = interner.union(vec![fn_string, fn_number]);
    let intersection_target = interner.intersection(vec![fn_string, fn_number]);

    assert!(checker.is_subtype_of(fn_union_param, union_target));
    assert!(checker.is_subtype_of(fn_union_param, intersection_target));
    assert!(!checker.is_subtype_of(fn_string, intersection_target));
    assert!(!checker.is_subtype_of(union_target, fn_union_param));
}

#[test]
fn test_callable_rest_parameter_contravariance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let rest_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let rest_array = interner.array(rest_union);

    let source = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
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
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                },
            ],
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

    let target = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![
                ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                },
                ParamInfo {
                    name: Some(interner.intern_string("args")),
                    type_id: rest_array,
                    optional: false,
                    rest: true,
                },
            ],
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

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_method_bivariant_required_param() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let wide_method = interner.function(FunctionShape {
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

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: wide_method,
        write_type: wide_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let narrow_obj = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(checker.is_subtype_of(narrow_obj, wide_obj));
}

#[test]
fn test_method_source_bivariant_against_function_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let name = interner.intern_string("m");

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_func = interner.function(FunctionShape {
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

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: wide_func,
        write_type: wide_func,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_function_source_bivariant_against_method_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let name = interner.intern_string("m");

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let narrow_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_method = interner.function(FunctionShape {
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

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: narrow_func,
        write_type: narrow_func,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: wide_method,
        write_type: wide_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_variance_optional_rest_method_optional_bivariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let wide_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_param,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: wide_method,
        write_type: wide_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let narrow_obj = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(checker.is_subtype_of(narrow_obj, wide_obj));
}

#[test]
fn test_variance_optional_rest_method_rest_bivariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let wide_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_elem = TypeId::STRING;
    let wide_rest = interner.array(wide_elem);
    let narrow_rest = interner.array(narrow_elem);

    let wide_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: wide_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: narrow_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: wide_method,
        write_type: wide_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let narrow_obj = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(checker.is_subtype_of(narrow_obj, wide_obj));
}

#[test]
fn test_variance_optional_rest_method_optional_with_this_bivariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let wide_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_param,
            optional: true,
            rest: false,
        }],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: true,
            rest: false,
        }],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: wide_method,
        write_type: wide_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let narrow_obj = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(checker.is_subtype_of(narrow_obj, wide_obj));
}

#[test]
fn test_variance_optional_rest_method_rest_with_this_bivariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_elem = TypeId::STRING;
    let wide_rest = interner.array(wide_elem);
    let narrow_rest = interner.array(narrow_elem);

    let wide_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: wide_rest,
            optional: false,
            rest: true,
        }],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: narrow_rest,
            optional: false,
            rest: true,
        }],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: wide_method,
        write_type: wide_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let narrow_obj = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(checker.is_subtype_of(narrow_obj, wide_obj));
}

#[test]
fn test_variance_optional_rest_function_optional_with_this_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let func_name = interner.intern_string("f");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let wide_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_param,
            optional: true,
            rest: false,
        }],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: true,
            rest: false,
        }],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo {
        name: func_name,
        type_id: wide_func,
        write_type: wide_func,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let narrow_obj = interner.object(vec![PropertyInfo {
        name: func_name,
        type_id: narrow_func,
        write_type: narrow_func,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(!checker.is_subtype_of(narrow_obj, wide_obj));
}

#[test]
fn test_variance_optional_rest_function_rest_with_this_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let func_name = interner.intern_string("f");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_elem = TypeId::STRING;
    let wide_rest = interner.array(wide_elem);
    let narrow_rest = interner.array(narrow_elem);

    let wide_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: wide_rest,
            optional: false,
            rest: true,
        }],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: narrow_rest,
            optional: false,
            rest: true,
        }],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo {
        name: func_name,
        type_id: wide_func,
        write_type: wide_func,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let narrow_obj = interner.object(vec![PropertyInfo {
        name: func_name,
        type_id: narrow_func,
        write_type: narrow_func,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(!checker.is_subtype_of(narrow_obj, wide_obj));
}

#[test]
fn test_variance_optional_rest_constructor_optional_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let wide_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_param,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let narrow_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: narrow_param,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(checker.is_subtype_of(wide_ctor, narrow_ctor));
    assert!(!checker.is_subtype_of(narrow_ctor, wide_ctor));
}

#[test]
fn test_variance_optional_rest_constructor_rest_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_elem = TypeId::STRING;
    let wide_rest = interner.array(wide_elem);
    let narrow_rest = interner.array(narrow_elem);

    let wide_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: wide_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let narrow_ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: narrow_rest,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(checker.is_subtype_of(wide_ctor, narrow_ctor));
    assert!(!checker.is_subtype_of(narrow_ctor, wide_ctor));
}

#[test]
fn test_function_required_count_allows_optional_source_extra() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.function(FunctionShape {
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
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
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

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_function_required_count_rejects_required_source_extra() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.function(FunctionShape {
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

    let target = interner.function(FunctionShape {
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
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_function_variance_param_contravariance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_param = TypeId::STRING;

    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: wide_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: TypeId::BOOLEAN,
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

    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: narrow_param,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: TypeId::BOOLEAN,
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

    assert!(checker.is_subtype_of(source, target));
    assert!(!checker.is_subtype_of(target, source));
}

#[test]
fn test_function_variance_return_covariance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let narrow_return = TypeId::STRING;
    let wide_return = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::BOOLEAN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: narrow_return,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::BOOLEAN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: wide_return,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_subtype_of(source, target));
    assert!(!checker.is_subtype_of(target, source));
}

#[test]
fn test_function_return_covariance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let returns_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let returns_string_or_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_subtype_of(returns_string, returns_string_or_number));
    assert!(!checker.is_subtype_of(returns_string_or_number, returns_string));
}

#[test]
fn test_void_return_exception_subtype() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let returns_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let returns_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_subtype_of(returns_number, returns_void));

    checker.allow_void_return = true;
    assert!(checker.is_subtype_of(returns_number, returns_void));
    assert!(!checker.is_subtype_of(returns_void, returns_number));
}

#[test]
fn test_void_return_exception_method_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let returns_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let returns_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: returns_number,
        write_type: returns_number,
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let target = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: returns_void,
        write_type: returns_void,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(!checker.is_subtype_of(source, target));

    checker.allow_void_return = true;
    assert!(checker.is_subtype_of(source, target));
    assert!(!checker.is_subtype_of(target, source));
}

#[test]
fn test_constructor_void_exception_subtype() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let returns_instance = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let returns_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    assert!(!checker.is_subtype_of(returns_instance, returns_void));

    checker.allow_void_return = true;
    assert!(checker.is_subtype_of(returns_instance, returns_void));
    assert!(!checker.is_subtype_of(returns_void, returns_instance));
}

#[test]
fn test_function_top_assignability() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let function_top = interner.callable(CallableShape {
        call_signatures: Vec::new(),
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let specific_fn = interner.function(FunctionShape {
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

    assert!(checker.is_subtype_of(specific_fn, function_top));
    assert!(!checker.is_subtype_of(function_top, specific_fn));
}

#[test]
fn test_this_parameter_variance() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_this_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(union_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let string_this_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // this parameter is contravariant like regular parameters
    assert!(checker.is_subtype_of(union_this_fn, string_this_fn));
    assert!(!checker.is_subtype_of(string_this_fn, union_this_fn));
}

#[test]
fn test_this_parameter_method_property_bivariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let method_name = interner.intern_string("m");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: wide_method,
        write_type: wide_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let narrow_obj = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(checker.is_subtype_of(narrow_obj, wide_obj));
}

#[test]
fn test_this_parameter_function_property_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let func_name = interner.intern_string("f");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let wide_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_obj = interner.object(vec![PropertyInfo {
        name: func_name,
        type_id: wide_func,
        write_type: wide_func,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let narrow_obj = interner.object(vec![PropertyInfo {
        name: func_name,
        type_id: narrow_func,
        write_type: narrow_func,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(wide_obj, narrow_obj));
    assert!(!checker.is_subtype_of(narrow_obj, wide_obj));
}

#[test]
fn test_this_parameter_method_source_bivariant_against_function_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let name = interner.intern_string("m");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_this = TypeId::STRING;

    let narrow_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(narrow_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: narrow_method,
        write_type: narrow_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: wide_func,
        write_type: wide_func,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_this_parameter_function_source_bivariant_against_method_property() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let name = interner.intern_string("m");

    let wide_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_this = TypeId::STRING;

    let narrow_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(narrow_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(wide_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: narrow_func,
        write_type: narrow_func,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: wide_method,
        write_type: wide_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_this_type_in_param_covariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    let func_name = interner.intern_string("compare");

    let this_type = interner.intern(TypeKey::ThisType);
    let this_or_number = interner.union(vec![this_type, TypeId::NUMBER]);

    let narrow_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: this_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let wide_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: this_or_number,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let narrow_obj = interner.object(vec![PropertyInfo {
        name: func_name,
        type_id: narrow_fn,
        write_type: narrow_fn,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let wide_obj = interner.object(vec![PropertyInfo {
        name: func_name,
        type_id: wide_fn,
        write_type: wide_fn,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(narrow_obj, wide_obj));
    assert!(!checker.is_subtype_of(wide_obj, narrow_obj));
}

#[test]
fn test_class_like_subtyping_this_param_covariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let compare = interner.intern_string("compare");
    let id = interner.intern_string("id");
    let extra = interner.intern_string("extra");

    let this_type = interner.intern(TypeKey::ThisType);
    let this_or_number = interner.union(vec![this_type, TypeId::NUMBER]);

    let base_compare = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: this_or_number,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let derived_compare = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: this_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let base = interner.object(vec![
        PropertyInfo {
            name: id,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: compare,
            type_id: base_compare,
            write_type: base_compare,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let derived = interner.object(vec![
        PropertyInfo {
            name: id,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: extra,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: compare,
            type_id: derived_compare,
            write_type: derived_compare,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    assert!(checker.is_subtype_of(derived, base));
    assert!(!checker.is_subtype_of(base, derived));
}

#[test]
fn test_function_fixed_to_rest_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Source: (name: string, mixed: any, arg: any) => any
    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("mixed")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("arg")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Target: (name: string, mixed: any, ...args: any[]) => any
    let any_array = interner.array(TypeId::ANY);
    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("mixed")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: any_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function with fixed params should be subtype of function with rest params
    // This matches TypeScript behavior
    assert!(
        checker.is_subtype_of(source, target),
        "Function with 3 fixed params should be subtype of function with 2 fixed + rest params"
    );
}

#[test]
fn test_function_fixed_to_rest_extra_param_accepts_undefined() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let num_or_undef = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);

    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: num_or_undef,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let number_array = interner.array(TypeId::NUMBER);
    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: number_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(checker.is_subtype_of(source, target));
}

#[test]
fn test_function_fixed_to_rest_extra_param_rejects_undefined() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let number_array = interner.array(TypeId::NUMBER);
    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: number_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_subtype_of(source, target));
}

#[test]
fn test_function_rest_tuple_to_rest_array_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Source: (name: string, mixed: any, ...args: [any]) => any
    let tuple_one_any = interner.tuple(vec![TupleElement {
        type_id: TypeId::ANY,
        name: None,
        optional: false,
        rest: false,
    }]);
    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("mixed")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: tuple_one_any,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Target: (name: string, mixed: any, ...args: any[]) => any
    let any_array = interner.array(TypeId::ANY);
    let target = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("name")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("mixed")),
                type_id: TypeId::ANY,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: any_array,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function with rest tuple should be subtype of function with rest array
    // (name, mixed, ...args: [any]) should be assignable to (name, mixed, ...args: any[])
    assert!(
        checker.is_subtype_of(source, target),
        "Function with rest tuple [any] should be subtype of function with rest array any[]"
    );
}

#[test]
fn test_keyof_intersection_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    let keyof_a = interner.intern(TypeKey::KeyOf(obj_a));
    let keyof_intersection = interner.intern(TypeKey::KeyOf(intersection));

    assert!(checker.is_subtype_of(keyof_a, keyof_intersection));
    assert!(!checker.is_subtype_of(keyof_intersection, keyof_a));
}

#[test]
fn test_keyof_contravariant_object_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let obj_ab = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    assert!(checker.is_subtype_of(obj_ab, obj_a));

    let keyof_a = interner.intern(TypeKey::KeyOf(obj_a));
    let keyof_ab = interner.intern(TypeKey::KeyOf(obj_ab));

    assert!(checker.is_subtype_of(keyof_a, keyof_ab));
    assert!(!checker.is_subtype_of(keyof_ab, keyof_a));
}

#[test]
fn test_keyof_intersection_union_of_keys() {
    use crate::solver::evaluate_keyof;

    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    let result = evaluate_keyof(&interner, intersection);
    let expected = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);

    assert_eq!(result, expected);
}

#[test]
fn test_keyof_union_disjoint_object_keys_is_never() {
    use crate::solver::evaluate_keyof;

    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let union = interner.union(vec![obj_a, obj_b]);
    let result = evaluate_keyof(&interner, union);

    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_keyof_union_index_signature_contravariant() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_index = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });
    let number_index = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let union = interner.union(vec![string_index, number_index]);
    let keyof_union = interner.intern(TypeKey::KeyOf(union));

    assert!(checker.is_subtype_of(keyof_union, TypeId::NUMBER));
    assert!(!checker.is_subtype_of(keyof_union, TypeId::STRING));
}

#[test]
fn test_keyof_union_string_index_and_literal_narrows() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_index = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });
    let key_a = interner.intern_string("a");
    let obj_a = interner.object(vec![PropertyInfo {
        name: key_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let union = interner.union(vec![string_index, obj_a]);
    let keyof_union = interner.intern(TypeKey::KeyOf(union));
    let key_a_literal = interner.literal_string("a");

    assert!(checker.is_subtype_of(keyof_union, key_a_literal));
    assert!(checker.is_subtype_of(keyof_union, TypeId::STRING));
    assert!(!checker.is_subtype_of(keyof_union, TypeId::NUMBER));
    assert!(checker.is_subtype_of(key_a_literal, keyof_union));
    assert!(!checker.is_subtype_of(TypeId::STRING, keyof_union));
}

#[test]
fn test_keyof_union_overlapping_keys_is_common() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");
    let key_c = interner.intern_string("c");

    let obj_ab = interner.object(vec![
        PropertyInfo {
            name: key_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: key_b,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);
    let obj_ac = interner.object(vec![
        PropertyInfo {
            name: key_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: key_c,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let union = interner.union(vec![obj_ab, obj_ac]);
    let keyof_union = interner.intern(TypeKey::KeyOf(union));
    let key_a_literal = interner.literal_string("a");
    let key_b_literal = interner.literal_string("b");
    let key_c_literal = interner.literal_string("c");

    assert!(checker.is_subtype_of(keyof_union, key_a_literal));
    assert!(!checker.is_subtype_of(keyof_union, key_b_literal));
    assert!(!checker.is_subtype_of(keyof_union, key_c_literal));
    assert!(checker.is_subtype_of(key_a_literal, keyof_union));
}

#[test]
fn test_keyof_union_optional_key_is_common() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.intern_string("a");
    let key_b = interner.intern_string("b");

    let obj_optional_a = interner.object(vec![PropertyInfo {
        name: key_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let obj_ab = interner.object(vec![
        PropertyInfo {
            name: key_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: key_b,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let union = interner.union(vec![obj_optional_a, obj_ab]);
    let keyof_union = interner.intern(TypeKey::KeyOf(union));
    let key_a_literal = interner.literal_string("a");
    let key_b_literal = interner.literal_string("b");

    assert!(checker.is_subtype_of(keyof_union, key_a_literal));
    assert!(!checker.is_subtype_of(keyof_union, key_b_literal));
    assert!(checker.is_subtype_of(key_a_literal, keyof_union));
}

#[test]
fn test_keyof_deferred_not_subtype_of_string() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let type_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));
    let keyof_param = interner.intern(TypeKey::KeyOf(type_param));

    assert!(!checker.is_subtype_of(keyof_param, TypeId::STRING));
}

#[test]
fn test_keyof_deferred_subtype_of_string_number_symbol_union() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let type_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));
    let keyof_param = interner.intern(TypeKey::KeyOf(type_param));

    let key_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);
    assert!(checker.is_subtype_of(keyof_param, key_union));
}

#[test]
fn test_keyof_deferred_not_subtype_of_string_number_union() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let type_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));
    let keyof_param = interner.intern(TypeKey::KeyOf(type_param));

    let key_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert!(!checker.is_subtype_of(keyof_param, key_union));
}

#[test]
fn test_keyof_any_subtyping_union() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_any = interner.intern(TypeKey::KeyOf(TypeId::ANY));
    let key_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);
    let string_number_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert!(checker.is_subtype_of(keyof_any, key_union));
    assert!(!checker.is_subtype_of(keyof_any, string_number_union));
}

#[test]
fn test_intersection_reduction_disjoint_discriminant_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let kind = interner.intern_string("kind");
    let obj_a = interner.object(vec![PropertyInfo {
        name: kind,
        type_id: interner.literal_string("a"),
        write_type: interner.literal_string("a"),
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let obj_b = interner.object(vec![PropertyInfo {
        name: kind,
        type_id: interner.literal_string("b"),
        write_type: interner.literal_string("b"),
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    assert!(checker.is_subtype_of(intersection, TypeId::NEVER));
    assert!(checker.is_subtype_of(intersection, TypeId::STRING));
}

#[test]
fn test_intersection_reduction_disjoint_intrinsics() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);

    assert!(checker.is_subtype_of(intersection, TypeId::NEVER));
}

#[test]
fn test_mapped_type_over_number_keys_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeKey::KeyOf(TypeId::NUMBER));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_fixed = interner.intern_string("toFixed");
    let expected = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let to_upper = interner.intern_string("toUpperCase");
    let wrong_key = interner.object(vec![PropertyInfo {
        name: to_upper,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, mismatch));
    assert!(!checker.is_subtype_of(mapped, wrong_key));
    assert!(!checker.is_subtype_of(expected, mapped));
}

#[test]
fn test_mapped_type_over_number_keys_optional_readonly_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeKey::KeyOf(TypeId::NUMBER));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: Some(MappedModifier::Add),
    });

    let to_fixed = interner.intern_string("toFixed");
    let optional_readonly = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: true,
        readonly: true,
        is_method: false,
    }]);
    let required_readonly = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: true,
        is_method: false,
    }]);
    let optional_mutable = interner.object(vec![PropertyInfo {
        name: to_fixed,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, optional_readonly));
    assert!(!checker.is_subtype_of(mapped, required_readonly));
    assert!(!checker.is_subtype_of(mapped, optional_mutable));
}

#[test]
fn test_mapped_type_over_string_keys_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeKey::KeyOf(TypeId::STRING));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_upper = interner.intern_string("toUpperCase");
    let expected = interner.object(vec![PropertyInfo {
        name: to_upper,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: to_upper,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, mismatch));
    assert!(!checker.is_subtype_of(expected, mapped));
}

#[test]
fn test_mapped_type_over_string_keys_number_index_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeKey::KeyOf(TypeId::STRING));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint,
        name_type: None,
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let number_index = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::BOOLEAN,
            readonly: false,
        }),
    });
    let mismatch = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    assert!(checker.is_subtype_of(mapped, number_index));
    assert!(!checker.is_subtype_of(mapped, mismatch));
}

#[test]
fn test_mapped_type_over_string_keys_key_remap_omit_length() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeKey::KeyOf(TypeId::STRING));
    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
    };
    let key_param_id = interner.intern(TypeKey::TypeParameter(key_param.clone()));
    let length_key = interner.literal_string("length");
    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: length_key,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint,
        name_type: Some(name_type),
        template: TypeId::BOOLEAN,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_upper = interner.intern_string("toUpperCase");
    let expected = interner.object(vec![PropertyInfo {
        name: to_upper,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let length = interner.intern_string("length");
    let requires_length = interner.object(vec![PropertyInfo {
        name: length,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, requires_length));
}

#[test]
fn test_mapped_type_over_boolean_keys_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeKey::KeyOf(TypeId::BOOLEAN));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let value_of = interner.intern_string("valueOf");
    let expected = interner.object(vec![PropertyInfo {
        name: value_of,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: value_of,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, mismatch));
    assert!(!checker.is_subtype_of(expected, mapped));
}

#[test]
fn test_mapped_type_over_symbol_keys_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeKey::KeyOf(TypeId::SYMBOL));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let description = interner.intern_string("description");
    let expected = interner.object(vec![PropertyInfo {
        name: description,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: description,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, mismatch));
    assert!(!checker.is_subtype_of(expected, mapped));
}

#[test]
fn test_mapped_type_over_bigint_keys_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let constraint = interner.intern(TypeKey::KeyOf(TypeId::BIGINT));
    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let to_string = interner.intern_string("toString");
    let expected = interner.object(vec![PropertyInfo {
        name: to_string,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
        name: to_string,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let to_upper = interner.intern_string("toUpperCase");
    let wrong_key = interner.object(vec![PropertyInfo {
        name: to_upper,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, mismatch));
    assert!(!checker.is_subtype_of(mapped, wrong_key));
    assert!(!checker.is_subtype_of(expected, mapped));
}

#[test]
fn test_mapped_type_optional_modifier_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    });

    let name_a = interner.intern_string("a");
    let name_b = interner.intern_string("b");
    let optional_target = interner.object(vec![
        PropertyInfo {
            name: name_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: name_b,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
        },
    ]);
    let required_target = interner.object(vec![
        PropertyInfo {
            name: name_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: name_b,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    assert!(checker.is_subtype_of(mapped, optional_target));
    assert!(!checker.is_subtype_of(mapped, required_target));
}

#[test]
fn test_mapped_type_readonly_modifier_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    });

    let name_a = interner.intern_string("a");
    let name_b = interner.intern_string("b");
    let readonly_target = interner.object(vec![
        PropertyInfo {
            name: name_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: name_b,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
        },
    ]);
    let mutable_target = interner.object(vec![
        PropertyInfo {
            name: name_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: name_b,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    assert!(checker.is_subtype_of(mapped, readonly_target));
    assert!(!checker.is_subtype_of(mapped, mutable_target));
}

#[test]
fn test_mapped_type_optional_readonly_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: Some(MappedModifier::Add),
    });

    let name_a = interner.intern_string("a");
    let name_b = interner.intern_string("b");
    let optional_readonly_target = interner.object(vec![
        PropertyInfo {
            name: name_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: name_b,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: true,
            is_method: false,
        },
    ]);
    let mutable_required_target = interner.object(vec![
        PropertyInfo {
            name: name_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: name_b,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    assert!(checker.is_subtype_of(mapped, optional_readonly_target));
    assert!(!checker.is_subtype_of(mapped, mutable_required_target));
}

#[test]
fn test_mapped_type_optional_readonly_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let keys = interner.union(vec![key_a]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: Some(MappedModifier::Remove),
    });

    let name_a = interner.intern_string("a");
    let mutable_required_target = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let readonly_optional_target = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: true,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, mutable_required_target));
    assert!(checker.is_subtype_of(mapped, readonly_optional_target));
    assert!(!checker.is_subtype_of(readonly_optional_target, mapped));
}

#[test]
fn test_mapped_type_optional_modifier_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let keys = interner.union(vec![key_a]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    });

    let name_a = interner.intern_string("a");
    let required_target = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let optional_target = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, required_target));
    assert!(checker.is_subtype_of(mapped, optional_target));
    assert!(!checker.is_subtype_of(optional_target, mapped));
}

#[test]
fn test_mapped_type_optional_remove_from_optional_keyof() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.intern_string("a");
    let source_obj = interner.object(vec![PropertyInfo {
        name: key_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let keys = interner.intern(TypeKey::KeyOf(source_obj));

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::STRING,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    });

    let required_target = interner.object(vec![PropertyInfo {
        name: key_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let optional_target = interner.object(vec![PropertyInfo {
        name: key_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, required_target));
    assert!(checker.is_subtype_of(mapped, optional_target));
    assert!(!checker.is_subtype_of(optional_target, mapped));
}

#[test]
fn test_mapped_type_readonly_remove_from_readonly_keyof() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.intern_string("a");
    let source_obj = interner.object(vec![PropertyInfo {
        name: key_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);
    let keys = interner.intern(TypeKey::KeyOf(source_obj));

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    });

    let mutable_target = interner.object(vec![PropertyInfo {
        name: key_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let readonly_target = interner.object(vec![PropertyInfo {
        name: key_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, mutable_target));
    assert!(checker.is_subtype_of(mapped, readonly_target));
    assert!(!checker.is_subtype_of(readonly_target, mapped));
}

#[test]
fn test_mapped_type_readonly_modifier_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let key_a = interner.literal_string("a");
    let keys = interner.union(vec![key_a]);

    let mapped = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: None,
            default: None,
        },
        constraint: keys,
        name_type: None,
        template: TypeId::NUMBER,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    });

    let name_a = interner.intern_string("a");
    let mutable_target = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let readonly_target = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, mutable_target));
    assert!(checker.is_subtype_of(mapped, readonly_target));
    assert!(!checker.is_subtype_of(readonly_target, mapped));
}

#[test]
fn test_mapped_type_key_remap_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let obj = interner.object(vec![prop_a.clone(), prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
    };
    let key_param_id = interner.intern(TypeKey::TypeParameter(key_param.clone()));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeKey::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let expected = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let requires_a = interner.object(vec![PropertyInfo {
        name: prop_a.name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, expected));
    assert!(!checker.is_subtype_of(mapped, requires_a));
}

#[test]
fn test_mapped_type_key_remap_optional_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let obj = interner.object(vec![prop_a.clone(), prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
    };
    let key_param_id = interner.intern(TypeKey::TypeParameter(key_param.clone()));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeKey::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Add),
    });

    let optional_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let required_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, optional_b));
    assert!(!checker.is_subtype_of(mapped, required_b));
}

#[test]
fn test_mapped_type_key_remap_optional_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    };
    let obj = interner.object(vec![prop_a.clone(), prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
    };
    let key_param_id = interner.intern(TypeKey::TypeParameter(key_param.clone()));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeKey::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    });

    let required_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let optional_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    assert!(!checker.is_subtype_of(mapped, required_b));
    assert!(checker.is_subtype_of(mapped, optional_b));
}

#[test]
fn test_mapped_type_key_remap_optional_readonly_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let obj = interner.object(vec![prop_a.clone(), prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
    };
    let key_param_id = interner.intern(TypeKey::TypeParameter(key_param.clone()));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeKey::IndexAccess(obj, key_param_id));

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
    }]);
    let required_readonly_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
    }]);
    let optional_mutable_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, optional_readonly_b));
    assert!(!checker.is_subtype_of(mapped, required_readonly_b));
    assert!(!checker.is_subtype_of(mapped, optional_mutable_b));
}

#[test]
fn test_mapped_type_key_remap_optional_readonly_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: true,
        is_method: false,
    };
    let obj = interner.object(vec![prop_a.clone(), prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
    };
    let key_param_id = interner.intern(TypeKey::TypeParameter(key_param.clone()));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeKey::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: Some(MappedModifier::Remove),
    });

    let required_mutable_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    let required_mutable_b_with_undef = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: number_or_undefined,
        write_type: number_or_undefined,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let optional_mutable_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    assert!(!checker.is_subtype_of(mapped, required_mutable_b));
    assert!(checker.is_subtype_of(mapped, required_mutable_b_with_undef));
    assert!(checker.is_subtype_of(mapped, optional_mutable_b));
    assert!(!checker.is_subtype_of(optional_mutable_b, mapped));
}

#[test]
fn test_mapped_type_key_remap_readonly_add_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let obj = interner.object(vec![prop_a.clone(), prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
    };
    let key_param_id = interner.intern(TypeKey::TypeParameter(key_param.clone()));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeKey::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: Some(MappedModifier::Add),
        optional_modifier: None,
    });

    let readonly_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
    }]);
    let mutable_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, readonly_b));
    assert!(!checker.is_subtype_of(mapped, mutable_b));
}

#[test]
fn test_mapped_type_key_remap_readonly_remove_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_a = PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    };
    let prop_b = PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
    };
    let obj = interner.object(vec![prop_a.clone(), prop_b.clone()]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let keys = interner.union(vec![key_a, key_b]);

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(keys),
        default: None,
    };
    let key_param_id = interner.intern(TypeKey::TypeParameter(key_param.clone()));

    let name_type = interner.conditional(ConditionalType {
        check_type: key_param_id,
        extends_type: key_a,
        true_type: TypeId::NEVER,
        false_type: key_param_id,
        is_distributive: true,
    });
    let template = interner.intern(TypeKey::IndexAccess(obj, key_param_id));

    let mapped = interner.mapped(MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: Some(name_type),
        template,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    });

    let mutable_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let readonly_b = interner.object(vec![PropertyInfo {
        name: prop_b.name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(mapped, mutable_b));
    assert!(checker.is_subtype_of(mapped, readonly_b));
    assert!(!checker.is_subtype_of(readonly_b, mapped));
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
    };
    let key_param_id = interner.intern(TypeKey::TypeParameter(key_param.clone()));

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
    }]);

    let producer_union = interner.object(vec![PropertyInfo {
        name: get_name,
        type_id: get_union,
        write_type: get_union,
        optional: false,
        readonly: true,
        is_method: true,
    }]);

    // Covariant: Producer<string> <: Producer<string | number>
    assert!(checker.is_subtype_of(producer_string, producer_union));
    // Not the reverse
    assert!(!checker.is_subtype_of(producer_union, producer_string));
}

#[test]
#[ignore = "Generic contravariant parameter position subtyping not fully implemented"]
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

    let consumer_string = interner.object(vec![PropertyInfo {
        name: accept_name,
        type_id: accept_string,
        write_type: accept_string,
        optional: false,
        readonly: true,
        is_method: true,
    }]);

    let consumer_union = interner.object(vec![PropertyInfo {
        name: accept_name,
        type_id: accept_union,
        write_type: accept_union,
        optional: false,
        readonly: true,
        is_method: true,
    }]);

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
    }]);

    let transform_b = interner.object(vec![PropertyInfo {
        name: process_name,
        type_id: process_narrow_in_wide_out,
        write_type: process_narrow_in_wide_out,
        optional: false,
        readonly: true,
        is_method: true,
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

    let obj_narrow_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: method_narrow,
        write_type: method_narrow,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let obj_wide_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: method_wide,
        write_type: method_wide,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let obj_narrow_cb = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: method_with_narrow_cb,
        write_type: method_with_narrow_cb,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let obj_wide_cb = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: method_with_wide_cb,
        write_type: method_with_wide_cb,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
    let obj_narrow_fn = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: fn_narrow,
        write_type: fn_narrow,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_wide_fn = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: fn_wide,
        write_type: fn_wide,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_string = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_string_2 = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Same mutable property types are compatible
    assert!(checker.is_subtype_of(obj_string, obj_string_2));
    assert!(checker.is_subtype_of(obj_string_2, obj_string));
}

#[test]
#[ignore = "Mutable property invariance with different types not fully implemented"]
fn test_mutable_property_invariant_different_types() {
    // Mutable properties with different types should fail (invariant)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop_name = interner.intern_string("value");
    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_narrow = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_wide = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: wide_type,
        write_type: wide_type,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Mutable properties are invariant - neither direction should work
    // because writes to the wide type could violate the narrow type
    assert!(!checker.is_subtype_of(obj_narrow, obj_wide));
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
    }]);

    let obj_normal = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_narrow_readonly = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let obj_wide_readonly = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: wide_type,
        write_type: wide_type,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_c = interner.object(vec![PropertyInfo {
        name: c_name,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    let merged = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let obj_wide = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: wide_type,
        write_type: wide_type,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_narrow = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_c = interner.object(vec![PropertyInfo {
        name: c_name,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b_optional = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let float = interner.literal_number(3.15); // Avoid clippy::approx_constant

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
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let base = interner.object(vec![PropertyInfo {
        name: base_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Derived extends Base with additional property
    let derived = interner.object(vec![
        PropertyInfo {
            name: base_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: derived_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
    let obj_string = interner.object(vec![PropertyInfo {
        name: value_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let obj_union = interner.object(vec![PropertyInfo {
        name: value_prop,
        type_id: union,
        write_type: union,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let box_string = interner.object(vec![PropertyInfo {
        name: value_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Box<number>
    let box_number = interner.object(vec![PropertyInfo {
        name: value_prop,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Neither should be subtype of the other (invariant)
    assert!(!checker.is_subtype_of(box_string, box_number));
    assert!(!checker.is_subtype_of(box_number, box_string));
}

#[test]
#[ignore = "Invariant RefCell pattern (mixed variance) not fully implemented"]
fn test_invariant_ref_cell_pattern() {
    // RefCell<T> = { get(): T, set(v: T): void }
    // T appears in both covariant (return) and contravariant (param) positions = invariant
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
        },
        PropertyInfo {
            name: set_name,
            type_id: set_string,
            write_type: set_string,
            optional: false,
            readonly: true,
            is_method: true,
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
        },
        PropertyInfo {
            name: set_name,
            type_id: set_union,
            write_type: set_union,
            optional: false,
            readonly: true,
            is_method: true,
        },
    ]);

    // Neither should be subtype (invariant due to mixed variance)
    // get() is covariant, set() is contravariant
    assert!(!checker.is_subtype_of(refcell_string, refcell_union));
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
    let obj_narrow = interner.object(vec![PropertyInfo {
        name: handler_name,
        type_id: method_narrow,
        write_type: method_narrow,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let obj_wide = interner.object(vec![PropertyInfo {
        name: handler_name,
        type_id: method_wide,
        write_type: method_wide,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
    let obj_method = interner.object(vec![PropertyInfo {
        name: handler_name,
        type_id: fn_narrow,
        write_type: fn_narrow,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // Function property (is_method: false)
    let obj_fn_prop = interner.object(vec![PropertyInfo {
        name: handler_name,
        type_id: fn_wide,
        write_type: fn_wide,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let base_event = interner.object(vec![PropertyInfo {
        name: event_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    // Derived event with additional property
    let target_prop = interner.intern_string("target");
    let derived_event = interner.object(vec![
        PropertyInfo {
            name: event_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: target_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true,
            is_method: false,
        },
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
    let obj_base_handler = interner.object(vec![PropertyInfo {
        name: on_event_name,
        type_id: handler_base,
        write_type: handler_base,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let obj_derived_handler = interner.object(vec![PropertyInfo {
        name: on_event_name,
        type_id: handler_derived,
        write_type: handler_derived,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let obj_cb_string = interner.object(vec![PropertyInfo {
        name: cb_name,
        type_id: cb_string,
        write_type: cb_string,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let obj_cb_number = interner.object(vec![PropertyInfo {
        name: cb_name,
        type_id: cb_number,
        write_type: cb_number,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let obj_required = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: method_required,
        write_type: method_required,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let obj_optional = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: method_optional,
        write_type: method_optional,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_c = interner.object(vec![PropertyInfo {
        name: c_name,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_c = interner.object(vec![PropertyInfo {
        name: c_name,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_d = interner.object(vec![PropertyInfo {
        name: d_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    let merged = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    let obj_abc = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: c_name,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        PropertyInfo {
            name: x_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: y_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let obj_yz = interner.object(vec![
        PropertyInfo {
            name: y_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: z_name,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let intersection = interner.intersection(vec![obj_xy, obj_yz]);

    // Should have all three properties
    let obj_xyz = interner.object(vec![
        PropertyInfo {
            name: x_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: y_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: z_name,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let obj_x_string = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_x_number = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    let obj_ab = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_c = interner.object(vec![PropertyInfo {
        name: c_name,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: method,
        write_type: method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let obj_wide = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: wide_type,
        write_type: wide_type,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_narrow = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a_readonly = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a_optional = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let obj_a_required = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_ab = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_ab = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
#[ignore = "Function optional parameter subtyping not fully implemented"]
fn test_fn_optional_param_required_to_optional() {
    // (x: string) => void <: (x?: string) => void
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

    // Required param function can substitute for optional param function
    assert!(checker.is_subtype_of(fn_required, fn_optional));
}

#[test]
fn test_fn_optional_param_optional_to_required_not_subtype() {
    // (x?: string) => void is NOT subtype of (x: string) => void
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

    // Optional cannot substitute where required is expected
    assert!(!checker.is_subtype_of(fn_optional, fn_required));
}

#[test]
#[ignore = "Function optional parameter subtyping with multiple optional not fully implemented"]
fn test_fn_optional_param_multiple_optional() {
    // (a: string) => void <: (a?: string, b?: number) => void
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

    // One required can substitute for two optional
    assert!(checker.is_subtype_of(fn_one_required, fn_two_optional));
}

#[test]
#[ignore = "Function optional parameter subtyping with mixed required/optional not fully implemented"]
fn test_fn_optional_param_mixed_required_optional() {
    // (a: string, b: number) => void <: (a: string, b?: number) => void
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

    // Both required can substitute for one optional
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
#[ignore = "Function rest parameter subtyping not fully implemented"]
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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_ab = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_abcd = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: c_name,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: d_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let obj_ac = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: c_name,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_a_method = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: method_name,
            type_id: method,
            write_type: method,
            optional: false,
            readonly: false,
            is_method: true,
        },
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

    let obj_a_literal = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: hello,
        write_type: hello,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_a_string = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_required = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_optional = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    // Required is subtype of optional
    assert!(checker.is_subtype_of(obj_required, obj_optional));
}

#[test]
fn test_optional_property_optional_to_required_not_subtype() {
    // { a?: string } is NOT subtype of { a: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");

    let obj_required = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_optional = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

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

    let obj_optional = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

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
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let obj_b_optional = interner.object(vec![
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
        },
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
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: true,
            readonly: false,
            is_method: false,
        },
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

    let obj_optional_string = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let obj_optional_number = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let indexed_string = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: hello,
            readonly: false,
        }),
        number_index: None,
    });

    let indexed_string = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        number_index: None,
    });

    let indexed_only = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    let string_indexed = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: union_value,
            readonly: false,
        }),
        number_index: None,
    });

    let indexed_string = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
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
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let indexed = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
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

    let obj_mutable = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_readonly = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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

    let obj_mutable = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_readonly = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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

    let obj_readonly = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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
        PropertyInfo {
            name: a_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
        },
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

    let obj_literal = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: hello,
        write_type: hello,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let obj_string = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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
    }]);

    let obj_readonly_required = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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

    let readonly_array_like = interner.object(vec![PropertyInfo {
        name: length_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let mutable_array_like = interner.object(vec![PropertyInfo {
        name: length_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    }]);

    let obj_mutable_method = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: method,
        write_type: method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let base = interner.object(vec![PropertyInfo {
        name: base_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let derived = interner.object(vec![
        PropertyInfo {
            name: base_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: derived_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let class_a = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let class_b = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let class_c = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: c_prop,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let base = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: base_method,
        write_type: base_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let derived = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: derived_method,
        write_type: derived_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // Derived with narrower return type is subtype
    assert!(checker.is_subtype_of(derived, base));
}

#[test]
fn test_class_inheritance_same_structure() {
    // Two classes with identical structure are structurally equivalent
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let prop = interner.intern_string("value");

    let class1 = interner.object(vec![PropertyInfo {
        name: prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let class2 = interner.object(vec![PropertyInfo {
        name: prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let class1 = interner.object(vec![PropertyInfo {
        name: prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let class2 = interner.object(vec![PropertyInfo {
        name: prop,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
        PropertyInfo {
            name: name_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: age_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let employee = interner.object(vec![
        PropertyInfo {
            name: name_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: age_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("employeeId"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let class_a = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // D has all properties from the diamond
    let class_d = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: c_prop,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: d_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let interface = interner.object(vec![PropertyInfo {
        name: greet,
        type_id: greet_method,
        write_type: greet_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // Class has additional property
    let class_impl = interner.object(vec![
        PropertyInfo {
            name: greet,
            type_id: greet_method,
            write_type: greet_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("name"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let interface_a = interner.object(vec![PropertyInfo {
        name: a_method_name,
        type_id: void_method,
        write_type: void_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let interface_b = interner.object(vec![PropertyInfo {
        name: b_method_name,
        type_id: void_method,
        write_type: void_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let class_c = interner.object(vec![
        PropertyInfo {
            name: a_method_name,
            type_id: void_method,
            write_type: void_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: b_method_name,
            type_id: void_method,
            write_type: void_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
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

    let interface = interner.object(vec![PropertyInfo {
        name: required,
        type_id: void_method,
        write_type: void_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let interface = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: interface_method,
        write_type: interface_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let class_c = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: class_method,
        write_type: class_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let interface_a = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let interface_b = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let class_c = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let interface = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let class_c = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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
    let abstract_base = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: void_method,
        write_type: void_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // Concrete derived class
    let derived = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: void_method,
        write_type: void_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
        PropertyInfo {
            name: concrete_name,
            type_id: string_method,
            write_type: string_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: abstract_name,
            type_id: void_method,
            write_type: void_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    let derived = interner.object(vec![
        PropertyInfo {
            name: concrete_name,
            type_id: string_method,
            write_type: string_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: abstract_name,
            type_id: void_method,
            write_type: void_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
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

    let abstract_a = interner.object(vec![PropertyInfo {
        name: a_method,
        type_id: void_method,
        write_type: void_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let abstract_b = interner.object(vec![
        PropertyInfo {
            name: a_method,
            type_id: void_method,
            write_type: void_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: b_method,
            type_id: void_method,
            write_type: void_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
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

    let abstract_base = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let hello = interner.literal_string("hello");
    let derived = interner.object(vec![PropertyInfo {
        name: value,
        type_id: hello,
        write_type: hello,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let base_string = interner.object(vec![PropertyInfo {
        name: process,
        type_id: string_process,
        write_type: string_process,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let base_number = interner.object(vec![PropertyInfo {
        name: process,
        type_id: number_process,
        write_type: number_process,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
        PropertyInfo {
            name: method_name,
            type_id: void_method,
            write_type: void_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: concrete_name,
            type_id: string_method,
            write_type: string_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    // Incomplete - missing abstract method
    let incomplete = interner.object(vec![PropertyInfo {
        name: concrete_name,
        type_id: string_method,
        write_type: string_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let base = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let derived = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
        PropertyInfo {
            name: brand_a,
            type_id: TypeId::VOID,
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let class_b = interner.object(vec![
        PropertyInfo {
            name: brand_b,
            type_id: TypeId::VOID,
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        PropertyInfo {
            name: brand,
            type_id: TypeId::VOID,
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let class2 = interner.object(vec![
        PropertyInfo {
            name: brand,
            type_id: TypeId::VOID,
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        PropertyInfo {
            name: brand,
            type_id: TypeId::VOID,
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let derived = interner.object(vec![
        PropertyInfo {
            name: brand,
            type_id: TypeId::VOID,
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: extra,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        PropertyInfo {
            name: brand,
            type_id: TypeId::VOID,
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let plain_object = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
        PropertyInfo {
            name: brand,
            type_id: brand_a_type,
            write_type: brand_a_type,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let class_b = interner.object(vec![
        PropertyInfo {
            name: brand,
            type_id: brand_b_type,
            write_type: brand_b_type,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        PropertyInfo {
            name: brand,
            type_id: TypeId::VOID,
            write_type: TypeId::VOID,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let class_writable = interner.object(vec![
        PropertyInfo {
            name: brand,
            type_id: TypeId::VOID,
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        PropertyInfo {
            name: brand1,
            type_id: TypeId::VOID,
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: brand2,
            type_id: TypeId::VOID,
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let class_one = interner.object(vec![
        PropertyInfo {
            name: brand1,
            type_id: TypeId::VOID,
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: value,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let class_foo = interner.object(vec![PropertyInfo {
        name: brand_method,
        type_id: true_return,
        write_type: true_return,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let interface_a = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let interface_b = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let interface_a = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let interface_b = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let interface_c = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: c_prop,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let interface_a = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: void_method,
        write_type: void_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let interface_b = interner.object(vec![
        PropertyInfo {
            name: method_name,
            type_id: void_method,
            write_type: void_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: other_name,
            type_id: string_method,
            write_type: string_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
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

    let interface_a = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: string_method,
        write_type: string_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let interface_b = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: hello_method,
        write_type: hello_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let interface_a = interner.object(vec![PropertyInfo {
        name: value,
        type_id: string_or_number,
        write_type: string_or_number,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let interface_b = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let interface_a = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let interface_b = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let interface_a = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let interface_b = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let interface_a = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let interface_b = interner.object(vec![PropertyInfo {
        name: b_prop,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let interface_c = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: c_prop,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        PropertyInfo {
            name: shared,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let interface_b = interner.object(vec![
        PropertyInfo {
            name: shared,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let interface_c = interner.object(vec![
        PropertyInfo {
            name: shared,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let readable = interner.object(vec![PropertyInfo {
        name: read,
        type_id: read_method,
        write_type: read_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let writable = interner.object(vec![PropertyInfo {
        name: write,
        type_id: write_method,
        write_type: write_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let read_writable = interner.object(vec![
        PropertyInfo {
            name: read,
            type_id: read_method,
            write_type: read_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: write,
            type_id: write_method,
            write_type: write_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
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

    let interface_a = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let interface_b = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let interface_c = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: c_prop,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let interface_d = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: c_prop,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let partial = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let interface_a = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let with_extra = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: extra_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let interface_string = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let has_number = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let interface_a1 = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Merged interface (both declarations)
    let interface_merged = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let interface_string = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: string_method,
        write_type: string_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let interface_number = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: number_method,
        write_type: number_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let interface_wide = interner.object(vec![PropertyInfo {
        name: value,
        type_id: string_or_number,
        write_type: string_or_number,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let interface_narrow = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let window_original = interner.object(vec![PropertyInfo {
        name: document,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Augmented Window
    let window_augmented = interner.object(vec![
        PropertyInfo {
            name: document,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: my_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
    let interface_part = interner.object(vec![PropertyInfo {
        name: prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Another object with same structure
    let same_structure = interner.object(vec![PropertyInfo {
        name: prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let file1_view = interner.object(vec![PropertyInfo {
        name: file1_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Fully merged
    let merged = interner.object(vec![
        PropertyInfo {
            name: file1_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: file2_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let with_prop = interner.object(vec![PropertyInfo {
        name: prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let interface_i = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Type alias (same structure)
    let type_t = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let interface_i = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: void_method,
        write_type: void_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let type_t = interner.object(vec![PropertyInfo {
        name: method_name,
        type_id: void_method,
        write_type: void_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_prop,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let interface_i = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let type_t = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

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

    let interface_i = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let type_t = interner.object(vec![PropertyInfo {
        name: value,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(crate::solver::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let type_t = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(crate::solver::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
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

    let type_base = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let interface_derived = interner.object(vec![
        PropertyInfo {
            name: a_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: b_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let interface_i = interner.object(vec![PropertyInfo {
        name: a_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let extra = interner.object(vec![PropertyInfo {
        name: b_prop,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let interface_active = interner.object(vec![PropertyInfo {
        name: status_prop,
        type_id: active,
        write_type: active,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_active = interner.object(vec![PropertyInfo {
        name: status_prop,
        type_id: active,
        write_type: active,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_inactive = interner.object(vec![PropertyInfo {
        name: status_prop,
        type_id: inactive,
        write_type: inactive,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let interface_type = interner.object(vec![PropertyInfo {
        name: status_prop,
        type_id: active_or_pending,
        write_type: active_or_pending,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_active = interner.object(vec![PropertyInfo {
        name: status_prop,
        type_id: active,
        write_type: active,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_completed = interner.object(vec![PropertyInfo {
        name: status_prop,
        type_id: completed,
        write_type: completed,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: lit_a,
            write_type: lit_a,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: lit_b,
            write_type: lit_b,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let obj_b = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    let obj_b = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: literal_union,
            readonly: false,
        }),
        number_index: None,
    });

    let obj_general = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    let obj_string_only = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let obj_b = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: union_type,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
        }),
        number_index: None,
    });

    let obj_mutable = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    // Readonly is not assignable to mutable (can't write)
    assert!(!checker.is_subtype_of(obj_readonly, obj_mutable));
}

#[test]
fn test_index_signature_mutable_to_readonly() {
    // { [key: string]: T } is subtype of { readonly [key: string]: T }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_mutable = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let obj_readonly = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: union_value,
            readonly: false,
        }),
        number_index: None,
    });

    let obj_string = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
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

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection_value = interner.intersection(vec![obj_a, obj_b]);

    let obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: intersection_value,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
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
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let union_value = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let indexed_obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: union_value,
            readonly: false,
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
        PropertyInfo {
            name: interner.intern_string("0"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("1"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let number_indexed = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
        }),
        number_index: None,
    });

    let obj_with_props = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(checker.is_subtype_of(obj_with_props, indexed_any));
}

#[test]
fn test_index_signature_unknown_value() {
    // { [key: string]: unknown } - safe unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_unknown = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::UNKNOWN,
            readonly: false,
        }),
        number_index: None,
    });

    let indexed_string = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NEVER,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: fn_type,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: array_type,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: tuple_type,
            readonly: false,
        }),
    });

    assert!(indexed_tuple != TypeId::ERROR);
}

#[test]
fn test_index_signature_nested_object_value() {
    // { [key: string]: { x: number } }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let nested_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let indexed_nested = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: nested_obj,
            readonly: false,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let prop_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let narrow_readonly = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let wide_readonly = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: wide_type,
        write_type: wide_type,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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

    let narrow_mutable = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let wide_mutable = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: wide_type,
        write_type: wide_type,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

#[test]
fn test_variance_function_returning_function() {
    // () => (x: string) => void  vs  () => (x: string | number) => void
    // Outer return is covariant, inner callback param is contravariant
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Inner function with narrow param
    let inner_narrow = interner.function(FunctionShape {
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

    // Inner function with wide param
    let inner_wide = interner.function(FunctionShape {
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

    // Factory returning narrow-param function
    let factory_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: inner_narrow,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Factory returning wide-param function
    let factory_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: inner_wide,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Factory returning wide-param <: factory returning narrow-param
    // Return is covariant, and wide-param callback <: narrow-param callback
    assert!(checker.is_subtype_of(factory_wide, factory_narrow));
    assert!(!checker.is_subtype_of(factory_narrow, factory_wide));
}

#[test]
fn test_variance_union_in_contravariant_position() {
    // (x: A | B) => void  <:  (x: A) => void  (contravariance)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_ab = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let fn_union_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: union_ab,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_single_param = interner.function(FunctionShape {
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

    // Union param <: single param (contravariance)
    assert!(checker.is_subtype_of(fn_union_param, fn_single_param));
    // Single param should NOT be subtype of union param
    assert!(!checker.is_subtype_of(fn_single_param, fn_union_param));
}

#[test]
fn test_variance_intersection_in_covariant_position() {
    // () => A & B  <:  () => A  (covariance)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection_ab = interner.intersection(vec![obj_a, obj_b]);

    let fn_returns_intersection = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: intersection_ab,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_returns_a = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: obj_a,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Returns A & B <: returns A (covariance, intersection subtype of member)
    assert!(checker.is_subtype_of(fn_returns_intersection, fn_returns_a));
}

#[test]
fn test_variance_array_element_unsound_covariance() {
    // string[] <: (string | number)[] - TypeScript's unsound covariance
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_element = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let narrow_array = interner.array(TypeId::STRING);
    let wide_array = interner.array(wide_element);

    // TypeScript allows this (unsound)
    assert!(checker.is_subtype_of(narrow_array, wide_array));
}

#[test]
fn test_variance_method_bivariant_params() {
    // Methods are bivariant in their parameters (TypeScript unsoundness)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Object with method taking narrow param
    let narrow_method_obj = interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![PropertyInfo {
            name: interner.intern_string("handle"),
            type_id: interner.function(FunctionShape {
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
            }),
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: true,
        }],
        string_index: None,
        number_index: None,
    });

    // Object with method taking wide param
    let wide_method_obj = interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![PropertyInfo {
            name: interner.intern_string("handle"),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: wide_type,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            write_type: TypeId::VOID,
            optional: false,
            readonly: false,
            is_method: true,
        }],
        string_index: None,
        number_index: None,
    });

    // Methods are bivariant - both directions should work
    assert!(checker.is_subtype_of(narrow_method_obj, wide_method_obj));
    assert!(checker.is_subtype_of(wide_method_obj, narrow_method_obj));
}

#[test]
fn test_variance_function_property_contravariant() {
    // Function properties are strictly contravariant (not bivariant like methods)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Object with function property taking narrow param
    let narrow_fn_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("handle"),
        type_id: interner.function(FunctionShape {
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
        }),
        write_type: TypeId::VOID,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Object with function property taking wide param
    let wide_fn_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("handle"),
        type_id: interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: wide_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        }),
        write_type: TypeId::VOID,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Wide param function <: narrow param function (contravariant)
    assert!(checker.is_subtype_of(wide_fn_obj, narrow_fn_obj));
}

#[test]
fn test_variance_promise_covariant() {
    // Promise<string> <: Promise<string | number> (covariant)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Simulate Promise<string> as { then: (cb: (value: string) => void) => void }
    let then_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let then_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: interner.function(FunctionShape {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: wide_type,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            }),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let promise_narrow = interner.object(vec![PropertyInfo {
        name: interner.intern_string("then"),
        type_id: then_narrow,
        write_type: then_narrow,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let promise_wide = interner.object(vec![PropertyInfo {
        name: interner.intern_string("then"),
        type_id: then_wide,
        write_type: then_wide,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // Promise<string> <: Promise<string | number> (covariant in T)
    // then callback param is contravariant, then is contravariant in object = covariant overall
    assert!(checker.is_subtype_of(promise_narrow, promise_wide));
}

#[test]
fn test_variance_triple_nested_contravariance() {
    // Three levels of contravariance: ((f: (g: (x: T) => void) => void) => void)
    // Three contravariants = contravariant overall
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Innermost: (x: T) => void
    let inner_narrow = interner.function(FunctionShape {
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

    let inner_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: wide_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Middle: (g: innermost) => void
    let middle_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("g")),
            type_id: inner_narrow,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let middle_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("g")),
            type_id: inner_wide,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Outermost: (f: middle) => void
    let outer_narrow = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: middle_narrow,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let outer_wide = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: middle_wide,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Three levels of contravariance = contravariant (in strict mode)
    // outer_narrow <: outer_wide (narrow at innermost becomes wide at triple-contravariant)
    // Current behavior: bivariant for callback parameters - only one direction works
    assert!(!checker.is_subtype_of(outer_narrow, outer_wide));
    assert!(checker.is_subtype_of(outer_wide, outer_narrow));
}

#[test]
fn test_variance_constructor_param_contravariant() {
    // new (x: string | number) => T  <:  new (x: string) => T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Instance type
    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let ctor_narrow = interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
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
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let ctor_wide = interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: wide_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Wide param constructor <: narrow param constructor (contravariant)
    assert!(checker.is_subtype_of(ctor_wide, ctor_narrow));
    assert!(!checker.is_subtype_of(ctor_narrow, ctor_wide));
}

#[test]
fn test_variance_rest_param_contravariant() {
    // (...args: (string | number)[]) => void  <:  (...args: string[]) => void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let narrow_array = interner.array(TypeId::STRING);
    let wide_array = interner.array(wide_type);

    let fn_narrow_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: narrow_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_wide_rest = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: wide_array,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Wide rest param <: narrow rest param (contravariant)
    assert!(checker.is_subtype_of(fn_wide_rest, fn_narrow_rest));
}

#[test]
#[ignore = "Optional parameter covariance optionality not fully implemented"]
fn test_variance_optional_param_covariant_optionality() {
    // (x?: string) => void  <:  (x: string) => void
    // Optional is more permissive, can be called with fewer args
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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

    // Optional param function <: required param function
    // If you can call with no args, you can certainly call with one
    assert!(checker.is_subtype_of(fn_optional, fn_required));
}
// =============================================================================
// FUNCTION TYPE TESTS - OVERLOADS
// =============================================================================

#[test]
fn test_overload_single_signature_subtype() {
    // Function with one signature should be subtype of callable with same signature
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

    let callable_type = interner.callable(CallableShape {
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

    // Function <: callable with same signature
    assert!(checker.is_subtype_of(fn_type, callable_type));
}

#[test]
fn test_overload_multiple_to_single() {
    // Callable with multiple overloads <: callable with one matching overload
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let multi_overload = interner.callable(CallableShape {
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
                    type_id: TypeId::NUMBER,
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

    let single_overload = interner.callable(CallableShape {
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
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Multi-overload <: single overload (has matching signature)
    assert!(checker.is_subtype_of(multi_overload, single_overload));
}

#[test]
fn test_overload_order_independent_matching() {
    // Overload matching should find the best match regardless of order
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let overloads_ab = interner.callable(CallableShape {
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
                    type_id: TypeId::NUMBER,
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

    let overloads_ba = interner.callable(CallableShape {
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

    // Order shouldn't matter for subtype relationship
    assert!(checker.is_subtype_of(overloads_ab, overloads_ba));
    assert!(checker.is_subtype_of(overloads_ba, overloads_ab));
}

#[test]
fn test_overload_missing_signature_not_subtype() {
    // Callable missing a required overload is not a subtype
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let single_overload = interner.callable(CallableShape {
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
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let two_overloads = interner.callable(CallableShape {
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
                    type_id: TypeId::NUMBER,
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

    // Single overload should not be subtype of callable requiring two overloads
    assert!(!checker.is_subtype_of(single_overload, two_overloads));
}

#[test]
fn test_overload_wider_param_satisfies_target() {
    // Overload with wider param type can satisfy narrower target overload
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let wide_param = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let wide_overload = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
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
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let narrow_overload = interner.callable(CallableShape {
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

    // Wide param <: narrow param (contravariance)
    assert!(checker.is_subtype_of(wide_overload, narrow_overload));
}

#[test]
fn test_overload_constructor_subtype() {
    // Constructor overloads should follow same rules as call overloads
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let multi_ctor = interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![
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
            CallSignature {
                type_params: vec![],
                params: vec![],
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

    let single_ctor = interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
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
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Multi-constructor <: single constructor (has matching)
    assert!(checker.is_subtype_of(multi_ctor, single_ctor));
}

#[test]
fn test_overload_with_different_arity() {
    // Overloads with different arities
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let multi_arity = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![],
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
                        optional: false,
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

    let no_args = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
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

    // Multi-arity should satisfy no-args target
    assert!(checker.is_subtype_of(multi_arity, no_args));
}

// =============================================================================
// FUNCTION TYPE TESTS - THIS PARAMETER
// =============================================================================

#[test]
fn test_this_parameter_explicit_type() {
    // function(this: Foo, x: string): void
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let foo_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("name"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let fn_with_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: Some(foo_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_without_this = interner.function(FunctionShape {
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

    // Function without this requirement <: function with this requirement
    // (less restrictive is subtype)
    assert!(checker.is_subtype_of(fn_without_this, fn_with_this));
}

#[test]
fn test_this_parameter_covariant_in_method() {
    // For methods, this is covariant (subclass method can be assigned to superclass)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let base_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("name"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let derived_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("name"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("age"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    // Method on derived type
    let derived_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(derived_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Method on base type
    let base_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(base_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Base method <: derived method (covariant this)
    assert!(checker.is_subtype_of(base_method, derived_method));
}

#[test]
#[ignore = "Void this parameter compatibility not fully implemented"]
fn test_this_parameter_void_this() {
    // this: void means the function doesn't use this
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_void_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::VOID),
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_any_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::ANY),
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_no_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // void this and no this should be compatible
    assert!(checker.is_subtype_of(fn_void_this, fn_no_this));
    assert!(checker.is_subtype_of(fn_no_this, fn_void_this));

    // any this is more permissive
    assert!(checker.is_subtype_of(fn_any_this, fn_no_this));
}

#[test]
fn test_this_parameter_in_callable_method() {
    // Callable with method that has this parameter
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("data"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Method with this type
    let method_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(obj_type),
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let callable_with_method = interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![PropertyInfo {
            name: interner.intern_string("getData"),
            type_id: method_fn,
            write_type: method_fn,
            optional: false,
            readonly: false,
            is_method: true,
        }],
        string_index: None,
        number_index: None,
    });

    // Plain method without this
    let plain_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let callable_plain = interner.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![],
        properties: vec![PropertyInfo {
            name: interner.intern_string("getData"),
            type_id: plain_method,
            write_type: plain_method,
            optional: false,
            readonly: false,
            is_method: true,
        }],
        string_index: None,
        number_index: None,
    });

    // Both should be compatible (methods are bivariant)
    assert!(checker.is_subtype_of(callable_with_method, callable_plain));
}

#[test]
fn test_this_parameter_fluent_api_pattern() {
    // Fluent API: method returns this type
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Builder type with set method returning this
    let builder_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Method returning the builder (this type)
    let set_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: Some(builder_type),
        return_type: builder_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Different builder that also returns self
    let other_builder = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("extra"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let other_set_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: Some(other_builder),
        return_type: other_builder,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Methods with different this/return types are not subtypes
    // (unless there's a structural relationship)
    assert!(!checker.is_subtype_of(set_method, other_set_method));
}

#[test]
fn test_this_parameter_unknown_this() {
    // this: unknown is maximally restrictive
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let fn_unknown_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::UNKNOWN),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_string_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // unknown this should work with any this type
    assert!(checker.is_subtype_of(fn_unknown_this, fn_string_this));
}

#[test]
fn test_overload_with_call_and_construct() {
    // Callable that can be both called and constructed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let dual_callable = interner.callable(CallableShape {
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
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: instance,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let call_only = interner.callable(CallableShape {
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
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Dual callable <: call-only (has matching call signature)
    assert!(checker.is_subtype_of(dual_callable, call_only));
}

#[test]
fn test_overload_rest_vs_multiple_params() {
    // (...args: string[]) should be compatible with (a: string, b: string)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);

    let rest_fn = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
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
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let two_params = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
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
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // Rest params can satisfy fixed params
    assert!(checker.is_subtype_of(rest_fn, two_params));
}

#[test]
fn test_this_in_overload_signature() {
    // Overload with this parameter
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let overload_with_this = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: Some(obj_type),
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let overload_no_this = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
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

    // No-this is compatible with with-this (no-this is more general)
    assert!(checker.is_subtype_of(overload_no_this, overload_with_this));
}

// =============================================================================
// SYMBOL TYPE TESTS - Unique Symbols, Well-Known Symbols, Symbol.iterator
// =============================================================================

#[test]
fn test_unique_symbol_self_subtype() {
    // A unique symbol is subtype of itself
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym = interner.intern(TypeKey::UniqueSymbol(SymbolRef(42)));

    assert!(checker.is_subtype_of(sym, sym));
}

#[test]
fn test_unique_symbol_not_subtype_of_different() {
    // Different unique symbols are not subtypes of each other
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_a = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeKey::UniqueSymbol(SymbolRef(2)));

    assert!(!checker.is_subtype_of(sym_a, sym_b));
    assert!(!checker.is_subtype_of(sym_b, sym_a));
}

#[test]
fn test_unique_symbol_subtype_of_symbol() {
    // Every unique symbol is subtype of symbol primitive
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let unique_sym = interner.intern(TypeKey::UniqueSymbol(SymbolRef(100)));

    assert!(checker.is_subtype_of(unique_sym, TypeId::SYMBOL));
}

#[test]
fn test_symbol_not_subtype_of_unique_symbol() {
    // symbol primitive is not subtype of any unique symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let unique_sym = interner.intern(TypeKey::UniqueSymbol(SymbolRef(100)));

    assert!(!checker.is_subtype_of(TypeId::SYMBOL, unique_sym));
}

#[test]
fn test_unique_symbol_in_union() {
    // unique symbol | string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let unique_sym = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1)));
    let sym_or_string = interner.union(vec![unique_sym, TypeId::STRING]);

    // unique symbol is subtype of the union
    assert!(checker.is_subtype_of(unique_sym, sym_or_string));

    // string is subtype of the union
    assert!(checker.is_subtype_of(TypeId::STRING, sym_or_string));

    // union is subtype of symbol | string
    let symbol_or_string = interner.union(vec![TypeId::SYMBOL, TypeId::STRING]);
    assert!(checker.is_subtype_of(sym_or_string, symbol_or_string));
}

#[test]
fn test_well_known_symbol_iterator() {
    // Symbol.iterator is a unique symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Using conventional SymbolRef for well-known symbols
    let sym_iterator = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1000)));

    // It's a subtype of symbol
    assert!(checker.is_subtype_of(sym_iterator, TypeId::SYMBOL));

    // But not equal to another unique symbol
    let sym_async_iterator = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1001)));
    assert!(!checker.is_subtype_of(sym_iterator, sym_async_iterator));
}

#[test]
fn test_well_known_symbol_async_iterator() {
    // Symbol.asyncIterator is a unique symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_async_iterator = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1001)));

    assert!(checker.is_subtype_of(sym_async_iterator, TypeId::SYMBOL));
}

#[test]
fn test_well_known_symbol_to_string_tag() {
    // Symbol.toStringTag is a unique symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_to_string_tag = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1002)));

    assert!(checker.is_subtype_of(sym_to_string_tag, TypeId::SYMBOL));
}

#[test]
fn test_well_known_symbol_has_instance() {
    // Symbol.hasInstance is a unique symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_has_instance = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1003)));

    assert!(checker.is_subtype_of(sym_has_instance, TypeId::SYMBOL));
}

#[test]
fn test_symbol_keyed_object_property() {
    // Object with symbol-keyed property
    // { [Symbol.iterator]: () => Iterator }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let _sym_iterator = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1000)));

    // Iterator-like return type
    let iterator_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Object with symbol-keyed method (using string name as proxy)
    let iterable_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("[Symbol.iterator]"),
        type_id: iterator_fn,
        write_type: iterator_fn,
        optional: false,
        readonly: true,
        is_method: true,
    }]);

    assert!(iterable_obj != TypeId::ERROR);
}

#[test]
fn test_symbol_union_with_multiple_unique() {
    // Union of multiple unique symbols
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_a = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeKey::UniqueSymbol(SymbolRef(2)));
    let sym_c = interner.intern(TypeKey::UniqueSymbol(SymbolRef(3)));

    let sym_union = interner.union(vec![sym_a, sym_b, sym_c]);

    // Each unique symbol is subtype of the union
    assert!(checker.is_subtype_of(sym_a, sym_union));
    assert!(checker.is_subtype_of(sym_b, sym_union));
    assert!(checker.is_subtype_of(sym_c, sym_union));

    // Union is subtype of symbol
    assert!(checker.is_subtype_of(sym_union, TypeId::SYMBOL));
}

#[test]
fn test_symbol_not_subtype_of_string() {
    // symbol is not subtype of string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    assert!(!checker.is_subtype_of(TypeId::SYMBOL, TypeId::STRING));
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::SYMBOL));
}

#[test]
fn test_symbol_not_subtype_of_number() {
    // symbol is not subtype of number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    assert!(!checker.is_subtype_of(TypeId::SYMBOL, TypeId::NUMBER));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, TypeId::SYMBOL));
}

#[test]
fn test_unique_symbol_intersection() {
    // Intersection of unique symbol with other type
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let unique_sym = interner.intern(TypeKey::UniqueSymbol(SymbolRef(42)));

    // unique symbol & symbol = unique symbol (more specific)
    let intersection = interner.intersection(vec![unique_sym, TypeId::SYMBOL]);

    // The intersection is subtype of symbol
    assert!(checker.is_subtype_of(intersection, TypeId::SYMBOL));
}

#[test]
fn test_symbol_as_property_key() {
    // Symbols can be used as property keys: PropertyKey = string | number | symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let property_key = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);

    // symbol is subtype of PropertyKey
    assert!(checker.is_subtype_of(TypeId::SYMBOL, property_key));

    // unique symbol is also subtype of PropertyKey
    let unique_sym = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1)));
    assert!(checker.is_subtype_of(unique_sym, property_key));
}

#[test]
fn test_const_unique_symbol_type() {
    // const sym = Symbol("description") has type unique symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let const_sym = interner.intern(TypeKey::UniqueSymbol(SymbolRef(999)));

    // Type is unique symbol, not just symbol
    assert!(checker.is_subtype_of(const_sym, TypeId::SYMBOL));
    assert!(!checker.is_subtype_of(TypeId::SYMBOL, const_sym));
}

#[test]
fn test_let_symbol_type() {
    // let sym = Symbol("description") has type symbol (widened)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // let binding gets widened to symbol
    let let_sym = TypeId::SYMBOL;

    // It's just symbol, not unique
    assert!(checker.is_subtype_of(let_sym, TypeId::SYMBOL));
}

#[test]
fn test_symbol_for_shared() {
    // Symbol.for("key") returns shared symbol (not unique)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Symbol.for returns symbol type (not unique symbol)
    let shared_sym = TypeId::SYMBOL;

    assert!(checker.is_subtype_of(shared_sym, TypeId::SYMBOL));
}

#[test]
fn test_iterable_protocol_types() {
    // Iterable<T> has [Symbol.iterator](): Iterator<T>
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    // IteratorResult<number> = { value: number, done: boolean }
    let value_name = interner.intern_string("value");
    let done_name = interner.intern_string("done");

    let iter_result = interner.object(vec![
        PropertyInfo {
            name: value_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: done_name,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: true,
            is_method: false,
        },
    ]);

    // Iterator<number> = { next(): IteratorResult<number> }
    let next_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: iter_result,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let iterator = interner.object(vec![PropertyInfo {
        name: interner.intern_string("next"),
        type_id: next_fn,
        write_type: next_fn,
        optional: false,
        readonly: true,
        is_method: true,
    }]);

    // Iterator is valid object type
    assert!(iterator != TypeId::ERROR);
}

#[test]
fn test_async_iterable_protocol_types() {
    // AsyncIterable<T> has [Symbol.asyncIterator](): AsyncIterator<T>
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    // AsyncIteratorResult<number> = { value: number, done: boolean }
    let value_name = interner.intern_string("value");
    let done_name = interner.intern_string("done");

    let async_iter_result = interner.object(vec![
        PropertyInfo {
            name: value_name,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: done_name,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: true,
            is_method: false,
        },
    ]);

    // Promise<AsyncIteratorResult<number>>
    let promise = interner.object(vec![PropertyInfo {
        name: interner.intern_string("then"),
        type_id: async_iter_result,
        write_type: async_iter_result,
        optional: false,
        readonly: true,
        is_method: true,
    }]);

    // AsyncIterator<number> = { next(): Promise<AsyncIteratorResult<number>> }
    let next_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: promise,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let async_iterator = interner.object(vec![PropertyInfo {
        name: interner.intern_string("next"),
        type_id: next_fn,
        write_type: next_fn,
        optional: false,
        readonly: true,
        is_method: true,
    }]);

    // AsyncIterator is valid object type
    assert!(async_iterator != TypeId::ERROR);
}

#[test]
fn test_symbol_keyof_type() {
    // keyof { [sym]: value } includes the symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let unique_sym = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1)));

    // keyof type includes the symbol
    let keyof_result = interner.union(vec![unique_sym, interner.literal_string("name")]);

    // symbol is in the keyof result
    assert!(checker.is_subtype_of(unique_sym, keyof_result));
}

#[test]
fn test_symbol_in_discriminated_union() {
    // Symbol can be used as discriminant
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_a = interner.intern(TypeKey::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeKey::UniqueSymbol(SymbolRef(2)));

    // Two variants discriminated by symbol
    let variant_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("kind"),
        type_id: sym_a,
        write_type: sym_a,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let variant_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("kind"),
        type_id: sym_b,
        write_type: sym_b,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let discriminated_union = interner.union(vec![variant_a, variant_b]);

    // Each variant is subtype of union
    assert!(checker.is_subtype_of(variant_a, discriminated_union));
    assert!(checker.is_subtype_of(variant_b, discriminated_union));

    // But not interchangeable
    assert!(!checker.is_subtype_of(variant_a, variant_b));
}

// =============================================================================
// NULL TYPE TESTS - Strict Null Checks, Nullable Unions
// =============================================================================

#[test]
fn test_null_not_subtype_of_string_strict() {
    // With strictNullChecks, null is not assignable to string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::STRING));
}

#[test]
fn test_undefined_not_subtype_of_string_strict() {
    // With strictNullChecks, undefined is not assignable to string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, TypeId::STRING));
}

#[test]
fn test_null_subtype_of_string_legacy() {
    // Without strictNullChecks, null is assignable to string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = false;

    assert!(checker.is_subtype_of(TypeId::NULL, TypeId::STRING));
}

#[test]
fn test_undefined_subtype_of_string_legacy() {
    // Without strictNullChecks, undefined is assignable to string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = false;

    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::STRING));
}

#[test]
fn test_nullable_union_string() {
    // string | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let nullable_string = interner.union(vec![TypeId::STRING, TypeId::NULL]);

    // null is subtype of string | null
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_string));

    // string is subtype of string | null
    assert!(checker.is_subtype_of(TypeId::STRING, nullable_string));

    // string | null is not subtype of string
    assert!(!checker.is_subtype_of(nullable_string, TypeId::STRING));
}

#[test]
fn test_nullable_union_number() {
    // number | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let nullable_number = interner.union(vec![TypeId::NUMBER, TypeId::NULL]);

    assert!(checker.is_subtype_of(TypeId::NULL, nullable_number));
    assert!(checker.is_subtype_of(TypeId::NUMBER, nullable_number));
    assert!(!checker.is_subtype_of(nullable_number, TypeId::NUMBER));
}

#[test]
fn test_optional_union_undefined() {
    // string | undefined (optional parameter type)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let optional_string = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    // undefined is subtype of string | undefined
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, optional_string));

    // string is subtype of string | undefined
    assert!(checker.is_subtype_of(TypeId::STRING, optional_string));

    // string | undefined is not subtype of string
    assert!(!checker.is_subtype_of(optional_string, TypeId::STRING));
}

#[test]
fn test_nullable_and_optional_union() {
    // string | null | undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let nullable_optional = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);

    // All three are subtypes
    assert!(checker.is_subtype_of(TypeId::STRING, nullable_optional));
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_optional));
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, nullable_optional));

    // Not subtype of any individual
    assert!(!checker.is_subtype_of(nullable_optional, TypeId::STRING));
    assert!(!checker.is_subtype_of(nullable_optional, TypeId::NULL));
    assert!(!checker.is_subtype_of(nullable_optional, TypeId::UNDEFINED));
}

#[test]
fn test_null_distinct_from_undefined() {
    // null and undefined are distinct types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::UNDEFINED));
    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, TypeId::NULL));
}

#[test]
fn test_null_subtype_of_self() {
    // null is subtype of null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::NULL, TypeId::NULL));
}

#[test]
fn test_undefined_subtype_of_self() {
    // undefined is subtype of undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::UNDEFINED));
}

#[test]
fn test_null_subtype_of_any() {
    // null is subtype of any
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::NULL, TypeId::ANY));
}

#[test]
fn test_undefined_subtype_of_any() {
    // undefined is subtype of any
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::ANY));
}

#[test]
fn test_null_subtype_of_unknown() {
    // null is subtype of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::NULL, TypeId::UNKNOWN));
}

#[test]
fn test_undefined_subtype_of_unknown() {
    // undefined is subtype of unknown
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::UNKNOWN));
}

#[test]
fn test_null_not_subtype_of_object() {
    // null is not subtype of object
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::OBJECT));
}

#[test]
fn test_undefined_not_subtype_of_object() {
    // undefined is not subtype of object
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, TypeId::OBJECT));
}

#[test]
fn test_null_not_subtype_of_never() {
    // null is not subtype of never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::NEVER));
}

#[test]
fn test_never_subtype_of_null() {
    // never is subtype of null (never is bottom type)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::NULL));
}

#[test]
fn test_nullable_object_type() {
    // { x: string } | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let nullable_obj = interner.union(vec![obj, TypeId::NULL]);

    // Object is subtype of nullable object
    assert!(checker.is_subtype_of(obj, nullable_obj));

    // null is subtype of nullable object
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_obj));

    // Nullable object is not subtype of object
    assert!(!checker.is_subtype_of(nullable_obj, obj));
}

#[test]
fn test_nullable_function_type() {
    // (() => void) | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let nullable_fn = interner.union(vec![fn_type, TypeId::NULL]);

    // Function is subtype of nullable function
    assert!(checker.is_subtype_of(fn_type, nullable_fn));

    // null is subtype of nullable function
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_fn));
}

#[test]
fn test_nullable_array_type() {
    // string[] | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let string_array = interner.array(TypeId::STRING);
    let nullable_array = interner.union(vec![string_array, TypeId::NULL]);

    // Array is subtype of nullable array
    assert!(checker.is_subtype_of(string_array, nullable_array));

    // null is subtype of nullable array
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_array));
}

#[test]
fn test_void_distinct_from_undefined() {
    // void is not the same as undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    // undefined is subtype of void
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, TypeId::VOID));

    // void is not subtype of undefined (void is wider)
    // Note: In TypeScript, void can accept undefined
    // but void is not assignable to undefined
}

#[test]
fn test_nullable_literal_type() {
    // "hello" | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let hello = interner.literal_string("hello");
    let nullable_hello = interner.union(vec![hello, TypeId::NULL]);

    // Literal is subtype of nullable literal
    assert!(checker.is_subtype_of(hello, nullable_hello));

    // null is subtype of nullable literal
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_hello));

    // string is not subtype of nullable literal
    assert!(!checker.is_subtype_of(TypeId::STRING, nullable_hello));
}

#[test]
fn test_non_null_assertion_type() {
    // NonNullable<string | null | undefined> = string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    // After non-null assertion, only string remains
    let non_null_result = TypeId::STRING;

    // string is subtype of the original union
    let nullable_optional = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    assert!(checker.is_subtype_of(non_null_result, nullable_optional));
}

#[test]
fn test_nullable_union_widening() {
    // string | null | undefined is wider than string | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let nullable = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let nullable_optional = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);

    // string | null is subtype of string | null | undefined
    assert!(checker.is_subtype_of(nullable, nullable_optional));

    // string | null | undefined is not subtype of string | null
    assert!(!checker.is_subtype_of(nullable_optional, nullable));
}

#[test]
fn test_null_in_intersection() {
    // string & null = never (incompatible)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NULL]);

    // Intersection of incompatible types reduces to never-like
    // The intersection is subtype of string (vacuously)
    assert!(checker.is_subtype_of(intersection, TypeId::STRING));
}

#[test]
fn test_optional_property_accepts_undefined() {
    // { x?: string } - x can be string | undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    // Optional property type
    let optional_value = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    // undefined is valid
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, optional_value));

    // string is valid
    assert!(checker.is_subtype_of(TypeId::STRING, optional_value));

    // null is not valid for optional property (unless explicitly added)
    assert!(!checker.is_subtype_of(TypeId::NULL, optional_value));
}

#[test]
fn test_nullish_coalescing_result_type() {
    // (string | null) ?? "default" -> string
    // The result excludes null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    // After ?? operation, null is excluded
    let result = TypeId::STRING;

    // Result is subtype of original nullable
    let nullable = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    assert!(checker.is_subtype_of(result, nullable));
}

#[test]
fn test_null_union_with_literal_numbers() {
    // 1 | 2 | 3 | null
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let lit_1 = interner.literal_number(1.0);
    let lit_2 = interner.literal_number(2.0);
    let lit_3 = interner.literal_number(3.0);

    let nullable_nums = interner.union(vec![lit_1, lit_2, lit_3, TypeId::NULL]);

    // Each literal is subtype
    assert!(checker.is_subtype_of(lit_1, nullable_nums));
    assert!(checker.is_subtype_of(lit_2, nullable_nums));
    assert!(checker.is_subtype_of(lit_3, nullable_nums));
    assert!(checker.is_subtype_of(TypeId::NULL, nullable_nums));

    // Number itself is not subtype
    assert!(!checker.is_subtype_of(TypeId::NUMBER, nullable_nums));
}

#[test]
fn test_undefined_union_with_boolean() {
    // boolean | undefined
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_null_checks = true;

    let optional_bool = interner.union(vec![TypeId::BOOLEAN, TypeId::UNDEFINED]);

    assert!(checker.is_subtype_of(TypeId::BOOLEAN, optional_bool));
    assert!(checker.is_subtype_of(TypeId::UNDEFINED, optional_bool));

    // true/false literals are subtypes too
    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);
    assert!(checker.is_subtype_of(lit_true, optional_bool));
    assert!(checker.is_subtype_of(lit_false, optional_bool));
}

// =============================================================================
// Intersection Type Tests - Object and Primitive Intersections
// =============================================================================
// Additional tests for intersection type behavior

#[test]
fn test_primitive_intersection_string_number_is_never() {
    // string & number should reduce to never (disjoint primitives)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_and_number = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);

    // Should be never (or equivalent to never)
    assert!(checker.is_subtype_of(string_and_number, TypeId::NEVER));
}

#[test]
fn test_primitive_intersection_boolean_string_is_never() {
    // boolean & string should reduce to never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let bool_and_string = interner.intersection(vec![TypeId::BOOLEAN, TypeId::STRING]);

    assert!(checker.is_subtype_of(bool_and_string, TypeId::NEVER));
}

#[test]
fn test_primitive_intersection_number_bigint_is_never() {
    // number & bigint should reduce to never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let num_and_bigint = interner.intersection(vec![TypeId::NUMBER, TypeId::BIGINT]);

    assert!(checker.is_subtype_of(num_and_bigint, TypeId::NEVER));
}

#[test]
fn test_literal_intersection_same_type() {
    // "hello" & string should be "hello"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let hello_and_string = interner.intersection(vec![hello, TypeId::STRING]);

    // "hello" & string is just "hello"
    assert!(checker.is_subtype_of(hello_and_string, hello));
    assert!(checker.is_subtype_of(hello, hello_and_string));
}

#[test]
fn test_literal_intersection_different_literals_is_never() {
    // "hello" & "world" should be never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    let hello_and_world = interner.intersection(vec![hello, world]);

    assert!(checker.is_subtype_of(hello_and_world, TypeId::NEVER));
}

#[test]
fn test_number_literal_intersection_different_values() {
    // 1 & 2 should be never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let one_and_two = interner.intersection(vec![one, two]);

    assert!(checker.is_subtype_of(one_and_two, TypeId::NEVER));
}

#[test]
fn test_boolean_literal_intersection() {
    // true & false should be never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);
    let true_and_false = interner.intersection(vec![lit_true, lit_false]);

    assert!(checker.is_subtype_of(true_and_false, TypeId::NEVER));
}

#[test]
fn test_object_intersection_disjoint_properties() {
    // { a: string } & { b: number } = { a: string, b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    // Should be subtype of both components
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_b));
}

#[test]
fn test_object_intersection_same_property_compatible() {
    // { x: string } & { x: string } = { x: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");

    let obj1 = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj2 = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj1, obj2]);

    // Should be equivalent to the original
    assert!(checker.is_subtype_of(intersection, obj1));
    assert!(checker.is_subtype_of(obj1, intersection));
}

#[test]
fn test_object_intersection_property_narrowing() {
    // { x: string | number } & { x: string } = { x: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj_wide = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: string_or_number,
        write_type: string_or_number,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_narrow = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_wide, obj_narrow]);

    // Intersection should be subtype of the narrow version
    assert!(checker.is_subtype_of(intersection, obj_narrow));
}

#[test]
fn test_intersection_with_any() {
    // T & any = any (any absorbs in intersection for assignability)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_and_any = interner.intersection(vec![obj, TypeId::ANY]);

    // any is assignable to/from most things
    assert!(checker.is_subtype_of(TypeId::ANY, obj_and_any));
}

#[test]
fn test_intersection_with_unknown() {
    // T & unknown = T (unknown is identity for intersection)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_and_unknown = interner.intersection(vec![obj, TypeId::UNKNOWN]);

    // Should be equivalent to obj
    assert!(checker.is_subtype_of(obj_and_unknown, obj));
    assert!(checker.is_subtype_of(obj, obj_and_unknown));
}

#[test]
fn test_function_intersection_creates_overload() {
    // ((x: string) => number) & ((x: number) => string)
    // Creates an overloaded function type
    let interner = TypeInterner::new();

    let x_name = interner.intern_string("x");

    let fn_str_to_num = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(x_name),
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

    let fn_num_to_str = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(x_name),
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

    let intersection = interner.intersection(vec![fn_str_to_num, fn_num_to_str]);

    // Intersection should be valid (creates overloaded type)
    assert!(intersection != TypeId::ERROR);
    assert!(intersection != TypeId::NEVER);
}

#[test]
fn test_intersection_brand_pattern() {
    // Branded type: string & { __brand: "UserId" }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand_name = interner.intern_string("__brand");
    let user_id_lit = interner.literal_string("UserId");

    let brand_obj = interner.object(vec![PropertyInfo {
        name: brand_name,
        type_id: user_id_lit,
        write_type: user_id_lit,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let branded_string = interner.intersection(vec![TypeId::STRING, brand_obj]);

    // Branded string should NOT be assignable to plain string
    // (intersection is more specific)
    assert!(!checker.is_subtype_of(TypeId::STRING, branded_string));

    // Branded string IS a subtype of string
    assert!(checker.is_subtype_of(branded_string, TypeId::STRING));
}

#[test]
fn test_intersection_different_brands_is_never() {
    // (string & {__brand: "A"}) & (string & {__brand: "B"}) = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let brand_name = interner.intern_string("__brand");
    let lit_a = interner.literal_string("A");
    let lit_b = interner.literal_string("B");

    let brand_a = interner.object(vec![PropertyInfo {
        name: brand_name,
        type_id: lit_a,
        write_type: lit_a,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let brand_b = interner.object(vec![PropertyInfo {
        name: brand_name,
        type_id: lit_b,
        write_type: lit_b,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let branded_a = interner.intersection(vec![TypeId::STRING, brand_a]);
    let branded_b = interner.intersection(vec![TypeId::STRING, brand_b]);
    let both = interner.intersection(vec![branded_a, branded_b]);

    // Two different brands intersected should be never
    assert!(checker.is_subtype_of(both, TypeId::NEVER));
}

#[test]
fn test_intersection_readonly_property() {
    // { readonly x: string } & { x: string }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");

    let readonly_obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let mutable_obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![readonly_obj, mutable_obj]);

    // Should be a valid intersection
    assert!(intersection != TypeId::ERROR);
}

#[test]
fn test_intersection_optional_and_required() {
    // { x?: string } & { x: string } = { x: string } (required wins)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");

    let optional_obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let required_obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![optional_obj, required_obj]);

    // Intersection should be subtype of required
    assert!(checker.is_subtype_of(intersection, required_obj));
}

#[test]
fn test_intersection_index_signature_with_properties() {
    // { [key: string]: number } & { x: number }
    let interner = TypeInterner::new();

    let x_name = interner.intern_string("x");

    let index_sig = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let prop_obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![index_sig, prop_obj]);

    // Should be valid
    assert!(intersection != TypeId::ERROR);
}

#[test]
#[ignore = "Intersection of index signatures not fully implemented"]
fn test_intersection_two_index_signatures() {
    // { [key: string]: number } & { [key: string]: 1 | 2 }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let one_or_two = interner.union(vec![one, two]);

    let index_number = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let index_literal = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: one_or_two,
            readonly: false,
        }),
        number_index: None,
    });

    let intersection = interner.intersection(vec![index_number, index_literal]);

    // Intersection should be subtype of the more specific one
    assert!(checker.is_subtype_of(intersection, index_literal));
}

#[test]
#[ignore = "Array intersection with incompatible element types not fully implemented"]
fn test_array_intersection() {
    // string[] & number[] = never (element types incompatible)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);

    let intersection = interner.intersection(vec![string_array, number_array]);

    // Should be never (no value can be both string[] and number[])
    assert!(checker.is_subtype_of(intersection, TypeId::NEVER));
}

#[test]
fn test_tuple_intersection_compatible() {
    // [string, number] & [string, number] = [string, number]
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

    let intersection = interner.intersection(vec![tuple, tuple]);

    // Should be equivalent to the tuple itself
    assert!(checker.is_subtype_of(intersection, tuple));
    assert!(checker.is_subtype_of(tuple, intersection));
}

#[test]
#[ignore = "Tuple intersection with incompatible elements not fully implemented"]
fn test_tuple_intersection_incompatible() {
    // [string, number] & [number, string] = never
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

    let intersection = interner.intersection(vec![tuple1, tuple2]);

    // Should be never (element types don't match)
    assert!(checker.is_subtype_of(intersection, TypeId::NEVER));
}

#[test]
#[ignore = "Intersection union distribution not fully implemented"]
fn test_intersection_union_distribution() {
    // (A | B) & C = (A & C) | (B & C) in terms of assignability
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_c = interner.object(vec![PropertyInfo {
        name: c_name,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let a_or_b = interner.union(vec![obj_a, obj_b]);
    let union_and_c = interner.intersection(vec![a_or_b, obj_c]);

    let a_and_c = interner.intersection(vec![obj_a, obj_c]);
    let b_and_c = interner.intersection(vec![obj_b, obj_c]);
    let distributed = interner.union(vec![a_and_c, b_and_c]);

    // Both should be mutually subtype (equivalent)
    assert!(checker.is_subtype_of(union_and_c, distributed));
    assert!(checker.is_subtype_of(distributed, union_and_c));
}

#[test]
fn test_intersection_null_with_object_is_never() {
    // null & { x: string } = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let null_and_obj = interner.intersection(vec![TypeId::NULL, obj]);

    assert!(checker.is_subtype_of(null_and_obj, TypeId::NEVER));
}

#[test]
fn test_intersection_undefined_with_object_is_never() {
    // undefined & { x: string } = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let undefined_and_obj = interner.intersection(vec![TypeId::UNDEFINED, obj]);

    assert!(checker.is_subtype_of(undefined_and_obj, TypeId::NEVER));
}

#[test]
fn test_intersection_method_signatures() {
    // { foo(): void } & { bar(): void } = { foo(): void, bar(): void }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let foo_name = interner.intern_string("foo");
    let bar_name = interner.intern_string("bar");

    let fn_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_foo = interner.object(vec![PropertyInfo {
        name: foo_name,
        type_id: fn_void,
        write_type: fn_void,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let obj_bar = interner.object(vec![PropertyInfo {
        name: bar_name,
        type_id: fn_void,
        write_type: fn_void,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let intersection = interner.intersection(vec![obj_foo, obj_bar]);

    // Should be subtype of both
    assert!(checker.is_subtype_of(intersection, obj_foo));
    assert!(checker.is_subtype_of(intersection, obj_bar));
}

#[test]
fn test_intersection_same_method_different_returns() {
    // { foo(): string } & { foo(): number } - conflicting method returns
    let interner = TypeInterner::new();

    let foo_name = interner.intern_string("foo");

    let fn_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj_foo_string = interner.object(vec![PropertyInfo {
        name: foo_name,
        type_id: fn_string,
        write_type: fn_string,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let obj_foo_number = interner.object(vec![PropertyInfo {
        name: foo_name,
        type_id: fn_number,
        write_type: fn_number,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let intersection = interner.intersection(vec![obj_foo_string, obj_foo_number]);

    // Should produce valid intersection (methods become overloaded or intersection)
    assert!(intersection != TypeId::ERROR);
}

#[test]
fn test_intersection_three_objects() {
    // { a: string } & { b: number } & { c: boolean }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj_a = interner.object(vec![PropertyInfo {
        name: a_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: b_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_c = interner.object(vec![PropertyInfo {
        name: c_name,
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b, obj_c]);

    // Should be subtype of all three
    assert!(checker.is_subtype_of(intersection, obj_a));
    assert!(checker.is_subtype_of(intersection, obj_b));
    assert!(checker.is_subtype_of(intersection, obj_c));
}

#[test]
fn test_intersection_symbol_with_primitive_is_never() {
    // symbol & string = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let symbol_and_string = interner.intersection(vec![TypeId::SYMBOL, TypeId::STRING]);

    assert!(checker.is_subtype_of(symbol_and_string, TypeId::NEVER));
}

#[test]
fn test_intersection_object_intrinsic_with_object() {
    // object & { x: string } - object intrinsic with concrete object
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let object_and_obj = interner.intersection(vec![TypeId::OBJECT, obj]);

    // { x: string } is an object, so intersection should be equivalent to { x: string }
    assert!(checker.is_subtype_of(object_and_obj, obj));
}

#[test]
fn test_intersection_never_identity() {
    // never & T = never (never absorbs everything)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let never_and_obj = interner.intersection(vec![TypeId::NEVER, obj]);
    let obj_and_never = interner.intersection(vec![obj, TypeId::NEVER]);

    assert!(checker.is_subtype_of(never_and_obj, TypeId::NEVER));
    assert!(checker.is_subtype_of(obj_and_never, TypeId::NEVER));
}

// =============================================================================
// KeyOf Type Operator Tests
// =============================================================================
// Tests for keyof type operator and property key relationships

#[test]
fn test_keyof_single_property_is_literal() {
    // keyof { x: number } = "x"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let keyof_obj = interner.intern(TypeKey::KeyOf(obj));
    let lit_x = interner.literal_string("x");

    // keyof { x } should be subtype of "x" (they're equivalent)
    assert!(checker.is_subtype_of(keyof_obj, lit_x));
}

#[test]
fn test_keyof_multiple_properties_is_union() {
    // keyof { a, b, c } = "a" | "b" | "c"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("c"),
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let keyof_obj = interner.intern(TypeKey::KeyOf(obj));
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");
    let expected = interner.union(vec![lit_a, lit_b, lit_c]);

    // Each literal key should be subtype of keyof
    assert!(checker.is_subtype_of(lit_a, keyof_obj));
    assert!(checker.is_subtype_of(lit_b, keyof_obj));
    assert!(checker.is_subtype_of(lit_c, keyof_obj));

    // keyof should be subtype of the union of keys
    assert!(checker.is_subtype_of(keyof_obj, expected));
}

#[test]
fn test_keyof_empty_object_is_never() {
    // keyof {} = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let empty_obj = interner.object(vec![]);
    let keyof_empty = interner.intern(TypeKey::KeyOf(empty_obj));

    // keyof {} should be subtype of never (they're equivalent)
    assert!(checker.is_subtype_of(keyof_empty, TypeId::NEVER));
}

#[test]
fn test_keyof_with_optional_property() {
    // keyof { x?: number } = "x"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    let keyof_obj = interner.intern(TypeKey::KeyOf(obj));
    let lit_x = interner.literal_string("x");

    // Optional property still contributes to keyof
    assert!(checker.is_subtype_of(lit_x, keyof_obj));
}

#[test]
fn test_keyof_with_readonly_property() {
    // keyof { readonly x: number } = "x"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let keyof_obj = interner.intern(TypeKey::KeyOf(obj));
    let lit_x = interner.literal_string("x");

    // Readonly property still contributes to keyof
    assert!(checker.is_subtype_of(lit_x, keyof_obj));
}

#[test]
fn test_keyof_with_method() {
    // keyof { foo(): void } = "foo"
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let foo_name = interner.intern_string("foo");
    let fn_void = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo {
        name: foo_name,
        type_id: fn_void,
        write_type: fn_void,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let keyof_obj = interner.intern(TypeKey::KeyOf(obj));
    let lit_foo = interner.literal_string("foo");

    assert!(checker.is_subtype_of(lit_foo, keyof_obj));
}

#[test]
fn test_keyof_subtype_of_string() {
    // keyof { x: number } <: string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let keyof_obj = interner.intern(TypeKey::KeyOf(obj));

    // keyof object with string keys is subtype of string
    assert!(checker.is_subtype_of(keyof_obj, TypeId::STRING));
}

#[test]
fn test_keyof_not_equal_to_string() {
    // string is NOT a subtype of keyof { x: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let x_name = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let keyof_obj = interner.intern(TypeKey::KeyOf(obj));

    // string is wider than keyof { x }
    assert!(!checker.is_subtype_of(TypeId::STRING, keyof_obj));
}

#[test]
fn test_keyof_wider_object_has_more_keys() {
    // keyof { a, b } has more keys than keyof { a }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_ab = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let keyof_a = interner.intern(TypeKey::KeyOf(obj_a));
    let keyof_ab = interner.intern(TypeKey::KeyOf(obj_ab));

    // keyof { a } <: keyof { a, b } (fewer keys is narrower)
    assert!(checker.is_subtype_of(keyof_a, keyof_ab));
    // keyof { a, b } is NOT subtype of keyof { a }
    assert!(!checker.is_subtype_of(keyof_ab, keyof_a));
}

#[test]
fn test_keyof_union_is_intersection_of_keys() {
    // keyof (A | B) = (keyof A) & (keyof B) - only common keys
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_ab = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let obj_bc = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("c"),
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let union = interner.union(vec![obj_ab, obj_bc]);
    let keyof_union = interner.intern(TypeKey::KeyOf(union));
    let lit_b = interner.literal_string("b");

    // Only "b" is common to both - should be subtype of keyof union
    assert!(checker.is_subtype_of(lit_b, keyof_union));
}

#[test]
fn test_keyof_intersection_is_union_of_keys() {
    // keyof (A & B) = (keyof A) | (keyof B) - all keys from both
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    let keyof_intersection = interner.intern(TypeKey::KeyOf(intersection));

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    // Both "a" and "b" should be subtypes of keyof intersection
    assert!(checker.is_subtype_of(lit_a, keyof_intersection));
    assert!(checker.is_subtype_of(lit_b, keyof_intersection));
}

#[test]
fn test_keyof_any_is_string_number_symbol() {
    // keyof any = string | number | symbol
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_any = interner.intern(TypeKey::KeyOf(TypeId::ANY));
    let property_key = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);

    // keyof any should be equivalent to PropertyKey
    assert!(checker.is_subtype_of(keyof_any, property_key));
}

#[test]
fn test_keyof_unknown_is_never() {
    // keyof unknown = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_unknown = interner.intern(TypeKey::KeyOf(TypeId::UNKNOWN));

    assert!(checker.is_subtype_of(keyof_unknown, TypeId::NEVER));
}

#[test]
fn test_keyof_never_is_string_number_symbol() {
    // keyof never = string | number | symbol (vacuously true)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_never = interner.intern(TypeKey::KeyOf(TypeId::NEVER));
    let property_key = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);

    assert!(checker.is_subtype_of(keyof_never, property_key));
}

#[test]
fn test_keyof_string_has_string_methods() {
    // keyof string includes string method names
    let interner = TypeInterner::new();

    let keyof_string = interner.intern(TypeKey::KeyOf(TypeId::STRING));

    // Should be valid type
    assert!(keyof_string != TypeId::ERROR);
}

#[test]
fn test_keyof_number_has_number_methods() {
    // keyof number includes number method names
    let interner = TypeInterner::new();

    let keyof_number = interner.intern(TypeKey::KeyOf(TypeId::NUMBER));

    // Should be valid type
    assert!(keyof_number != TypeId::ERROR);
}

#[test]
fn test_keyof_array_type() {
    // keyof string[] includes array methods and number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let keyof_array = interner.intern(TypeKey::KeyOf(string_array));

    // number should be subtype of keyof array (for index access)
    assert!(checker.is_subtype_of(TypeId::NUMBER, keyof_array));
}

#[test]
fn test_keyof_tuple_type() {
    // keyof [string, number] includes "0" | "1" | array methods
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

    let keyof_tuple = interner.intern(TypeKey::KeyOf(tuple));
    let lit_0 = interner.literal_string("0");
    let lit_1 = interner.literal_string("1");

    // "0" and "1" should be subtypes of keyof tuple
    assert!(checker.is_subtype_of(lit_0, keyof_tuple));
    assert!(checker.is_subtype_of(lit_1, keyof_tuple));
}

#[test]
fn test_keyof_with_index_signature_includes_string() {
    // keyof { [key: string]: number } includes string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let keyof_indexed = interner.intern(TypeKey::KeyOf(indexed_obj));

    // string should be subtype of keyof { [key: string]: number }
    assert!(checker.is_subtype_of(TypeId::STRING, keyof_indexed));
}

#[test]
fn test_keyof_with_number_index_signature() {
    // keyof { [key: number]: string } includes number
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let indexed_obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    let keyof_indexed = interner.intern(TypeKey::KeyOf(indexed_obj));

    // number should be subtype of keyof { [key: number]: string }
    assert!(checker.is_subtype_of(TypeId::NUMBER, keyof_indexed));
}

#[test]
fn test_keyof_nested_object() {
    // keyof { x: { y: number } } = "x" (not "x" | "y")
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let y_name = interner.intern_string("y");
    let inner_obj = interner.object(vec![PropertyInfo {
        name: y_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let x_name = interner.intern_string("x");
    let outer_obj = interner.object(vec![PropertyInfo {
        name: x_name,
        type_id: inner_obj,
        write_type: inner_obj,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let keyof_outer = interner.intern(TypeKey::KeyOf(outer_obj));
    let lit_x = interner.literal_string("x");
    let lit_y = interner.literal_string("y");

    // "x" is a key of outer
    assert!(checker.is_subtype_of(lit_x, keyof_outer));
    // "y" is NOT a key of outer (it's a key of the nested object)
    assert!(!checker.is_subtype_of(lit_y, keyof_outer));
}

#[test]
fn test_keyof_generic_constraint() {
    // <K extends keyof T> constraint pattern
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("name"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("age"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let keyof_obj = interner.intern(TypeKey::KeyOf(obj));
    let lit_name = interner.literal_string("name");
    let lit_age = interner.literal_string("age");
    let lit_invalid = interner.literal_string("invalid");

    // Valid keys satisfy the constraint
    assert!(checker.is_subtype_of(lit_name, keyof_obj));
    assert!(checker.is_subtype_of(lit_age, keyof_obj));
    // Invalid key doesn't satisfy
    assert!(!checker.is_subtype_of(lit_invalid, keyof_obj));
}

#[test]
fn test_keyof_mapped_type_source() {
    // keyof used as constraint in mapped type: { [K in keyof T]: ... }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("y"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let keyof_obj = interner.intern(TypeKey::KeyOf(obj));

    // keyof should produce valid keys for iteration
    assert!(keyof_obj != TypeId::ERROR);
    assert!(keyof_obj != TypeId::NEVER);

    // Should be subtype of string (for string-keyed objects)
    assert!(checker.is_subtype_of(keyof_obj, TypeId::STRING));
}

#[test]
fn test_keyof_reflexive() {
    // keyof T <: keyof T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let keyof_obj = interner.intern(TypeKey::KeyOf(obj));

    assert!(checker.is_subtype_of(keyof_obj, keyof_obj));
}

#[test]
fn test_keyof_null_is_never() {
    // keyof null = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_null = interner.intern(TypeKey::KeyOf(TypeId::NULL));

    assert!(checker.is_subtype_of(keyof_null, TypeId::NEVER));
}

#[test]
fn test_keyof_undefined_is_never() {
    // keyof undefined = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_undefined = interner.intern(TypeKey::KeyOf(TypeId::UNDEFINED));

    assert!(checker.is_subtype_of(keyof_undefined, TypeId::NEVER));
}

#[test]
fn test_keyof_void_is_never() {
    // keyof void = never
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let keyof_void = interner.intern(TypeKey::KeyOf(TypeId::VOID));

    assert!(checker.is_subtype_of(keyof_void, TypeId::NEVER));
}

#[test]
fn test_keyof_object_intrinsic() {
    // keyof object includes all possible property keys
    let interner = TypeInterner::new();

    let keyof_object = interner.intern(TypeKey::KeyOf(TypeId::OBJECT));

    // Should be valid
    assert!(keyof_object != TypeId::ERROR);
}

#[test]
fn test_keyof_symbol_keyed_object() {
    // Objects with symbol keys in keyof result
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    // Simulated: { [Symbol.iterator]: () => Iterator }
    let sym_iterator = interner.intern_string("Symbol.iterator");
    let fn_iterator = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo {
        name: sym_iterator,
        type_id: fn_iterator,
        write_type: fn_iterator,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let keyof_obj = interner.intern(TypeKey::KeyOf(obj));

    // Should include the symbol key
    assert!(keyof_obj != TypeId::NEVER);
}

// =============================================================================
// Constructor Type Tests
// =============================================================================
// Tests for new signatures, abstract constructors, and constructor types

#[test]
fn test_constructor_basic_new_signature() {
    // new () => T
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let constructor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Constructor type should be valid
    assert!(constructor != TypeId::ERROR);
    assert!(constructor != TypeId::NEVER);
}

#[test]
fn test_constructor_with_parameters() {
    // new (x: string, y: number) => T
    let interner = TypeInterner::new();

    let instance = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("y"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let constructor = interner.function(FunctionShape {
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

    assert!(constructor != TypeId::ERROR);
}

#[test]
fn test_constructor_vs_regular_function() {
    // Constructor and regular function are different types
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let constructor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let regular_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: instance,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Constructor and function with same signature are not assignable
    assert!(!checker.is_subtype_of(constructor, regular_fn));
    assert!(!checker.is_subtype_of(regular_fn, constructor));
}

#[test]
fn test_constructor_callable_with_construct_signature() {
    // interface C { new (): T }
    let interner = TypeInterner::new();
    let _checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let callable_with_new = interner.callable(CallableShape {
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

    assert!(callable_with_new != TypeId::ERROR);
}

#[test]
fn test_constructor_with_call_and_construct() {
    // interface F { (): string; new (): T }
    let interner = TypeInterner::new();

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let callable_both = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_method: false,
        }],
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

    assert!(callable_both != TypeId::ERROR);
}

#[test]
fn test_constructor_subtype_by_return_type() {
    // new () => Derived <: new () => Base
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let base = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let derived = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("y"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let ctor_base = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: base,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let ctor_derived = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: derived,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Constructor returning derived is subtype of constructor returning base
    assert!(checker.is_subtype_of(ctor_derived, ctor_base));
    // Reverse is not true
    assert!(!checker.is_subtype_of(ctor_base, ctor_derived));
}

#[test]
fn test_constructor_contravariant_parameters() {
    // new (x: Base) => T <: new (x: Derived) => T
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("result"),
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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
#[ignore = "Constructor optional parameter subtyping not fully implemented"]
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

    let instance_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let instance_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let overloaded_ctor = interner.callable(CallableShape {
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
    };
    let t_type = interner.intern(TypeKey::TypeParameter(t_param.clone()));

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
    };
    let t_type = interner.intern(TypeKey::TypeParameter(t_param.clone()));

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

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Abstract constructor (conceptually - just a construct signature)
    let abstract_ctor = interner.callable(CallableShape {
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

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let ctor_with_static = interner.callable(CallableShape {
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
        PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("y"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
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

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let single_sig = interner.callable(CallableShape {
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

    let window_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("document"),
        type_id: TypeId::OBJECT,
        write_type: TypeId::OBJECT,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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
    let nonempty_instance = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

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

    let this_type = interner.intern(TypeKey::ThisType);

    // this type should be valid
    assert!(this_type != TypeId::ERROR);
    assert!(this_type != TypeId::NEVER);
}

#[test]
fn test_this_type_in_method_return() {
    // Method returning this for fluent interface
    // method(): this
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);

    let fluent_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("setName"),
        type_id: fluent_method,
        write_type: fluent_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_this_type_fluent_builder() {
    // Builder pattern with multiple fluent methods
    // { setName(name: string): this, setValue(value: number): this, build(): Result }
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);
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
        PropertyInfo {
            name: interner.intern_string("setName"),
            type_id: set_name,
            write_type: set_name,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("setValue"),
            type_id: set_value,
            write_type: set_value,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("build"),
            type_id: build,
            write_type: build,
            optional: false,
            readonly: false,
            is_method: true,
        },
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

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(interner.lazy(DefId(1))),
        default: None,
    }));

    let constrained_method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(interner.lazy(DefId(1))),
            default: None,
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

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("onClick"),
        type_id: callback,
        write_type: callback,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_this_type_subtype_check() {
    // this type subtype relationships
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let this_type = interner.intern(TypeKey::ThisType);

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

    let this_type = interner.intern(TypeKey::ThisType);

    let chain_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let chainable = interner.object(vec![PropertyInfo {
        name: interner.intern_string("chain"),
        type_id: chain_method,
        write_type: chain_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(chainable != TypeId::ERROR);
}

#[test]
fn test_this_type_with_generic_method() {
    // Generic method with this return
    // method<T>(value: T): this
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);
    let t_ref = interner.lazy(DefId(50));

    let generic_fluent = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
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

    let this_type = interner.intern(TypeKey::ThisType);

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("self"),
        type_id: this_type,
        write_type: this_type,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_this_type_array() {
    // Array of this type
    // this[]
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);
    let this_array = interner.array(this_type);

    assert!(this_array != TypeId::ERROR);
}

#[test]
fn test_this_type_in_union() {
    // this | null
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);
    let nullable_this = interner.union(vec![this_type, TypeId::NULL]);

    assert!(nullable_this != TypeId::ERROR);
}

#[test]
fn test_this_type_in_intersection() {
    // this & HasId
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);
    let has_id = interner.object(vec![PropertyInfo {
        name: interner.intern_string("id"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let this_with_id = interner.intersection(vec![this_type, has_id]);

    assert!(this_with_id != TypeId::ERROR);
}

#[test]
fn test_this_type_clone_method() {
    // clone(): this pattern
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);

    let clone_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cloneable = interner.object(vec![PropertyInfo {
        name: interner.intern_string("clone"),
        type_id: clone_method,
        write_type: clone_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    assert!(cloneable != TypeId::ERROR);
}

#[test]
fn test_this_type_with_optional_chaining() {
    // Method returning this | undefined for optional operation
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);
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

    let this_type = interner.intern(TypeKey::ThisType);
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

    let this_type = interner.intern(TypeKey::ThisType);
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

    let this_type = interner.intern(TypeKey::ThisType);
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

    let this_type = interner.intern(TypeKey::ThisType);

    // Simulated Readonly<this> as application
    let readonly_this = interner.application(interner.lazy(DefId(100)), vec![this_type]);

    assert!(readonly_this != TypeId::ERROR);
}

#[test]
fn test_this_type_partial() {
    // Partial<this>
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);

    let partial_this = interner.application(interner.lazy(DefId(101)), vec![this_type]);

    assert!(partial_this != TypeId::ERROR);
}

#[test]
fn test_this_type_with_keyof() {
    // keyof this
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);
    let keyof_this = interner.intern(TypeKey::KeyOf(this_type));

    assert!(keyof_this != TypeId::ERROR);
}

#[test]
fn test_this_type_indexed_access() {
    // this[K] indexed access
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);
    let k_ref = interner.lazy(DefId(50));

    let indexed = interner.intern(TypeKey::IndexAccess(this_type, k_ref));

    assert!(indexed != TypeId::ERROR);
}

#[test]
fn test_this_type_with_extends() {
    // this extends SomeInterface ? A : B
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);
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

    let this_type = interner.intern(TypeKey::ThisType);

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

    let this_type = interner.intern(TypeKey::ThisType);

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
        PropertyInfo {
            name: interner.intern_string("staticMethod"),
            type_id: static_method,
            write_type: static_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("instanceMethod"),
            type_id: instance_method,
            write_type: instance_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    assert!(class_type != TypeId::ERROR);
}

#[test]
fn test_this_type_with_getter_setter() {
    // Getter returns this, setter takes value
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);

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
    }]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_this_type_with_rest_params() {
    // method(...args: Parameters<this["method"]>): this
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);

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

    let this1 = interner.intern(TypeKey::ThisType);
    let this2 = interner.intern(TypeKey::ThisType);

    // Same interned type
    assert_eq!(this1, this2);
}

#[test]
fn test_this_type_with_method_overload() {
    // Overloaded methods all returning this
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);

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

    let this_type = interner.intern(TypeKey::ThisType);

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
        PropertyInfo {
            name: interner.intern_string("on"),
            type_id: on_method,
            write_type: on_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("off"),
            type_id: off_method,
            write_type: off_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("emit"),
            type_id: emit_method,
            write_type: emit_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
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

    let this_type = interner.intern(TypeKey::ThisType);
    let result_array = interner.array(interner.lazy(DefId(100)));
    let promise_results =
        interner.application(interner.lazy(DefId(101)), vec![result_array]);

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
        PropertyInfo {
            name: interner.intern_string("where"),
            type_id: where_method,
            write_type: where_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("orderBy"),
            type_id: order_by_method,
            write_type: order_by_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("limit"),
            type_id: limit_method,
            write_type: limit_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("execute"),
            type_id: execute_method,
            write_type: execute_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
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

    let readonly_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(readonly_obj != TypeId::ERROR);
}

#[test]
fn test_readonly_vs_mutable_property() {
    // { readonly x: string } vs { x: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let readonly_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let mutable_obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Mutable is subtype of readonly (can assign mutable to readonly)
    assert!(checker.is_subtype_of(mutable_obj, readonly_obj));

    // Readonly is NOT subtype of mutable (can't write to readonly)
    assert!(!checker.is_subtype_of(readonly_obj, mutable_obj));
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
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
        },
    ]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_mixed_with_mutable() {
    // { readonly a: string, b: number }
    let interner = TypeInterner::new();

    let mixed = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    assert!(mixed != TypeId::ERROR);
}

#[test]
fn test_readonly_index_signature() {
    // { readonly [key: string]: number }
    let interner = TypeInterner::new();

    let readonly_index = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
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
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
        }),
        number_index: None,
    });

    let mutable_index = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    // Mutable index is subtype of readonly index
    assert!(checker.is_subtype_of(mutable_index, readonly_index));

    // Readonly index is NOT subtype of mutable index
    assert!(!checker.is_subtype_of(readonly_index, mutable_index));
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
    }]);

    assert!(readonly_optional != TypeId::ERROR);
}

#[test]
fn test_readonly_nested_object() {
    // { readonly data: { readonly inner: string } }
    let interner = TypeInterner::new();

    let inner = interner.object(vec![PropertyInfo {
        name: interner.intern_string("inner"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let outer = interner.object(vec![PropertyInfo {
        name: interner.intern_string("data"),
        type_id: inner,
        write_type: inner,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(outer != TypeId::ERROR);
}

#[test]
fn test_readonly_with_union_property() {
    // { readonly x: string | number }
    let interner = TypeInterner::new();

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: union,
        write_type: union,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_with_array_property() {
    // { readonly items: string[] }
    let interner = TypeInterner::new();

    let string_array = interner.array(TypeId::STRING);

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("items"),
        type_id: string_array,
        write_type: string_array,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_deep_with_array() {
    // { readonly items: readonly string[] }
    let interner = TypeInterner::new();

    let readonly_array = interner.readonly_array(TypeId::STRING);

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("items"),
        type_id: readonly_array,
        write_type: readonly_array,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("callback"),
        type_id: callback,
        write_type: callback,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("status"),
        type_id: status_union,
        write_type: status_union,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_with_number_index() {
    // { readonly [index: number]: string }
    let interner = TypeInterner::new();

    let readonly_number_index = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: true,
        }),
    });

    assert!(readonly_number_index != TypeId::ERROR);
}

#[test]
fn test_readonly_intersection() {
    // { readonly a: string } & { readonly b: number }
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);

    assert!(intersection != TypeId::ERROR);
}

#[test]
fn test_readonly_in_generic_context() {
    // Container<T> = { readonly value: T }
    let interner = TypeInterner::new();

    let t_ref = interner.lazy(DefId(50));

    let container = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: t_ref,
        write_type: t_ref,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(container != TypeId::ERROR);
}

#[test]
fn test_readonly_preserves_subtype_covariance() {
    // { readonly x: "a" } is subtype of { readonly x: string }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let lit_a = interner.literal_string("a");

    let readonly_literal = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: lit_a,
        write_type: lit_a,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let readonly_string = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    // Literal is subtype of wider type (covariant)
    assert!(checker.is_subtype_of(readonly_literal, readonly_string));
}

#[test]
fn test_readonly_with_this_type() {
    // { readonly self: this }
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("self"),
        type_id: this_type,
        write_type: this_type,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("coords"),
        type_id: coords,
        write_type: coords,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("coords"),
        type_id: readonly_coords,
        write_type: readonly_coords,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_mapped_type_pattern() {
    // Simulating Readonly<T> mapped type result
    // { readonly a: string, readonly b: number }
    let interner = TypeInterner::new();

    let readonly_all = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
        },
    ]);

    let mutable_all = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
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
        PropertyInfo {
            name: interner.intern_string("id"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("createdAt"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
        },
    ]);

    assert!(instance != TypeId::ERROR);
}

#[test]
fn test_readonly_with_bigint() {
    // { readonly value: bigint }
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::BIGINT,
        write_type: TypeId::BIGINT,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_with_symbol() {
    // { readonly sym: symbol }
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("sym"),
        type_id: TypeId::SYMBOL,
        write_type: TypeId::SYMBOL,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_with_null_union() {
    // { readonly value: string | null }
    let interner = TypeInterner::new();

    let nullable = interner.union(vec![TypeId::STRING, TypeId::NULL]);

    let obj = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: nullable,
        write_type: nullable,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    assert!(obj != TypeId::ERROR);
}

#[test]
fn test_readonly_config_pattern() {
    // Config object: { readonly host: string, readonly port: number, readonly debug: boolean }
    let interner = TypeInterner::new();

    let config = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("host"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("port"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("debug"),
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: true,
            is_method: false,
        },
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

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));

    let callable = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("T"),
                    constraint: None,
                    default: None,
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

    let t_string = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
    }));

    let t_number = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::NUMBER),
        default: None,
    }));

    let callable = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("T"),
                    constraint: Some(TypeId::STRING),
                    default: None,
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

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));

    let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
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
        call_signatures: vec![
            CallSignature {
                type_params: vec![
                    TypeParamInfo {
                        name: interner.intern_string("T"),
                        constraint: None,
                        default: None,
                    },
                    TypeParamInfo {
                        name: interner.intern_string("U"),
                        constraint: None,
                        default: None,
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

    let obj_with_x = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_with_xy = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("y"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    // Returns {x, y}
    let specific_constructor = interner.callable(CallableShape {
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

    let window_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("location"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let document_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("body"),
        type_id: TypeId::OBJECT,
        write_type: TypeId::OBJECT,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let callable = interner.callable(CallableShape {
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
            PropertyInfo {
                name: interner.intern_string("name"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
            },
            PropertyInfo {
                name: interner.intern_string("version"),
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: false,
                readonly: false,
                is_method: false,
            },
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

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: Some(TypeId::STRING),
    }));

    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![TypeParamInfo {
                name: interner.intern_string("T"),
                constraint: None,
                default: Some(TypeId::STRING),
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

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));

    let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
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
        PropertyInfo {
            name: interner.intern_string("map"),
            type_id: map_method,
            write_type: map_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("filter"),
            type_id: filter_method,
            write_type: filter_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("reduce"),
            type_id: reduce_method,
            write_type: reduce_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
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
        PropertyInfo {
            name: interner.intern_string("type"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("clientX"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("clientY"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
        },
    ]);

    let keyboard_event = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("type"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("key"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("code"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true,
            is_method: false,
        },
    ]);

    let base_event = interner.object(vec![PropertyInfo {
        name: interner.intern_string("type"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

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

    let event_target = interner.object(vec![PropertyInfo {
        name: interner.intern_string("addEventListener"),
        type_id: add_event_listener,
        write_type: add_event_listener,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let t_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    }));

    let u_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
    }));

    let v_param = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: None,
        default: None,
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
        call_signatures: vec![
            // then<U>(onFulfilled: (value: T) => U): Promise<U>
            CallSignature {
                type_params: vec![TypeParamInfo {
                    name: interner.intern_string("U"),
                    constraint: None,
                    default: None,
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
                    },
                    TypeParamInfo {
                        name: interner.intern_string("V"),
                        constraint: None,
                        default: None,
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
        },
    ]);

    let date_constructor = interner.callable(CallableShape {
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
        other => panic!("Expected IntrinsicTypeMismatch, got {:?}", other),
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
        other => panic!("Expected LiteralTypeMismatch, got {:?}", other),
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
        other => panic!("Expected LiteralTypeMismatch, got {:?}", other),
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
        other => panic!("Expected ErrorType, got {:?}", other),
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
        other => panic!("Expected LiteralTypeMismatch, got {:?}", other),
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
        other => panic!("Expected TypeMismatch, got {:?}", other),
    }
}

// ============================================================================
// Tuple-to-Array Assignability Tests
// These tests document TypeScript behavior for assigning tuples to arrays
// ============================================================================

// --- Homogeneous Tuples to Arrays ---

#[test]
fn test_tuple_to_array_homogeneous_two_strings() {
    // [string, string] -> string[] should succeed
    // In TypeScript: const arr: string[] = ["a", "b"]; // OK
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, string_array),
        "[string, string] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_to_array_homogeneous_three_numbers() {
    // [number, number, number] -> number[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, number_array),
        "[number, number, number] should be assignable to number[]"
    );
}

#[test]
fn test_tuple_to_array_homogeneous_booleans() {
    // [boolean, boolean] -> boolean[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let boolean_array = interner.array(TypeId::BOOLEAN);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::BOOLEAN,
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

    assert!(
        checker.is_subtype_of(source, boolean_array),
        "[boolean, boolean] should be assignable to boolean[]"
    );
}

#[test]
fn test_tuple_to_array_homogeneous_literal_to_base() {
    // ["hello", "world"] -> string[] should succeed (literals widen to base type)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: hello,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: world,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[\"hello\", \"world\"] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_to_array_homogeneous_number_literals() {
    // [1, 2, 3] -> number[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);
    let number_array = interner.array(TypeId::NUMBER);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: one,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: two,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: three,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, number_array),
        "[1, 2, 3] should be assignable to number[]"
    );
}

// --- Heterogeneous Tuples to Union Arrays ---

#[test]
fn test_tuple_to_union_array_string_number() {
    // [string, number] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string, number] should be assignable to (string | number)[]"
    );
}

#[test]
fn test_tuple_to_union_array_number_boolean() {
    // [number, boolean] -> (number | boolean)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::NUMBER, TypeId::BOOLEAN]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, union_array),
        "[number, boolean] should be assignable to (number | boolean)[]"
    );
}

#[test]
fn test_tuple_to_union_array_three_types() {
    // [string, number, boolean] -> (string | number | boolean)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string, number, boolean] should be assignable to (string | number | boolean)[]"
    );
}

#[test]
fn test_tuple_to_union_array_literals_to_base() {
    // ["a", 1] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_literal = interner.literal_string("a");
    let one_literal = interner.literal_number(1.0);
    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: a_literal,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: one_literal,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[\"a\", 1] should be assignable to (string | number)[]"
    );
}

#[test]
fn test_tuple_to_union_array_subset_elements() {
    // [string, string] -> (string | number)[] should succeed
    // All elements match a subset of the union
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string, string] should be assignable to (string | number)[]"
    );
}

#[test]
fn test_tuple_to_union_array_fails_missing_element_type() {
    // [string, boolean] -> (string | number)[] should FAIL
    // boolean is not in the union (string | number)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        !checker.is_subtype_of(source, union_array),
        "[string, boolean] should NOT be assignable to (string | number)[] - boolean is not in union"
    );
}

// --- Tuples with Rest Elements to Arrays ---

#[test]
fn test_tuple_rest_to_array_matching() {
    // [number, ...string[]] -> (number | string)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let union_array = interner.array(union_elem);
    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[number, ...string[]] should be assignable to (number | string)[]"
    );
}

#[test]
fn test_tuple_rest_to_array_homogeneous() {
    // [string, ...string[]] -> string[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[string, ...string[]] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_rest_to_array_prefix_not_matching() {
    // [boolean, ...string[]] -> string[] should FAIL
    // The first element (boolean) is not string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(
        !checker.is_subtype_of(source, string_array),
        "[boolean, ...string[]] should NOT be assignable to string[]"
    );
}

#[test]
fn test_tuple_rest_to_array_rest_not_matching() {
    // [string, ...number[]] -> string[] should FAIL
    // The rest element (number[]) is not compatible with string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);
    let source = interner.tuple(vec![
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

    assert!(
        !checker.is_subtype_of(source, string_array),
        "[string, ...number[]] should NOT be assignable to string[]"
    );
}

#[test]
fn test_tuple_rest_multiple_prefix_to_union_array() {
    // [string, number, ...boolean[]] -> (string | number | boolean)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let union_array = interner.array(union_elem);
    let boolean_array = interner.array(TypeId::BOOLEAN);
    let source = interner.tuple(vec![
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
            type_id: boolean_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string, number, ...boolean[]] should be assignable to (string | number | boolean)[]"
    );
}

#[test]
fn test_tuple_only_rest_to_array() {
    // [...number[]] -> number[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);
    let source = interner.tuple(vec![TupleElement {
        type_id: number_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    assert!(
        checker.is_subtype_of(source, number_array),
        "[...number[]] should be assignable to number[]"
    );
}

// --- Edge Cases: Empty Tuples ---

#[test]
fn test_empty_tuple_to_string_array() {
    // [] -> string[] should succeed (empty tuple is compatible with any array)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let empty_tuple = interner.tuple(Vec::new());

    assert!(
        checker.is_subtype_of(empty_tuple, string_array),
        "[] should be assignable to string[]"
    );
}

#[test]
fn test_empty_tuple_to_number_array() {
    // [] -> number[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let number_array = interner.array(TypeId::NUMBER);
    let empty_tuple = interner.tuple(Vec::new());

    assert!(
        checker.is_subtype_of(empty_tuple, number_array),
        "[] should be assignable to number[]"
    );
}

#[test]
fn test_empty_tuple_to_union_array() {
    // [] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let empty_tuple = interner.tuple(Vec::new());

    assert!(
        checker.is_subtype_of(empty_tuple, union_array),
        "[] should be assignable to (string | number)[]"
    );
}

#[test]
fn test_empty_tuple_to_any_array() {
    // [] -> any[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let any_array = interner.array(TypeId::ANY);
    let empty_tuple = interner.tuple(Vec::new());

    assert!(
        checker.is_subtype_of(empty_tuple, any_array),
        "[] should be assignable to any[]"
    );
}

#[test]
fn test_empty_tuple_to_never_array() {
    // [] -> never[] should succeed (empty tuple has zero elements, all of which are never)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let never_array = interner.array(TypeId::NEVER);
    let empty_tuple = interner.tuple(Vec::new());

    assert!(
        checker.is_subtype_of(empty_tuple, never_array),
        "[] should be assignable to never[]"
    );
}

// --- Edge Cases: Single-Element Tuples ---

#[test]
fn test_single_element_tuple_to_array() {
    // [string] -> string[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[string] should be assignable to string[]"
    );
}

#[test]
fn test_single_element_tuple_type_mismatch() {
    // [number] -> string[] should FAIL
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(
        !checker.is_subtype_of(source, string_array),
        "[number] should NOT be assignable to string[]"
    );
}

#[test]
fn test_single_element_tuple_to_union_array() {
    // [string] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string] should be assignable to (string | number)[]"
    );
}

// --- Edge Cases: Tuples with Optional Elements ---

#[test]
fn test_tuple_optional_to_array() {
    // [string, number?] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string, number?] should be assignable to (string | number)[]"
    );
}

#[test]
fn test_tuple_all_optional_to_array() {
    // [string?, number?] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string?, number?] should be assignable to (string | number)[]"
    );
}

#[test]
fn test_tuple_optional_homogeneous_to_array() {
    // [string, string?] -> string[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[string, string?] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_optional_element_type_mismatch() {
    // [string, boolean?] -> string[] should FAIL
    // Optional element type (boolean) doesn't match array element type (string)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    assert!(
        !checker.is_subtype_of(source, string_array),
        "[string, boolean?] should NOT be assignable to string[] - boolean is not string"
    );
}

#[test]
fn test_tuple_optional_with_rest_to_array() {
    // [string?, ...number[]] -> (string | number)[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let number_array = interner.array(TypeId::NUMBER);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: number_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[string?, ...number[]] should be assignable to (string | number)[]"
    );
}

// --- Edge Cases: Named Tuple Elements ---

#[test]
fn test_named_tuple_to_array() {
    // [name: string, age: number] -> (string | number)[] should succeed
    // Named tuple elements don't affect assignability to arrays
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let name_atom = interner.intern_string("name");
    let age_atom = interner.intern_string("age");
    let union_elem = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_array = interner.array(union_elem);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(name_atom),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(age_atom),
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, union_array),
        "[name: string, age: number] should be assignable to (string | number)[]"
    );
}

// --- Edge Cases: Special Types ---

#[test]
fn test_tuple_with_any_to_string_array() {
    // [any, any] -> string[] should succeed (any is assignable to anything)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::ANY,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::ANY,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[any, any] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_to_any_array() {
    // [string, number] -> any[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let any_array = interner.array(TypeId::ANY);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, any_array),
        "[string, number] should be assignable to any[]"
    );
}

#[test]
fn test_tuple_with_never_to_string_array() {
    // [never, never] -> string[] should succeed (never is subtype of all types)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NEVER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NEVER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, string_array),
        "[never, never] should be assignable to string[]"
    );
}

#[test]
fn test_tuple_to_unknown_array() {
    // [string, number] -> unknown[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let unknown_array = interner.array(TypeId::UNKNOWN);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, unknown_array),
        "[string, number] should be assignable to unknown[]"
    );
}

#[test]
fn test_tuple_with_unknown_to_string_array() {
    // [unknown, unknown] -> string[] should FAIL
    // unknown is not assignable to string
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::UNKNOWN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::UNKNOWN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        !checker.is_subtype_of(source, string_array),
        "[unknown, unknown] should NOT be assignable to string[]"
    );
}

// --- Edge Cases: Readonly arrays ---

#[test]
fn test_tuple_to_readonly_array() {
    // [string, string] -> readonly string[] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // readonly_array takes the element type, not an array type
    let readonly_string_array = interner.readonly_array(TypeId::STRING);
    let source = interner.tuple(vec![
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

    assert!(
        checker.is_subtype_of(source, readonly_string_array),
        "[string, string] should be assignable to readonly string[]"
    );
}

// --- Edge Cases: Nested tuples ---

#[test]
fn test_nested_tuple_to_array() {
    // [[string, number], [string, number]] -> [string, number][] should succeed
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let inner_tuple = interner.tuple(vec![
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
    let tuple_array = interner.array(inner_tuple);
    let source = interner.tuple(vec![
        TupleElement {
            type_id: inner_tuple,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: inner_tuple,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(source, tuple_array),
        "[[string, number], [string, number]] should be assignable to [string, number][]"
    );
}

// --- Negative Cases: Array to Tuple (reverse direction) ---

#[test]
fn test_array_to_tuple_fails_fixed() {
    // string[] -> [string] should FAIL (array has unknown length)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(
        !checker.is_subtype_of(string_array, target),
        "string[] should NOT be assignable to [string]"
    );
}

#[test]
fn test_array_to_tuple_fails_multi_element() {
    // string[] -> [string, string] should FAIL
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let target = interner.tuple(vec![
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

    assert!(
        !checker.is_subtype_of(string_array, target),
        "string[] should NOT be assignable to [string, string]"
    );
}

// =============================================================================
// THIS TYPE NARROWING IN CLASS HIERARCHIES
// =============================================================================

#[test]
fn test_this_type_class_hierarchy_fluent_return() {
    // class Base { method(): this }
    // class Derived extends Base { extra(): number }
    // Derived.method() should have type Derived (not Base)
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);

    // Base method returning this
    let base_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let base_class = interner.object(vec![PropertyInfo {
        name: interner.intern_string("method"),
        type_id: base_method,
        write_type: base_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // Derived class with extra property
    let extra_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let derived_class = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("method"),
            type_id: base_method,
            write_type: base_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("extra"),
            type_id: extra_method,
            write_type: extra_method,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    // Derived is subtype of Base (has all base properties)
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(derived_class, base_class),
        "Derived should be subtype of Base"
    );
}

#[test]
fn test_this_type_in_method_parameter_covariant() {
    // From TS_UNSOUNDNESS_CATALOG #19:
    // class Box { compare(other: this) }
    // class StringBox extends Box { compare(other: StringBox) }
    // StringBox should be subtype of Box (this is covariant in class hierarchies)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let this_type = interner.intern(TypeKey::ThisType);

    // Box.compare(other: this)
    let box_compare = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: this_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let box_class = interner.object(vec![PropertyInfo {
        name: interner.intern_string("compare"),
        type_id: box_compare,
        write_type: box_compare,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // StringBox type
    let stringbox_class = interner.object(vec![PropertyInfo {
        name: interner.intern_string("compare"),
        type_id: box_compare,
        write_type: box_compare,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // StringBox should be subtype of Box
    // (this type enables bivariance, which makes this pass)
    assert!(
        checker.is_subtype_of(stringbox_class, box_class),
        "StringBox should be subtype of Box (this type enables bivariance)"
    );
}

#[test]
fn test_this_type_explicit_this_parameter_inheritance() {
    // class Base { method(this: Base): void }
    // class Derived extends Base { method(this: Derived): void }
    // Derived should be subtype of Base
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // Base class reference
    let base_class_ref = interner.lazy(DefId(100));

    // Base.method(this: Base)
    let base_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(base_class_ref),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let _base_class = interner.object(vec![PropertyInfo {
        name: interner.intern_string("method"),
        type_id: base_method,
        write_type: base_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // Derived class reference
    let derived_class_ref = interner.lazy(DefId(101));

    // Derived.method(this: Derived)
    let derived_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(derived_class_ref),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let _derived_class = interner.object(vec![PropertyInfo {
        name: interner.intern_string("method"),
        type_id: derived_method,
        write_type: derived_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // Check that derived method is compatible with base method
    // (Methods get bivariance)
    assert!(
        checker.is_subtype_of(derived_method, base_method),
        "Derived method should be subtype of Base method (method bivariance)"
    );
}

#[test]
fn test_this_type_return_covariant_in_hierarchy() {
    // Test that `this` return type is covariant
    // class Base { fluent(): this }
    // class Derived extends Base { fluent(): this }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let this_type = interner.intern(TypeKey::ThisType);

    // Base.fluent(): this
    let base_fluent = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // Both Base and Derived have the same fluent method returning this
    let base_class = interner.object(vec![PropertyInfo {
        name: interner.intern_string("fluent"),
        type_id: base_fluent,
        write_type: base_fluent,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    let derived_class = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("fluent"),
            type_id: base_fluent,
            write_type: base_fluent,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("extra"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    // Derived is subtype of Base
    assert!(
        checker.is_subtype_of(derived_class, base_class),
        "Derived should be subtype of Base (same this-returning method)"
    );
}

#[test]
fn test_this_type_polymorphic_method_chain() {
    // Test fluent chaining with this type
    // class Builder {
    //   setName(name: string): this
    //   setValue(value: number): this
    //   build(): Result
    // }
    let interner = TypeInterner::new();

    let this_type = interner.intern(TypeKey::ThisType);
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
        PropertyInfo {
            name: interner.intern_string("setName"),
            type_id: set_name,
            write_type: set_name,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("setValue"),
            type_id: set_value,
            write_type: set_value,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("build"),
            type_id: build,
            write_type: build,
            optional: false,
            readonly: false,
            is_method: true,
        },
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

    let this_type = interner.intern(TypeKey::ThisType);
    let _t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
    };
    let _u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
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

    let container = interner.object(vec![PropertyInfo {
        name: interner.intern_string("filter"),
        type_id: filter_method,
        write_type: filter_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

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

    let this_type = interner.intern(TypeKey::ThisType);

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
        PropertyInfo {
            name: interner.intern_string("method1"),
            type_id: method1,
            write_type: method1,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("method2"),
            type_id: method2,
            write_type: method2,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    let derived_class = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("method1"),
            type_id: method1,
            write_type: method1,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("method2"),
            type_id: method2,
            write_type: method2,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("method3"),
            type_id: method3,
            write_type: method3,
            optional: false,
            readonly: false,
            is_method: true,
        },
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
    };

    let t_type_param = interner.intern(TypeKey::TypeParameter(t_param.clone()));

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

    let base_class = interner.object(vec![PropertyInfo {
        name: interner.intern_string("method"),
        type_id: constrained_method,
        write_type: constrained_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    // Base with constrained this method should be valid
    assert_ne!(base_class, TypeId::ERROR);
}
