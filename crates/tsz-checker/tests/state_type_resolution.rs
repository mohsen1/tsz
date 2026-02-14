use super::*;
use tsz_solver::{CallSignature, CallableShape, FunctionShape, TypeInterner, TypeParamInfo};

#[test]
fn classifies_resolution_and_signature_paths() {
    let types = TypeInterner::new();

    let callable = types.callable(CallableShape {
        call_signatures: vec![CallSignature::new(vec![], TypeId::NUMBER)],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
    });
    let function = types.function(FunctionShape::new(vec![], TypeId::STRING));
    let app = types.application(TypeId::STRING, vec![TypeId::NUMBER]);
    let type_param = types.type_param(TypeParamInfo {
        name: types.intern_string("T"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });

    assert!(callable_shape_for_type(&types, callable).is_some());
    assert!(matches!(
        classify_for_signatures(&types, callable),
        SignatureTypeKind::Callable(_)
    ));
    assert!(matches!(
        classify_for_signatures(&types, function),
        SignatureTypeKind::Function(_)
    ));
    assert!(matches!(
        classify_constructor_type(&types, function),
        ConstructorTypeKind::Function(_)
    ));
    assert_eq!(
        get_application_info(&types, app),
        Some((TypeId::STRING, vec![TypeId::NUMBER]))
    );
    assert!(is_type_parameter(&types, type_param));
}
