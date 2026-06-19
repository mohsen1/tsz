//! Tests for the failure-explanation work budget (issue #13243).
//!
//! The boolean relation walk short-circuits at the first failing branch, but
//! the explain pass drills into every property / signature / branch the walk
//! never visited, each driving a fresh `evaluate_type` → instantiation. On
//! deeply-generic, diagnostic-heavy relations that breadth is combinatorial and
//! does not terminate. `SubtypeChecker` caps the number of distinct (cache-miss)
//! evaluations performed while elaborating one failure; once the budget is
//! exhausted the remaining elaboration collapses to a coarse `TypeMismatch`,
//! without changing the relation verdict.
//!
//! These tests assert the two ends of that contract:
//! 1. With the default (generous) budget the explain pass produces its full
//!    detailed reason — the budget is inert on terminating workloads.
//! 2. With the budget forced to zero the very first elaboration collapses to
//!    the coarse verdict — the bound is real and load-bearing.
//!
//! Property names are varied between cases so the behavior cannot depend on any
//! particular binder string.

use crate::PropertyInfo;
use crate::diagnostics::SubtypeFailureReason;
use crate::intern::TypeInterner;
use crate::relations::subtype::SubtypeChecker;
use crate::types::TypeId;

/// `{ <keep>: string }` is not assignable to `{ <keep>: string; <missing>: number }`
/// because `<missing>` is absent from the source. Returns `(source, target,
/// missing_name_atom)`.
fn missing_property_pair(
    db: &TypeInterner,
    keep: &str,
    missing: &str,
) -> (TypeId, TypeId, tsz_common::interner::Atom) {
    let keep_atom = db.intern_string(keep);
    let missing_atom = db.intern_string(missing);

    let source = db.object(vec![PropertyInfo::new(keep_atom, TypeId::STRING)]);
    let target = db.object(vec![
        PropertyInfo::new(keep_atom, TypeId::STRING),
        PropertyInfo::new(missing_atom, TypeId::NUMBER),
    ]);

    (source, target, missing_atom)
}

/// Names the failing property a reason blames, if it carries one. Used to prove
/// the detailed elaboration survived (it identifies `<missing>` by name) while
/// the collapsed elaboration does not.
fn blamed_property(reason: &SubtypeFailureReason) -> Option<tsz_common::interner::Atom> {
    match reason {
        SubtypeFailureReason::MissingProperty { property_name, .. } => Some(*property_name),
        SubtypeFailureReason::MissingProperties { property_names, .. } => {
            property_names.first().copied()
        }
        _ => None,
    }
}

#[test]
fn default_budget_yields_detailed_missing_property_reason() {
    let interner = TypeInterner::new();
    let (source, target, missing) = missing_property_pair(&interner, "alpha", "beta");

    let mut checker = SubtypeChecker::new(&interner);
    let reason = checker
        .explain_failure(source, target)
        .expect("incompatible object types must produce a failure reason");

    assert_eq!(
        blamed_property(&reason),
        Some(missing),
        "default budget must keep the detailed missing-property elaboration, got {reason:?}"
    );
}

#[test]
fn exhausted_budget_collapses_to_coarse_type_mismatch() {
    let interner = TypeInterner::new();
    let (source, target, _missing) = missing_property_pair(&interner, "alpha", "beta");

    // Budget of zero: the first elaboration node has no fuel and must report the
    // bare verdict instead of drilling into the missing property.
    let mut checker = SubtypeChecker::new(&interner).with_explain_budget(0);
    let reason = checker
        .explain_failure(source, target)
        .expect("a failing relation must still produce a reason under an exhausted budget");

    assert!(
        matches!(
            reason,
            SubtypeFailureReason::TypeMismatch {
                source_type,
                target_type,
            } if source_type == source && target_type == target
        ),
        "exhausted budget must collapse to the coarse TypeMismatch verdict, got {reason:?}"
    );
}

#[test]
fn budget_behavior_is_independent_of_property_names() {
    // Same structural shape, different binder names: the detailed-vs-collapsed
    // contract must not depend on the identifiers chosen.
    let interner = TypeInterner::new();
    let (source, target, missing) = missing_property_pair(&interner, "first_key", "second_key");

    let mut detailed = SubtypeChecker::new(&interner);
    assert_eq!(
        blamed_property(
            &detailed
                .explain_failure(source, target)
                .expect("must produce a reason")
        ),
        Some(missing),
    );

    let mut collapsed = SubtypeChecker::new(&interner).with_explain_budget(0);
    let reason = collapsed
        .explain_failure(source, target)
        .expect("must produce a reason");
    assert!(
        matches!(reason, SubtypeFailureReason::TypeMismatch { .. }),
        "got {reason:?}"
    );
}
