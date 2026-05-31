use crate::call_checker::CallableContext;
use crate::context::TypingRequest;
use crate::query_boundaries::assignability as assign_query;
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::checkers::call::is_type_parameter_type;
use crate::query_boundaries::common;
use crate::query_boundaries::common::CallResult;
use crate::query_boundaries::common::ContextualTypeContext;
use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tracing::trace;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::super::call_result::CallResultContext;
use super::super::complex::is_contextually_sensitive;
use super::post_generic::PostGenericCallDiagnostics;

impl<'a> CheckerState<'a> {
    pub(crate) fn get_type_of_call_expression_inner(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let contextual_type = request.contextual_type;
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return TypeId::ERROR;
        };
        if self.is_unshadowed_commonjs_require_identifier(call.expression)
            && let Some(args) = &call.arguments
            && let Some(first_arg) = args.nodes.first().copied()
            && let Some(module_specifier) = self.get_require_module_specifier(first_arg)
        {
            let module_type =
                self.commonjs_module_value_type(&module_specifier, Some(self.ctx.current_file_idx));
            if let Some(module_type) = module_type {
                return module_type;
            }
            self.emit_module_not_found_error(&module_specifier, first_arg);
            return TypeId::ANY;
        }

        let early_args: &[NodeIndex] = call
            .arguments
            .as_ref()
            .map(|a| a.nodes.as_slice())
            .unwrap_or(&[]);

        // For IIFEs, wrap the contextual type into a callable type so the
        // function expression resolver can extract the return type.
        let iife_info = self.setup_iife_contextual_type(call.expression, contextual_type);
        let higher_order_callee_context = if iife_info.is_none() {
            self.setup_higher_order_callee_contextual_type(
                call.expression,
                contextual_type,
                early_args,
            )
        } else {
            None
        };
        let callee_request = iife_info
            .map(|(wrapper_fn, _)| request.read().contextual(wrapper_fn))
            .or_else(|| {
                higher_order_callee_context.map(|wrapper_fn| request.read().contextual(wrapper_fn))
            })
            .unwrap_or(*request);
        if iife_info.is_some() || higher_order_callee_context.is_some() {
            self.invalidate_expression_for_contextual_retry(call.expression);
        }

