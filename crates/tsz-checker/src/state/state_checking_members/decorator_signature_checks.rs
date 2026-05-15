//! Class-member decorator signature validation helpers.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::query_boundaries::common::CallResult;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

/// Decorator-type sentinels that short-circuit signature validation. `ERROR`
/// is an unresolved type (we have already reported it elsewhere); `ANY` and
/// `UNKNOWN` are explicitly permissive and tsc does not emit a follow-on
/// TS1239/TS1240 for them.
#[inline]
const fn decorator_type_is_unchecked(t: TypeId) -> bool {
    matches!(t, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN)
}

impl<'a> CheckerState<'a> {
    /// TS1240 for ES class-member decorators (TC39 stage 3).
    ///
    /// The runtime calling convention for the first argument varies by member kind:
    ///
    /// - Plain field (`x = …`): `undefined`
    /// - Auto-accessor (`accessor x = …`): a `ClassAccessorDecoratorTarget<This, V>`
    ///   object — `{ get(this: This): V; set(this: This, value: V): void }`
    ///
    /// Callers select the first-arg type per member kind; this helper resolves
    /// the decorator type and verifies it is callable with `(first_arg, ANY)`,
    /// emitting TS1240 otherwise. The second argument (the decorator context)
    /// is `ANY` because the calling convention is distinguished by the first
    /// argument shape alone — the context object differs by kind but tsc
    /// reports the same TS1240 either way.
    pub(crate) fn check_es_member_decorator_call_signature(
        &mut self,
        decorator_node: NodeIndex,
        decorator_type: TypeId,
        first_arg: TypeId,
    ) {
        if decorator_type_is_unchecked(decorator_type) {
            return;
        }

        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if decorator_type_is_unchecked(resolved) {
            return;
        }

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

    /// Resolve `ClassAccessorDecoratorTarget<any, any>` from the lib globals.
    ///
    /// Returns `None` if the lib is not available (e.g. `--noLib`); callers
    /// fall back to a permissive shape so absent libs do not cause false
    /// positives.
    pub(crate) fn resolve_class_accessor_decorator_target_any(&mut self) -> Option<TypeId> {
        let lib_binders = self.get_lib_binders();
        let sym_id = self
            .ctx
            .binder
            .get_global_type_with_libs("ClassAccessorDecoratorTarget", &lib_binders)?;
        let base = self.ctx.create_lazy_type_ref(sym_id);
        Some(
            self.ctx
                .types
                .factory()
                .application(base, vec![TypeId::ANY, TypeId::ANY]),
        )
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
        if decorator_type_is_unchecked(decorator_type) {
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

    /// TS1240/TS1271 for legacy property decorators.
    ///
    /// Under `experimentalDecorators`, plain fields use the legacy property
    /// decorator ABI `(target, propertyKey)`, while `accessor` fields use
    /// `(target, propertyKey, descriptor)`. Both forms require the decorator
    /// return type to be `void` or `any`.
    pub(crate) fn check_legacy_property_decorator_call_signature(
        &mut self,
        decorator_node: NodeIndex,
        decorator_type: TypeId,
        is_auto_accessor: bool,
    ) {
        if decorator_type_is_unchecked(decorator_type) {
            return;
        }

        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if decorator_type_is_unchecked(resolved) {
            return;
        }

        let arg_types: &[TypeId] = if is_auto_accessor {
            &[TypeId::ANY, TypeId::STRING, TypeId::ANY]
        } else {
            &[TypeId::ANY, TypeId::STRING]
        };
        let (result, _, _) =
            self.resolve_call_with_checker_adapter(resolved, arg_types, false, None, None);

        let return_type = match result {
            CallResult::Success(return_type) => Some(return_type),
            _ => {
                self.error_at_node(
                    decorator_node,
                    diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_PROPERTY_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                    diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_PROPERTY_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                );
                self.recover_decorator_return_type_with_any_args(resolved)
                    .or_else(|| {
                        crate::query_boundaries::checkers::call::stable_call_recovery_return_type(
                            self.ctx.types,
                            resolved,
                        )
                    })
            }
        };

        let Some(return_type) = return_type else {
            return;
        };
        let return_type = self.evaluate_type_for_assignability(return_type);
        if matches!(return_type, TypeId::ERROR | TypeId::ANY) {
            return;
        }
        if !self.is_assignable_to(return_type, TypeId::VOID) {
            let return_str = self.format_type_diagnostic(return_type);
            let message = format_message(
                diagnostic_messages::DECORATOR_FUNCTION_RETURN_TYPE_IS_BUT_IS_EXPECTED_TO_BE_VOID_OR_ANY,
                &[&return_str],
            );
            self.error_at_node(
                decorator_node,
                &message,
                diagnostic_codes::DECORATOR_FUNCTION_RETURN_TYPE_IS_BUT_IS_EXPECTED_TO_BE_VOID_OR_ANY,
            );
        }
    }

    fn recover_decorator_return_type_with_any_args(
        &mut self,
        decorator_type: TypeId,
    ) -> Option<TypeId> {
        let arg_count =
            crate::query_boundaries::class_type::function_shape(self.ctx.types, decorator_type)
                .map(|shape| shape.params.len())
                .or_else(|| {
                    crate::query_boundaries::class_type::callable_shape_for_type(
                        self.ctx.types,
                        decorator_type,
                    )
                    .and_then(|shape| {
                        shape
                            .call_signatures
                            .first()
                            .map(|signature| signature.params.len())
                    })
                })?;

        let args = vec![TypeId::ANY; arg_count];
        let (result, _, _) =
            self.resolve_call_with_checker_adapter(decorator_type, &args, false, None, None);
        match result {
            CallResult::Success(return_type) => Some(return_type),
            _ => None,
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
        if decorator_type_is_unchecked(decorator_type) {
            return;
        }

        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if decorator_type_is_unchecked(resolved) {
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
