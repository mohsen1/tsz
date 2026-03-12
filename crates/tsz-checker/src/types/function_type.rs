//! Function, method, and arrow function type resolution.
use crate::computation::complex::{
    expression_needs_contextual_return_type, is_contextually_sensitive,
};
use crate::diagnostics::format_message;
use crate::query_boundaries::type_checking_utilities as type_query;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{ContextualTypeContext, TypeId, TypeParamInfo};
impl<'a> CheckerState<'a> {
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
        if !is_function_declaration && !is_method_or_constructor {
            // Check for required parameters following optional parameters (TS1016)
            self.check_parameter_ordering(parameters, Some(idx));
            self.check_binding_pattern_optionality(&parameters.nodes, body.is_some());
            // Check that rest parameters have array types (TS2370)
            self.check_rest_parameter_types(&parameters.nodes);
            self.check_strict_mode_reserved_parameter_names(
                &parameters.nodes,
                idx,
                self.ctx.enclosing_class.is_some(),
            );
        }

        // TS1100: `eval` or `arguments` used as a function expression name in strict mode.
        if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            && let Some(name_idx) = name_node
            && let Some(name_n) = self.ctx.arena.get(name_idx)
            && let Some(ident) = self.ctx.arena.get_identifier(name_n)
        {
            let name = &ident.escaped_text;
            if self.is_strict_mode_for_node(name_idx)
                && crate::state_checking::is_eval_or_arguments(name)
                && !(self.ctx.enclosing_class.is_some() && name.as_str() == "arguments")
            {
                self.emit_eval_or_arguments_strict_mode_error(name_idx, name);
            }
        }

        // Push enclosing type parameters so nested functions can reference outer generic scopes.
        let enclosing_type_param_updates = self.push_enclosing_type_parameters(idx);

        let (mut type_params, type_param_updates) = self.push_type_parameters(type_parameters);

        // Check for unused type parameters (TS6133) in function expressions and arrows
        if !is_function_declaration && !is_method_or_constructor {
            self.check_unused_type_params(type_parameters, idx);
        }

