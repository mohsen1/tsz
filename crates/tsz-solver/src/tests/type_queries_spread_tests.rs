//! Tests for `is_valid_spread_type` — verifying that spread validation
//! matches tsc's `isValidSpreadType()` behavior.
//!
//! Key behaviors:
//! - Primitives, literals, unknown, null, undefined, void are NOT spreadable
//! - Objects, arrays, functions, any, never, error ARE spreadable
//! - Unions: definitely-falsy members are removed first, then remaining checked
//! - Type parameters: resolved to constraint before checking

use super::*;
use crate::intern::TypeInterner;
use crate::type_queries::is_valid_spread_type;
use crate::types::TypeParamInfo;

// =============================================================================
// Basic spreadable / non-spreadable types
// =============================================================================

#[test]
fn spread_any_never_error_are_valid() {
    let db = TypeInterner::new();
    assert!(is_valid_spread_type(&db, TypeId::ANY));
    assert!(is_valid_spread_type(&db, TypeId::NEVER));
    assert!(is_valid_spread_type(&db, TypeId::ERROR));
}

#[test]
fn spread_primitives_are_invalid() {
    let db = TypeInterner::new();
    assert!(!is_valid_spread_type(&db, TypeId::STRING));
    assert!(!is_valid_spread_type(&db, TypeId::NUMBER));
    assert!(!is_valid_spread_type(&db, TypeId::BOOLEAN));
    assert!(!is_valid_spread_type(&db, TypeId::BIGINT));
    assert!(!is_valid_spread_type(&db, TypeId::SYMBOL));
    assert!(!is_valid_spread_type(&db, TypeId::UNKNOWN));
}

#[test]
fn spread_null_undefined_void_are_invalid() {
    let db = TypeInterner::new();
    assert!(!is_valid_spread_type(&db, TypeId::NULL));
    assert!(!is_valid_spread_type(&db, TypeId::UNDEFINED));
    assert!(!is_valid_spread_type(&db, TypeId::VOID));
}

#[test]
fn spread_object_type_is_valid() {
    let db = TypeInterner::new();
    let obj = db.object(vec![]);
    assert!(is_valid_spread_type(&db, obj));
}

// =============================================================================
// Union with falsy members (the core fix)
// =============================================================================

#[test]
fn spread_union_with_false_and_object_is_valid() {
    // `false | { x: number }` should be valid for spread.
    // tsc removes `false` (definitely falsy) leaving only the object.
    let db = TypeInterner::new();
    let obj = db.object(vec![]);
    let union = db.union(vec![TypeId::BOOLEAN_FALSE, obj]);
    assert!(is_valid_spread_type(&db, union));
}

#[test]
fn spread_union_with_null_and_object_is_valid() {
    // `null | { x: number }` — null is definitely falsy, removed.
    let db = TypeInterner::new();
    let obj = db.object(vec![]);
    let union = db.union(vec![TypeId::NULL, obj]);
    assert!(is_valid_spread_type(&db, union));
}

#[test]
fn spread_union_with_undefined_and_object_is_valid() {
    // `undefined | { x: number }` — undefined is definitely falsy, removed.
    let db = TypeInterner::new();
    let obj = db.object(vec![]);
    let union = db.union(vec![TypeId::UNDEFINED, obj]);
    assert!(is_valid_spread_type(&db, union));
}

#[test]
fn spread_union_entirely_falsy_is_invalid() {
    // `false | null | undefined` — all definitely falsy, nothing remains.
    let db = TypeInterner::new();
    let union = db.union(vec![TypeId::BOOLEAN_FALSE, TypeId::NULL, TypeId::UNDEFINED]);
    assert!(!is_valid_spread_type(&db, union));
}

#[test]
fn spread_union_with_string_primitive_is_invalid() {
    // `string | null` — null is falsy but string is not falsy, and string is not spreadable.
    let db = TypeInterner::new();
    let union = db.union(vec![TypeId::STRING, TypeId::NULL]);
    assert!(!is_valid_spread_type(&db, union));
}

#[test]
fn spread_union_with_zero_literal_and_object_is_valid() {
    // `0 | { x: number }` — 0 is definitely falsy, removed.
    let db = TypeInterner::new();
    let zero = db.literal_number(0.0);
    let obj = db.object(vec![]);
    let union = db.union(vec![zero, obj]);
    assert!(is_valid_spread_type(&db, union));
}

#[test]
fn spread_union_with_empty_string_and_object_is_valid() {
    // `"" | { x: number }` — "" is definitely falsy, removed.
    let db = TypeInterner::new();
    let empty = db.literal_string("");
    let obj = db.object(vec![]);
    let union = db.union(vec![empty, obj]);
    assert!(is_valid_spread_type(&db, union));
}

#[test]
fn spread_union_with_nonempty_string_literal_is_invalid() {
    // `"hello" | { x: number }` — "hello" is NOT definitely falsy, and is not spreadable.
    let db = TypeInterner::new();
    let hello = db.literal_string("hello");
    let obj = db.object(vec![]);
    let union = db.union(vec![hello, obj]);
    assert!(!is_valid_spread_type(&db, union));
}

// =============================================================================
// Type parameter constraint resolution
// =============================================================================

#[test]
fn spread_unconstrained_type_param_is_valid() {
    // `T` with no constraint — tsc treats unconstrained type params as valid.
    let db = TypeInterner::new();
    let tp = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let tp_id = db.type_param(tp);
    assert!(is_valid_spread_type(&db, tp_id));
}

#[test]
fn spread_type_param_constrained_to_object_is_valid() {
    // `T extends { x: number }` — constraint is an object, so valid.
    let db = TypeInterner::new();
    let obj = db.object(vec![]);
    let tp = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: Some(obj),
        default: None,
        is_const: false,
    };
    let tp_id = db.type_param(tp);
    assert!(is_valid_spread_type(&db, tp_id));
}

#[test]
fn spread_type_param_constrained_to_string_is_invalid() {
    // `T extends string` — constraint is a primitive, so not valid.
    let db = TypeInterner::new();
    let tp = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    };
    let tp_id = db.type_param(tp);
    assert!(!is_valid_spread_type(&db, tp_id));
}
