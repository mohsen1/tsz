use tsz_solver::computation::{
    InstantiationOptions, InstantiationRequest, InstantiationResult, TypeSubstitution,
    instantiate_type_with_request,
};
use tsz_solver::construction::TypeInterner;
use tsz_solver::type_handles::TypeId;

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
