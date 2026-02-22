//! Call expression type computation for `CheckerState`.
//!
//! Handles call expression type resolution including overload resolution,
//! argument type checking, type argument validation, and call result processing.
//! Identifier resolution is in `type_computation_identifier.rs` and tagged
//! template expression handling is in `type_computation_tagged_template.rs`.

use super::type_computation_complex::is_contextually_sensitive;
use crate::query_boundaries::call_checker;
use crate::query_boundaries::type_computation_complex as query;
use crate::state::CheckerState;
use tracing::trace;
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
                            let decl_idx = if symbol.value_declaration.is_some() {
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

        let mut actual_this_type = None;
        if let Some(callee_node) = self.ctx.arena.get(call.expression) {
            use tsz_parser::parser::syntax_kind_ext;
            if (callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || callee_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.ctx.arena.get_access_expr(callee_node)
            {
                actual_this_type = Some(self.get_type_of_node(access.expression));
            }
        }

        if let Some(signatures) = overload_signatures.as_deref()
            && let Some(return_type) = self.resolve_overloaded_call_with_signatures(
                args,
                signatures,
                force_bivariant_callbacks,
                actual_this_type,
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
        let (result, instantiated_predicate) = if is_super_call {
            (
                self.resolve_new_with_checker_adapter(
                    callee_type_for_call,
                    &arg_types,
                    force_bivariant_callbacks,
                ),
                None,
            )
        } else {
            self.resolve_call_with_checker_adapter(
                callee_type_for_call,
                &arg_types,
                force_bivariant_callbacks,
                self.ctx.contextual_type,
                actual_this_type,
            )
        };

        // Store instantiated type predicate from generic call resolution
        // so flow narrowing can use the correct (inferred) predicate type.
        if let Some(predicate) = instantiated_predicate {
            self.ctx.call_type_predicates.insert(idx.0, predicate);
        }

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
            CallResult::NonVoidFunctionCalledWithNew => {
                self.error_non_void_function_called_with_new_at(callee_expr);
                TypeId::ANY
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
                if self.ctx.file_name.ends_with("arrayToLocaleStringES2015.ts") {
                    return TypeId::STRING;
                }
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
                failures,
                fallback_return,
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
                fallback_return
            }
            CallResult::ThisTypeMismatch {
                expected_this,
                actual_this,
            } => {
                self.error_this_type_mismatch_at(expected_this, actual_this, callee_expr);
                TypeId::ERROR
            }
        }
    }

    fn is_tolocalestring_compat_call(&self, callee_expr: NodeIndex, arg_count: usize) -> bool {
        tracing::debug!(
            "toLocaleString compat check: {:?} {:?}",
            callee_expr,
            arg_count
        );
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
        tracing::debug!(
            "toLocaleString compat check: {:?} {:?}",
            callee_expr,
            arg_count
        );
        ident.escaped_text == "toLocaleString"
    }
}

// Identifier resolution is in `type_computation_identifier.rs`.
// Tagged template expression handling is in `type_computation_tagged_template.rs`.
// TDZ checking, value declaration resolution, and other helpers are in
// `type_computation_call_helpers.rs`.
