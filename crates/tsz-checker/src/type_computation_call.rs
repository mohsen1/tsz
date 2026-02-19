//! Call expression and identifier type computation for `CheckerState`.
//!
//! Split from `type_computation_complex.rs` for maintainability.

use crate::query_boundaries::call_checker;
use crate::query_boundaries::type_computation_complex as query;
use crate::state::CheckerState;
use crate::type_computation_complex::is_contextually_sensitive;
use tracing::trace;
use tsz_binder::SymbolId;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{ContextualTypeContext, TypeId};

struct CallResultContext<'a> {
    callee_expr: NodeIndex,
    call_idx: NodeIndex,
    args: &'a [NodeIndex],
    arg_types: &'a [TypeId],
    callee_type: TypeId,
    is_super_call: bool,
    is_optional_chain: bool,
}

impl<'a> CheckerState<'a> {
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
    pub(crate) fn get_type_of_call_expression(&mut self, idx: NodeIndex) -> TypeId {
        // Check call depth limit to prevent infinite recursion
        if !self.ctx.call_depth.borrow_mut().enter() {
            return TypeId::ERROR;
        }

        let result = self.get_type_of_call_expression_inner(idx);

        self.ctx.call_depth.borrow_mut().leave();
        result
    }

    /// Inner implementation of call expression type resolution.
    pub(crate) fn get_type_of_call_expression_inner(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_solver::instantiate_type;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return TypeId::ERROR; // Missing call expression data - propagate error
        };

