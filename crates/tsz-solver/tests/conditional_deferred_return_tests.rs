//! Regression tests for deferred conditional evaluation through the public solver API.

use tsz_solver::computation::evaluate_conditional;
use tsz_solver::construction::TypeInterner;
use tsz_solver::query::conditional_type_id;
use tsz_solver::type_handles::{ConditionalType, PropertyInfo, TypeId, TypeParamInfo};

fn type_param(interner: &TypeInterner, name: &str) -> TypeId {
    interner.type_param(TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
        origin: tsz_solver::TypeParamOrigin::User,
    })
}

fn infer_param(interner: &TypeInterner, name: &str) -> TypeId {
    interner.infer(TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
        origin: tsz_solver::TypeParamOrigin::User,
    })
}

#[test]
fn infer_conditional_over_unresolved_type_parameter_stays_deferred() {
    for (check_name, infer_name) in [("Input", "Inner"), ("Subject", "Value")] {
        let interner = TypeInterner::new();
        let check_type = type_param(&interner, check_name);
        let inferred = infer_param(&interner, infer_name);
        let value_name = interner.intern_string("val");
        let extends_type = interner.object(vec![PropertyInfo::new(value_name, inferred)]);

        let result = evaluate_conditional(
            &interner,
            &ConditionalType {
                check_type,
                extends_type,
                true_type: inferred,
                false_type: TypeId::STRING,
                is_distributive: false,
            },
        );

        assert_ne!(
            result,
            TypeId::STRING,
            "unresolved {check_name} must not collapse to the false branch"
        );
        assert!(
            conditional_type_id(&interner, result).is_some(),
            "unresolved {check_name} should stay as a deferred conditional, got {:?}",
            interner.lookup(result)
        );
    }
}

#[test]
fn infer_conditional_over_unresolved_application_pattern_stays_deferred() {
    let interner = TypeInterner::new();
    let check_type = type_param(&interner, "Input");
    let inferred = infer_param(&interner, "Element");
    let unresolved_array = interner.unresolved_type_name(interner.intern_string("Array"));
    let extends_type = interner.application(unresolved_array, vec![inferred]);

    let result = evaluate_conditional(
        &interner,
        &ConditionalType {
            check_type,
            extends_type,
            true_type: inferred,
            false_type: TypeId::STRING,
            is_distributive: false,
        },
    );

    assert_ne!(
        result,
        TypeId::STRING,
        "unresolved application patterns must not collapse to the false branch"
    );
    assert!(
        conditional_type_id(&interner, result).is_some(),
        "unresolved application pattern should stay deferred, got {:?}",
        interner.lookup(result)
    );
}

/// A distributive conditional whose check type is a *concrete* object and whose
/// extends type is an unresolved cross-module reference (a bare
/// `UnresolvedTypeName`, the pre-`DefId` form of an imported alias such as
/// `AnyRecord`) must defer — not collapse to the false branch.
///
/// This is the `DeepPick<…, { k: never }>` family (issue #13618): the library's
/// `Filter extends AnyRecord ? { …mapped… } : never` reports `Filter <: AnyRecord`
/// as a definitive failure while `AnyRecord` is still an unresolved name, so the
/// whole pick collapsed to `never` and a valid value was rejected with a false
/// `TS2322 … not assignable to 'never'`. Folding an `UnresolvedTypeName` into the
/// error/false branch is schedule-dependent; the conditional must stay deferred
/// until a later resolver generation binds the reference.
#[test]
fn distributive_conditional_concrete_check_over_unresolved_extends_defers() {
    for (extends_name, true_marker) in [("AnyRecord", TypeId::NUMBER), ("Marker", TypeId::BOOLEAN)]
    {
        let interner = TypeInterner::new();
        let prop = interner.intern_string("id");
        let check_type = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);
        let extends_type = interner.unresolved_type_name(interner.intern_string(extends_name));

        let result = evaluate_conditional(
            &interner,
            &ConditionalType {
                check_type,
                extends_type,
                true_type: true_marker,
                false_type: TypeId::NEVER,
                is_distributive: true,
            },
        );

        assert_ne!(
            result,
            TypeId::NEVER,
            "unresolved extends '{extends_name}' must not collapse the conditional to the \
             false (never) branch"
        );
        assert!(
            conditional_type_id(&interner, result).is_some(),
            "unresolved extends '{extends_name}' should stay deferred, got {:?}",
            interner.lookup(result)
        );
    }
}

/// The symmetric case: a concrete check type whose extends is an unresolved
/// reference must not eagerly take the *true* branch either. The relation
/// machinery treats an `UnresolvedTypeName` as related to everything
/// (error/`any`-like), so `T extends Builtin ? T : …` over a still-unresolved
/// imported `Builtin` would otherwise report `T <: Builtin` as true and collapse
/// to `T` (the second face of #13618, surfacing as a false `TS2741`).
#[test]
fn conditional_concrete_check_over_unresolved_extends_does_not_take_true_branch() {
    for extends_name in ["Builtin", "Primitive"] {
        let interner = TypeInterner::new();
        let prop = interner.intern_string("id");
        let check_type = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);
        let extends_type = interner.unresolved_type_name(interner.intern_string(extends_name));

        let result = evaluate_conditional(
            &interner,
            &ConditionalType {
                check_type,
                extends_type,
                true_type: check_type,
                false_type: TypeId::NEVER,
                is_distributive: false,
            },
        );

        assert_ne!(
            result, check_type,
            "unresolved extends '{extends_name}' must not eagerly take the true branch"
        );
        assert!(
            conditional_type_id(&interner, result).is_some(),
            "unresolved extends '{extends_name}' should stay deferred, got {:?}",
            interner.lookup(result)
        );
    }
}
