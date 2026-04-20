use super::*;
use crate::QueryCache;
use crate::TypeInterner;
use crate::TypeResolver;
use crate::def::DefId;
use crate::diagnostics::SubtypeFailureReason;
use crate::{TypeSubstitution, Visibility, instantiate_type};
use tsz_binder::SymbolId;

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

    // tsc rule: `if (s & TypeFlags.Any) return !(t & TypeFlags.Never)`
    // any is NOT assignable to never, even in tsc's own assignability check.
    assert!(!checker.is_subtype_of(TypeId::ANY, TypeId::NEVER));
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
fn test_error_type_permissive_subtyping() {
    // ERROR types are assignable to/from everything (like `any` in tsc).
    // This prevents cascading diagnostics when type resolution fails.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // ERROR is a subtype of concrete types (like `any`)
    assert!(checker.is_subtype_of(TypeId::ERROR, TypeId::STRING));
    // Concrete types are subtypes of ERROR (like `any`)
    assert!(checker.is_subtype_of(TypeId::STRING, TypeId::ERROR));
    // ERROR is a subtype of itself (reflexive)
    assert!(checker.is_subtype_of(TypeId::ERROR, TypeId::ERROR));
}
#[test]
fn test_error_type_acts_like_any() {
    // ERROR acts like `any` — assignable to/from all types.
    // This matches tsc behavior where errorType silences cascading errors.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    // ERROR is a subtype of object types (like `any`)
    assert!(checker.is_subtype_of(TypeId::ERROR, TypeId::OBJECT));
    // Tuples are subtypes of ERROR (like `any`)
    assert!(checker.is_subtype_of(tuple, TypeId::ERROR));
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
fn test_synthetic_promise_base_is_covariant_in_inner_type() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let one_name = interner.intern_string("one");
    let two_name = interner.intern_string("two");

    let source_tuple = interner.tuple(vec![
        TupleElement {
            type_id: interner.literal_number(1.0),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.literal_string("two"),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let target_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(one_name),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(two_name),
            optional: false,
            rest: false,
        },
    ]);

    let source_promise = interner.application(TypeId::PROMISE_BASE, vec![source_tuple]);
    let target_promise = interner.application(TypeId::PROMISE_BASE, vec![target_tuple]);

    assert!(checker.is_subtype_of(source_promise, target_promise));
    assert!(!checker.is_subtype_of(target_promise, source_promise));
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
    let target = interner.object(vec![PropertyInfo::method(to_upper, method(TypeId::STRING))]);
    let mismatch = interner.object(vec![PropertyInfo::method(to_upper, method(TypeId::NUMBER))]);

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
    let mismatch = interner.object_with_index(ObjectShape {
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
    let target = interner.object(vec![PropertyInfo::method(to_fixed, method(TypeId::STRING))]);

    let mismatch = interner.object(vec![PropertyInfo::method(to_fixed, method(TypeId::NUMBER))]);

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
    let target = interner.object(vec![PropertyInfo::method(to_upper, method(TypeId::STRING))]);
    let mismatch = interner.object(vec![PropertyInfo::method(to_upper, method(TypeId::NUMBER))]);

    assert!(checker.is_subtype_of(TypeId::STRING, target));
    assert!(!checker.is_subtype_of(TypeId::STRING, mismatch));
}
#[test]
fn test_generic_function_mapped_apparent_constraint_not_erased_by_alpha_rename() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
    });

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    let t_key = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(interner.keyof(t_type)),
        default: None,
        is_const: false,
    };
    let t_key_type = interner.type_param(t_key);
    let foo_param_type = interner.mapped(MappedType {
        type_param: t_key,
        constraint: interner.keyof(t_type),
        name_type: None,
        template: interner.index_access(t_type, t_key_type),
        readonly_modifier: None,
        optional_modifier: None,
    });
    let foo = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("target")),
            type_id: foo_param_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![t_param],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(interner.array(TypeId::STRING)),
        default: None,
        is_const: false,
    };
    let u_type = interner.type_param(u_param);
    let u_key = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: Some(interner.keyof(u_type)),
        default: None,
        is_const: false,
    };
    let u_key_type = interner.type_param(u_key);
    let bar_param_type = interner.mapped(MappedType {
        type_param: u_key,
        constraint: interner.keyof(u_type),
        name_type: None,
        template: interner.index_access(obj, u_key_type),
        readonly_modifier: None,
        optional_modifier: None,
    });
    let bar = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("source")),
            type_id: bar_param_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![u_param],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(
        !checker.is_subtype_of(foo, bar),
        "target constraint must remain visible so mapped apparent members stay incompatible"
    );
}
#[test]
fn test_apparent_string_length_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let length = interner.intern_string("length");
    let target = interner.object(vec![PropertyInfo::new(length, TypeId::NUMBER)]);
    let mismatch = interner.object(vec![PropertyInfo::new(length, TypeId::STRING)]);

    assert!(checker.is_subtype_of(TypeId::STRING, target));
    assert!(!checker.is_subtype_of(TypeId::STRING, mismatch));
}
#[test]
fn test_apparent_string_number_index_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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
    let mismatch = interner.object_with_index(ObjectShape {
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
    let target = interner.object(vec![PropertyInfo::method(
        value_of,
        method(TypeId::BOOLEAN),
    )]);
    let mismatch = interner.object(vec![PropertyInfo::method(value_of, method(TypeId::NUMBER))]);

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
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    let mismatch = interner.object(vec![PropertyInfo {
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
    let target = interner.object(vec![PropertyInfo::method(value_of, method(TypeId::BIGINT))]);
    let mismatch = interner.object(vec![PropertyInfo::method(value_of, method(TypeId::NUMBER))]);

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
    let target = interner.object(vec![PropertyInfo::method(has_own, method(TypeId::BOOLEAN))]);
    let mismatch = interner.object(vec![PropertyInfo::method(has_own, method(TypeId::STRING))]);

    assert!(checker.is_subtype_of(TypeId::NUMBER, target));
    assert!(!checker.is_subtype_of(TypeId::NUMBER, mismatch));
}
#[test]
fn test_object_trifecta_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
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
    let object_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toString"),
        to_string,
    )]);

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
    let object_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toString"),
        to_string,
    )]);
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
    let number_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toFixed"),
        to_fixed,
    )]);

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
    let bigint_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toString"),
        to_string,
    )]);

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
    let boolean_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toString"),
        to_string,
    )]);

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
    let string_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toUpperCase"),
        to_upper,
    )]);

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
    let symbol_interface = interner.object(vec![PropertyInfo::new(
        interner.intern_string("description"),
        description,
    )]);

    let def_id = DefId(6);
    env.insert_def(def_id, symbol_interface);
    let symbol_ref = interner.lazy(def_id);

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(checker.is_subtype_of(TypeId::SYMBOL, symbol_ref));
    assert!(!checker.is_subtype_of(symbol_ref, TypeId::SYMBOL));
}

