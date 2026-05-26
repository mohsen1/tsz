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
    })
}

fn infer_param(interner: &TypeInterner, name: &str) -> TypeId {
    interner.infer(TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
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
