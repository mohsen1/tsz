//! Call expression type computation for `CheckerState`.
//!
//! Handles call expression type resolution including overload resolution,
//! argument type checking, type argument validation, and call result processing.
//! Identifier resolution is in `identifier.rs` and tagged
//! template expression handling is in `tagged_template.rs`.

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
use tsz_solver::TypeId;

use super::call_result::CallResultContext;
use super::complex::is_contextually_sensitive;

impl<'a> CheckerState<'a> {
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

        self.ctx.call_depth.borrow_mut().leave();
        result
    }

    /// Inner implementation of call expression type resolution.
    pub(crate) fn get_type_of_call_expression_inner(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;
        let contextual_type = request.contextual_type;
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return TypeId::ERROR; // Missing call expression data - propagate error
        };
        if self.is_unshadowed_commonjs_require_identifier(call.expression)
            && let Some(args) = &call.arguments
            && let Some(first_arg) = args.nodes.first().copied()
            && let Some(module_specifier) = self.get_require_module_specifier(first_arg)
        {
            if let Some(module_type) =
                self.commonjs_module_value_type(&module_specifier, Some(self.ctx.current_file_idx))
            {
                return module_type;
            }
            self.emit_module_not_found_error(&module_specifier, first_arg);
            return TypeId::ANY;
        }

        // For IIFEs, wrap the contextual type into a callable type so
        // the function expression resolver can extract the return type.
        let iife_info = self.setup_iife_contextual_type(call.expression, contextual_type);
        let callee_request = iife_info
            .map(|(wrapper_fn, _)| request.read().contextual(wrapper_fn))
            .unwrap_or(*request);
        if iife_info.is_some() {
            self.invalidate_expression_for_contextual_retry(call.expression);
        }

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

        trace!(
            callee_type = ?callee_type,
            callee_expr = ?call.expression,
            "Call expression callee type resolved"
        );

        // Check for dynamic import module resolution (TS2307)
        if self.is_dynamic_import(call) {
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

            // TS2712: Dynamic import requires Promise constructor.
            // When the lib doesn't include Promise as a value (e.g., @lib: es5),
            // dynamic import() cannot work because it returns a Promise.
            if !self.ctx.has_promise_constructor_in_scope() {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    idx,
                    diagnostic_messages::A_DYNAMIC_IMPORT_CALL_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YOU_HAVE,
                    diagnostic_codes::A_DYNAMIC_IMPORT_CALL_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YOU_HAVE,
                );
            }

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
            }
            // Still need to check arguments for definite assignment (TS2454) and other errors.
            // Return Some(ANY) for every index so spread arguments are accepted (avoids
            // false TS2556 — `any` is callable with any arguments).
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| Some(TypeId::ANY),
                check_excess_properties,
                None, // No skipping needed
                CallableContext::none(),
            );
            return TypeId::ANY;
        }
        if callee_type == TypeId::ERROR {
            self.reemit_namespace_value_error_for_call_callee(call.expression);
            // Still evaluate type arguments to catch TS2304 for unresolved type names
            // (e.g., `this.super<T>(0)` where T is undeclared)
            if let Some(ref type_args_list) = call.type_arguments {
                for &arg_idx in &type_args_list.nodes {
                    self.get_type_from_type_node(arg_idx);
                }
            }
            // Still need to check arguments for definite assignment (TS2454) and other errors.
            // Return Some(ANY) for every index so spread arguments are accepted (avoids
            // false TS2556 when the callee couldn't be resolved).
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| Some(TypeId::ANY),
                check_excess_properties,
                None, // No skipping needed
                CallableContext::none(),
            );
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        // TS18046: Calling an expression of type `unknown` is not allowed.
        // tsc emits TS18046 instead of TS2349 when the callee is `unknown`.
        // Without strictNullChecks, unknown is treated like any (callable, returns any).
        if callee_type == TypeId::UNKNOWN {
            if self.error_is_of_type_unknown(call.expression) {
                // Still need to check arguments for definite assignment (TS2454)
                let check_excess_properties = false;
                self.collect_call_argument_types_with_context(
                    args,
                    |_i, _arg_count| None,
                    check_excess_properties,
                    None,
                    CallableContext::none(),
                );
                return TypeId::ERROR;
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
            return TypeId::ANY;
        }

        // Calling `never` returns `never` (bottom type propagation).
        // tsc treats `never` as having no call signatures.
        // For method calls (e.g., `a.toFixed()` where `a: never`), TS2339 is already
        // emitted by the property access check, so we suppress the redundant TS2349.
        // For direct calls on `never` (e.g., `f()` where `f: never`), emit TS2349.
        if callee_type == TypeId::NEVER {
            let is_method_call = matches!(
                self.ctx.arena.get(call.expression).map(|n| n.kind),
                Some(
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        | syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                )
            );
            if !is_method_call {
                self.error_not_callable_at(callee_type, call.expression);
            }
            return TypeId::NEVER;
        }

        let mut nullish_cause = None;
        if (node.flags as u32) & node_flags::OPTIONAL_CHAIN != 0 {
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
        let mut type_arg_count_mismatch = false;
        if let Some(ref type_args_list) = call.type_arguments
            && !type_args_list.nodes.is_empty()
        {
            type_arg_count_mismatch =
                self.validate_call_type_arguments(callee_type, type_args_list, idx);

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

        // When the type argument count is wrong (TS2558 already emitted), don't proceed
        // with argument type checking against the incorrectly-instantiated signature.
        // tsc skips argument checking in this case. Without this guard, the checker
        // would run generic inference on an uninstantiated signature and emit spurious
        // TS2345 errors for arguments that are actually valid.
        if type_arg_count_mismatch {
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
        // When the callee is a Union type, do NOT treat the collected member
        // signatures as overloads. Union call semantics require the call to be
        // valid for ALL members (handled by solver's resolve_union_call), while
        // overload resolution accepts the call if ANY single signature matches.
        // Without this guard, `(F1 | F2)("a")` would succeed if F1 alone accepts
        // 1 arg, silently ignoring F2 which requires 2 args — missing TS2554.
        let callee_is_union = common::is_union_type(self.ctx.types, callee_type_for_resolution);
        let overload_signatures = if callee_is_union {
            None
        } else {
            match classification {
                query::CallSignaturesKind::Callable(_) => {
                    // Delegate to solver query for overload detection
                    call_checker::get_overload_call_signatures(
                        self.ctx.types,
                        callee_type_for_resolution,
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
            self.ctx.arena.get(unwrapped_callee).map(|n| n.kind),
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
            return self.handle_call_result(
                overload_resolution.result,
                CallResultContext {
                    callee_expr: call.expression,
                    call_idx: idx,
                    args,
                    arg_types: &overload_resolution.arg_types,
                    callee_type: callee_type_for_resolution,
                    is_super_call: false,
                    is_optional_chain: nullish_cause.is_some(),
                    allow_contextual_mismatch_deferral: true,
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
        // Extract the shape from the same resolved callee type used for contextual typing.
        // Using a less-resolved form here can make Round 2 infer from a pre-instantiation
        // method signature even though callback contextual typing is based on the fully
        // resolved receiver-specific callable type.
        let callee_shape = call_checker::get_contextual_signature_for_arity(
            self.ctx.types,
            callee_type_for_context,
            args.len(),
        );
        let is_generic_call = callee_shape
            .as_ref()
            .is_some_and(|s| !s.type_params.is_empty())
            && call.type_arguments.is_none(); // Only use two-pass if no explicit type args
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
            })
            .collect();
        // For union callees, skip excess property checking during argument collection.
        // The solver's resolve_union_call intersects parameter types across members,
        // so `{x: 0, y: 0}` is valid for `((a: {x}) => R) | ((a: {y}) => R)` even
        // though it has "excess" properties against each individual member type.
        let check_excess_properties = overload_signatures.is_none() && !callee_is_union;
        // Two-pass argument collection for generic calls is only needed when at least one
        // argument is contextually sensitive (e.g. lambdas/object literals needing contextual type).
        // Preserve literal types in array literals during generic call argument collection.
        // This ensures `['foo', 'bar']` is typed as `("foo" | "bar")[]` (not `string[]`),
        // enabling correct type parameter inference (e.g., K = "foo" | "bar").
        // tsc preserves literals during inference and only widens at assignment sites.
        let prev_preserve_literals = self.ctx.preserve_literal_types;
        let prev_generic_excess_skip = self.ctx.generic_excess_skip.take();
        let callable_ctx = CallableContext::new(callee_type_for_context);
        if is_generic_call {
            self.ctx.preserve_literal_types = true;
        }
        let mut non_generic_contextual_types: Option<Vec<Option<TypeId>>> = None;
        // Track whether we pushed a ThisType marker to this_type_stack during call processing.
        let mut pushed_this_type_from_shape = false;
        // Extracted ThisType<T> from shape params (via alias expansion if needed).
        // Saved so it can be re-pushed around retry arg typing calls.
        let mut shape_this_type: Option<TypeId> = None;
        // Track whether Round 2 successfully used a non-empty return context substitution.
        // When true, the post-inference retry should be suppressed because Round 2 already
        // correctly resolved the callback parameter types using the return context.
        let mut had_return_context_substitution = false;
        let mut arg_types = if is_generic_call {
            if let Some(shape) = callee_shape {
                // Pre-compute which parameter positions should skip excess property
                // checking because the original parameter type contains a type parameter.
                // For generic calls like `parrot<T extends Named>({name, sayHello(){}})`,
                // the instantiated type is the constraint `Named`, but tsc skips excess
                // property checks because `T` captures the full object type.
                //
                // Use the raw FunctionShape parameter types (which preserve type parameters)
                // rather than ctx_helper.get_parameter_type_for_call (which may resolve
                // through Lazy/Application types and lose type parameter information).
                let excess_skip: Vec<bool> = {
                    let arg_count = args.len();
                    (0..arg_count)
                        .map(|i| {
                            // Check both the raw shape parameter type and the contextual
                            // parameter type. Rest parameters use the last param, and the
                            // contextual helper handles that mapping.
                            let from_shape = if i < shape.params.len() {
                                common::contains_type_parameters(
                                    self.ctx.types,
                                    shape.params[i].type_id,
                                )
                            } else if let Some(last) = shape.params.last() {
                                // Rest parameter: check the rest param's type
                                last.rest
                                    && common::contains_type_parameters(
                                        self.ctx.types,
                                        last.type_id,
                                    )
                            } else {
                                false
                            };
                            let from_ctx = ctx_helper
                                .get_parameter_type_for_call(i, arg_count)
                                .is_some_and(|param_type| {
                                    common::contains_type_parameters(self.ctx.types, param_type)
                                });
                            from_shape || from_ctx
                        })
                        .collect()
                };
                let has_any_excess_skip = excess_skip.iter().any(|&s| s);
                if has_any_excess_skip {
                    self.ctx.generic_excess_skip = Some(excess_skip);
                }

                // Pre-compute which arguments are contextually sensitive to avoid borrowing self in closures.
                let sensitive_args: Vec<bool> = args
                    .iter()
                    .map(|&arg| is_contextually_sensitive(self, arg))
                    .collect();
                let suppress_generic_return_context = args
                    .iter()
                    .copied()
                    .any(|arg| self.suppress_generic_return_context_for_arg(arg))
                    || self.suppress_generic_return_context_for_direct_arg_overlap(&shape, args);
                let generic_inference_contextual_type = if suppress_generic_return_context {
                    None
                } else {
                    contextual_type
                };
                trace!(
                    type_params = ?shape
                        .type_params
                        .iter()
                        .map(|tp| self.ctx.types.resolve_atom(tp.name))
                        .collect::<Vec<_>>(),
                    generic_inference_contextual_type = ?generic_inference_contextual_type.map(|t| t.0),
                    suppress_generic_return_context,
                    "Generic call contextual type gate"
                );
                let round1_skip_outer_context: Vec<bool> = args
                    .iter()
                    .map(|&arg| self.round1_should_skip_outer_contextual_type(arg))
                    .collect();
                let needs_two_pass = sensitive_args.iter().copied().any(std::convert::identity);

                // Extract ThisType<T> from shape params via alias expansion.
                // Store for re-use across retry arg typing calls.
                if shape_this_type.is_none() {
                    let _env = self.ctx.type_env.borrow();
                    for param in &shape.params {
                        let ctx_helper = ContextualTypeContext::with_expected_and_options(
                            self.ctx.types,
                            param.type_id,
                            self.ctx.compiler_options.no_implicit_any,
                        );
                        if let Some(tt) = ctx_helper.get_this_type_from_marker() {
                            shape_this_type = Some(tt);
                            break;
                        }
                    }
                }

                if needs_two_pass {
                    // Round 1: skip sensitive args so inference can start from concrete inputs.
                    let round1_diag_snap = contextual_type
                        .is_some()
                        .then(|| self.ctx.snapshot_diagnostics());
                    // Also snapshot TS2454 dedup state so that rolling back Round 1
                    // diagnostics allows TS2454 to be re-emitted during Round 2.
                    // Without this, TS2454 emitted during Round 1 gets removed from
                    // the diagnostics vector but remains in the dedup set, silencing
                    // the error in Round 2.
                    let round1_ts2454_snap = round1_diag_snap
                        .as_ref()
                        .map(|_| self.ctx.emitted_ts2454_errors.clone());
                    let mut round1_arg_types = self.collect_call_argument_types_with_context(
                        args,
                        |i, _arg_count| {
                            let skip_round1_context = (i < sensitive_args.len()
                                && sensitive_args[i])
                                || (i < round1_skip_outer_context.len()
                                    && round1_skip_outer_context[i]);
                            if skip_round1_context {
                                None
                            } else {
                                base_contextual_param_types.get(i).copied().flatten()
                            }
                        },
                        check_excess_properties,
                        Some(&sensitive_args), // Skip sensitive args in Round 1
                        callable_ctx,
                    );

                    // Seed inference from non-sensitive object-literal properties.
                    let mut extracted_round1_partials = vec![false; args.len()];
                    let type_param_names: Vec<tsz_common::Atom> =
                        shape.type_params.iter().map(|tp| tp.name).collect();
                    for (i, &arg_idx) in args.iter().enumerate() {
                        if !sensitive_args[i] {
                            continue;
                        }
                        let param_type = shape.params.get(i).map(|p| p.type_id).or_else(|| {
                            let last = shape.params.last()?;
                            last.rest.then_some(last.type_id)
                        });
                        let partial = param_type
                            .and_then(|param_type| {
                                self.extract_inference_contributing_object_type(
                                    arg_idx,
                                    param_type,
                                    &type_param_names,
                                )
                                .or_else(|| {
                                    self.extract_inference_contributing_array_type(
                                        arg_idx,
                                        param_type,
                                        &type_param_names,
                                    )
                                })
                            })
                            .or_else(|| self.extract_non_sensitive_object_type(arg_idx));
                        if let Some(partial) = partial {
                            round1_arg_types[i] = partial;
                            extracted_round1_partials[i] = true;
                        }
                    }
                    // If only callback members remain, include ones with concrete param context.
                    for (i, &arg_idx) in args.iter().enumerate() {
                        if !sensitive_args[i] || extracted_round1_partials[i] {
                            continue;
                        }
                        let Some(param_type) =
                            shape.params.get(i).map(|p| p.type_id).or_else(|| {
                                let last = shape.params.last()?;
                                last.rest.then_some(last.type_id)
                            })
                        else {
                            continue;
                        };
                        if let Some(partial) = self
                            .extract_inference_contributing_object_type(
                                arg_idx,
                                param_type,
                                &type_param_names,
                            )
                            .or_else(|| {
                                self.extract_inference_contributing_array_type(
                                    arg_idx,
                                    param_type,
                                    &type_param_names,
                                )
                            })
                        {
                            round1_arg_types[i] = partial;
                            extracted_round1_partials[i] = true;
                        }
                    }
                    for (i, arg_type) in round1_arg_types.iter_mut().enumerate() {
                        if !sensitive_args.get(i).copied().unwrap_or(false) {
                            continue;
                        }
                        let Some(arg_node) = self.ctx.arena.get(args[i]) else {
                            continue;
                        };
                        if arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
                            && arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
                        {
                            continue;
                        }
                        let Some(param_type) =
                            shape.params.get(i).map(|p| p.type_id).or_else(|| {
                                let last = shape.params.last()?;
                                last.rest.then_some(last.type_id)
                            })
                        else {
                            continue;
                        };
                        if extracted_round1_partials.get(i).copied().unwrap_or(false) {
                            continue;
                        }
                        if self.sensitive_callback_placeholder_should_skip_round1_inference(
                            &shape, param_type,
                        ) {
                            *arg_type = TypeId::UNKNOWN;
                        }
                    }
                    // Nested calls whose outer contextual type was intentionally skipped in
                    // Round 1 should not poison outer inference with provisional `error` or
                    // `__infer_*` results. Leave them for Round 2 unless they resolved cleanly.
                    for (i, arg_type) in round1_arg_types.iter_mut().enumerate() {
                        if !round1_skip_outer_context.get(i).copied().unwrap_or(false) {
                            continue;
                        }
                        let unresolved =
                            common::is_unresolved_inference_result(self.ctx.types, *arg_type);
                        if unresolved {
                            *arg_type = TypeId::UNKNOWN;
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
                            .enumerate()
                            .map(|(i, p)| {
                                let arg_type = round1_arg_types.get(i).copied();
                                let preserve_raw_application = arg_type.is_some_and(|arg_type| {
                                    common::should_preserve_application_for_inference(
                                        self.ctx.types,
                                        p.type_id,
                                        arg_type,
                                    )
                                });

                                common::ParamInfo {
                                    name: p.name,
                                    type_id: if preserve_raw_application {
                                        p.type_id
                                    } else {
                                        self.evaluate_type_with_env(p.type_id)
                                    },
                                    optional: p.optional,
                                    rest: p.rest,
                                }
                            })
                            .collect();
                        common::FunctionShape {
                            params: new_params,
                            return_type: shape.return_type,
                            this_type: shape.this_type,
                            type_params: shape.type_params.clone(),
                            type_predicate: shape.type_predicate.clone(),
                            is_constructor: shape.is_constructor,
                            is_method: shape.is_method,
                        }
                    };
                    let mut substitution = {
                        let env = self.ctx.type_env.borrow();
                        call_checker::compute_contextual_types_with_context(
                            self.ctx.types,
                            &self.ctx,
                            &env,
                            &evaluated_shape,
                            &round1_arg_types,
                            generic_inference_contextual_type,
                        )
                    };

                    // Extract ThisType<T> marker from raw parameter types and
                    // instantiate with the Round 1 substitution. Push to
                    // this_type_stack so nested object literal methods resolve
                    // `this` to the inferred type.
                    if !pushed_this_type_from_shape {
                        for param in &shape.params {
                            let ctx_helper = ContextualTypeContext::with_expected_and_options(
                                self.ctx.types,
                                param.type_id,
                                self.ctx.compiler_options.no_implicit_any,
                            );
                            if let Some(this_type) = ctx_helper.get_this_type_from_marker() {
                                let instantiated =
                                    crate::query_boundaries::common::instantiate_type(
                                        self.ctx.types,
                                        this_type,
                                        &substitution,
                                    );
                                self.ctx.this_type_stack.push(instantiated);
                                pushed_this_type_from_shape = true;
                                break;
                            }
                        }
                    }

                    for (i, &arg_idx) in args.iter().enumerate() {
                        if !sensitive_args.get(i).copied().unwrap_or(false) {
                            continue;
                        }
                        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                            continue;
                        };
                        if arg_node.kind != syntax_kind_ext::ARROW_FUNCTION
                            && arg_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
                        {
                            continue;
                        }
                        let Some(param_type) =
                            shape.params.get(i).map(|p| p.type_id).or_else(|| {
                                let last = shape.params.last()?;
                                last.rest.then_some(last.type_id)
                            })
                        else {
                            continue;
                        };

                        let names_to_strip: Vec<_> = shape
                            .type_params
                            .iter()
                            .filter_map(|tp| {
                                substitution.get(tp.name).and_then(|inferred| {
                                    self.should_strip_sensitive_placeholder_substitution(
                                        &shape, param_type, tp.name, inferred,
                                    )
                                    .then_some(tp.name)
                                })
                            })
                            .collect();
                        if !names_to_strip.is_empty() {
                            let names_to_strip: rustc_hash::FxHashSet<_> =
                                names_to_strip.into_iter().collect();
                            let mut filtered =
                                crate::query_boundaries::common::TypeSubstitution::new();
                            for (&name, &type_id) in substitution.map() {
                                if !names_to_strip.contains(&name) {
                                    filtered.insert(name, type_id);
                                }
                            }
                            substitution = filtered;
                        }
                    }
                    let inferred_type_params_by_name: Vec<_> = shape
                        .type_params
                        .iter()
                        .filter_map(|tp| {
                            substitution
                                .get(tp.name)
                                .map(|ty| (self.ctx.types.resolve_atom(tp.name), ty))
                        })
                        .collect();
                    trace!(
                        substitution_is_empty = substitution.is_empty(),
                        "Round 1 inference: substitution computed"
                    );
                    let mut round2_substitution = substitution.clone();
                    if let Some(ctx_type) = generic_inference_contextual_type {
                        let tracked_type_params: FxHashSet<_> =
                            shape.type_params.iter().map(|tp| tp.name).collect();
                        let return_context_substitution = self
                            .compute_return_context_substitution_from_shape(&shape, Some(ctx_type));
                        trace!(
                            type_params = ?shape
                                .type_params
                                .iter()
                                .map(|tp| self.ctx.types.resolve_atom(tp.name))
                                .collect::<Vec<_>>(),
                            contextual_type = ctx_type.0,
                            contextual_type_display = %self.format_type(ctx_type),
                            contextual_type_union_members = ?common::union_members(
                                self.ctx.types,
                                ctx_type,
                            )
                            .map(|members| members
                                .into_iter()
                                .map(|member| (
                                    self.format_type(member),
                                    query::get_application_info(self.ctx.types, member)
                                        .map(|(_, args)| args),
                                ))
                                .collect::<Vec<_>>()),
                            return_type_display = %self.format_type(shape.return_type),
                            return_context_substitution = ?return_context_substitution
                                .map()
                                .iter()
                                .map(|(name, ty)| (self.ctx.types.resolve_atom(*name), ty.0))
                                .collect::<Vec<_>>(),
                            "Round 2 return-context substitution"
                        );
                        for (&name, &ty) in return_context_substitution.map().iter() {
                            if ty == TypeId::UNKNOWN
                                || ty == TypeId::ERROR
                                || self.target_contains_blocking_return_context_type_params(
                                    ty,
                                    &tracked_type_params,
                                )
                            {
                                continue;
                            }

                            let should_update = match round2_substitution.get(name) {
                                None => true,
                                Some(existing) if existing == ty => false,
                                Some(existing) => {
                                    existing == TypeId::UNKNOWN
                                        || existing == TypeId::ERROR
                                        || self.inference_type_is_anyish(existing)
                                        || common::contains_type_parameters(
                                            self.ctx.types,
                                            existing,
                                        )
                                        || common::contains_infer_types(self.ctx.types, existing)
                                        || !assign_query::is_fresh_subtype_of(
                                            self.ctx.types,
                                            existing,
                                            ty,
                                        )
                                }
                            };

                            if should_update {
                                round2_substitution.insert(name, ty);
                                had_return_context_substitution = true;
                            }
                        }
                    }
                    for param in &evaluated_shape.params {
                        for referenced in
                            common::collect_referenced_types(self.ctx.types, param.type_id)
                        {
                            if let Some(info) = common::type_param_info(self.ctx.types, referenced)
                                && round2_substitution.get(info.name).is_none()
                            {
                                let param_name = self.ctx.types.resolve_atom(info.name);
                                if let Some((_, inferred)) = inferred_type_params_by_name
                                    .iter()
                                    .find(|(name, _)| name.as_str() == param_name.as_str())
                                {
                                    round2_substitution.insert(info.name, *inferred);
                                }
                            }
                        }
                    }
                    trace!("Round 2 substitution prepared");

                    // === Pre-inference from annotated callback parameters ===
                    // When a callback is context-sensitive (has unannotated params) AND has
                    // some annotated params, use those annotations to enrich the substitution
                    // BEFORE computing Round 2 contextual types. This matches tsc's behavior
                    // where annotated callback params contribute to inference even when the
                    // callback as a whole is context-sensitive.
                    //
                    // Example: test<T extends C>((t1: D, t2) => { t2.test2 })
                    //   - Round 1 skips the callback (it's sensitive)
                    //   - But t1: D tells us T = D
                    //   - Without this, T resolves to constraint C, causing false TS2551
                    for (i, &arg_idx) in args.iter().enumerate() {
                        if i < sensitive_args.len()
                            && sensitive_args[i]
                            && let Some(shape_param_type) = shape.params.get(i).map(|p| p.type_id)
                            && let Some(shape_fn) =
                                query::get_function_shape(self.ctx.types, shape_param_type)
                            && let Some(arg_node) = self.ctx.arena.get(arg_idx)
                            && let Some(func) = self.ctx.arena.get_function(arg_node)
                        {
                            for (j, &param_idx) in func.parameters.nodes.iter().enumerate() {
                                if let Some(param_node) = self.ctx.arena.get(param_idx)
                                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                                    && param.type_annotation.is_some()
                                    && let Some(shape_fn_param) = shape_fn.params.get(j)
                                    && let Some(tp_info) = query::type_parameter_info(
                                        self.ctx.types,
                                        shape_fn_param.type_id,
                                    )
                                {
                                    let is_callee_tp =
                                        shape.type_params.iter().any(|tp| tp.name == tp_info.name);
                                    // Only override the substitution if it was
                                    // defaulted to the constraint (not inferred
                                    // from concrete arguments).
                                    let existing = substitution.get(tp_info.name);
                                    let is_defaulted =
                                        existing.is_none() || existing == tp_info.constraint;
                                    if is_callee_tp && is_defaulted {
                                        let ann_type =
                                            self.get_type_from_type_node(param.type_annotation);
                                        substitution.insert(tp_info.name, ann_type);
                                        // Also update round2_substitution so contextual
                                        // typing of unannotated parameters sees the
                                        // inferred type from annotations.
                                        round2_substitution.insert(tp_info.name, ann_type);
                                        trace!(
                                            param_index = j,
                                            ann_type = ann_type.0,
                                            "Pre-inference: annotated callback param enriched substitution"
                                        );
                                    }
                                }
                            }
                        }
                    }

                    // Sanitize certain function-literal arg types for the second resolve_call
                    // pass. The sensitive placeholder `(any?) => any` from Round 1 can
                    // contaminate the solver's type parameter inference when the shape
                    // param is a bare type parameter or intersection (e.g., `T` or
                    // `T & Callback`). In those cases, T gets inferred as `(any?) => any`,
                    // producing Callable types with conflicting call signatures that break
                    // contextual typing and cause false TS7006 errors.
                    //
                    // However, when the shape param is a generic callable like
                    // `Predicate<A>`, the placeholder's callable structure is useful for
                    // inferring inner type params (A = any from placeholder params).
                    // Replacing with UNKNOWN would lose this inference (A = unknown).
                    //
                    // Rule: only sanitize when the shape param IS or CONTAINS a top-level
                    // type parameter (bare T, T & Callable, etc). Leave generic callables
                    // like Predicate<A> alone since those handle the placeholder correctly.
                    let sanitized_arg_types: Vec<TypeId> = round1_arg_types
                        .iter()
                        .enumerate()
                        .map(|(i, &ty)| {
                            if i < sensitive_args.len()
                                && sensitive_args[i]
                                && self.ctx.arena.get(args[i]).is_some_and(|n| {
                                    n.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                                        || n.kind == syntax_kind_ext::ARROW_FUNCTION
                                })
                                && shape.params.get(i).is_some_and(|p| {
                                    common::is_type_parameter_or_intersection_with_type_parameter(
                                        self.ctx.types,
                                        p.type_id,
                                    )
                                })
                            {
                                TypeId::UNKNOWN
                            } else {
                                ty
                            }
                        })
                        .collect();
                    let round1_instantiated_params = self
                        .resolve_call_with_checker_adapter(
                            callee_type_for_context,
                            &sanitized_arg_types,
                            force_bivariant_callbacks,
                            generic_inference_contextual_type,
                            actual_this_type,
                        )
                        .2;

                    if let Some(snap) = &round1_diag_snap {
                        self.ctx.rollback_diagnostics_filtered(snap, |diag| {
                            Self::should_preserve_speculative_call_diagnostic(diag)
                        });
                    }
                    if let Some(ts2454_snap) = &round1_ts2454_snap {
                        self.ctx.restore_ts2454_state(ts2454_snap);
                    }

                    // === Pre-evaluate instantiated parameter types ===
                    // After instantiation with Round 1 substitution, parameter types may
                    // contain unevaluated IndexAccess/KeyOf over Lazy(DefId) references
                    // (e.g., OptionsForKey[K] → OptionsForKey["a"]). The QueryCache's
                    // evaluate_type uses NoopResolver which can't resolve Lazy types.
                    // Use evaluate_type_with_env which resolves Lazy types via the
                    // TypeEnvironment before evaluation.
                    let arg_count = args.len();
                    let has_spread_args = args.iter().any(|&arg_idx| {
                        self.ctx
                            .arena
                            .get(arg_idx)
                            .is_some_and(|n| n.kind == syntax_kind_ext::SPREAD_ELEMENT)
                    });

                    if !has_spread_args {
                        let mut progressive_arg_types = round1_arg_types.clone();
                        let mut round2_arg_types = Vec::with_capacity(arg_count);

                        for (i, &arg_idx) in args.iter().enumerate() {
                            if sensitive_args.get(i).copied().unwrap_or(false)
                                && let Some(first_branch_idx) =
                                    self.zero_param_callback_first_conditional_branch(arg_idx)
                                && let Some(param_type) = shape.params.get(i).map(|p| p.type_id)
                                && let Some(callback_shape) =
                                    query::get_function_shape(self.ctx.types, param_type)
                            {
                                let first_branch_type = self.get_type_of_node(first_branch_idx);
                                let tracked_type_params: FxHashSet<_> =
                                    shape.type_params.iter().map(|tp| tp.name).collect();
                                let mut first_branch_substitution =
                                    crate::query_boundaries::common::TypeSubstitution::new();
                                let mut visited = FxHashSet::default();
                                self.collect_return_context_substitution(
                                    callback_shape.return_type,
                                    first_branch_type,
                                    &tracked_type_params,
                                    &mut first_branch_substitution,
                                    &mut visited,
                                );
                                for (&name, &ty) in first_branch_substitution.map().iter() {
                                    let should_update = match round2_substitution.get(name) {
                                        None => true,
                                        Some(existing) if existing == ty => false,
                                        Some(existing) => {
                                            existing == TypeId::UNKNOWN
                                                || common::contains_type_parameters(
                                                    self.ctx.types,
                                                    existing,
                                                )
                                                || common::contains_infer_types(
                                                    self.ctx.types,
                                                    existing,
                                                )
                                        }
                                    };
                                    if ty != TypeId::UNKNOWN
                                        && ty != TypeId::ERROR
                                        && !common::contains_type_parameters(self.ctx.types, ty)
                                        && !common::contains_infer_types(self.ctx.types, ty)
                                        && should_update
                                    {
                                        round2_substitution.insert(name, ty);
                                    }
                                }
                            }
                            if sensitive_args.get(i).copied().unwrap_or(false) {
                                self.clear_contextual_resolution_cache();
                                self.invalidate_expression_for_contextual_retry(arg_idx);
                            }
                            let contextual_substitution = self
                                .widen_round2_contextual_substitution(&shape, &round2_substitution);
                            let round2_contextual_types = self.compute_round2_contextual_types(
                                &shape,
                                round1_instantiated_params.as_deref(),
                                &sensitive_args,
                                &contextual_substitution,
                                arg_count,
                            );
                            let expected_type = round2_contextual_types
                                .get(i)
                                .copied()
                                .flatten()
                                .or_else(|| base_contextual_param_types.get(i).copied().flatten());
                            let arg_type = if expected_type.is_some_and(|expected| {
                                expected != TypeId::UNKNOWN
                                    && expected != TypeId::ERROR
                                    && !common::contains_infer_types(self.ctx.types, expected)
                                    && !common::contains_type_parameters(self.ctx.types, expected)
                            }) {
                                let (start, end) = self
                                    .ctx
                                    .arena
                                    .get(arg_idx)
                                    .map(|node| (node.pos, node.end))
                                    .unwrap_or((0, 0));
                                self.ctx.diagnostics.retain(|diag| {
                                    diag.start < start
                                        || diag.start >= end
                                        // TS2454 (variable used before being assigned) is a
                                        // semantic fact about the variable, not a speculative
                                        // inference artifact. Preserve it across round 2
                                        // re-typing so it isn't silently dropped.
                                        || diag.code
                                            == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
                                });
                                self.ctx.rebuild_emitted_diagnostics_from_current();
                                self.compute_single_call_argument_type(
                                    arg_idx,
                                    expected_type,
                                    check_excess_properties,
                                    i,
                                    args.len(),
                                    true,
                                    callable_ctx,
                                )
                            } else {
                                self.compute_single_call_argument_type(
                                    arg_idx,
                                    expected_type,
                                    check_excess_properties,
                                    i,
                                    args.len(),
                                    true,
                                    callable_ctx,
                                )
                            };
                            let arg_type_for_refinement = expected_type
                                .map(|expected| {
                                    if self
                                        .target_has_concrete_return_context_for_generic_refinement(
                                            expected,
                                        )
                                    {
                                        self.instantiate_generic_function_argument_against_target_for_refinement(
                                            arg_type, expected,
                                        )
                                    } else {
                                        self.instantiate_generic_function_argument_against_target_params(
                                            arg_type, expected,
                                        )
                                    }
                                })
                                .unwrap_or(arg_type);
                            trace!(
                                arg_index = i,
                                expected_type = ?expected_type.map(|t| t.0),
                                expected_type_display = ?expected_type.map(|t| self.format_type(t)),
                                arg_type = arg_type.0,
                                arg_type_display = %self.format_type(arg_type),
                                "Round 2: recomputed argument type"
                            );
                            round2_arg_types.push(arg_type);
                            if i < progressive_arg_types.len() {
                                progressive_arg_types[i] = arg_type_for_refinement;
                            }

                            if let Some(shape_param_type) =
                                shape.params.get(i).map(|p| p.type_id).or_else(|| {
                                    let last = shape.params.last()?;
                                    last.rest.then_some(last.type_id)
                                })
                            {
                                let tracked_type_params: FxHashSet<_> =
                                    shape.type_params.iter().map(|tp| tp.name).collect();
                                let mut arg_substitution =
                                    crate::query_boundaries::common::TypeSubstitution::new();
                                let mut visited = FxHashSet::default();
                                self.collect_return_context_substitution(
                                    shape_param_type,
                                    arg_type_for_refinement,
                                    &tracked_type_params,
                                    &mut arg_substitution,
                                    &mut visited,
                                );
                                for (&name, &raw_ty) in arg_substitution.map().iter() {
                                    let ty = if shape
                                        .type_params
                                        .iter()
                                        .find(|tp| tp.name == name)
                                        .is_some_and(|tp| !tp.is_const)
                                    {
                                        self.widen_literal_type(raw_ty)
                                    } else {
                                        raw_ty
                                    };
                                    if ty == TypeId::UNKNOWN
                                        || ty == TypeId::ERROR
                                        || self.target_contains_blocking_return_context_type_params(
                                            ty,
                                            &tracked_type_params,
                                        )
                                    {
                                        continue;
                                    }

                                    let should_update = match round2_substitution.get(name) {
                                        None => true,
                                        Some(existing) if existing == ty => false,
                                        Some(existing) => {
                                            existing == TypeId::UNKNOWN
                                                || existing == TypeId::ERROR
                                                || self.inference_type_is_anyish(existing)
                                                || common::contains_infer_types(
                                                    self.ctx.types,
                                                    existing,
                                                )
                                                || common::contains_type_parameters(
                                                    self.ctx.types,
                                                    existing,
                                                )
                                        }
                                    };
                                    if should_update {
                                        round2_substitution.insert(name, ty);
                                    }
                                }
                            }

                            let expected_still_unresolved = expected_type.is_some_and(|expected| {
                                common::contains_infer_types(self.ctx.types, expected)
                                    || common::contains_type_parameters(self.ctx.types, expected)
                            });
                            let arg_is_callable =
                                common::is_callable_type(self.ctx.types, arg_type);
                            let skip_return_only_refinement = self
                                .ctx
                                .arena
                                .get(arg_idx)
                                .and_then(|node| self.ctx.arena.get_function(node))
                                .is_some_and(|func| {
                                    func.parameters.nodes.is_empty()
                                        && func.type_annotation.is_none()
                                });
                            let should_refine_substitution =
                                sensitive_args.get(i).copied().unwrap_or(false)
                                    || (expected_still_unresolved && arg_is_callable);
                            if should_refine_substitution && !skip_return_only_refinement {
                                let refined_substitution = {
                                    let env = self.ctx.type_env.borrow();
                                    call_checker::compute_contextual_types_with_context(
                                        self.ctx.types,
                                        &self.ctx,
                                        &env,
                                        &evaluated_shape,
                                        &progressive_arg_types,
                                        generic_inference_contextual_type,
                                    )
                                };
                                let mut substitution_changed = false;
                                for (&name, &ty) in refined_substitution.map().iter() {
                                    if ty == TypeId::UNKNOWN
                                        || common::contains_infer_types(self.ctx.types, ty)
                                        || common::contains_type_parameters(self.ctx.types, ty)
                                    {
                                        continue;
                                    }

                                    let should_update = match round2_substitution.get(name) {
                                        None => true,
                                        Some(existing) if existing == ty => false,
                                        Some(existing) => {
                                            existing == TypeId::UNKNOWN
                                                || common::contains_infer_types(
                                                    self.ctx.types,
                                                    existing,
                                                )
                                                || common::contains_type_parameters(
                                                    self.ctx.types,
                                                    existing,
                                                )
                                        }
                                    };

                                    if should_update {
                                        round2_substitution.insert(name, ty);
                                        substitution_changed = true;
                                    }
                                }
                                if substitution_changed {
                                    trace!("Round 2 substitution refined");
                                }
                            }
                        }

                        round2_arg_types
                    } else {
                        let contextual_substitution =
                            self.widen_round2_contextual_substitution(&shape, &round2_substitution);
                        let round2_contextual_types = self.compute_round2_contextual_types(
                            &shape,
                            round1_instantiated_params.as_deref(),
                            &sensitive_args,
                            &contextual_substitution,
                            arg_count,
                        );

                        self.collect_call_argument_types_with_context(
                            args,
                            |i, _arg_count| {
                                if i < round2_contextual_types.len() {
                                    round2_contextual_types[i]
                                } else {
                                    base_contextual_param_types.get(i).copied().flatten()
                                }
                            },
                            check_excess_properties,
                            None,
                            callable_ctx,
                        )
                    }
                } else {
                    // Extract ThisType<T> marker from raw parameter types.
                    // In the single-pass path, no inference substitution is available,
                    // so we push the raw (uninstantiated) ThisType marker.
                    // This allows property access on `this` in object literal methods
                    // to suppress false TS2339 errors.
                    if !pushed_this_type_from_shape {
                        for param in &shape.params {
                            let ctx_helper2 = ContextualTypeContext::with_expected_and_options(
                                self.ctx.types,
                                param.type_id,
                                self.ctx.compiler_options.no_implicit_any,
                            );
                            if let Some(this_type) = ctx_helper2.get_this_type_from_marker() {
                                self.ctx.this_type_stack.push(this_type);
                                pushed_this_type_from_shape = true;
                                break;
                            }
                        }
                    }

                    // Single-pass generic calls still erase type params from empty-array
                    // contextual types so `[]` does not feed raw `T[]` back into inference.
                    let type_param_eraser = {
                        use crate::query_boundaries::common::TypeSubstitution;
                        let mut sub = TypeSubstitution::new();
                        for tp in &shape.type_params {
                            sub.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
                        }
                        sub
                    };
                    let arena = self.ctx.arena;
                    let single_pass_contextual_types: Vec<Option<TypeId>> = (0..args.len())
                        .map(|i| {
                            let param_type =
                                base_contextual_param_types.get(i).copied().flatten()?;
                            let is_empty_array_literal = arena.get(args[i]).is_some_and(|n| {
                                n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                    && arena
                                        .get_literal_expr(n)
                                        .is_some_and(|lit| lit.elements.nodes.is_empty())
                            });
                            let param_type = if is_empty_array_literal {
                                use crate::query_boundaries::common::instantiate_type;
                                instantiate_type(self.ctx.types, param_type, &type_param_eraser)
                            } else {
                                param_type
                            };
                            Some(self.normalize_contextual_call_param_type(param_type))
                        })
                        .collect();
                    let initial_arg_snap = contextual_type
                        .is_some()
                        .then(|| self.ctx.snapshot_diagnostics());
                    let initial_ts2454_snap = initial_arg_snap
                        .as_ref()
                        .map(|_| self.ctx.emitted_ts2454_errors.clone());
                    let preserved_object_literal_implicit_any_spans: Vec<_> = args
                        .iter()
                        .copied()
                        .filter(|&arg_idx| {
                            self.ctx.arena.get(arg_idx).is_some_and(|node| {
                                node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            })
                        })
                        .flat_map(|arg_idx| {
                            self.object_literal_noncontextual_function_param_spans(arg_idx)
                        })
                        .collect();
                    let initial_arg_types = self.collect_call_argument_types_with_context(
                        args,
                        |i, _arg_count| {
                            if i < single_pass_contextual_types.len() {
                                single_pass_contextual_types[i]
                            } else {
                                base_contextual_param_types.get(i).copied().flatten()
                            }
                        },
                        check_excess_properties,
                        None, // No skipping needed for single-pass
                        callable_ctx,
                    );

                    let needs_refresh = contextual_type.is_some()
                        && args.iter().enumerate().any(|(i, &arg)| {
                            self.argument_needs_refresh_for_contextual_call(
                                arg,
                                base_contextual_param_types.get(i).copied().flatten(),
                            )
                        });
                    if !needs_refresh {
                        initial_arg_types
                    } else {
                        let return_context_substitution = self
                            .compute_return_context_substitution_from_shape(
                                &shape,
                                contextual_type,
                            );
                        if !return_context_substitution.is_empty() {
                            if let Some(snap) = &initial_arg_snap {
                                self.ctx.rollback_diagnostics_filtered(snap, |diag| {
                                    Self::should_preserve_speculative_call_diagnostic(diag)
                                        || matches!(
                                            diag.code,
                                            diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                                                | diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                                                | diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                                                | diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
                                        ) && preserved_object_literal_implicit_any_spans.iter().any(
                                            |(start, end)| diag.start >= *start && diag.start < *end,
                                        ) || matches!(
                                            diag.code,
                                            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                                                | diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID
                                        )
                                });
                            }
                            if let Some(ts2454_snap) = &initial_ts2454_snap {
                                self.ctx.restore_ts2454_state(ts2454_snap);
                            }
                            self.clear_contextual_resolution_cache();
                            for (i, &arg_idx) in args.iter().enumerate() {
                                if self.argument_needs_refresh_for_contextual_call(
                                    arg_idx,
                                    base_contextual_param_types.get(i).copied().flatten(),
                                ) {
                                    self.invalidate_expression_for_contextual_retry(arg_idx);
                                }
                            }
                            let refreshed_contextual_types: Vec<Option<TypeId>> = (0..args.len())
                                .map(|i| {
                                    let param =
                                        shape.params.get(i).map(|p| (p.type_id, p.rest)).or_else(
                                            || {
                                                let last = shape.params.last()?;
                                                last.rest.then_some((last.type_id, true))
                                            },
                                        )?;
                                    let instantiated =
                                        crate::query_boundaries::common::instantiate_type(
                                            self.ctx.types,
                                            param.0,
                                            &return_context_substitution,
                                        );
                                    let param_type = if param.1 {
                                        self.rest_argument_element_type_with_env(instantiated)
                                    } else {
                                        instantiated
                                    };
                                    Some(self.normalize_contextual_call_param_type(param_type))
                                })
                                .collect();
                            self.collect_call_argument_types_with_context(
                                args,
                                |i, _arg_count| {
                                    refreshed_contextual_types
                                        .get(i)
                                        .copied()
                                        .flatten()
                                        .or_else(|| {
                                            base_contextual_param_types.get(i).copied().flatten()
                                        })
                                },
                                check_excess_properties,
                                None,
                                callable_ctx,
                            )
                        } else if let Some(instantiated_params) = self
                            .resolve_call_with_checker_adapter(
                                callee_type_for_context,
                                &initial_arg_types,
                                force_bivariant_callbacks,
                                contextual_type,
                                actual_this_type,
                            )
                            .2
                        {
                            if let Some(snap) = &initial_arg_snap {
                                self.ctx.rollback_diagnostics_filtered(snap, |diag| {
                                    Self::should_preserve_speculative_call_diagnostic(diag)
                                        || matches!(
                                            diag.code,
                                            diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                                                | diagnostic_codes::REST_PARAMETER_IMPLICITLY_HAS_AN_ANY_TYPE
                                                | diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
                                                | diagnostic_codes::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN
                                        ) && preserved_object_literal_implicit_any_spans.iter().any(
                                            |(start, end)| diag.start >= *start && diag.start < *end,
                                        ) || matches!(
                                            diag.code,
                                            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                                                | diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID
                                        )
                                });
                            }
                            if let Some(ts2454_snap) = &initial_ts2454_snap {
                                self.ctx.restore_ts2454_state(ts2454_snap);
                            }
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
                                .contextual_param_types_from_instantiated_params(
                                    &instantiated_params,
                                    args.len(),
                                )
                                .into_iter()
                                .map(|param_type| {
                                    param_type.map(|param_type| {
                                        self.normalize_contextual_call_param_type(param_type)
                                    })
                                })
                                .collect::<Vec<_>>();
                            let refreshed_arg_types = self
                                .collect_call_argument_types_with_context(
                                    args,
                                    |i, _arg_count| {
                                        refreshed_contextual_types
                                            .get(i)
                                            .copied()
                                            .flatten()
                                            .or_else(|| {
                                                base_contextual_param_types
                                                    .get(i)
                                                    .copied()
                                                    .flatten()
                                            })
                                    },
                                    check_excess_properties,
                                    None,
                                    callable_ctx,
                                );
                            self.refine_generic_function_args_against_instantiated_params(
                                refreshed_arg_types,
                                &instantiated_params,
                            )
                        } else {
                            initial_arg_types
                        }
                    }
                }
            } else {
                // Shouldn't happen for generic call detection, but keep single-pass fallback.
                let single_pass_contextual_types: Vec<Option<TypeId>> = (0..args.len())
                    .map(|i| {
                        let param_type = base_contextual_param_types.get(i).copied().flatten()?;
                        Some(self.normalize_contextual_call_param_type(param_type))
                    })
                    .collect();
                self.collect_call_argument_types_with_context(
                    args,
                    |i, _arg_count| {
                        if i < single_pass_contextual_types.len() {
                            single_pass_contextual_types[i]
                        } else {
                            base_contextual_param_types.get(i).copied().flatten()
                        }
                    },
                    check_excess_properties,
                    None, // No skipping needed for single-pass
                    callable_ctx,
                )
            }
        } else {
            // === Single-pass: Standard argument collection ===
            // Non-generic calls or calls with explicit type arguments use the standard flow.
            let single_pass_contextual_types: Vec<Option<TypeId>> = (0..args.len())
                .map(|i| {
                    let param_type = base_contextual_param_types.get(i).copied().flatten()?;
                    Some(self.normalize_contextual_call_param_type(param_type))
                })
                .collect();
            non_generic_contextual_types = Some(single_pass_contextual_types.clone());
            self.collect_call_argument_types_with_context(
                args,
                |i, _arg_count| {
                    if i < single_pass_contextual_types.len() {
                        single_pass_contextual_types[i]
                    } else {
                        base_contextual_param_types.get(i).copied().flatten()
                    }
                },
                check_excess_properties,
                None, // No skipping needed for single-pass
                callable_ctx,
            )
        };
        self.ctx.preserve_literal_types = prev_preserve_literals;
        self.ctx.generic_excess_skip = prev_generic_excess_skip;
        if pushed_this_type_from_shape {
            self.ctx.this_type_stack.pop();
        }
        self.ensure_relation_input_ready(callee_type_for_resolution);

        // Resolve applications/lazy refs to callable forms before solver dispatch.
        let callee_type_for_call = self.evaluate_application_type(callee_type_for_resolution);
        let callee_type_for_call = self.resolve_lazy_type(callee_type_for_call);

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
            self.sanitize_generic_inference_arg_types(args, &arg_types)
        } else {
            (arg_types.clone(), false)
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
                    result = CallResult::Success(return_type);
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
                    CallResult::Success(ret) => {
                        let contextual_return = self.evaluate_contextual_type(ctx_type);
                        !self.is_assignable_to_with_env(*ret, contextual_return)
                    }
                    _ => true,
                }
            } else {
                false
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
            // Re-push ThisType for the retry so object literal methods see the right `this`.
            let retry_pushed_this = if let Some(tt) = shape_this_type {
                self.ctx.this_type_stack.push(tt);
                true
            } else {
                false
            };
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
            if retry_pushed_this {
                self.ctx.this_type_stack.pop();
            }

            let (retry_generic_arg_types, retry_sanitized) =
                self.sanitize_generic_inference_arg_types(args, &arg_types);
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

        // Store instantiated type predicate from generic call resolution
        // so flow narrowing can use the correct (inferred) predicate type.
        if let Some(predicate) = instantiated_predicate {
            self.ctx.call_type_predicates.insert(idx.0, predicate);
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
                self.ctx
                    .call_type_predicates
                    .insert(idx.0, (extracted.predicate, extracted.params));
            }
        }

        let (mut result, allow_contextual_mismatch_deferral) = self.finalize_generic_call_result(
            callee_type_for_call,
            generic_instantiated_params.as_ref(),
            args,
            &arg_types,
            result,
            sanitized_generic_inference,
            needs_real_type_recheck,
            shape_this_type,
        );
        if let CallResult::ArgumentTypeMismatch {
            actual,
            expected,
            fallback_return,
            ..
        } = result
            && fallback_return != TypeId::ERROR
            && self.should_defer_contextual_argument_mismatch(actual, expected)
        {
            result = CallResult::Success(fallback_return);
        }

        let call_context = CallResultContext {
            callee_expr: call.expression,
            call_idx: idx,
            args,
            arg_types: &arg_types,
            callee_type: callee_type_for_call,
            is_super_call,
            is_optional_chain: nullish_cause.is_some(),
            allow_contextual_mismatch_deferral,
        };
        self.handle_call_result(result, call_context)
    }
}

// Identifier resolution is in `identifier.rs`.
// Tagged template expression handling is in `tagged_template.rs`.
// TDZ checking, value declaration resolution, and other helpers are in
// `call_helpers.rs`.
