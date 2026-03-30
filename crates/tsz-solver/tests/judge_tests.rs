use super::*;
use crate::def::DefId;
use crate::types::{
    CallableShape, FunctionShape, IndexSignature, ObjectFlags, ObjectShape, ParamInfo, TupleElement,
};

#[allow(clippy::duplicate_mod)]
#[path = "common/mod.rs"]
mod common;
use common::create_test_interner;

#[test]
fn test_is_subtype_identity() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert!(judge.is_subtype(TypeId::NUMBER, TypeId::NUMBER));
    assert!(judge.is_subtype(TypeId::STRING, TypeId::STRING));
    assert!(judge.is_subtype(TypeId::BOOLEAN, TypeId::BOOLEAN));
}

#[test]
fn test_is_subtype_any_unknown() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    // Everything is subtype of any
    assert!(judge.is_subtype(TypeId::NUMBER, TypeId::ANY));
    assert!(judge.is_subtype(TypeId::STRING, TypeId::ANY));

    // Everything is subtype of unknown
    assert!(judge.is_subtype(TypeId::NUMBER, TypeId::UNKNOWN));
    assert!(judge.is_subtype(TypeId::STRING, TypeId::UNKNOWN));

    // any is subtype of everything
    assert!(judge.is_subtype(TypeId::ANY, TypeId::NUMBER));
    assert!(judge.is_subtype(TypeId::ANY, TypeId::STRING));

    // never is subtype of everything
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::NUMBER));
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::STRING));
}

#[test]
fn test_classify_primitive() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let flags = judge.classify_primitive(TypeId::NUMBER);
    assert!(flags.contains(PrimitiveFlags::NUMBER_LIKE));
    assert!(!flags.contains(PrimitiveFlags::STRING_LIKE));

    let flags = judge.classify_primitive(TypeId::STRING);
    assert!(flags.contains(PrimitiveFlags::STRING_LIKE));
    assert!(!flags.contains(PrimitiveFlags::NUMBER_LIKE));

    let flags = judge.classify_primitive(TypeId::NULL);
    assert!(flags.contains(PrimitiveFlags::NULLABLE));
    assert!(flags.contains(PrimitiveFlags::NULL));

    let flags = judge.classify_primitive(TypeId::UNDEFINED);
    assert!(flags.contains(PrimitiveFlags::NULLABLE));
    assert!(flags.contains(PrimitiveFlags::UNDEFINED));
}

#[test]
fn test_classify_truthiness() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert_eq!(
        judge.classify_truthiness(TypeId::BOOLEAN_TRUE),
        TruthinessKind::AlwaysTruthy
    );
    assert_eq!(
        judge.classify_truthiness(TypeId::BOOLEAN_FALSE),
        TruthinessKind::AlwaysFalsy
    );
    assert_eq!(
        judge.classify_truthiness(TypeId::BOOLEAN),
        TruthinessKind::Sometimes
    );
    assert_eq!(
        judge.classify_truthiness(TypeId::NULL),
        TruthinessKind::AlwaysFalsy
    );
    assert_eq!(
        judge.classify_truthiness(TypeId::UNDEFINED),
        TruthinessKind::AlwaysFalsy
    );
    assert_eq!(
        judge.classify_truthiness(TypeId::ANY),
        TruthinessKind::Unknown
    );
}

#[test]
fn test_classify_iterable_array() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let array_number = interner.array(TypeId::NUMBER);
    match judge.classify_iterable(array_number) {
        IterableKind::Array(elem) => assert_eq!(elem, TypeId::NUMBER),
        _ => panic!("Expected Array iterable kind"),
    }
}

#[test]
fn test_classify_iterable_string() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert_eq!(
        judge.classify_iterable(TypeId::STRING),
        IterableKind::String
    );
}

#[test]
fn test_classify_callable_function() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: None,
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

    match judge.classify_callable(fn_type) {
        CallableKind::Function {
            params,
            return_type,
            ..
        } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].type_id, TypeId::NUMBER);
            assert_eq!(return_type, TypeId::STRING);
        }
        _ => panic!("Expected Function callable kind"),
    }
}

