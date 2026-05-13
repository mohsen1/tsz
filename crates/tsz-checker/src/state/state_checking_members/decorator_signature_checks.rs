//! Class-member decorator signature validation helpers.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::query_boundaries::common::CallResult;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Guards against skippable decorator types, then evaluates to the assignability-
    /// resolved form. Returns `None` when the caller should skip further checks.
    fn resolve_decorator_type(&mut self, ty: TypeId) -> Option<TypeId> {
        if ty == TypeId::ERROR || ty == TypeId::ANY || ty == TypeId::UNKNOWN {
            return None;
        }
        self.ensure_relation_input_ready(ty);
        let resolved = self.evaluate_type_for_assignability(ty);
        if resolved == TypeId::ERROR || resolved == TypeId::ANY || resolved == TypeId::UNKNOWN {
            return None;
        }
        Some(resolved)
    }

    /// TS1240 for ES property/accessor decorators.
    ///
    /// The runtime calling convention for ES (TC39) class-member decorators varies
    /// by member kind:
    ///
    /// - Plain field: `decorator(undefined, context)` — `first_arg` is `TypeId::UNDEFINED`
    /// - Auto-accessor: `decorator(target, context)` where `target` is a
    ///   `ClassAccessorDecoratorTarget<V>` object — `first_arg` is `TypeId::ANY`
    ///   (the solver does not yet model `ClassAccessorDecoratorTarget` precisely, so
    ///   `ANY` is the conservative safe choice that avoids false positives while still
    ///   catching non-callable decorators).
    pub(crate) fn check_es_member_decorator_call_signature(
        &mut self,
        decorator_node: NodeIndex,
        decorator_type: TypeId,
        first_arg: TypeId,
    ) {
        let Some(resolved) = self.resolve_decorator_type(decorator_type) else {
            return;
        };

        let (result, _, _) = self.resolve_call_with_checker_adapter(
            resolved,
            &[first_arg, TypeId::ANY],
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

    /// TS1329: Check if a method/accessor decorator accepts too few arguments.
    ///
    /// For experimental decorators, method/accessor decorators are called as
    /// `decorator(target, propertyKey, descriptor)` - 3 arguments.
    /// If the decorator expression has call signatures but none can accept 3 args,
    /// emit TS1329 suggesting to call it first: `@dec()` instead of `@dec`.
    pub(crate) fn check_method_decorator_arity(
        &mut self,
        decorator_expr: NodeIndex,
        decorator_type: TypeId,
        decorator_node: NodeIndex,
    ) {
        if decorator_type == TypeId::ERROR
            || decorator_type == TypeId::ANY
            || decorator_type == TypeId::UNKNOWN
        {
            return;
        }

        let has_too_few_args = if let Some(shape) =
            crate::query_boundaries::class_type::function_shape(self.ctx.types, decorator_type)
        {
            shape.params.is_empty()
        } else if let Some(callable) = crate::query_boundaries::class_type::callable_shape_for_type(
            self.ctx.types,
            decorator_type,
        ) {
            !callable.call_signatures.is_empty()
                && callable
                    .call_signatures
                    .iter()
                    .all(|sig| sig.params.is_empty())
        } else {
            false
        };

        if has_too_few_args {
            let name = self.get_decorator_expression_name(decorator_expr);
            let msg = diagnostic_messages::ACCEPTS_TOO_FEW_ARGUMENTS_TO_BE_USED_AS_A_DECORATOR_HERE_DID_YOU_MEAN_TO_CALL_IT
                .replace("{0}", &name);
            self.error_at_node(
                decorator_node,
                &msg,
                diagnostic_codes::ACCEPTS_TOO_FEW_ARGUMENTS_TO_BE_USED_AS_A_DECORATOR_HERE_DID_YOU_MEAN_TO_CALL_IT,
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
        let Some(resolved) = self.resolve_decorator_type(decorator_type) else {
            return;
        };

        // Only the key argument shape varies by parameter position.
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
