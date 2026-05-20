//! Comprehensive tests for the `Object`/`{}`/`object` trifecta assignability matrix.
//!
//! Verifies the full 3-column × N-row matrix of TypeScript's three distinct
//! object super-types against every intrinsic, literal, template-literal, and
//! composite source type that tsc handles.
//!
//! Column labels:
//!   `object`  = `TypeId::OBJECT` (lowercase keyword)
//!   `{}`      = empty object literal type (`interner.object([])`)
//!   `Object`  = global Object interface (registered via `TypeEnvironment`)

use super::*;
use crate::TypeInterner;
use crate::def::DefId;

fn make_to_string(interner: &TypeInterner) -> TypeId {
    interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    })
}

fn register_object_iface(interner: &TypeInterner, env: &mut TypeEnvironment, def_id: DefId) {
    let ts = make_to_string(interner);
    let iface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("toString"),
        ts,
    )]);
    env.insert_def(def_id, iface);
    // Register as the boxed Object so `is_global_object_interface_type` fires.
    env.register_boxed_def_id(IntrinsicKind::Object, def_id);
}

// ────────────────────────────────────────────────────────────────────────────
// Intrinsic source × 3 targets — full matrix
// ────────────────────────────────────────────────────────────────────────────

/// All five primitive widening types must satisfy the matrix column pattern:
///   `primitive <: object`  → false
///   `primitive <: {}`      → true
///   `primitive <: Object`  → true
#[test]
fn trifecta_primitives_vs_all_three_targets() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(1));
    let object_ref = interner.lazy(DefId(1));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    for prim in [
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::BIGINT,
        TypeId::SYMBOL,
    ] {
        assert!(
            !checker.is_subtype_of(prim, TypeId::OBJECT),
            "{prim:?} <: object should be false"
        );
        assert!(
            checker.is_subtype_of(prim, empty_obj),
            "{prim:?} <: {{}} should be true"
        );
        assert!(
            checker.is_subtype_of(prim, object_ref),
            "{prim:?} <: Object should be true"
        );
    }
}

/// `null` and `undefined` must be rejected by all three super-types.
#[test]
fn trifecta_null_undefined_rejected_by_all() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(2));
    let object_ref = interner.lazy(DefId(2));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    for nullish in [TypeId::NULL, TypeId::UNDEFINED] {
        assert!(
            !checker.is_subtype_of(nullish, TypeId::OBJECT),
            "{nullish:?} <: object should be false"
        );
        assert!(
            !checker.is_subtype_of(nullish, empty_obj),
            "{nullish:?} <: {{}} should be false"
        );
        assert!(
            !checker.is_subtype_of(nullish, object_ref),
            "{nullish:?} <: Object should be false"
        );
    }
}

/// `void` must be rejected by all three super-types.
#[test]
fn trifecta_void_rejected_by_all() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(3));
    let object_ref = interner.lazy(DefId(3));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(!checker.is_subtype_of(TypeId::VOID, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::VOID, empty_obj));
    assert!(!checker.is_subtype_of(TypeId::VOID, object_ref));
}

/// `unknown` must be rejected by all three super-types.
/// (`unknown` might be null/undefined, so it cannot satisfy a non-nullish constraint.)
#[test]
fn trifecta_unknown_rejected_by_all() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(4));
    let object_ref = interner.lazy(DefId(4));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, empty_obj));
    assert!(!checker.is_subtype_of(TypeId::UNKNOWN, object_ref));
}

/// `never` is the bottom type and must be accepted by all three super-types.
#[test]
fn trifecta_never_accepted_by_all() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(5));
    let object_ref = interner.lazy(DefId(5));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    assert!(checker.is_subtype_of(TypeId::NEVER, TypeId::OBJECT));
    assert!(checker.is_subtype_of(TypeId::NEVER, empty_obj));
    assert!(checker.is_subtype_of(TypeId::NEVER, object_ref));
}