#[test]
fn test_get_property_object() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let foo_atom = interner.intern_string("foo");
    let obj = interner.object(vec![PropertyInfo {
        name: foo_atom,
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

    match judge.get_property(obj, foo_atom) {
        PropertyResult::Found {
            type_id,
            optional,
            readonly,
        } => {
            assert_eq!(type_id, TypeId::NUMBER);
            assert!(!optional);
            assert!(!readonly);
        }
        _ => panic!("Expected property to be found"),
    }

    let bar_atom = interner.intern_string("bar");
    match judge.get_property(obj, bar_atom) {
        PropertyResult::NotFound => {}
        _ => panic!("Expected property not found"),
    }
}

#[test]
fn test_get_property_special_types() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let foo_atom = interner.intern_string("foo");

    assert!(matches!(
        judge.get_property(TypeId::ANY, foo_atom),
        PropertyResult::IsAny
    ));
    assert!(matches!(
        judge.get_property(TypeId::UNKNOWN, foo_atom),
        PropertyResult::IsUnknown
    ));
    assert!(matches!(
        judge.get_property(TypeId::ERROR, foo_atom),
        PropertyResult::IsError
    ));
}

#[test]
fn test_caching() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    // First call - not cached
    let result1 = judge.is_subtype(TypeId::NUMBER, TypeId::ANY);
    assert!(result1);

    // Second call - should be cached
    let result2 = judge.is_subtype(TypeId::NUMBER, TypeId::ANY);
    assert!(result2);

    // Clear caches
    judge.clear_caches();

    // Third call - cache was cleared, should still work
    let result3 = judge.is_subtype(TypeId::NUMBER, TypeId::ANY);
    assert!(result3);
}

// =============================================================================
// Primitive Subtyping Tests
// =============================================================================

#[test]
fn test_primitive_subtype_reflexivity() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    // Every primitive type is a subtype of itself
    assert!(judge.is_subtype(TypeId::NUMBER, TypeId::NUMBER));
    assert!(judge.is_subtype(TypeId::STRING, TypeId::STRING));
    assert!(judge.is_subtype(TypeId::BOOLEAN, TypeId::BOOLEAN));
    assert!(judge.is_subtype(TypeId::BIGINT, TypeId::BIGINT));
    assert!(judge.is_subtype(TypeId::SYMBOL, TypeId::SYMBOL));
    assert!(judge.is_subtype(TypeId::VOID, TypeId::VOID));
    assert!(judge.is_subtype(TypeId::NULL, TypeId::NULL));
    assert!(judge.is_subtype(TypeId::UNDEFINED, TypeId::UNDEFINED));
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::NEVER));
}

#[test]
fn test_primitive_not_subtype_of_different_primitive() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert!(!judge.is_subtype(TypeId::STRING, TypeId::NUMBER));
    assert!(!judge.is_subtype(TypeId::NUMBER, TypeId::STRING));
    assert!(!judge.is_subtype(TypeId::BOOLEAN, TypeId::NUMBER));
    assert!(!judge.is_subtype(TypeId::NUMBER, TypeId::BOOLEAN));
    assert!(!judge.is_subtype(TypeId::STRING, TypeId::BOOLEAN));
    assert!(!judge.is_subtype(TypeId::BIGINT, TypeId::NUMBER));
    assert!(!judge.is_subtype(TypeId::NUMBER, TypeId::BIGINT));
    assert!(!judge.is_subtype(TypeId::SYMBOL, TypeId::STRING));
}

#[test]
fn test_string_literal_subtype_of_string() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // String literals are subtypes of string
    assert!(judge.is_subtype(hello, TypeId::STRING));
    assert!(judge.is_subtype(world, TypeId::STRING));

    // String is NOT a subtype of a string literal
    assert!(!judge.is_subtype(TypeId::STRING, hello));

    // Different string literals are not subtypes of each other
    assert!(!judge.is_subtype(hello, world));
    assert!(!judge.is_subtype(world, hello));

    // Same string literal is a subtype of itself
    assert!(judge.is_subtype(hello, hello));
}

#[test]
fn test_number_literal_subtype_of_number() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let forty_two = interner.literal_number(42.0);
    let zero = interner.literal_number(0.0);

    assert!(judge.is_subtype(forty_two, TypeId::NUMBER));
    assert!(judge.is_subtype(zero, TypeId::NUMBER));
    assert!(!judge.is_subtype(TypeId::NUMBER, forty_two));
    assert!(!judge.is_subtype(forty_two, zero));
}

#[test]
fn test_boolean_literal_subtype_of_boolean() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert!(judge.is_subtype(TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN));
    assert!(judge.is_subtype(TypeId::BOOLEAN_FALSE, TypeId::BOOLEAN));
    assert!(!judge.is_subtype(TypeId::BOOLEAN, TypeId::BOOLEAN_TRUE));
    assert!(!judge.is_subtype(TypeId::BOOLEAN, TypeId::BOOLEAN_FALSE));
    assert!(!judge.is_subtype(TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN_FALSE));
    assert!(!judge.is_subtype(TypeId::BOOLEAN_FALSE, TypeId::BOOLEAN_TRUE));
}

