use tsz_solver::TypeId;
use tsz_solver::computation::{
    EvaluationResult, InstantiationOptions, InstantiationRequest, InstantiationResult, Termination,
    TypeSubstitution, evaluate_type_result_with_request, evaluate_type_with_request,
    instantiate_type_with_request,
};
use tsz_solver::construction::TypeInterner;
use tsz_solver::evaluation::request::EvaluationRequest;

#[test]
fn computation_exports_staged_instantiation_api() {
    let interner = TypeInterner::new();
    let substitution = TypeSubstitution::new();
    let options = InstantiationOptions::new().with_preserve_meta_types(true);
    let request = InstantiationRequest::new(TypeId::STRING, &substitution).with_options(options);

    let result: InstantiationResult = instantiate_type_with_request(&interner, request);

    assert!(!result.depth_exceeded());
    assert_eq!(result.into_type_id(), TypeId::STRING);
}

#[test]
fn computation_exports_staged_evaluation_api() {
    let interner = TypeInterner::new();
    let request = EvaluationRequest::new(TypeId::STRING);

    let result: EvaluationResult = evaluate_type_result_with_request(&interner, request);

    assert_eq!(result.type_id(), TypeId::STRING);
    assert_eq!(result.termination(), Termination::Complete);
    assert_eq!(result.into_type_id(), TypeId::STRING);
    assert_eq!(
        evaluate_type_with_request(&interner, request),
        TypeId::STRING,
        "the legacy TypeId-returning helper must keep the same collapsed value"
    );
}
