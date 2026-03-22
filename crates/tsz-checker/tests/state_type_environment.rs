use super::*;
use tsz_solver::{DefId, TypeInterner};

#[test]
fn classifies_and_extracts_environment_resolution_shapes() {
    let types = TypeInterner::new();

    let lazy = types.lazy(DefId(42));
    let app = types.application(TypeId::STRING, vec![TypeId::NUMBER]);
    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let keyof_string = types.keyof(TypeId::STRING);

    assert!(matches!(
        lazy_def_id(&types, lazy),
        Some(def_id) if def_id == DefId(42)
    ));
    assert!(matches!(
        classify_for_type_resolution(&types, lazy),
        TypeResolutionKind::Lazy(def_id) if def_id == DefId(42)
    ));
    assert!(matches!(
        classify_for_property_access_resolution(&types, app),
        PropertyAccessResolutionKind::Application(_)
    ));
    assert_eq!(
        application_info(&types, app),
        Some((TypeId::STRING, vec![TypeId::NUMBER]))
    );
    assert_eq!(
        tsz_solver::type_queries::get_union_members(&types, union),
        Some(vec![TypeId::STRING, TypeId::NUMBER])
    );
    assert_eq!(
        tsz_solver::type_queries::get_intersection_members(&types, TypeId::STRING),
        None
    );
    assert!(matches!(
        classify_mapped_constraint(&types, keyof_string),
        MappedConstraintKind::KeyOf(inner) if inner == TypeId::STRING
    ));
}

#[test]
fn mapped_source_classification_via_boundary() {
    let types = TypeInterner::new();

    // Array source
    let arr = types.array(TypeId::NUMBER);
    assert!(matches!(
        classify_mapped_source(&types, arr),
        MappedSourceKind::Array(elem) if elem == TypeId::NUMBER
    ));

    // Object source
    let obj = types.object(vec![]);
    assert!(matches!(
        classify_mapped_source(&types, obj),
        MappedSourceKind::Object
    ));

    // Tuple source
    let tup = types.tuple(vec![tsz_solver::TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert!(matches!(
        classify_mapped_source(&types, tup),
        MappedSourceKind::Tuple(_)
    ));
}

#[test]
fn mapped_modifier_computation_via_boundary() {
    let types = TypeInterner::new();

    let mapped = tsz_solver::MappedType {
        type_param: tsz_solver::TypeParamInfo {
            name: types.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: Some(tsz_solver::MappedModifier::Remove),
        readonly_modifier: Some(tsz_solver::MappedModifier::Add),
    };

    let (opt, ro) = compute_mapped_modifiers(&mapped, true, true, false);
    assert!(!opt, "-? should remove optional");
    assert!(ro, "+readonly should add readonly");
}

#[test]
fn identity_name_mapping_detection_via_boundary() {
    let types = TypeInterner::new();
    let k_name = types.intern_string("K");

    // No name_type — identity
    let mapped = tsz_solver::MappedType {
        type_param: tsz_solver::TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };
    assert!(is_identity_name_mapping(&types, &mapped));

    // name_type = K — identity
    let k_param = types.type_param(tsz_solver::TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
        is_const: false,
    });
    let mapped_with_as_k = tsz_solver::MappedType {
        type_param: tsz_solver::TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        name_type: Some(k_param),
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };
    assert!(is_identity_name_mapping(&types, &mapped_with_as_k));
}

#[test]
fn solver_evaluator_handles_mapped_type_with_resolver() {
    // Verify that the solver's TypeEvaluator can evaluate mapped types when
    // given a resolver. This validates the architectural path where mapped types
    // flow through evaluate_type_with_env (solver) rather than the checker-side
    // evaluate_mapped_type_with_resolution.
    use tsz_solver::{MappedType, TypeEnvironment, TypeEvaluator, TypeParamInfo};

    let types = TypeInterner::new();
    let k_name = types.intern_string("K");
    let a_name = types.intern_string("a");
    let b_name = types.intern_string("b");

    // Build mapped type: { [K in "a" | "b"]: number }
    let constraint = types.union(vec![
        types.literal_string_atom(a_name),
        types.literal_string_atom(b_name),
    ]);
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::NUMBER,
        optional_modifier: None,
        readonly_modifier: None,
    };
    let mapped_type = types.mapped(mapped);

    // Evaluate using the solver's TypeEvaluator with an empty TypeEnvironment.
    // This should produce an object with properties { a: number; b: number }.
    let env = TypeEnvironment::new();
    let mut evaluator = TypeEvaluator::with_resolver(&types, &env);
    let result = evaluator.evaluate(mapped_type);

    // The result should be an Object, not the original Mapped type.
    assert_ne!(
        result, mapped_type,
        "Mapped type should be evaluated to an Object"
    );
    // Should be an object — use solver query API to check
    assert!(
        tsz_solver::type_queries::is_object_type(&types, result),
        "Mapped type should evaluate to an Object"
    );
    // Check properties via PropertyAccessEvaluator
    let evaluator = tsz_solver::operations::property::PropertyAccessEvaluator::new(&types);
    let a_type = evaluator
        .resolve_property_access(result, "a")
        .success_type();
    let b_type = evaluator
        .resolve_property_access(result, "b")
        .success_type();
    assert_eq!(
        a_type,
        Some(TypeId::NUMBER),
        "Property 'a' should be number"
    );
    assert_eq!(
        b_type,
        Some(TypeId::NUMBER),
        "Property 'b' should be number"
    );
}

#[test]
fn non_homomorphic_mapped_type_delegates_to_solver_after_constraint_resolution() {
    // When a non-homomorphic mapped type (constraint is NOT `keyof T`) has its
    // constraint resolved from a Lazy ref to concrete keys, the checker should
    // delegate to the solver's evaluator rather than doing manual property
    // expansion. This test validates the architectural improvement where
    // resolved non-homomorphic mapped types are evaluated via
    // evaluate_type_with_env instead of the checker's inline expansion loop.
    use tsz_solver::{MappedType, TypeEnvironment, TypeEvaluator, TypeParamInfo};

    let types = TypeInterner::new();
    let p_name = types.intern_string("P");
    let x_name = types.intern_string("x");
    let y_name = types.intern_string("y");

    // Simulate a non-homomorphic mapped type: { [P in "x" | "y"]: string }
    // This would arise from resolving `{ [P in Keys]: string }` where
    // `type Keys = "x" | "y"` has been resolved from a Lazy ref.
    let constraint = types.union(vec![
        types.literal_string_atom(x_name),
        types.literal_string_atom(y_name),
    ]);
    let mapped = MappedType {
        type_param: TypeParamInfo {
            name: p_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint,
        name_type: None,
        template: TypeId::STRING,
        optional_modifier: None,
        readonly_modifier: None,
    };
    let mapped_type = types.mapped(mapped);

    // The solver should handle this directly (no checker fallback needed).
    let env = TypeEnvironment::new();
    let mut evaluator = TypeEvaluator::with_resolver(&types, &env);
    let result = evaluator.evaluate(mapped_type);

    assert_ne!(
        result, mapped_type,
        "Non-homomorphic mapped type with resolved constraint should be evaluated"
    );
    assert!(
        tsz_solver::type_queries::is_object_type(&types, result),
        "Should evaluate to an Object type"
    );

    // Verify both properties exist
    let pa = tsz_solver::operations::property::PropertyAccessEvaluator::new(&types);
    assert_eq!(
        pa.resolve_property_access(result, "x").success_type(),
        Some(TypeId::STRING),
    );
    assert_eq!(
        pa.resolve_property_access(result, "y").success_type(),
        Some(TypeId::STRING),
    );
}