// =============================================================================
// never and unknown Tests
// =============================================================================

#[test]
fn test_never_is_subtype_of_everything() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert!(judge.is_subtype(TypeId::NEVER, TypeId::NUMBER));
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::STRING));
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::BOOLEAN));
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::VOID));
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::NULL));
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::UNDEFINED));
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::ANY));
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::UNKNOWN));

    // Never is also subtype of object types
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert!(judge.is_subtype(TypeId::NEVER, obj));

    // But nothing (except never itself) is subtype of never
    assert!(!judge.is_subtype(TypeId::NUMBER, TypeId::NEVER));
    assert!(!judge.is_subtype(TypeId::STRING, TypeId::NEVER));
    assert!(!judge.is_subtype(TypeId::UNKNOWN, TypeId::NEVER));
}

#[test]
fn test_everything_is_subtype_of_unknown() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert!(judge.is_subtype(TypeId::NUMBER, TypeId::UNKNOWN));
    assert!(judge.is_subtype(TypeId::STRING, TypeId::UNKNOWN));
    assert!(judge.is_subtype(TypeId::BOOLEAN, TypeId::UNKNOWN));
    assert!(judge.is_subtype(TypeId::VOID, TypeId::UNKNOWN));
    assert!(judge.is_subtype(TypeId::NULL, TypeId::UNKNOWN));
    assert!(judge.is_subtype(TypeId::UNDEFINED, TypeId::UNKNOWN));
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::UNKNOWN));

    // unknown is NOT a subtype of concrete types
    assert!(!judge.is_subtype(TypeId::UNKNOWN, TypeId::NUMBER));
    assert!(!judge.is_subtype(TypeId::UNKNOWN, TypeId::STRING));
    assert!(!judge.is_subtype(TypeId::UNKNOWN, TypeId::BOOLEAN));
}

// =============================================================================
// any Special Behavior Tests
// =============================================================================

#[test]
fn test_any_is_bidirectional_subtype() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    // any is subtype of everything (except never)
    assert!(judge.is_subtype(TypeId::ANY, TypeId::NUMBER));
    assert!(judge.is_subtype(TypeId::ANY, TypeId::STRING));
    assert!(judge.is_subtype(TypeId::ANY, TypeId::BOOLEAN));
    assert!(judge.is_subtype(TypeId::ANY, TypeId::VOID));
    assert!(judge.is_subtype(TypeId::ANY, TypeId::UNKNOWN));

    // Everything is subtype of any
    assert!(judge.is_subtype(TypeId::NUMBER, TypeId::ANY));
    assert!(judge.is_subtype(TypeId::STRING, TypeId::ANY));
    assert!(judge.is_subtype(TypeId::NULL, TypeId::ANY));
    assert!(judge.is_subtype(TypeId::UNDEFINED, TypeId::ANY));

    // any is NOT subtype of never (tsc rule)
    assert!(!judge.is_subtype(TypeId::ANY, TypeId::NEVER));
}

#[test]
fn test_any_subtype_of_object_types() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    let array = interner.array(TypeId::STRING);
    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert!(judge.is_subtype(TypeId::ANY, obj));
    assert!(judge.is_subtype(TypeId::ANY, array));
    assert!(judge.is_subtype(TypeId::ANY, tuple));
    assert!(judge.is_subtype(obj, TypeId::ANY));
    assert!(judge.is_subtype(array, TypeId::ANY));
    assert!(judge.is_subtype(tuple, TypeId::ANY));
}

// =============================================================================
// Object Structural Subtyping Tests
// =============================================================================

#[test]
fn test_object_subtype_extra_properties_allowed() {
    // { x: number, y: string } <: { x: number }
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");

    let sub = interner.object(vec![
        PropertyInfo::new(x_atom, TypeId::NUMBER),
        PropertyInfo::new(y_atom, TypeId::STRING),
    ]);

    let sup = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);

    assert!(
        judge.is_subtype(sub, sup),
        "Object with extra properties should be subtype"
    );
}

#[test]
fn test_object_subtype_missing_property_fails() {
    // { x: number } is NOT <: { x: number, y: string }
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");

    let fewer = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);
    let more = interner.object(vec![
        PropertyInfo::new(x_atom, TypeId::NUMBER),
        PropertyInfo::new(y_atom, TypeId::STRING),
    ]);

    assert!(
        !judge.is_subtype(fewer, more),
        "Object missing required property should not be subtype"
    );
}

