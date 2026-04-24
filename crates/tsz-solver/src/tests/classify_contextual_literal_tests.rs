//! Tests for `classify_for_contextual_literal`.
//!
//! This classifier is used by the checker when deciding whether a literal
//! expression should be preserved (kept as a literal type) or widened against
//! its contextual type. When the contextual type is a deferred conditional
//! type, tsc recurses through the conditional's default constraint. We expose
//! a `Conditional { true_type, false_type }` variant so the checker can check
//! both branches.

use crate::intern::TypeInterner;
use crate::type_queries::extended::{ContextualLiteralAllowKind, classify_for_contextual_literal};
use crate::types::{ConditionalType, TypeData, TypeId, TypeParamInfo};

fn type_param(interner: &TypeInterner, name: &str, constraint: Option<TypeId>) -> TypeId {
    interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string(name),
        constraint,
        default: None,
        is_const: false,
    }))
}

fn string_literal(interner: &TypeInterner, value: &str) -> TypeId {
    interner.literal_string(value)
}

#[test]
fn classify_contextual_literal_plain_union() {
    let interner = TypeInterner::new();
    let a_lit = string_literal(&interner, "a");
    let b_lit = string_literal(&interner, "b");
    let union = interner.union2(a_lit, b_lit);
    assert!(matches!(
        classify_for_contextual_literal(&interner, union),
        ContextualLiteralAllowKind::Members(_)
    ));
}

#[test]
fn classify_contextual_literal_type_parameter() {
    let interner = TypeInterner::new();
    let t = type_param(&interner, "T", None);
    assert!(matches!(
        classify_for_contextual_literal(&interner, t),
        ContextualLiteralAllowKind::TypeParameter { constraint: None }
    ));
}

/// Deferred conditional types must expose their branches so the checker can
/// check whether a literal is allowed by either branch.
///
/// For `Foo<T> = T extends true ? string : "a"`, the expression `"a"` must
/// stay as literal `"a"` (not widen to `string`) when assigned to `Foo<T>`,
/// because the `"a"` branch accepts it.
#[test]
fn classify_contextual_literal_deferred_conditional_exposes_branches() {
    let interner = TypeInterner::new();
    let t = type_param(&interner, "T", None);
    let a_lit = string_literal(&interner, "a");
    // T extends true ? string : "a"
    let cond = interner.conditional(ConditionalType {
        check_type: t,
        extends_type: TypeId::BOOLEAN_TRUE,
        true_type: TypeId::STRING,
        false_type: a_lit,
        is_distributive: true,
    });
    match classify_for_contextual_literal(&interner, cond) {
        ContextualLiteralAllowKind::Conditional {
            true_type,
            false_type,
        } => {
            assert_eq!(true_type, TypeId::STRING);
            assert_eq!(false_type, a_lit);
        }
        other => panic!("expected ContextualLiteralAllowKind::Conditional, got {other:?}"),
    }
}

#[test]
fn classify_contextual_literal_non_deferred_type_returns_not_allowed() {
    let interner = TypeInterner::new();
    assert!(matches!(
        classify_for_contextual_literal(&interner, TypeId::NUMBER),
        ContextualLiteralAllowKind::NotAllowed
    ));
}
