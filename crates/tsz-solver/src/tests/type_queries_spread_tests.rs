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
use crate::objects::ObjectLiteralBuilder;
use crate::type_queries::is_valid_spread_type;
use crate::types::{PropertyInfo, StringIntrinsicKind, TemplateSpan, TypeParamInfo, Visibility};

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

// =============================================================================
// Template literal types and string intrinsics (not spreadable)
// =============================================================================

#[test]
fn spread_template_literal_is_invalid() {
    // `\`${number}\`` is a string subtype — not spreadable (TS2698).
    let db = TypeInterner::new();
    let tpl = db.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);
    assert!(!is_valid_spread_type(&db, tpl));
}

#[test]
fn spread_template_literal_with_text_is_invalid() {
    // `\`prefix_${string}\`` — still a template literal, not spreadable.
    let db = TypeInterner::new();
    let prefix = db.intern_string("prefix_");
    let tpl = db.template_literal(vec![
        TemplateSpan::Text(prefix),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    assert!(!is_valid_spread_type(&db, tpl));
}

#[test]
fn spread_string_intrinsic_uppercase_is_invalid() {
    // `Uppercase<string>` is a string intrinsic — not spreadable.
    let db = TypeInterner::new();
    let upper = db.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);
    assert!(!is_valid_spread_type(&db, upper));
}

#[test]
fn spread_string_intrinsic_lowercase_is_invalid() {
    // `Lowercase<string>` — not spreadable.
    let db = TypeInterner::new();
    let lower = db.string_intrinsic(StringIntrinsicKind::Lowercase, TypeId::STRING);
    assert!(!is_valid_spread_type(&db, lower));
}

#[test]
fn spread_properties_skip_non_public_and_prototype_members() {
    let db = TypeInterner::new();
    let obj = db.object(vec![
        PropertyInfo {
            name: db.intern_string("visible"),
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
        },
        PropertyInfo {
            name: db.intern_string("#hidden"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Private,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
        PropertyInfo {
            name: db.intern_string("method"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: true,
            is_class_prototype: true,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        },
    ]);

    let props = ObjectLiteralBuilder::new(&db).collect_spread_properties(obj);
    assert_eq!(
        props.len(),
        1,
        "expected only public own properties in spread"
    );
    assert_eq!(db.resolve_atom_ref(props[0].name).as_ref(), "visible");
    assert!(
        !props[0].readonly,
        "spread properties should be mutable copies"
    );
}

#[test]
fn spread_union_template_literal_with_object_is_invalid() {
    // `\`${number}\` | { x: number }` — template literal is NOT falsy,
    // so it stays in the union and makes spread invalid.
    let db = TypeInterner::new();
    let tpl = db.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);
    let obj = db.object(vec![]);
    let union = db.union(vec![tpl, obj]);
    assert!(!is_valid_spread_type(&db, union));
}

// =============================================================================
// Intersection with falsy types
// =============================================================================

#[test]
fn spread_union_with_intersection_containing_undefined_is_valid() {
    // `T | T & undefined` — the intersection `T & undefined` is definitely falsy
    // (any value in `T & undefined` must be undefined), so it gets filtered out.
    // Remaining: `T` (unconstrained type param) → valid spread type.
    let db = TypeInterner::new();
    let tp = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let tp_id = db.type_param(tp);
    let intersection = db.intersection(vec![tp_id, TypeId::UNDEFINED]);
    let union = db.union(vec![tp_id, intersection]);
    assert!(is_valid_spread_type(&db, union));
}

#[test]
fn spread_intersection_with_undefined_is_invalid_on_its_own() {
    // `T & undefined` alone is not a valid spread type (it's always undefined).
    let db = TypeInterner::new();
    let tp = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let tp_id = db.type_param(tp);
    let intersection = db.intersection(vec![tp_id, TypeId::UNDEFINED]);
    assert!(!is_valid_spread_type(&db, intersection));
}