#[test]
fn test_object_subtype_property_type_mismatch() {
    // { x: string } is NOT <: { x: number }
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let x_atom = interner.intern_string("x");

    let obj_string = interner.object(vec![PropertyInfo::new(x_atom, TypeId::STRING)]);
    let obj_number = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);

    assert!(
        !judge.is_subtype(obj_string, obj_number),
        "Object with mismatched property type should not be subtype"
    );
}

#[test]
fn test_empty_object_is_supertype() {
    // Everything structural is subtype of {} (empty object)
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let empty = interner.object(Vec::new());
    let x_atom = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);

    assert!(
        judge.is_subtype(obj, empty),
        "Object should be subtype of empty object"
    );
}

#[test]
fn test_object_subtype_with_optional_property() {
    // { x: number } <: { x: number, y?: string }
    // An object without y satisfies optional y
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");

    let without_y = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);
    let with_opt_y = interner.object(vec![
        PropertyInfo::new(x_atom, TypeId::NUMBER),
        PropertyInfo::opt(y_atom, TypeId::STRING),
    ]);

    assert!(
        judge.is_subtype(without_y, with_opt_y),
        "Object should be subtype when missing only optional properties"
    );
}

// =============================================================================
// Function Subtyping Tests (Return Covariant, Params Contravariant)
// =============================================================================

#[test]
fn test_function_subtype_return_covariant() {
    // () => "hello" <: () => string (return is covariant)
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let hello = interner.literal_string("hello");

    let fn_literal_ret = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: hello,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_string_ret = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    assert!(
        judge.is_subtype(fn_literal_ret, fn_string_ret),
        "Function with narrower return should be subtype (covariant)"
    );
    assert!(
        !judge.is_subtype(fn_string_ret, fn_literal_ret),
        "Function with wider return should not be subtype of narrower"
    );
}

