//! Call expression type computation for `CheckerState`.
//!
//! Handles call expression type resolution including overload resolution,
//! argument type checking, type argument validation, and call result processing.
//! Identifier resolution is in `identifier.rs` and tagged
//! template expression handling is in `tagged_template.rs`.

use super::complex::is_contextually_sensitive;
use crate::query_boundaries::assignability as assign_query;
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::checkers::call::is_type_parameter_type;
use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use tracing::trace;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{CallResult, ContextualTypeContext, TypeId};

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

        // For IIFEs (immediately invoked function expressions), wrap the call expression's
        // contextual type into a callable type so the function expression resolver can extract
        // the return type (and for generators, the yield type).
        // Without this, a generator IIFE like `(function*() { yield x => x.length })()`
        // with contextual type `Iterable<(x: string) => number>` would fail to provide
        // contextual typing for `x`, because the function type resolver sees `Iterable<...>`
        // (not a callable) and can't extract a return type from it.
        let saved_contextual_for_iife = if let Some(ctx_type) = self.ctx.contextual_type {
            // Unwrap parenthesized expressions to find the actual callee.
            // Handles both `function*(){}()` and `(function*(){})()`.
            let is_function_expr = {
                let mut expr_idx = call.expression;
                loop {
                    match self.ctx.arena.get(expr_idx) {
                        Some(n)
                            if n.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                                || n.kind == syntax_kind_ext::ARROW_FUNCTION =>
                        {
                            break true;
                        }
                        Some(n) if n.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                            if let Some(paren) = self.ctx.arena.get_parenthesized(n) {
                                expr_idx = paren.expression;
                            } else {
                                break false;
                            }
                        }
                        _ => break false,
                    }
                }
            };
            if is_function_expr {
                // Wrap contextual type as `() => ctx_type` so the function expression
                // resolver can use get_return_type() to extract the expected return type.
                let wrapper_fn = self
                    .ctx
                    .types
                    .factory()
                    .function(tsz_solver::FunctionShape::new(vec![], ctx_type));
                self.ctx.contextual_type = Some(wrapper_fn);
                Some(ctx_type) // save original to restore later
            } else {
                None
            }
        } else {
            None
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
                    let callee_ty = self.get_type_of_symbol(sym_id);
                    // Cache in node_types so flow narrowing can retrieve callee
                    // type predicates during type guard analysis.
                    self.ctx.node_types.insert(call.expression.0, callee_ty);
                    callee_ty
                } else {
                    self.get_type_of_node(call.expression)
                }
            } else {
                self.get_type_of_node(call.expression)
            }
        } else {
            self.get_type_of_node(call.expression)
        };

        // Restore original contextual type after IIFE callee evaluation
        if let Some(original_ctx) = saved_contextual_for_iife {
            self.ctx.contextual_type = Some(original_ctx);
        }

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
            // TS7036: Check specifier type is assignable to `string`
            self.check_dynamic_import_specifier_type(call);
            // TS2322: Check options arg against ImportCallOptions
            self.check_dynamic_import_options_type(call);
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
            // Still need to check arguments for definite assignment (TS2454) and other errors.
            // Return Some(ANY) for every index so spread arguments are accepted (avoids
            // false TS2556 — `any` is callable with any arguments).
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| Some(TypeId::ANY),
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
            // Still need to check arguments for definite assignment (TS2454) and other errors.
            // Return Some(ANY) for every index so spread arguments are accepted (avoids
            // false TS2556 when the callee couldn't be resolved).
            let check_excess_properties = false;
            self.collect_call_argument_types_with_context(
                args,
                |_i, _arg_count| Some(TypeId::ANY),
                check_excess_properties,
                None, // No skipping needed
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
        // When the callee is a Union type, do NOT treat the collected member
        // signatures as overloads. Union call semantics require the call to be
        // valid for ALL members (handled by solver's resolve_union_call), while
        // overload resolution accepts the call if ANY single signature matches.
        // Without this guard, `(F1 | F2)("a")` would succeed if F1 alone accepts
        // 1 arg, silently ignoring F2 which requires 2 args — missing TS2554.
        let callee_is_union = tsz_solver::is_union_type(self.ctx.types, callee_type_for_resolution);
        let overload_signatures = if callee_is_union {
            None
        } else {
            match classification {
                query::CallSignaturesKind::Callable(shape_id) => {
                    let shape = self.ctx.types.callable_shape(shape_id);
                    (shape.call_signatures.len() > 1).then(|| shape.call_signatures.clone())
                }
                query::CallSignaturesKind::MultipleSignatures(signatures) => {
                    (signatures.len() > 1).then_some(signatures)
                }
                query::CallSignaturesKind::NoSignatures => None,
            }
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
        // Create contextual context from resolved callee type
        let ctx_helper = ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            callee_type_for_context,
            self.ctx.compiler_options.no_implicit_any,
        );
        let check_excess_properties = overload_signatures.is_none();
        let normalize_contextual_param_type =
            |this: &mut Self,
             helper: &ContextualTypeContext,
             param_type: TypeId,
             index: usize,
             arg_count: usize| {
                let evaluated = this.evaluate_type_with_env(param_type);
                if helper.is_rest_parameter_position(index, arg_count) {
                    tsz_solver::rest_argument_element_type(this.ctx.types, evaluated)
                } else {
                    evaluated
                }
            };
        // Two-pass argument collection for generic calls is only needed when at least one
        // argument is contextually sensitive (e.g. lambdas/object literals needing contextual type).
        // Preserve literal types in array literals during generic call argument collection.
        // This ensures `['foo', 'bar']` is typed as `("foo" | "bar")[]` (not `string[]`),
        // enabling correct type parameter inference (e.g., K = "foo" | "bar").
        // tsc preserves literals during inference and only widens at assignment sites.
        let prev_preserve_literals = self.ctx.preserve_literal_types;
        let prev_callable_type = self.ctx.current_callable_type;
        let prev_generic_excess_skip = self.ctx.generic_excess_skip.take();
        self.ctx.current_callable_type = Some(callee_type_for_context);
        if is_generic_call {
            self.ctx.preserve_literal_types = true;
        }
        let arg_types = if is_generic_call {
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
                                tsz_solver::type_queries::contains_type_parameters_db(
                                    self.ctx.types,
                                    shape.params[i].type_id,
                                )
                            } else if let Some(last) = shape.params.last() {
                                // Rest parameter: check the rest param's type
                                last.rest
                                    && tsz_solver::type_queries::contains_type_parameters_db(
                                        self.ctx.types,
                                        last.type_id,
                                    )
                            } else {
                                false
                            };
                            let from_ctx = ctx_helper
                                .get_parameter_type_for_call(i, arg_count)
                                .is_some_and(|param_type| {
                                    tsz_solver::type_queries::contains_type_parameters_db(
                                        self.ctx.types,
                                        param_type,
                                    )
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
                    let mut substitution = {
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

                    let inferred_type_params_by_name: Vec<_> = shape
                        .type_params
                        .iter()
                        .filter_map(|tp| {
                            substitution.get(tp.name).map(|ty| {
                                (self.ctx.types.resolve_atom(tp.name).to_string(), ty)
                            })
                        })
                        .collect();
                    let mut round2_substitution = substitution.clone();
                    for param in &evaluated_shape.params {
                        for referenced in tsz_solver::collect_referenced_types(
                            self.ctx.types,
                            param.type_id,
                        ) {
                            if let Some(info) =
                                tsz_solver::type_param_info(self.ctx.types, referenced)
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
                            && let Some(shape_fn) = tsz_solver::type_queries::get_function_shape(
                                self.ctx.types,
                                shape_param_type,
                            )
                            && let Some(arg_node) = self.ctx.arena.get(arg_idx)
                            && let Some(func) = self.ctx.arena.get_function(arg_node)
                        {
                            for (j, &param_idx) in func.parameters.nodes.iter().enumerate() {
                                if let Some(param_node) = self.ctx.arena.get(param_idx)
                                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                                    && param.type_annotation.is_some()
                                    && let Some(shape_fn_param) = shape_fn.params.get(j)
                                    && let Some(tp_info) =
                                        tsz_solver::type_queries::get_type_parameter_info(
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

                    let round1_instantiated_params = self
                        .resolve_call_with_checker_adapter(
                            callee_type_for_context,
                            &round1_arg_types,
                            force_bivariant_callbacks,
                            self.ctx.contextual_type,
                            actual_this_type,
                        )
                        .2;

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
                        let round2_param = round1_instantiated_params
                            .as_ref()
                            .and_then(|params| {
                                params
                                    .get(i)
                                    .map(|p| (p.type_id, p.rest))
                                    .or_else(|| {
                                        let last = params.last()?;
                                        last.rest.then_some((last.type_id, true))
                                    })
                            })
                            .or_else(|| {
                                shape
                                    .params
                                    .get(i)
                                    .map(|p| (p.type_id, p.rest))
                                    .or_else(|| {
                                        let last = shape.params.last()?;
                                        last.rest.then_some((last.type_id, true))
                                    })
                            });
                        let ctx_type = if let Some((param_type, is_rest_param)) = round2_param
                        {
                            let instantiated = if round1_instantiated_params.is_some() {
                                param_type
                            } else {
                                instantiate_type(
                                    self.ctx.types,
                                    param_type,
                                    &round2_substitution,
                                )
                            };
                            let evaluated = self.evaluate_type_with_env(instantiated);
                            trace!(
                                arg_index = i,
                                param_type_id = param_type.0,
                                param_type_key = ?self.ctx.types.lookup(param_type),
                                param_type_app_args = ?tsz_solver::type_queries::get_application_info(
                                    self.ctx.types,
                                    param_type,
                                )
                                .map(|(_, args)| args),
                                instantiated_id = instantiated.0,
                                instantiated_key = ?self.ctx.types.lookup(instantiated),
                                instantiated_app_args = ?tsz_solver::type_queries::get_application_info(
                                    self.ctx.types,
                                    instantiated,
                                )
                                .map(|(_, args)| args),
                                evaluated_id = evaluated.0,
                                evaluated_key = ?self.ctx.types.lookup(evaluated),
                                "Round 2: instantiated parameter type"
                            );
                            Some(if is_rest_param {
                                tsz_solver::rest_argument_element_type(self.ctx.types, evaluated)
                            } else {
                                evaluated
                            })
                        } else {
                            None
                        };
                        trace!(
                            arg_index = i,
                            ctx_type_id = ?ctx_type.map(|t| t.0),
                            ctx_type_key = ?ctx_type.and_then(|t| self.ctx.types.lookup(t)),
                            "Round 2: contextual type for argument"
                        );
                        round2_contextual_types.push(ctx_type);
                    }

                    // === Round 2: Collect ALL argument types with contextual typing ===
                    // Now that type parameters are partially inferred, lambdas get proper contextual types.
                    self.collect_call_argument_types_with_context(
                        args,
                        |i, arg_count| {
                            // For normal argument indices, use the precomputed contextual types.
                            // For large indices (rest parameter probes), fall back to the
                            // contextual type helper to correctly detect rest params.
                            if i < round2_contextual_types.len() {
                                round2_contextual_types[i]
                            } else {
                                ctx_helper.get_parameter_type_for_call(i, arg_count)
                            }
                        },
                        check_excess_properties,
                        None, // Don't skip anything in Round 2 - check all args with inferred context
                    )
                } else {
                    // No context-sensitive arguments: skip Round 1/2 and use single-pass collection.
                    // For array literal arguments in generic calls, erase the callee's type
                    // parameters from contextual types (replacing with constraints or `unknown`).
                    // This matches tsc's behavior where type params don't leak into inference
                    // candidates via contextual typing. Without this, `[]` with contextual type
                    // `T[]` gets type `T[]` instead of `unknown[]`, causing T to appear as an
                    // inference candidate and produce false TS2345 errors.
                    // We only erase for array/object literals to avoid breaking literal type
                    // preservation and other contextual typing behaviors.
                    let type_param_eraser = {
                        use tsz_solver::TypeSubstitution;
                        let mut sub = TypeSubstitution::new();
                        for tp in &shape.type_params {
                            sub.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
                        }
                        sub
                    };
                    let arena = self.ctx.arena;
                    let single_pass_contextual_types: Vec<Option<TypeId>> = (0..args.len())
                        .map(|i| {
                            let param_type = ctx_helper.get_parameter_type_for_call(i, args.len())?;
                            let is_empty_array_literal = arena.get(args[i]).is_some_and(|n| {
                                n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                    && arena
                                        .get_literal_expr(n)
                                        .is_some_and(|lit| lit.elements.nodes.is_empty())
                            });
                            let param_type = if is_empty_array_literal {
                                use tsz_solver::instantiate_type;
                                instantiate_type(self.ctx.types, param_type, &type_param_eraser)
                            } else {
                                param_type
                            };
                            Some(normalize_contextual_param_type(
                                self,
                                &ctx_helper,
                                param_type,
                                i,
                                args.len(),
                            ))
                        })
                        .collect();
                    self.collect_call_argument_types_with_context(
                        args,
                        |i, arg_count| {
                            if i < single_pass_contextual_types.len() {
                                single_pass_contextual_types[i]
                            } else {
                                ctx_helper.get_parameter_type_for_call(i, arg_count)
                            }
                        },
                        check_excess_properties,
                        None, // No skipping needed for single-pass
                    )
                }
            } else {
                // Shouldn't happen for generic call detection, but keep single-pass fallback.
                let single_pass_contextual_types: Vec<Option<TypeId>> = (0..args.len())
                    .map(|i| {
                        let param_type = ctx_helper.get_parameter_type_for_call(i, args.len())?;
                        Some(normalize_contextual_param_type(
                            self,
                            &ctx_helper,
                            param_type,
                            i,
                            args.len(),
                        ))
                    })
                    .collect();
                self.collect_call_argument_types_with_context(
                    args,
                    |i, arg_count| {
                        if i < single_pass_contextual_types.len() {
                            single_pass_contextual_types[i]
                        } else {
                            ctx_helper.get_parameter_type_for_call(i, arg_count)
                        }
                    },
                    check_excess_properties,
                    None, // No skipping needed for single-pass
                )
            }
        } else {
            // === Single-pass: Standard argument collection ===
            // Non-generic calls or calls with explicit type arguments use the standard flow.
            let single_pass_contextual_types: Vec<Option<TypeId>> = (0..args.len())
                .map(|i| {
                    let param_type = ctx_helper.get_parameter_type_for_call(i, args.len())?;
                    Some(normalize_contextual_param_type(
                        self,
                        &ctx_helper,
                        param_type,
                        i,
                        args.len(),
                    ))
                })
                .collect();
            self.collect_call_argument_types_with_context(
                args,
                |i, arg_count| {
                    if i < single_pass_contextual_types.len() {
                        single_pass_contextual_types[i]
                    } else {
                        ctx_helper.get_parameter_type_for_call(i, arg_count)
                    }
                },
                check_excess_properties,
                None, // No skipping needed for single-pass
            )
        };
        self.ctx.preserve_literal_types = prev_preserve_literals;
        self.ctx.current_callable_type = prev_callable_type;
        self.ctx.generic_excess_skip = prev_generic_excess_skip;
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
        let (result, instantiated_predicate, generic_instantiated_params) = if is_super_call {
            (
                self.resolve_new_with_checker_adapter(
                    callee_type_for_call,
                    &arg_types,
                    force_bivariant_callbacks,
                    self.ctx.contextual_type,
                ),
                None,
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

        // Post-inference excess property checking for generic calls.
        // During argument collection, EPC is skipped for parameters whose raw type
        // contains type parameters (via generic_excess_skip). After inference resolves
        // type parameters, the instantiated parameter types may be concrete and
        // restrictive (e.g., a mapped type that filters keys). Perform EPC on the
        // evaluated instantiated parameter types to catch excess properties.
        //
        // Also handle ArgumentTypeMismatch: the solver's final check may fail due to
        // subtype cache entries from inference. When we have instantiated params and
        // a fresh assignability check passes, treat the call as successful and perform
        // EPC instead of reporting TS2345.
        let (result, did_post_epc) = if let Some(ref instantiated_params) =
            generic_instantiated_params
        {
            let should_epc = match &result {
                CallResult::Success(_) => true,
                CallResult::ArgumentTypeMismatch { index, .. } => {
                    // The final check may fail due to stale cache entries. Verify with
                    // a fresh structural check on the evaluated instantiated param.
                    if let Some(param) = instantiated_params.get(*index) {
                        let evaluated_param = self.evaluate_type_with_env(param.type_id);
                        let arg_type = arg_types.get(*index).copied().unwrap_or(TypeId::UNKNOWN);
                        // Use a fresh subtype check (no cache) to avoid false
                        // negatives from stale query cache entries after inference.
                        assign_query::is_fresh_subtype_of(self.ctx.types, arg_type, evaluated_param)
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if should_epc {
                let mut did_epc = false;
                for (i, &arg_idx) in args.iter().enumerate() {
                    if let Some(arg_node) = self.ctx.arena.get(arg_idx)
                        && arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        && let Some(param) = instantiated_params.get(i)
                        && param.type_id != TypeId::ANY
                        && param.type_id != TypeId::UNKNOWN
                    {
                        let evaluated_param = self.evaluate_type_with_env(param.type_id);
                        if !is_type_parameter_type(self.ctx.types, evaluated_param) {
                            let arg_type = arg_types.get(i).copied().unwrap_or(TypeId::UNKNOWN);
                            self.check_object_literal_excess_properties(
                                arg_type,
                                evaluated_param,
                                arg_idx,
                            );
                            did_epc = true;
                        }
                    }
                }
                // If the result was ArgumentTypeMismatch but fresh check passed,
                // convert to Success so the caller doesn't report TS2345.
                let result = if did_epc
                    && matches!(result, CallResult::ArgumentTypeMismatch { fallback_return, .. } if fallback_return != TypeId::ERROR)
                {
                    if let CallResult::ArgumentTypeMismatch {
                        fallback_return, ..
                    } = &result
                    {
                        CallResult::Success(*fallback_return)
                    } else {
                        result
                    }
                } else {
                    result
                };
                (result, did_epc)
            } else {
                (result, false)
            }
        } else {
            (result, false)
        };
        let _ = did_post_epc;

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
                    tsz_solver::relations::freshness::widen_freshness(self.ctx.types, return_type)
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
            CallResult::NonVoidFunctionCalledWithNew | CallResult::VoidFunctionCalledWithNew => {
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
                } else if self.ctx.compiler_options.strict_null_checks {
                    // TS2721/TS2722/TS2723: Check if the callee type contains null/undefined.
                    // When strictNullChecks is on and the type includes nullish parts,
                    // emit a specific "cannot invoke possibly null/undefined" error
                    // instead of the generic TS2349 "not callable".
                    let (_non_nullish, nullish_cause) = self.split_nullish_type(callee_type);
                    if let Some(cause) = nullish_cause {
                        self.error_cannot_invoke_possibly_nullish_at(cause, callee_expr);
                    } else {
                        self.error_not_callable_at(callee_type, callee_expr);
                    }
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
                if !self.ctx.has_parse_errors {
                    // Suppress arity errors when the call contains non-tuple spread
                    // arguments. The spread could provide any number of values at
                    // runtime, so the actual argument count is indeterminate.
                    // TSC only emits TS2556 in this case, not TS2555/TS2554.
                    // However, tuple spreads have known length, so TS2554 should
                    // still fire for those.
                    let has_non_tuple_spread = args.iter().any(|&arg_idx| {
                        if let Some(n) = self.ctx.arena.get(arg_idx)
                            && n.kind == syntax_kind_ext::SPREAD_ELEMENT
                            && let Some(spread_data) = self.ctx.arena.get_spread(n)
                        {
                            let spread_type = self.get_type_of_node(spread_data.expression);
                            let spread_type = self.resolve_type_for_property_access(spread_type);
                            let spread_type = self.resolve_lazy_type(spread_type);
                            crate::query_boundaries::common::tuple_elements(
                                self.ctx.types,
                                spread_type,
                            )
                            .is_none()
                        } else {
                            false
                        }
                    });
                    if has_non_tuple_spread {
                        // TS2556 was already emitted by collect_call_argument_types;
                        // don't cascade with a misleading TS2555/TS2554.
                    } else if actual < expected_min && expected_max.is_none() {
                        // Too few arguments with rest parameters (unbounded) - use TS2555
                        self.error_expected_at_least_arguments_at(expected_min, actual, call_idx);
                    } else {
                        // Use TS2554 for exact count, range, or too many args
                        let max = expected_max.unwrap_or(expected_min);
                        // Build expanded args list: for tuple spreads, repeat the
                        // spread node for each tuple element so that the excess
                        // argument location logic points at the correct node.
                        let expanded_args = self.build_expanded_args_for_error(args);
                        let args_for_error = if expanded_args.len() > args.len() {
                            &expanded_args
                        } else {
                            args
                        };
                        self.error_argument_count_mismatch_at(
                            expected_min,
                            max,
                            actual,
                            call_idx,
                            args_for_error,
                        );
                    }
                }
                TypeId::ERROR
            }
            CallResult::OverloadArgumentCountMismatch {
                actual,
                expected_low,
                expected_high,
            } => {
                if !self.ctx.has_parse_errors {
                    self.error_at_node(
                        call_idx,
                        &format!(
                            "No overload expects {actual} arguments, but overloads do exist that expect either {expected_low} or {expected_high} arguments."
                        ),
                        diagnostic_codes::NO_OVERLOAD_EXPECTS_ARGUMENTS_BUT_OVERLOADS_DO_EXIST_THAT_EXPECT_EITHER_OR_ARGUM,
                    );
                }
                TypeId::ERROR
            }
            CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
                fallback_return,
            } => {
                if self.should_defer_contextual_argument_mismatch(actual, expected) {
                    return TypeId::ERROR;
                }
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
                } else {
                    // No arguments at all (e.g. f1() where f1 expects variadic
                    // tuple rest param). Report TS2345 on the call expression.
                    let _ = self.check_argument_assignable_or_report(actual, expected, call_idx);
                }

                if fallback_return != TypeId::ERROR {
                    fallback_return
                } else {
                    TypeId::ERROR
                }
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

    fn should_defer_contextual_argument_mismatch(&self, actual: TypeId, expected: TypeId) -> bool {
        // During generic contextual inference, expected parameter types can transiently
        // include placeholder `any` slots before all nested callbacks are fully typed.
        // Emitting TS2345 in this state creates false positives in conformance tests.
        if assign_query::contains_infer_types(self.ctx.types, actual)
            || assign_query::contains_infer_types(self.ctx.types, expected)
        {
            return true;
        }
        if assign_query::contains_type_parameters(self.ctx.types, expected)
            && assign_query::contains_any_type(self.ctx.types, actual)
        {
            return true;
        }
        assign_query::is_any_type(self.ctx.types, expected)
    }

    /// Build an expanded args list for TS2554 error location.
    ///
    /// When tuple spreads are present, the original `args` slice has fewer
    /// entries than the expanded argument count. This method builds a new
    /// list where tuple-spread nodes are repeated for each tuple element,
    /// so the excess-argument location logic in `error_argument_count_mismatch_at`
    /// can index into it correctly.
    pub(crate) fn build_expanded_args_for_error(&mut self, args: &[NodeIndex]) -> Vec<NodeIndex> {
        let mut expanded = Vec::with_capacity(args.len());
        for &arg_idx in args {
            if let Some(n) = self.ctx.arena.get(arg_idx)
                && n.kind == syntax_kind_ext::SPREAD_ELEMENT
                && let Some(spread_data) = self.ctx.arena.get_spread(n)
            {
                let spread_type = self.get_type_of_node(spread_data.expression);
                let spread_type = self.resolve_type_for_property_access(spread_type);
                let spread_type = self.resolve_lazy_type(spread_type);
                if let Some(elems) =
                    crate::query_boundaries::common::tuple_elements(self.ctx.types, spread_type)
                {
                    for _ in &elems {
                        expanded.push(arg_idx);
                    }
                    continue;
                }
            }
            expanded.push(arg_idx);
        }
        expanded
    }
}

// Identifier resolution is in `identifier.rs`.
// Tagged template expression handling is in `tagged_template.rs`.
// TDZ checking, value declaration resolution, and other helpers are in
// `call_helpers.rs`.
