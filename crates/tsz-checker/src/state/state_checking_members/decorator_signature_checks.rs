//! Class-member decorator signature validation helpers.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// TS1240 for ES field decorators: the runtime invokes field decorators as
    /// `decorator(undefined, context)`. Resolve the decorator expression against
    /// that value argument so signatures that require the field value itself are
    /// rejected like tsc.
    pub(crate) fn check_es_property_decorator_call_signature(
        &mut self,
        decorator_node: NodeIndex,
        decorator_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::query_boundaries::common::CallResult;

        if decorator_type == TypeId::ERROR
            || decorator_type == TypeId::ANY
            || decorator_type == TypeId::UNKNOWN
        {
            return;
        }

        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if resolved == TypeId::ERROR || resolved == TypeId::ANY || resolved == TypeId::UNKNOWN {
            return;
        }

        let (result, _, _) = self.resolve_call_with_checker_adapter(
            resolved,
            &[TypeId::UNDEFINED, TypeId::ANY],
            false,
            None,
            None,
        );

        if !matches!(result, CallResult::Success(_)) {
            self.error_at_node(
                decorator_node,
                diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_PROPERTY_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_PROPERTY_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
            );
        }
    }

    /// TS1241 / TS1329 for experimental method and accessor decorators: the runtime invokes
    /// the decorator as `decorator(target, propertyKey, descriptor)` with 3 arguments.
    /// When the resolved signature is incompatible, TS1329 is preferred when all call
    /// signatures have zero parameters (the decorator looks like an un-invoked factory —
    /// suggest `@dec()` over `@dec`); TS1241 covers all other incompatibilities.
    pub(crate) fn check_method_or_accessor_decorator_signature(
        &mut self,
        decorator_expr: NodeIndex,
        decorator_type: TypeId,
        decorator_node: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::query_boundaries::common::CallResult;

        if decorator_type == TypeId::ERROR
            || decorator_type == TypeId::ANY
            || decorator_type == TypeId::UNKNOWN
        {
            return;
        }

        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if resolved == TypeId::ERROR || resolved == TypeId::ANY || resolved == TypeId::UNKNOWN {
            return;
        }

        // `string` for propertyKey catches decorators whose key type is incompatible (e.g.
        // `number`); `any` for target and descriptor avoids false positives on those slots.
        let (result, _, _) = self.resolve_call_with_checker_adapter(
            resolved,
            &[TypeId::ANY, TypeId::STRING, TypeId::ANY],
            false,
            None,
            None,
        );

        if matches!(result, CallResult::Success(_)) {
            return;
        }

        // TS1329 takes priority when all signatures have zero params (un-invoked factory hint).
        let is_zero_param_callable = if let Some(shape) =
            crate::query_boundaries::class_type::function_shape(self.ctx.types, resolved)
        {
            shape.params.is_empty()
        } else if let Some(callable) =
            crate::query_boundaries::class_type::callable_shape_for_type(self.ctx.types, resolved)
        {
            !callable.call_signatures.is_empty()
                && callable
                    .call_signatures
                    .iter()
                    .all(|sig| sig.params.is_empty())
        } else {
            false
        };

        if is_zero_param_callable {
            let name = self.get_decorator_expression_name(decorator_expr);
            let msg = diagnostic_messages::ACCEPTS_TOO_FEW_ARGUMENTS_TO_BE_USED_AS_A_DECORATOR_HERE_DID_YOU_MEAN_TO_CALL_IT
                .replace("{0}", &name);
            self.error_at_node(
                decorator_node,
                &msg,
                diagnostic_codes::ACCEPTS_TOO_FEW_ARGUMENTS_TO_BE_USED_AS_A_DECORATOR_HERE_DID_YOU_MEAN_TO_CALL_IT,
            );
        } else {
            self.error_at_node(
                decorator_node,
                diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_METHOD_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_METHOD_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
            );
        }
    }

    /// TS1239: Check that a parameter decorator expression has a compatible
    /// call signature for the runtime invocation
    /// `decorator(target, propertyKey, parameterIndex)`.
    ///
    /// For experimental decorators, the runtime calling convention differs
    /// between constructor parameters and method/accessor parameters:
    ///
    /// - Constructor parameters: `decorator(classCtor, undefined, paramIndex)`
    /// - Method/accessor parameters:
    ///   `decorator(prototype, methodName, paramIndex)`
    ///
    /// When the decorator's resolved signature cannot be called with the
    /// shape that matches the parameter's enclosing function, tsc emits
    /// TS1239 ("Unable to resolve signature of parameter decorator when
    /// called as an expression."). The most common case is a decorator
    /// like `(target: Object, key: string, idx: number) => void` applied to
    /// a constructor parameter — `key: string` rejects `undefined`.
    ///
    /// `is_constructor_parameter` selects the `key` argument shape:
    /// `TypeId::UNDEFINED` for constructor params, `TypeId::STRING` for
    /// method/accessor params. We pass `TypeId::ANY` for `target` and the
    /// concrete-enough `TypeId::NUMBER` for `parameterIndex`; the call only
    /// needs to reject decorators whose param TYPES disagree with the
    /// runtime shape.
    pub(crate) fn check_parameter_decorator_call_signature(
        &mut self,
        decorator_node: NodeIndex,
        decorator_type: TypeId,
        is_constructor_parameter: bool,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::query_boundaries::common::CallResult;

        if decorator_type == TypeId::ERROR
            || decorator_type == TypeId::ANY
            || decorator_type == TypeId::UNKNOWN
        {
            return;
        }

        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if resolved == TypeId::ERROR || resolved == TypeId::ANY || resolved == TypeId::UNKNOWN {
            return;
        }

        // Per the runtime calling convention above, only the key argument
        // shape varies by parameter position.
        let key_arg = if is_constructor_parameter {
            TypeId::UNDEFINED
        } else {
            TypeId::STRING
        };

        let (result, _, _) = self.resolve_call_with_checker_adapter(
            resolved,
            &[TypeId::ANY, key_arg, TypeId::NUMBER],
            false,
            None,
            None,
        );

        if !matches!(result, CallResult::Success(_)) {
            self.error_at_node(
                decorator_node,
                diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_PARAMETER_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_PARAMETER_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
            );
        }
    }

    fn get_decorator_expression_name(&self, expr: NodeIndex) -> String {
        if let Some(node) = self.ctx.arena.get(expr)
            && let Some(ident) = self.ctx.arena.get_identifier(node)
        {
            return ident.escaped_text.to_string();
        }
        "decorator".to_string()
    }
}