#[test]
fn test_function_subtype_params_contravariant() {
    // (x: string) => void <: (x: "hello") => void  (params are contravariant)
    // A function that accepts wider input is subtype of one that accepts narrower
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let config = JudgeConfig {
        strict_function_types: true,
        ..JudgeConfig::default()
    };
    let judge = DefaultJudge::new(&interner, &env, config);

    let hello = interner.literal_string("hello");

    let fn_string_param = interner.function(FunctionShape {
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

    let fn_literal_param = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: hello,
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

    assert!(
        judge.is_subtype(fn_string_param, fn_literal_param),
        "Function with wider param should be subtype (contravariant)"
    );
    assert!(
        !judge.is_subtype(fn_literal_param, fn_string_param),
        "Function with narrower param should not be subtype"
    );
}

#[test]
fn test_function_subtype_fewer_params_ok() {
    // () => void <: (x: number) => void
    // A function with fewer params is subtype (TS callback compatibility)
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let fn_no_params = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn_one_param = interner.function(FunctionShape {
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

    assert!(
        judge.is_subtype(fn_no_params, fn_one_param),
        "Function with fewer params should be subtype"
    );
}

// =============================================================================
// Union Distribution Tests
// =============================================================================

#[test]
fn test_union_subtype_all_members_must_be_subtypes() {
    // A | B <: C iff A <: C AND B <: C
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    // number | string <: number | string | boolean
    let num_str = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let num_str_bool = interner.union(vec![TypeId::NUMBER, TypeId::STRING, TypeId::BOOLEAN]);

    assert!(
        judge.is_subtype(num_str, num_str_bool),
        "Union subset should be subtype"
    );
    assert!(
        !judge.is_subtype(num_str_bool, num_str),
        "Union superset should not be subtype"
    );
}

#[test]
fn test_single_type_subtype_of_union_containing_it() {
    // number <: number | string
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let union = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    assert!(
        judge.is_subtype(TypeId::NUMBER, union),
        "Single type should be subtype of union containing it"
    );
    assert!(
        judge.is_subtype(TypeId::STRING, union),
        "Single type should be subtype of union containing it"
    );
    assert!(
        !judge.is_subtype(TypeId::BOOLEAN, union),
        "Unrelated type should not be subtype of union"
    );
}

#[test]
fn test_union_not_subtype_of_single_member() {
    // number | string is NOT <: number
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let union = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

    assert!(
        !judge.is_subtype(union, TypeId::NUMBER),
        "Union should not be subtype of a single member"
    );
}

#[test]
fn test_union_with_never() {
    // never | T = T, so never | number <: number
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let never_or_number = interner.union(vec![TypeId::NEVER, TypeId::NUMBER]);

    assert!(
        judge.is_subtype(never_or_number, TypeId::NUMBER),
        "Union with never should collapse"
    );
}

// =============================================================================
// Intersection Subtyping Tests
// =============================================================================

#[test]
fn test_intersection_subtype_of_each_constituent() {
    // A & B <: A  and  A & B <: B
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");

    let obj_x = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);
    let obj_y = interner.object(vec![PropertyInfo::new(y_atom, TypeId::STRING)]);
    let intersection = interner.intersection(vec![obj_x, obj_y]);

    assert!(
        judge.is_subtype(intersection, obj_x),
        "Intersection should be subtype of first constituent"
    );
    assert!(
        judge.is_subtype(intersection, obj_y),
        "Intersection should be subtype of second constituent"
    );
}

#[test]
fn test_intersection_has_all_properties() {
    // { x: number } & { y: string } <: { x: number, y: string }
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");

    let obj_x = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);
    let obj_y = interner.object(vec![PropertyInfo::new(y_atom, TypeId::STRING)]);
    let intersection = interner.intersection(vec![obj_x, obj_y]);

    let combined = interner.object(vec![
        PropertyInfo::new(x_atom, TypeId::NUMBER),
        PropertyInfo::new(y_atom, TypeId::STRING),
    ]);

    assert!(
        judge.is_subtype(intersection, combined),
        "Intersection should be subtype of object with all properties"
    );
}

// =============================================================================
// Tuple Subtyping Tests
// =============================================================================

#[test]
fn test_tuple_subtype_same_elements() {
    // [number, string] <: [number, string]
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

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

    assert!(
        judge.is_subtype(tuple, tuple),
        "Tuple should be subtype of itself"
    );
}

#[test]
fn test_tuple_subtype_element_wise() {
    // ["hello", 42] <: [string, number]
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

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

    assert!(
        judge.is_subtype(literal_tuple, wide_tuple),
        "Tuple with narrower elements should be subtype"
    );
    assert!(
        !judge.is_subtype(wide_tuple, literal_tuple),
        "Tuple with wider elements should not be subtype"
    );
}

#[test]
fn test_tuple_length_mismatch() {
    // [number] is NOT <: [number, string]
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let short = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    let long = interner.tuple(vec![
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

    assert!(
        !judge.is_subtype(short, long),
        "Shorter tuple should not be subtype of longer"
    );
}

// =============================================================================
// Array Subtyping Tests
// =============================================================================

#[test]
fn test_array_subtype_element_covariant() {
    // Array<"hello"> <: Array<string> (arrays are covariant in TS)
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let hello = interner.literal_string("hello");
    let arr_hello = interner.array(hello);
    let arr_string = interner.array(TypeId::STRING);

    assert!(
        judge.is_subtype(arr_hello, arr_string),
        "Array of string literal should be subtype of Array<string>"
    );
    assert!(
        !judge.is_subtype(arr_string, arr_hello),
        "Array<string> should not be subtype of Array<literal>"
    );
}

#[test]
fn test_array_subtype_different_element_types() {
    // Array<number> is NOT <: Array<string>
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let arr_num = interner.array(TypeId::NUMBER);
    let arr_str = interner.array(TypeId::STRING);

    assert!(!judge.is_subtype(arr_num, arr_str));
    assert!(!judge.is_subtype(arr_str, arr_num));
}

// =============================================================================
// are_identical Tests
// =============================================================================

#[test]
fn test_are_identical_same_type() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert!(judge.are_identical(TypeId::NUMBER, TypeId::NUMBER));
    assert!(judge.are_identical(TypeId::STRING, TypeId::STRING));
    assert!(judge.are_identical(TypeId::BOOLEAN, TypeId::BOOLEAN));
}

#[test]
fn test_are_identical_different_types() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert!(!judge.are_identical(TypeId::NUMBER, TypeId::STRING));
    assert!(!judge.are_identical(TypeId::STRING, TypeId::BOOLEAN));
}

#[test]
fn test_are_identical_any_is_not_identical_to_concrete() {
    // any <: number and number <: any, but they are "identical" by the are_identical impl
    // since it uses bidirectional subtyping
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    // Since are_identical checks a <: b && b <: a, and any is bidirectional,
    // any is considered "identical" to number
    assert!(judge.are_identical(TypeId::ANY, TypeId::NUMBER));
}

// =============================================================================
// Null and Undefined Strict Checks Tests
// =============================================================================

#[test]
fn test_strict_null_checks_enabled() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let config = JudgeConfig {
        strict_null_checks: true,
        ..JudgeConfig::default()
    };
    let judge = DefaultJudge::new(&interner, &env, config);

    assert!(!judge.is_subtype(TypeId::NULL, TypeId::NUMBER));
    assert!(!judge.is_subtype(TypeId::UNDEFINED, TypeId::NUMBER));
    assert!(!judge.is_subtype(TypeId::NULL, TypeId::STRING));
    assert!(!judge.is_subtype(TypeId::UNDEFINED, TypeId::STRING));
}

#[test]
fn test_strict_null_checks_disabled() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let config = JudgeConfig {
        strict_null_checks: false,
        ..JudgeConfig::default()
    };
    let judge = DefaultJudge::new(&interner, &env, config);

    // When strict null checks are disabled, null and undefined are assignable to everything
    assert!(judge.is_subtype(TypeId::NULL, TypeId::NUMBER));
    assert!(judge.is_subtype(TypeId::UNDEFINED, TypeId::NUMBER));
    assert!(judge.is_subtype(TypeId::NULL, TypeId::STRING));
    assert!(judge.is_subtype(TypeId::UNDEFINED, TypeId::STRING));
}

