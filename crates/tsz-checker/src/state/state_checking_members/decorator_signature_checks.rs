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

        let Some(resolved) = self.prepare_decorator_callee(resolved) else {
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

    /// Mirror of tsc's `isUntypedFunctionCall` for decorator-callee resolution.
    ///
    /// tsc treats a non-callable callee as "untyped" — and skips signature
    /// validation — only when the callee has *no* call signatures, *no*
    /// construct signatures, is not a union, and is **assignable to the
    /// global `Function` type**. The full Function-shaped check is stricter
    /// than "has a `bind` member": objects like `{ bind: any }` are not
    /// assignable to `Function` (which also requires `apply`, `call`,
    /// `prototype`, `length`, …) and must still emit TS1238/1239/1240/1241.
    ///
    /// Without this fallback, decorator factories whose declared return type
    /// is `Function` would produce a spurious decorator-signature diagnostic
    /// because the `Function` interface has no explicit call signatures of
    /// its own.
    ///
    /// Returns `Some(t)` when the caller should continue with the callee, or
    /// `None` when the callee is Function-typed and the caller should skip
    /// the call check entirely.
    ///
    /// Hot path: most decorators are explicit function types with a known
    /// call signature, so `has_function_shape` short-circuits before the
    /// more expensive Function-membership probe.
    pub(crate) fn prepare_decorator_callee(&mut self, decorator_type: TypeId) -> Option<TypeId> {
        if crate::query_boundaries::common::has_function_shape(self.ctx.types, decorator_type) {
            return Some(decorator_type);
        }
        if self.decorator_callee_is_untyped_function(decorator_type) {
            return None;
        }
        Some(decorator_type)
    }

    /// True when `decorator_type` would qualify as an "untyped function call"
    /// callee under tsc's `isUntypedFunctionCall`: no call signatures, no
    /// construct signatures, not a union, and assignable to the global
    /// `Function` type. Callers use this to skip signature validation when
    /// tsc would.
    fn decorator_callee_is_untyped_function(&mut self, decorator_type: TypeId) -> bool {
        // Reject unions outright — tsc's `isUntypedFunctionCall` explicitly
        // excludes them ("a union of function types that happen to have no
        // common signatures" is still a typed call).
        if crate::query_boundaries::common::union_members(self.ctx.types, decorator_type).is_some()
        {
            return false;
        }

        // Reject callees with call signatures (handled by the caller's
        // fast path, but be defensive) or construct signatures.
        let has_calls = crate::query_boundaries::common::call_signatures_for_type(
            self.ctx.types,
            decorator_type,
        )
        .is_some_and(|sigs| !sigs.is_empty());
        if has_calls {
            return false;
        }
        let has_constructs = crate::query_boundaries::class_type::construct_signatures_for_type(
            self.ctx.types,
            decorator_type,
        )
        .is_some_and(|sigs| !sigs.is_empty());
        if has_constructs {
            return false;
        }

        // The direct global `Function` type (and `typeof v` where v: Function)
        // is the only common decorator-typed-as-Function case. Match it
        // narrowly via the existing `is_global_function_type` query, which
        // compares via the canonical Function `DefId`.
        if self.is_global_function_type(decorator_type) {
            return true;
        }

        // For rare cases like `interface SubFunc extends Function {}` used
        // as a decorator return type, fall back to a structural
        // assignability check against the global `Function` interface.
        let Some(function_type) = self.global_function_type_id() else {
            return false;
        };
        self.is_assignable_to(decorator_type, function_type)
    }

    fn global_function_type_id(&mut self) -> Option<TypeId> {
        let lib_binders = self.get_lib_binders();
        let sym_id = self
            .ctx
            .binder
            .get_global_type_with_libs("Function", &lib_binders)?;
        Some(self.ctx.create_lazy_type_ref(sym_id))
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

    pub(crate) fn check_method_or_accessor_decorator_call_signature(
        &mut self,
        decorator_expr: NodeIndex,
        decorator_type: TypeId,
        decorator_node: NodeIndex,
        member_node: NodeIndex,
        experimental_decorators: bool,
    ) {
        if decorator_type_is_unchecked(decorator_type) {
            return;
        }

        self.ensure_relation_input_ready(decorator_type);
        let resolved = self.evaluate_type_for_assignability(decorator_type);
        if decorator_type_is_unchecked(resolved) {
            return;
        }

        let Some(resolved) = self.prepare_decorator_callee(resolved) else {
            return;
        };

        if self.method_decorator_has_zero_arg_factory_shape(
            decorator_expr,
            resolved,
            decorator_node,
        ) {
            return;
        }

        let arg_types = if experimental_decorators {
            // tsc's `getLegacyDecoratorArgumentCount` adapts the supplied
            // argument count to the decorator's signature for method/accessor
            // decorators: 2 args when every call signature has ≤ 2 parameters,
            // 3 args otherwise. Without this adaptation, a 2-parameter legacy
            // decorator factory like `(target: object, key: PropertyKey) =>
            // void` produces a spurious TS1241 when applied to a method.
            if Self::legacy_method_decorator_uses_two_args(self.ctx.types, resolved) {
                vec![TypeId::ANY, TypeId::STRING]
            } else {
                vec![TypeId::ANY, TypeId::STRING, TypeId::ANY]
            }
        } else {
            self.es_method_or_accessor_decorator_args(member_node)
                .unwrap_or_else(|| vec![TypeId::ANY, TypeId::OBJECT])
        };

        let (result, _, _) =
            self.resolve_call_with_checker_adapter(resolved, &arg_types, false, None, None);

        let return_type = match result {
            CallResult::Success(return_type) => Some(return_type),
            _ => {
                self.error_at_node(
                    decorator_node,
                    diagnostic_messages::UNABLE_TO_RESOLVE_SIGNATURE_OF_METHOD_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
                    diagnostic_codes::UNABLE_TO_RESOLVE_SIGNATURE_OF_METHOD_DECORATOR_WHEN_CALLED_AS_AN_EXPRESSION,
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

        self.check_method_or_accessor_decorator_return_type(
            decorator_node,
            member_node,
            experimental_decorators,
            return_type,
        );
    }

    /// Mirror of tsc's `getLegacyDecoratorArgumentCount` for the
    /// method/accessor decorator case. Returns `true` when every call
    /// signature on the decorator has ≤ 2 parameters, indicating that tsc
    /// would supply only 2 arguments (target, propertyKey) instead of the
    /// usual 3 (target, propertyKey, descriptor).
    ///
    /// The decision is made over *all* call signatures so that an overloaded
    /// decorator with a 3-parameter signature still receives the descriptor
    /// argument. This matches tsc's overload semantics: the resolved
    /// signature drives the arity, and any signature that needs the
    /// descriptor will be chosen when 3 args are supplied.
    fn legacy_method_decorator_uses_two_args(
        db: &dyn tsz_solver::TypeDatabase,
        decorator_type: TypeId,
    ) -> bool {
        if let Some(shape) = crate::query_boundaries::class_type::function_shape(db, decorator_type)
        {
            return shape.params.len() <= 2;
        }

        if let Some(callable) =
            crate::query_boundaries::class_type::callable_shape_for_type(db, decorator_type)
        {
            if callable.call_signatures.is_empty() {
                // Callable shape with no call signatures: the subsequent call
                // will fail regardless of argcount, so pick the legacy 3-arg
                // default to keep recovery-path diagnostics stable.
                return false;
            }
            return callable
                .call_signatures
                .iter()
                .all(|sig| sig.params.len() <= 2);
        }

        // No statically known shape: default to the historical 3-arg call so
        // recovery paths and error reporting stay aligned with the prior
        // unconditional behavior.
        false
    }

    /// TS1329: Check if a method/accessor decorator accepts too few arguments.
    ///
    /// Method/accessor decorators are invoked with at least two arguments in
    /// stage-3 mode and three arguments in legacy mode. If every call signature
    /// has zero parameters, tsc reports the decorator-factory hint instead of
    /// the generic method-decorator signature failure.
    fn method_decorator_has_zero_arg_factory_shape(
        &mut self,
        decorator_expr: NodeIndex,
        decorator_type: TypeId,
        decorator_node: NodeIndex,
    ) -> bool {
        if decorator_type_is_unchecked(decorator_type) {
            return false;
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
            return true;
        }

        false
    }

    fn es_method_or_accessor_decorator_args(
        &mut self,
        member_idx: NodeIndex,
    ) -> Option<Vec<TypeId>> {
        let member = self.ctx.arena.get(member_idx)?;
        match member.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::METHOD_DECLARATION => {
                let value_type = self.method_decorator_value_type(member_idx)?;
                let context_type = self
                    .resolve_decorator_context_type(
                        "ClassMethodDecoratorContext",
                        vec![TypeId::ANY, value_type],
                    )
                    .unwrap_or(TypeId::OBJECT);
                Some(vec![value_type, context_type])
            }
            k if k == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR => {
                let value_type = self.accessor_decorator_value_type(member_idx)?;
                let value = self
                    .accessor_value_type_argument(member_idx)
                    .unwrap_or(TypeId::ANY);
                let context_type = self
                    .resolve_decorator_context_type(
                        "ClassGetterDecoratorContext",
                        vec![TypeId::ANY, value],
                    )
                    .unwrap_or(TypeId::OBJECT);
                Some(vec![value_type, context_type])
            }
            k if k == tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR => {
                let value_type = self.accessor_decorator_value_type(member_idx)?;
                let value = self
                    .accessor_value_type_argument(member_idx)
                    .unwrap_or(TypeId::ANY);
                let context_type = self
                    .resolve_decorator_context_type(
                        "ClassSetterDecoratorContext",
                        vec![TypeId::ANY, value],
                    )
                    .unwrap_or(TypeId::OBJECT);
                Some(vec![value_type, context_type])
            }
            _ => None,
        }
    }

    fn resolve_decorator_context_type(&mut self, name: &str, args: Vec<TypeId>) -> Option<TypeId> {
        let lib_binders = self.get_lib_binders();
        let sym_id = self
            .ctx
            .binder
            .get_global_type_with_libs(name, &lib_binders)?;
        let base = self.ctx.create_lazy_type_ref(sym_id);
        Some(self.ctx.types.factory().application(base, args))
    }

    fn method_decorator_value_type(&mut self, member_idx: NodeIndex) -> Option<TypeId> {
        let member = self.ctx.arena.get(member_idx)?;
        let method = self.ctx.arena.get_method_decl(member)?;
        let (type_params, type_param_updates) = self.push_type_parameters(&method.type_parameters);
        let (params, this_type) = self.extract_params_from_parameter_list(&method.parameters);
        let return_type = if method.type_annotation.is_some() {
            self.get_type_from_type_node(method.type_annotation)
        } else if method.body.is_some() {
            self.infer_return_type_from_body(member_idx, method.body, None)
        } else {
            TypeId::ANY
        };
        self.pop_type_parameters(type_param_updates);

        Some(
            self.ctx
                .types
                .factory()
                .function(tsz_solver::FunctionShape {
                    type_params,
                    params,
                    this_type,
                    return_type,
                    type_predicate: None,
                    is_constructor: false,
                    is_method: true,
                }),
        )
    }

    fn accessor_decorator_value_type(&mut self, member_idx: NodeIndex) -> Option<TypeId> {
        let member = self.ctx.arena.get(member_idx)?;
        let accessor = self.ctx.arena.get_accessor(member)?;
        let (params, this_type) = self.extract_params_from_parameter_list(&accessor.parameters);
        let return_type = if member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR {
            if accessor.type_annotation.is_some() {
                self.get_type_from_type_node(accessor.type_annotation)
            } else if accessor.body.is_some() {
                self.infer_return_type_from_body(member_idx, accessor.body, None)
            } else {
                TypeId::ANY
            }
        } else {
            TypeId::VOID
        };

        Some(
            self.ctx
                .types
                .factory()
                .function(tsz_solver::FunctionShape {
                    type_params: Vec::new(),
                    params,
                    this_type,
                    return_type,
                    type_predicate: None,
                    is_constructor: false,
                    is_method: true,
                }),
        )
    }

    fn accessor_value_type_argument(&mut self, member_idx: NodeIndex) -> Option<TypeId> {
        let member = self.ctx.arena.get(member_idx)?;
        let accessor = self.ctx.arena.get_accessor(member)?;
        if member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR {
            if accessor.type_annotation.is_some() {
                return Some(self.get_type_from_type_node(accessor.type_annotation));
            }
            if accessor.body.is_some() {
                return Some(self.infer_return_type_from_body(member_idx, accessor.body, None));
            }
            return Some(TypeId::ANY);
        }

        let first_param = accessor.parameters.nodes.first().copied()?;
        let param_node = self.ctx.arena.get(first_param)?;
        let param = self.ctx.arena.get_parameter(param_node)?;
        if param.type_annotation.is_some() {
            Some(self.get_type_from_type_node(param.type_annotation))
        } else {
            Some(TypeId::ANY)
        }
    }

    fn check_method_or_accessor_decorator_return_type(
        &mut self,
        decorator_node: NodeIndex,
        member_idx: NodeIndex,
        experimental_decorators: bool,
        return_type: Option<TypeId>,
    ) {
        let Some(return_type) = return_type else {
            return;
        };
        let return_type = self.evaluate_type_for_assignability(return_type);
        if matches!(return_type, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN) {
            return;
        }

        let Some(expected_return) = self
            .method_or_accessor_decorator_expected_return_type(member_idx, experimental_decorators)
        else {
            return;
        };
        if !self.is_assignable_to(return_type, expected_return) {
            let return_str = self.format_type_diagnostic(return_type);
            let expected_str = self.format_type_diagnostic(expected_return);
            let message = format_message(
                diagnostic_messages::DECORATOR_FUNCTION_RETURN_TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&return_str, &expected_str],
            );
            self.error_at_node(
                decorator_node,
                &message,
                diagnostic_codes::DECORATOR_FUNCTION_RETURN_TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }
    }

    fn method_or_accessor_decorator_expected_return_type(
        &mut self,
        member_idx: NodeIndex,
        experimental_decorators: bool,
    ) -> Option<TypeId> {
        if !experimental_decorators {
            let value_type = self.method_or_accessor_decorator_value_type(member_idx)?;
            return Some(self.ctx.types.factory().union2(TypeId::VOID, value_type));
        }

        let descriptor_value = self.legacy_method_or_accessor_descriptor_value_type(member_idx)?;
        let descriptor_type =
            self.resolve_decorator_context_type("TypedPropertyDescriptor", vec![descriptor_value])?;
        Some(
            self.ctx
                .types
                .factory()
                .union2(TypeId::VOID, descriptor_type),
        )
    }

    fn method_or_accessor_decorator_value_type(&mut self, member_idx: NodeIndex) -> Option<TypeId> {
        let member = self.ctx.arena.get(member_idx)?;
        match member.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::METHOD_DECLARATION => {
                self.method_decorator_value_type(member_idx)
            }
            k if k == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR
                || k == tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR =>
            {
                self.accessor_decorator_value_type(member_idx)
            }
            _ => None,
        }
    }

    fn legacy_method_or_accessor_descriptor_value_type(
        &mut self,
        member_idx: NodeIndex,
    ) -> Option<TypeId> {
        let member = self.ctx.arena.get(member_idx)?;
        match member.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::METHOD_DECLARATION => {
                self.method_decorator_value_type(member_idx)
            }
            k if k == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR
                || k == tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR =>
            {
                self.accessor_value_type_argument(member_idx)
            }
            _ => None,
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

        let Some(resolved) = self.prepare_decorator_callee(resolved) else {
            return;
        };

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

        let Some(resolved) = self.prepare_decorator_callee(resolved) else {
            return;
        };

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
