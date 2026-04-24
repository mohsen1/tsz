//! Call expression type computation for `CheckerState`.
//!
//! Handles call expression type resolution including overload resolution,
//! argument type checking, type argument validation, and call result processing.
//! Identifier resolution is in `identifier.rs` and tagged
//! template expression handling is in `tagged_template.rs`.
//!
//! Split into submodules:
//! - `inner` — the main `get_type_of_call_expression_inner` implementation

mod inner;

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn callee_suppresses_contextual_any(
        &self,
        callee_idx: NodeIndex,
        snap: &crate::context::speculation::DiagnosticSnapshot,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let callee_idx = self.ctx.arena.skip_parenthesized_and_assertions(callee_idx);
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };

        let is_simple_error_path = matches!(
            callee_node.kind,
            k if k == tsz_scanner::SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        );
        if !is_simple_error_path {
            return false;
        }

        let has_callee_side_failure =
            self.ctx.speculative_diagnostics_since(snap).iter().any(|diag| {
                diag.start >= callee_node.pos
                    && diag.start < callee_node.end
                    && matches!(
                        diag.code,
                        diagnostic_codes::CANNOT_FIND_NAME
                            | diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN
                            | diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_STATIC_MEMBER
                            | diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS
                            | diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                            | diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN
                            | diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_VALUE
                            | diagnostic_codes::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW
                            | diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE
                            | diagnostic_codes::TYPE_HAS_NO_CALL_SIGNATURES
                    )
            });

        has_callee_side_failure || self.property_access_base_is_error_symbol(callee_idx)
    }

    fn property_access_base_is_error_symbol(&self, callee_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let callee_idx = self.ctx.arena.skip_parenthesized_and_assertions(callee_idx);
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && callee_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let base_expr = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(access.expression);
        let Some(base_node) = self.ctx.arena.get(base_expr) else {
            return false;
        };
        if base_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }

        self.resolve_identifier_symbol(base_expr)
            .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied())
            == Some(TypeId::ERROR)
    }

    fn reemit_namespace_value_error_for_call_callee(&mut self, callee_idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext;

        let callee_idx = self.ctx.arena.skip_parenthesized_and_assertions(callee_idx);
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return;
        };

        let base_expr = match callee_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                self.ctx
                    .arena
                    .get_access_expr(callee_node)
                    .map(|access| access.expression)
            }
            _ => None,
        };

        let Some(base_expr) = base_expr else {
            return;
        };
        let base_expr = self.ctx.arena.skip_parenthesized_and_assertions(base_expr);

        let _ = self.report_namespace_value_access_for_type_only_import_equals_expr(base_expr);
    }

    #[allow(clippy::too_many_arguments)]
    fn finalize_call_after_argument_collection(
        &mut self,
        idx: NodeIndex,
        callee_expr: NodeIndex,
        args: &[NodeIndex],
        mut arg_types: Vec<TypeId>,
        callee_type: TypeId,
        callee_type_for_resolution: TypeId,
        base_contextual_param_types: &[Option<TypeId>],
        non_generic_contextual_types: Option<&[Option<TypeId>]>,
        check_excess_properties: bool,
        callable_ctx: crate::call_checker::CallableContext,
        is_generic_call: bool,
        contextual_type: Option<TypeId>,
        force_bivariant_callbacks: bool,
        actual_this_type: Option<TypeId>,
        is_super_call: bool,
        is_optional_chain: bool,
        had_return_context_substitution: bool,
        shape_this_type: Option<TypeId>,
        pushed_this_type_from_shape: bool,
    ) -> TypeId {
        use crate::query_boundaries::assignability as assign_query;
        use crate::query_boundaries::checkers::call as call_checker;
        use crate::query_boundaries::checkers::call::is_type_parameter_type;
        use crate::query_boundaries::common;
        use crate::query_boundaries::common::ContextualTypeContext;
        use tsz_parser::parser::syntax_kind_ext;

        self.ensure_relation_input_ready(callee_type_for_resolution);

        let callee_type_for_call = self.evaluate_application_type(callee_type_for_resolution);
        let callee_type_for_call = self.resolve_lazy_type(callee_type_for_call);
        let callee_type_for_call = self.resolve_lazy_members_in_union(callee_type_for_call);
        let callee_type_for_call =
            self.replace_function_type_for_call(callee_type, callee_type_for_call);
        if callee_type_for_call == TypeId::ANY {
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None,
                check_excess_properties,
                None,
                crate::call_checker::CallableContext::none(),
            );
            return if is_optional_chain {
                common::union_with_undefined(self.ctx.types, TypeId::ANY)
            } else {
                TypeId::ANY
            };
        }

        self.ensure_relation_input_ready(callee_type_for_call);

        let (generic_inference_arg_types, sanitized_generic_inference) = if is_generic_call {
            self.sanitize_generic_inference_arg_types(callee_expr, args, &arg_types)
        } else {
            (arg_types.clone(), false)
        };
        let call_resolution_contextual_type = contextual_type;

        let (mut result, mut instantiated_predicate, mut generic_instantiated_params) =
            if is_super_call {
                (
                    self.resolve_new_with_checker_adapter(
                        callee_type_for_call,
                        &generic_inference_arg_types,
                        force_bivariant_callbacks,
                        call_resolution_contextual_type,
                    ),
                    None,
                    None,
                )
            } else {
                self.resolve_call_with_checker_adapter(
                    callee_type_for_call,
                    &generic_inference_arg_types,
                    force_bivariant_callbacks,
                    call_resolution_contextual_type,
                    actual_this_type,
                )
            };
        let needs_real_type_recheck = is_generic_call
            && args.iter().enumerate().any(|(i, &arg_idx)| {
                self.argument_needs_refresh_for_contextual_call(
                    arg_idx,
                    base_contextual_param_types.get(i).copied().flatten(),
                )
            });

        if !is_generic_call
            && let crate::query_boundaries::common::CallResult::ArgumentTypeMismatch {
                index,
                fallback_return,
                ..
            } = result.clone()
            && let Some(expected) = non_generic_contextual_types
                .and_then(|types| types.get(index).copied().flatten())
                .map(|expected| self.evaluate_contextual_type(expected))
            && let Some(&arg_idx) = args.get(index)
            && let Some(actual) = Some(self.refreshed_generic_call_arg_type_with_context(
                arg_idx,
                arg_types.get(index).copied().unwrap_or(TypeId::UNKNOWN),
                Some(expected),
            ))
        {
            let fresh_subtype = assign_query::is_fresh_subtype_of(self.ctx.types, actual, expected);
            let recover_object_literal =
                fresh_subtype
                    && !self.object_literal_has_computed_property_names(arg_idx)
                    && self.ctx.arena.get(arg_idx).is_some_and(|node| {
                        node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    });
            if recover_object_literal {
                if expected != TypeId::ANY
                    && expected != TypeId::UNKNOWN
                    && !is_type_parameter_type(self.ctx.types, expected)
                    && !self.contextual_type_is_unresolved_for_argument_refresh(expected)
                {
                    self.check_object_literal_excess_properties(actual, expected, arg_idx);
                }
                let recovered_return = if fallback_return != TypeId::ERROR {
                    Some(fallback_return)
                } else {
                    assign_query::get_function_return_type(self.ctx.types, callee_type_for_call)
                };
                if let Some(return_type) = recovered_return {
                    result = crate::query_boundaries::common::CallResult::Success(return_type);
                }
            }
        }

        let should_retry_generic_call = if is_generic_call
            && !had_return_context_substitution
            && args.iter().enumerate().any(|(i, &arg_idx)| {
                self.argument_needs_refresh_for_contextual_call(
                    arg_idx,
                    base_contextual_param_types.get(i).copied().flatten(),
                )
            }) {
            if let Some(ctx_type) = contextual_type {
                match &result {
                    crate::query_boundaries::common::CallResult::Success(ret) => {
                        let contextual_return = self.evaluate_contextual_type(ctx_type);
                        !self.is_assignable_to_with_env(*ret, contextual_return)
                    }
                    _ => true,
                }
            } else {
                true
            }
        } else {
            false
        };

        if is_generic_call
            && should_retry_generic_call
            && let Some(instantiated_params) = generic_instantiated_params.as_ref()
        {
            self.clear_contextual_resolution_cache();
            for (i, &arg_idx) in args.iter().enumerate() {
                if self.argument_needs_refresh_for_contextual_call(
                    arg_idx,
                    base_contextual_param_types.get(i).copied().flatten(),
                ) {
                    self.invalidate_expression_for_contextual_retry(arg_idx);
                }
            }
            let refreshed_contextual_types = self
                .contextual_param_types_from_instantiated_params(instantiated_params, args.len())
                .into_iter()
                .map(|param_type| {
                    param_type
                        .map(|param_type| self.normalize_contextual_call_param_type(param_type))
                })
                .collect::<Vec<_>>();
            arg_types = self.collect_call_argument_types_with_context(
                args,
                |i, _arg_count| {
                    refreshed_contextual_types
                        .get(i)
                        .copied()
                        .flatten()
                        .or_else(|| base_contextual_param_types.get(i).copied().flatten())
                },
                check_excess_properties,
                None,
                callable_ctx,
            );

            let (retry_generic_arg_types, retry_sanitized) =
                self.sanitize_generic_inference_arg_types(callee_expr, args, &arg_types);
            let retry = if is_super_call {
                (
                    self.resolve_new_with_checker_adapter(
                        callee_type_for_call,
                        &retry_generic_arg_types,
                        force_bivariant_callbacks,
                        contextual_type,
                    ),
                    None,
                    None,
                )
            } else {
                self.resolve_call_with_checker_adapter(
                    callee_type_for_call,
                    &retry_generic_arg_types,
                    force_bivariant_callbacks,
                    contextual_type,
                    actual_this_type,
                )
            };
            result = if retry_sanitized || needs_real_type_recheck {
                if let Some(instantiated_params) = retry.2.as_ref() {
                    self.recheck_generic_call_arguments_with_real_types(
                        retry.0.clone(),
                        instantiated_params,
                        args,
                        &arg_types,
                    )
                } else {
                    retry.0
                }
            } else {
                retry.0
            };
            instantiated_predicate = retry.1;
            generic_instantiated_params = retry.2;
        }

        if is_generic_call
            && let crate::query_boundaries::common::CallResult::Success(return_type) = result
            && let Some(ctx_type) =
                contextual_type.filter(|&ct| ct != TypeId::ANY && ct != TypeId::UNKNOWN)
            && (common::contains_type_parameters(self.ctx.types, return_type)
                || common::contains_infer_types(self.ctx.types, return_type)
                || common::contains_type_by_id(self.ctx.types, return_type, TypeId::UNKNOWN))
            && let Some(shape) = call_checker::get_contextual_signature_for_arity(
                self.ctx.types,
                callee_type_for_call,
                args.len(),
            )
        {
            let mut return_context_substitution =
                self.compute_return_context_substitution_from_shape(&shape, Some(ctx_type));
            let return_param_names: rustc_hash::FxHashSet<_> = self
                .function_like_return_parameter_type_params(&shape)
                .into_iter()
                .collect();
            if !return_param_names.is_empty() {
                let mut filtered = crate::query_boundaries::common::TypeSubstitution::new();
                for (&name, &type_id) in return_context_substitution.map() {
                    if !return_param_names.contains(&name) {
                        filtered.insert(name, type_id);
                    }
                }
                return_context_substitution = filtered;
            }

            if !return_context_substitution.is_empty() {
                let instantiated_return = crate::query_boundaries::common::instantiate_type(
                    self.ctx.types,
                    return_type,
                    &return_context_substitution,
                );
                if instantiated_return != return_type {
                    result =
                        crate::query_boundaries::common::CallResult::Success(instantiated_return);
                }
            }
        }

        if let Some(predicate) = instantiated_predicate {
            let stored_predicate =
                call_checker::extract_predicate_signature(self.ctx.types, callee_type_for_call)
                    .filter(|sig| {
                        sig.predicate.type_id.is_some_and(|pred_ty| {
                            common::type_param_info(self.ctx.types, pred_ty).is_some()
                        })
                    })
                    .map(|sig| (sig.predicate, sig.params))
                    .unwrap_or(predicate);
            self.ctx
                .call_type_predicates
                .insert(idx.0, stored_predicate);
        } else {
            let is_sound_union = if common::is_union_type(self.ctx.types, callee_type_for_call) {
                call_checker::is_valid_union_predicate(self.ctx.types, callee_type_for_call)
            } else {
                true
            };
            if is_sound_union
                && let Some(extracted) =
                    call_checker::extract_predicate_signature(self.ctx.types, callee_type_for_call)
            {
                self.ctx
                    .call_type_predicates
                    .insert(idx.0, (extracted.predicate, extracted.params));
            }
        }

        let (mut result, mut allow_contextual_mismatch_deferral) = self
            .finalize_generic_call_result(
                callee_type_for_call,
                generic_instantiated_params.as_ref(),
                args,
                &arg_types,
                result,
                sanitized_generic_inference,
                needs_real_type_recheck,
                shape_this_type,
            );
        let finalized_contextual_param_types = generic_instantiated_params
            .as_ref()
            .map(|params| self.contextual_param_types_from_instantiated_params(params, args.len()));
        let forced_block_body_callback_mismatch = self
            .current_block_body_callback_return_mismatch_arg(args, |checker, index| {
                finalized_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(index).copied().flatten())
                    .or_else(|| {
                        ContextualTypeContext::with_expected_and_options(
                            checker.ctx.types,
                            callee_type_for_call,
                            checker.ctx.compiler_options.no_implicit_any,
                        )
                        .get_parameter_type_for_call(index, args.len())
                    })
            })
            .inspect(|&(index, actual, expected)| {
                if let crate::query_boundaries::common::CallResult::Success(return_type) = result {
                    allow_contextual_mismatch_deferral = false;
                    result = crate::query_boundaries::common::CallResult::ArgumentTypeMismatch {
                        index,
                        expected,
                        actual,
                        fallback_return: return_type,
                    };
                }
            })
            .is_some();
        let forced_binding_pattern_unknown_context_mismatch = self
            .current_binding_pattern_callback_unknown_context_arg(args, |checker, index| {
                finalized_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(index).copied().flatten())
                    .or_else(|| {
                        ContextualTypeContext::with_expected_and_options(
                            checker.ctx.types,
                            callee_type_for_call,
                            checker.ctx.compiler_options.no_implicit_any,
                        )
                        .get_parameter_type_for_call(index, args.len())
                    })
            })
            .inspect(|&(index, actual, expected)| {
                if matches!(
                    result,
                    crate::query_boundaries::common::CallResult::Success(_)
                ) && let Some(&arg_idx) = args.get(index)
                {
                    allow_contextual_mismatch_deferral = false;
                    self.error_argument_not_assignable_at(actual, expected, arg_idx);
                }
            })
            .is_some();
        if forced_block_body_callback_mismatch {
            allow_contextual_mismatch_deferral = false;
        }
        if let crate::query_boundaries::common::CallResult::ArgumentTypeMismatch {
            actual: _,
            expected: _,
            fallback_return,
            ..
        } = result
            && !forced_block_body_callback_mismatch
            && !forced_binding_pattern_unknown_context_mismatch
            && fallback_return != TypeId::ERROR
        {}

        let call_context = super::call_result::CallResultContext {
            callee_expr,
            call_idx: idx,
            args,
            arg_types: &arg_types,
            callee_type: callee_type_for_call,
            is_super_call,
            is_optional_chain,
            allow_contextual_mismatch_deferral,
        };
        if pushed_this_type_from_shape {
            self.ctx.this_type_stack.pop();
        }
        self.handle_call_result(result, call_context)
    }

    /// Get the type of a call expression (e.g., `foo()`, `obj.method()`).
    ///
    /// Computes the return type of function/method calls.
    /// Handles:
    /// - Dynamic imports (returns `Promise<any>`)
    /// - Super calls (returns `void`)
    /// - Optional chaining (`obj?.method()`)
    /// - Overload resolution
    /// - Argument type checking
    /// - Type argument validation (TS2344)
    #[allow(dead_code)]
    pub(crate) fn get_type_of_call_expression(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_call_expression_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_call_expression_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        // Check call depth limit to prevent infinite recursion
        if !self.ctx.call_depth.borrow_mut().enter() {
            return TypeId::ERROR;
        }

        let result = self.get_type_of_call_expression_inner(idx, request);

        // TS2590: Check if the call produced a union type that is too complex.
        // The solver sets a flag during union normalization when the constituent
        // count exceeds the threshold. We check and clear it here to emit the
        // diagnostic at the call expression that triggered it.
        if self.ctx.types.take_union_too_complex() {
            use crate::diagnostics::diagnostic_messages;
            self.error_at_node(
                idx,
                diagnostic_messages::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
                diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
            );
        }

        self.ctx.call_depth.borrow_mut().leave();
        result
    }

    /// Check if a call is a dynamic import and handle all associated diagnostics.
    /// Returns `Some(type_id)` if this is a dynamic import (the caller should return it),
    /// or `None` if this is not a dynamic import.
    fn check_and_resolve_dynamic_import(
        &mut self,
        idx: NodeIndex,
        call: &tsz_parser::parser::node::CallExprData,
    ) -> Option<TypeId> {
        if !self.is_dynamic_import(call) {
            return None;
        }

        // TS1323: Dynamic imports require a module kind that supports them
        if !self.ctx.compiler_options.module.supports_dynamic_import() {
            self.error_at_node(
                idx,
                crate::diagnostics::diagnostic_messages::DYNAMIC_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ES2020_ES2022,
                diagnostic_codes::DYNAMIC_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ES2020_ES2022,
            );
        }

        // TS1325: Check for spread elements in import arguments
        if let Some(ref args_list) = call.arguments {
            for &arg_idx in &args_list.nodes {
                if let Some(arg_node) = self.ctx.arena.get(arg_idx)
                    && arg_node.kind == tsz_parser::parser::syntax_kind_ext::SPREAD_ELEMENT
                {
                    self.error_at_node(
                        arg_idx,
                        crate::diagnostics::diagnostic_messages::ARGUMENT_OF_DYNAMIC_IMPORT_CANNOT_BE_SPREAD_ELEMENT,
                        diagnostic_codes::ARGUMENT_OF_DYNAMIC_IMPORT_CANNOT_BE_SPREAD_ELEMENT,
                    );
                }
            }
        }

        // TS1324: Second argument only supported for certain module kinds.
        // Only emit when dynamic imports are supported (TS1323 not emitted),
        // otherwise TS1323 already covers the unsupported case.
        if let Some(ref args_list) = call.arguments
            && args_list.nodes.len() >= 2
            && self.ctx.compiler_options.module.supports_dynamic_import()
            && !self
                .ctx
                .compiler_options
                .module
                .supports_dynamic_import_options()
        {
            self.error_at_node(
                args_list.nodes[1],
                crate::diagnostics::diagnostic_messages::DYNAMIC_IMPORTS_ONLY_SUPPORT_A_SECOND_ARGUMENT_WHEN_THE_MODULE_OPTION_IS_SET_TO,
                diagnostic_codes::DYNAMIC_IMPORTS_ONLY_SUPPORT_A_SECOND_ARGUMENT_WHEN_THE_MODULE_OPTION_IS_SET_TO,
            );
        }

        // TS7036: Check specifier type is assignable to `string`
        self.check_dynamic_import_specifier_type(call);
        // TS2322/TS2559: Check options arg against ImportCallOptions
        self.check_dynamic_import_options_type(call);
        self.check_dynamic_import_module_specifier(call);

        // TS2712: Dynamic import requires Promise constructor support from the
        // active libs / declarations. This is lib-driven, not target-driven:
        // `@target: es2015` with `@lib: es5` still needs the diagnostic.
        if self.ctx.promise_constructor_diagnostics_required() {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::A_DYNAMIC_IMPORT_CALL_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YOU_HAVE,
                diagnostic_codes::A_DYNAMIC_IMPORT_CALL_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YOU_HAVE,
            );
        }

        // Dynamic imports return Promise<typeof module>
        // This creates Promise<ModuleNamespace> where ModuleNamespace contains all exports
        Some(self.get_dynamic_import_type(call))
    }

    /// Handle `unknown` and `never` callee types with appropriate diagnostics.
    /// Returns `Some(type_id)` if the callee type was handled (caller should return),
    /// or `None` to continue with normal call resolution.
    fn check_callee_unknown_or_never(
        &mut self,
        callee_type: TypeId,
        callee_expr: NodeIndex,
        args: &[NodeIndex],
    ) -> Option<TypeId> {
        use crate::call_checker::CallableContext;
        use tsz_parser::parser::syntax_kind_ext;

        // TS18046: Calling an expression of type `unknown` is not allowed.
        // tsc emits TS18046 instead of TS2349 when the callee is `unknown`.
        // Without strictNullChecks, unknown is treated like any (callable, returns any).
        if callee_type == TypeId::UNKNOWN {
            if self.error_is_of_type_unknown(callee_expr) {
                // Still need to check arguments for definite assignment (TS2454)
                let check_excess_properties = false;
                self.collect_call_argument_types_with_context(
                    args,
                    |_i, _arg_count| None,
                    check_excess_properties,
                    None,
                    CallableContext::none(),
                );
                return Some(TypeId::ERROR);
            }
            // Without strictNullChecks, treat unknown like any: callable, returns any
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None,
                check_excess_properties,
                None,
                CallableContext::none(),
            );
            return Some(TypeId::ANY);
        }

        // Calling `never` returns `never` (bottom type propagation).
        // tsc treats `never` as having no call signatures.
        // For method calls (e.g., `a.toFixed()` where `a: never`), TS2339 is already
        // emitted by the property access check, so we suppress the redundant TS2349.
        // For direct calls on `never` (e.g., `f()` where `f: never`), emit TS2349.
        if callee_type == TypeId::NEVER {
            let is_method_call = matches!(
                self.ctx.arena.kind_at(callee_expr),
                Some(
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                )
            );
            if !is_method_call {
                self.error_not_callable_at(callee_type, callee_expr);
            }
            return Some(TypeId::NEVER);
        }

        None
    }
}

// Identifier resolution is in `identifier.rs`.
// Tagged template expression handling is in `tagged_template.rs`.
// TDZ checking, value declaration resolution, and other helpers are in
// `call_helpers.rs`.