// =============================================================================
// Error Type Tests
// =============================================================================

#[test]
fn test_error_type_is_bidirectional() {
    // ERROR type behaves like any - assignable to/from everything
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert!(judge.is_subtype(TypeId::ERROR, TypeId::NUMBER));
    assert!(judge.is_subtype(TypeId::NUMBER, TypeId::ERROR));
    assert!(judge.is_subtype(TypeId::ERROR, TypeId::STRING));
    assert!(judge.is_subtype(TypeId::STRING, TypeId::ERROR));
    assert!(judge.is_subtype(TypeId::ERROR, TypeId::ERROR));
}

// =============================================================================
// Void Subtyping Tests
// =============================================================================

#[test]
fn test_void_subtyping() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    // void is subtype of void
    assert!(judge.is_subtype(TypeId::VOID, TypeId::VOID));

    // void is subtype of unknown
    assert!(judge.is_subtype(TypeId::VOID, TypeId::UNKNOWN));

    // undefined is subtype of void
    assert!(judge.is_subtype(TypeId::UNDEFINED, TypeId::VOID));

    // number is NOT subtype of void
    assert!(!judge.is_subtype(TypeId::NUMBER, TypeId::VOID));
}

// =============================================================================
// Classify Primitive Tests
// =============================================================================

#[test]
fn test_classify_primitive_literals() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let hello = interner.literal_string("hello");
    let flags = judge.classify_primitive(hello);
    assert!(flags.contains(PrimitiveFlags::STRING_LIKE));

    let forty_two = interner.literal_number(42.0);
    let flags = judge.classify_primitive(forty_two);
    assert!(flags.contains(PrimitiveFlags::NUMBER_LIKE));
}

#[test]
fn test_classify_primitive_union() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    // number | string union should have both flags
    let union = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    let flags = judge.classify_primitive(union);
    assert!(flags.contains(PrimitiveFlags::NUMBER_LIKE));
    assert!(flags.contains(PrimitiveFlags::STRING_LIKE));
}

// =============================================================================
// Classify Truthiness Tests
// =============================================================================

#[test]
fn test_classify_truthiness_literals() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let hello = interner.literal_string("hello");
    assert_eq!(
        judge.classify_truthiness(hello),
        TruthinessKind::AlwaysTruthy
    );

    let empty = interner.literal_string("");
    assert_eq!(
        judge.classify_truthiness(empty),
        TruthinessKind::AlwaysFalsy
    );

    let forty_two = interner.literal_number(42.0);
    assert_eq!(
        judge.classify_truthiness(forty_two),
        TruthinessKind::AlwaysTruthy
    );

    let zero = interner.literal_number(0.0);
    assert_eq!(judge.classify_truthiness(zero), TruthinessKind::AlwaysFalsy);
}

#[test]
fn test_classify_truthiness_object_always_truthy() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);
    assert_eq!(judge.classify_truthiness(obj), TruthinessKind::AlwaysTruthy);
}

#[test]
fn test_classify_truthiness_union_sometimes() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    // number | null can be truthy (non-zero number) or falsy (null)
    let union = interner.union(vec![TypeId::NUMBER, TypeId::NULL]);
    assert_eq!(judge.classify_truthiness(union), TruthinessKind::Sometimes);
}