/// Regression test: primitive → object must be rejected even when boxed wrappers
/// are registered. Previously, `is_target_boxed_type` had a structural fallback
/// that checked `Number_interface <: object` (unidirectional). Since Number IS an
/// object type, this returned true — incorrectly treating `object` as the Number
/// boxed wrapper. The fix requires bidirectional subtyping (structural equivalence).
#[test]
fn test_primitive_not_subtype_of_object_with_boxed_wrappers_registered() {
    let interner = TypeInterner::new();

    // Create Number boxed wrapper interface
    let to_fixed = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let number_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toFixed"),
        to_fixed,
    )]);

    // Create String boxed wrapper interface
    let to_upper = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let string_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toUpperCase"),
        to_upper,
    )]);

    // Create Boolean boxed wrapper interface
    let to_string = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let boolean_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toString"),
        to_string,
    )]);

    // Register boxed wrappers on the interner (simulating what the checker does)
    interner.set_boxed_type(IntrinsicKind::Number, number_interface);
    interner.set_boxed_type(IntrinsicKind::String, string_interface);
    interner.set_boxed_type(IntrinsicKind::Boolean, boolean_interface);

    let mut checker = SubtypeChecker::new(&interner);

    // Primitives → object must FAIL (object = non-primitive keyword)
    assert!(!checker.is_subtype_of(TypeId::NUMBER, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::STRING, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::BOOLEAN, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::BIGINT, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::SYMBOL, TypeId::OBJECT));

    // Primitives → their boxed wrapper must SUCCEED
    assert!(checker.is_subtype_of(TypeId::NUMBER, number_interface));
    assert!(checker.is_subtype_of(TypeId::STRING, string_interface));
    assert!(checker.is_subtype_of(TypeId::BOOLEAN, boolean_interface));

    // Object types → object must SUCCEED
    let plain_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(checker.is_subtype_of(plain_obj, TypeId::OBJECT));
    assert!(checker.is_subtype_of(number_interface, TypeId::OBJECT));

    // Nullish → object must FAIL
    assert!(!checker.is_subtype_of(TypeId::NULL, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::UNDEFINED, TypeId::OBJECT));
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

    let weak_target = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);

    let no_overlap = interner.object(vec![PropertyInfo::new(b, TypeId::NUMBER)]);

    let overlap = interner.object(vec![PropertyInfo::new(a, TypeId::NUMBER)]);

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

    let weak_target = interner.object(vec![PropertyInfo::opt(a, TypeId::NUMBER)]);

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
        PropertyInfo::opt(a, TypeId::NUMBER),
        PropertyInfo::opt(b, TypeId::STRING),
    ]);

    // SubtypeChecker no longer rejects based on weak type rules
    let no_overlap = interner.object(vec![PropertyInfo::new(c, TypeId::BOOLEAN)]);
    // SubtypeChecker passes this (CompatChecker would reject it)
    assert!(checker.is_subtype_of(no_overlap, weak_target));

    // Partial overlap (shares 'a' property) - should pass
    let partial_overlap = interner.object(vec![PropertyInfo::new(a, TypeId::NUMBER)]);
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
        PropertyInfo::opt(a, TypeId::NUMBER),
        PropertyInfo {
            name: b,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false, // Required!
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ]);

    let c = interner.intern_string("c");
    let unrelated_source = interner.object(vec![PropertyInfo::new(c, TypeId::BOOLEAN)]);

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
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let narrow_accessor = interner.object(vec![PropertyInfo {
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
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    let source = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::UNDEFINED,
        write_type: TypeId::UNDEFINED,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert!(checker.is_subtype_of(source, target));

    checker.exact_optional_property_types = true;
    assert!(!checker.is_subtype_of(source, target));
}
#[test]
fn test_unique_symbol_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let sym_a = interner.intern(TypeData::UniqueSymbol(SymbolRef(1)));
    let sym_b = interner.intern(TypeData::UniqueSymbol(SymbolRef(2)));

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
    // Following tsc's semantics, DepthExceeded is treated as true (Ternary.Maybe).
    // This matches tsc's behavior where recursive depth overflow assumes types are
    // related, preventing false TS2344 errors on circular generic constraints.
    // The depth_exceeded flag is still set for TS2589 diagnostic emission.
    let result = checker.check_subtype(deep_string, deep_number);
    assert!(matches!(result, SubtypeResult::DepthExceeded));
    assert!(checker.guard.is_exceeded());
}
#[test]
fn test_no_unchecked_indexed_access_array_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let index_access = interner.intern(TypeData::IndexAccess(string_array, TypeId::NUMBER));

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
    let index_access = interner.intern(TypeData::IndexAccess(tuple, TypeId::NUMBER));
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

    let index_access = interner.intern(TypeData::IndexAccess(indexed, TypeId::NUMBER));

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

    let index_access = interner.intern(TypeData::IndexAccess(indexed, TypeId::STRING));

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

    let index_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let index_access = interner.intern(TypeData::IndexAccess(indexed, index_type));

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
        PropertyInfo::new(kind, interner.literal_string("a")),
        PropertyInfo::new(key_a, TypeId::NUMBER),
    ]);
    let obj_b = interner.object(vec![
        PropertyInfo::new(kind, interner.literal_string("b")),
        PropertyInfo::new(key_b, TypeId::STRING),
    ]);

    let union_obj = interner.union(vec![obj_a, obj_b]);
    let key_union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);
    let index_access = interner.intern(TypeData::IndexAccess(union_obj, key_union));
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    assert!(checker.is_subtype_of(index_access, expected));
    assert!(!checker.is_subtype_of(index_access, TypeId::NUMBER));
}
#[test]
fn test_object_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    // { x: number }
    let obj_x = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    // { x: number, y: string }
    let obj_xy = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
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
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    let mutable_obj = interner.object(vec![PropertyInfo {
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
    }]);

    // TypeScript allows readonly property → mutable property assignment
    assert!(checker.is_subtype_of(readonly_obj, mutable_obj));
    assert!(checker.is_subtype_of(mutable_obj, readonly_obj));
}
#[test]
fn test_readonly_array_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let mutable_array = interner.array(TypeId::STRING);
    let readonly_array = interner.intern(TypeData::ReadonlyType(mutable_array));

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
    let readonly_tuple = interner.intern(TypeData::ReadonlyType(tuple));

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
fn test_array_to_iterable_protocol_subtyping() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);
    let mut checker = SubtypeChecker::with_resolver(&interner, &cache).with_query_db(&cache);

    let array_length = interner.intern_string("length");
    let array_base = interner.object(vec![PropertyInfo::readonly(array_length, TypeId::NUMBER)]);
    interner.set_array_base_type(array_base, vec![]);

    let iterator_name = interner.intern_string("[Symbol.iterator]");
    let next_name = interner.intern_string("next");
    let value_name = interner.intern_string("value");
    let done_name = interner.intern_string("done");

    let iterator_result_type = |value_ty| {
        interner.object(vec![
            PropertyInfo::new(value_name, value_ty),
            PropertyInfo::readonly(done_name, TypeId::BOOLEAN),
        ])
    };

    let iterator_type = |value_ty| {
        let next = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: iterator_result_type(value_ty),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        interner.object(vec![PropertyInfo::method(next_name, next)])
    };

    let iterable_of = |value_ty| {
        let iter_fn = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: iterator_type(value_ty),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        interner.object(vec![PropertyInfo::method(iterator_name, iter_fn)])
    };

    let iterable_number = iterable_of(TypeId::NUMBER);
    let iterable_string = iterable_of(TypeId::STRING);
    let iterator_info =
        crate::operations::iterators::get_iterator_info(&cache, iterable_number, false)
            .expect("iterable target should expose iterable info");
    assert_eq!(iterator_info.yield_type, TypeId::NUMBER);
    let source = interner.array(TypeId::NUMBER);

    assert!(!checker.is_subtype_of(array_base, iterable_number));
    let interface_result = checker
        .check_array_interface_subtype(TypeId::NUMBER, iterable_number)
        .expect("array interface check should apply");
    assert!(interface_result.is_true());
    assert!(checker.is_subtype_of(source, iterable_number));
    assert!(!checker.is_subtype_of(source, iterable_string));
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
fn test_reference_lazy_fallback_uses_symbol_to_def_mapping() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    // Register a real DefId and map a raw SymbolId back to it.
    let real_def = DefId(100);
    env.insert_def(real_def, TypeId::STRING);
    env.register_def_symbol_mapping(real_def, SymbolId(5));

    let raw_reference = interner.reference(SymbolRef(5));

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker.is_subtype_of(raw_reference, TypeId::STRING));
    assert!(!checker.is_subtype_of(raw_reference, TypeId::NUMBER));
}
#[test]
fn test_lazy_type_params_falls_back_from_symbol_based_lazy_ref() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let generic_def = DefId(200);
    env.insert_def_with_params(generic_def, TypeId::STRING, vec![t_param]);
    env.register_def_symbol_mapping(generic_def, SymbolId(42));

    let raw_lazy = env
        .get_lazy_type_params(DefId(42))
        .expect("fallback should resolve params");
    assert_eq!(raw_lazy.len(), 1);
    assert_eq!(raw_lazy[0], t_param);

    let symbol_reference = interner.reference(SymbolRef(42));
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(checker.is_subtype_of(symbol_reference, TypeId::STRING));
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
    let obj_x = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    // Create a Ref that resolves to { x: number, y: string }
    let obj_xy = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
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
        params: vec![ParamInfo::unnamed(TypeId::STRING)],
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
        params: vec![ParamInfo::unnamed(TypeId::NUMBER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // (x: number) => void IS subtype of (...args: any[]) => void
    // because `any` in the target rest parameter is always compatible.
    assert!(checker.is_subtype_of(source, target));

    checker.allow_bivariant_rest = true;
    assert!(checker.is_subtype_of(source, target));
}
#[test]
fn test_never_param_is_not_subtype_of_any_rest_target() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);
    checker.allow_bivariant_rest = true;

    let target = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: interner.array(TypeId::ANY),
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
        params: vec![ParamInfo::unnamed(TypeId::NEVER)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(!checker.is_subtype_of(source, target));
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
    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("0"),
        TypeId::STRING,
    )]);

    // { [x: number]: string }
    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
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
    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("0"),
        TypeId::NUMBER,
    )]);

    // { [x: number]: string }
    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    // This should FAIL - numeric property has wrong type
    assert!(!checker.is_subtype_of(source, target));
}
#[test]
fn test_anonymous_number_index_signature_vacuously_compatible_with_no_numeric_keys() {
    // Anonymous object types are allowed to satisfy numeric index signatures
    // structurally when they have no numeric members.
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("one"),
        TypeId::NUMBER,
    )]);

    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    assert!(checker.is_subtype_of(source, target));
}
#[test]
fn test_named_object_without_number_index_does_not_satisfy_number_index_target() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::new(
            interner.intern_string("one"),
            TypeId::NUMBER,
        )],
        ObjectFlags::empty(),
        Some(SymbolId(1)),
    );

    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    assert!(!checker.is_subtype_of(source, target));
    assert!(matches!(
        checker.explain_failure(source, target),
        Some(SubtypeFailureReason::MissingIndexSignature {
            index_kind: "number"
        })
    ));
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

    let source_method = interner.object(vec![PropertyInfo::method(
        interner.intern_string("0"),
        narrow_method,
    )]);

    let source_prop = interner.object(vec![PropertyInfo::new(
        interner.intern_string("0"),
        narrow_method,
    )]);

    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: wide_fn,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    };
    let target = interner.object_with_index(target_shape);

    assert!(checker.is_subtype_of(source_method, target));
    assert!(!checker.is_subtype_of(source_prop, target));
}
#[test]
fn test_named_class_satisfies_string_index_signature_structurally() {
    // A named class with a method { foo(): void } is structurally assignable
    // to { [key: string]: unknown } because the method's return type (void)
    // is assignable to unknown (the index signature value type).
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let class_def = DefId(4100);
    let class_symbol = SymbolId(4100);
    env.register_def_symbol_mapping(class_def, class_symbol);
    env.insert_def_kind(class_def, crate::def::DefKind::Class);

    let source = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::method(
            interner.intern_string("foo"),
            interner.function(FunctionShape {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: true,
            }),
        )],
        ObjectFlags::empty(),
        Some(class_symbol),
    );

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::UNKNOWN,
            readonly: false,
            param_name: None,
        }),
    });

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    // Named class structurally satisfies the string index signature because
    // all its properties (foo: () => void) have values assignable to unknown.
    assert!(checker.is_subtype_of(source, target));
}
#[test]
fn test_namespace_object_can_satisfy_string_index_structurally() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();

    let namespace_def = DefId(4101);
    let namespace_symbol = SymbolId(4101);
    env.register_def_symbol_mapping(namespace_def, namespace_symbol);
    env.insert_def_kind(namespace_def, crate::def::DefKind::Namespace);

    let source = interner.object_with_flags_and_symbol(
        vec![PropertyInfo::new(
            interner.intern_string("unrelated"),
            TypeId::NUMBER,
        )],
        ObjectFlags::empty(),
        Some(namespace_symbol),
    );

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::UNKNOWN,
            readonly: false,
            param_name: None,
        }),
    });

    let mut checker = SubtypeChecker::with_resolver(&interner, &env);
    assert!(
        checker.is_subtype_of(source, target),
        "Namespace value objects should keep their implicit structural compatibility"
    );
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

    let source_method = interner.object(vec![PropertyInfo::method(
        interner.intern_string("foo"),
        narrow_method,
    )]);

    let source_prop = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        narrow_method,
    )]);

    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: wide_fn,
            readonly: false,
            param_name: None,
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
        PropertyInfo::new(interner.intern_string("0"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("1"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("2"), TypeId::STRING),
    ]);

    // { [x: number]: string }
    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
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
        PropertyInfo::new(interner.intern_string("0"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("foo"), TypeId::STRING),
    ]);

    // { [x: number]: string; [y: string]: string }
    let target_shape = ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
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
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));
}
#[test]
fn test_readonly_index_signature_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let readonly_source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
    });

    let mutable_target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let readonly_target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
    });

    // Per tsc behavior, readonly on index signatures does NOT affect assignability.
    // A readonly index signature IS assignable to a mutable index signature.
    assert!(checker.is_subtype_of(readonly_source, mutable_target));
    assert!(checker.is_subtype_of(mutable_target, readonly_target));
}
#[test]
fn test_readonly_property_with_mutable_index_signature() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo::readonly(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let mutable_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let readonly_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
    });

    // tsc allows readonly properties to be assigned to writable index signatures.
    // The readonly constraint prevents writing through the source reference, but
    // doesn't prevent the type from satisfying a writable index signature target.
    assert!(checker.is_subtype_of(source, mutable_index));
    assert!(checker.is_subtype_of(source, readonly_index));
}
#[test]
fn test_object_with_index_properties_match_target_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![
            PropertyInfo::new(interner.intern_string("0"), TypeId::STRING),
            PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        ],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(checker.is_subtype_of(source, target));
}
#[test]
fn test_object_with_index_property_mismatch_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("name"),
            TypeId::STRING,
        )],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));
}
#[test]
fn test_object_with_index_property_mismatch_number_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("0"),
            TypeId::STRING,
        )],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
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
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    // Index signatures do NOT satisfy required named properties (TS2741)
    assert!(!checker.is_subtype_of(source, target));
}
#[test]
fn test_object_with_index_named_property_mismatch_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    assert!(!checker.is_subtype_of(source, target));
}
#[test]
fn test_object_to_indexed_property_mismatch_string_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(!checker.is_subtype_of(source, target));
}
#[test]
fn test_object_with_index_satisfies_numeric_property_number_index() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    });

    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("0"),
        TypeId::STRING,
    )]);

    // Index signatures do NOT satisfy required named properties (TS2741)
    assert!(!checker.is_subtype_of(source, target));
}
#[test]
fn test_object_with_index_noncanonical_numeric_property_fails() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        string_index: None,
    });

    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("01"),
        TypeId::STRING,
    )]);

    assert!(!checker.is_subtype_of(source, target));
}
#[test]
fn test_object_with_index_readonly_index_to_mutable_property_fails() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        number_index: None,
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
    });

    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);

    assert!(!checker.is_subtype_of(source, target));
}
#[test]
fn test_type_parameter_constraint_assignability() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    assert!(checker.is_subtype_of(t_param, TypeId::STRING));
    assert!(!checker.is_subtype_of(t_param, TypeId::NUMBER));

    let unconstrained = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    assert!(!checker.is_subtype_of(unconstrained, TypeId::STRING));
}
#[test]
fn test_base_constraint_assignability_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));
    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));
    let v_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: Some(TypeId::NUMBER),
        default: None,
        is_const: false,
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

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    assert!(!checker.is_subtype_of(TypeId::STRING, t_param));
}
#[test]
fn test_type_parameter_identity_only() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));
    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    }));

    assert!(!checker.is_subtype_of(t_param, u_param));
}
#[test]
fn test_deferred_conditional_source_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
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

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
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

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
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

    // A structural mismatch that cannot match via subtype_of_conditional_target either:
    // Different extends AND branches that don't cover the source branches.
    let real_mismatch = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::STRING,
        false_type: TypeId::STRING,
        is_distributive: true,
    });

    assert!(checker.is_subtype_of(source, target));
    // Note: source <: mismatch passes via fallthrough + subtype_of_conditional_target
    // because mismatch's branches are (number|boolean), which covers source's branches.
    // tsc would reject this for local type aliases but accept it for generic type aliases.
    // Our solver treats both the same way (accepting), which is the more permissive behavior.
    assert!(checker.is_subtype_of(source, mismatch));
    // A true structural mismatch: target branches are `string`, which don't cover source branches.
    assert!(!checker.is_subtype_of(source, real_mismatch));
}
#[test]
fn test_conditional_tuple_wrapper_no_distribution_subtyping() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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
        symbol: None,
        is_abstract: false,
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
        symbol: None,
        is_abstract: false,
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

    let wide_obj = interner.object(vec![PropertyInfo::method(method_name, wide_method)]);
    let narrow_obj = interner.object(vec![PropertyInfo::method(method_name, narrow_method)]);

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
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let target = interner.object(vec![PropertyInfo {
        name,
        type_id: wide_func,
        write_type: wide_func,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    assert!(checker.is_subtype_of(source, target));
}
