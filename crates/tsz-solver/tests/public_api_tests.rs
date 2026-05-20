use tsz_solver::construction::TypeInterner;
use tsz_solver::{
    InstantiationOptions, InstantiationRequest, InstantiationResult, TypeId, TypeSubstitution,
    instantiate_type_with_request,
};

#[test]
fn root_exports_staged_instantiation_api() {
    let interner = TypeInterner::new();
    let substitution = TypeSubstitution::new();
    let options = InstantiationOptions::new().with_preserve_meta_types(true);
    let request = InstantiationRequest::new(TypeId::STRING, &substitution).with_options(options);

    let result: InstantiationResult = instantiate_type_with_request(&interner, request);

    assert!(!result.depth_exceeded());
    assert_eq!(result.into_type_id(), TypeId::STRING);
}