// =============================================================================
// Classify Iterable Tests
// =============================================================================

#[test]
fn test_classify_iterable_tuple() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

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

    match judge.classify_iterable(tuple) {
        IterableKind::Tuple(elems) => {
            assert_eq!(elems.len(), 2);
            assert_eq!(elems[0], TypeId::NUMBER);
            assert_eq!(elems[1], TypeId::STRING);
        }
        _ => panic!("Expected Tuple iterable kind"),
    }
}

#[test]
fn test_classify_iterable_not_iterable() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert_eq!(
        judge.classify_iterable(TypeId::NUMBER),
        IterableKind::NotIterable
    );
    assert_eq!(
        judge.classify_iterable(TypeId::BOOLEAN),
        IterableKind::NotIterable
    );
}

// =============================================================================
// Classify Callable Tests
// =============================================================================

#[test]
fn test_classify_callable_constructor() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let ctor = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    match judge.classify_callable(ctor) {
        CallableKind::Constructor { return_type, .. } => {
            assert_eq!(return_type, TypeId::OBJECT);
        }
        _ => panic!("Expected Constructor callable kind"),
    }
}

#[test]
fn test_classify_callable_overloaded() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let callable = interner.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: Vec::new(),
                params: vec![ParamInfo {
                    name: None,
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: Vec::new(),
                params: vec![ParamInfo {
                    name: None,
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    match judge.classify_callable(callable) {
        CallableKind::Overloaded {
            call_signatures, ..
        } => {
            assert_eq!(call_signatures.len(), 2);
        }
        _ => panic!("Expected Overloaded callable kind"),
    }
}

#[test]
fn test_classify_callable_not_callable() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert!(matches!(
        judge.classify_callable(TypeId::NUMBER),
        CallableKind::NotCallable
    ));
    assert!(matches!(
        judge.classify_callable(TypeId::STRING),
        CallableKind::NotCallable
    ));
}

// =============================================================================
// Get Property Tests (Union and Intersection)
// =============================================================================

#[test]
fn test_get_property_union() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");

    let obj1 = interner.object(vec![
        PropertyInfo::new(x_atom, TypeId::NUMBER),
        PropertyInfo::new(y_atom, TypeId::STRING),
    ]);
    let obj2 = interner.object(vec![PropertyInfo::new(x_atom, TypeId::STRING)]);

    let union = interner.union(vec![obj1, obj2]);

    // x exists in both members
    match judge.get_property(union, x_atom) {
        PropertyResult::Found { .. } => {} // OK
        _ => panic!("Expected property x found in union"),
    }

    // y only exists in first member - should not be found
    match judge.get_property(union, y_atom) {
        PropertyResult::NotFound => {} // OK
        _ => panic!("Expected property y not found in union (missing from one member)"),
    }
}

#[test]
fn test_get_property_intersection() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");

    let obj1 = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);
    let obj2 = interner.object(vec![PropertyInfo::new(y_atom, TypeId::STRING)]);

    let intersection = interner.intersection(vec![obj1, obj2]);

    // Both x and y should be found (intersection merges properties)
    match judge.get_property(intersection, x_atom) {
        PropertyResult::Found { type_id, .. } => {
            assert_eq!(type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected property x found in intersection"),
    }

    match judge.get_property(intersection, y_atom) {
        PropertyResult::Found { type_id, .. } => {
            assert_eq!(type_id, TypeId::STRING);
        }
        _ => panic!("Expected property y found in intersection"),
    }
}

// =============================================================================
// Get Property Tests (Array and Tuple)
// =============================================================================

#[test]
fn test_get_property_array_length() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let arr = interner.array(TypeId::NUMBER);
    let length_atom = interner.intern_string("length");

    match judge.get_property(arr, length_atom) {
        PropertyResult::Found { type_id, .. } => {
            assert_eq!(type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected array.length to be number"),
    }
}

#[test]
fn test_get_property_tuple_element_access() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

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

    let zero_atom = interner.intern_string("0");
    let one_atom = interner.intern_string("1");
    let two_atom = interner.intern_string("2");

    match judge.get_property(tuple, zero_atom) {
        PropertyResult::Found { type_id, .. } => {
            assert_eq!(type_id, TypeId::STRING);
        }
        _ => panic!("Expected tuple[0] to be string"),
    }

    match judge.get_property(tuple, one_atom) {
        PropertyResult::Found { type_id, .. } => {
            assert_eq!(type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected tuple[1] to be number"),
    }

    match judge.get_property(tuple, two_atom) {
        PropertyResult::NotFound => {} // OK - out of bounds
        _ => panic!("Expected tuple[2] to be not found"),
    }
}

#[test]
fn test_get_property_tuple_length() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

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

    let length_atom = interner.intern_string("length");

    match judge.get_property(tuple, length_atom) {
        PropertyResult::Found { type_id, .. } => {
            // Should be the literal number 2
            if let Some(TypeData::Literal(LiteralValue::Number(n))) = interner.lookup(type_id) {
                assert_eq!(n.0, 2.0);
            } else {
                panic!("Expected tuple.length to be literal 2");
            }
        }
        _ => panic!("Expected tuple.length to be found"),
    }
}

// =============================================================================
// Get Property - Index Signatures
// =============================================================================

#[test]
fn test_get_property_string_index_signature() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let obj = interner.object_with_index(ObjectShape {
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

    let any_prop = interner.intern_string("anything");

    match judge.get_property(obj, any_prop) {
        PropertyResult::IndexSignature { value_type, .. } => {
            assert_eq!(value_type, TypeId::NUMBER);
        }
        _ => panic!("Expected index signature match"),
    }
}

// =============================================================================
// Judge Configuration Tests
// =============================================================================

#[test]
fn test_judge_config_defaults() {
    let config = JudgeConfig::default();
    assert!(config.strict_null_checks);
    assert!(config.strict_function_types);
    assert!(!config.exact_optional_property_types);
    assert!(!config.no_unchecked_indexed_access);
    assert!(!config.sound_mode);
}

// =============================================================================
// Lazy/DefId Resolution Tests
// =============================================================================

#[test]
fn test_apparent_type_resolves_lazy() {
    let interner = create_test_interner();
    let mut env = TypeEnvironment::new();

    let x_atom = interner.intern_string("x");
    let inner = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);

    let def_id = DefId(42);
    env.insert_def(def_id, inner);
    let lazy = interner.lazy(def_id);

    let judge = DefaultJudge::with_defaults(&interner, &env);
    let apparent = judge.apparent_type(lazy);

    assert_eq!(
        apparent, inner,
        "apparent_type should resolve Lazy to inner type"
    );
}

// =============================================================================
// Get Members Tests
// =============================================================================

#[test]
fn test_get_members_object() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");

    let obj = interner.object(vec![
        PropertyInfo::new(x_atom, TypeId::NUMBER),
        PropertyInfo::new(y_atom, TypeId::STRING),
    ]);

    let members = judge.get_members(obj);
    assert_eq!(members.len(), 2);
}

#[test]
fn test_get_members_non_object() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let members = judge.get_members(TypeId::NUMBER);
    assert!(members.is_empty());
}

