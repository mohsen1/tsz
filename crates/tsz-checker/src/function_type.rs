//! Function Type Resolution Module
//!
//! This module contains function type resolution methods for `CheckerState`
//! as part of the Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Function/method/arrow function type resolution
//! - Parameter type inference and contextual typing
//! - Return type inference and validation
//! - Property access type resolution
//! - Async function Promise return type validation
//!
//! This module extends `CheckerState` with utilities for function type
//! resolution, providing cleaner separation of function typing logic.

use crate::diagnostics::format_message;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{ContextualTypeContext, TypeId, TypeParamInfo};

// =============================================================================
// Function Type Resolution
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Function Type Resolution
    // =========================================================================

    /// Get type of function declaration/expression/arrow.
    pub(crate) fn get_type_of_function(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_solver::{FunctionShape, ParamInfo};

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };
        // Determine if this is a function expression or arrow function (a closure)
        let is_closure = matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION
        );

        // Rule #42: Increment closure depth when entering a function expression or arrow function
        // This causes mutable variables (let/var) to lose narrowing inside the closure
        if is_closure {
            self.ctx.inside_closure_depth += 1;
        }

        // Helper macro to decrement closure depth before returning
        // This ensures we properly track closure depth even on early returns
        macro_rules! return_with_cleanup {
            ($expr:expr) => {{
                if is_closure {
                    self.ctx.inside_closure_depth -= 1;
                }
                $expr
            }};
        }

        let (type_parameters, parameters, type_annotation, body, name_node, name_for_error) =
            if let Some(func) = self.ctx.arena.get_function(node) {
                let name_node = if func.name.is_none() {
                    None
                } else {
                    Some(func.name)
                };
                let name_for_error = if func.name.is_none() {
                    None
                } else {
                    self.get_function_name_from_node(idx)
                };
                (
                    &func.type_parameters,
                    &func.parameters,
                    func.type_annotation,
                    func.body,
                    name_node,
                    name_for_error,
                )
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                (
                    &method.type_parameters,
                    &method.parameters,
                    method.type_annotation,
                    method.body,
                    Some(method.name),
                    self.property_name_for_error(method.name),
                )
            } else if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                // Support GET_ACCESSOR and SET_ACCESSOR nodes (object literal and class accessors)
                (
                    &accessor.type_parameters,
                    &accessor.parameters,
                    accessor.type_annotation,
                    accessor.body,
                    Some(accessor.name),
                    self.property_name_for_error(accessor.name),
                )
            } else {
                return return_with_cleanup!(TypeId::ERROR); // Missing function/method/accessor data - propagate error
            };

        let (function_is_async, function_is_generator) =
            if let Some(func) = self.ctx.arena.get_function(node) {
                (func.is_async, func.asterisk_token)
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                (
                    self.has_async_modifier(&method.modifiers),
                    method.asterisk_token,
                )
            } else {
                (false, false)
            };

        // Function declarations don't report implicit any for parameters (handled by check_statement)
        let is_function_declaration = node.kind == syntax_kind_ext::FUNCTION_DECLARATION;
        let is_method_or_constructor = matches!(
            node.kind,
            syntax_kind_ext::METHOD_DECLARATION | syntax_kind_ext::CONSTRUCTOR
        );
        let is_arrow_function = node.kind == syntax_kind_ext::ARROW_FUNCTION;

        // Check for duplicate parameter names in function expressions and arrow functions (TS2300)
        // Note: Methods and constructors are checked in check_method_declaration and check_constructor_declaration
        // Function declarations are checked in check_statement
        if !is_function_declaration && !is_method_or_constructor {
            self.check_duplicate_parameters(parameters);
            // Check for required parameters following optional parameters (TS1016)
            self.check_parameter_ordering(parameters);
            // Check that rest parameters have array types (TS2370)
            self.check_rest_parameter_types(&parameters.nodes);
            self.check_strict_mode_reserved_parameter_names(
                &parameters.nodes,
                idx,
                self.ctx.enclosing_class.is_some(),
            );
        }

        // For nested functions/methods, push enclosing type parameters first so that
        // type parameter constraints, parameter types, and return types can reference
        // outer generic scopes.  This is needed because get_type_of_function can be
        // called lazily (via get_type_of_symbol) outside the enclosing function's
        // check_function_declaration scope.
        let enclosing_type_param_updates = self.push_enclosing_type_parameters(idx);

        let (mut type_params, type_param_updates) = self.push_type_parameters(type_parameters);

        // Check for unused type parameters in function expressions and arrow functions (TS6133)
        // Function declarations, methods, classes, interfaces, and type aliases are checked
        // in the checking path (check_statement, check_method_declaration, etc.)
        if !is_function_declaration && !is_method_or_constructor {
            self.check_unused_type_params(type_parameters, idx);
        }

        // Collect parameter info using solver's ParamInfo struct
        let mut params = Vec::new();
        let mut param_types: Vec<Option<TypeId>> = Vec::new();
        let mut this_type = None;
        let this_atom = self.ctx.types.intern_string("this");

        // Setup contextual typing context if available
        // IMPORTANT: Evaluate compound types before creating context to resolve:
        // - Application types: fix TS2571 false positives (see: docs/TS2571_INVESTIGATION.md)
        // - Lazy types (type aliases): fix TS7006 false positives for contextual parameter typing
        // - IndexedAccess/KeyOf types: fix TS7006 when parameter type is e.g. Type["a"]
        let mut contextual_signature_type_params = None;
        let ctx_helper = if let Some(ctx_type) = self.ctx.contextual_type {
            use tsz_solver::type_queries::{
                EvaluationNeeded, classify_for_evaluation, get_lazy_def_id, get_type_application,
            };

            // Evaluate the contextual type to resolve type aliases and generic applications
            let evaluated_type = if get_type_application(self.ctx.types, ctx_type).is_some() {
                self.evaluate_application_type(ctx_type)
            } else if get_lazy_def_id(self.ctx.types, ctx_type).is_some() {
                self.judge_evaluate(ctx_type)
            } else if matches!(
                classify_for_evaluation(self.ctx.types, ctx_type),
                EvaluationNeeded::IndexAccess { .. } | EvaluationNeeded::KeyOf(..)
            ) {
                // Evaluate IndexedAccess (e.g., Type["a"]) and KeyOf types so they
                // resolve to concrete function types usable for parameter typing.
                self.judge_evaluate(ctx_type)
            } else {
                // For unions/intersections, evaluate to resolve lazy members
                // so contextual parameter typing can extract callable signatures.
                self.evaluate_contextual_type(ctx_type)
            };

            contextual_signature_type_params =
                self.contextual_type_params_from_expected(evaluated_type);

            Some(ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                evaluated_type,
                self.ctx.compiler_options.no_implicit_any,
            ))
        } else {
            None
        };

        // Contextually typed closures can acquire generic signatures even without
        // explicit `<T>` syntax. This is required for parity with TypeScript in
        // cases like:
        //   const f: <T>(x: T) => void = x => {};
        if is_closure
            && type_params.is_empty()
            && let Some(contextual_type_params) = contextual_signature_type_params
        {
            type_params = contextual_type_params;
        }

        // For arrow functions, capture the outer `this` type to preserve lexical `this`
        // Arrow functions should inherit `this` from their enclosing scope
        let outer_this_type = if is_arrow_function {
            self.current_this_type()
        } else {
            None
        };

        // Extract JSDoc for the function to check for @param/@returns annotations.
        // This suppresses false TS7006/TS7010/TS7011 in JS files with JSDoc type annotations.
        let func_jsdoc = self.get_jsdoc_for_function(idx);
        let mut jsdoc_type_param_types: FxHashMap<String, TypeId> = FxHashMap::default();

        // In JS/checkJs, support minimal generic JSDoc function typing:
        //   @template T
        //   @returns {T}
        // This enables return assignability checks for expression-bodied arrows.
        let mut jsdoc_type_param_updates: Vec<(String, Option<TypeId>)> = Vec::new();
        if self.is_js_file()
            && type_params.is_empty()
            && let Some(ref jsdoc) = func_jsdoc
        {
            let template_names = Self::jsdoc_template_type_params(jsdoc);
            if !template_names.is_empty() {
                let mut jsdoc_type_params = Vec::with_capacity(template_names.len());
                let factory = self.ctx.types.factory();
                for name in template_names {
                    let atom = self.ctx.types.intern_string(&name);
                    let info = TypeParamInfo {
                        name: atom,
                        constraint: None,
                        default: None,
                        is_const: false,
                    };
                    let ty = factory.type_param(info.clone());
                    jsdoc_type_param_types.insert(name.clone(), ty);
                    jsdoc_type_params.push(info);
                    // Register in type_parameter_scope so inline JSDoc casts
                    // like `/** @type {T} */(expr)` can resolve `T`.
                    let previous = self.ctx.type_parameter_scope.insert(name.clone(), ty);
                    jsdoc_type_param_updates.push((name, previous));
                }
                type_params = jsdoc_type_params;
            }
        }
        let jsdoc_return_context = func_jsdoc
            .as_ref()
            .and_then(|j| Self::jsdoc_returns_type_name(j))
            .and_then(|name| jsdoc_type_param_types.get(&name).copied());

        let mut contextual_index = 0;
        for &param_idx in &parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                // Get parameter name
                let name = if let Some(name_node) = self.ctx.arena.get(param.name) {
                    if let Some(name_data) = self.ctx.arena.get_identifier(name_node) {
                        Some(self.ctx.types.intern_string(&name_data.escaped_text))
                    } else {
                        None
                    }
                } else {
                    None
                };

                let is_this_param = name == Some(this_atom);

                let is_js_file = self.ctx.file_name.ends_with(".js")
                    || self.ctx.file_name.ends_with(".jsx")
                    || self.ctx.file_name.ends_with(".mjs")
                    || self.ctx.file_name.ends_with(".cjs");
                let contextual_type = if let Some(ref helper) = ctx_helper {
                    helper.get_parameter_type(contextual_index)
                } else {
                    None
                };
                // TS7006: In TS files, contextual `unknown` is still a concrete contextual
                // type and should suppress implicit-any reporting for callback parameters.
                // Keep the old JS behavior where weak contextual `unknown` is treated as no context.
                // Rest parameters (`...x`) are always contextually typed when a contextual
                // type helper exists — even if the contextual function has fewer parameters,
                // the rest param captures the "remaining" args (type `[]` for 0-param context).
                let has_contextual_type = contextual_type
                    .is_some_and(|t| t != TypeId::UNKNOWN || !is_js_file)
                    || (param.dot_dot_dot_token && ctx_helper.is_some());

                // Use type annotation if present, otherwise infer from context
                let type_id = if param.type_annotation.is_some() {
                    // Check parameter type for parameter properties in function types
                    self.check_type_for_parameter_properties(param.type_annotation);
                    // Check for undefined type names in parameter type
                    self.check_type_for_missing_names(param.type_annotation);
                    self.get_type_from_type_node(param.type_annotation)
                } else if is_this_param {
                    // For `this` parameter without type annotation:
                    // - Arrow functions: inherit outer `this` type to preserve lexical scoping
                    // - Regular functions: use ANY (will trigger TS2683 when used, not TS2571)
                    // - Contextual type: if provided, use it (for function types with explicit `this`)
                    if let Some(ref helper) = ctx_helper {
                        helper
                            .get_this_type()
                            .or(outer_this_type)
                            .unwrap_or(TypeId::ANY)
                    } else {
                        outer_this_type.unwrap_or(TypeId::ANY)
                    }
                } else {
                    // Infer from contextual type, default to ANY for implicit any parameters
                    // TypeScript uses `any` (with TS7006) when no contextual type is available.
                    if is_js_file {
                        // In checkJs mode, contextual `unknown` from weak callback types
                        // (e.g. `(...args: unknown[]) => T`) should not force parameters
                        // to become `unknown`; TypeScript treats these as effectively `any`.
                        contextual_type
                            .filter(|t| *t != TypeId::UNKNOWN)
                            .unwrap_or(TypeId::ANY)
                    } else {
                        contextual_type.unwrap_or(TypeId::ANY)
                    }
                };

                if is_this_param {
                    if this_type.is_none() {
                        this_type = Some(type_id);
                    }
                    param_types.push(None);
                    continue;
                }

                // Check all function parameters for implicit any (TS7006)
                // This includes function declarations, expressions, arrow functions, and methods
                //
                // For closures (function expressions and arrow functions), skip TS7006 during
                // the build_type_environment phase. During that phase, contextual types are not
                // yet available (they're set during check_variable_declaration). The closure
                // will be re-evaluated with contextual types during the checking phase.
                //
                // JSDoc @param {type} annotations also suppress TS7006 in JS files.
                // Also check for inline /** @type {T} */ annotations on the parameter itself.
                // A @type {function-type} on the function also suppresses TS7006 for all params.
                let has_jsdoc_param = if !has_contextual_type && param.type_annotation.is_none() {
                    let from_func_jsdoc = if let Some(ref jsdoc) = func_jsdoc {
                        let pname = self.parameter_name_for_error(param.name);
                        Self::jsdoc_has_param_type(jsdoc, &pname)
                            || Self::jsdoc_has_type_tag(jsdoc)
                            || self.ctx.arena.get(param.name).is_some_and(|n| {
                                n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                    || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            }) && Self::jsdoc_has_type_annotations(jsdoc)
                    } else {
                        false
                    };
                    from_func_jsdoc || self.param_has_inline_jsdoc_type(param_idx)
                } else {
                    false
                };
                // Skip TS7006 for:
                // 1. Closures during build_type_environment (no contextual type yet)
                // 2. SET_ACCESSOR nodes — their TS7006 is handled by the caller
                //    (type_computation.rs for object literals, ambient_signature_checks.rs
                //    for class members) with proper paired-getter detection.
                let is_setter = node.kind == syntax_kind_ext::SET_ACCESSOR;
                let skip_implicit_any = is_setter
                    || (is_closure && !self.ctx.is_checking_statements && !has_contextual_type);
                if !skip_implicit_any {
                    self.maybe_report_implicit_any_parameter(
                        param,
                        has_contextual_type || has_jsdoc_param,
                    );
                }

                // Check if optional or has initializer
                // In JS files, parameters without type annotations are implicitly optional
                let is_js_file = self.ctx.file_name.ends_with(".js")
                    || self.ctx.file_name.ends_with(".jsx")
                    || self.ctx.file_name.ends_with(".mjs")
                    || self.ctx.file_name.ends_with(".cjs");
                let optional = param.question_token
                    || param.initializer.is_some()
                    || (is_js_file && param.type_annotation.is_none());
                let rest = param.dot_dot_dot_token;

                // Under strictNullChecks, optional parameters (with `?`) get
                // `undefined` added to their type.  Parameters with a default
                // value but no `?` do NOT — the default guarantees a value.
                let effective_type = if param.question_token
                    && self.ctx.strict_null_checks()
                    && type_id != TypeId::ANY
                    && type_id != TypeId::ERROR
                    && type_id != TypeId::UNDEFINED
                {
                    self.ctx
                        .types
                        .factory()
                        .union(vec![type_id, TypeId::UNDEFINED])
                } else {
                    type_id
                };

                params.push(ParamInfo {
                    name,
                    type_id: effective_type,
                    optional,
                    rest,
                });
                param_types.push(Some(effective_type));
                contextual_index += 1;
            }
        }

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in regular functions
        self.check_parameter_properties(&parameters.nodes);

        // Get return type from annotation or infer
        let has_type_annotation = type_annotation.is_some();
        let (mut return_type, type_predicate) = if has_type_annotation {
            // Check return type for parameter properties in function types
            self.check_type_for_parameter_properties(type_annotation);
            // Check for undefined type names in return type
            self.check_type_for_missing_names(type_annotation);
            self.return_type_and_predicate(type_annotation, &params)
        } else {
            // Use UNKNOWN as default to enforce strict checking
            // This ensures return statements are checked even without annotation
            (TypeId::UNKNOWN, None)
        };

        // Save the annotated return type before evaluation. evaluate_application_type()
        // expands Application types (like Promise<string>) into concrete object shapes,
        // which is useful for body checking but destroys type identity needed by callers
        // (e.g., await unwrapping needs to see Promise<T> as an Application).
        let annotated_return_type = has_type_annotation.then_some(return_type);

        // Evaluate Application types in return type to get their structural form
        // This allows proper comparison of return expressions against type alias applications like Reducer<S, A>
        return_type = self.evaluate_application_type(return_type);

        // Check the function body (for type errors within the body)
        // Save/restore the arguments tracking flag for nested function handling
        let saved_uses_arguments = self.ctx.js_body_uses_arguments;
        self.ctx.js_body_uses_arguments = false;

        // Push this_type BEFORE parameter initializer checks so that default
        // values like `a = this.getNumber()` see the correct `this` type and
        // don't trigger false TS2683.
        let mut pushed_this_type_early = false;
        if let Some(tt) = this_type {
            self.ctx.this_type_stack.push(tt);
            pushed_this_type_early = true;
        }

        self.check_non_impl_parameter_initializers(&parameters.nodes, false, body.is_some());
        if body.is_some() {
            // Track that we're inside a nested function for abstract property access checks.
            // This must happen before infer_return_type_from_body which evaluates body expressions.
            self.ctx.function_depth += 1;
            self.cache_parameter_types(&parameters.nodes, Some(&param_types));
            self.record_destructured_parameter_binding_groups(&parameters.nodes, &param_types);

            // Assign contextual types to destructuring parameters (binding patterns)
            // This allows destructuring patterns in callbacks to infer element types from contextual types
            self.assign_contextual_types_to_destructuring_params(&parameters.nodes, &param_types);

            // Check that parameter default values are assignable to declared types (TS2322).
            // Only do this for closures (function expressions, arrow functions) since
            // function/method declarations are checked by statement_callback_bridge.rs.
            // Without this guard, function declarations get duplicate diagnostics.
            if is_closure {
                self.check_parameter_initializers(&parameters.nodes);
            }

            // Check async function requirements (needed before TS7010 check)
            let (is_async, is_generator, async_node_idx): (bool, bool, NodeIndex) =
                if let Some(func) = self.ctx.arena.get_function(node) {
                    (func.is_async, func.asterisk_token, func.name)
                } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    (
                        self.has_async_modifier(&method.modifiers),
                        method.asterisk_token,
                        method.name,
                    )
                } else {
                    (false, false, NodeIndex::NONE)
                };

            // this_type was already pushed early (before parameter initializer checks)

            // Push contextual yield type EARLY (before infer_return_type_from_body)
            // so yield expressions get contextual typing during inference.
            let early_yield_type = if is_generator && !has_type_annotation {
                ctx_helper.as_ref().and_then(|helper| {
                    let ret_type = helper.get_return_type()?;
                    let ret_ctx = ContextualTypeContext::with_expected(self.ctx.types, ret_type);
                    ret_ctx.get_generator_yield_type()
                })
            } else {
                None
            };
            if early_yield_type.is_some() {
                self.ctx.push_yield_type(early_yield_type);
            }

            let mut has_contextual_return = false;
            if !has_type_annotation {
                let return_context = jsdoc_return_context.or_else(|| {
                    ctx_helper
                        .as_ref()
                        .and_then(tsz_solver::ContextualTypeContext::get_return_type)
                });
                // Async function bodies return the awaited inner type; the function
                // type itself is Promise<inner>. Contextual return typing must
                // therefore use the inner type, not Promise<inner>.
                let return_context = if is_async && !is_generator {
                    return_context
                        .and_then(|ctx_ty| self.unwrap_promise_type(ctx_ty).or(Some(ctx_ty)))
                } else {
                    return_context
                };
                // TS7010/TS7011: Only count as contextual return if it's not UNKNOWN
                // UNKNOWN is a "no type" value and shouldn't prevent implicit any errors
                has_contextual_return = return_context.is_some_and(|t| t != TypeId::UNKNOWN);
                let inferred = self.infer_return_type_from_body(body, return_context);
                return_type = jsdoc_return_context.unwrap_or(inferred);
            }

            // TS7010/TS7011 (implicit any return) is emitted for functions without
            // return type annotations when noImplicitAny is enabled and the return
            // type cannot be inferred (e.g., is 'any' or only returns undefined)
            // Async functions infer Promise<void>, not 'any', so they should NOT trigger TS7010
            // maybe_report_implicit_any_return handles the noImplicitAny check internally
            //
            // CRITICAL FIX: Skip TS7010 check if there's a contextual return type
            // When a function is used as a callback (e.g., array.map(x => ...)), the
            // contextual type provides the expected return type. TypeScript doesn't
            // emit TS7010 in these cases because the contextual type guides inference.
            //
            // JSDoc type annotations also suppress TS7010/TS7011 in JS files.
            // When a function has any JSDoc type info (@param, @returns, @template),
            // tsc considers it as having explicit types and doesn't emit TS7010.
            let has_jsdoc_return = func_jsdoc
                .as_ref()
                .is_some_and(|j| Self::jsdoc_has_type_annotations(j));
            let is_promise_executor = self.is_promise_executor_function(idx);
            let is_accessor_node = node.kind == syntax_kind_ext::GET_ACCESSOR
                || node.kind == syntax_kind_ext::SET_ACCESSOR;
            // For closures (function expressions / arrow functions), defer TS7010/TS7011
            // during the build_type_environment phase.  During that phase, contextual
            // types are not yet available (they're set during check_variable_declaration).
            // The closure will be re-evaluated with contextual types during the checking
            // phase, at which point TS7010/TS7011 can fire if still warranted.
            let skip_implicit_any_return =
                is_closure && !self.ctx.is_checking_statements && !has_contextual_return;
            if !is_function_declaration
                && !is_accessor_node
                && !is_async
                && !has_contextual_return
                && !has_jsdoc_return
                && !is_promise_executor
                && !skip_implicit_any_return
            {
                self.maybe_report_implicit_any_return(
                    name_for_error,
                    name_node,
                    return_type,
                    has_type_annotation,
                    has_contextual_return,
                    idx,
                );
            }

            // TS2705: Async function in ES5/ES3 requires the Promise constructor.
            // Only emit when targeting ES5/ES3 and Promise is not available globally.
            if is_async && !is_generator {
                use crate::context::ScriptTarget;
                let is_es5_or_lower = matches!(
                    self.ctx.compiler_options.target,
                    ScriptTarget::ES3 | ScriptTarget::ES5
                );
                let should_check_promise_constructor =
                    !is_function_declaration || has_type_annotation;
                let missing_promise_for_target = !self.ctx.has_promise_constructor_in_scope();
                if is_es5_or_lower && should_check_promise_constructor && missing_promise_for_target
                {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    let diagnostic_node = if async_node_idx.is_none() {
                        idx
                    } else {
                        async_node_idx
                    };
                    if !is_function_declaration {
                        self.error_at_node_msg(
                            diagnostic_node,
                            diagnostic_codes::CANNOT_FIND_GLOBAL_VALUE,
                            &["Promise"],
                        );
                    }
                    self.error_at_node(
                        diagnostic_node,
                        diagnostic_messages::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
                        diagnostic_codes::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
                    );
                }
            }

            // TS2705: Async function must return Promise
            // Check ALL async functions (not just arrow functions and function expressions)
            // Note: Async generators (async function* or async *method) should NOT trigger TS2705
            // because they return AsyncGenerator or AsyncIterator, not Promise
            if has_type_annotation {
                // Only check non-generator async functions with explicit return types that aren't Promise
                // Skip this check if:
                // 1. The return type is ERROR (unresolved reference, likely Promise not in lib)
                // 2. The return type annotation text looks like it references Promise
                if is_async && !is_generator {
                    use tsz_scanner::SyntaxKind;

                    let should_emit_ts2705 = if self.is_promise_type(return_type) {
                        // Return type is Promise - OK
                        false
                    } else if return_type != TypeId::ERROR {
                        // Return type resolved successfully but is not Promise - emit error
                        !self.return_type_annotation_looks_like_promise(type_annotation)
                    } else {
                        // Return type is ERROR - use syntactic fallback
                        // Check if the type annotation is a primitive keyword (never valid for async function)
                        let type_node_result = self.ctx.arena.get(type_annotation);
                        match type_node_result {
                            Some(type_node) => {
                                // Primitives are definitely not valid async function return types
                                matches!(
                                    type_node.kind as u32,
                                    k if k == SyntaxKind::StringKeyword as u32
                                        || k == SyntaxKind::NumberKeyword as u32
                                        || k == SyntaxKind::BooleanKeyword as u32
                                        || k == SyntaxKind::VoidKeyword as u32
                                        || k == SyntaxKind::UndefinedKeyword as u32
                                        || k == SyntaxKind::NullKeyword as u32
                                        || k == SyntaxKind::NeverKeyword as u32
                                        || k == SyntaxKind::ObjectKeyword as u32
                                )
                            }
                            None => false,
                        }
                    };

                    if should_emit_ts2705 {
                        use crate::context::ScriptTarget;
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

                        // For ES5/ES3 targets, emit TS1055 instead of TS2705
                        // TS1055: "Type 'X' is not a valid async function return type in ES5 because
                        //          it does not refer to a Promise-compatible constructor value."
                        // TS2705: "Async function return type must be Promise."
                        let is_es5_or_lower = matches!(
                            self.ctx.compiler_options.target,
                            ScriptTarget::ES3 | ScriptTarget::ES5
                        );

                        if is_es5_or_lower {
                            let type_name = self.format_type(return_type);
                            self.error_at_node(
                                type_annotation,
                                &format_message(
                                    diagnostic_messages::TYPE_IS_NOT_A_VALID_ASYNC_FUNCTION_RETURN_TYPE_IN_ES5_BECAUSE_IT_DOES_NOT_REFER,
                                    &[&type_name],
                                ),
                                diagnostic_codes::TYPE_IS_NOT_A_VALID_ASYNC_FUNCTION_RETURN_TYPE_IN_ES5_BECAUSE_IT_DOES_NOT_REFER,
                            );
                        } else {
                            // TS1064: For ES6+ targets, the return type must be Promise<T>
                            let type_name = self.format_type(return_type);
                            self.error_at_node(
                                type_annotation,
                                &format_message(
                                    diagnostic_messages::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
                                    &[&type_name],
                                ),
                                diagnostic_codes::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
                            );
                        }
                    }
                }

                // Note: TS2705 check for functions without explicit return types has been removed.
                // TypeScript only emits TS2705 when there's an explicit non-Promise return type.
                // The "Promise not in lib" check was also removed as it was emitting false positives.
            }

            // TS2366 (not all code paths return value) for function expressions and arrow functions
            // Check if all code paths return a value when return type requires it
            if !is_function_declaration && body.is_some() {
                let check_return_type = return_type;
                let requires_return = self.requires_return_value(check_return_type);
                let has_return = self.body_has_return_with_value(body);
                let falls_through = self.function_body_falls_through(body);

                // Determine if this is an async function
                let is_async = if let Some(func) = self.ctx.arena.get_function(node) {
                    func.is_async
                } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    self.has_async_modifier(&method.modifiers)
                } else {
                    false
                };

                // TS2355: Skip for async functions - they implicitly return Promise<void>
                // Async functions without a return statement automatically resolve to Promise<void>
                // so they should not emit "function must return a value" errors
                if has_type_annotation && requires_return && falls_through && !is_async {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    if !has_return {
                        self.error_at_node(
                            type_annotation,
                            "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                            diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                        );
                    } else if self.ctx.strict_null_checks() {
                        // TS2366: Only emit with strictNullChecks
                        self.error_at_node(
                            type_annotation,
                            diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                            diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                        );
                    }
                } else if self.ctx.no_implicit_returns() && has_return && falls_through {
                    // TS7030: noImplicitReturns - not all code paths return a value
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    let error_node = if let Some(nn) = name_node { nn } else { body };
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                        diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                    );
                }
            }

            // Determine if this is an async function for context tracking
            let is_async_for_context = if let Some(func) = self.ctx.arena.get_function(node) {
                func.is_async
            } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                self.has_async_modifier(&method.modifiers)
            } else {
                false
            };

            // Enter async context if applicable
            if is_async_for_context {
                self.ctx.enter_async_context();
            }

            // Push this_type to the stack before checking the body
            // This ensures this references inside the function have the proper type context
            // For functions with explicit this parameter: use that type
            // For arrow functions: use outer this type (already captured in this_type)
            // For regular functions without explicit this: this_type is None, which triggers TS2683 when this is used
            // this_type was already pushed early (before parameter initializer checks)
            // so we don't need to push it again here

            // For generator functions with explicit return type (Generator<Y, R, N> or AsyncGenerator<Y, R, N>),
            // return statements should be checked against TReturn (R), not the full Generator type.
            // This matches TypeScript's behavior where `return x` in a generator checks `x` against TReturn.
            //
            // For async functions with return type Promise<T>, return statements should be checked
            // against T, not Promise<T>. The function body returns T, which gets auto-wrapped.
            let body_return_type = if is_generator && has_type_annotation {
                self.get_generator_return_type_argument(return_type)
                    .unwrap_or(return_type)
            } else if is_async_for_context && has_type_annotation {
                // Unwrap Promise<T> to T for async function return type checking
                self.unwrap_promise_type(return_type).unwrap_or(return_type)
            } else {
                return_type
            };

            self.push_return_type(body_return_type);

            // For generator functions with explicit annotations, push the yield type
            // from the annotation. Contextually-typed generators already had their yield
            // type pushed early (before infer_return_type_from_body).
            if is_generator && has_type_annotation {
                let yield_type = self.get_generator_yield_type_argument(return_type);
                self.ctx.push_yield_type(yield_type);
            } else if early_yield_type.is_none() {
                // No early push was done, push None for stack balance
                self.ctx.push_yield_type(None);
            }

            if let Some(jsdoc_expected_return) = jsdoc_return_context
                && let Some(body_node) = self.ctx.arena.get(body)
                && body_node.kind != syntax_kind_ext::BLOCK
            {
                // In JS/checkJs, expression-bodied arrows can carry inline JSDoc casts
                // (e.g. `/** @type {T} */(expr)`); use that annotated type when present.
                let actual_return = self
                    .jsdoc_type_annotation_for_node(body)
                    .unwrap_or_else(|| self.get_type_of_node(body));
                self.check_assignable_or_report(actual_return, jsdoc_expected_return, body);
            }
            // Skip body statement checking for function declarations.
            // Function declarations are checked via check_function_declaration (in
            // state_checking_members.rs) which correctly maintains the full type
            // parameter scope chain for nested functions.  get_type_of_function can
            // be called lazily (e.g. via get_type_of_symbol) outside the enclosing
            // function's scope, so it would only have its own type params - not the
            // outer function's - causing false TS2304 "Cannot find name" errors for
            // outer type parameters like T/U in nested generics.
            if !is_function_declaration {
                // Save and reset control flow context (function body creates new context)
                let saved_cf_context = (
                    self.ctx.iteration_depth,
                    self.ctx.switch_depth,
                    self.ctx.label_stack.len(),
                    self.ctx.had_outer_loop,
                );
                // If we were in a loop/switch, or already had an outer loop, mark it
                if self.ctx.iteration_depth > 0
                    || self.ctx.switch_depth > 0
                    || self.ctx.had_outer_loop
                {
                    self.ctx.had_outer_loop = true;
                }
                self.ctx.iteration_depth = 0;
                self.ctx.switch_depth = 0;
                // Note: function_depth was already incremented at body entry
                // For expression-bodied arrows (non-block bodies), propagate the
                // contextual return type so nested expressions (object literals,
                // lambdas) retain proper contextual typing during statement checking.
                let prev_ctx_for_body = self.ctx.contextual_type;
                if let Some(body_node) = self.ctx.arena.get(body)
                    && body_node.kind != syntax_kind_ext::BLOCK
                    && !has_type_annotation
                {
                    let body_return_context = ctx_helper
                        .as_ref()
                        .and_then(tsz_solver::ContextualTypeContext::get_return_type);
                    if body_return_context.is_some() {
                        self.ctx.contextual_type = body_return_context;
                    }
                }
                self.check_statement(body);
                self.ctx.contextual_type = prev_ctx_for_body;
                // Restore control flow context
                self.ctx.iteration_depth = saved_cf_context.0;
                self.ctx.switch_depth = saved_cf_context.1;
                self.ctx.label_stack.truncate(saved_cf_context.2);
                self.ctx.had_outer_loop = saved_cf_context.3;
            }
            self.pop_return_type();
            self.ctx.pop_yield_type();

            // Exit async context
            if is_async_for_context {
                self.ctx.exit_async_context();
            }

            // Restore function_depth (incremented at body entry)
            self.ctx.function_depth -= 1;
        }

        // Pop this_type that was pushed before parameter initializer checks
        if pushed_this_type_early {
            self.ctx.this_type_stack.pop();
        }

        // In JS files, functions that reference `arguments` in their body should accept
        // any number of extra arguments (TSC adds an implicit rest parameter).
        // Only add if the function doesn't already have a rest parameter.
        // For function declarations, the body wasn't checked yet (it's checked in
        // check_function_declaration), so we also pre-walk the body for `arguments`.
        let is_js_file_for_rest = self.ctx.file_name.ends_with(".js")
            || self.ctx.file_name.ends_with(".jsx")
            || self.ctx.file_name.ends_with(".mjs")
            || self.ctx.file_name.ends_with(".cjs");
        let uses_arguments = self.ctx.js_body_uses_arguments
            || (is_function_declaration && self.body_has_arguments_reference(body));
        if is_js_file_for_rest && uses_arguments && !params.last().is_some_and(|p| p.rest) {
            params.push(ParamInfo {
                name: None,
                type_id: TypeId::ANY,
                optional: true,
                rest: true,
            });
        }
        // Restore the arguments tracking flag
        self.ctx.js_body_uses_arguments = saved_uses_arguments;

        // Create function type using TypeInterner
        // For annotated return types, use the original un-evaluated type so callers see
        // Promise<T>, Array<T>, etc. as Application types. This preserves type identity
        // for await unwrapping and generic type parameter extraction.
        // For inferred return types (no annotation), use the inferred type as-is.
        let mut final_return_type = if !has_type_annotation && function_is_generator {
            // Unannotated generators should remain permissive until full
            // Generator<Y, R, N>/AsyncGenerator<Y, R, N> inference is implemented.
            TypeId::ANY
        } else {
            annotated_return_type.unwrap_or(return_type)
        };
        // Unannotated async functions infer Promise<T>, where T is inferred from
        // return statements in the function body.
        if !has_type_annotation && function_is_async && !function_is_generator {
            // Resolve the real Promise type from lib files when available,
            // so that the return type is structurally compatible with PromiseLike<T>.
            // Fall back to synthetic PROMISE_BASE only without lib files.
            let promise_base = {
                let lib_binders = self.get_lib_binders();
                self.ctx
                    .binder
                    .get_global_type_with_libs("Promise", &lib_binders)
                    .map(|sym_id| self.ctx.create_lazy_type_ref(sym_id))
                    .unwrap_or(TypeId::PROMISE_BASE)
            };
            final_return_type = self
                .ctx
                .types
                .factory()
                .application(promise_base, vec![final_return_type]);
        }

        let shape = FunctionShape {
            type_params,
            params,
            this_type,
            return_type: final_return_type,
            type_predicate,
            is_constructor: false,
            is_method: false,
        };

        let function_type = self.ctx.types.factory().function(shape);

        self.pop_type_parameters(jsdoc_type_param_updates);
        self.pop_type_parameters(type_param_updates);
        self.pop_type_parameters(enclosing_type_param_updates);

        return_with_cleanup!(function_type)
    }

    fn contextual_type_params_from_expected(&self, expected: TypeId) -> Option<Vec<TypeParamInfo>> {
        tsz_solver::type_queries::extract_contextual_type_params(self.ctx.types, expected)
    }

    /// Check if a function body references the `arguments` object.
    /// Walks the AST recursively but stops at nested function boundaries.
    /// Used by JS files to determine if a function needs an implicit rest parameter.
    fn body_has_arguments_reference(&self, body: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(body) else {
            return false;
        };

        // Check if this node is an identifier named "arguments"
        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            return ident.escaped_text == "arguments";
        }

        // Stop at nested function/method/class boundaries
        let k = node.kind;
        if k == syntax_kind_ext::FUNCTION_DECLARATION
            || k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::ARROW_FUNCTION
            || k == syntax_kind_ext::METHOD_DECLARATION
            || k == syntax_kind_ext::CLASS_DECLARATION
            || k == syntax_kind_ext::CLASS_EXPRESSION
        {
            return false;
        }

        // Walk children based on node kind
        if let Some(block) = self.ctx.arena.get_block(node) {
            for &stmt in &block.statements.nodes {
                if self.body_has_arguments_reference(stmt) {
                    return true;
                }
            }
        } else if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
            if self.body_has_arguments_reference(expr_stmt.expression) {
                return true;
            }
        } else if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
            for &decl in &var_stmt.declarations.nodes {
                if self.body_has_arguments_reference(decl) {
                    return true;
                }
            }
        } else if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
            if self.body_has_arguments_reference(var_decl.initializer) {
                return true;
            }
        } else if let Some(ret) = self.ctx.arena.get_return_statement(node) {
            if self.body_has_arguments_reference(ret.expression) {
                return true;
            }
        } else if let Some(call) = self.ctx.arena.get_call_expr(node) {
            if self.body_has_arguments_reference(call.expression) {
                return true;
            }
            if let Some(ref args) = call.arguments {
                for &arg in &args.nodes {
                    if self.body_has_arguments_reference(arg) {
                        return true;
                    }
                }
            }
        } else if let Some(bin) = self.ctx.arena.get_binary_expr(node) {
            if self.body_has_arguments_reference(bin.left)
                || self.body_has_arguments_reference(bin.right)
            {
                return true;
            }
        } else if let Some(access) = self.ctx.arena.get_access_expr(node) {
            if self.body_has_arguments_reference(access.expression) {
                return true;
            }
            // Element access: also check the argument (e.g. arguments[0])
            if self.body_has_arguments_reference(access.name_or_argument) {
                return true;
            }
        } else if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
            if self.body_has_arguments_reference(if_stmt.expression)
                || self.body_has_arguments_reference(if_stmt.then_statement)
                || self.body_has_arguments_reference(if_stmt.else_statement)
            {
                return true;
            }
        } else if let Some(loop_stmt) = self.ctx.arena.get_loop(node) {
            if self.body_has_arguments_reference(loop_stmt.initializer)
                || self.body_has_arguments_reference(loop_stmt.condition)
                || self.body_has_arguments_reference(loop_stmt.incrementor)
                || self.body_has_arguments_reference(loop_stmt.statement)
            {
                return true;
            }
        } else if let Some(for_in_of) = self.ctx.arena.get_for_in_of(node) {
            if self.body_has_arguments_reference(for_in_of.expression)
                || self.body_has_arguments_reference(for_in_of.statement)
            {
                return true;
            }
        } else if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
            if self.body_has_arguments_reference(paren.expression) {
                return true;
            }
        } else if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
            if self.body_has_arguments_reference(unary.operand) {
                return true;
            }
        } else if let Some(unary_ex) = self.ctx.arena.get_unary_expr_ex(node) {
            if self.body_has_arguments_reference(unary_ex.expression) {
                return true;
            }
        } else if let Some(spread) = self.ctx.arena.get_spread(node) {
            if self.body_has_arguments_reference(spread.expression) {
                return true;
            }
        } else if let Some(cond) = self.ctx.arena.get_conditional_expr(node)
            && (self.body_has_arguments_reference(cond.condition)
                || self.body_has_arguments_reference(cond.when_true)
                || self.body_has_arguments_reference(cond.when_false))
        {
            return true;
        }

        false
    }

    /// Push type parameters from all enclosing generic functions/classes/interfaces.
    ///
    /// When `get_type_of_function` is called lazily (e.g., via `get_type_of_symbol`),
    /// outer type parameters are not in scope.  This method walks up the AST to find
    /// enclosing declarations with type parameters and adds them to the scope so that
    /// type parameter references in constraints, parameter types, and return types
    /// resolve correctly.
    ///
    /// Returns the update list for `pop_type_parameters`.
    pub(crate) fn push_enclosing_type_parameters(
        &mut self,
        func_idx: NodeIndex,
    ) -> Vec<(String, Option<TypeId>)> {
        use tsz_parser::parser::syntax_kind_ext;

        // Collect enclosing type parameter node indices (inner-to-outer order)
        let mut enclosing_param_indices: Vec<Vec<NodeIndex>> = Vec::new();
        let mut current = func_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            let type_param_nodes: Option<Vec<NodeIndex>> = match parent.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
                {
                    self.ctx
                        .arena
                        .get_function(parent)
                        .and_then(|f| f.type_parameters.as_ref())
                        .map(|tp| tp.nodes.clone())
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    self.ctx
                        .arena
                        .get_class(parent)
                        .and_then(|c| c.type_parameters.as_ref())
                        .map(|tp| tp.nodes.clone())
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                    .ctx
                    .arena
                    .get_interface(parent)
                    .and_then(|i| i.type_parameters.as_ref())
                    .map(|tp| tp.nodes.clone()),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(parent)
                    .and_then(|m| m.type_parameters.as_ref())
                    .map(|tp| tp.nodes.clone()),
                _ => None,
            };

            if let Some(indices) = type_param_nodes {
                enclosing_param_indices.push(indices);
            }

            current = parent_idx;
        }

        if enclosing_param_indices.is_empty() {
            return Vec::new();
        }

        // Push from outermost to innermost (reverse the inner-to-outer collection)
        // Use two-pass approach (like push_type_parameters) so that constraints
        // like `U extends T` can reference other type parameters from the same scope.
        let mut updates = Vec::new();
        let mut added_params: Vec<NodeIndex> = Vec::new();
        let factory = self.ctx.types.factory();

        // Pass 1: Add all type parameters to scope WITHOUT constraints
        for param_indices in enclosing_param_indices.into_iter().rev() {
            for param_idx in param_indices {
                let Some(node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                    continue;
                };

                let name = self
                    .ctx
                    .arena
                    .get(data.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());
                let atom = self.ctx.types.intern_string(&name);

                let info = tsz_solver::TypeParamInfo {
                    name: atom,
                    constraint: None,
                    default: None,
                    is_const: false,
                };
                let type_id = factory.type_param(info);

                // Only add if not already in scope (inner scope should shadow outer)
                if !self.ctx.type_parameter_scope.contains_key(&name) {
                    let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
                    updates.push((name, previous));
                    added_params.push(param_idx);
                }
            }
        }

        // Pass 2: Resolve constraints now that all type parameters are in scope
        for param_idx in added_params {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };

            if data.constraint == NodeIndex::NONE {
                continue;
            }

            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());
            let atom = self.ctx.types.intern_string(&name);

            let constraint_type = self.get_type_from_type_node(data.constraint);
            let constraint = (constraint_type != TypeId::ERROR).then_some(constraint_type);

            // Update scope with constrained version
            let info = tsz_solver::TypeParamInfo {
                name: atom,
                constraint,
                default: None,
                is_const: false,
            };
            let constrained_type_id = factory.type_param(info);
            self.ctx
                .type_parameter_scope
                .insert(name, constrained_type_id);
        }

        updates
    }
}