        // Collect parameter info using solver's ParamInfo struct
        let mut params = Vec::new();
        let mut param_types: Vec<Option<TypeId>> = Vec::new();
        let mut destructuring_context_param_types: Vec<Option<TypeId>> = Vec::new();
        let mut this_type = None;
        let this_atom = self.ctx.types.intern_string("this");
        let closure_already_checked =
            is_closure && self.ctx.implicit_any_checked_closures.contains(&idx);
        // Setup contextual typing context, evaluating compound types first.
        let mut contextual_signature_type_params = None;
        let mut has_jsdoc_type_function = false;
        let mut ctx_helper = if let Some(ctx_type) = self.ctx.contextual_type {
            tracing::debug!(
                "function_type: contextual_type = {:?}, is_closure = {}",
                ctx_type,
                is_closure
            );
            use tsz_solver::type_queries::{
                EvaluationNeeded, classify_for_evaluation, get_lazy_def_id, get_type_application,
            };

            let evaluated_type = if get_type_application(self.ctx.types, ctx_type).is_some() {
                self.evaluate_application_type(ctx_type)
            } else if get_lazy_def_id(self.ctx.types, ctx_type).is_some()
                || matches!(
                    classify_for_evaluation(self.ctx.types, ctx_type),
                    EvaluationNeeded::IndexAccess { .. } | EvaluationNeeded::KeyOf(..)
                )
            {
                self.judge_evaluate(ctx_type)
            } else {
                self.evaluate_contextual_type(ctx_type)
            };
            // Preserve original when evaluation degrades to UNKNOWN (unresolved conditionals)
            let evaluated_type = if evaluated_type == TypeId::UNKNOWN {
                ctx_type
            } else {
                evaluated_type
            };

            // Evaluate Application types in rest params (solver's NoopResolver can't resolve these)
            let evaluated_type = self.evaluate_contextual_rest_param_applications(evaluated_type);
            let evaluated_type = self.normalize_contextual_signature_with_env(evaluated_type);

            contextual_signature_type_params =
                self.contextual_type_params_from_expected(evaluated_type);
            Some(ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                evaluated_type,
                self.ctx.compiler_options.no_implicit_any,
            ))
        } else if self.is_js_file() && is_function_declaration {
            // For function declarations in JS files with @type {FunctionType},
            // use the function type as contextual type for parameter typing.
            if let Some(evaluated_type) = self.jsdoc_callable_type_annotation_for_function(idx) {
                contextual_signature_type_params =
                    self.contextual_type_params_from_expected(evaluated_type);
                has_jsdoc_type_function = true;
                Some(ContextualTypeContext::with_expected_and_options(
                    self.ctx.types,
                    evaluated_type,
                    self.ctx.compiler_options.no_implicit_any,
                ))
            } else {
                None
            }
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

        if self.is_js_file() && is_function_declaration && !has_jsdoc_type_function {
            if let Some(evaluated_type) = self.jsdoc_callable_type_annotation_for_function(idx) {
                has_jsdoc_type_function = true;
                ctx_helper = Some(ContextualTypeContext::with_expected_and_options(
                    self.ctx.types,
                    evaluated_type,
                    self.ctx.compiler_options.no_implicit_any,
                ));
            } else if func_jsdoc
                .as_ref()
                .is_some_and(|jsdoc| self.jsdoc_type_tag_references_callback_typedef(idx, jsdoc))
            {
                has_jsdoc_type_function = true;
            }
        }

        // In JS/checkJs, support minimal generic JSDoc function typing:
        //   @template T
        //   @returns {T}
        // This enables return assignability checks for expression-bodied arrows.
        let mut jsdoc_type_param_updates: Vec<(String, Option<TypeId>, bool)> = Vec::new();
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
                    jsdoc_type_param_updates.push((name, previous, false));
                }
                type_params = jsdoc_type_params;
            }
        }
        let jsdoc_return_context = func_jsdoc
            .as_ref()
            .and_then(|j| Self::jsdoc_returns_type_name(j))
            .and_then(|name| jsdoc_type_param_types.get(&name).copied());

        // Check if this closure is inside a decorator expression.
        // Decorator arrow functions like `@((t, c) => {})` should not emit TS7006
        // because tsc provides contextual types for decorator parameters, which we
        // don't yet implement. Walking up the parent chain to find a DECORATOR node.
        let is_in_decorator = is_closure && {
            let mut current = idx;
            let mut found = false;
            // Walk up at most 3 levels: arrow -> paren -> decorator (for `@((t, c) => {})`)
            for _ in 0..3 {
                if let Some(ext) = self.ctx.arena.get_extended(current) {
                    let parent = ext.parent;
                    if parent.is_none() {
                        break;
                    }
                    if let Some(parent_node) = self.ctx.arena.get(parent) {
                        if parent_node.kind == syntax_kind_ext::DECORATOR {
                            found = true;
                            break;
                        }
                        current = parent;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            found
        };

        // Pre-extract ordered @param names for positional matching with binding patterns.
        let jsdoc_param_names: Vec<String> = func_jsdoc
            .as_ref()
            .map(|jsdoc| {
                Self::extract_jsdoc_param_names(jsdoc)
                    .into_iter()
                    .map(|(name, _)| name)
                    .collect()
            })
            .unwrap_or_default();

        let mut contextual_index = 0;
        for &param_idx in &parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
            {
                // Get parameter name
                let name = if let Some(name_node) = self.ctx.arena.get(param.name) {
                    if let Some(name_data) = self.ctx.arena.get_identifier(name_node) {
                        Some(self.ctx.types.intern_string(&name_data.escaped_text))
                    } else if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    {
                        self.binding_name_for_signature_display(param.name)
                    } else {
                        None
                    }
                } else {
                    None
                };
                let is_this_param = name == Some(this_atom);
                let is_js_file = self.is_js_file();
                let contextual_type = if let Some(ref helper) = ctx_helper {
                    let expected_contextual_type = helper.expected().and_then(|expected| {
                        self.contextual_parameter_type_with_env_from_expected(
                            expected,
                            contextual_index,
                            param.dot_dot_dot_token,
                        )
                    });
                    let direct = if param.dot_dot_dot_token {
                        // Rest parameter: get the full tuple/array type from context,
                        // not just the element at this position.
                        helper.get_rest_parameter_type(contextual_index)
                    } else {
                        helper.get_parameter_type(contextual_index)
                    };

                    if let Some(extracted) = direct {
                        if let Some(from_expected) = expected_contextual_type {
                            let direct_is_placeholderish = extracted == TypeId::ANY
                                || extracted == TypeId::UNKNOWN
                                || tsz_solver::type_queries::contains_infer_types_db(
                                    self.ctx.types,
                                    extracted,
                                );
                            let expected_is_more_informative = from_expected != TypeId::ANY
                                && from_expected != TypeId::UNKNOWN
                                && !tsz_solver::type_queries::contains_infer_types_db(
                                    self.ctx.types,
                                    from_expected,
                                );
                            let direct_is_strict_subtype = extracted != from_expected
                                && self.is_subtype_of(extracted, from_expected)
                                && !self.is_subtype_of(from_expected, extracted);
                            if (direct_is_placeholderish && expected_is_more_informative)
                                || direct_is_strict_subtype
                            {
                                Some(from_expected)
                            } else {
                                let resolved = self.resolve_type_query_type(extracted);
                                let evaluated = self.evaluate_type_with_env(resolved);
                                if evaluated != extracted {
                                    expected_contextual_type.or(Some(extracted))
                                } else {
                                    Some(extracted)
                                }
                            }
                        } else {
                            let resolved = self.resolve_type_query_type(extracted);
                            let evaluated = self.evaluate_type_with_env(resolved);
                            if evaluated != extracted {
                                expected_contextual_type.or(Some(extracted))
                            } else {
                                Some(extracted)
                            }
                        }
                    } else {
                        expected_contextual_type
                    }
                } else {
                    None
                };
                let has_unknown_expected_context = ctx_helper
                    .as_ref()
                    .and_then(tsz_solver::ContextualTypeContext::expected)
                    .is_some_and(|t| t == TypeId::UNKNOWN);
                // TS7006: In TS files, contextual `unknown` is still a concrete contextual
                // type and should suppress implicit-any reporting for callback parameters.
                // Keep the old JS behavior where weak contextual `unknown` is treated as no context.
                // Rest parameters (`...x`) are always contextually typed when a contextual
                // type helper exists — even if the contextual function has fewer parameters,
                // the rest param captures the "remaining" args (type `[]` for 0-param context).
                let has_contextual_type = contextual_type
                    .is_some_and(|t| t != TypeId::UNKNOWN || !is_js_file)
                    || (has_unknown_expected_context && !is_js_file)
                    || (param.dot_dot_dot_token && ctx_helper.is_some());
                if is_closure && has_contextual_type {
                    self.ctx.implicit_any_contextual_closures.insert(idx);
                }
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
                    // In JS files with JSDoc, @param {Type} annotations provide explicit
                    // parameter types that take priority over contextual types.
                    // This is how tsc handles JS files: @param types are the primary
                    // source of parameter type information.
                    let jsdoc_param_type = if is_js_file {
                        if let Some(comment_start) = self.get_jsdoc_comment_pos_for_function(idx) {
                            if let Some(ref jsdoc) = func_jsdoc {
                                // Use positional matching for binding patterns
                                let pname = self.effective_jsdoc_param_name(
                                    param.name,
                                    &jsdoc_param_names,
                                    contextual_index,
                                );
                                self.resolve_jsdoc_param_type_with_pos(
                                    jsdoc,
                                    &pname,
                                    Some(comment_start),
                                )
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let iife_arg_type = if contextual_type.is_none() {
                        self.infer_iife_parameter_type_from_arguments(
                            idx,
                            contextual_index,
                            param.dot_dot_dot_token,
                            param.question_token || param.initializer.is_some(),
                        )
                    } else {
                        None
                    };
                    let inferred_type = if let Some(jsdoc_type) = jsdoc_param_type {
                        jsdoc_type
                    } else if is_js_file {
                        contextual_type
                            .filter(|t| *t != TypeId::UNKNOWN)
                            .or(iife_arg_type)
                            .unwrap_or(TypeId::ANY)
                    } else {
                        contextual_type.or(iife_arg_type).unwrap_or(TypeId::ANY)
                    };
                    // JSDoc @param [name] bracket-optional without explicit type → T | undefined
                    let inferred_type = if is_js_file
                        && jsdoc_param_type.is_none()
                        && self.ctx.strict_null_checks()
                        && inferred_type != TypeId::ANY
                        && inferred_type != TypeId::UNDEFINED
                    {
                        let pname = self.effective_jsdoc_param_name(
                            param.name,
                            &jsdoc_param_names,
                            contextual_index,
                        );
                        if let Some(ref jsdoc) = func_jsdoc
                            && Self::is_jsdoc_param_optional_by_brackets(jsdoc, &pname)
                        {
                            self.ctx
                                .types
                                .factory()
                                .union(vec![inferred_type, TypeId::UNDEFINED])
                        } else {
                            inferred_type
                        }
                    } else {
                        inferred_type
                    };
                    if inferred_type == TypeId::ANY && param.initializer.is_some() {
                        let init_type = self.get_type_of_node(param.initializer);
                        // Only widen when the initializer is a "fresh" literal expression
                        let is_enum_member = self.is_enum_member_type_for_widening(init_type);
                        if is_enum_member || self.is_fresh_literal_expression(param.initializer) {
                            self.widen_initializer_type_for_mutable_binding(init_type)
                        } else {
                            init_type
                        }
                    } else {
                        inferred_type
                    }
                };
                let mut element_type_from_pattern = None;
                if let Some(name_node) = self.ctx.arena.get(param.name)
                    && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                {
                    let pattern_type = self.infer_type_from_binding_pattern(param.name, type_id);
                    if pattern_type != TypeId::ANY {
                        element_type_from_pattern = Some(pattern_type);
                    }
                }
                let binding_context_type = type_id;
                let mut type_id = if let Some(pattern_type) = element_type_from_pattern {
                    if param.type_annotation.is_some() {
                        type_id
                    } else {
                        pattern_type
                    }
                } else {
                    type_id
                };
                if is_this_param {
                    if this_type.is_none() {
                        this_type = Some(type_id);
                    }
                    param_types.push(None);
                    destructuring_context_param_types.push(None);
                    continue;
                }
                // TS7006: Check for implicit any. Skip closures during build_type_environment
                // (no contextual type yet). JSDoc @param/@type annotations suppress TS7006.
                let has_jsdoc_param = if !has_contextual_type && param.type_annotation.is_none() {
                    let from_func_jsdoc = if let Some(ref jsdoc) = func_jsdoc {
                        let pname = self.effective_jsdoc_param_name(
                            param.name,
                            &jsdoc_param_names,
                            contextual_index,
                        );
                        let has_callable_jsdoc_type = has_jsdoc_type_function
                            || (is_js_file
                                && is_function_declaration
                                && self
                                    .jsdoc_callable_type_annotation_for_function(idx)
                                    .is_some())
                            || self
                                .extract_type_predicate_from_jsdoc_type_tag(jsdoc)
                                .is_some()
                            || self.jsdoc_type_tag_references_callback_typedef(idx, jsdoc);
                        Self::jsdoc_has_param_type(jsdoc, &pname)
                            || has_callable_jsdoc_type
                            || Self::jsdoc_type_tag_declares_callable(jsdoc)
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
                // Skip TS7006 for setters (handled by caller), closures during
                // build_type_environment (no contextual type), decorator closures,
                // re-entrant closure resolution (first call handles diagnostics),
                // and ambient declarations (declare class/module private members).
                let is_setter = node.kind == syntax_kind_ext::SET_ACCESSOR;
                // In ambient contexts (declare class, .d.ts), tsc suppresses
                // TS7006/TS7031 for private members since they're excluded from
                // .d.ts output. check_method_declaration in ambient_signature_checks.rs
                // handles this for the method-checking path, but get_type_of_function
                // also processes these methods and must skip as well.
                let is_ambient_private = self.ctx.is_ambient_declaration(idx)
                    && self
                        .ctx
                        .enclosing_class
                        .as_ref()
                        .is_some_and(|c| c.is_declared);
                let skip_implicit_any = is_setter
                    || (is_closure && !self.ctx.is_checking_statements && !has_contextual_type)
                    || (is_in_decorator && !has_contextual_type)
                    || closure_already_checked
                    || is_ambient_private;
                if !skip_implicit_any {
                    self.maybe_report_implicit_any_parameter(
                        param,
                        has_contextual_type || has_jsdoc_param,
                        contextual_index,
                    );
                }
                // In JS files, params without type annotations are implicitly optional
                // unless a JSDoc @param tag or @type function annotation exists.
                let js_implicit_optional = self.is_js_file()
                    && !has_jsdoc_type_function
                    && param.type_annotation.is_none()
                    && !{
                        let jsdoc_for_opt = func_jsdoc
                            .as_ref()
                            .cloned()
                            .or_else(|| self.find_jsdoc_for_function(idx));
                        jsdoc_for_opt.is_some_and(|jsdoc| {
                            let pname = self.effective_jsdoc_param_name(
                                param.name,
                                &jsdoc_param_names,
                                contextual_index,
                            );
                            Self::jsdoc_has_required_param_tag(&jsdoc, &pname)
                        })
                    };
                let optional =
                    param.question_token || param.initializer.is_some() || js_implicit_optional;
                let rest = param.dot_dot_dot_token
                    || (self.is_js_file()
                        && func_jsdoc.as_ref().is_some_and(|jsdoc| {
                            let pname = self.effective_jsdoc_param_name(
                                param.name,
                                &jsdoc_param_names,
                                contextual_index,
                            );
                            Self::jsdoc_param_is_rest(jsdoc, &pname)
                        }));
                if rest
                    && matches!(
                        self.ctx.types.lookup(type_id),
                        Some(
                            tsz_solver::TypeData::Application(_)
                                | tsz_solver::TypeData::Mapped(_)
                                | tsz_solver::TypeData::Conditional(_)
                                | tsz_solver::TypeData::Intersection(_)
                        )
                    )
                {
                    let evaluated = if matches!(
                        self.ctx.types.lookup(type_id),
                        Some(tsz_solver::TypeData::Application(_))
                    ) {
                        self.evaluate_application_type(type_id)
                    } else {
                        self.evaluate_type_with_env(type_id)
                    };
                    let array_like = matches!(
                        type_query::classify_array_like(self.ctx.types, evaluated),
                        type_query::ArrayLikeKind::Array(_)
                            | type_query::ArrayLikeKind::Tuple
                            | type_query::ArrayLikeKind::Readonly(_)
                    );
                    let no_infer = matches!(
                        self.ctx.types.lookup(evaluated),
                        Some(tsz_solver::TypeData::NoInfer(_))
                    );
                    if array_like || no_infer {
                        type_id = evaluated;
                    }
                }
                // Body type includes `| undefined` for optional params;
                // ParamInfo.type_id uses declared type (no `| undefined`)
                // to match tsc error messages. Solver handles optionality
                // via the `optional` flag in call_args.
                let body_effective_type = if param.question_token
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
                let effective_binding_context_type = if param.question_token
                    && self.ctx.strict_null_checks()
                    && binding_context_type != TypeId::ANY
                    && binding_context_type != TypeId::ERROR
                    && binding_context_type != TypeId::UNDEFINED
                {
                    self.ctx
                        .types
                        .factory()
                        .union(vec![binding_context_type, TypeId::UNDEFINED])
                } else {
                    binding_context_type
                };
                params.push(ParamInfo {
                    name,
                    type_id,
                    optional,
                    rest,
                });
                param_types.push(Some(body_effective_type));
                destructuring_context_param_types.push(Some(effective_binding_context_type));
                contextual_index += 1;
            }
        }

        // Record that we've checked this closure for implicit-any diagnostics so
        // later re-entrant passes do not re-emit TS7006/TS7031. Do this after the
        // full parameter walk so sibling parameters in the same closure are all checked.
        if is_closure && !closure_already_checked {
            self.ctx.implicit_any_checked_closures.insert(idx);
        }

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors, not in regular functions
        self.check_parameter_properties(&parameters.nodes);

        // Get return type from annotation or infer
        let has_type_annotation = type_annotation.is_some();
        let (mut return_type, mut type_predicate) = if has_type_annotation {
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

        // Check JSDoc @returns for type predicates (e.g., @returns {x is string})
        // This covers JS files where return types are specified via JSDoc instead of syntax.
        if type_predicate.is_none()
            && let Some(predicate) = self.extract_jsdoc_return_type_predicate(&func_jsdoc, &params)
        {
            let is_asserts = predicate.asserts;
            return_type = if is_asserts {
                TypeId::VOID
            } else {
                TypeId::BOOLEAN
            };
            type_predicate = Some(predicate);
        }

        // Check JSDoc @type {CallbackType} for type predicates (e.g., @callback with @return {x is number}).
        if type_predicate.is_none()
            && let Some(ref jsdoc) = func_jsdoc
            && let Some(pred) = self.extract_type_predicate_from_jsdoc_type_tag(jsdoc)
        {
            return_type = if pred.asserts {
                TypeId::VOID
            } else {
                TypeId::BOOLEAN
            };
            type_predicate = Some(pred);
        }

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
        let mut has_contextual_return = false;
        let mut return_context_for_circularity = None;
        let mut early_yield_type: Option<TypeId> = None;
        let mut final_generator_yield_type: Option<TypeId> = None;
        let mut early_gen_return_type: Option<TypeId> = None;
        let mut early_gen_next_type: Option<TypeId> = None;

        // Push this_type BEFORE parameter initializer checks so that default
        // values like `a = this.getNumber()` see the correct `this` type and
        // don't trigger false TS2683.
        let implicit_this = this_type.or_else(|| {
            if is_arrow_function {
                outer_this_type
            } else {
                ctx_helper.as_ref().and_then(|h| h.get_this_type())
                    .or_else(|| {
                        // Traverse up to see if we are the RHS of `obj.prop = func` or `obj.prop ??= func`
                        let mut current = idx;
                        for _ in 0..3 {
                            let parent = self.ctx.arena.get_extended(current)?.parent;
                            let parent_node = self.ctx.arena.get(parent)?;
                            if parent_node.kind == tsz_parser::parser::syntax_kind_ext::BINARY_EXPRESSION {
                                if let Some(binary) = self.ctx.arena.get_binary_expr(parent_node)
                                    && binary.right == current && self.is_assignment_operator(binary.operator_token) {
                                        let left = binary.left;
                                        if let Some(left_node) = self.ctx.arena.get(left)
                                            && (left_node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                                                || left_node.kind == tsz_parser::parser::syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                                                && let Some(access) = self.ctx.arena.get_access_expr(left_node) {
                                                    let receiver = self.get_type_of_node(access.expression);
                                                    if receiver != tsz_solver::TypeId::ERROR {
                                                        return Some(receiver);
                                                    }
                                                }
                                    }
                                break; // Only check immediate assignment parent
                            } else if parent_node.kind == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                                current = parent; // Skip parens
                                continue;
                            } else {
                                break;
                            }
                        }
                        None
                    })
            }
        });

        let mut pushed_this_type_early = false;
        if let Some(tt) = implicit_this {
            self.ctx.this_type_stack.push(tt);
            pushed_this_type_early = true;
        }

        self.check_non_impl_parameter_initializers(&parameters.nodes, false, body.is_some());
        if body.is_some() {
            // Track that we're inside a nested function for abstract property access checks.
            // This must happen before infer_return_type_from_body which evaluates body expressions.
            self.ctx.function_depth += 1;
            self.cache_parameter_types(&parameters.nodes, Some(&param_types));
            self.record_destructured_parameter_binding_groups(
                &parameters.nodes,
                &destructuring_context_param_types,
            );

            // Assign contextual types to destructuring parameters (binding patterns)
            // This allows destructuring patterns in callbacks to infer element types from contextual types
            self.assign_contextual_types_to_destructuring_params(
                &parameters.nodes,
                &destructuring_context_param_types,
            );

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
            // Also extract return and next types from contextual Generator<Y, R, N>.
            if is_generator
                && !has_type_annotation
                && let Some(gen_types) = ctx_helper.as_ref().and_then(|helper| {
                    let ret_type = helper.get_return_type()?;
                    let ret_ctx = ContextualTypeContext::with_expected(self.ctx.types, ret_type);
                    Some((
                        ret_ctx.get_generator_yield_type(),
                        ret_ctx.get_generator_return_type(),
                        ret_ctx.get_generator_next_type(),
                    ))
                })
            {
                early_yield_type = gen_types.0;
                final_generator_yield_type = gen_types.0;
                early_gen_return_type = gen_types.1;
                early_gen_next_type = gen_types.2;
            }
            if early_yield_type.is_some() {
                self.ctx.push_yield_type(early_yield_type);
            }

            if !has_type_annotation {
                let return_context = jsdoc_return_context.or_else(|| {
                    ctx_helper
                        .as_ref()
                        .and_then(tsz_solver::ContextualTypeContext::get_return_type)
                        .or_else(|| {
                            self.ctx.contextual_type.and_then(|ty| {
                                tsz_solver::type_queries::get_return_type(self.ctx.types, ty)
                            })
                        })
                });
                // Async function bodies return the awaited inner type; the function
                // type itself is Promise<inner>. Contextual return typing must
                // therefore use the inner type, not Promise<inner>.
                let return_context = if is_async && !is_generator {
                    return_context.map(|ctx_ty| self.unwrap_async_return_type_for_body(ctx_ty))
                } else {
                    return_context
                };
                return_context_for_circularity = return_context;
                // TS7010/TS7011: Only count as contextual return if it's not UNKNOWN
                // UNKNOWN is a "no type" value and shouldn't prevent implicit any errors
                has_contextual_return = return_context.is_some_and(|t| t != TypeId::UNKNOWN);
                let inferred = self.infer_return_type_from_body(idx, body, return_context);
                return_type = jsdoc_return_context.unwrap_or(inferred);

                if self.is_js_file()
                    && is_function_declaration
                    && let Some(instance_type) =
                        self.synthesize_js_constructor_instance_type(idx, TypeId::ANY, &[])
                    && let Some(union_members) =
                        tsz_solver::type_queries::get_union_members(self.ctx.types, return_type)
                    && union_members.len() == 2
                    && union_members.contains(&TypeId::UNDEFINED)
                    && union_members.iter().copied().any(|member| {
                        member != TypeId::UNDEFINED
                            && self.is_assignable_to(member, instance_type)
                            && self.is_assignable_to(instance_type, member)
                    })
                {
                    return_type = instance_type;
                }
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
            if is_closure
                && !has_type_annotation
                && !has_jsdoc_return
                && self.ctx.is_checking_statements
                && !self.contextual_return_suppresses_circularity(return_context_for_circularity)
            {
                self.record_pending_circular_return_sites(idx, body);
            }
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
                    name_for_error.clone(),
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
                    let should_emit_ts2705 = if self.is_global_promise_type(return_type) {
                        // Return type is exactly the global Promise<T> - OK
                        false
                    } else if self.is_non_promise_application_type(return_type) {
                        // Return type is an Application with a non-Promise base (e.g., MyPromise<T>).
                        // TSC requires exactly Promise<T>, not subclasses.
                        true
                    } else if return_type != TypeId::ERROR {
                        // Return type evaluated to a non-Application form (e.g., Object).
                        // Fall back to syntactic check on the annotation text. This handles
                        // type aliases like `type PromiseAlias<T> = Promise<T>` which resolve
                        // to the same Object type as Promise<T>.
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
                            // TSC uses getAwaitedTypeNoAlias(returnType) || voidType for the message.
                            // Extract the promise type argument (e.g., void from MyPromise<void>).
                            let inner_type = self
                                .promise_like_return_type_argument(return_type)
                                .unwrap_or(TypeId::VOID);
                            let type_name = self.format_type(inner_type);
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
                // Determine if this is an async function or generator
                let (is_async, is_generator) = if let Some(func) = self.ctx.arena.get_function(node)
                {
                    (func.is_async, func.asterisk_token)
                } else if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    (
                        self.has_async_modifier(&method.modifiers),
                        method.asterisk_token,
                    )
                } else {
                    (false, false)
                };
                let effective_return_type = annotated_return_type.unwrap_or(return_type);
                let mut check_return_type = self.return_type_for_implicit_return_check(
                    effective_return_type,
                    is_async,
                    is_generator,
                );
                // For async functions, if we couldn't unwrap Promise<T> (e.g. lib files not loaded),
                // fall back to the annotation syntax. If it looks like Promise<...>, suppress TS2355.
                if is_async
                    && check_return_type == effective_return_type
                    && has_type_annotation
                    && self.return_type_annotation_looks_like_promise(type_annotation)
                {
                    check_return_type = TypeId::VOID;
                }
                let requires_return = self.requires_return_value(check_return_type);
                let has_return = self.body_has_return_with_value(body);
                let falls_through = self.function_body_falls_through(body);
                if has_type_annotation
                    && requires_return
                    && falls_through
                    && check_return_type != TypeId::VOID
                {
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
                    // TSC skips TS7030 for functions returning void, any, or unions containing void/any
                    let ts7030_check_type = self.return_type_for_implicit_return_check(
                        annotated_return_type.unwrap_or(return_type),
                        is_async,
                        function_is_generator,
                    );
                    if !self.should_skip_no_implicit_return_check(
                        ts7030_check_type,
                        has_type_annotation,
                    ) {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        let error_node = if let Some(nn) = name_node { nn } else { body };
                        self.error_at_node(
                            error_node,
                            diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                            diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                        );
                    }
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
            let contextual_void_return_exception = !has_type_annotation
                && jsdoc_return_context.is_none()
                && has_contextual_return
                && return_context_for_circularity == Some(TypeId::VOID);
            let body_return_type = if is_generator && has_type_annotation {
                let original_type = annotated_return_type.unwrap_or(return_type);
                // TS2505: A generator cannot have a 'void' type annotation.
                if original_type == TypeId::VOID || return_type == TypeId::VOID {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        type_annotation,
                        "A generator cannot have a 'void' type annotation.",
                        diagnostic_codes::A_GENERATOR_CANNOT_HAVE_A_VOID_TYPE_ANNOTATION,
                    );
                    TypeId::ANY // Use ANY to suppress return statement checks
                } else {
                    // Use the pre-expansion annotated return type because
                    // evaluate_application_type() may have expanded Generator<Y,R,N>
                    // into its structural object form, which get_generator_return_type_argument
                    // can't recognise (it needs an Application type).
                    self.get_generator_return_type_argument(original_type)
                        .unwrap_or(return_type)
                }
            } else if is_async_for_context && has_type_annotation {
                // Unwrap Promise<T> to T for async function return type checking.
                // Use the pre-expansion annotated return type because
                // evaluate_application_type() may have expanded Promise<T> into its
                // structural object form, which unwrap_promise_type() can't recognise.
                let original_type = annotated_return_type.unwrap_or(return_type);
                self.unwrap_promise_type(original_type)
                    .unwrap_or(return_type)
            } else if is_async_for_context {
                // For contextually-typed async functions (no explicit annotation),
                // also unwrap Promise from the return type. For unions like
                // Promise<T> | StateMachine<T>, unwrap each Promise member to get
                // T | StateMachine<T> as the effective body return type.
                self.unwrap_async_return_type_for_body(return_type)
            } else if contextual_void_return_exception {
                // Contextual `() => void` callbacks are allowed to return values.
                // Skip statement-level return assignability and let the outer
                // function-type assignability relation handle the ergonomics.
                TypeId::ANY
            } else if has_type_annotation || has_contextual_return || jsdoc_return_context.is_some()
            {
                return_type
            } else {
                // When the return type was purely inferred from the body (no
                // annotation, no contextual type, no JSDoc @returns), push ANY
                // so that check_return_statement skips the assignability check.
                // Checking a return expression against its own inferred type is
                // circular and can produce false positives when contextual typing
                // widens inner types differently than non-contextual inference.
                TypeId::ANY
            };

            self.push_return_type(body_return_type);

            // For generator functions with explicit annotations, push the yield type
            // from the annotation. Contextually-typed generators already had their yield
            // type pushed early (before infer_return_type_from_body).
            if is_generator && has_type_annotation {
                let original_type = annotated_return_type.unwrap_or(return_type);
                let yield_type = self.get_generator_yield_type_argument(original_type);
                self.ctx.push_yield_type(yield_type);
            } else if is_generator && early_yield_type.is_none() && !has_type_annotation {
                // Unannotated generator: push None so dispatch.rs defers TS7057.
                // After body check, we'll compute the inferred yield type union
                // and emit either TS7055/TS7025 (if yield type is any) or flush
                // deferred TS7057 diagnostics.
                self.ctx.push_yield_type(None);
            } else if early_yield_type.is_none() {
                // No early push was done, push None for stack balance
                self.ctx.push_yield_type(None);
            }

            // For expression-bodied arrows/functions, check the expression against
            // the expected return type.  Use body_return_type which has already been
            // unwrapped for async (Promise<T> → T) and generators (Generator<Y,R,N> → R).
            let expected_expression_return_type = has_type_annotation
                .then_some(body_return_type)
                .or(jsdoc_return_context)
                .or(return_context_for_circularity);
            if expected_expression_return_type.is_some()
                && let Some(body_node) = self.ctx.arena.get(body)
                && body_node.kind != syntax_kind_ext::BLOCK
            {
                let raw_expected_return_type = expected_expression_return_type.unwrap();
                let expected_return_type =
                    if tsz_solver::is_index_access_type(self.ctx.types, raw_expected_return_type) {
                        let evaluated = self.evaluate_type_with_env(raw_expected_return_type);
                        if evaluated != TypeId::ERROR {
                            evaluated
                        } else {
                            raw_expected_return_type
                        }
                    } else {
                        raw_expected_return_type
                    };
                if expected_return_type != TypeId::ANY
                    && !self.type_contains_error(expected_return_type)
                {
                    // In JS/checkJs, expression-bodied arrows can carry inline JSDoc casts
                    // (e.g. `/** @type {T} */(expr)`); use that annotated type when present.
                    let mut actual_return_node = body;
                    let actual_return = self
                        .jsdoc_type_annotation_for_node_direct(actual_return_node)
                        .or_else(|| {
                            // Parenthesized expression wrappers can separate the annotation
                            // from the final body node in `.js` files (for cast-like syntax).
                            while let Some(parent_idx) = self
                                .ctx
                                .arena
                                .get_extended(actual_return_node)
                                .map(|ext| ext.parent)
                            {
                                let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                                    break;
                                };
                                if parent_node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                                    break;
                                }
                                actual_return_node = parent_idx;
                                if let Some(ty) =
                                    self.jsdoc_type_annotation_for_node_direct(actual_return_node)
                                {
                                    return Some(ty);
                                }
                            }
                            None
                        })
                        .unwrap_or_else(|| {
                            // For explicit annotations/JSDoc, type the body under that
                            // return context so literal expressions are preserved.
                            // For contextual-return-only closures, read the raw body type
                            // and let the later assignability check report on the whole
                            // expression instead of a nested contextualized subexpression.
                            let prev_ctx = self.ctx.contextual_type;
                            let can_apply_contextual_body =
                                !self.type_has_unresolved_inference_holes(expected_return_type);
                            let literal_sensitive_return =
                                tsz_solver::literal_value(self.ctx.types, expected_return_type)
                                    .is_some()
                                    || tsz_solver::type_queries::get_enum_def_id(
                                        self.ctx.types,
                                        expected_return_type,
                                    )
                                    .is_some()
                                    || (tsz_solver::type_queries::is_symbol_or_unique_symbol(
                                        self.ctx.types,
                                        expected_return_type,
                                    ) && expected_return_type != TypeId::SYMBOL)
                                    || expected_return_type == TypeId::NEVER
                                    || tsz_solver::union_list_id(
                                        self.ctx.types,
                                        expected_return_type,
                                    )
                                    .is_some_and(|list_id| {
                                        self.ctx.types.type_list(list_id).iter().any(|&member| {
                                            tsz_solver::is_literal_type(self.ctx.types, member)
                                                || tsz_solver::type_queries::get_enum_def_id(
                                                    self.ctx.types,
                                                    member,
                                                )
                                                .is_some()
                                        })
                                    });
                            let concrete_return_context = expected_return_type != TypeId::ANY
                                && expected_return_type != TypeId::UNKNOWN
                                && !tsz_solver::type_queries::contains_type_parameters_db(
                                    self.ctx.types,
                                    expected_return_type,
                                );
                            let keep_contextual_body = has_type_annotation
                                || jsdoc_return_context.is_some()
                                || literal_sensitive_return
                                || (can_apply_contextual_body
                                    && (is_contextually_sensitive(self, body)
                                        || (concrete_return_context
                                            && expression_needs_contextual_return_type(
                                                self, body,
                                            ))));
                            self.ctx.contextual_type =
                                keep_contextual_body.then_some(expected_return_type);
                            let prev_preserve_literals = self.ctx.preserve_literal_types;
                            if keep_contextual_body {
                                self.ctx.preserve_literal_types = true;
                            }
                            self.clear_type_cache_recursive(body);
                            let t = self.get_type_of_node(body);
                            self.ctx.contextual_type = prev_ctx;
                            self.ctx.preserve_literal_types = prev_preserve_literals;
                            t
                        });
                    // For async expression-bodied arrows, unwrap Promise from the
                    // actual return type, matching check_return_statement behavior.
                    // `async (): Promise<T> => p` where p is Promise<T>: the body
                    // expression type is Promise<T> but the expected type is T
                    // (already unwrapped). We must unwrap the actual type too so
                    // the assignability check compares T vs T, not Promise<T> vs T.
                    let actual_return = if is_async_for_context {
                        self.unwrap_promise_type(actual_return)
                            .unwrap_or(actual_return)
                    } else {
                        actual_return
                    };
                    let suppress_contextual_return_check = !has_type_annotation
                        && jsdoc_return_context.is_none()
                        && self.type_has_unresolved_inference_holes(expected_return_type);
                    let use_generic_return_mismatch = !has_type_annotation
                        && jsdoc_return_context.is_none()
                        && self.ctx.arena.get(body).is_some_and(|body_node| {
                            body_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                        })
                        && self.type_has_unresolved_inference_holes(expected_return_type);
                    if contextual_void_return_exception {
                        // Contextual `() => void` callbacks may return a value.
                        // Don't report a direct body-vs-void mismatch here.
                    } else if suppress_contextual_return_check {
                        // Leave callback return inference to the generic/reverse-mapped
                        // inference pass when the expected return still contains
                        // unresolved placeholders.
                    } else if use_generic_return_mismatch {
                        let conditional_branch_mismatch = self
                            .ctx
                            .arena
                            .get(body)
                            .and_then(|body_node| self.ctx.arena.get_conditional_expr(body_node))
                            .is_some_and(|cond| {
                                let prev_ctx = self.ctx.contextual_type;
                                let diag_len = self.ctx.diagnostics.len();
                                let emitted_before = self.ctx.emitted_diagnostics.clone();
                                self.ctx.contextual_type = Some(expected_return_type);
                                self.clear_type_cache_recursive(cond.when_true);
                                self.clear_type_cache_recursive(cond.when_false);
                                let mut when_true = self.get_type_of_node(cond.when_true);
                                let mut when_false = self.get_type_of_node(cond.when_false);
                                self.ctx.contextual_type = prev_ctx;
                                self.ctx.diagnostics.truncate(diag_len);
                                self.ctx.emitted_diagnostics = emitted_before;
                                if is_async_for_context {
                                    when_true =
                                        self.unwrap_promise_type(when_true).unwrap_or(when_true);
                                    when_false =
                                        self.unwrap_promise_type(when_false).unwrap_or(when_false);
                                }
                                !self.is_assignable_to(when_true, expected_return_type)
                                    || !self.is_assignable_to(when_false, expected_return_type)
                            });
                        if conditional_branch_mismatch
                            && let Some(loc) = self.get_source_location(body)
                        {
                            let src_str = self.format_type(actual_return);
                            let tgt_str = self.format_type(expected_return_type);
                            let message = format_message(
                                    crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                    &[&src_str, &tgt_str],
                                );
                            self.ctx.diagnostics.push(crate::diagnostics::Diagnostic::error(
                                    self.ctx.file_name.clone(),
                                    loc.start,
                                    loc.length(),
                                    message,
                                    crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                ));
                        }
                    } else {
                        let assignability_ok = self.check_assignable_or_report(
                            actual_return,
                            expected_return_type,
                            body,
                        );
                        if assignability_ok
                            && let Some(body_node) = self.ctx.arena.get(body)
                            && body_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                        {
                            self.check_conditional_return_branches_against_type(
                                body,
                                expected_return_type,
                                is_async_for_context,
                            );
                        }
                    }
                }
            }
            // Skip body checking for function declarations — they are checked via
            // check_function_declaration which maintains the full type param scope chain.
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
                // Propagate contextual return type for expression-bodied arrows.
                let prev_ctx_for_body = self.ctx.contextual_type;
                if let Some(body_node) = self.ctx.arena.get(body)
                    && body_node.kind != syntax_kind_ext::BLOCK
                    && !has_type_annotation
                {
                    let body_return_context = ctx_helper
                        .as_ref()
                        .and_then(tsz_solver::ContextualTypeContext::get_return_type)
                        .or_else(|| {
                            self.ctx.contextual_type.and_then(|ty| {
                                tsz_solver::type_queries::get_return_type(self.ctx.types, ty)
                            })
                        });
                    let suppress_contextual_return_for_conditional_body = jsdoc_return_context
                        .is_none()
                        && body_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                        && body_return_context.is_some_and(|return_type| {
                            self.type_has_unresolved_inference_holes(return_type)
                        });
                    if body_return_context.is_some()
                        && !suppress_contextual_return_for_conditional_body
                    {
                        self.ctx.contextual_type = body_return_context;
                    }
                }
                let suppress_expression_body_diagnostics =
                    self.ctx.arena.get(body).is_some_and(|body_node| {
                        body_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                            && !has_type_annotation
                            && jsdoc_return_context.is_none()
                            && self.ctx.contextual_type.is_some_and(|return_type| {
                                self.type_has_unresolved_inference_holes(return_type)
                            })
                    });
                let diag_len = self.ctx.diagnostics.len();
                let emitted_before = suppress_expression_body_diagnostics
                    .then(|| self.ctx.emitted_diagnostics.clone());
                // Save outer generator's yield collection state (for nested generators)
                let saved_yield_collection =
                    std::mem::take(&mut self.ctx.generator_yield_operand_types);
                self.check_statement(body);

                if suppress_expression_body_diagnostics {
                    self.ctx.diagnostics.truncate(diag_len);
                    if let Some(emitted_before) = emitted_before {
                        self.ctx.emitted_diagnostics = emitted_before;
                    }
                }

                // For annotated generator expressions, check that Generator<TYield, any, any>
                // is assignable to the declared return type.
                if is_generator && has_type_annotation {
                    let declared_type = annotated_return_type.unwrap_or(return_type);
                    let yield_t = self.ctx.current_yield_type();
                    let error_node = if type_annotation != NodeIndex::NONE {
                        type_annotation
                    } else {
                        idx
                    };
                    self.check_generator_return_type_assignability(
                        function_is_async,
                        yield_t,
                        declared_type,
                        error_node,
                    );
                }

                // For unannotated generator expressions, determine the inferred yield type
                // and emit TS7055/TS7025 if TYield is 'any'.
                // TS7055 and TS7057 are independent — TS7055 fires at function name when
                // TYield is implicit any, while TS7057 fires per-expression.
                if is_generator && !has_type_annotation && early_yield_type.is_none() {
                    let yield_types = std::mem::take(&mut self.ctx.generator_yield_operand_types);
                    // Compute inferred yield type from collected operand types
                    let inferred_yield = if yield_types.is_empty() {
                        TypeId::NEVER // No yields → never
                    } else {
                        self.ctx.types.factory().union(yield_types)
                    };
                    // Widen and check for implicit any (mirrors infer_return_type_from_body)
                    let widened = self.widen_literal_type(inferred_yield);
                    let final_yield = if !self.ctx.strict_null_checks()
                        && tsz_solver::type_queries::is_only_null_or_undefined(
                            self.ctx.types,
                            widened,
                        ) {
                        TypeId::ANY
                    } else {
                        widened
                    };
                    final_generator_yield_type = Some(final_yield);
                    if final_yield == TypeId::ANY
                        && self.ctx.no_implicit_any()
                        && !self.is_js_file()
                    {
                        use crate::diagnostics::diagnostic_codes;
                        if let Some(name) = &name_for_error {
                            // TS7055: Named generator's yield type is implicitly 'any'
                            self.error_at_node_msg(
                                name_node.unwrap_or(idx),
                                diagnostic_codes::WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_YIELD_TYPE,
                                &[name, "any"],
                            );
                        } else {
                            // TS7025: Unnamed generator expression
                            self.error_at_node_msg(
                                idx,
                                diagnostic_codes::GENERATOR_IMPLICITLY_HAS_YIELD_TYPE_CONSIDER_SUPPLYING_A_RETURN_TYPE_ANNOTATION,
                                &["any"],
                            );
                        }
                    }
                }

                // Restore outer generator's yield collection state
                self.ctx.generator_yield_operand_types = saved_yield_collection;

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
        // Some call sites compute function types before body checking has set
        // `js_body_uses_arguments` (notably function expressions in variable initializers).
        // Always pre-walk the body as a fallback so JS implicit rest parameter inference
        // remains stable across declaration/expression contexts.
        let uses_arguments =
            self.ctx.js_body_uses_arguments || self.body_has_arguments_reference(body);
        if self.is_js_file() && uses_arguments && !params.last().is_some_and(|p| p.rest) {
            params.push(ParamInfo {
                name: None,
                type_id: self.ctx.types.factory().array(TypeId::ANY),
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
            // However, we must return a Generator-like type to avoid suppressing TS2322
            // when the generator is returned or yielded to a context expecting something else.
            // Use void for TReturn: unannotated generators have no explicit return value,
            // so TReturn is void (matching tsc). This ensures generator.return() is callable
            // without arguments, since void-typed params are effectively optional.
            let gen_name = if function_is_async {
                "AsyncGenerator"
            } else {
                "Generator"
            };
            // Ensure the lib type is loaded/resolved (side effect: populates file_locals).
            let _resolved = self.resolve_lib_type_by_name(gen_name);
            // Use Lazy(DefId) as the Application base instead of the resolved TypeId.
            // resolve_lib_type_by_name returns a fully expanded structural interface body,
            // which loses the Generator identity during type relations. Lazy(DefId) preserves
            // the symbolic reference so evaluate_application can properly instantiate it.
            let lazy_base = self.ctx.binder.file_locals.get(gen_name).map(|sym_id| {
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                self.ctx.types.factory().lazy(def_id)
            });
            if let Some(base) = lazy_base {
                // Use contextual generator type params when available, otherwise
                // fall back to defaults (any/void/unknown).
                let yield_t = final_generator_yield_type.unwrap_or(TypeId::ANY);
                let return_t = early_gen_return_type.unwrap_or(TypeId::VOID);
                let next_t = early_gen_next_type.unwrap_or(TypeId::UNKNOWN);
                self.ctx
                    .types
                    .factory()
                    .application(base, vec![yield_t, return_t, next_t])
            } else {
                TypeId::ANY
            }
        } else {
            annotated_return_type.unwrap_or(return_type)
        };
        // Unannotated async functions infer Promise<T>, where T is inferred from
        // return statements in the function body.
        if !has_type_annotation && function_is_async && !function_is_generator {
            // Async functions implicitly await their return values. If the body
            // returns Promise<T> (e.g., `async () => fetch(url)`), the runtime
            // awaits it to get T, then the async wrapper produces Promise<T> —
            // NOT Promise<Promise<T>>. Unwrap any existing Promise layer first.
            if let Some(inner) = self.unwrap_promise_type(final_return_type) {
                final_return_type = inner;
            }
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

    /// Extract a type predicate from JSDoc `@returns {x is Type}` / `@return {this is Entry}`.
    ///
    /// Parse JSDoc `@return` for type predicates and build `TypePredicate` with parameter index.
    pub(crate) fn extract_jsdoc_return_type_predicate(
        &mut self,
        func_jsdoc: &Option<String>,
        params: &[tsz_solver::ParamInfo],
    ) -> Option<tsz_solver::TypePredicate> {
        use tsz_solver::{TypePredicate, TypePredicateTarget};

        let jsdoc = func_jsdoc.as_ref()?;
        let (is_asserts, param_name, type_str) = Self::jsdoc_returns_type_predicate(jsdoc)?;

        // Build the target
        let target = if param_name == "this" {
            TypePredicateTarget::This
        } else {
            let atom = self.ctx.types.intern_string(&param_name);
            TypePredicateTarget::Identifier(atom)
        };

        // Resolve the type (if present)
        let type_id = type_str.and_then(|ts| self.resolve_jsdoc_type_str(&ts));

        // Find parameter index for identifier targets
        let mut parameter_index = None;
        if let TypePredicateTarget::Identifier(name) = &target {
            parameter_index = params.iter().position(|p| p.name == Some(*name));
        }

        Some(TypePredicate {
            asserts: is_asserts,
            target,
            type_id,
            parameter_index,
        })
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
    ) -> Vec<(String, Option<TypeId>, bool)> {
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

                let is_const = self
                    .ctx
                    .arena
                    .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);
                let info = tsz_solver::TypeParamInfo {
                    name: atom,
                    constraint: None,
                    default: None,
                    is_const,
                };
                let type_id = factory.type_param(info);

                // Function type parameters must shadow outer aliases/type parameters
                // with the same name for the duration of this function signature.
                let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
                updates.push((name, previous, false));
                added_params.push(param_idx);
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
            let is_const = self
                .ctx
                .arena
                .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);
            let info = tsz_solver::TypeParamInfo {
                name: atom,
                constraint,
                default: None,
                is_const,
            };
            let constrained_type_id = factory.type_param(info);
            self.ctx
                .type_parameter_scope
                .insert(name, constrained_type_id);
        }

        updates
    }

    /// Evaluate Application types in rest parameters of contextual function types.
    ///
    /// When a generic function is instantiated, rest parameter types may remain as
    /// unevaluated Application types (e.g., `UnwrapContainers<[Container<string>, Container<number>]>`).
    /// The solver's contextual parameter extractor uses `NoopResolver` and cannot evaluate these,
    /// so it returns the whole Application type for each callback parameter instead of individual
    /// tuple elements. This method resolves Application types in rest params using the checker's
    /// `TypeEnvironment`, which can resolve `Lazy(DefId)` references.
    fn evaluate_contextual_rest_param_applications(&mut self, type_id: TypeId) -> TypeId {
        use tsz_solver::type_queries::get_function_shape;

        let Some(shape) = get_function_shape(self.ctx.types, type_id) else {
            return type_id;
        };

        let Some(last_param) = shape.params.last() else {
            return type_id;
        };

        if !last_param.rest {
            return type_id;
        }

        // Only try to evaluate if the rest param type is an Application
        if !tsz_solver::is_generic_application(self.ctx.types, last_param.type_id) {
            return type_id;
        }

        let evaluated_rest = self.evaluate_application_type(last_param.type_id);
        if evaluated_rest == last_param.type_id {
            return type_id;
        }

        // Create a new function shape with the evaluated rest param type
        let mut new_params = shape.params.clone();
        new_params.last_mut().unwrap().type_id = evaluated_rest;

        let new_shape = tsz_solver::FunctionShape {
            type_params: shape.type_params.clone(),
            params: new_params,
            this_type: shape.this_type,
            return_type: shape.return_type,
            type_predicate: shape.type_predicate.clone(),
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        };

        self.ctx.types.function(new_shape)
    }
}

#[cfg(test)]
mod tests;