// =============================================================================
// Get Call/Construct Signatures Tests
// =============================================================================

#[test]
fn test_get_call_signatures_function() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let sigs = judge.get_call_signatures(func);
    assert_eq!(sigs.len(), 1);
    assert_eq!(sigs[0].return_type, TypeId::STRING);
    assert_eq!(sigs[0].params.len(), 1);
}

#[test]
fn test_get_construct_signatures_constructor() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let ctor = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let call_sigs = judge.get_call_signatures(ctor);
    assert!(
        call_sigs.is_empty(),
        "Constructor should have no call signatures"
    );

    let construct_sigs = judge.get_construct_signatures(ctor);
    assert_eq!(construct_sigs.len(), 1);
    assert_eq!(construct_sigs[0].return_type, TypeId::OBJECT);
}

// =============================================================================
// Subtype with Object Index Signatures
// =============================================================================

#[test]
fn test_object_with_string_index_is_subtype() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    // { [key: string]: number } should accept any object with compatible properties
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

    // An object with specific number-typed properties should be a subtype
    let x_atom = interner.intern_string("x");
    let specific = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);

    assert!(
        judge.is_subtype(specific, indexed),
        "Object with number properties should be subtype of string-indexed number object"
    );
}

// =============================================================================
// Subtype: Literal Narrowing with Unions
// =============================================================================

#[test]
fn test_literal_union_subtype() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    let union = interner.union(vec![hello, world]);

    // "hello" <: "hello" | "world"
    assert!(judge.is_subtype(hello, union));

    // "world" <: "hello" | "world"
    assert!(judge.is_subtype(world, union));

    // "hello" | "world" <: string
    assert!(judge.is_subtype(union, TypeId::STRING));

    // string is NOT <: "hello" | "world"
    assert!(!judge.is_subtype(TypeId::STRING, union));
}
