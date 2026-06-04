impl<'a> CheckerState<'a> {
    pub(crate) fn instantiate_callable_result_from_request(
        &mut self,
        idx: NodeIndex,
        result_type: TypeId,
        request: &TypingRequest,
    ) -> TypeId {
        let Some(expected_type) = request.contextual_type else {
            return result_type;
        };
        if matches!(result_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return result_type;
        }

        let result_eval = self.evaluate_type_with_env(result_type);
        let has_generic_signature =
            crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                result_type,
            )
            .or_else(|| {
                crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    result_eval,
                )
            })
            .is_some_and(|shape| !shape.type_params.is_empty());
        if !has_generic_signature {
            return result_type;
        }

        if self.is_immediate_call_or_new_callee(idx) || !self.is_immediate_call_or_new_argument(idx)
        {
            return result_type;
        }

        let expected_type = self.contextual_type_option_for_expression(Some(expected_type));
        let Some(expected_type) = expected_type else {
            return result_type;
        };

        let instantiated =
            if self.target_has_concrete_return_context_for_generic_refinement(expected_type) {
                self.instantiate_generic_function_argument_against_target_for_refinement(
                    result_type,
                    expected_type,
                )
            } else {
                self.instantiate_generic_function_argument_against_target_params(
                    result_type,
                    expected_type,
                )
            };

        if instantiated == TypeId::ERROR {
            result_type
        } else {
            instantiated
        }
    }
}
