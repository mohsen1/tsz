//! Function Type Resolution Module
//!
//! This module contains function type resolution methods for CheckerState
//! as part of the Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Function/method/arrow function type resolution
//! - Parameter type inference and contextual typing
//! - Return type inference and validation
//! - Property access type resolution
//! - Async function Promise return type validation
//!
//! This module extends CheckerState with utilities for function type
//! resolution, providing cleaner separation of function typing logic.

use crate::state::{CheckerState, MAX_INSTANTIATION_DEPTH};
use crate::types::diagnostics::format_message;
use rustc_hash::FxHashMap;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
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

        let (_function_is_async, function_is_generator) =
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
        // IMPORTANT: Evaluate Application and Lazy types before creating context
        // - Application types: fix TS2571 false positives (see: docs/TS2571_INVESTIGATION.md)
        // - Lazy types (type aliases): fix TS7006 false positives for contextual parameter typing
        let mut contextual_signature_type_params = None;
        let ctx_helper = if let Some(ctx_type) = self.ctx.contextual_type {
            use tsz_solver::type_queries::{get_lazy_def_id, get_type_application};

            // Evaluate the contextual type to resolve type aliases and generic applications
            let evaluated_type = if get_type_application(self.ctx.types, ctx_type).is_some() {
                // Evaluate Application type to get the actual function signature
                // This fixes cases like: Destructuring<TFuncs1, T> where the contextual type
                // is a generic type alias that needs to be instantiated
                self.evaluate_application_type(ctx_type)
            } else if get_lazy_def_id(self.ctx.types, ctx_type).is_some() {
                // Evaluate Lazy type (type alias) to get the underlying function signature
                // This fixes cases like: type Handler = (e: string) => void
                // where contextual typing should infer parameter types from the alias
                self.judge_evaluate(ctx_type)
            } else {
                // Not an Application or Lazy type, use as-is
                ctx_type
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
                    let ty = factory.type_param(info);
                    jsdoc_type_param_types.insert(name, ty);
                    jsdoc_type_params.push(info);
                }
                type_params = jsdoc_type_params;
            }
        }
        let jsdoc_return_context = func_jsdoc
            .as_ref()
            .and_then(|j| Self::jsdoc_returns_type_name(j))
            .and_then(|name| jsdoc_type_param_types.get(&name).copied());

        let mut contextual_index = 0;
        for &param_idx in parameters.nodes.iter() {
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
                let has_contextual_type =
                    contextual_type.is_some_and(|t| t != TypeId::UNKNOWN || !is_js_file);

                // Use type annotation if present, otherwise infer from context
                let type_id = if !param.type_annotation.is_none() {
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
                let has_jsdoc_param = if !has_contextual_type && param.type_annotation.is_none() {
                    let from_func_jsdoc = if let Some(ref jsdoc) = func_jsdoc {
                        let pname = self.parameter_name_for_error(param.name);
                        Self::jsdoc_has_param_type(jsdoc, &pname)
                    } else {
                        false
                    };
                    from_func_jsdoc || self.param_has_inline_jsdoc_type(param_idx)
                } else {
                    false
                };
                let skip_implicit_any =
                    is_closure && !self.ctx.is_checking_statements && !has_contextual_type;
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
                    || !param.initializer.is_none()
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
        let has_type_annotation = !type_annotation.is_none();
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
        let annotated_return_type = if has_type_annotation {
            Some(return_type)
        } else {
            None
        };

        // Evaluate Application types in return type to get their structural form
        // This allows proper comparison of return expressions against type alias applications like Reducer<S, A>
        return_type = self.evaluate_application_type(return_type);

        // Check the function body (for type errors within the body)
        // Save/restore the arguments tracking flag for nested function handling
        let saved_uses_arguments = self.ctx.js_body_uses_arguments;
        self.ctx.js_body_uses_arguments = false;
        self.check_non_impl_parameter_initializers(&parameters.nodes, false, !body.is_none());
        if !body.is_none() {
            // Track that we're inside a nested function for abstract property access checks.
            // This must happen before infer_return_type_from_body which evaluates body expressions.
            self.ctx.function_depth += 1;
            self.cache_parameter_types(&parameters.nodes, Some(&param_types));

            // Assign contextual types to destructuring parameters (binding patterns)
            // This allows destructuring patterns in callbacks to infer element types from contextual types
            self.assign_contextual_types_to_destructuring_params(&parameters.nodes, &param_types);

            // Check that parameter default values are assignable to declared types (TS2322)
            self.check_parameter_initializers(&parameters.nodes);

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

            // Push this_type EARLY so that infer_return_type_from_body has
            // the correct `this` context (prevents false TS2683 during inference)
            let mut pushed_this_type_early = false;
            if let Some(tt) = this_type {
                self.ctx.this_type_stack.push(tt);
                pushed_this_type_early = true;
            }

            let mut has_contextual_return = false;
            if !has_type_annotation {
                let return_context = jsdoc_return_context.or_else(|| {
                    ctx_helper
                        .as_ref()
                        .and_then(|helper| helper.get_return_type())
                });
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
            if !is_function_declaration
                && !is_accessor_node
                && !is_async
                && !has_contextual_return
                && !has_jsdoc_return
                && !is_promise_executor
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
                    use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
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
                        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};

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
            if !is_function_declaration && !body.is_none() {
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
                    use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
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
                    use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
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
            // this_type was already pushed early (before infer_return_type_from_body)
            // so we don't need to push it again here
            let pushed_this_type = pushed_this_type_early;

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
            if let Some(jsdoc_expected_return) = jsdoc_return_context
                && let Some(body_node) = self.ctx.arena.get(body)
                && body_node.kind != syntax_kind_ext::BLOCK
            {
                let actual_return = self.get_type_of_node(body);
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
                self.check_statement(body);
                // Restore control flow context
                self.ctx.iteration_depth = saved_cf_context.0;
                self.ctx.switch_depth = saved_cf_context.1;
                self.ctx.label_stack.truncate(saved_cf_context.2);
                self.ctx.had_outer_loop = saved_cf_context.3;
            }
            self.pop_return_type();

            if pushed_this_type {
                self.ctx.this_type_stack.pop();
            }

            // Exit async context
            if is_async_for_context {
                self.ctx.exit_async_context();
            }

            // Restore function_depth (incremented at body entry)
            self.ctx.function_depth -= 1;
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
        let final_return_type = if !has_type_annotation && function_is_generator {
            // Unannotated generators should remain permissive until full
            // Generator<Y, R, N>/AsyncGenerator<Y, R, N> inference is implemented.
            TypeId::ANY
        } else {
            annotated_return_type.unwrap_or(return_type)
        };

        let shape = FunctionShape {
            type_params,
            params,
            this_type,
            return_type: final_return_type,
            type_predicate,
            is_constructor: false,
            is_method: false,
        };

        self.pop_type_parameters(type_param_updates);
        self.pop_type_parameters(enclosing_type_param_updates);

        return_with_cleanup!(self.ctx.types.factory().function(shape))
    }

    fn contextual_type_params_from_expected(&self, expected: TypeId) -> Option<Vec<TypeParamInfo>> {
        use tsz_solver::type_queries::{
            get_callable_shape, get_function_shape, get_type_application, get_union_members,
        };

        if let Some(shape) = get_function_shape(self.ctx.types, expected) {
            return if shape.type_params.is_empty() {
                None
            } else {
                Some(shape.type_params.clone())
            };
        }

        if let Some(shape) = get_callable_shape(self.ctx.types, expected) {
            if shape.call_signatures.len() != 1 {
                return None;
            }
            let sig = &shape.call_signatures[0];
            return if sig.type_params.is_empty() {
                None
            } else {
                Some(sig.type_params.clone())
            };
        }

        if let Some(app) = get_type_application(self.ctx.types, expected) {
            return self.contextual_type_params_from_expected(app.base);
        }

        if let Some(members) = get_union_members(self.ctx.types, expected) {
            if members.is_empty() {
                return None;
            }

            let mut candidate: Option<Vec<TypeParamInfo>> = None;
            for &member in members.iter() {
                let params = self.contextual_type_params_from_expected(member)?;
                if let Some(existing) = &candidate {
                    if existing.len() != params.len()
                        || existing
                            .iter()
                            .zip(params.iter())
                            .any(|(left, right)| left != right)
                    {
                        return None;
                    }
                } else {
                    candidate = Some(params);
                }
            }

            return candidate;
        }

        None
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
        } else if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
            if self.body_has_arguments_reference(cond.condition)
                || self.body_has_arguments_reference(cond.when_true)
                || self.body_has_arguments_reference(cond.when_false)
            {
                return true;
            }
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
        let mut updates = Vec::new();
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
                    .map(|id_data| id_data.escaped_text.clone())
                    .unwrap_or_else(|| "T".to_string());
                let atom = self.ctx.types.intern_string(&name);

                // Create an unconstrained type parameter placeholder.
                // Constraints are not resolved here - that happens in the proper
                // check_function_declaration flow with full scope context.
                let info = tsz_solver::TypeParamInfo {
                    name: atom,
                    constraint: None,
                    default: None,
                    is_const: false,
                };
                let type_id = self.ctx.types.factory().type_param(info);

                // Only add if not already in scope (inner scope should shadow outer)
                if !self.ctx.type_parameter_scope.contains_key(&name) {
                    let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
                    updates.push((name, previous));
                }
            }
        }
        updates
    }

    // =========================================================================
    // Property Access Type Resolution
    // =========================================================================

    /// Get type of property access expression.
    pub(crate) fn get_type_of_property_access(&mut self, idx: NodeIndex) -> TypeId {
        if *self.ctx.instantiation_depth.borrow() >= MAX_INSTANTIATION_DEPTH {
            return TypeId::ERROR; // Max instantiation depth exceeded - propagate error
        }

        *self.ctx.instantiation_depth.borrow_mut() += 1;
        let result = self.get_type_of_property_access_inner(idx);
        *self.ctx.instantiation_depth.borrow_mut() -= 1;
        result
    }

    /// Inner implementation of property access type resolution.
    fn get_type_of_property_access_inner(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_solver::operations_property::PropertyAccessResult;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return TypeId::ERROR; // Missing access expression data - propagate error
        };
        let factory = self.ctx.types.factory();

        // Get the property name first (needed for abstract property check regardless of object type)
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            // Preserve diagnostics on the base expression (e.g. TS2304 for `missing.`)
            // even when parser recovery could not build a property name node.
            let _ = self.get_type_of_node(access.expression);
            return TypeId::ERROR;
        };
        if let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text.is_empty()
        {
            // Preserve diagnostics on the base expression when member name is missing.
            let _ = self.get_type_of_node(access.expression);
            return TypeId::ERROR;
        }

        // Check for abstract property access in constructor BEFORE evaluating types (error 2715)
        // This must happen even when `this` has type ANY
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            if self.is_this_expression(access.expression)
                && let Some(ref class_info) = self.ctx.enclosing_class.clone()
                && class_info.in_constructor
                && self.ctx.function_depth == 0  // Skip inside nested functions/arrow functions
                && self.is_abstract_member(&class_info.member_nodes, property_name)
            {
                self.error_abstract_property_in_constructor(
                    property_name,
                    &class_info.name,
                    access.name_or_argument,
                );
            }
        }

        // Fast path for enum member value access (`E.Member`).
        // This avoids the general property-access pipeline (accessibility checks,
        // type environment classification, etc.) for a very common hot path.
        if let Some(name_ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &name_ident.escaped_text;
            if let Some(base_sym_id) = self.resolve_identifier_symbol(access.expression)
                && let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id)
                && base_symbol.flags & symbol_flags::ENUM != 0
                && let Some(exports) = base_symbol.exports.as_ref()
                && let Some(member_sym_id) = exports.get(property_name)
            {
                // TS2450: Check if enum is used before its declaration (TDZ violation).
                // Only non-const enums are flagged (const enums are always hoisted).
                if let Some(base_node) = self.ctx.arena.get(access.expression) {
                    if let Some(base_ident) = self.ctx.arena.get_identifier(base_node) {
                        let base_name = &base_ident.escaped_text;
                        if self.check_tdz_violation(base_sym_id, access.expression, base_name) {
                            return TypeId::ERROR;
                        }
                    }
                }

                // Check if the member is an enum member or a namespace export
                let member_symbol = self.ctx.binder.get_symbol(member_sym_id);
                let is_enum_member = member_symbol
                    .map(|s| s.flags & symbol_flags::ENUM_MEMBER != 0)
                    .unwrap_or(false);

                if is_enum_member {
                    // Enum member property access should produce the member type (`E.A`),
                    // not the enum namespace type (`E`). Member types remain assignable to
                    // their parent enum via solver compatibility rules.
                    let member_type = self.get_type_of_symbol(member_sym_id);
                    return self.apply_flow_narrowing(idx, member_type);
                } else {
                    // Namespace exports (functions, variables, etc.) - use their actual type
                    let member_type = self.get_type_of_symbol(member_sym_id);
                    return self.apply_flow_narrowing(idx, member_type);
                }
            }
        }

        // Get the type of the object
        let original_object_type = self.get_type_of_node(access.expression);

        // Evaluate Application types to resolve generic type aliases/interfaces
        // But preserve original for error messages to maintain nominal identity (e.g., D<string>)
        let object_type = self.evaluate_application_type(original_object_type);

        // Handle optional chain continuations: for `o?.b.c`, when processing `.c`,
        // the object type from `o?.b` includes `undefined` from the optional chain.
        // But `.c` should only be reached when `o` is defined, so we strip nullish
        // types. Only do this when this access is NOT itself an optional chain
        // (`question_dot_token` is false) but is part of one (parent has `?.`).
        let object_type = if !access.question_dot_token
            && crate::optional_chain::is_optional_chain(&self.ctx.arena, access.expression)
        {
            let (non_nullish, _) = self.split_nullish_type(object_type);
            non_nullish.unwrap_or(object_type)
        } else {
            object_type
        };

        if name_node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return self.get_type_of_private_property_access(
                idx,
                access,
                access.name_or_argument,
                object_type,
            );
        }

        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;
            if self.is_global_this_expression(access.expression) {
                let property_type =
                    self.resolve_global_this_property_type(property_name, access.name_or_argument);
                if property_type == TypeId::ERROR {
                    return TypeId::ERROR;
                }
                return self.apply_flow_narrowing(idx, property_type);
            }
        }

        // Don't report errors for any/error types - check BEFORE accessibility
        // to prevent cascading errors when the object type is already invalid
        if object_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if object_type == TypeId::ERROR {
            return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
        }

        // Check for never type - emit TS18050 "The value 'never' cannot be used here"
        if object_type == TypeId::NEVER {
            if !access.question_dot_token {
                self.report_never_type_usage(access.expression);
            }
            return if access.question_dot_token {
                TypeId::UNDEFINED
            } else {
                TypeId::ERROR
            };
        }

        // Enforce private/protected access modifiers when possible
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;
            if !self.check_property_accessibility(
                access.expression,
                property_name,
                access.name_or_argument,
                object_type,
            ) {
                return TypeId::ERROR;
            }
        }

        // Check for merged class/enum/function + namespace symbols
        // When a class/enum/function merges with a namespace (same name), the symbol has both
        // value constructor flags and MODULE flags. We need to check the symbol's exports.
        // This handles value access like `Foo.value` when Foo is both a class and namespace.
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            // For value access to merged symbols, check the exports directly
            // This is needed because the type system doesn't track which symbol a Callable came from
            if let Some(expr_node) = self.ctx.arena.get(access.expression)
                && let Some(expr_ident) = self.ctx.arena.get_identifier(expr_node)
            {
                let expr_name = &expr_ident.escaped_text;
                // Try file_locals first (fast path for top-level symbols)
                if let Some(sym_id) = self.ctx.binder.file_locals.get(expr_name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    // Check if this is a merged symbol (has both MODULE and value constructor flags)
                    let is_merged = (symbol.flags & symbol_flags::MODULE) != 0
                        && (symbol.flags
                            & (symbol_flags::CLASS
                                | symbol_flags::FUNCTION
                                | symbol_flags::REGULAR_ENUM))
                            != 0;

                    if is_merged
                        && let Some(exports) = symbol.exports.as_ref()
                        && let Some(member_id) = exports.get(property_name)
                    {
                        // For merged symbols, we return the type for any exported member
                        let member_type = self.get_type_of_symbol(member_id);
                        return self.apply_flow_narrowing(idx, member_type);
                    }
                }
            }
        }

        // If it's an identifier, look up the property
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            let property_name = &ident.escaped_text;

            if self.is_type_only_import_equals_namespace_expr(access.expression) {
                if let Some(ns_name) = self.entity_name_text(access.expression) {
                    self.error_namespace_used_as_value_at(&ns_name, access.expression);
                    if let Some(sym_id) = self.resolve_identifier_symbol(access.expression)
                        && self.alias_resolves_to_type_only(sym_id)
                    {
                        self.error_type_only_value_at(&ns_name, access.expression);
                    }
                }
                return TypeId::ERROR;
            }

            if let Some(member_type) =
                self.resolve_namespace_value_member(object_type, property_name)
            {
                return self.apply_flow_narrowing(idx, member_type);
            }
            if self.namespace_has_type_only_member(object_type, property_name) {
                if self.is_unresolved_import_symbol(access.expression) {
                    return TypeId::ERROR;
                }
                // Don't emit TS2693 in heritage clause context — the heritage
                // checker will emit the appropriate error (e.g., TS2689).
                if self
                    .find_enclosing_heritage_clause(access.name_or_argument)
                    .is_none()
                {
                    // Emit TS2708 for namespace member access (e.g., ns.Interface())
                    // This is "Cannot use namespace as a value"
                    // Get the namespace name from the left side of the access
                    if let Some(ns_name) = self.entity_name_text(access.expression) {
                        self.error_namespace_used_as_value_at(&ns_name, access.expression);
                    }
                    // Also emit TS2693 for the type-only member itself
                    self.error_type_only_value_at(property_name, access.name_or_argument);
                }
                return TypeId::ERROR;
            }
            if self.is_namespace_value_type(object_type)
                && !self.is_enum_instance_property_access(object_type, access.expression)
            {
                if !access.question_dot_token && !property_name.starts_with('#') {
                    self.error_property_not_exist_at(property_name, original_object_type, idx);
                }
                return TypeId::ERROR;
            }

            let object_type_for_access = self.resolve_type_for_property_access(object_type);
            if object_type_for_access == TypeId::ANY {
                return TypeId::ANY;
            }
            if object_type_for_access == TypeId::ERROR {
                return TypeId::ERROR; // Return ERROR instead of ANY to expose type errors
            }

            if self.ctx.strict_bind_call_apply()
                && let Some(strict_method_type) =
                    self.strict_bind_call_apply_method_type(object_type_for_access, property_name)
            {
                return self.apply_flow_narrowing(idx, strict_method_type);
            }

            // Use the environment-aware resolver so that array methods, boxed
            // primitive types, and other lib-registered types are available.
            let result =
                self.resolve_property_access_with_env(object_type_for_access, property_name);

            match result {
                PropertyAccessResult::Success {
                    type_id: prop_type,
                    from_index_signature,
                    ..
                } => {
                    // Check for error 4111: property access from index signature
                    if from_index_signature
                        && self
                            .ctx
                            .compiler_options
                            .no_property_access_from_index_signature
                    {
                        use crate::types::diagnostics::diagnostic_codes;
                        self.error_at_node(
                            access.name_or_argument,
                            &format!(
                                "Property '{}' comes from an index signature, so it must be accessed with ['{}'].",
                                property_name, property_name
                            ),
                            diagnostic_codes::PROPERTY_COMES_FROM_AN_INDEX_SIGNATURE_SO_IT_MUST_BE_ACCESSED_WITH,
                        );
                    }
                    self.apply_flow_narrowing(idx, prop_type)
                }

                PropertyAccessResult::PropertyNotFound { .. } => {
                    if let Some(augmented_type) = self.resolve_array_global_augmentation_property(
                        object_type_for_access,
                        property_name,
                    ) {
                        return self.apply_flow_narrowing(idx, augmented_type);
                    }
                    // Check for optional chaining (?.) - suppress TS2339 error when using optional chaining
                    if access.question_dot_token {
                        // With optional chaining, missing property results in undefined
                        return TypeId::UNDEFINED;
                    }
                    // In JS checkJs mode, CommonJS `module.exports` accesses are valid.
                    if property_name == "exports"
                        && (self.ctx.file_name.ends_with(".js")
                            || self.ctx.file_name.ends_with(".jsx"))
                        && let Some(obj_node) = self.ctx.arena.get(access.expression)
                        && let Some(ident) = self.ctx.arena.get_identifier(obj_node)
                        && ident.escaped_text == "module"
                    {
                        return TypeId::ANY;
                    }
                    // Check for expando function pattern: func.prop = value
                    // TypeScript allows property assignments to function/class declarations
                    // without emitting TS2339. The assigned properties become part of the
                    // function's type (expando pattern).
                    if self.is_expando_function_assignment(
                        idx,
                        access.expression,
                        object_type_for_access,
                    ) {
                        return TypeId::ANY;
                    }

                    // JavaScript files allow dynamic property assignment on 'this' without errors.
                    // In JS files, accessing a property on 'this' that doesn't exist should not error
                    // and should return 'any' type, matching TypeScript's behavior.
                    let is_js_file =
                        self.ctx.file_name.ends_with(".js") || self.ctx.file_name.ends_with(".jsx");
                    let is_this_access =
                        if let Some(obj_node) = self.ctx.arena.get(access.expression) {
                            obj_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
                        } else {
                            false
                        };

                    if is_js_file && is_this_access {
                        // Allow dynamic property on 'this' in JavaScript files
                        return TypeId::ANY;
                    }

                    // TS2576: super.member where `member` exists on the base class static side.
                    if self.is_super_expression(access.expression)
                        && let Some(ref class_info) = self.ctx.enclosing_class
                        && let Some(base_idx) = self.get_base_class_idx(class_info.class_idx)
                        && self.is_method_member_in_class_hierarchy(base_idx, property_name, true)
                            == Some(true)
                    {
                        use crate::types::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };

                        let base_name = self.get_class_name_from_decl(base_idx);
                        let static_member_name = format!("{}.{}", base_name, property_name);
                        let object_type_str = self.format_type(original_object_type);
                        let message = format_message(
                            diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                            &[property_name, &object_type_str, &static_member_name],
                        );
                        self.error_at_node(
                            idx,
                            &message,
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                        );
                        return TypeId::ERROR;
                    }

                    // TS2576: instance.member where `member` exists on the class static side.
                    if !self.is_super_expression(access.expression)
                        && let Some((class_idx, is_static_access)) =
                            self.resolve_class_for_access(access.expression, object_type_for_access)
                        && !is_static_access
                        && self.is_method_member_in_class_hierarchy(class_idx, property_name, true)
                            == Some(true)
                    {
                        use crate::types::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };

                        let class_name = self.get_class_name_from_decl(class_idx);
                        let static_member_name = format!("{}.{}", class_name, property_name);
                        let object_type_str = self.format_type(original_object_type);
                        let message = format_message(
                            diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                            &[property_name, &object_type_str, &static_member_name],
                        );
                        self.error_at_node(
                            idx,
                            &message,
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_INSTEAD,
                        );
                        return TypeId::ERROR;
                    }

                    // Don't emit TS2339 for private fields (starting with #) - they're handled elsewhere
                    if !property_name.starts_with('#') {
                        // Property access expressions are VALUE context - always emit TS2339.
                        // TS2694 (namespace has no exported member) is for TYPE context only,
                        // which is handled separately in type name resolution.
                        // Use original_object_type to preserve nominal identity (e.g., D<string>)
                        self.error_property_not_exist_at(property_name, original_object_type, idx);
                    }
                    TypeId::ERROR
                }

                PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type,
                    cause,
                } => {
                    // Check for optional chaining (?.)
                    if access.question_dot_token {
                        // Suppress error, return (property_type | undefined)
                        let base_type = property_type.unwrap_or(TypeId::UNKNOWN);
                        return factory.union(vec![base_type, TypeId::UNDEFINED]);
                    }

                    // Report error based on the cause (TS2531/TS2532/TS2533 or TS18050)
                    // TS18050 is for definitely-nullish values in strict mode
                    // TS2531/2532/2533 are for possibly-nullish values in strict mode
                    use crate::types::diagnostics::diagnostic_codes;

                    // Suppress cascade errors when cause is ERROR/ANY/UNKNOWN
                    if cause == TypeId::ERROR || cause == TypeId::ANY || cause == TypeId::UNKNOWN {
                        return property_type.unwrap_or(TypeId::ERROR);
                    }

                    // Check if the type is entirely nullish (no non-nullish part in union)
                    let is_type_nullish = object_type_for_access == TypeId::NULL
                        || object_type_for_access == TypeId::UNDEFINED;

                    // For possibly-nullish values in non-strict mode, don't error
                    // But for definitely-nullish values in non-strict mode, fall through to error reporting below
                    if !self.ctx.compiler_options.strict_null_checks && !is_type_nullish {
                        return self
                            .apply_flow_narrowing(idx, property_type.unwrap_or(TypeId::ERROR));
                    }
                    // Check if the expression is a literal null/undefined keyword (not a variable)
                    // TS18050 is only for `null.foo` and `undefined.bar`, not `x.foo` where x: null
                    // TS18050 is emitted even without strictNullChecks, so check first
                    let is_literal_nullish =
                        if let Some(expr_node) = self.ctx.arena.get(access.expression) {
                            expr_node.kind == SyntaxKind::NullKeyword as u16
                                || (expr_node.kind == SyntaxKind::Identifier as u16
                                    && self
                                        .ctx
                                        .arena
                                        .get_identifier(expr_node)
                                        .is_some_and(|ident| ident.escaped_text == "undefined"))
                        } else {
                            false
                        };

                    // When the expression IS a literal null/undefined keyword (e.g., null.foo or undefined.bar),
                    // emit TS18050 "The value 'X' cannot be used here."
                    if is_literal_nullish {
                        let value_name = if cause == TypeId::NULL {
                            "null"
                        } else if cause == TypeId::UNDEFINED {
                            "undefined"
                        } else {
                            "null | undefined"
                        };
                        self.error_at_node_msg(
                            access.expression,
                            diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
                            &[value_name],
                        );
                        return self
                            .apply_flow_narrowing(idx, property_type.unwrap_or(TypeId::ERROR));
                    }

                    // Without strictNullChecks, null/undefined are in every type's domain,
                    // so TS18047/TS18048/TS18049 are never emitted (matches tsc behavior).
                    // Note: TS18050 for literal null/undefined is handled above.
                    if !self.ctx.compiler_options.strict_null_checks {
                        return self
                            .apply_flow_narrowing(idx, property_type.unwrap_or(TypeId::ERROR));
                    }

                    // Try to get the name if the expression is an identifier
                    // Use specific error codes (TS18047/18048/18049) when name is available
                    let name = self
                        .ctx
                        .arena
                        .get(access.expression)
                        .and_then(|node| self.ctx.arena.get_identifier(node))
                        .map(|ident| ident.escaped_text.clone());

                    let (code, message): (u32, String) = if let Some(ref name) = name {
                        // Use specific error codes with the variable name
                        if cause == TypeId::NULL {
                            (
                                diagnostic_codes::IS_POSSIBLY_NULL,
                                format!("'{}' is possibly 'null'.", name),
                            )
                        } else if cause == TypeId::UNDEFINED {
                            (
                                diagnostic_codes::IS_POSSIBLY_UNDEFINED,
                                format!("'{}' is possibly 'undefined'.", name),
                            )
                        } else {
                            (
                                diagnostic_codes::IS_POSSIBLY_NULL_OR_UNDEFINED,
                                format!("'{}' is possibly 'null' or 'undefined'.", name),
                            )
                        }
                    } else {
                        // Fall back to generic error codes
                        if cause == TypeId::NULL {
                            (
                                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL,
                                "Object is possibly 'null'.".to_string(),
                            )
                        } else if cause == TypeId::UNDEFINED {
                            (
                                diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED,
                                "Object is possibly 'undefined'.".to_string(),
                            )
                        } else {
                            (
                                diagnostic_codes::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED,
                                "Object is possibly 'null' or 'undefined'.".to_string(),
                            )
                        }
                    };

                    // Report the error on the expression part
                    self.error_at_node(access.expression, &message, code);

                    // Error recovery: return the property type found in valid members
                    self.apply_flow_narrowing(idx, property_type.unwrap_or(TypeId::ERROR))
                }

                PropertyAccessResult::IsUnknown => {
                    // TS2339: Property does not exist on type 'unknown'
                    // Use the same error as TypeScript for property access on unknown
                    self.error_property_not_exist_at(property_name, object_type_for_access, idx);
                    TypeId::ERROR
                }
            }
        } else {
            TypeId::ANY
        }
    }

    fn resolve_array_global_augmentation_property(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        use rustc_hash::FxHashMap;
        use std::sync::Arc;
        use tsz_parser::parser::NodeArena;
        use tsz_parser::parser::node::NodeAccess;
        use tsz_solver::operations_property::PropertyAccessResult;
        use tsz_solver::type_queries::{
            get_array_element_type, get_tuple_elements, get_type_application, unwrap_readonly,
        };
        use tsz_solver::{TypeLowering, types::is_compiler_managed_type};

        let base_type = unwrap_readonly(self.ctx.types, object_type);

        let element_type = if let Some(elem) = get_array_element_type(self.ctx.types, base_type) {
            Some(elem)
        } else if let Some(elems) = get_tuple_elements(self.ctx.types, base_type) {
            let mut members = Vec::new();
            for elem in elems {
                let mut ty = if elem.rest {
                    get_array_element_type(self.ctx.types, elem.type_id).unwrap_or(elem.type_id)
                } else {
                    elem.type_id
                };
                if elem.optional {
                    ty = self.ctx.types.factory().union(vec![ty, TypeId::UNDEFINED]);
                }
                members.push(ty);
            }
            Some(self.ctx.types.factory().union(members))
        } else if let Some(app) = get_type_application(self.ctx.types, base_type) {
            app.args.first().copied()
        } else {
            None
        }?;

        let augmentation_decls = self.ctx.binder.global_augmentations.get("Array")?;
        if augmentation_decls.is_empty() {
            return None;
        }

        let all_arenas = self.ctx.all_arenas.clone();
        let all_binders = self.ctx.all_binders.clone();
        let lib_contexts = self.ctx.lib_contexts.clone();
        let binder_for_arena = |arena_ref: &NodeArena| -> Option<&tsz_binder::BinderState> {
            let arenas = all_arenas.as_ref()?;
            let binders = all_binders.as_ref()?;
            let arena_ptr = arena_ref as *const NodeArena;
            for (idx, arena) in arenas.iter().enumerate() {
                if Arc::as_ptr(arena) == arena_ptr {
                    return binders.get(idx).map(Arc::as_ref);
                }
            }
            None
        };

        let resolve_in_scope = |binder: &tsz_binder::BinderState,
                                arena_ref: &NodeArena,
                                node_idx: NodeIndex|
         -> Option<u32> {
            let ident_name = arena_ref.get_identifier_text(node_idx)?;
            let mut scope_id = binder.find_enclosing_scope(arena_ref, node_idx)?;
            while scope_id != tsz_binder::ScopeId::NONE {
                let scope = binder.scopes.get(scope_id.0 as usize)?;
                if let Some(sym_id) = scope.table.get(ident_name) {
                    return Some(sym_id.0);
                }
                scope_id = scope.parent;
            }
            None
        };

        let mut cross_file_groups: FxHashMap<usize, (Arc<NodeArena>, Vec<NodeIndex>)> =
            FxHashMap::default();
        for aug in augmentation_decls {
            if let Some(ref arena) = aug.arena {
                let key = Arc::as_ptr(arena) as usize;
                cross_file_groups
                    .entry(key)
                    .or_insert_with(|| (Arc::clone(arena), Vec::new()))
                    .1
                    .push(aug.node);
            } else {
                let key = self.ctx.arena as *const NodeArena as usize;
                cross_file_groups
                    .entry(key)
                    .or_insert_with(|| (Arc::new(self.ctx.arena.clone()), Vec::new()))
                    .1
                    .push(aug.node);
            }
        }

        let mut found_types = Vec::new();
        for (_, (arena, decls)) in cross_file_groups {
            let decl_binder = binder_for_arena(arena.as_ref()).unwrap_or(self.ctx.binder);
            let resolver = |node_idx: NodeIndex| -> Option<u32> {
                if let Some(sym_id) = decl_binder.get_node_symbol(node_idx) {
                    return Some(sym_id.0);
                }
                if let Some(sym_id) = resolve_in_scope(decl_binder, arena.as_ref(), node_idx) {
                    return Some(sym_id);
                }
                let ident_name = arena.as_ref().get_identifier_text(node_idx)?;
                if is_compiler_managed_type(ident_name) {
                    return None;
                }
                if let Some(found_sym) = decl_binder.file_locals.get(ident_name) {
                    return Some(found_sym.0);
                }
                if let Some(all_binders) = all_binders.as_ref() {
                    for binder in all_binders.iter() {
                        if let Some(found_sym) = binder.file_locals.get(ident_name) {
                            return Some(found_sym.0);
                        }
                    }
                }
                for ctx in &lib_contexts {
                    if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
                        return Some(found_sym.0);
                    }
                }
                None
            };
            let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
                if let Some(sym_id) = decl_binder.get_node_symbol(node_idx) {
                    return Some(
                        self.ctx
                            .get_or_create_def_id(tsz_binder::SymbolId(sym_id.0)),
                    );
                }
                if let Some(sym_id) = resolve_in_scope(decl_binder, arena.as_ref(), node_idx) {
                    return Some(self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id)));
                }
                let ident_name = arena.as_ref().get_identifier_text(node_idx)?;
                if is_compiler_managed_type(ident_name) {
                    return None;
                }
                let sym_id = decl_binder.file_locals.get(ident_name).or_else(|| {
                    if let Some(all_binders) = all_binders.as_ref() {
                        for binder in all_binders.iter() {
                            if let Some(found_sym) = binder.file_locals.get(ident_name) {
                                return Some(found_sym);
                            }
                        }
                    }
                    lib_contexts
                        .iter()
                        .find_map(|ctx| ctx.binder.file_locals.get(ident_name))
                })?;
                Some(
                    self.ctx
                        .get_or_create_def_id(tsz_binder::SymbolId(sym_id.0)),
                )
            };

            let decls_with_arenas: Vec<(NodeIndex, &NodeArena)> = decls
                .iter()
                .map(|&decl_idx| (decl_idx, arena.as_ref()))
                .collect();
            let lowering = TypeLowering::with_hybrid_resolver(
                arena.as_ref(),
                self.ctx.types,
                &resolver,
                &def_id_resolver,
                &|_| None,
            );
            let (aug_type, params) =
                lowering.lower_merged_interface_declarations(&decls_with_arenas);
            if aug_type == TypeId::ERROR {
                continue;
            }

            if let PropertyAccessResult::Success { type_id, .. } =
                self.resolve_property_access_with_env(aug_type, property_name)
            {
                found_types.push(type_id);
                continue;
            }

            if !params.is_empty() {
                let mut args = Vec::with_capacity(params.len());
                args.push(element_type);
                for _ in 1..params.len() {
                    args.push(TypeId::ANY);
                }
                let app_type = self.ctx.types.factory().application(aug_type, args);
                if let PropertyAccessResult::Success { type_id, .. } =
                    self.resolve_property_access_with_env(app_type, property_name)
                {
                    found_types.push(type_id);
                }
            }
        }

        if found_types.is_empty() {
            None
        } else if found_types.len() == 1 {
            Some(found_types[0])
        } else {
            Some(self.ctx.types.factory().union(found_types))
        }
    }

    /// Check if a property access is an expando function assignment pattern.
    ///
    /// TypeScript allows assigning properties to function and class declarations:
    /// ```typescript
    /// function foo() {}
    /// foo.bar = 1;  // OK - expando pattern, no TS2339
    /// ```
    ///
    /// Returns true if:
    /// 1. The property access is the LHS of a `=` assignment
    /// 2. The object expression is an identifier bound to a function or class declaration
    /// 3. The object type is a function type
    fn is_expando_function_assignment(
        &self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
        object_type: TypeId,
    ) -> bool {
        use tsz_solver::visitor::is_function_type;

        // Check if object type is a function type
        if !is_function_type(self.ctx.types, object_type) {
            return false;
        }

        // Check if property access is LHS of a `=` assignment
        let parent_idx = match self.ctx.arena.get_extended(property_access_idx) {
            Some(ext) if !ext.parent.is_none() => ext.parent,
            _ => return false,
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
            return false;
        };
        if binary.operator_token != SyntaxKind::EqualsToken as u16
            || binary.left != property_access_idx
        {
            return false;
        }

        // Check if the object expression is an identifier bound to a function/class declaration
        let Some(expr_node) = self.ctx.arena.get(object_expr_idx) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(expr_node) else {
            return false;
        };

        // Look up the symbol - try file_locals first, then full scope resolution
        let sym_id = self
            .ctx
            .binder
            .file_locals
            .get(&ident.escaped_text)
            .or_else(|| self.resolve_identifier_symbol(object_expr_idx));

        if let Some(sym_id) = sym_id {
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                return (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) != 0;
            }
        }

        false
    }

    fn strict_bind_call_apply_method_type(
        &self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        if property_name != "apply" {
            return None;
        }

        let factory = self.ctx.types.factory();
        use tsz_solver::type_queries::{get_callable_shape, get_function_shape};

        let (params, return_type) =
            if let Some(shape) = get_function_shape(self.ctx.types, object_type) {
                (shape.params.clone(), shape.return_type)
            } else if let Some(shape) = get_callable_shape(self.ctx.types, object_type) {
                let sig = shape.call_signatures.first()?;
                (sig.params.clone(), sig.return_type)
            } else {
                return None;
            };

        let tuple_elements: Vec<tsz_solver::TupleElement> = params
            .iter()
            .map(|param| tsz_solver::TupleElement {
                type_id: param.type_id,
                name: param.name,
                optional: param.optional,
                rest: param.rest,
            })
            .collect();
        let args_tuple = factory.tuple(tuple_elements);

        let method_shape = tsz_solver::FunctionShape {
            params: vec![
                tsz_solver::ParamInfo {
                    name: Some(self.ctx.types.intern_string("thisArg")),
                    type_id: TypeId::ANY,
                    optional: false,
                    rest: false,
                },
                tsz_solver::ParamInfo {
                    name: Some(self.ctx.types.intern_string("args")),
                    type_id: args_tuple,
                    optional: true,
                    rest: false,
                },
            ],
            this_type: None,
            return_type,
            type_params: vec![],
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };

        Some(factory.function(method_shape))
    }
}