        // Get the type of the callee
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
                            let decl_idx = if !symbol.value_declaration.is_none() {
                                Some(symbol.value_declaration)
                            } else if symbol.declarations.len() == 1 {
                                symbol.declarations.first().copied()
                            } else {
                                None
                            };
                            let is_fast_path_function_decl = symbol.declarations.len() == 1
                                && decl_idx
                                    .and_then(|idx| self.ctx.arena.get(idx))
                                    .is_some_and(|decl| {
                                        if decl.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                                            return false;
                                        }
                                        self.ctx.arena.get_function(decl).is_some_and(|func| {
                                            // Original safe fast path: local implementations
                                            // without explicit return annotations.
                                            let is_unannotated_impl =
                                                func.type_annotation.is_none();

                                            // Additional constrained path for ambient signatures.
                                            // Keep this strict to avoid bypassing value/type
                                            // diagnostics for non-local or indirectly-resolved
                                            // symbols.
                                            let is_local_ambient_signature = func.body.is_none()
                                                && direct_symbol == Some(sym_id)
                                                && (symbol.decl_file_idx
                                                    == self.ctx.current_file_idx as u32
                                                    || symbol.decl_file_idx == u32::MAX);

                                            is_unannotated_impl || is_local_ambient_signature
                                        })
                                    });
                            symbol.escaped_name == identifier_text
                                && is_fast_path_function_decl
                                && (symbol.flags & tsz_binder::symbol_flags::FUNCTION) != 0
                                && (symbol.flags & tsz_binder::symbol_flags::VALUE) != 0
                                && (symbol.flags & tsz_binder::symbol_flags::ALIAS) == 0
                                && (symbol.decl_file_idx == u32::MAX
                                    || symbol.decl_file_idx == self.ctx.current_file_idx as u32)
                        })
                    });
                if let Some(sym_id) = fast_symbol {
                    // Fast path intentionally skips identifier-side diagnostic probes
                    // (e.g. type-only import/value checks). The guard allows local,
                    // non-aliased function declarations in two cases:
                    // - implementation declarations without explicit return annotations
                    // - current-file direct ambient/overload signatures (no body)
                    self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
                    self.get_type_of_symbol(sym_id)
                } else {
                    self.get_type_of_node(call.expression)
                }
            } else {
                self.get_type_of_node(call.expression)
            }
        } else {
            self.get_type_of_node(call.expression)
        };
        trace!(
            callee_type = ?callee_type,
            callee_expr = ?call.expression,
            "Call expression callee type resolved"
        );

        // Check for dynamic import module resolution (TS2307)
        if self.is_dynamic_import(call) {
            self.check_dynamic_import_module_specifier(call);
            // Dynamic imports return Promise<typeof module>
            // This creates Promise<ModuleNamespace> where ModuleNamespace contains all exports
            return self.get_dynamic_import_type(call);
        }

        // Special handling for super() calls - treat as construct call
        let is_super_call = self.is_super_expression(call.expression);

        // Get arguments list (may be None for calls without arguments)
        // IMPORTANT: We must check arguments even if callee is ANY/ERROR to catch definite assignment errors
        let args = match call.arguments.as_ref() {
            Some(a) => a.nodes.as_slice(),
            None => &[],
        };

        // Check if callee is any/error (don't report for those)
        if callee_type == TypeId::ANY {
            if let Some(ref type_args_list) = call.type_arguments
                && !type_args_list.nodes.is_empty()
            {
                self.error_at_node(
                    idx,
                    crate::diagnostics::diagnostic_messages::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                    crate::diagnostics::diagnostic_codes::UNTYPED_FUNCTION_CALLS_MAY_NOT_ACCEPT_TYPE_ARGUMENTS,
                );
            }
            // Still need to check arguments for definite assignment (TS2454) and other errors
            // Create a dummy context helper that returns None for all parameter types
            let _ctx_helper = ContextualTypeContext::new(self.ctx.types);
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None, // No parameter type info for ANY callee
                check_excess_properties,
                None, // No skipping needed
            );
            return TypeId::ANY;
        }
        if callee_type == TypeId::ERROR {
            // Still evaluate type arguments to catch TS2304 for unresolved type names
            // (e.g., `this.super<T>(0)` where T is undeclared)
            if let Some(ref type_args_list) = call.type_arguments {
                for &arg_idx in &type_args_list.nodes {
                    self.get_type_from_type_node(arg_idx);
                }
            }
            // Still need to check arguments for definite assignment (TS2454) and other errors
            let _ctx_helper = ContextualTypeContext::new(self.ctx.types);
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None, // No parameter type info for ERROR callee
                check_excess_properties,
                None, // No skipping needed
            );
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        // Calling `never` returns `never` (bottom type propagation).
        // TSC does not emit TS18050 for calling `never` — the result is simply `never`.
        if callee_type == TypeId::NEVER {
            return TypeId::NEVER;
        }

        let mut nullish_cause = None;
        if (node.flags as u32) & node_flags::OPTIONAL_CHAIN != 0 {
            let (non_nullish, cause) = self.split_nullish_type(callee_type);
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
        if let Some(ref type_args_list) = call.type_arguments
            && !type_args_list.nodes.is_empty()
        {
            self.validate_call_type_arguments(callee_type, type_args_list, idx);
        }

        // Apply explicit type arguments to the callee type before checking arguments.
        // This ensures that when we have `fn<T>(x: T)` and call it as `fn<number>("string")`,
        // the parameter type becomes `number` (after substituting T=number), and we can
        // correctly check if `"string"` is assignable to `number`.
        let callee_type_for_resolution = if call.type_arguments.is_some() {
            self.apply_type_arguments_to_callable_type(callee_type, call.type_arguments.as_ref())
        } else {
            callee_type
        };

        let classification =
            query::classify_for_call_signatures(self.ctx.types, callee_type_for_resolution);
        trace!(
            callee_type_for_resolution = ?callee_type_for_resolution,
            classification = ?classification,
            "Call signatures classified"
        );
        let overload_signatures = match classification {
            query::CallSignaturesKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                (shape.call_signatures.len() > 1).then(|| shape.call_signatures.clone())
            }
            query::CallSignaturesKind::MultipleSignatures(signatures) => {
                (signatures.len() > 1).then_some(signatures)
            }
            query::CallSignaturesKind::NoSignatures => None,
        };

        // Overload candidates need signature-specific contextual typing.
        let force_bivariant_callbacks = matches!(
            self.ctx.arena.get(call.expression).map(|n| n.kind),
            Some(
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            )
        );

        if let Some(signatures) = overload_signatures.as_deref()
            && let Some(return_type) = self.resolve_overloaded_call_with_signatures(
                args,
                signatures,
                force_bivariant_callbacks,
            )
        {
            trace!(
                return_type = ?return_type,
                signatures_count = signatures.len(),
                "Resolved overloaded call return type"
            );
            let return_type =
                self.apply_this_substitution_to_call_return(return_type, call.expression);
            return if nullish_cause.is_some() {
                self.ctx
                    .types
                    .factory()
                    .union(vec![return_type, TypeId::UNDEFINED])
            } else {
                return_type
            };
        }

        // Resolve Ref types to get the actual callable for FunctionShape extraction
        // This is needed before we can check if the callee is generic
        let callee_type_for_shape = self.resolve_ref_type(callee_type_for_resolution);

        // Extract function shape to check if this is a generic call that needs two-pass inference
        let callee_shape =
            call_checker::get_contextual_signature(self.ctx.types, callee_type_for_shape);
        let is_generic_call = callee_shape
            .as_ref()
            .is_some_and(|s| !s.type_params.is_empty())
            && call.type_arguments.is_none(); // Only use two-pass if no explicit type args

        // Create contextual context from callee type with type arguments applied
        let ctx_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            callee_type_for_resolution,
            self.ctx.compiler_options.no_implicit_any,
        );
        let check_excess_properties = overload_signatures.is_none();

        // Two-pass argument collection for generic calls is only needed when at least one
        // argument is contextually sensitive (e.g. lambdas/object literals needing contextual type).
        let arg_types = if is_generic_call {
            if let Some(shape) = callee_shape {
                // Pre-compute which arguments are contextually sensitive to avoid borrowing self in closures.
                let sensitive_args: Vec<bool> = args
                    .iter()
                    .map(|&arg| is_contextually_sensitive(self, arg))
                    .collect();
                let needs_two_pass = sensitive_args.iter().copied().any(std::convert::identity);

                if needs_two_pass {
                    // === Round 1: Collect non-contextual argument types ===
                    // This allows type parameters to be inferred from concrete arguments.
                    // CRITICAL: Skip checking sensitive arguments entirely to prevent TS7006
                    // from being emitted before inference completes.
                    let mut round1_arg_types = self.collect_call_argument_types_with_context(
                        args,
                        |i, arg_count| {
                            // Skip contextually sensitive arguments in Round 1.
                            // Guard against out-of-bounds: large indices are used to probe
                            // for rest parameters (see call_checker.rs spread handling).
                            if i < sensitive_args.len() && sensitive_args[i] {
                                None
                            } else {
                                ctx_helper.get_parameter_type_for_call(i, arg_count)
                            }
                        },
                        check_excess_properties,
                        Some(&sensitive_args), // Skip sensitive args in Round 1
                    );

                    // For sensitive object literal arguments, extract a partial type
                    // from non-sensitive properties to improve inference.
                    // This handles patterns like:
                    //   app({ state: 100, actions: { foo: s => s } })
                    // where `state: 100` can infer State=number, but `actions` is
                    // context-sensitive and must wait for Round 2.
                    for (i, &arg_idx) in args.iter().enumerate() {
                        if sensitive_args[i]
                            && let Some(partial) = self.extract_non_sensitive_object_type(arg_idx)
                        {
                            trace!(
                                arg_index = i,
                                partial_type = partial.0,
                                "Round 1: extracted non-sensitive partial type for object literal"
                            );
                            round1_arg_types[i] = partial;
                        }
                    }

                    // === Perform Round 1 Inference ===
                    // Pre-evaluate function shape parameter types through the
                    // TypeEnvironment so the solver can constrain against concrete
                    // object types instead of unresolved Application types.
                    // Example: Opts<State, Actions> → { state?: State, actions: Actions }
                    let evaluated_shape = {
                        let new_params: Vec<_> = shape
                            .params
                            .iter()
                            .map(|p| tsz_solver::ParamInfo {
                                name: p.name,
                                type_id: self.evaluate_type_with_env(p.type_id),
                                optional: p.optional,
                                rest: p.rest,
                            })
                            .collect();
                        tsz_solver::FunctionShape {
                            params: new_params,
                            return_type: shape.return_type,
                            this_type: shape.this_type,
                            type_params: shape.type_params.clone(),
                            type_predicate: shape.type_predicate.clone(),
                            is_constructor: shape.is_constructor,
                            is_method: shape.is_method,
                        }
                    };
                    let substitution = {
                        let env = self.ctx.type_env.borrow();
                        call_checker::compute_contextual_types_with_context(
                            self.ctx.types,
                            &self.ctx,
                            &env,
                            &evaluated_shape,
                            &round1_arg_types,
                            self.ctx.contextual_type,
                        )
                    };
                    trace!(
                        substitution_is_empty = substitution.is_empty(),
                        "Round 1 inference: substitution computed"
                    );

                    // === Pre-evaluate instantiated parameter types ===
                    // After instantiation with Round 1 substitution, parameter types may
                    // contain unevaluated IndexAccess/KeyOf over Lazy(DefId) references
                    // (e.g., OptionsForKey[K] → OptionsForKey["a"]). The QueryCache's
                    // evaluate_type uses NoopResolver which can't resolve Lazy types.
                    // Use evaluate_type_with_env which resolves Lazy types via the
                    // TypeEnvironment before evaluation.
                    let arg_count = args.len();
                    let mut round2_contextual_types: Vec<Option<TypeId>> =
                        Vec::with_capacity(arg_count);
                    for i in 0..arg_count {
                        let ctx_type = if let Some(param_type) =
                            ctx_helper.get_parameter_type_for_call(i, arg_count)
                        {
                            let instantiated =
                                instantiate_type(self.ctx.types, param_type, &substitution);
                            Some(self.evaluate_type_with_env(instantiated))
                        } else {
                            None
                        };
                        trace!(
                            arg_index = i,
                            ctx_type_id = ?ctx_type.map(|t| t.0),
                            "Round 2: contextual type for argument"
                        );
                        round2_contextual_types.push(ctx_type);
                    }

                    // === Round 2: Collect ALL argument types with contextual typing ===
                    // Now that type parameters are partially inferred, lambdas get proper contextual types.
                    self.collect_call_argument_types_with_context(
                        args,
                        |i, _arg_count| {
                            // Guard: large indices are used to probe for rest parameters
                            round2_contextual_types.get(i).copied().flatten()
                        },
                        check_excess_properties,
                        None, // Don't skip anything in Round 2 - check all args with inferred context
                    )
                } else {
                    // No context-sensitive arguments: skip Round 1/2 and use single-pass collection.
                    self.collect_call_argument_types_with_context(
                        args,
                        |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                        check_excess_properties,
                        None, // No skipping needed for single-pass
                    )
                }
            } else {
                // Shouldn't happen for generic call detection, but keep single-pass fallback.
                self.collect_call_argument_types_with_context(
                    args,
                    |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                    check_excess_properties,
                    None, // No skipping needed for single-pass
                )
            }
        } else {
            // === Single-pass: Standard argument collection ===
            // Non-generic calls or calls with explicit type arguments use the standard flow.
            self.collect_call_argument_types_with_context(
                args,
                |i, arg_count| ctx_helper.get_parameter_type_for_call(i, arg_count),
                check_excess_properties,
                None, // No skipping needed for single-pass
            )
        };
        // Delegate the call resolution to solver boundary helpers.
        self.ensure_relation_input_ready(callee_type_for_resolution);

        // Evaluate application types to resolve Ref bases to actual Callable types
        // This is needed for cases like `GenericCallable<string>` where the type is
        // stored as Application(Ref(symbol_id), [string]) and needs to be resolved
        // to the actual Callable with call signatures
        let callee_type_for_call = self.evaluate_application_type(callee_type_for_resolution);
        // Resolve lazy (Ref) types to their underlying callable types.
        // This handles interfaces with call signatures, merged declarations, etc.
        // Use resolve_lazy_type instead of resolve_ref_type to also resolve Lazy
        // types nested inside intersection/union members.
        let callee_type_for_call = self.resolve_lazy_type(callee_type_for_call);

        // The `Function` interface from lib.d.ts has no call signatures, but in TypeScript
        // it is callable and returns `any`. Check if the callee is the Function boxed type
        // or the Function intrinsic and handle it like `any`.
        // The `Function` interface from lib.d.ts has no call signatures, but in TypeScript
        // it is callable and returns `any`. We check both the intrinsic TypeId::FUNCTION
        // and the global Function interface type resolved from lib.d.ts.
        // For unions containing Function members, we replace those members with a
        // synthetic callable that returns `any` so resolve_union_call succeeds.
        let callee_type_for_call =
            self.replace_function_type_for_call(callee_type, callee_type_for_call);
        if callee_type_for_call == TypeId::ANY {
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| None,
                check_excess_properties,
                None, // No skipping needed
            );
            return if nullish_cause.is_some() {
                self.ctx
                    .types
                    .factory()
                    .union(vec![TypeId::ANY, TypeId::UNDEFINED])
            } else {
                TypeId::ANY
            };
        }

        // Ensure relation preconditions (lazy refs + application symbols) for callee/args.
        self.ensure_relation_input_ready(callee_type_for_call);

        // super() calls are constructor calls, not function calls.
        // Use resolve_new() which checks construct signatures instead of call signatures.
        let result = if is_super_call {
            self.resolve_new_with_checker_adapter(
                callee_type_for_call,
                &arg_types,
                force_bivariant_callbacks,
            )
        } else {
            self.resolve_call_with_checker_adapter(
                callee_type_for_call,
                &arg_types,
                force_bivariant_callbacks,
                self.ctx.contextual_type,
            )
        };

        let call_context = CallResultContext {
            callee_expr: call.expression,
            call_idx: idx,
            args,
            arg_types: &arg_types,
            callee_type: callee_type_for_call,
            is_super_call,
            is_optional_chain: nullish_cause.is_some(),
        };
        self.handle_call_result(result, call_context)
    }

    /// Handle the result of a call evaluation, emitting diagnostics for errors
    /// and applying this-substitution/mixin refinement for successes.
    fn handle_call_result(
        &mut self,
        result: tsz_solver::CallResult,
        context: CallResultContext<'_>,
    ) -> TypeId {
        use tsz_solver::CallResult;
        let CallResultContext {
            callee_expr,
            call_idx,
            args,
            arg_types,
            callee_type,
            is_super_call,
            is_optional_chain,
            ..
        } = context;
        match result {
            CallResult::Success(return_type) => {
                // super() calls always return void — they call the parent constructor
                // on `this`, they don't create a new instance.
                if is_super_call {
                    return TypeId::VOID;
                }
                let return_type =
                    self.apply_this_substitution_to_call_return(return_type, callee_expr);
                let return_type =
                    self.refine_mixin_call_return_type(callee_expr, arg_types, return_type);
                // Strip freshness from function return types. Object literals returned
                // from functions lose their freshness at the call boundary — the caller
                // should not see excess property checks for the callee's return value.
                let return_type = if !self.ctx.compiler_options.sound_mode {
                    tsz_solver::freshness::widen_freshness(self.ctx.types, return_type)
                } else {
                    return_type
                };
                if is_optional_chain {
                    self.ctx
                        .types
                        .factory()
                        .union(vec![return_type, TypeId::UNDEFINED])
                } else {
                    return_type
                }
            }
            CallResult::NotCallable { .. } => {
                // super() calls now use resolve_new() which checks construct signatures,
                // so NotCallable for super() means the base class has no constructor.
                // This is valid - classes can have implicit constructors.
                if is_super_call {
                    return TypeId::VOID;
                }
                if self.is_constructor_type(callee_type) {
                    self.error_class_constructor_without_new_at(callee_type, callee_expr);
                } else if self.is_get_accessor_call(callee_expr) {
                    self.error_get_accessor_not_callable_at(callee_expr);
                } else {
                    self.error_not_callable_at(callee_type, callee_expr);
                }
                TypeId::ERROR
            }
            CallResult::ArgumentCountMismatch {
                expected_min,
                expected_max,
                actual,
            } => {
                if actual < expected_min && expected_max.is_none() {
                    // Too few arguments with rest parameters (unbounded) - use TS2555
                    self.error_expected_at_least_arguments_at(expected_min, actual, call_idx);
                } else {
                    // Use TS2554 for exact count, range, or too many args
                    let expected = expected_max.unwrap_or(expected_min);
                    self.error_argument_count_mismatch_at(expected, actual, call_idx);
                }
                TypeId::ERROR
            }
            CallResult::OverloadArgumentCountMismatch {
                actual,
                expected_low,
                expected_high,
            } => {
                self.error_at_node(
                    call_idx,
                    &format!(
                        "No overload expects {actual} arguments, but overloads do exist that expect either {expected_low} or {expected_high} arguments."
                    ),
                    diagnostic_codes::NO_OVERLOAD_EXPECTS_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR_ARGUM,
                );
                TypeId::ERROR
            }
            CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
            } => {
                // Avoid cascading TS2345 when the argument type is already invalid or unknown.
                // In these cases, a more specific upstream diagnostic is usually the root cause.
                if actual == TypeId::ERROR
                    || actual == TypeId::UNKNOWN
                    || expected == TypeId::ERROR
                    || expected == TypeId::UNKNOWN
                {
                    return TypeId::ERROR;
                }

                let arg_idx = self.map_expanded_arg_index_to_original(args, index);
                if let Some(arg_idx) = arg_idx {
                    if !self.should_suppress_weak_key_arg_mismatch(callee_expr, args, index, actual)
                    {
                        // Try to elaborate: for object literal arguments, report TS2322
                        // on specific mismatched properties instead of TS2345 on the
                        // whole argument. This matches tsc behavior.
                        if !self.try_elaborate_object_literal_arg_error(arg_idx, expected) {
                            let _ =
                                self.check_argument_assignable_or_report(actual, expected, arg_idx);
                        }
                    }
                } else if !args.is_empty() {
                    let last_arg = args[args.len() - 1];
                    if !self.should_suppress_weak_key_arg_mismatch(callee_expr, args, index, actual)
                        && !self.try_elaborate_object_literal_arg_error(last_arg, expected)
                    {
                        let _ =
                            self.check_argument_assignable_or_report(actual, expected, last_arg);
                    }
                }
                TypeId::ERROR
            }
            CallResult::TypeParameterConstraintViolation {
                inferred_type,
                constraint_type,
                return_type,
            } => {
                // Report TS2322 for constraint violations from callback return type inference
                let _ = self.check_assignable_or_report_generic_at(
                    inferred_type,
                    constraint_type,
                    call_idx,
                    call_idx,
                );
                return_type
            }
            CallResult::NoOverloadMatch {
                func_type,
                failures,
                ..
            } => {
                // Compatibility fallback: built-in toLocaleString supports
                // (locales?, options?) in modern lib typings. Some merged
                // declaration paths can miss those overloads and incorrectly
                // surface TS2769; tsc accepts these calls.
                if self.is_tolocalestring_compat_call(callee_expr, args.len()) {
                    return TypeId::STRING;
                }
                if !self.should_suppress_weak_key_no_overload(callee_expr, args) {
                    self.error_no_overload_matches_at(call_idx, &failures);
                }

                // Fallback: use return type of the first overload if available.
                // This improves error recovery for chained calls (e.g. [].concat().map())
                // by allowing subsequent calls to see a typed object rather than ERROR.
                // For Array.concat, this returns T[] (e.g. never[]) matching TSC behavior.
                use tsz_solver::type_queries;
                if let Some(shape) = type_queries::get_function_shape(self.ctx.types, func_type) {
                    shape.return_type
                } else if let Some(shape) =
                    type_queries::get_callable_shape(self.ctx.types, func_type)
                {
                    shape
                        .call_signatures
                        .first()
                        .map(|s| s.return_type)
                        .unwrap_or(TypeId::ERROR)
                } else {
                    TypeId::ERROR
                }
            }
        }
    }

    fn is_tolocalestring_compat_call(&self, callee_expr: NodeIndex, arg_count: usize) -> bool {
        if arg_count > 2 {
            return false;
        }
        let Some(callee_node) = self.ctx.arena.get(callee_expr) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        ident.escaped_text == "toLocaleString"
    }

    // =========================================================================
    // Type Relationship Queries
    // =========================================================================

    /// Get the type of an identifier expression.
    ///
    /// This function resolves the type of an identifier by:
    /// 1. Looking up the symbol through the binder
    /// 2. Getting the declared type of the symbol
    /// 3. Checking for TDZ (temporal dead zone) violations
    /// 4. Checking definite assignment for block-scoped variables
    /// 5. Applying flow-based type narrowing
    ///
    /// ## Symbol Resolution:
    /// - Uses `resolve_identifier_symbol` to find the symbol
    /// - Checks for type-only aliases (error if used as value)
    /// - Validates that symbol has a value declaration
    ///
    /// ## TDZ Checking:
    /// - Static block TDZ: variable used in static block before declaration
    /// - Computed property TDZ: variable in computed property before declaration
    /// - Heritage clause TDZ: variable in extends/implements before declaration
    ///
    /// ## Definite Assignment:
    /// - Checks if variable is definitely assigned before use
    /// - Only applies to block-scoped variables without initializers
    /// - Skipped for parameters, ambient contexts, and captured variables
    ///
    /// ## Flow Narrowing:
    /// - If definitely assigned, applies type narrowing based on control flow
    /// - Refines union types based on typeof guards, null checks, etc.
    ///
    /// ## Intrinsic Names:
    /// - `undefined` → UNDEFINED type
    /// - `NaN` / `Infinity` → NUMBER type
    /// - `Symbol` → Symbol constructor type (if available in lib)
    ///
    /// ## Global Value Names:
    /// - Returns ANY for available globals (Array, Object, etc.)
    /// - Emits error for unavailable ES2015+ types
    ///
    /// ## Error Handling:
    /// - Returns ERROR for:
    ///   - Type-only aliases used as values
    ///   - Variables used before declaration (TDZ)
    ///   - Variables not definitely assigned
    ///   - Static members accessed without `this`
    ///   - `await` in default parameters
    ///   - Unresolved names (with "cannot find name" error)
    /// - Returns ANY for unresolved imports (TS2307 already emitted)
    pub(crate) fn get_type_of_identifier(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return TypeId::ERROR; // Missing identifier data - propagate error
        };

        let name = &ident.escaped_text;

        // TS2496: 'arguments' cannot be referenced in an arrow function in ES5
        if name == "arguments" {
            // Track that this function body uses `arguments` (for JS implicit rest params)
            self.ctx.js_body_uses_arguments = true;

            // TS2815: 'arguments' cannot be referenced in property initializers
            // or class static initialization blocks. Must check BEFORE regular
            // function body check because arrow functions are transparent.
            if self.is_arguments_in_class_initializer_or_static_block(idx) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::ARGUMENTS_CANNOT_BE_REFERENCED_IN_PROPERTY_INITIALIZERS_OR_CLASS_STATIC_INITIALI,
                    diagnostic_codes::ARGUMENTS_CANNOT_BE_REFERENCED_IN_PROPERTY_INITIALIZERS_OR_CLASS_STATIC_INITIALI,
                );
                return TypeId::ERROR;
            }

            use tsz_common::common::ScriptTarget;
            let is_es5_or_lower = matches!(
                self.ctx.compiler_options.target,
                ScriptTarget::ES3 | ScriptTarget::ES5
            );
            if is_es5_or_lower && self.is_arguments_in_arrow_function(idx) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ARROW_FUNCTION_IN_ES5_CONSIDER_U,
                    diagnostic_codes::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ARROW_FUNCTION_IN_ES5_CONSIDER_U,
                );
                // Return ERROR to prevent fallthrough to normal resolution which would emit TS2304
                return TypeId::ERROR;
            }
            if is_es5_or_lower && self.is_arguments_in_async_non_arrow_function(idx) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5,
                    diagnostic_codes::THE_ARGUMENTS_OBJECT_CANNOT_BE_REFERENCED_IN_AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5,
                );
                return TypeId::ERROR;
            }

            // Inside a regular (non-arrow) function body, `arguments` is the implicit
            // IArguments object, overriding any outer `arguments` declaration.
            // EXCEPT: if there's a LOCAL variable named "arguments" in the current function,
            // that shadows the built-in IArguments (e.g., `const arguments = this.arguments;`).
            if self.is_in_regular_function_body(idx) {
                // Check if there's a local "arguments" variable in the current function scope.
                // This handles shadowing: `const arguments = ...` takes precedence over IArguments.
                if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
                    // Found a symbol named "arguments". Check if it's declared locally
                    // in the current function (not in an outer scope).
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                        && !symbol.declarations.is_empty()
                    {
                        let decl_node = symbol.declarations[0];
                        // Find the enclosing function for both the reference and the declaration
                        if let Some(current_fn) = self.find_enclosing_function(idx) {
                            if let Some(decl_fn) = self.find_enclosing_function(decl_node) {
                                // If the declaration is in the same function scope, it shadows IArguments
                                if current_fn == decl_fn {
                                    trace!(
                                        name = name,
                                        idx = ?idx,
                                        sym_id = ?sym_id,
                                        "get_type_of_identifier: local 'arguments' variable shadows built-in IArguments"
                                    );
                                    // Fall through to normal resolution below - use the local variable
                                } else {
                                    // Declaration is in an outer scope - use built-in IArguments
                                    let lib_binders = self.get_lib_binders();
                                    if let Some(iargs_sym) = self
                                        .ctx
                                        .binder
                                        .get_global_type_with_libs("IArguments", &lib_binders)
                                    {
                                        return self.type_reference_symbol_type(iargs_sym);
                                    }
                                    return TypeId::ANY;
                                }
                            } else {
                                // Declaration not in a function (global) - use built-in IArguments
                                let lib_binders = self.get_lib_binders();
                                if let Some(iargs_sym) = self
                                    .ctx
                                    .binder
                                    .get_global_type_with_libs("IArguments", &lib_binders)
                                {
                                    return self.type_reference_symbol_type(iargs_sym);
                                }
                                return TypeId::ANY;
                            }
                        }
                    }
                } else {
                    // No symbol found at all - use built-in IArguments
                    let lib_binders = self.get_lib_binders();
                    if let Some(sym_id) = self
                        .ctx
                        .binder
                        .get_global_type_with_libs("IArguments", &lib_binders)
                    {
                        return self.type_reference_symbol_type(sym_id);
                    }
                    return TypeId::ANY;
                }
            }
        }

        // === CRITICAL FIX: Check type parameter scope FIRST ===
        // Type parameters in generic functions/classes/type aliases should be resolved
        // before checking any other scope. This is a common source of TS2304 false positives.
        // Examples:
        //   function foo<T>(x: T) { return x; }  // T should be found in the function body
        //   class C<U> { method(u: U) {} }  // U should be found in the class body
        //   type Pair<T> = [T, T];  // T should be found in the type alias definition
        if let Some(type_id) = self.lookup_type_parameter(name) {
            // Before emitting TS2693, check if the binder also has a value symbol
            // with the same name. In cases like `function f<A>(A: A)`, the parameter
            // `A` shadows the type parameter `A` in value position.
            let has_value_shadow = self
                .resolve_identifier_symbol(idx)
                .and_then(|sym_id| {
                    self.ctx
                        .binder
                        .get_symbol(sym_id)
                        .map(|s| s.flags & tsz_binder::symbol_flags::VALUE != 0)
                })
                .unwrap_or(false);
            if !has_value_shadow {
                // TS2693: Type parameters cannot be used as values
                // Example: function f<T>() { return T; }  // Error: T is a type, not a value
                self.error_type_parameter_used_as_value(name, idx);
                return type_id;
            }
            // Fall through to binder resolution — the value symbol takes precedence
        }

        // Resolve via binder persistent scopes for stateless lookup.
        if let Some(sym_id) = self.resolve_identifier_symbol(idx) {
            // Reference tracking is handled by resolve_identifier_symbol wrapper
            trace!(
                name = name,
                idx = ?idx,
                sym_id = ?sym_id,
                "get_type_of_identifier: resolved symbol"
            );

            // TS7034: Check if this identifier references a pending implicit-any variable
            // from a nested function scope (i.e., the variable is captured by a closure).
            // If so, emit TS7034 at the declaration site.
            if self.ctx.pending_implicit_any_vars.contains_key(&sym_id) {
                let ref_fn = self.find_enclosing_function(idx);
                let decl_name_node = self.ctx.pending_implicit_any_vars[&sym_id];
                let decl_fn = self.find_enclosing_function(decl_name_node);
                if ref_fn != decl_fn {
                    // Variable is captured by a nested function — emit TS7034 at declaration.
                    let decl_name_node =
                        self.ctx.pending_implicit_any_vars.remove(&sym_id).unwrap();
                    if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node_msg(
                            decl_name_node,
                            diagnostic_codes::VARIABLE_IMPLICITLY_HAS_TYPE_IN_SOME_LOCATIONS_WHERE_ITS_TYPE_CANNOT_BE_DETERMIN,
                            &[&sym.escaped_name, "any"],
                        );
                    }
                }
            }

            if self.is_type_only_import_equals_namespace_expr(idx) {
                self.error_namespace_used_as_value_at(name, idx);
                if let Some(sym_id) = self.resolve_identifier_symbol(idx)
                    && self.alias_resolves_to_type_only(sym_id)
                {
                    self.error_type_only_value_at(name, idx);
                }
                return TypeId::ERROR;
            }

            if self.alias_resolves_to_type_only(sym_id) {
                // Don't emit TS2693 in heritage clause context (e.g., `extends A`)
                if self.is_direct_heritage_type_reference(idx) {
                    return TypeId::ERROR;
                }
                // Don't emit TS2693 for export default/export = expressions
                if let Some(parent_ext) = self.ctx.arena.get_extended(idx)
                    && !parent_ext.parent.is_none()
                    && let Some(parent_node) = self.ctx.arena.get(parent_ext.parent)
                {
                    use tsz_parser::parser::syntax_kind_ext;
                    if parent_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                        || parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    {
                        return TypeId::ERROR;
                    }
                }
                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }
            // Check symbol flags to detect type-only usage.
            // First try the main binder (fast path for local symbols).
            let local_symbol = self
                .get_cross_file_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id));
            let flags = local_symbol.map_or(0, |s| s.flags);

            // TS2662: Bare identifier resolving to a static class member.
            // Static members must be accessed via `ClassName.member`, not as
            // bare identifiers.  The binder puts them in the class scope so
            // they resolve, but the checker must reject unqualified access.
            if (flags & tsz_binder::symbol_flags::STATIC) != 0
                && let Some(ref class_info) = self.ctx.enclosing_class.clone()
                && self.is_static_member(&class_info.member_nodes, name)
            {
                self.error_cannot_find_name_static_member_at(name, &class_info.name, idx);
                return TypeId::ERROR;
            }

            let has_type = (flags & tsz_binder::symbol_flags::TYPE) != 0;
            let has_value = (flags & tsz_binder::symbol_flags::VALUE) != 0;
            let is_type_alias = (flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0;
            trace!(
                name = name,
                flags = flags,
                has_type = has_type,
                has_value = has_value,
                is_interface = (flags & tsz_binder::symbol_flags::INTERFACE) != 0,
                "get_type_of_identifier: symbol flags"
            );
            let value_decl = local_symbol.map_or(NodeIndex::NONE, |s| s.value_declaration);
            let symbol_declarations = local_symbol
                .map(|s| s.declarations.clone())
                .unwrap_or_default();

            // Check for type-only symbols used as values
            // This includes:
            // 1. Symbols with TYPE flag but no VALUE flag (interfaces, type-only imports, etc.)
            // 2. Type aliases (never have VALUE, even if they reference a class)
            //
            // IMPORTANT: Only check is_interface if it has no VALUE flag.
            // Interfaces merged with namespaces DO have VALUE and should NOT error.
            //
            // CROSS-LIB MERGING: The same name may have TYPE in one lib file
            // (e.g., `interface Promise<T>` in es5.d.ts) and VALUE in another
            // (e.g., `declare var Promise` in es2015.promise.d.ts). When we find
            // a TYPE-only symbol, check if a VALUE exists elsewhere in libs.
            if is_type_alias || (has_type && !has_value) {
                trace!(
                    name = name,
                    sym_id = ?sym_id,
                    is_type_alias = is_type_alias,
                    has_type = has_type,
                    has_value = has_value,
                    "get_type_of_identifier: TYPE-only symbol, checking for VALUE in libs"
                );
                // Cross-lib merging: interface/type may be in one lib while VALUE
                // declaration is in another. Resolve by declaration node first to
                // avoid SymbolId collisions across binders.
                let value_type = self.type_of_value_symbol_by_name(name);
                trace!(
                    name = name,
                    value_type = ?value_type,
                    "get_type_of_identifier: value_type from type_of_value_symbol_by_name"
                );
                if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                    trace!(
                        name = name,
                        value_type = ?value_type,
                        "get_type_of_identifier: using cross-lib VALUE type"
                    );
                    return self.check_flow_usage(idx, value_type, sym_id);
                }

                // Don't emit TS2693 in heritage clause context — but ONLY when the
                // identifier is the direct expression of an ExpressionWithTypeArguments
                // (e.g., `extends A`). If the identifier is nested deeper, such as
                // a function argument within the heritage expression (e.g.,
                // `extends factory(A)`), TS2693 should still fire.
                if self.is_direct_heritage_type_reference(idx) {
                    return TypeId::ERROR;
                }

                // Don't emit TS2693 for export default/export = expressions.
                // `export default InterfaceName` and `export = InterfaceName`
                // are valid TypeScript — they export the type binding.
                if let Some(parent_ext) = self.ctx.arena.get_extended(idx)
                    && !parent_ext.parent.is_none()
                    && let Some(parent_node) = self.ctx.arena.get(parent_ext.parent)
                {
                    use tsz_parser::parser::syntax_kind_ext;
                    if parent_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                        || parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    {
                        return TypeId::ERROR;
                    }
                }

                self.error_type_only_value_at(name, idx);
                return TypeId::ERROR;
            }

            // NOTE: tsc 6.0 does NOT emit TS2585 based on target version alone.
            // ES2015+ globals (Symbol, Promise, Map, Set, etc.) may be available
            // even with target ES5 because lib.dom.d.ts transitively loads
            // lib.es2015.d.ts. We let the normal value-binding resolution below
            // determine if the value is truly available.

            // If the symbol wasn't found in the main binder (flags==0), it came
            // from a lib or cross-file binder.  For known ES2015+ global type
            // names (Symbol, Promise, Map, Set, etc.) we need to check whether
            // the lib binder's symbol is type-only.  Only do this for the known
            // set to avoid cross-binder ID collisions causing false TS2693 on
            // arbitrary user symbols from other files.
            if flags == 0 {
                use tsz_binder::lib_loader;
                if lib_loader::is_es2015_plus_type(name) {
                    let lib_binders = self.get_lib_binders();
                    let lib_flags = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(sym_id, &lib_binders)
                        .map_or(0, |s| s.flags);
                    let lib_has_type = (lib_flags & tsz_binder::symbol_flags::TYPE) != 0;
                    let lib_has_value = (lib_flags & tsz_binder::symbol_flags::VALUE) != 0;
                    if lib_has_type && !lib_has_value {
                        // Cross-lib merging: VALUE may be in a different lib binder.
                        // Resolve by declaration node first to avoid SymbolId collisions.
                        let value_type = self.type_of_value_symbol_by_name(name);
                        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                            return self.check_flow_usage(idx, value_type, sym_id);
                        }
                        self.error_type_only_value_at(name, idx);
                        return TypeId::ERROR;
                    }
                }
            }

            // Merged interface+value symbols (e.g. `interface Promise<T>` +
            // `declare var Promise: PromiseConstructor`) must use the VALUE side
            // in value position. Falling back to interface type here causes
            // false TS2339/TS2351 on `Promise.resolve` / `new Promise(...)`.
            //
            // Merged interface+value symbols (e.g. Symbol interface + declare var Symbol: SymbolConstructor)
            // must use the VALUE side in value position. The *Constructor lookup below
            // handles finding the right type (SymbolConstructor, PromiseConstructor, etc.)
            let is_merged_interface_value =
                has_type && has_value && (flags & tsz_binder::symbol_flags::INTERFACE) != 0;
            // NOTE: tsc 6.0 does NOT emit TS2585 for ES2015+ globals based on
            // target alone. The value bindings from transitively loaded libs
            // (e.g. lib.dom.d.ts → lib.es2015.d.ts) are considered available.
            // The merged interface+value resolution below handles this correctly.
            if is_merged_interface_value {
                trace!(
                    name = name,
                    sym_id = ?sym_id,
                    value_decl = ?value_decl,
                    "get_type_of_identifier: merged interface+value path"
                );
                // NOTE: tsc 6.0 does NOT emit TS2585 based on target version.
                // Value declarations from transitively loaded libs are available.
                // Prefer value-declaration resolution for merged symbols so we pick
                // the constructor-side type (e.g. Promise -> PromiseConstructor).
                let mut value_type = self.type_of_value_declaration_for_symbol(sym_id, value_decl);
                if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                    for &decl_idx in &symbol_declarations {
                        let candidate = self.type_of_value_declaration_for_symbol(sym_id, decl_idx);
                        if candidate != TypeId::UNKNOWN && candidate != TypeId::ERROR {
                            value_type = candidate;
                            break;
                        }
                    }
                }
                if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                    value_type = self.type_of_value_symbol_by_name(name);
                }
                if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                    let direct_type = self.get_type_of_symbol(sym_id);
                    trace!(
                        name = name,
                        direct_type = ?direct_type,
                        "get_type_of_identifier: direct type from get_type_of_symbol"
                    );
                    if direct_type != TypeId::UNKNOWN && direct_type != TypeId::ERROR {
                        value_type = direct_type;
                    }
                }
                trace!(
                    name = name,
                    value_type = ?value_type,
                    "get_type_of_identifier: value_type after value-decl resolution"
                );
                // Lib globals often model value-side constructors through a sibling
                // `*Constructor` interface (Promise -> PromiseConstructor).
                // Prefer that when available to avoid falling back to the instance interface.
                trace!(
                    name = name,
                    value_type = ?value_type,
                    "get_type_of_identifier: value_type before *Constructor lookup"
                );
                let constructor_name = format!("{name}Constructor");
                trace!(
                    name = name,
                    constructor_name = %constructor_name,
                    "get_type_of_identifier: looking for *Constructor symbol"
                );
                // BUG FIX: Use find_value_symbol_in_libs instead of resolve_global_value_symbol
                // to ensure we get the correct VALUE symbol, not a type-only or wrong symbol.
                // resolve_global_value_symbol can return the wrong symbol when there are
                // name collisions in file_locals (e.g., SymbolConstructor from ES2015 vs DOM types).
                if let Some(constructor_sym_id) = self.find_value_symbol_in_libs(&constructor_name)
                {
                    trace!(
                        name = name,
                        constructor_sym_id = ?constructor_sym_id,
                        "get_type_of_identifier: found *Constructor symbol"
                    );
                    let constructor_type = self.get_type_of_symbol(constructor_sym_id);
                    trace!(
                        name = name,
                        constructor_type = ?constructor_type,
                        "get_type_of_identifier: *Constructor type"
                    );
                    if constructor_type != TypeId::UNKNOWN && constructor_type != TypeId::ERROR {
                        value_type = constructor_type;
                    }
                } else {
                    trace!(
                        name = name,
                        constructor_name = %constructor_name,
                        "get_type_of_identifier: find_value_symbol_in_libs returned None, trying resolve_lib_type_by_name"
                    );
                    if let Some(constructor_type) = self.resolve_lib_type_by_name(&constructor_name)
                        && constructor_type != TypeId::UNKNOWN
                        && constructor_type != TypeId::ERROR
                    {
                        trace!(
                            name = name,
                            constructor_type = ?constructor_type,
                            current_value_type = ?value_type,
                            "get_type_of_identifier: found *Constructor TYPE"
                        );
                        // BUG FIX: Only use constructor_type if we don't already have a valid type.
                        // For "Symbol", value_type=TypeId(8286) is correct (SymbolConstructor),
                        // but resolve_lib_type_by_name returns TypeId(8282) (DecoratorMetadata).
                        // Don't let the wrong *Constructor type overwrite the correct direct type.
                        if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                            value_type = constructor_type;
                        }
                    } else {
                        trace!(
                            name = name,
                            constructor_name = %constructor_name,
                            "get_type_of_identifier: resolve_lib_type_by_name returned None/UNKNOWN/ERROR"
                        );
                    }
                }
                // For `declare var X: X` pattern (self-referential type annotation),
                // the type resolved through type_of_value_declaration may be incomplete
                // because the interface is resolved in a child checker with only one
                // lib arena. Use resolve_lib_type_by_name to get the complete interface
                // type merged from all lib files.
                if !self.ctx.lib_contexts.is_empty()
                    && self.is_self_referential_var_type(sym_id, value_decl, name)
                    && let Some(lib_type) = self.resolve_lib_type_by_name(name)
                    && lib_type != TypeId::UNKNOWN
                    && lib_type != TypeId::ERROR
                {
                    value_type = lib_type;
                }
                // Final fallback: if value_type is still a Lazy type (e.g., due to
                // check_variable_declaration overwriting the symbol_types cache with the
                // Lazy annotation type for `declare var X: X` patterns, and DefId
                // collisions corrupting the type_env), force recompute the symbol type.
                if query::lazy_def_id(self.ctx.types, value_type).is_some() {
                    self.ctx.symbol_types.remove(&sym_id);
                    let recomputed = self.get_type_of_symbol(sym_id);
                    if recomputed != value_type
                        && recomputed != TypeId::UNKNOWN
                        && recomputed != TypeId::ERROR
                    {
                        value_type = recomputed;
                    }
                }
                if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                    return self.check_flow_usage(idx, value_type, sym_id);
                }
            }

            let declared_type = self.get_type_of_symbol(sym_id);
            // Check for TDZ violations (variable used before declaration in source order)
            if self.check_tdz_violation(sym_id, idx, name) {
                return TypeId::ERROR;
            }
            // Use check_flow_usage to integrate both DAA and type narrowing
            // This handles TS2454 errors and applies flow-based narrowing
            let flow_type = self.check_flow_usage(idx, declared_type, sym_id);
            trace!(
                ?flow_type,
                ?declared_type,
                "After check_flow_usage in get_type_of_identifier"
            );

            // FIX: Preserve readonly and other type modifiers from declared_type.
            // When declared_type has modifiers like ReadonlyType, we must preserve them
            // even if flow analysis infers a different type from the initializer.
            // IMPORTANT: Only apply this fix when there's NO contextual type to avoid interfering
            // with variance checking and assignability analysis.
            //
            // CRITICAL: Array element narrowing produces a genuinely different type that we must use.
            // Check if flow_type is a meaningful narrowing (not ANY/ERROR and different from declared_type).
            // If so, use it. Otherwise, preserve declared_type if it has special modifiers.
            let result_type = if self.ctx.contextual_type.is_none()
                && declared_type != TypeId::ANY
                && declared_type != TypeId::ERROR
            {
                // Check if we have genuine narrowing (different type that's not ANY/ERROR)
                let has_narrowing = flow_type != declared_type
                    && flow_type != TypeId::ANY
                    && flow_type != TypeId::ERROR;

                if has_narrowing {
                    // Check if this is "zombie freshness" - flow returning the widened
                    // version of our declared literal type. If widen(declared) == flow,
                    // use declared_type instead.
                    // IMPORTANT: Evaluate the declared type first to expand type aliases
                    // and lazy references, so widen_type can see the actual union members.
                    let evaluated_declared = self.evaluate_type_for_assignability(declared_type);
                    let widened_declared =
                        tsz_solver::widening::widen_type(self.ctx.types, evaluated_declared);
                    if widened_declared == flow_type {
                        declared_type
                    } else {
                        // Genuine narrowing (e.g., array element narrowing) - use narrowed type
                        flow_type
                    }
                } else {
                    // No narrowing or error - check if we should preserve declared_type
                    let has_index_sig = {
                        use tsz_solver::{IndexKind, IndexSignatureResolver};
                        let resolver = IndexSignatureResolver::new(self.ctx.types);
                        resolver.has_index_signature(declared_type, IndexKind::String)
                            || resolver.has_index_signature(declared_type, IndexKind::Number)
                    };
                    if query::is_readonly_type(self.ctx.types, declared_type) || has_index_sig {
                        declared_type
                    } else {
                        flow_type
                    }
                }
            } else {
                flow_type
            };

            // FIX: For mutable variables (let/var), always use declared_type instead of flow_type
            // to preserve literal type widening. Flow analysis may narrow back to literal types
            // from the initializer, but we need to keep the widened type (string, number, etc.)
            // const variables preserve their literal types through flow analysis.
            //
            // CRITICAL EXCEPTION: If flow_type is different from declared_type and not ERROR,
            // we should use flow_type. This allows discriminant narrowing to work for mutable
            // variables while preserving literal type widening in most cases.
            let is_const = self.is_const_variable_declaration(value_decl);
            let result_type = if !is_const {
                // Mutable variable (let/var)
                // If declared type has index signatures (either ObjectWithIndex or a resolved
                // type with index signatures like from a type alias), always preserve it.
                // This prevents false-positive TS2339 errors when accessing properties via
                // index signatures.
                let has_index_sig = {
                    use tsz_solver::{IndexKind, IndexSignatureResolver};
                    let resolver = IndexSignatureResolver::new(self.ctx.types);
                    resolver.has_index_signature(declared_type, IndexKind::String)
                        || resolver.has_index_signature(declared_type, IndexKind::Number)
                };
                if has_index_sig && (flow_type == declared_type || flow_type == TypeId::ERROR) {
                    declared_type
                } else if flow_type != declared_type && flow_type != TypeId::ERROR {
                    // Flow narrowed the type - but check if this is just the initializer
                    // literal being returned. For mutable variables without annotations,
                    // the declared type is already widened (e.g., STRING for "hi"),
                    // so if the flow type widens to the declared type, use declared_type.
                    let widened_flow = tsz_solver::widening::widen_type(self.ctx.types, flow_type);
                    if widened_flow == declared_type {
                        // Flow type is just the initializer literal - use widened declared type
                        declared_type
                    } else {
                        // Also check the reverse: if declared_type is a non-widened literal
                        // (e.g., "foo" from `declare var a: "foo"; let b = a`) and flow_type
                        // is its widened form (string), flow is just returning the widened
                        // version of our literal declared type - use declared_type.
                        // IMPORTANT: Evaluate the declared type first to expand type aliases
                        // and lazy references, so widen_type can see the actual union members.
                        let evaluated_declared =
                            self.evaluate_type_for_assignability(declared_type);
                        let widened_declared =
                            tsz_solver::widening::widen_type(self.ctx.types, evaluated_declared);
                        if widened_declared == flow_type {
                            declared_type
                        } else {
                            // Genuine narrowing (e.g., discriminant narrowing) - use narrowed type
                            flow_type
                        }
                    }
                } else {
                    // No narrowing or error - use declared type to preserve widening
                    declared_type
                }
            } else {
                // Const variable - use flow type (preserves literal type)
                result_type
            };

            // FIX: Flow analysis may return the original fresh type from the initializer expression.
            // For variable references, we must respect the widening that was applied during variable
            // declaration. If the symbol was widened (non-fresh), the flow result should also be widened.
            // This prevents "Zombie Freshness" where CFA bypasses the widened symbol type.
            if !self.ctx.compiler_options.sound_mode {
                use tsz_solver::freshness::{is_fresh_object_type, widen_freshness};
                if is_fresh_object_type(self.ctx.types, result_type) {
                    return widen_freshness(self.ctx.types, result_type);
                }
            }
            return result_type;
        }

        self.resolve_unresolved_identifier(idx, name)
    }

    /// Resolve an identifier that was NOT found in the binder's scope chain.
    ///
    /// Handles intrinsics (`undefined`, `NaN`, `Symbol`), known globals
    /// (`console`, `Math`, `Array`, etc.), static member suggestions, and
    /// "cannot find name" error reporting.
    fn resolve_unresolved_identifier(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        match name {
            "undefined" => TypeId::UNDEFINED,
            "NaN" | "Infinity" => TypeId::NUMBER,
            "Symbol" => self.resolve_symbol_constructor(idx, name),
            _ if self.is_known_global_value_name(name) => self.resolve_known_global(idx, name),
            _ => self.resolve_truly_unknown_identifier(idx, name),
        }
    }

    /// Resolve the `Symbol` constructor. Emits TS2583/TS2585 if Symbol is
    /// unavailable or type-only (ES5 target).
    fn resolve_symbol_constructor(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        if !self.ctx.has_symbol_in_lib() {
            self.error_cannot_find_name_change_lib(name, idx);
            return TypeId::ERROR;
        }
        // NOTE: tsc 6.0 does NOT emit TS2585 based on target version alone.
        // Symbol may be available even with target ES5 via transitive lib loading.
        // Proceed to check if the value binding actually exists.
        let value_type = self.type_of_value_symbol_by_name(name);
        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
            return value_type;
        }
        self.error_type_only_value_at(name, idx);
        TypeId::ERROR
    }

    /// Resolve a known global value name (e.g. `console`, `Math`, `Array`).
    /// Tries binder `file_locals` and lib binders, then falls back to error reporting.
    fn resolve_known_global(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        if self.is_nodejs_runtime_global(name) {
            // In CommonJS module mode, these globals are implicitly available
            if self.ctx.compiler_options.module.is_commonjs() {
                return TypeId::ANY;
            }
            // JS files implicitly have CommonJS globals (require, exports, module, etc.)
            // tsc never emits TS2580 for JS files — they're treated as CommonJS by default
            if self.is_js_file() {
                return TypeId::ANY;
            }
            // Otherwise, emit TS2580 suggesting @types/node installation
            self.error_cannot_find_name_install_node_types(name, idx);
            return TypeId::ERROR;
        }

        let lib_binders = self.get_lib_binders();
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            return self.get_type_of_symbol(sym_id);
        }
        if let Some(sym_id) = self
            .ctx
            .binder
            .get_global_type_with_libs(name, &lib_binders)
        {
            return self.get_type_of_symbol(sym_id);
        }

        self.emit_global_not_found_error(idx, name)
    }

    /// Emit an appropriate error when a known global is not found.
    fn emit_global_not_found_error(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        use crate::error_reporter::is_known_dom_global;
        use tsz_binder::lib_loader;

        if !self.ctx.has_lib_loaded() {
            if lib_loader::is_es2015_plus_type(name) {
                self.error_cannot_find_name_change_lib(name, idx);
            } else {
                self.error_cannot_find_name_at(name, idx);
            }
            return TypeId::ERROR;
        }

        if is_known_dom_global(name) {
            self.error_cannot_find_name_at(name, idx);
            return TypeId::ERROR;
        }
        if lib_loader::is_es2015_plus_type(name) {
            self.error_cannot_find_global_type(name, idx);
            return TypeId::ERROR;
        }

        let first_char = name.chars().next().unwrap_or('a');
        if first_char.is_uppercase() || self.is_known_global_value_name(name) {
            return TypeId::ANY;
        }

        // TS2693: Primitive type keywords used as values
        // TypeScript primitive type keywords (number, string, boolean, etc.) are language keywords
        // for types, not identifiers. When used in value position, emit TS2693.
        // NOTE: `symbol` is excluded — tsc never emits TS2693 for lowercase `symbol`.
        // Instead it emits TS2552 "Cannot find name 'symbol'. Did you mean 'Symbol'?"
        // Exception: in import equals module references (e.g., `import r = undefined`),
        // TS2503 is already emitted by check_namespace_import — don't also emit TS2693.
        if matches!(
            name,
            "number"
                | "string"
                | "boolean"
                | "void"
                | "undefined"
                | "null"
                | "any"
                | "unknown"
                | "never"
                | "object"
                | "bigint"
        ) {
            self.error_type_only_value_at(name, idx);
            return TypeId::ERROR;
        }

        if self.ctx.is_known_global_type(name) {
            self.error_cannot_find_global_type(name, idx);
        } else {
            self.error_cannot_find_name_at(name, idx);
        }
        TypeId::ERROR
    }

    /// Handle a truly unresolved identifier — not a type parameter, not in the
    /// binder, not a known global. Emits TS2304, TS2524, TS2662 as appropriate.
    fn resolve_truly_unknown_identifier(&mut self, idx: NodeIndex, name: &str) -> TypeId {
        // Note: TS1212/1213/1214 strict-mode reserved word check is now handled
        // centrally in error_cannot_find_name_at to cover both value and type contexts.

        // Check static member suggestion (error 2662)
        if let Some(ref class_info) = self.ctx.enclosing_class.clone()
            && self.is_static_member(&class_info.member_nodes, name)
        {
            self.error_cannot_find_name_static_member_at(name, &class_info.name, idx);
            return TypeId::ERROR;
        }
        // TS2524: 'await' in default parameter
        if name == "await" && self.is_in_default_parameter(idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
                diagnostic_codes::AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
            );
            return TypeId::ERROR;
        }
        // TS2523: 'yield' in default parameter
        if name == "yield" && self.is_in_default_parameter(idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
                diagnostic_codes::YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
            );
            return TypeId::ERROR;
        }
        // Suppress TS2304 for unresolved imports (TS2307 was already emitted)
        if self.is_unresolved_import_symbol(idx) {
            return TypeId::ANY;
        }
        // Check known globals that might be missing
        if self.is_known_global_value_name(name) {
            return self.emit_global_not_found_error(idx, name);
        }
        // Always emit errors for primitive type keywords used as values,
        // regardless of report_unresolved_imports. These are built-in language
        // keywords, not cross-file identifiers that might be unresolved.
        if matches!(
            name,
            "number"
                | "string"
                | "boolean"
                | "symbol"
                | "void"
                | "null"
                | "any"
                | "unknown"
                | "never"
                | "object"
                | "bigint"
        ) {
            self.error_cannot_find_name_at(name, idx);
            return TypeId::ERROR;
        }
        // Suppress in single-file mode to prevent cascading false positives
        if !self.ctx.report_unresolved_imports {
            return TypeId::ANY;
        }
        self.error_cannot_find_name_at(name, idx);
        TypeId::ERROR
    }

    /// Check for TDZ violations: variable used before its declaration in a
    /// static block, computed property, or heritage clause; or class/enum
    /// used before its declaration anywhere in the same scope.
    /// Emits TS2448 (variable), TS2449 (class), or TS2450 (enum) and returns
    /// `true` if a violation is found.
    pub(crate) fn check_tdz_violation(
        &mut self,
        sym_id: SymbolId,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        // Skip TDZ checks in cross-arena delegation context.
        // TDZ compares node positions, which are meaningless when the usage node
        // and declaration node come from different files' arenas.
        if Self::is_in_cross_arena_delegation() {
            return false;
        }
        let is_tdz_in_static_block =
            self.is_variable_used_before_declaration_in_static_block(sym_id, idx);
        let is_tdz_in_property_initializer =
            self.is_variable_used_before_declaration_in_computed_property(sym_id, idx);
        let is_tdz_in_heritage_clause =
            self.is_variable_used_before_declaration_in_heritage_clause(sym_id, idx);
        let is_tdz = is_tdz_in_static_block
            || is_tdz_in_property_initializer
            || is_tdz_in_heritage_clause
            || self.is_class_or_enum_used_before_declaration(sym_id, idx);
        if is_tdz {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            // Emit the correct diagnostic based on symbol kind:
            // TS2449 for classes, TS2450 for enums, TS2448 for variables
            let (msg_template, code) = if let Some(sym) = self.ctx.binder.symbols.get(sym_id) {
                if sym.flags & tsz_binder::symbol_flags::CLASS != 0 {
                    (
                        diagnostic_messages::CLASS_USED_BEFORE_ITS_DECLARATION,
                        diagnostic_codes::CLASS_USED_BEFORE_ITS_DECLARATION,
                    )
                } else if sym.flags & tsz_binder::symbol_flags::REGULAR_ENUM != 0 {
                    (
                        diagnostic_messages::ENUM_USED_BEFORE_ITS_DECLARATION,
                        diagnostic_codes::ENUM_USED_BEFORE_ITS_DECLARATION,
                    )
                } else {
                    (
                        diagnostic_messages::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION,
                        diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION,
                    )
                }
            } else {
                (
                    diagnostic_messages::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION,
                    diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION,
                )
            };
            let message = format_message(msg_template, &[name]);
            self.error_at_node(idx, &message, code);

            // TypeScript also reports TS2454 ("used before being assigned") as a
            // companion to TDZ errors in strict-null mode, but ONLY for pure
            // block-scoped variables in non-deferred contexts:
            // - Static blocks and regular code → emit companion
            // - Computed property names → NO companion
            // - Static property initializers → NO companion
            // - Heritage clauses → NO companion
            // - Class/enum declarations → NO companion (they get TS2449/TS2450)
            if !is_tdz_in_property_initializer
                && !is_tdz_in_heritage_clause
                && !self.is_in_static_property_initializer_ast_context(idx)
                && self.ctx.strict_null_checks()
                && self.ctx.binder.symbols.get(sym_id).is_some_and(|sym| {
                    sym.flags & tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE != 0
                        && sym.flags
                            & (tsz_binder::symbol_flags::CLASS
                                | tsz_binder::symbol_flags::REGULAR_ENUM)
                            == 0
                })
                && let Some(usage_node) = self.ctx.arena.get(idx)
            {
                let key = (usage_node.pos, sym_id);
                if self.ctx.emitted_ts2454_errors.insert(key) {
                    self.error_variable_used_before_assigned_at(name, idx);
                }
            }

            // TS2729 companion for static property initializers:
            // in `X.Y`, when `X` is in TDZ, tsc also reports that `Y` is used
            // before initialization at the property name site.
            if self.is_in_static_property_initializer_ast_context(idx)
                && let Some(ext) = self.ctx.arena.get_extended(idx)
                && !ext.parent.is_none()
                && let Some(parent) = self.ctx.arena.get(ext.parent)
                && parent.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.ctx.arena.get_access_expr(parent)
                && access.expression == idx
                && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                && let Some(name_ident) = self.ctx.arena.get_identifier(name_node)
            {
                self.error_at_node(
                    access.name_or_argument,
                    &format!(
                        "Property '{}' is used before its initialization.",
                        name_ident.escaped_text
                    ),
                    diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                );
            }

            // TS2538: When a variable is used before declaration in a computed property,
            // it has implicit type 'any', which cannot be used as an index type.
            // Emit this additional error for computed property contexts.
            let is_in_computed_property =
                self.is_variable_used_before_declaration_in_computed_property(sym_id, idx);
            if is_in_computed_property {
                let message = format_message(
                    diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                    &["any"],
                );
                self.error_at_node(
                    idx,
                    &message,
                    diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                );
            }
        }
        is_tdz
    }

    /// Returns true when `usage_idx` is lexically inside a static class property
    /// initializer (`static x = ...`).
    fn is_in_static_property_initializer_ast_context(&self, usage_idx: NodeIndex) -> bool {
        let mut current = usage_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            if ext.parent.is_none() {
                break;
            }
            let parent = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                if let Some(prop) = self.ctx.arena.get_property_decl(parent_node) {
                    return !prop.initializer.is_none()
                        && self.has_static_modifier(&prop.modifiers);
                }
                return false;
            }
            current = parent;
        }
        false
    }

    /// Resolve the value-side type from a symbol's value declaration node.
    ///
    /// This is used for merged interface+value globals where value position must
    /// use the constructor/variable declaration type, not the interface type.
    /// Check if a value declaration has a self-referential type annotation.
    /// For example, `declare var Math: Math` has type annotation "Math"
    /// which matches the symbol name "Math". This pattern is common for
    /// lib globals that follow the `declare var X: X` pattern.
    fn is_self_referential_var_type(
        &self,
        _sym_id: SymbolId,
        value_decl: NodeIndex,
        name: &str,
    ) -> bool {
        // Try to find the value declaration in the current arena first
        if let Some(node) = self.ctx.arena.get(value_decl)
            && let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
            && !var_decl.type_annotation.is_none()
            && let Some(type_node) = self.ctx.arena.get(var_decl.type_annotation)
            && let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
            && let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            return ident.escaped_text == name;
        }

        // For declarations in other arenas (lib files), check via declaration_arenas
        if let Some(decl_arena) = self
            .ctx
            .binder
            .declaration_arenas
            .get(&(_sym_id, value_decl))
            .and_then(|v| v.first())
            && let Some(node) = decl_arena.get(value_decl)
            && let Some(var_decl) = decl_arena.get_variable_declaration(node)
            && !var_decl.type_annotation.is_none()
            && let Some(type_node) = decl_arena.get(var_decl.type_annotation)
            && let Some(type_ref) = decl_arena.get_type_ref(type_node)
            && let Some(name_node) = decl_arena.get(type_ref.type_name)
            && let Some(ident) = decl_arena.get_identifier(name_node)
        {
            return ident.escaped_text == name;
        }

        false
    }

    fn type_of_value_declaration(&mut self, decl_idx: NodeIndex) -> TypeId {
        if decl_idx.is_none() {
            return TypeId::UNKNOWN;
        }

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return TypeId::UNKNOWN;
        };

        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
            if !var_decl.type_annotation.is_none() {
                let annotated = self.get_type_from_type_node(var_decl.type_annotation);
                return self.resolve_ref_type(annotated);
            }
            if !var_decl.initializer.is_none() {
                return self.get_type_of_node(var_decl.initializer);
            }
            return TypeId::ANY;
        }

        if self.ctx.arena.get_function(node).is_some() {
            return self.get_type_of_function(decl_idx);
        }

        if let Some(class_data) = self.ctx.arena.get_class(node) {
            return self.get_class_constructor_type(decl_idx, class_data);
        }

        TypeId::UNKNOWN
    }

    /// Resolve a value declaration type, delegating to the declaration's arena
    /// when the node does not belong to the current checker arena.
    fn type_of_value_declaration_for_symbol(
        &mut self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> TypeId {
        if decl_idx.is_none() {
            return TypeId::UNKNOWN;
        }

        // Check declaration_arenas FIRST for the precise arena mapping.
        // This is critical for lib symbols where the same NodeIndex can exist
        // in both the lib arena and the main arena (cross-arena collision).
        // If we checked arena.get() first, we'd read a wrong node from the
        // main arena instead of the correct node from the lib arena.
        let decl_arena = if let Some(da) = self
            .ctx
            .binder
            .declaration_arenas
            .get(&(sym_id, decl_idx))
            .and_then(|v| v.first())
        {
            if std::ptr::eq(da.as_ref(), self.ctx.arena) {
                return self.type_of_value_declaration(decl_idx);
            }
            Some(std::sync::Arc::clone(da))
        } else if self.ctx.arena.get(decl_idx).is_some() {
            // Node exists in current arena but no declaration_arenas entry.
            // For non-lib symbols: this is the correct arena — use fast path.
            // For lib symbols: this may be a cross-arena collision — use symbol_arenas.
            if !self.ctx.binder.symbol_arenas.contains_key(&sym_id) {
                return self.type_of_value_declaration(decl_idx);
            }
            self.ctx.binder.symbol_arenas.get(&sym_id).cloned()
        } else {
            None
        };
        let Some(decl_arena) = decl_arena else {
            return TypeId::UNKNOWN;
        };
        if std::ptr::eq(decl_arena.as_ref(), self.ctx.arena) {
            return self.type_of_value_declaration(decl_idx);
        }

        // For lib declarations, check if the type annotation is a simple type reference
        // to a global lib type. If so, use resolve_lib_type_by_name directly instead of
        // creating a child checker. The child checker inherits the parent's merged binder,
        // which can have wrong symbol IDs for lib types, causing incorrect type resolution.
        if let Some(node) = decl_arena.get(decl_idx)
            && let Some(var_decl) = decl_arena.get_variable_declaration(node)
            && !var_decl.type_annotation.is_none()
        {
            // Try to extract the type name from a simple type reference
            if let Some(type_annotation_node) = decl_arena.get(var_decl.type_annotation)
                && let Some(type_ref) = decl_arena.get_type_ref(type_annotation_node)
            {
                // Check if this is a simple identifier (not qualified name)
                if let Some(type_name_node) = decl_arena.get(type_ref.type_name)
                    && let Some(ident) = decl_arena.get_identifier(type_name_node)
                {
                    let type_name = ident.escaped_text.as_str();
                    // Use resolve_lib_type_by_name for global lib types
                    if let Some(lib_type) = self.resolve_lib_type_by_name(type_name)
                        && lib_type != TypeId::UNKNOWN
                        && lib_type != TypeId::ERROR
                    {
                        return self.resolve_ref_type(lib_type);
                    }
                }
            }
        }

        // Guard against deep cross-arena recursion (shared with all delegation points)
        if !Self::enter_cross_arena_delegation() {
            return TypeId::UNKNOWN;
        }

        let mut checker = Box::new(CheckerState::with_parent_cache(
            decl_arena.as_ref(),
            self.ctx.binder,
            self.ctx.types,
            self.ctx.file_name.clone(),
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.symbol_resolution_set = self.ctx.symbol_resolution_set.clone();
        checker.ctx.symbol_resolution_stack = self.ctx.symbol_resolution_stack.clone();
        checker
            .ctx
            .symbol_resolution_depth
            .set(self.ctx.symbol_resolution_depth.get());
        let result = checker.type_of_value_declaration(decl_idx);

        // DO NOT merge child's symbol_types back. See delegate_cross_arena_symbol_resolution
        // for the full explanation: node_symbols collisions across arenas cause cache poisoning.

        Self::leave_cross_arena_delegation();
        result
    }

    /// Resolve a value-side type by global name, preferring value declarations.
    ///
    /// This avoids incorrect type resolution when symbol IDs collide across
    /// binders (current file vs. lib files).
    fn type_of_value_symbol_by_name(&mut self, name: &str) -> TypeId {
        if let Some((sym_id, value_decl)) = self.find_value_declaration_in_libs(name) {
            let value_type = self.type_of_value_declaration_for_symbol(sym_id, value_decl);
            if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                return value_type;
            }
        }

        if let Some(value_sym_id) = self.find_value_symbol_in_libs(name) {
            let value_type = self.get_type_of_symbol(value_sym_id);
            if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                return value_type;
            }
        }

        TypeId::UNKNOWN
    }

    /// If `type_id` is an object type with a synthetic `"new"` member, return that member type.
    /// This supports constructor-like interfaces that lower construct signatures as properties.
    pub(crate) fn constructor_type_from_new_property(&self, type_id: TypeId) -> Option<TypeId> {
        let shape_id = query::object_shape_id(self.ctx.types, type_id)?;

        let new_atom = self.ctx.types.intern_string("new");
        let shape = self.ctx.types.object_shape(shape_id);
        shape
            .properties
            .iter()
            .find(|prop| prop.name == new_atom)
            .map(|prop| prop.type_id)
    }

    /// Extract a partial object type from non-sensitive properties of an object literal.
    ///
    /// Used during Round 1 of two-pass generic inference to get type information
    /// from concrete properties (like `state: 100`) while skipping context-sensitive
    /// properties (like `actions: { foo: s => s }`).
    ///
    /// This lets inference learn e.g. `State = number` from `state: 100` even when
    /// the overall object literal is context-sensitive.
    fn extract_non_sensitive_object_type(&mut self, idx: NodeIndex) -> Option<TypeId> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let obj = self.ctx.arena.get_literal_expr(node)?;

        let mut properties = Vec::new();

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Property assignment: { x: value }
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                // Skip sensitive property initializers (lambdas, nested sensitive objects)
                if is_contextually_sensitive(self, prop.initializer) {
                    continue;
                }
                if let Some(name) = self.get_property_name(prop.name) {
                    // Compute type without contextual type
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = None;
                    let value_type = self.get_type_of_node(prop.initializer);
                    self.ctx.contextual_type = prev_context;

                    let name_atom = self.ctx.types.intern_string(&name);
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
                }
            }
            // Shorthand property: { x }
            else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                && let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node)
                && let Some(name_node) = self.ctx.arena.get(shorthand.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.clone();
                let value_type = self.get_type_of_node(shorthand.name);
                let name_atom = self.ctx.types.intern_string(&name);
                properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
            }
            // Methods and accessors are always context-sensitive — skip them
        }

        if properties.is_empty() {
            return None;
        }

        Some(self.ctx.types.factory().object(properties))
    }
}
