use tsz_solver::{CallableShape, TypeDatabase, TypeId};

pub(crate) fn callable_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<CallableShape>> {
    tsz_solver::type_queries::get_callable_shape(db, type_id)
}

pub(crate) fn call_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<tsz_solver::CallSignature>> {
    tsz_solver::type_queries::get_call_signatures(db, type_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::{CallSignature, CallableShape, FunctionShape, TypeInterner};

    #[test]
    fn extracts_callable_shape_and_call_signatures() {
        let types = TypeInterner::new();
        let callable = types.callable(CallableShape {
            call_signatures: vec![
                CallSignature::new(vec![], TypeId::STRING),
                CallSignature::new(vec![], TypeId::NUMBER),
            ],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
        });
        let function = types.function(FunctionShape::new(vec![], TypeId::BOOLEAN));

        let callable_shape = callable_shape_for_type(&types, callable).expect("callable shape");
        assert_eq!(callable_shape.call_signatures.len(), 2);

        let call_sigs = call_signatures_for_type(&types, callable).expect("call signatures");
        assert_eq!(call_sigs.len(), 2);
        assert_eq!(call_sigs[0].return_type, TypeId::STRING);
        assert_eq!(call_sigs[1].return_type, TypeId::NUMBER);

        assert!(
            call_signatures_for_type(&types, function).is_none(),
            "function types are not exposed as call-signature lists by this query"
        );
    }
}
