use super::*;
use tsz_solver::{
    FunctionShape, MappedType, ParamInfo, TupleElement, TypeId, TypeInterner, TypeParamInfo,
};

#[test]
fn exposes_type_computation_access_boundary_queries() {
    let types = TypeInterner::new();

    let tuple = types.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let string_literal = tsz_solver::type_queries::create_string_literal_type(&types, "x");
    let number_literal = tsz_solver::type_queries::create_number_literal_type(&types, 1.0);
    let object = types.object(vec![]);

    assert_eq!(
        tuple_elements(&types, tuple).map(|elements| elements.len()),
        Some(2)
    );
    assert_eq!(
        literal_property_name(&types, string_literal).map(|atom| types.resolve_atom(atom)),
        Some("x".to_string())
    );
    assert_eq!(
        literal_property_name(&types, number_literal).map(|atom| types.resolve_atom(atom)),
        Some("1".to_string())
    );
    assert!(is_valid_spread_type(&types, object));
    assert!(!is_valid_spread_type(&types, TypeId::NUMBER));
}

#[test]
fn generic_mapped_index_substitution_delegates_to_solver_index_access() {
    let types = TypeInterner::new();
    let mapped_key = types.intern_string("Key");
    let member_key = types.intern_string("Member");
    let valid_key = tsz_solver::type_queries::create_string_literal_type(&types, "valid");
    let missing_key = tsz_solver::type_queries::create_string_literal_type(&types, "missing");

    let mapped_type_param = TypeParamInfo {
        name: mapped_key,
        constraint: Some(valid_key),
        default: None,
        is_const: false,
    };
    let member_type_param = TypeParamInfo {
        name: member_key,
        constraint: Some(missing_key),
        default: None,
        is_const: false,
    };
    let member_type = types.type_param(member_type_param);
    let callable_template = types.function(FunctionShape::new(
        vec![ParamInfo::unnamed(TypeId::NUMBER)],
        TypeId::VOID,
    ));
    let mapped_type = types.mapped(MappedType {
        type_param: mapped_type_param,
        constraint: valid_key,
        name_type: None,
        template: callable_template,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let substitution =
        generic_index_access_substitution(&types, mapped_type, mapped_type, member_type, |ty| ty)
            .expect("expected generic mapped index substitution request");
    let evaluated = tsz_solver::computation::evaluate_type(&types, substitution.type_to_evaluate);

    assert_eq!(
        substitution.type_to_evaluate, substitution.index_access,
        "boundary must delegate generic mapped index evaluation to the solver index-access path"
    );
    assert_ne!(
        evaluated, callable_template,
        "solver key-space gate must reject out-of-key-space generic indexes before template substitution"
    );
}