        let callee_diag_snap = self.ctx.snapshot_diagnostics();
        let mut callee_type = if let Some(callee_node) = self.ctx.arena.get(call.expression) {
            if callee_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                let identifier_text = self
                    .ctx
                    .arena
                    .get_identifier(callee_node)
                    .map(|ident| ident.escaped_text.as_str())
                    .unwrap_or_default();
                let direct_symbol = self
                    .ctx
                    .binder
                    .node_symbols
                    .get(&call.expression.0)
                    .copied();
                let fast_symbol = direct_symbol
                    .or_else(|| self.resolve_identifier_symbol(call.expression))
                    .filter(|&sym_id| {
                        self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                            let decl_idx = if symbol.value_declaration.is_some() {
                                Some(symbol.value_declaration)
                            } else if symbol.declarations.len() == 1 {
                                symbol.declarations.first().copied()
                            } else {
                                None
                            };
                            self.is_fast_path_function_decl(
                                sym_id,
                                symbol,
                                decl_idx,
                                direct_symbol,
                                identifier_text,
                            )
                        })
                    });
                if let Some(sym_id) = fast_symbol {
                    // Fast path intentionally skips identifier-side diagnostic probes
                    // (e.g. type-only import/value checks). The guard allows local,
                    // non-aliased function declarations in two cases:
                    // - implementation declarations without explicit return annotations
                    // - current-file direct ambient/overload signatures (no body)
                    self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
                    let callee_ty = self.get_type_of_symbol(sym_id);
                    // Cache in node_types so flow narrowing can retrieve callee
                    // type predicates during type guard analysis.
                    if callee_request.is_empty() {
                        self.ctx.node_types.insert(call.expression.0, callee_ty);
                    }
                    callee_ty
                } else {
                    self.get_type_of_node_with_request(call.expression, &callee_request)
                }
            } else {
                self.get_type_of_node_with_request(call.expression, &callee_request)
            }
        } else {
            self.get_type_of_node_with_request(call.expression, &callee_request)
        };

        self.report_checked_js_nullable_this_property_method_call(call.expression);
        let callee_missing_value = callee_type == TypeId::ERROR
            && self.callee_suppresses_contextual_any(call.expression, &callee_diag_snap);

        // When the callee identifier resolves through a type-only alias chain,
        // `report_wrong_meaning` has just emitted TS1361/TS1362 at the callee
        // site. Even if the resolved callee_type still happens to be callable
        // (because the alias merges a namespace value with a function type
        // from the type-only-imported side), tsc treats `typeof <name>` as
        // having no call signatures in this position and emits TS2349 in
        // addition to TS1361/TS1362. Match that so the call site picks up the
        // companion "not callable" diagnostic. See `typeOnlyMerge3.ts`.
        let callee_emitted_type_only_value_error = self
            .ctx
            .speculative_diagnostics_since(&callee_diag_snap)
            .iter()
            .any(|diag| {
                self.ctx
                    .arena
                    .get(call.expression)
                    .is_some_and(|callee_node| {
                        diag.start >= callee_node.pos && diag.start < callee_node.end
                    })
                    && matches!(
                        diag.code,
                        diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_EXPORTED_USING_EXPORT_TYPE
                            | diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_IMPORTED_USING_IMPORT_TYPE
                    )
            });

        // Check for dynamic import module resolution (TS2307)
        if let Some(dynamic_import_type) = self.check_and_resolve_dynamic_import(idx, call) {
            return dynamic_import_type;
        }

        // Special handling for super() calls - treat as construct call
        let is_super_call = self.is_super_expression(call.expression);

        // Get arguments list (may be None for calls without arguments)
        // IMPORTANT: We must check arguments even if callee is ANY/ERROR to catch definite assignment errors
        let args = match call.arguments.as_ref() {
            Some(a) => a.nodes.as_slice(),
            None => &[],
        };
        let explicit_call_type_arguments = call.type_arguments.clone().or_else(|| {
            self.ctx
                .arena
                .get(call.expression)
                .and_then(|node| self.ctx.arena.get_expr_type_args(node))
                .and_then(|expr_type_args| expr_type_args.type_arguments.clone())
        });
        let mut circular_recursive_call_return_type = None;
        let circular_identifier_callee = self.circular_identifier_callee_symbol(call.expression);

        if self.callee_name_conflicts_with_namespace_module(call.expression) {
            self.error_not_callable_at(callee_type, call.expression);
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| Some(TypeId::ANY),
                check_excess_properties,
                None,
                CallableContext::none(),
            );
            return TypeId::ERROR;
        }

        // Check if callee is any/error (don't report for those)
        if callee_type == TypeId::ANY {
            let recursive_function_like_callee = circular_identifier_callee.or_else(|| {
                explicit_call_type_arguments
                    .is_some()
                    .then(|| self.function_like_unannotated_variable_callee_symbol(call.expression))
                    .flatten()
            });
            if let Some(sym_id) = recursive_function_like_callee {
                let type_args: Vec<TypeId> = explicit_call_type_arguments
                    .as_ref()
                    .map(|tl| {
                        tl.nodes
                            .iter()
                            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                            .collect()
                    })
                    .unwrap_or_default();
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                let factory = self.ctx.types.factory();
                let lazy = factory.lazy(def_id);
                let recursive_return_type = if type_args.is_empty() {
                    lazy
                } else {
                    factory.application(lazy, type_args)
                };

                if let Some(validation_callee_type) =
                    self.fresh_function_like_variable_call_type(sym_id)
                {
                    circular_recursive_call_return_type = Some(recursive_return_type);
                    callee_type = validation_callee_type;
                } else {
                    self.collect_call_argument_types_with_context(
                        args,
                        |_, _| Some(TypeId::ANY),
                        false,
                        None,
                        CallableContext::none(),
                    );
                    return recursive_return_type;
                }
            } else {
                if let Some(ref type_args_list) = explicit_call_type_arguments
                    && !type_args_list.nodes.is_empty()
                {
                    // When the callee is a property access on `this` inside a class and
                    // the property doesn't exist, tsc emits TS2339 (property not found)
                    // instead of TS2347 (untyped function calls). The ANY here came from
                    // this_type_stack suppression; check if the property genuinely doesn't
                    // exist and emit TS2339 in that case.
                    let suppressed_ts2347 = self
                        .try_emit_ts2339_for_missing_this_property(call.expression)
                        || self.is_this_property_access_in_class_context(call.expression);
                    if !suppressed_ts2347 {
                        self.error_at_node(
                        idx,
                        crate::diagnostics::diagnostic_messages::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                        crate::diagnostics::diagnostic_codes::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                    );
                    }
                    // Resolve type arguments even though the call is untyped. Without
                    // this, unresolved type names in arguments (e.g.
                    // `g<InvalidReference>()`) silently succeed — tsc still emits
                    // TS2304 for them. Mirrors the matching block in generic_checker.
                    for &type_arg_idx in &type_args_list.nodes {
                        self.get_type_of_node(type_arg_idx);
                    }
                }
                // Untyped calls accept ordinary args; callbacks still get their own context for TS7006.
                let cb_args: Vec<_> = args
                    .iter()
                    .map(|&idx| self.is_callback_like_argument(idx))
                    .collect();
                self.collect_call_argument_types_with_context(
                    args,
                    |i, _arg_count| (!matches!(cb_args.get(i), Some(true))).then_some(TypeId::ANY),
                    false,
                    None, // No skipping needed
                    CallableContext::none(),
                );
                return TypeId::ANY;
            }
        }
        if callee_type == TypeId::ERROR
            && let Some(recovered_type) = self.recover_declared_type_for_tdz_callee(call.expression)
        {
            callee_type = recovered_type;
        }

        if callee_type == TypeId::ERROR {
            // Circular identifiers and explicit type-argument calls to unannotated
            // function-like variables can resolve through a temporary ERROR/ANY
            // placeholder. Preserve those recursive calls as App(Lazy(def_id),
            // type_args) for depth-limited DTS expansion instead of collapsing
            // to any/ERROR.
            let circular_sym = circular_identifier_callee.or_else(|| {
                explicit_call_type_arguments
                    .is_some()
                    .then(|| self.function_like_unannotated_variable_callee_symbol(call.expression))
                    .flatten()
            });

            if let Some(sym_id) = circular_sym {
                let type_args: Vec<TypeId> = explicit_call_type_arguments
                    .as_ref()
                    .map(|tl| {
                        tl.nodes
                            .iter()
                            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                            .collect()
                    })
                    .unwrap_or_default();
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                let factory = self.ctx.types.factory();
                let lazy = factory.lazy(def_id);
                let recursive_return_type = if type_args.is_empty() {
                    lazy
                } else {
                    factory.application(lazy, type_args)
                };

                if let Some(validation_callee_type) =
                    self.fresh_function_like_variable_call_type(sym_id)
                {
                    circular_recursive_call_return_type = Some(recursive_return_type);
                    callee_type = validation_callee_type;
                } else {
                    self.collect_call_argument_types_with_context(
                        args,
                        |_, _| Some(TypeId::ANY),
                        false,
                        None,
                        CallableContext::none(),
                    );
                    return recursive_return_type;
                }
            } else {
                self.reemit_namespace_value_error_for_call_callee(call.expression);
                // Still evaluate type arguments to catch TS2304 for unresolved type names
                // (e.g., `this.super<T>(0)` where T is undeclared)
                if let Some(ref type_args_list) = explicit_call_type_arguments {
                    for &arg_idx in &type_args_list.nodes {
                        self.get_type_from_type_node(arg_idx);
                    }
                }
                // Still need to check arguments for definite assignment (TS2454) and other
                // errors. When the callee itself failed name/value resolution, avoid
                // fabricating contextual `any` for callback arguments because that would
                // suppress real TS7006 diagnostics. Other callee errors still preserve the
                // historical `any` fallback to avoid broader conformance regressions.
                let check_excess_properties = false;
                self.collect_call_argument_types_with_context(
                    args,
                    |i, _arg_count| {
                        if !callee_missing_value {
                            return Some(TypeId::ANY);
                        }
                        args.get(i)
                            .copied()
                            .and_then(|arg_idx| self.ctx.arena.get(arg_idx))
                            .filter(|arg_node| arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT)
                            .map(|_| TypeId::ANY)
                    },
                    check_excess_properties,
                    None, // No skipping needed
                    CallableContext::none(),
                );
                return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
            }
        }

        // Handle unknown/never callee types as early returns.
        if let Some(early_return) =
            self.check_callee_unknown_or_never(callee_type, call.expression, args)
        {
            return early_return;
        }

        // tsc companion-emits TS2349 ("This expression is not callable. Type
        // 'typeof X' has no call signatures.") alongside TS1361/TS1362 when a
        // type-only-aliased identifier is used as a call target. tsz keeps
        // the underlying callable on the resolved type (because the alias
        // chain merged a namespace value with a function-typed type-only
        // import), so the call would otherwise resolve to Success and the
        // accompanying TS2349 would be missing. See `typeOnlyMerge3.ts`.
        if callee_emitted_type_only_value_error {
            // Still evaluate arguments so downstream definite-assignment /
            // unresolved-name diagnostics still fire on argument sites.
            return self.error_not_callable_and_collect_any_args(
                callee_type,
                call.expression,
                args,
            );
        }

        if self.callee_name_conflicts_with_namespace_module(call.expression) {
            self.error_not_callable_at(callee_type, call.expression);
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| Some(TypeId::ANY),
                check_excess_properties,
                None,
                CallableContext::none(),
            );
            return TypeId::ERROR;
        }

        let mut nullish_cause = None;
        // A call is in an optional chain when it uses `?.()` directly, or when
        // the callee expression continues an earlier optional chain such as
        // `o?.a.b()`.
        let callee_is_optional_chain = node.is_optional_chain()
            || crate::types_domain::computation::access::is_optional_chain(
                self.ctx.arena,
                call.expression,
            );
        if callee_is_optional_chain {
            // Evaluate the callee type to resolve Application/Lazy types before
            // splitting nullish members. Without this, `Transform1<T>` stays as an
            // unevaluated Application and split_nullish_type can't see its union members.
            let callee_for_split = self.evaluate_type_with_env(callee_type);
            let (non_nullish, cause) = self.split_nullish_type(callee_for_split);
            nullish_cause = cause;
            let Some(non_nullish) = non_nullish else {
                return TypeId::UNDEFINED;
            };
            callee_type = non_nullish;
            if callee_type == TypeId::ANY {
                return TypeId::ANY;
            }
            if callee_type == TypeId::ERROR {
                return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
            }
        }

        // args is already defined above before the ANY/ERROR check

        // Validate explicit type arguments against constraints (TS2344)
        let mut type_arg_validation = crate::generic_checker::CallTypeArgumentValidation::default();
        if let Some(ref type_args_list) = explicit_call_type_arguments
            && !type_args_list.nodes.is_empty()
        {
            let validation_callee_type = if matches!(
                query::classify_for_call_signatures(self.ctx.types, callee_type),
                query::CallSignaturesKind::MultipleSignatures(_)
            ) {
                callee_type
            } else {
                self.direct_function_call_type_for_type_argument_validation(call.expression)
                    .unwrap_or(callee_type)
            };
            type_arg_validation =
                self.validate_call_type_arguments(validation_callee_type, type_args_list, idx);

            // `super<T>(...)` is always invalid (TS2754). Don't proceed with
            // argument checking — it would emit a false TS2554 because the
            // type-arg application fails and the resolved constructor has a
            // different parameter shape than the user intended.
            if is_super_call {
                // Still evaluate argument expressions for side-effect errors
                // (definite assignment, etc.) but don't type-check them against
                // the constructor signature.
                let check_excess_properties = false;
                self.collect_call_argument_types_with_context(
                    args,
                    |_i, _arg_count| Some(TypeId::ANY),
                    check_excess_properties,
                    None,
                    CallableContext::none(),
                );
                return TypeId::VOID;
            }
        }

        // When explicit type arguments are invalid, don't proceed with argument
        // type checking against the incorrectly-instantiated signature. tsc
        // reports the type-argument problem and suppresses cascading TS2345
        // argument diagnostics for that call.
        if type_arg_validation.count_mismatch || type_arg_validation.constraint_violation {
            // Still evaluate argument expressions for side-effect errors
            // (definite assignment, etc.) but don't type-check them against
            // the function signature.
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| Some(TypeId::ANY),
                check_excess_properties,
                None,
                CallableContext::none(),
            );
            if let Some(return_type) = circular_recursive_call_return_type {
                return return_type;
            }
            // Try to recover a return type for downstream type checking
            if let Some(return_type) =
                crate::query_boundaries::checkers::call::stable_call_recovery_return_type(
                    self.ctx.types,
                    callee_type,
                )
            {
                return return_type;
            }
            return TypeId::ERROR;
        }

        // Apply explicit type arguments to the callee type before checking arguments.
        // This ensures that when we have `fn<T>(x: T)` and call it as `fn<number>("string")`,
        // the parameter type becomes `number` (after substituting T=number), and we can
        // correctly check if `"string"` is assignable to `number`.
        let mut callee_type_for_resolution = if explicit_call_type_arguments.is_some() {
            self.apply_type_arguments_to_callable_type(
                callee_type,
                explicit_call_type_arguments.as_ref(),
            )
        } else {
            callee_type
        };

        // Resolve Lazy(DefId) and Application types before overload classification.
        // Interface-typed callees are stored as Lazy(DefId) which classify_for_call_signatures
        // doesn't handle, causing the overloaded path to be skipped and literal arguments
        // to be widened to `string` instead of matching specialized signatures.
        let mut resolved_for_classification =
            self.evaluate_application_type(callee_type_for_resolution);
        resolved_for_classification = self.resolve_lazy_type(resolved_for_classification);
        let mut classification =
            query::classify_for_call_signatures(self.ctx.types, resolved_for_classification);
        if matches!(classification, query::CallSignaturesKind::NoSignatures)
            && let Some(annotated_callee_type) =
                self.explicit_identifier_callee_annotation_type(call.expression)
        {
            callee_type_for_resolution = if explicit_call_type_arguments.is_some() {
                self.apply_type_arguments_to_callable_type(
                    annotated_callee_type,
                    explicit_call_type_arguments.as_ref(),
                )
            } else {
                annotated_callee_type
            };
            resolved_for_classification =
                self.evaluate_application_type(callee_type_for_resolution);
            resolved_for_classification = self.resolve_lazy_type(resolved_for_classification);
            classification =
                query::classify_for_call_signatures(self.ctx.types, resolved_for_classification);
        }
        if matches!(classification, query::CallSignaturesKind::NoSignatures)
            && let Some(direct_callee_type) =
                self.direct_function_call_type_for_type_argument_validation(call.expression)
        {
            callee_type_for_resolution = if explicit_call_type_arguments.is_some() {
                self.apply_type_arguments_to_callable_type(
                    direct_callee_type,
                    explicit_call_type_arguments.as_ref(),
                )
            } else {
                direct_callee_type
            };
            resolved_for_classification =
                self.evaluate_application_type(callee_type_for_resolution);
            resolved_for_classification = self.resolve_lazy_type(resolved_for_classification);
            classification =
                query::classify_for_call_signatures(self.ctx.types, resolved_for_classification);
        }
        trace!(
            callee_type_for_resolution = ?callee_type_for_resolution,
            classification = ?classification,
            "Call signatures classified"
        );
        // When the callee is a Union type, do NOT treat the collected member
        // signatures as overloads. Union call semantics require the call to be
        // valid for ALL members (handled by solver's resolve_union_call), while
        // overload resolution accepts the call if ANY single signature matches.
        // Without this guard, `(F1 | F2)("a")` would succeed if F1 alone accepts
        // 1 arg, silently ignoring F2 which requires 2 args — missing TS2554.
        let callee_is_union = common::is_union_type(self.ctx.types, resolved_for_classification);
        let overload_signatures = if callee_is_union {
            None
        } else {
            match classification {
                query::CallSignaturesKind::Callable(_) => {
                    // Delegate to solver query for overload detection
                    call_checker::get_overload_call_signatures(
                        self.ctx.types,
                        resolved_for_classification,
                    )
                }
                query::CallSignaturesKind::MultipleSignatures(signatures) => {
                    (signatures.len() > 1).then_some(signatures)
                }
                query::CallSignaturesKind::NoSignatures => None,
            }
        };

        // Unwrap parentheses, non-null assertions, and type assertions from the
        // callee expression to find the underlying property/element access.
        // This ensures `o.test!()`, `(o.test)()`, `(o.test!)()` etc. are all
        // recognized as method calls with `o` as the `this` receiver.
        let unwrapped_callee = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(call.expression);

        // Overload candidates need signature-specific contextual typing.
        let force_bivariant_callbacks = matches!(
            self.ctx.arena.kind_at(unwrapped_callee),
            Some(
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            )
        );

        let mut actual_this_type = None;
        if let Some(callee_node) = self.ctx.arena.get(unwrapped_callee) {
            use tsz_parser::parser::syntax_kind_ext;
            if (callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.ctx.arena.get_access_expr(callee_node)
            {
                let receiver_type = self.get_type_of_node(access.expression);
                actual_this_type = Some(if nullish_cause.is_some() {
                    let evaluated = self.evaluate_type_with_env(receiver_type);
                    let (non_nullish, _) = self.split_nullish_type(evaluated);
                    non_nullish.unwrap_or(evaluated)
                } else {
                    receiver_type
                });
            }
        }

        if let Some(signatures) = overload_signatures.as_deref()
            && let Some(overload_resolution) = self.resolve_overloaded_call_with_signatures(
                args,
                signatures,
                force_bivariant_callbacks,
                contextual_type,
                actual_this_type,
            )
        {
            trace!(
                result = ?overload_resolution.result,
                signatures_count = signatures.len(),
                "Resolved overloaded call return type"
            );
            if let Some(predicate) = overload_resolution.selected_type_predicate.clone() {
                self.store_call_type_predicate(idx, call.expression, predicate);
            }
            return self.handle_call_result(
                overload_resolution.result,
                CallResultContext {
                    callee_expr: call.expression,
                    call_idx: idx,
                    args,
                    arg_types: &overload_resolution.arg_types,
                    callee_type: callee_type_for_resolution,
                    callee_has_declared_generic_signature: false,
                    is_super_call: false,
                    is_optional_chain: nullish_cause.is_some(),
                    allow_contextual_mismatch_deferral: false,
                },
            );
        }

        // Resolve Lazy/Application types before creating the contextual context.
        // This ensures that when the callee is an interface type (stored as Lazy(DefId))
        // or a generic interface application (Application(Lazy, args)), the contextual
        // type context can properly extract parameter types from the resolved Callable shape.
        //
        // Without this resolution, ContextualTypeContext's get_parameter_type_for_call
        // calls evaluate_type (with NoopResolver) on the Lazy type, which returns the
        // Lazy type unchanged (NoopResolver.resolve_lazy returns None). The extractor
        // then falls back to default None output because visit_lazy is not overridden.
        // This causes false TS7006 emissions for callbacks passed to interface-typed callees.
        //
        // Examples that were wrongly emitting TS7006:
        //   interface Fn { (fn: (x: number) => void): void }
        //   declare const fn: Fn;
        //   fn(x => {});  // x was typed as any (false positive)
        let callee_type_for_context = self.evaluate_application_type(callee_type_for_resolution);
        let callee_type_for_context = self.resolve_lazy_type(callee_type_for_context);
        let callee_type_for_context = self.evaluate_contextual_type(callee_type_for_context);
        // Extract the shape from the same resolved callee type used for contextual typing.
        // Using a less-resolved form here can make Round 2 infer from a pre-instantiation
        // method signature even though callback contextual typing is based on the fully
        // resolved receiver-specific callable type.
        let mut callee_shape = call_checker::get_contextual_signature_for_arity(
            self.ctx.types,
            callee_type_for_context,
            args.len(),
        )
        .or_else(|| {
            call_checker::get_call_signature(self.ctx.types, callee_type_for_context, args.len())
        });
        if let Some(shape) = callee_shape.take() {
            callee_shape =
                Some(self.refresh_callee_shape_type_param_constraints(call.expression, shape));
        }
        let original_callee_shape = callee_shape.clone();
        let is_generic_call = callee_shape
            .as_ref()
            .is_some_and(|s| !s.type_params.is_empty())
            && explicit_call_type_arguments.is_none(); // Only use two-pass if no explicit type args
        // Create contextual context from resolved callee type
        let ctx_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            callee_type_for_context,
            self.ctx.compiler_options.no_implicit_any,
        );
        let base_contextual_param_types: Vec<Option<TypeId>> = (0..args.len())
            .map(|i| {
                self.contextual_parameter_type_for_call_with_env_from_expected(
                    callee_type_for_context,
                    i,
                    args.len(),
                )
                .or_else(|| ctx_helper.get_parameter_type_for_call(i, args.len()))
                .map(|param_type| self.normalize_contextual_call_param_type(param_type))
            })
            .collect();
        // For union callees, skip excess property checking during argument collection.
        // The solver's resolve_union_call intersects parameter types across members,
        // so `{x: 0, y: 0}` is valid for `((a: {x}) => R) | ((a: {y}) => R)` even
        // though it has "excess" properties against each individual member type.
        let check_excess_properties = overload_signatures.is_none() && !callee_is_union;
        // Two-pass argument collection for generic calls is only needed when at least one
        // argument is contextually sensitive; preserve literals for contextual object/array args.
        let prev_preserve_literals = self.ctx.preserve_literal_types;
        let prev_generic_excess_skip = self.ctx.generic_excess_skip.take();
        let callable_ctx = CallableContext::new(callee_type_for_context);
        let union_call_has_literal_argument = callee_is_union
            && args.iter().any(|&arg_idx| {
                self.ctx
                    .arena
                    .get(self.ctx.arena.skip_parenthesized_and_assertions(arg_idx))
                    .is_some_and(|node| {
                        node.kind == SyntaxKind::StringLiteral as u16
                            || node.kind == SyntaxKind::NumericLiteral as u16
                            || node.kind == SyntaxKind::BigIntLiteral as u16
                            || node.kind == SyntaxKind::TrueKeyword as u16
                            || node.kind == SyntaxKind::FalseKeyword as u16
                            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    })
            });
        if is_generic_call
            || union_call_has_literal_argument
            || args.iter().enumerate().any(|(i, &arg_idx)| {
                base_contextual_param_types
                    .get(i)
                    .copied()
                    .flatten()
                    .is_some()
                    && self
                        .ctx
                        .arena
                        .get(self.ctx.arena.skip_parenthesized_and_assertions(arg_idx))
                        .is_some_and(|node| {
                            node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                || node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        })
            })
        {
            self.ctx.preserve_literal_types = true;
        }
        // Kept in a separate shard to respect the repo-wide source file ceiling.
        let (
            non_generic_contextual_types,
            pushed_this_type_from_shape,
            had_return_context_substitution,
            checker_round2_substitution,
            checker_round2_shape,
            direct_literal_conflict_substitution,
            mut arg_types,
        ) = include!("inner_argument_collection.rs");
        self.ctx.preserve_literal_types = prev_preserve_literals;
        // NOTE: generic_excess_skip is NOT restored here. It's kept until after all
        // excess property checks are done (including recovery paths and handle_call_result).
        // It's restored right before handle_call_result at the end of this function.
        // Keep shape_this_type on the stack through finalize_generic_call_result
        // and handle_call_result. Without this, post-inference rechecks triggered by
        // the call result handler would see an empty this_type_stack and fall back to
        // the wrong contextual type, causing false TS2339 errors.
        // We pop it at the end of this function.
        self.ensure_relation_input_ready(callee_type_for_resolution);

        // Resolve applications/lazy refs to callable forms before solver dispatch.
        let callee_type_for_call = self.evaluate_application_type(callee_type_for_resolution);
        let callee_type_for_call = self.resolve_lazy_type(callee_type_for_call);
        // For union types, resolve Lazy members so the solver can inspect their
        // callable shapes (e.g., for `this` type checks in TS2684). The solver's
        // NoopResolver can't resolve Lazy types, so we do it here.
        let callee_type_for_call = self.resolve_lazy_members_in_union(callee_type_for_call);

        // Boxed/global `Function` is callable in TS even without explicit signatures.
        let callee_type_for_call =
            self.replace_function_type_for_call(callee_type, callee_type_for_call);
        if callee_type_for_call == TypeId::ANY {
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None,
                check_excess_properties,
                None, // No skipping needed
                CallableContext::none(),
            );
            return if nullish_cause.is_some() {
                common::union_with_undefined(self.ctx.types, TypeId::ANY)
            } else {
                TypeId::ANY
            };
        }

        self.ensure_relation_input_ready(callee_type_for_call);

        // `super()` uses construct signatures, not call signatures.
        let (generic_inference_arg_types, sanitized_generic_inference) = if is_generic_call {
            self.sanitize_generic_inference_arg_types(call.expression, args, &arg_types)
        } else {
            (std::borrow::Cow::Borrowed(arg_types.as_slice()), false)
        };
        let generic_inference_arg_source_markers = if is_generic_call {
            self.call_arg_source_type_annotation_markers(args, generic_inference_arg_types.len())
        } else {
            Vec::new()
        };
        let call_resolution_contextual_type = if is_generic_call {
            // Generic calls in contextual positions need the outer request at the
            // solver boundary, even when they have arguments. The checker-side
            // round-1/round-2 passes refine argument shapes, but higher-order
            // cases like `map(xs, identity)`, `compose(list, box)`, and
            // `consumeClass(createClass(x => ...))` still require return-context
            // seeding in the final generic solve step to instantiate parameter and
            // callback types from the contextual result.
            contextual_type
        } else {
            contextual_type
        };

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
            } else if generic_inference_arg_source_markers.iter().any(|&m| m) {
                self.resolve_call_with_checker_adapter_and_arg_sources(
                    callee_type_for_call,
                    &generic_inference_arg_types,
                    force_bivariant_callbacks,
                    call_resolution_contextual_type,
                    actual_this_type,
                    &generic_inference_arg_source_markers,
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
        // When the checker's intra-expression Round 2 produced a substitution that
        // pins type parameters the solver could not (the solver's single-pass
        // inference dropped the binding because the same parameter appears in a
        // homomorphic-mapped + `infer` return position that fails reverse
        // inference), refine `instantiated_params` so the post-call assignability
        // recheck sees the tighter expected types. We only override when the
        // solver effectively defaulted to the type parameter's constraint.
        if is_generic_call
            && !is_super_call
            && let Some(checker_sub) = checker_round2_substitution.as_ref()
            && let Some(orig_shape) = checker_round2_shape.as_ref()
            && let Some(params) = generic_instantiated_params.as_mut()
        {
            self.refine_instantiated_params_with_checker_substitution(
                orig_shape,
                params,
                checker_sub,
            );
        }
        if is_generic_call
            && !is_super_call
            && let Some(conflicts) = direct_literal_conflict_substitution.as_ref()
            && let Some(orig_shape) = checker_round2_shape.as_ref()
            && let Some(params) = generic_instantiated_params.as_mut()
        {
            self.refine_bare_instantiated_params_with_direct_literal_conflicts(
                orig_shape, params, conflicts,
            );
        }
        let needs_real_type_recheck = is_generic_call
            && (!is_super_call
                || args.iter().enumerate().any(|(i, &arg_idx)| {
                    self.argument_needs_refresh_for_contextual_call(
                        arg_idx,
                        base_contextual_param_types.get(i).copied().flatten(),
                    )
                }));

        if !is_generic_call
            && let CallResult::ArgumentTypeMismatch {
                index,
                fallback_return,
                ..
            } = result.clone()
            && let Some(expected) = non_generic_contextual_types
                .as_ref()
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
                // Skip excess property checking when the original parameter type was a
                // type parameter (captured via generic_excess_skip during arg collection).
                let skip_epc_for_generic = self
                    .ctx
                    .generic_excess_skip
                    .as_ref()
                    .is_some_and(|skip| index < skip.len() && skip[index]);
                if expected != TypeId::ANY
                    && expected != TypeId::UNKNOWN
                    && !is_type_parameter_type(self.ctx.types, expected)
                    && !skip_epc_for_generic
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
                    result = CallResult::Success(return_type);
                }
            }
        }

        let retry_contextual_param_types = if is_generic_call && had_return_context_substitution {
            generic_instantiated_params.as_ref().map(|params| {
                self.contextual_param_types_from_instantiated_params(params, args.len())
            })
        } else {
            None
        };
        let has_contextual_signature_instantiation_arg =
            args.iter().enumerate().any(|(i, &arg_idx)| {
                let expected_type = retry_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(i).copied().flatten())
                    .or_else(|| base_contextual_param_types.get(i).copied().flatten());
                self.expression_needs_contextual_signature_instantiation(arg_idx, expected_type)
            });
        let has_contextual_refresh_arg = args.iter().enumerate().any(|(i, &arg_idx)| {
            self.argument_needs_refresh_for_contextual_call(
                arg_idx,
                retry_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(i).copied().flatten())
                    .or_else(|| base_contextual_param_types.get(i).copied().flatten()),
            )
        });
        let should_retry_generic_call = if is_generic_call
            && (!had_return_context_substitution || has_contextual_signature_instantiation_arg)
            && has_contextual_refresh_arg
        {
            if let Some(ctx_type) = contextual_type {
                match &result {
                    CallResult::Success(ret) => {
                        let contextual_return = self.evaluate_contextual_type(ctx_type);
                        !self
                            .assign_relation_outcome_with_env(*ret, contextual_return)
                            .related
                    }
                    _ => true,
                }
            } else {
                true
            }
        } else {
            false
        };

        let mut retried_arg_types = None;
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
            let instantiated_params = call_checker::get_contextual_signature_for_arity(
                self.ctx.types,
                callee_type_for_call,
                args.len(),
            )
            .map(|shape| {
                self.resolve_signature_parameter_type_queries(&shape.params, instantiated_params)
            })
            .unwrap_or_else(|| instantiated_params.clone());
            let refreshed_contextual_types = self
                .contextual_param_types_from_instantiated_params(&instantiated_params, args.len())
                .into_iter()
                .map(|param_type| {
                    param_type
                        .map(|param_type| self.normalize_contextual_call_param_type(param_type))
                })
                .collect::<Vec<_>>();
            let retry_arg_diag_snap = self.ctx.snapshot_diagnostics();
            let refreshed_arg_types = self.collect_call_argument_types_with_context(
                args,
                |i, _arg_count| {
                    refreshed_contextual_types
                        .get(i)
                        .copied()
                        .flatten()
                        // A `never` contextual type is uninformative: it only
                        // arises when the instantiated parameter reduced to `never`
                        // (a forbidden argument). Using it to re-type the argument
                        // would spuriously widen a literal (`'a'` -> `string`) and
                        // mask the TS2345 the first resolve already found. Fall back
                        // to the base contextual type instead.
                        .filter(|&t| t != TypeId::NEVER)
                        .or_else(|| base_contextual_param_types.get(i).copied().flatten())
                },
                check_excess_properties,
                None,
                callable_ctx,
            );
            let retry_has_callback_body_errors =
                self.overload_candidate_has_callback_body_errors(args, &retry_arg_diag_snap);
            let retry_has_callback_like_arg = args
                .iter()
                .copied()
                .any(|arg_idx| self.is_callback_like_argument(arg_idx));

            let (retry_generic_arg_types, retry_sanitized) = self
                .sanitize_generic_inference_arg_types(call.expression, args, &refreshed_arg_types);
            let retry_arg_source_markers =
                self.call_arg_source_type_annotation_markers(args, retry_generic_arg_types.len());
            let mut retry = if is_super_call {
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
            } else if retry_arg_source_markers.iter().any(|&m| m) {
                self.resolve_call_with_checker_adapter_and_arg_sources(
                    callee_type_for_call,
                    &retry_generic_arg_types,
                    force_bivariant_callbacks,
                    contextual_type,
                    actual_this_type,
                    &retry_arg_source_markers,
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
            // Apply the same checker-side substitution refinement to the retry's
            // freshly-inferred params, so the recheck below sees the tighter
            // expected types for the post-call assignability check.
            if let Some(checker_sub) = checker_round2_substitution.as_ref()
                && let Some(orig_shape) = checker_round2_shape.as_ref()
                && let Some(retry_params) = retry.2.as_mut()
            {
                self.refine_instantiated_params_with_checker_substitution(
                    orig_shape,
                    retry_params,
                    checker_sub,
                );
            }
            if let Some(conflicts) = direct_literal_conflict_substitution.as_ref()
                && let Some(orig_shape) = checker_round2_shape.as_ref()
                && let Some(retry_params) = retry.2.as_mut()
            {
                self.refine_bare_instantiated_params_with_direct_literal_conflicts(
                    orig_shape,
                    retry_params,
                    conflicts,
                );
            }
            result = if (retry_sanitized || needs_real_type_recheck)
                && !retry_has_callback_body_errors
                && !retry_has_callback_like_arg
            {
                if let Some(instantiated_params) = retry.2.as_ref() {
                    self.recheck_generic_call_arguments_with_real_types(
                        retry.0.clone(),
                        instantiated_params,
                        args,
                        &refreshed_arg_types,
                    )
                } else {
                    retry.0
                }
            } else {
                retry.0
            };
            instantiated_predicate = retry.1;
            generic_instantiated_params = retry.2;
            retried_arg_types = Some(refreshed_arg_types);
        }

        if is_generic_call
            && let CallResult::Success(return_type) = result
            && let Some(ctx_type) =
                contextual_type.filter(|&ct| ct != TypeId::ANY && ct != TypeId::UNKNOWN)
            && let Some(shape) = call_checker::get_contextual_signature_for_arity(
                self.ctx.types,
                callee_type_for_call,
                args.len(),
            )
        {
            let mut return_context_substitution =
                self.compute_return_context_substitution_from_shape(&shape, Some(ctx_type));
            let return_param_names: FxHashSet<_> = self
                .function_like_return_parameter_type_params(&shape)
                .into_iter()
                .collect();
            let same_return_context_application =
                common::application_info(self.ctx.types, shape.return_type)
                    .zip(common::application_info(self.ctx.types, ctx_type))
                    .is_some_and(|((return_base, _), (ctx_base, _))| return_base == ctx_base);
            let return_context_specializes_return_params = !return_param_names.is_empty()
                && self.contextual_return_type_specializes_wrapped_params(
                    shape.return_type,
                    ctx_type,
                    &return_param_names,
                    &mut FxHashSet::default(),
                );
            if !return_param_names.is_empty()
                && !same_return_context_application
                && !return_context_specializes_return_params
            {
                let mut filtered = crate::query_boundaries::common::TypeSubstitution::new();
                for (&name, &type_id) in return_context_substitution.map() {
                    if !return_param_names.contains(&name) {
                        filtered.insert(name, type_id);
                    }
                }
                return_context_substitution = filtered;
            }

            if !return_context_substitution.is_empty() {
                let has_callback_like_arg = args
                    .iter()
                    .copied()
                    .any(|arg| self.is_callback_like_argument(arg));
                let contextual_return_is_concrete =
                    !common::contains_type_parameters(self.ctx.types, ctx_type)
                        && !common::contains_infer_types(self.ctx.types, ctx_type)
                        && !common::contains_type_by_id(self.ctx.types, ctx_type, TypeId::UNKNOWN);
                if !has_callback_like_arg && contextual_return_is_concrete {
                    let instantiated_shape_return =
                        crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            shape.return_type,
                            &return_context_substitution,
                        );
                    let contextual_params_fit_args = args.iter().enumerate().all(|(i, _)| {
                        let Some(param) = shape.params.get(i).or_else(|| {
                            let last = shape.params.last()?;
                            last.rest.then_some(last)
                        }) else {
                            return true;
                        };
                        let instantiated_param = crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            param.type_id,
                            &return_context_substitution,
                        );
                        let expected = if param.rest {
                            self.rest_argument_element_type_with_env(instantiated_param)
                        } else {
                            instantiated_param
                        };
                        let actual = generic_inference_arg_types
                            .get(i)
                            .copied()
                            .or_else(|| {
                                retried_arg_types
                                    .as_ref()
                                    .and_then(|types| types.get(i).copied())
                            })
                            .or_else(|| arg_types.get(i).copied())
                            .unwrap_or(TypeId::UNKNOWN);
                        if matches!(actual, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
                            return false;
                        }
                        self.assign_relation_outcome_with_env(actual, expected)
                            .related
                    });
                    if contextual_params_fit_args
                        && self
                            .assign_relation_outcome_with_env(instantiated_shape_return, ctx_type)
                            .related
                    {
                        result = CallResult::Success(instantiated_shape_return);
                    }
                }
                if let CallResult::Success(current_return) = result
                    && current_return == return_type
                    && (common::contains_type_parameters(self.ctx.types, return_type)
                        || common::contains_infer_types(self.ctx.types, return_type)
                        || common::contains_type_by_id(
                            self.ctx.types,
                            return_type,
                            TypeId::UNKNOWN,
                        ))
                {
                    let instantiated_return = crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        return_type,
                        &return_context_substitution,
                    );
                    if instantiated_return != return_type {
                        result = CallResult::Success(instantiated_return);
                    }
                }
                if let CallResult::Success(current_return) = result
                    && current_return != shape.return_type
                    && common::contains_type_by_id(self.ctx.types, current_return, TypeId::UNKNOWN)
                {
                    let instantiated_return = crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        shape.return_type,
                        &return_context_substitution,
                    );
                    if instantiated_return != shape.return_type
                        && self
                            .assign_relation_outcome_with_env(instantiated_return, ctx_type)
                            .related
                    {
                        result = CallResult::Success(instantiated_return);
                    }
                }
            }
        }
        drop(generic_inference_arg_types);
        if let Some(refreshed_arg_types) = retried_arg_types {
            arg_types = refreshed_arg_types;
        }

        // Store instantiated type predicate from generic call resolution
        // so flow narrowing can use the correct (inferred) predicate type.
        let stored_call_predicate = if let Some(predicate) = instantiated_predicate {
            let stored_predicate =
                call_checker::extract_predicate_signature(self.ctx.types, callee_type_for_call)
                    .filter(|sig| {
                        // Only defer to `resolve_generic_predicate` when the type parameter
                        // actually appears in a parameter type; otherwise use the instantiated
                        // predicate directly (T appears only in the predicate, not in params).
                        sig.predicate.type_id.is_some_and(|pred_ty| {
                            common::type_param_info(self.ctx.types, pred_ty).is_some_and(
                                |tp_info| {
                                    sig.params.iter().any(|p| {
                                        common::contains_type_parameter_named(
                                            self.ctx.types,
                                            p.type_id,
                                            tp_info.name,
                                        )
                                    })
                                },
                            )
                        })
                    })
                    .map(|sig| (sig.predicate, sig.params))
                    .unwrap_or(predicate);
            Some(stored_predicate)
        } else {
            // For non-generic calls with type predicates (e.g., `isString(x): x is string`),
            // extract the predicate from the callee's signature and store it in
            // call_type_predicates. This ensures flow narrowing can find the predicate
            // even when node_types is temporarily emptied during overload resolution
            // of a containing call expression (e.g., `console.log(thing.toUpperCase())`
            // triggers overload resolution which empties node_types before checking args).
            let is_sound_union = if common::is_union_type(self.ctx.types, callee_type_for_call) {
                call_checker::is_valid_union_predicate(self.ctx.types, callee_type_for_call)
            } else {
                true
            };
            if is_sound_union
                && let Some(extracted) =
                    call_checker::extract_predicate_signature(self.ctx.types, callee_type_for_call)
            {
                Some((extracted.predicate, extracted.params))
            } else {
                None
            }
        };

        if let Some(stored_predicate) = stored_call_predicate {
            self.store_call_type_predicate(idx, call.expression, stored_predicate);
        }

        let (mut result, mut allow_contextual_mismatch_deferral) = self
            .finalize_generic_call_result(super::super::call_finalize::GenericCallFinalizeCtx {
                callee_type_for_call,
                generic_instantiated_params: generic_instantiated_params.as_ref(),
                args,
                arg_types: &arg_types,
                result,
                sanitized_generic_inference,
                needs_real_type_recheck,
            });
        let finalized_contextual_param_types = generic_instantiated_params
            .as_ref()
            .map(|params| self.contextual_param_types_from_instantiated_params(params, args.len()));
        self.run_post_generic_call_diagnostics(PostGenericCallDiagnostics {
            result: &mut result,
            allow_contextual_mismatch_deferral: &mut allow_contextual_mismatch_deferral,
            callee_type_for_call,
            args,
            arg_types: &arg_types,
            base_contextual_param_types: &base_contextual_param_types,
            finalized_contextual_param_types: finalized_contextual_param_types.as_deref(),
            original_callee_shape: original_callee_shape.as_ref(),
            emit_unknown_callback_body_diagnostics: is_generic_call && contextual_type.is_none(),
            check_excess_properties,
            callable_ctx,
        });
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
                if let CallResult::Success(return_type) = result {
                    allow_contextual_mismatch_deferral = false;
                    result = CallResult::ArgumentTypeMismatch {
                        index,
                        expected,
                        actual,
                        fallback_return: return_type,
                    };
                }
            })
            .is_some();
        if forced_block_body_callback_mismatch {
            allow_contextual_mismatch_deferral = false;
        }
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
                if matches!(result, CallResult::Success(_))
                    && let Some(&arg_idx) = args.get(index)
                {
                    allow_contextual_mismatch_deferral = false;
                    self.error_argument_not_assignable_at(actual, expected, arg_idx);
                }
            })
            .is_some();
        if let CallResult::ArgumentTypeMismatch {
            actual: _,
            expected: _,
            fallback_return,
            ..
        } = result
            && !forced_block_body_callback_mismatch
            && !forced_binding_pattern_unknown_context_mismatch
            && fallback_return != TypeId::ERROR
        {
            // Keep the ArgumentTypeMismatch result to ensure TS2345 is emitted
            // Deferral logic removed to fix missing TS2345 errors
        }

        if let CallResult::ArgumentTypeMismatch {
            fallback_return, ..
        } = result
            && self.call_is_simple_evolving_array_mutation(call.expression)
        {
            result = CallResult::Success(fallback_return);
        }

        if let CallResult::ArgumentTypeMismatch {
            index,
            fallback_return,
            ..
        } = result
            && fallback_return != TypeId::ERROR
            && let Some(&arg_idx) = args.get(index)
            && self.is_callback_like_argument(arg_idx)
            && self
                .callback_body_spans(arg_idx)
                .iter()
                .any(|(start, end)| {
                    self.ctx.diagnostics.iter().any(|diag| {
                        matches!(diag.code, 2322 | 2339 | 2345 | 2347 | 2769)
                            && diag.start >= *start
                            && diag.start < *end
                    })
                })
        {
            result = CallResult::Success(fallback_return);
        }

        if self.ctx.in_const_assertion
            && is_generic_call
            && args.len() == 1
            && let (CallResult::Success(return_type), Some(&arg_type)) =
                (result.clone(), arg_types.first())
            && return_type == common::widen_literal_type(self.ctx.types, arg_type)
            && return_type != arg_type
        {
            result = CallResult::Success(arg_type);
        }

        if let CallResult::Success(return_type) = result {
            for (index, &actual) in arg_types.iter().enumerate() {
                let expected = finalized_contextual_param_types
                    .as_ref()
                    .and_then(|types| types.get(index).copied().flatten())
                    .or_else(|| {
                        ContextualTypeContext::with_expected_and_options(
                            self.ctx.types,
                            callee_type_for_call,
                            self.ctx.compiler_options.no_implicit_any,
                        )
                        .get_parameter_type_for_call(index, args.len())
                    });
                if let Some(expected) = expected
                    && !(expected == TypeId::NEVER
                        && common::index_access_parts(self.ctx.types, actual).is_some_and(
                            |(_, index)| common::contains_type_parameters(self.ctx.types, index),
                        ))
                    && self
                        .checker_only_assignability_failure_reason(actual, expected)
                        .is_some()
                {
                    result = CallResult::ArgumentTypeMismatch {
                        index,
                        expected,
                        actual,
                        fallback_return: return_type,
                    };
                    allow_contextual_mismatch_deferral = false;
                    break;
                }
            }
        }

        let call_context = CallResultContext {
            callee_expr: call.expression,
            call_idx: idx,
            args,
            arg_types: &arg_types,
            callee_type: callee_type_for_call,
            callee_has_declared_generic_signature: common::function_shape_for_type(
                self.ctx.types,
                callee_type_for_resolution,
            )
            .is_some_and(|shape| !shape.type_params.is_empty())
                || common::callable_shape_for_type(self.ctx.types, callee_type_for_resolution)
                    .is_some_and(|shape| {
                        shape
                            .call_signatures
                            .iter()
                            .any(|sig| !sig.type_params.is_empty())
                    }),
            is_super_call,
            is_optional_chain: nullish_cause.is_some(),
            allow_contextual_mismatch_deferral,
        };
        // Pop the shape_this_type that was kept on the stack since the
        // argument collection phase.
        if pushed_this_type_from_shape {
            self.ctx.this_type_stack.pop();
        }
        // Keep generic_excess_skip set through handle_call_result so that error
        // elaboration respects the skip flag for generic calls with type parameter
        // targets. Restore it after handle_call_result completes.
        let call_result = self.handle_call_result(result, call_context);
        self.ctx.generic_excess_skip = prev_generic_excess_skip;
        if let Some(return_type) = circular_recursive_call_return_type
            && call_result != TypeId::ERROR
        {
            let immediately_called = self
                .ctx
                .arena
                .parent_of(idx)
                .and_then(|parent_idx| self.ctx.arena.get(parent_idx))
                .and_then(|parent_node| self.ctx.arena.get_call_expr(parent_node))
                .is_some_and(|parent_call| parent_call.expression == idx);
            if common::is_callable_type(self.ctx.types, call_result)
                || (immediately_called && call_result == TypeId::ANY)
            {
                return call_result;
            }
            return return_type;
        }
        call_result
    }
}
