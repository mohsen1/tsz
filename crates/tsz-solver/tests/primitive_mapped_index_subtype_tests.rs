//! Relation-path coverage for a primitive source against a deferred mapped type
//! that is structurally a pure index-signature object.
//!
//! Structural rule: `{ [P in any]: V }` (the shape of `Record<any, V>` /
//! `Record<PropertyKey, V>`) is equivalent to
//! `{ [k: string]: V; [k: number]: V }` — a *pure* index-signature object with
//! no named properties. A primitive (`string`, `number`, `boolean`, …) has no
//! index signature, so tsc rejects `primitive <: { [P in any]: V }` even though
//! the primitive's boxed wrapper would structurally satisfy the index signature.
//!
//! Such an `any`-constrained mapped type is intentionally NOT expanded by the
//! eager evaluator (to keep error-message display stable), so it reaches the
//! subtype checker as a raw `Mapped` node. The apparent-primitive dispatch must
//! expand it (`try_expand_mapped`) so the pure-index guard owns the relation
//! instead of falling through to the boxed-wrapper fallback. Regression guard
//! for the zod false-positive TS2339 in issue #14220.

use super::*;
use crate::computation::SubtypeChecker;
use crate::construction::TypeInterner;
use crate::types::{MappedType, PropertyInfo, TypeParamInfo};

/// Build a raw mapped type `{ [iter in any]: template }` without going through
/// the eager evaluator, so the subtype checker sees the deferred `Mapped` node —
/// exactly the form that reaches relations for `Record<any, V>`.
fn any_constrained_mapped(interner: &TypeInterner, template: TypeId) -> TypeId {
    let iter_param = TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    interner.mapped(MappedType {
        type_param: iter_param,
        constraint: TypeId::ANY,
        name_type: None,
        template,
        optional_modifier: None,
        readonly_modifier: None,
    })
}

#[test]
fn string_is_not_subtype_of_record_any_any() {
    let interner = TypeInterner::new();
    let mapped = any_constrained_mapped(&interner, TypeId::ANY);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(TypeId::STRING, mapped),
        "primitive `string` must not satisfy a pure index-signature `{{ [P in any]: any }}`"
    );
}

#[test]
fn number_and_boolean_are_not_subtype_of_record_any_any() {
    let interner = TypeInterner::new();
    let mapped = any_constrained_mapped(&interner, TypeId::ANY);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(TypeId::NUMBER, mapped),
        "primitive `number` must not satisfy `{{ [P in any]: any }}`"
    );
    assert!(
        !checker.is_subtype_of(TypeId::BOOLEAN, mapped),
        "primitive `boolean` must not satisfy `{{ [P in any]: any }}`"
    );
}

#[test]
fn string_literal_is_not_subtype_of_record_any_any() {
    let interner = TypeInterner::new();
    let mapped = any_constrained_mapped(&interner, TypeId::ANY);
    let lit = interner.literal_string("x");
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(lit, mapped),
        "a string literal must not satisfy `{{ [P in any]: any }}`"
    );
}

#[test]
fn object_is_still_subtype_of_record_any_any() {
    let interner = TypeInterner::new();
    let mapped = any_constrained_mapped(&interner, TypeId::ANY);
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(obj, mapped),
        "an object literal must still satisfy `Record<any, any>` (positive control)"
    );
}

#[test]
fn empty_object_is_still_subtype_of_record_any_any() {
    let interner = TypeInterner::new();
    let mapped = any_constrained_mapped(&interner, TypeId::ANY);
    let empty = interner.object(vec![]);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(empty, mapped),
        "an empty object must still satisfy `Record<any, any>` (positive control)"
    );
}

#[test]
fn string_is_not_subtype_of_record_any_string_value() {
    // Narrow the value type: `{ [P in any]: string }`. A primitive must still be
    // rejected — the relation hinges on the source being a primitive, not on the
    // index value type.
    let interner = TypeInterner::new();
    let mapped = any_constrained_mapped(&interner, TypeId::STRING);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(TypeId::STRING, mapped),
        "primitive `string` must not satisfy `{{ [P in any]: string }}`"
    );
}

// The `object` keyword (and a type parameter constrained `extends object`) has no
// index signature of its own, but tsc's `any`-index waiver in
// `indexSignaturesRelatedTo` accepts it against an all-`any` index signature —
// exactly the shape that `Record<any, any>` / `{ [P in any]: any }` reduces to.
// The keyword reaches relations as a deferred `Mapped` node (same as the
// primitive cases above), so the `object`-keyword arm must expand it before
// inspecting the structural shape. Regression guard for the tRPC false-positive
// TS2322/TS2344 in issue #14751 (the source-direction mirror of #14220).

#[test]
fn object_keyword_is_subtype_of_record_any_any() {
    let interner = TypeInterner::new();
    let mapped = any_constrained_mapped(&interner, TypeId::ANY);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(TypeId::OBJECT, mapped),
        "the `object` keyword must satisfy `Record<any, any>` (`{{ [P in any]: any }}`), \
         matching tsc's any-index waiver — the same accept path `{{}}` already takes"
    );
}

#[test]
fn object_keyword_is_not_subtype_of_record_any_unknown_value() {
    // A concrete `unknown` index value is NOT waived: `object` has no index
    // signature, so tsc rejects `object <: { [P in any]: unknown }`. Only the
    // all-`any` index is relaxed.
    let interner = TypeInterner::new();
    let mapped = any_constrained_mapped(&interner, TypeId::UNKNOWN);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(TypeId::OBJECT, mapped),
        "the `object` keyword must not satisfy `{{ [P in any]: unknown }}` (concrete value rejects)"
    );
}

#[test]
fn object_keyword_is_not_subtype_of_record_any_string_value() {
    // A narrowed `string` index value is likewise not waived.
    let interner = TypeInterner::new();
    let mapped = any_constrained_mapped(&interner, TypeId::STRING);
    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(TypeId::OBJECT, mapped),
        "the `object` keyword must not satisfy `{{ [P in any]: string }}` (concrete value rejects)"
    );
}