/// Object shapes, arrays, tuples, and functions are accepted by all three.
#[test]
fn trifecta_non_primitive_objects_accepted_by_all() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(6));
    let object_ref = interner.lazy(DefId(6));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

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

    for src in [obj, array, tuple, func] {
        assert!(
            checker.is_subtype_of(src, TypeId::OBJECT),
            "{src:?} <: object should be true"
        );
        assert!(
            checker.is_subtype_of(src, empty_obj),
            "{src:?} <: {{}} should be true"
        );
        assert!(
            checker.is_subtype_of(src, object_ref),
            "{src:?} <: Object should be true"
        );
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Literal source types
// ────────────────────────────────────────────────────────────────────────────

/// String, number, boolean, and bigint literals follow the same matrix as their
/// widened primitive counterparts: rejected by `object`, accepted by `{}` and `Object`.
#[test]
fn trifecta_literal_types_vs_all_three_targets() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(7));
    let object_ref = interner.lazy(DefId(7));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    let str_lit = interner.literal_string("hello");
    let num_lit = interner.literal_number(42.0);
    let bool_true = TypeId::BOOLEAN_TRUE;
    let bool_false = TypeId::BOOLEAN_FALSE;
    let bigint_lit = interner.literal_bigint("100");

    for lit in [str_lit, num_lit, bool_true, bool_false, bigint_lit] {
        assert!(
            !checker.is_subtype_of(lit, TypeId::OBJECT),
            "{lit:?} <: object should be false (literal is a primitive)"
        );
        assert!(
            checker.is_subtype_of(lit, empty_obj),
            "{lit:?} <: {{}} should be true"
        );
        assert!(
            checker.is_subtype_of(lit, object_ref),
            "{lit:?} <: Object should be true"
        );
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Template-literal source types
// ────────────────────────────────────────────────────────────────────────────

/// Template-literal types widen to `string` and therefore follow the string row
/// of the matrix: rejected by `object`, accepted by `{}` and `Object`.
#[test]
fn trifecta_template_literal_follows_string_row() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(8));
    let object_ref = interner.lazy(DefId(8));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    // Template literal: `"hello"` (a single constant text span)
    let tmpl = interner.template_literal(vec![TemplateSpan::Text(interner.intern_string("hello"))]);

    assert!(!checker.is_subtype_of(tmpl, TypeId::OBJECT));
    assert!(checker.is_subtype_of(tmpl, empty_obj));
    assert!(checker.is_subtype_of(tmpl, object_ref));
}

// ────────────────────────────────────────────────────────────────────────────
// Union source types
// ────────────────────────────────────────────────────────────────────────────

/// A union of non-nullish non-primitive types (e.g., `{a:1} | number[]`)
/// follows the full object row: accepted by all three.
#[test]
fn trifecta_union_of_objects_accepted_by_all() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(9));
    let object_ref = interner.lazy(DefId(9));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::NUMBER,
    )]);
    let arr = interner.array(TypeId::NUMBER);
    let union_objs = interner.union(vec![obj, arr]);

    assert!(checker.is_subtype_of(union_objs, TypeId::OBJECT));
    assert!(checker.is_subtype_of(union_objs, empty_obj));
    assert!(checker.is_subtype_of(union_objs, object_ref));
}

/// A union of primitives (`string | number`) is rejected by `object` but
/// accepted by `{}` and `Object`.
#[test]
fn trifecta_union_of_primitives_follows_primitive_row() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(10));
    let object_ref = interner.lazy(DefId(10));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    let union_prims = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    assert!(!checker.is_subtype_of(union_prims, TypeId::OBJECT));
    assert!(checker.is_subtype_of(union_prims, empty_obj));
    assert!(checker.is_subtype_of(union_prims, object_ref));
}

/// A union containing `null` must be rejected by all three super-types.
/// `string | null <: object` — false; `string | null <: {}` — false; etc.
#[test]
fn trifecta_union_with_null_rejected_by_all() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(11));
    let object_ref = interner.lazy(DefId(11));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    let nullable_str = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let nullable_obj = interner.union(vec![
        interner.object(vec![PropertyInfo::new(
            interner.intern_string("x"),
            TypeId::NUMBER,
        )]),
        TypeId::NULL,
    ]);

    assert!(!checker.is_subtype_of(nullable_str, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(nullable_str, empty_obj));
    assert!(!checker.is_subtype_of(nullable_str, object_ref));

    assert!(!checker.is_subtype_of(nullable_obj, TypeId::OBJECT));
    assert!(!checker.is_subtype_of(nullable_obj, empty_obj));
    assert!(!checker.is_subtype_of(nullable_obj, object_ref));
}

/// A mixed union of object and primitive (`{} | string`) follows the primitive
/// row for `object` (rejected) but is accepted by `{}` and `Object`.
#[test]
fn trifecta_mixed_union_object_and_primitive() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(12));
    let object_ref = interner.lazy(DefId(12));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    let obj = interner.object(Vec::new());
    let mixed = interner.union(vec![obj, TypeId::STRING]);

    // string member prevents acceptance by the `object` keyword
    assert!(!checker.is_subtype_of(mixed, TypeId::OBJECT));
    // both object and string are accepted by {} and Object
    assert!(checker.is_subtype_of(mixed, empty_obj));
    assert!(checker.is_subtype_of(mixed, object_ref));
}

// ────────────────────────────────────────────────────────────────────────────
// `any` source
// ────────────────────────────────────────────────────────────────────────────

/// `any <: {}` and `any <: Object` are always true.
/// `any <: object` depends on `AnyPropagationMode`; in the default (`All`) mode
/// it is also true.
#[test]
fn trifecta_any_source_default_mode() {
    let interner = TypeInterner::new();
    let mut env = TypeEnvironment::new();
    register_object_iface(&interner, &mut env, DefId(13));
    let object_ref = interner.lazy(DefId(13));
    let empty_obj = interner.object(Vec::new());
    let mut checker = SubtypeChecker::with_resolver(&interner, &env);

    // Default AnyPropagationMode::All: any is permissive everywhere.
    assert!(checker.is_subtype_of(TypeId::ANY, TypeId::OBJECT));
    assert!(checker.is_subtype_of(TypeId::ANY, empty_obj));
    assert!(checker.is_subtype_of(TypeId::ANY, object_ref));
}
