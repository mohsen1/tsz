//! Function, method, and arrow function type resolution.
use crate::computation::complex::{
    expression_needs_contextual_return_type, is_contextually_sensitive,
};
use crate::context::TypingRequest;
use crate::context::speculation::DiagnosticSpeculationGuard;
use crate::diagnostics::format_message;
use crate::query_boundaries::type_checking_utilities as type_query;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{ContextualTypeContext, TypeId, TypeParamInfo};
impl<'a> CheckerState<'a> {
    pub(crate) fn js_prototype_owner_expression_for_node(
        &self,
        node_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = node_idx;
        for _ in 0..6 {
            let parent = self.ctx.arena.get_extended(current)?.parent;
            if parent.is_none() {
                break;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            match parent_node.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION
                | syntax_kind_ext::PROPERTY_ASSIGNMENT
                | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                    current = parent;
                }
                syntax_kind_ext::BINARY_EXPRESSION => {
                    let binary = self.ctx.arena.get_binary_expr(parent_node)?;
                    if binary.right != current
                        || !self.is_assignment_operator(binary.operator_token)
                    {
                        return None;
                    }
                    return self.js_prototype_owner_expression_from_assignment_left(binary.left);
                }
                _ => break,
            }
        }
        None
    }

    fn js_prototype_owner_expression_for_function(&self, func_idx: NodeIndex) -> Option<NodeIndex> {
        self.js_prototype_owner_expression_for_node(func_idx)
    }

    fn js_prototype_owner_expression_from_assignment_left(
        &self,
        left_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let left_node = self.ctx.arena.get(left_idx)?;
        let left_access = self.ctx.arena.get_access_expr(left_node)?;

        if self.access_name_matches(left_access.name_or_argument, "prototype") {
            return Some(left_access.expression);
        }

        let proto_node = self.ctx.arena.get(left_access.expression)?;
        let proto_access = self.ctx.arena.get_access_expr(proto_node)?;
        if self.access_name_matches(proto_access.name_or_argument, "prototype") {
            return Some(proto_access.expression);
        }

        None
    }

    pub(crate) fn js_prototype_owner_function_target(
        &self,
        owner_expr: NodeIndex,
    ) -> Option<NodeIndex> {
        let owner_text = self.expression_text(owner_expr)?;

        if !owner_text.contains('.')
            && let Some(sym_id) = self.ctx.binder.file_locals.get(owner_text.as_str())
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            let value_decl = symbol.value_declaration;
            let value_node = self.ctx.arena.get(value_decl)?;
            if value_node.is_function_like() {
                return Some(value_decl);
            }
            if let Some(var_decl) = self.ctx.arena.get_variable_declaration(value_node) {
                let init_node = self.ctx.arena.get(var_decl.initializer)?;
                if init_node.is_function_like() {
                    return Some(var_decl.initializer);
                }
            }
        }

        for raw_idx in 0..self.ctx.arena.len() {
            let idx = NodeIndex(raw_idx as u32);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                continue;
            };
            if self.expression_text(binary.left).as_deref() != Some(owner_text.as_str()) {
                continue;
            }
            let Some(right_node) = self.ctx.arena.get(binary.right) else {
                continue;
            };
            if right_node.is_function_like() {
                return Some(binary.right);
            }
        }

        None
    }

    fn access_name_matches(&self, name_idx: NodeIndex, expected: &str) -> bool {
        self.ctx.arena.get(name_idx).is_some_and(|name_node| {
            self.ctx
                .arena
                .get_identifier(name_node)
                .is_some_and(|ident| ident.escaped_text == expected)
                || (name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
                    && self
                        .ctx
                        .arena
                        .get_literal(name_node)
                        .is_some_and(|lit| lit.text == expected))
        })
    }

    pub(crate) fn js_constructor_body_instance_type_for_function(
        &mut self,
        func_idx: NodeIndex,
    ) -> Option<TypeId> {
        let body_idx = self
            .ctx
            .arena
            .get(func_idx)
            .and_then(|node| self.ctx.arena.get_function(node))
            .and_then(|func| {
                if func.body.is_none() {
                    None
                } else {
                    Some(func.body)
                }
            })?;
        let mut properties = rustc_hash::FxHashMap::default();
        self.collect_js_constructor_this_properties(body_idx, &mut properties, None, false);

        if properties.is_empty() {
            None
        } else {
            Some(
                self.ctx
                    .types
                    .factory()
                    .object(properties.into_values().collect()),
            )
        }
    }

    /// Get type of function declaration/expression/arrow.
    pub(crate) fn get_type_of_function(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_function_impl(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_function_impl(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        use tsz_solver::{FunctionShape, ParamInfo};
        let contextual_type = request.contextual_type;
        let contextual_type_is_assertion = request.origin.is_assertion();
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
            self.check_binding_pattern_optionality(&parameters.nodes, body.is_some(), Some(idx));
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

        if !is_function_declaration && !is_method_or_constructor {
            self.check_duplicate_type_parameters(type_parameters);
            self.check_type_parameters_for_missing_names(type_parameters);
        }

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
        let mut contextual_signature_shape = None;
        let mut contextual_signature_type_param_updates = Vec::new();
        let mut has_jsdoc_type_function = false;
        let mut ctx_helper = if let Some(ctx_type) = contextual_type {
            use crate::query_boundaries::type_checking_utilities::{
                EvaluationNeeded, classify_for_evaluation, lazy_def_id, type_application,
            };

            let evaluated_type = if type_application(self.ctx.types, ctx_type).is_some() {
                self.evaluate_application_type(ctx_type)
            } else if lazy_def_id(self.ctx.types, ctx_type).is_some()
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
            contextual_signature_shape =
                crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    evaluated_type,
                );
            let evaluated_type = self.normalize_contextual_signature_with_env(evaluated_type);
            let helper_probe = ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                evaluated_type,
                self.ctx.compiler_options.no_implicit_any,
            );
            let evaluated_type = if helper_probe.get_this_type().is_none()
                && helper_probe.get_return_type().is_none()
                && helper_probe.get_parameter_type(0).is_none()
                && helper_probe.get_rest_parameter_type(0).is_none()
                && !tsz_solver::is_union_type(self.ctx.types, evaluated_type)
                && !tsz_solver::is_intersection_type(self.ctx.types, evaluated_type)
            {
                crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    evaluated_type,
                )
                .map(|shape| self.ctx.types.factory().function(shape))
                .unwrap_or(evaluated_type)
            } else {
                evaluated_type
            };

            contextual_signature_type_params =
                self.contextual_type_params_from_expected(evaluated_type);
            Some(ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                evaluated_type,
                self.ctx.compiler_options.no_implicit_any,
            ))
        } else if self.is_js_file() && (is_function_declaration || is_closure) {
            // In JS/checkJs, JSDoc `@type {FunctionType}` can live either on a
            // function declaration or on an enclosing variable statement for a
            // function expression (`const f = function() {}`), so support both.
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
        let inherited_contextual_generics =
            is_closure && type_params.is_empty() && contextual_signature_type_params.is_some();
        if is_closure
            && type_params.is_empty()
            && let Some(contextual_type_params) = contextual_signature_type_params
        {
            contextual_signature_type_param_updates =
                self.push_contextual_type_parameter_infos(&contextual_type_params);
            type_params = contextual_type_params;
        }

        // For arrow functions, capture the outer `this` type to preserve lexical `this`
        // Arrow functions should inherit `this` from their enclosing scope
        let outer_this_type = if is_arrow_function {
            self.current_this_type()
        } else {
            None
        };
        let prototype_owner_expr = if self.is_js_file() && !is_arrow_function {
            self.js_prototype_owner_expression_for_node(idx)
        } else {
            None
        };
        let prototype_owner_target = prototype_owner_expr
            .and_then(|owner_expr| self.js_prototype_owner_function_target(owner_expr));
        let js_constructor_target = if self.is_js_file() && !is_arrow_function {
            if is_function_declaration {
                Some(idx)
            } else if is_closure {
                self.ctx
                    .arena
                    .get_extended(idx)
                    .map(|ext| ext.parent)
                    .filter(|parent| {
                        self.ctx.arena.get(*parent).is_some_and(|node| {
                            node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION
                        })
                    })
            } else {
                None
            }
        } else {
            None
        };
        // Extract JSDoc for the function to check for @param/@returns annotations.
        // This suppresses false TS7006/TS7010/TS7011 in JS files with JSDoc type annotations.
        let func_jsdoc = self.get_jsdoc_for_function(idx);
        let mut jsdoc_type_param_types: FxHashMap<String, TypeId> = FxHashMap::default();

        // TS2730: Arrow functions cannot have a 'this' parameter.
        // In JS files, a @this JSDoc tag on an arrow function is an error because
        // arrow functions capture `this` lexically.
        if is_arrow_function
            && self.is_js_file()
            && let Some(ref jsdoc) = func_jsdoc
            && jsdoc.contains("@this")
        {
            if let Some(sf) = self.source_file_data_for_node(idx) {
                let source_text = sf.text.to_string();
                let comments = sf.comments.clone();
                if let Some((_, jsdoc_start)) =
                    self.try_jsdoc_with_ancestor_walk_and_pos(idx, &comments, &source_text)
                {
                    // jsdoc_start is the comment's pos (start of `/**`).
                    // Search from there to find `@this` in the raw source.
                    let search_start = jsdoc_start as usize;
                    if let Some(this_off) = source_text[search_start..].find("@this") {
                        // Verify this is @this tag, not a substring of another tag
                        let at_pos = search_start + this_off;
                        let after = &source_text[at_pos + 5..];
                        let is_this_tag = after.starts_with(' ')
                            || after.starts_with('{')
                            || after.starts_with('\n')
                            || after.starts_with('\r');
                        if is_this_tag {
                            // tsc points at "this" (after the "@"), not "@this"
                            self.ctx.error(
                                (at_pos + 1) as u32,
                                4, // length of "this"
                                "An arrow function cannot have a 'this' parameter.".to_string(),
                                crate::diagnostics::diagnostic_codes::AN_ARROW_FUNCTION_CANNOT_HAVE_A_THIS_PARAMETER,
                            );
                        }
                    }
                }
            }
        }

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
            && let Some(owner_target) = prototype_owner_target
            && let Some(owner_jsdoc) = self.find_jsdoc_for_function(owner_target)
        {
            let factory = self.ctx.types.factory();
            for name in Self::jsdoc_template_type_params(&owner_jsdoc) {
                let atom = self.ctx.types.intern_string(&name);
                let info = TypeParamInfo {
                    name: atom,
                    constraint: None,
                    default: None,
                    is_const: false,
                };
                let ty = factory.type_param(info);
                jsdoc_type_param_types.insert(name.clone(), ty);
                let previous = self.ctx.type_parameter_scope.insert(name.clone(), ty);
                jsdoc_type_param_updates.push((name, previous, false));
            }
        }
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

        let js_constructor_instance_type = js_constructor_target.and_then(|target_idx| {
            self.synthesize_js_constructor_instance_type(target_idx, TypeId::ANY, &[])
        });
        let js_prototype_owner_instance_type = prototype_owner_target.and_then(|owner_target| {
            self.js_constructor_body_instance_type_for_function(owner_target)
        });

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

        // Check if this closure is inside a JSDoc @type cast parenthesized expression.
        // In JS files, `/** @type {SomeType} */(expr)` acts as a type assertion.
        // Even if the import type in the JSDoc can't be fully resolved, the presence
        // of a @type annotation means the user explicitly typed the expression,
        // so TS7006 should be suppressed for closures within the cast.
        let is_in_jsdoc_type_cast =
            is_closure && self.is_js_file() && { self.is_inside_jsdoc_type_cast(idx) };

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
        if is_closure
            && self.is_js_file()
            && parameters.nodes.is_empty()
            && self.body_has_arguments_reference(body)
            && let Some(ref jsdoc) = func_jsdoc
            && !jsdoc.contains("@callback")
        {
            self.check_jsdoc_param_tag_names(jsdoc, &parameters.nodes, idx);
        }

        // Track whether any parameter actually receives a contextual type from
        // ctx_helper. Used after the loop to decide whether to mark the closure as
        // "contextually checked". We cannot unconditionally mark based on
        // ctx_helper.is_some() because the expected type may be a bare type parameter
        // or non-callable type that provides no parameter types.
        let mut _any_param_contextually_typed = false;

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
                        if param.dot_dot_dot_token {
                            self.contextual_parameter_type_with_env_from_expected(
                                expected,
                                contextual_index,
                                true,
                            )
                        } else {
                            self.contextual_parameter_type_for_call_with_env_from_expected(
                                expected,
                                contextual_index,
                                parameters.nodes.len(),
                            )
                            .or_else(|| {
                                self.contextual_parameter_type_with_env_from_expected(
                                    expected,
                                    contextual_index,
                                    false,
                                )
                            })
                        }
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
                            let direct_is_constrained_type_param = extracted != from_expected
                                && crate::query_boundaries::common::type_parameter_constraint(
                                    self.ctx.types,
                                    extracted,
                                )
                                .is_some_and(|constraint| {
                                    let evaluated_constraint =
                                        self.evaluate_type_with_env(constraint);
                                    evaluated_constraint == from_expected
                                        || self
                                            .is_assignable_to(from_expected, evaluated_constraint)
                                });
                            let direct_is_rest_tuple_container = !param.dot_dot_dot_token
                                && extracted != from_expected
                                && (tsz_solver::type_queries::get_tuple_elements(
                                    self.ctx.types,
                                    extracted,
                                )
                                .is_some()
                                    || tsz_solver::type_queries::get_array_element_type(
                                        self.ctx.types,
                                        extracted,
                                    )
                                    .is_some());
                            let expected_is_more_informative = from_expected != TypeId::ANY
                                && from_expected != TypeId::UNKNOWN
                                && !tsz_solver::type_queries::contains_infer_types_db(
                                    self.ctx.types,
                                    from_expected,
                                );
                            let direct_is_strict_subtype = extracted != from_expected
                                && self.is_subtype_of(extracted, from_expected)
                                && !self.is_subtype_of(from_expected, extracted);
                            if direct_is_rest_tuple_container
                                || (direct_is_placeholderish && expected_is_more_informative)
                                || direct_is_constrained_type_param
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
                let has_never_expected_context = ctx_helper
                    .as_ref()
                    .and_then(tsz_solver::ContextualTypeContext::expected)
                    .is_some_and(|t| t == TypeId::NEVER);
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
                let suppresses_implicit_any_context =
                    has_contextual_type && !has_never_expected_context;
                if is_closure && suppresses_implicit_any_context {
                    self.ctx.implicit_any_contextual_closures.insert(idx);
                    _any_param_contextually_typed = true;
                }
                // Use type annotation if present, otherwise infer from context
                let (type_id, has_external_binding_context) = if param.type_annotation.is_some() {
                    // Check parameter type for parameter properties in function types
                    self.check_type_for_parameter_properties(param.type_annotation);
                    // Check for undefined type names in parameter type
                    self.check_type_for_missing_names(param.type_annotation);
                    (self.get_type_from_type_node(param.type_annotation), false)
                } else if is_this_param {
                    // For `this` parameter without type annotation:
                    // - Arrow functions: inherit outer `this` type to preserve lexical scoping
                    // - Regular functions: use ANY (will trigger TS2683 when used, not TS2571)
                    // - Contextual type: if provided, use it (for function types with explicit `this`)
                    let ty = if let Some(ref helper) = ctx_helper {
                        helper
                            .get_this_type()
                            .or(outer_this_type)
                            .unwrap_or(TypeId::ANY)
                    } else {
                        outer_this_type.unwrap_or(TypeId::ANY)
                    };
                    (ty, false)
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
                        let raw = self.infer_iife_parameter_type_from_arguments(
                            idx,
                            contextual_index,
                            param.dot_dot_dot_token,
                            param.question_token || param.initializer.is_some(),
                        );
                        // When the IIFE returns `undefined` (no argument provided) but
                        // the parameter has a default value, discard the IIFE inference
                        // so the default value type is used instead. E.g.:
                        //   (({ u = 22 } = { u: 23 }) => u)()
                        if raw == Some(TypeId::UNDEFINED) && param.initializer.is_some() {
                            None
                        } else {
                            raw
                        }
                    } else {
                        None
                    };
                    let has_external_binding_context = contextual_type.is_some()
                        || iife_arg_type.is_some()
                        || jsdoc_param_type.is_some();
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
                                .union2(inferred_type, TypeId::UNDEFINED)
                        } else {
                            inferred_type
                        }
                    } else {
                        inferred_type
                    };
                    let ty = if inferred_type == TypeId::ANY && param.initializer.is_some() {
                        let mut init_type = self.get_type_of_node(param.initializer);
                        if self.is_js_file()
                            && (init_type == TypeId::ANY || init_type == TypeId::UNKNOWN)
                            && self.ctx.arena.get(param.initializer).is_some_and(|n| {
                                n.kind == tsz_scanner::SyntaxKind::Identifier as u16
                            })
                        {
                            let current_param_sym = self
                                .ctx
                                .arena
                                .get(param.name)
                                .and_then(|name_node| {
                                    (name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16)
                                        .then(|| self.resolve_identifier_symbol(param.name))
                                })
                                .flatten();
                            if let Some(sym_id) = self.resolve_identifier_symbol(param.initializer)
                                && Some(sym_id) != current_param_sym
                            {
                                let jsdoc_decl_type = self
                                    .ctx
                                    .binder
                                    .get_symbol(sym_id)
                                    .and_then(|sym| {
                                        if sym.value_declaration.is_some() {
                                            Some(sym.value_declaration)
                                        } else {
                                            sym.declarations.first().copied()
                                        }
                                    })
                                    .and_then(|decl_idx| {
                                        self.jsdoc_type_annotation_for_node(decl_idx)
                                    });
                                let resolved_init_type = jsdoc_decl_type
                                    .filter(|t| {
                                        *t != TypeId::ANY
                                            && *t != TypeId::UNKNOWN
                                            && *t != TypeId::ERROR
                                    })
                                    .unwrap_or_else(|| self.get_type_of_symbol(sym_id));
                                if resolved_init_type != TypeId::ANY
                                    && resolved_init_type != TypeId::UNKNOWN
                                    && resolved_init_type != TypeId::ERROR
                                {
                                    init_type = resolved_init_type;
                                }
                            }
                        }
                        // Only widen when the initializer is a "fresh" literal expression
                        let is_enum_member = self.is_enum_member_type_for_widening(init_type);
                        if is_enum_member || self.is_fresh_literal_expression(param.initializer) {
                            self.widen_initializer_type_for_mutable_binding(init_type)
                        } else {
                            init_type
                        }
                    } else {
                        inferred_type
                    };
                    (ty, has_external_binding_context)
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
                let cached_param_type = (!has_contextual_type && param.type_annotation.is_none())
                    .then(|| {
                        self.ctx
                            .node_types
                            .get(&param.name.0)
                            .copied()
                            .or_else(|| self.ctx.node_types.get(&param_idx.0).copied())
                    })
                    .flatten()
                    .filter(|cached_param_type| {
                        *cached_param_type != TypeId::ANY
                            && *cached_param_type != TypeId::UNKNOWN
                            && *cached_param_type != TypeId::ERROR
                    });
                let mut type_id = if let Some(pattern_type) = element_type_from_pattern {
                    if param.type_annotation.is_some()
                        || ((has_contextual_type || has_external_binding_context)
                            && type_id != TypeId::ANY
                            && type_id != TypeId::UNKNOWN)
                    {
                        // When a type annotation, concrete contextual type, or IIFE
                        // argument type is available, preserve it.  The binding pattern
                        // only determines individual variable bindings, not the
                        // parameter's overall type.  Without this guard, array
                        // destructuring `([a])` would reconstruct a tuple `[T]` instead
                        // of keeping the contextual array type `T[]`, causing a false
                        // TS2345.  Including `has_external_binding_context` ensures IIFE
                        // argument types are preserved for destructuring parameters:
                        //   (({ a, b }) => a)({ a: 1, b: 2 })
                        //
                        // When the contextual type is `unknown` (e.g. from an uninferred
                        // type parameter), fall back to the binding pattern type. `unknown`
                        // is not a useful structural type for destructuring — properties
                        // can't be extracted from it — and tsc falls back to the pattern
                        // type `{a: any}` in this case, which correctly produces TS2345
                        // when the callback is checked for assignability.
                        type_id
                    } else {
                        pattern_type
                    }
                } else {
                    type_id
                };
                if let Some(cached_param_type) = cached_param_type {
                    type_id = cached_param_type;
                }
                let has_effective_contextual_type =
                    has_contextual_type || cached_param_type.is_some();
                let binding_context_type = (has_external_binding_context
                    || cached_param_type.is_some())
                .then_some(type_id);
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
                let has_jsdoc_param =
                    if !has_effective_contextual_type && param.type_annotation.is_none() {
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
                let has_broad_jsdoc_function_type = param.type_annotation.is_none()
                    && func_jsdoc
                        .as_ref()
                        .is_some_and(|jsdoc| Self::jsdoc_type_tag_is_broad_function(jsdoc));
                let implicit_any_type_hint = if self.is_js_file()
                    && param.initializer.is_some()
                    && !has_effective_contextual_type
                    && !has_jsdoc_param
                    && param.type_annotation.is_none()
                {
                    if !self.ctx.strict_null_checks()
                        && (type_id == TypeId::NULL || type_id == TypeId::UNDEFINED)
                    {
                        type_id = TypeId::ANY;
                        Some("any")
                    } else if self
                        .ctx
                        .arena
                        .get(param.initializer)
                        .and_then(|n| self.ctx.arena.get_literal_expr(n))
                        .is_some_and(|lit| lit.elements.nodes.is_empty())
                        && let Some(elem_type) = tsz_solver::type_queries::get_array_element_type(
                            self.ctx.types,
                            type_id,
                        )
                        && (elem_type == TypeId::ANY || elem_type == TypeId::NEVER)
                    {
                        type_id = self.ctx.types.factory().array(TypeId::ANY);
                        Some("any[]")
                    } else {
                        None
                    }
                } else {
                    None
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
                // Check the node's own modifiers directly rather than relying on
                // enclosing_class (which may not be set when get_type_of_function
                // is called outside the class member checking pass).
                let is_ambient_private = self.ctx.is_ambient_declaration(idx)
                    && (self
                        .ctx
                        .arena
                        .get_method_decl(node)
                        .is_some_and(|m| self.has_private_modifier(&m.modifiers))
                        || self
                            .ctx
                            .arena
                            .get_accessor(node)
                            .is_some_and(|a| self.has_private_modifier(&a.modifiers))
                        || self
                            .ctx
                            .arena
                            .get_constructor(node)
                            .is_some_and(|c| self.has_private_modifier(&c.modifiers)));
                // When ctx_helper's expected type is  (e.g. a mapped type property
                // mapping excess keys to ), no param contextual types can be derived.
                // Do not defer TS7006: the second pass will use the inferred type (possibly
                // ) and incorrectly suppress TS7006. Emit immediately.
                let ctx_helper_expected_is_never = ctx_helper
                    .as_ref()
                    .and_then(tsz_solver::ContextualTypeContext::expected)
                    .is_some_and(|t| t == TypeId::NEVER);
                let skip_implicit_any = is_setter
                    || (is_closure
                        && !self.ctx.is_checking_statements
                        && !has_effective_contextual_type
                        && !ctx_helper_expected_is_never)
                    || (is_in_decorator && !has_effective_contextual_type)
                    || is_in_jsdoc_type_cast
                    || closure_already_checked
                    || is_ambient_private;
                if !skip_implicit_any {
                    if has_broad_jsdoc_function_type
                        && !param.dot_dot_dot_token
                        && param.initializer.is_none()
                        && !self.is_this_parameter_name(param.name)
                    {
                        let param_name = self.parameter_name_for_error(param.name);
                        if !param_name.is_empty() {
                            self.error_at_node_msg(
                                param.name,
                                crate::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
                                &[&param_name, "any"],
                            );
                        }
                    } else {
                        self.maybe_report_implicit_any_parameter_with_type_hint(
                            param,
                            has_effective_contextual_type
                                || has_jsdoc_param
                                || has_external_binding_context,
                            contextual_index,
                            implicit_any_type_hint,
                        );
                    }
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
                {
                    let db = self.ctx.types.as_type_database();
                    if rest
                        && (tsz_solver::is_generic_application(db, type_id)
                            || tsz_solver::is_mapped_type(db, type_id)
                            || tsz_solver::is_conditional_type(db, type_id)
                            || tsz_solver::is_intersection_type(db, type_id))
                    {
                        let evaluated = if tsz_solver::is_generic_application(db, type_id) {
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
                        let no_infer =
                            tsz_solver::visitor::no_infer_inner_type(db, evaluated).is_some();
                        if array_like || no_infer {
                            type_id = evaluated;
                        }
                    }
                }
                // Store the declared type for optional params — do NOT add
                // `| undefined` here.  The solver's `check_argument_types_with`
                // already unions `| undefined` at check time, and tsc uses the
                // declared type (without undefined) in error messages.
                let needs_undefined = param.question_token
                    && type_id != TypeId::ANY
                    && type_id != TypeId::UNKNOWN
                    && type_id != TypeId::ERROR
                    && !tsz_solver::type_contains_undefined(self.ctx.types, type_id);
                params.push(ParamInfo {
                    name,
                    type_id,
                    optional,
                    rest,
                });
                let cached_type = if needs_undefined && self.ctx.strict_null_checks() {
                    self.ctx.types.factory().union2(type_id, TypeId::UNDEFINED)
                } else {
                    type_id
                };
                param_types.push(Some(cached_type));
                destructuring_context_param_types.push(binding_context_type);
                // Only increment contextual_index for non-`this` parameters.
                // The contextual FunctionShape stores `this` separately in `this_type`,
                // not in the `params` array, so `this` doesn't consume a param index.
                if !is_this_param {
                    contextual_index += 1;
                }
            }
        }

        // Record that we've checked this closure for implicit-any diagnostics so
        // later re-entrant passes do not re-emit TS7006/TS7031. Do this after the
        // full parameter walk so sibling parameters in the same closure are all checked.
        // Only mark as "checked" in statement-checking mode or when a contextual type was
        // available. Closures skipped in build_type_environment due to no contextual type
        // need a second chance in the statement-checking pass — don't pre-emptively lock them.
        if is_closure
            && !closure_already_checked
            && (self.ctx.is_checking_statements || ctx_helper.is_some())
        {
            self.ctx.implicit_any_checked_closures.insert(idx);
        }
        // Track closures that deferred TS7006 during type env building.
        // These closures were processed before is_checking_statements was set, without
        // a contextual type, so skip_implicit_any was true. Their cached types may
        // prevent re-processing during statement checking, so we record them for an
        // explicit re-check after is_checking_statements is set.
        if is_closure
            && !closure_already_checked
            && !self.ctx.is_checking_statements
            && ctx_helper.is_none()
            && self.ctx.no_implicit_any()
        {
            self.ctx.deferred_implicit_any_closures.push(idx);
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
                    .or(js_constructor_instance_type)
                    .or(js_prototype_owner_instance_type)
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
                                                    if let Some(proto_node) = self.ctx.arena.get(access.expression)
                                                        && (proto_node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                                                            || proto_node.kind == tsz_parser::parser::syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                                                        && let Some(proto_access) = self.ctx.arena.get_access_expr(proto_node)
                                                        && let Some(proto_name_node) = self.ctx.arena.get(proto_access.name_or_argument)
                                                        && let Some(proto_ident) = self.ctx.arena.get_identifier(proto_name_node)
                                                        && proto_ident.escaped_text == "prototype" {
                                                            let constructor_type = self.get_type_of_node(proto_access.expression);
                                                            if let Some(instance_type) = self.synthesize_js_constructor_instance_type(
                                                                proto_access.expression,
                                                                constructor_type,
                                                                &[],
                                                            ) {
                                                                return Some(instance_type);
                                                            }
                                                        }
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
                            }
                            break;
                        }
                        None
                    })
            }
        });

        let mut pushed_this_type_early = false;
        if let Some(tt) = implicit_this {
            self.ctx.this_type_stack.push(tt);
            self.ctx.function_owned_this_stack.push(idx);
            pushed_this_type_early = true;
            // Track closures with contextual this types.
            // Any non-None implicit_this for a closure comes from a contextual source
            // (parameter type with this, JS constructor, prototype owner, or prototype assignment).
            if is_closure {
                self.ctx.closures_with_contextual_this_type.insert(idx);
            }
        }

        self.check_non_impl_parameter_initializers(&parameters.nodes, false, body.is_some());
        if body.is_some() {
            // Track that we're inside a nested function for abstract property access checks.
            // This must happen before infer_return_type_from_body which evaluates body expressions.
            self.ctx.function_depth += 1;
            self.cache_parameter_types(&parameters.nodes, Some(&param_types));
            let refresh_body_for_contextual_param_retyping =
                is_closure && (ctx_helper.is_some() || func_jsdoc.is_some());
            if refresh_body_for_contextual_param_retyping {
                // Function expressions are often visited once during environment building
                // and again with contextual/JSDoc parameter types during checked mode.
                // Re-evaluate the body from the shared cached parameter types so reads like
                // `acceptNum(b)` see the same optionality/type-tag result as the signature.
                //
                // Targeted invalidation: clear body only (not param symbols,
                // which were just set by cache_parameter_types above).
                self.invalidate_function_body_for_param_retyping(body);
            }
            self.record_destructured_parameter_binding_groups(&parameters.nodes, &param_types);
            self.record_contextual_tuple_parameter_groups(&parameters.nodes, contextual_type);

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
                // Do NOT set final_generator_yield_type from the contextual yield type.
                // The final yield type must be inferred from the actual body (what is
                // yielded), not from the contextual annotation. This allows the normal
                // assignment check at the variable declaration site to catch cases like
                //   var g: () => Iterable<Foo> = function* () { yield new Baz; }
                // where Baz is not assignable to Foo. Using the contextual type would
                // hide the incompatibility by making the generator appear to yield Foo.
                early_gen_return_type = gen_types.1;
                early_gen_next_type = gen_types.2;
            }
            if early_yield_type.is_some() {
                self.ctx.push_yield_type(early_yield_type);
            }

            if !has_type_annotation {
                // When the contextual type comes from a type assertion (`as` or
                // angle-bracket cast), suppress the contextual return type.
                // Parameter types are still contextually typed, but the body's
                // return value should NOT be checked against the asserted return
                // type — only TS2352 fires at the assertion site.
                let return_context = if contextual_type_is_assertion {
                    jsdoc_return_context
                } else {
                    jsdoc_return_context.or_else(|| {
                        ctx_helper
                            .as_ref()
                            .and_then(tsz_solver::ContextualTypeContext::get_return_type)
                            .or_else(|| {
                                contextual_type.and_then(|ty| {
                                    crate::query_boundaries::checkers::call::get_contextual_signature(
                                        self.ctx.types,
                                        ty,
                                    )
                                    .map(|shape| shape.return_type)
                                    .or_else(|| {
                                        tsz_solver::type_queries::get_return_type(self.ctx.types, ty)
                                    })
                                })
                            })
                    })
                };
                // Async function bodies return the awaited inner type; the function
                // type itself is Promise<inner>. Contextual return typing must
                // therefore use the inner type, not Promise<inner>.
                let return_context = if is_async && !is_generator {
                    return_context.map(|ctx_ty| self.unwrap_async_return_type_for_body(ctx_ty))
                } else if is_generator && !is_async {
                    // Generator function bodies return TReturn, not Generator<Y, TReturn, N>.
                    // When the contextual return type is a Generator application, unwrap
                    // to TReturn so that `return expr` in the body is contextually typed
                    // against the correct type (matching tsc behavior).
                    early_gen_return_type
                        .or(return_context.and_then(|ctx_ty| {
                            let ret_ctx = tsz_solver::ContextualTypeContext::with_expected(
                                self.ctx.types,
                                ctx_ty,
                            );
                            ret_ctx.get_generator_return_type()
                        }))
                        .or(return_context)
                } else if is_generator {
                    // Async generator function bodies return TReturn, not AsyncGenerator<Y, TReturn, N>.
                    // Unwrap TReturn from the contextual AsyncGenerator application so that
                    // `return expr` in the body is contextually typed against TReturn
                    // (matching tsc behavior, same as sync generators).
                    early_gen_return_type
                        .or(return_context.and_then(|ctx_ty| {
                            let ret_ctx = tsz_solver::ContextualTypeContext::with_expected(
                                self.ctx.types,
                                ctx_ty,
                            );
                            ret_ctx.get_generator_return_type()
                        }))
                        .or(return_context)
                } else {
                    return_context
                };
                return_context_for_circularity = return_context;
                // TS7010/TS7011: Only count as contextual return if it's not UNKNOWN
                // UNKNOWN is a "no type" value and shouldn't prevent implicit any errors
                has_contextual_return = return_context.is_some_and(|t| t != TypeId::UNKNOWN);

                // For async functions, expand the return context to include Promise
                // types so that `return new Promise(resolve => resolve())` can infer
                // T = void during return type inference. The same transformation is
                // applied in check_return_statement (core_statement_checks.rs) but
                // that runs AFTER inference. During inference, the raw contextual
                // return type (e.g., `void`) doesn't carry enough information for
                // the generic Promise constructor to infer T. Transform:
                //   void -> void | PromiseLike<void> | Promise<void>
                // This enables contextually-typed async callbacks like
                //   run(async () => { return new Promise(resolve => resolve()); })
                // to correctly infer T = void when `run` expects `() => void`.
                let inference_return_context = if is_async
                    && !is_generator
                    && let Some(ctx_type) = return_context
                    && ctx_type != TypeId::ANY
                    && ctx_type != TypeId::UNKNOWN
                    && ctx_type != TypeId::NEVER
                    && !tsz_solver::is_union_type(self.ctx.types, ctx_type)
                    && !self.is_promise_type(ctx_type)
                {
                    let promise_like_t = self.get_promise_like_type(ctx_type);
                    let promise_t = self.get_promise_type(ctx_type);
                    let mut members = vec![ctx_type, promise_like_t];
                    if let Some(pt) = promise_t {
                        members.push(pt);
                    }
                    Some(self.ctx.types.factory().union(members))
                } else {
                    return_context
                };

                // When the return context is (or references) a const type parameter,
                // enable const assertion mode so array/object literals in the callback
                // body are inferred as readonly tuples/readonly objects. This matches
                // tsc's behavior where `const` type parameter context flows down into
                // callback returns during inference.
                let prev_const_assertion = self.ctx.in_const_assertion;
                if !self.ctx.in_const_assertion
                    && let Some(ret_ctx) = return_context
                {
                    let has_const_tp = self.return_context_has_const_type_param(ret_ctx);
                    if has_const_tp {
                        self.ctx.in_const_assertion = true;
                    }
                }
                let inferred =
                    self.infer_return_type_from_body(idx, body, inference_return_context);
                self.ctx.in_const_assertion = prev_const_assertion;
                return_type = jsdoc_return_context.unwrap_or(inferred);

                if let Some(instance_type) = js_constructor_instance_type
                    && (return_type == TypeId::UNDEFINED || return_type == TypeId::VOID)
                {
                    return_type = instance_type;
                }

                if let Some(instance_type) = js_constructor_instance_type
                    && let Some(union_members) =
                        crate::query_boundaries::common::union_members(self.ctx.types, return_type)
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

            // TS2705/TS2468: Check async Promise constructor availability
            self.check_async_promise_constructor_availability(
                is_async,
                is_generator,
                is_function_declaration,
                has_type_annotation,
                async_node_idx,
                idx,
            );

            // TS2705/TS1055/TS1064: Check async return type is Promise
            // Use the pre-evaluation return type (annotated_return_type) so that
            // type aliases like `type MyPromise<T> = Promise<T>` are still seen
            // as Application types. After evaluate_application_type(), the type
            // is flattened to an Object shape and loses its Promise identity.
            self.check_async_return_type_is_promise(
                has_type_annotation,
                is_async,
                is_generator,
                annotated_return_type.unwrap_or(return_type),
                type_annotation,
            );

            // TS2366/TS2355/TS7030: Check return completeness
            self.check_function_return_completeness(
                is_function_declaration,
                body,
                idx,
                annotated_return_type,
                return_type,
                has_type_annotation,
                type_annotation,
                function_is_generator,
                name_node,
                idx,
            );

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

            // When the body return type contains the polymorphic `this` type
            // (e.g. from `async (): Promise<this> => this`), substitute it
            // with the concrete `this` type from the enclosing class so that
            // the return-statement assignability check compares against the
            // same concrete type that the `this` keyword expression resolves to.
            // Only apply when the function has an explicit type annotation;
            // contextually-typed functions may carry `ThisType` from their
            // contextual signature but substituting would produce false positives.
            let body_return_type = if has_type_annotation
                && tsz_solver::contains_this_type(self.ctx.types, body_return_type)
            {
                if let Some(concrete_this) = self.current_this_type() {
                    tsz_solver::substitute_this_type(
                        self.ctx.types,
                        body_return_type,
                        concrete_this,
                    )
                } else {
                    body_return_type
                }
            } else {
                body_return_type
            };

            self.push_return_type(body_return_type);

            // For generator functions with explicit annotations, push the yield type
            // from the annotation. Contextually-typed generators already had their yield
            // type pushed early (before infer_return_type_from_body).
            if is_generator && has_type_annotation {
                let original_type = annotated_return_type.unwrap_or(return_type);
                let yield_type = self.get_generator_yield_type_argument(original_type);
                self.ctx.push_yield_type(yield_type);
                // Push the next type from the annotation for yield result typing
                let next_type = self.get_generator_next_type_argument(original_type);
                self.ctx.push_generator_next_type(next_type);
            } else if is_generator && early_yield_type.is_none() && !has_type_annotation {
                // Unannotated generator: push None so dispatch.rs defers TS7057.
                // After body check, we'll compute the inferred yield type union
                // and emit either TS7055/TS7025 (if yield type is any) or flush
                // deferred TS7057 diagnostics.
                self.ctx.push_yield_type(None);
                self.ctx.push_generator_next_type(None);
            } else if early_yield_type.is_none() {
                // No early push was done, push None for stack balance
                self.ctx.push_yield_type(None);
                self.ctx.push_generator_next_type(None);
            } else if is_generator {
                // Contextually-typed generator: push the next type from contextual extraction
                self.ctx.push_generator_next_type(early_gen_next_type);
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
                let raw_expected_return_type =
                    expected_expression_return_type.expect("is_some checked in outer condition");
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
                                && !crate::query_boundaries::common::contains_type_parameters(
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
                            let body_request = if keep_contextual_body {
                                TypingRequest::with_contextual_type(expected_return_type)
                            } else {
                                TypingRequest::NONE
                            };
                            let prev_preserve_literals = self.ctx.preserve_literal_types;
                            if keep_contextual_body {
                                self.ctx.preserve_literal_types = true;
                            }
                            if body_request.is_empty() {
                                self.invalidate_expression_for_contextual_retry(body);
                            }
                            let t = self.get_type_of_node_with_request(body, &body_request);
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
                        // Use union-aware unwrapping so `[0] | Promise<never>`
                        // becomes `[0] | never` = `[0]`, not kept as-is.
                        self.unwrap_async_return_type_for_body(actual_return)
                    } else {
                        actual_return
                    };
                    // Suppress the inner return type check when the expected type has
                    // unresolved inference holes, OR when the actual return is callable
                    // but expected is not (function-to-non-function shape mismatch),
                    // OR when the body is a simple expression (not object/array literal
                    // or block). tsc reports simple expression-bodied arrow return type
                    // mismatches as TS2345 on the argument, not TS2322 on the body.
                    let body_is_simple_expression =
                        self.ctx.arena.get(body).is_some_and(|body_node| {
                            let effective_kind =
                                if body_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                                    self.ctx
                                        .arena
                                        .get_parenthesized(body_node)
                                        .and_then(|paren| self.ctx.arena.get(paren.expression))
                                        .map(|inner| inner.kind)
                                        .unwrap_or(body_node.kind)
                                } else {
                                    body_node.kind
                                };
                            effective_kind != syntax_kind_ext::BLOCK
                                && effective_kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                && effective_kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        });
                    let suppress_contextual_return_check = !has_type_annotation
                        && jsdoc_return_context.is_none()
                        && (self.type_has_unresolved_inference_holes(expected_return_type)
                            || (tsz_solver::type_queries::is_callable_type(
                                self.ctx.types,
                                actual_return,
                            ) && !tsz_solver::type_queries::is_callable_type(
                                self.ctx.types,
                                expected_return_type,
                            ))
                            || body_is_simple_expression);
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
                                let guard = DiagnosticSpeculationGuard::new(&self.ctx);
                                let return_req =
                                    TypingRequest::with_contextual_type(expected_return_type);
                                let mut when_true =
                                    self.get_type_of_node_with_request(cond.when_true, &return_req);
                                let mut when_false = self
                                    .get_type_of_node_with_request(cond.when_false, &return_req);
                                guard.rollback(&mut self.ctx);
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
                        let assignability_ok = self
                            .check_assignable_or_report_at_without_source_elaboration(
                                actual_return,
                                expected_return_type,
                                body,
                                body,
                            );
                        if !assignability_ok {
                            // Find and store any new TS2322 diagnostics from this check
                            for diag in self.ctx.diagnostics.iter().rev() {
                                if diag.code
                                    == tsz_common::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                                    && let Some(body_node) = self.ctx.arena.get(body)
                                    && diag.start >= body_node.pos
                                    && diag.start < body_node.end
                                {
                                    self.ctx
                                        .callback_return_type_errors
                                        .push(diag.clone());
                                    break;
                                }
                            }
                        }
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
                // Compute the effective body context as a local variable instead of
                // modifying the ambient ctx.contextual_type.
                let outer_ctx = contextual_type;
                let effective_body_ctx = if let Some(body_node) = self.ctx.arena.get(body)
                    && body_node.kind != syntax_kind_ext::BLOCK
                    && !has_type_annotation
                {
                    let body_return_context = ctx_helper
                        .as_ref()
                        .and_then(tsz_solver::ContextualTypeContext::get_return_type)
                        .or_else(|| {
                            outer_ctx.and_then(|ty| {
                                crate::query_boundaries::checkers::call::get_contextual_signature(
                                    self.ctx.types,
                                    ty,
                                )
                                .map(|shape| shape.return_type)
                                .or_else(|| {
                                    tsz_solver::type_queries::get_return_type(self.ctx.types, ty)
                                })
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
                        body_return_context
                    } else {
                        outer_ctx
                    }
                } else {
                    outer_ctx
                };
                let suppress_expression_body_diagnostics =
                    self.ctx.arena.get(body).is_some_and(|body_node| {
                        body_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                            && !has_type_annotation
                            && jsdoc_return_context.is_none()
                            && effective_body_ctx.is_some_and(|return_type| {
                                self.type_has_unresolved_inference_holes(return_type)
                            })
                    });
                let diag_guard = suppress_expression_body_diagnostics
                    .then(|| DiagnosticSpeculationGuard::new(&self.ctx));
                // During type environment building (before is_checking_statements),
                // skip full body checking for class methods/constructors. The class
                // context (enclosing_class, this_type_stack) is not yet established,
                // so `this` would resolve to `any`, producing incorrect cache entries
                // (e.g., `{ ...this.method() }` cached as `{}`). The body will be
                // properly checked later during check_class_member with correct context.
                // The method's return type is already computed above (from annotation
                // or infer_return_type_from_body which snapshots/restores).
                let skip_body_check = !self.ctx.is_checking_statements && is_method_or_constructor;
                // Save outer generator's yield collection state (for nested generators)
                let saved_yield_collection =
                    std::mem::take(&mut self.ctx.generator_yield_operand_types);
                let saved_had_ts7057 = std::mem::replace(&mut self.ctx.generator_had_ts7057, false);
                if !skip_body_check {
                    self.check_statement_with_request(body, &TypingRequest::NONE);
                }
                if let Some(guard) = diag_guard {
                    guard.rollback(&mut self.ctx);
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
                if is_generator && !has_type_annotation {
                    let yield_types = std::mem::take(&mut self.ctx.generator_yield_operand_types);
                    // Compute inferred yield type; skip widening when contextual
                    // yield type preserved literals (`yield 0` stays `0` not `number`).
                    let inferred_yield = if yield_types.is_empty() {
                        TypeId::NEVER
                    } else {
                        self.ctx.types.factory().union(yield_types)
                    };
                    let widened = if early_yield_type.is_some() {
                        inferred_yield
                    } else {
                        self.widen_literal_type(inferred_yield)
                    };
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
                    // Suppress TS7055 when TS7057 was already emitted for a yield
                    // in this generator. tsc emits one or the other, not both:
                    // TS7057 covers the per-expression case; TS7055 is for the
                    // function-level "yield type is implicitly any" case.
                    if final_yield == TypeId::ANY
                        && self.ctx.no_implicit_any()
                        && !self.is_js_file()
                        && !self.ctx.generator_had_ts7057
                        // Suppress TS7055/TS7025 when the generator has a contextual
                        // yield type — the yield type is implicitly provided by context,
                        // not truly missing.
                        && early_yield_type.is_none()
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
                self.ctx.generator_had_ts7057 = saved_had_ts7057;

                // Restore control flow context
                self.ctx.iteration_depth = saved_cf_context.0;
                self.ctx.switch_depth = saved_cf_context.1;
                self.ctx.label_stack.truncate(saved_cf_context.2);
                self.ctx.had_outer_loop = saved_cf_context.3;
            }
            self.pop_return_type();
            self.ctx.pop_yield_type();
            self.ctx.pop_generator_next_type();

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
            self.ctx.function_owned_this_stack.pop();
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
                // For TReturn, prefer the body-inferred return type (from return
                // statements) over the contextual type, falling back to void when
                // neither is available. This ensures generic type parameters like
                // `Ret` in `<Ret>(f: () => Generator<never, Ret, never>)` get
                // inferred from the generator body's return statements.
                let yield_t = final_generator_yield_type.unwrap_or(TypeId::ANY);
                // For TReturn: prefer the body-inferred return type when concrete (not
                // UNKNOWN/VOID/UNDEFINED), falling back to the contextual TReturn, then VOID.
                // Also exclude `any` when a contextual TReturn is available - the inferred
                // `any` may be stale from an earlier type-checking pass where the variable
                // `next = yield ...` was typed as `any` before contextual types were set.
                let body_return_t = if return_type != TypeId::UNKNOWN
                    && return_type != TypeId::VOID
                    && return_type != TypeId::UNDEFINED
                    && !(return_type == TypeId::ANY && early_gen_return_type.is_some())
                {
                    Some(return_type)
                } else {
                    None
                };
                let return_t = body_return_t
                    .or(early_gen_return_type)
                    .unwrap_or(TypeId::VOID);
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
            params: if inherited_contextual_generics {
                contextual_signature_shape
                    .as_ref()
                    .map(|shape| shape.params.clone())
                    .unwrap_or(params)
            } else {
                params
            },
            this_type,
            return_type: final_return_type,
            type_predicate,
            is_constructor: js_constructor_instance_type.is_some(),
            is_method: false,
        };
        let function_type = self.ctx.types.factory().function(shape);

        self.pop_type_parameters(jsdoc_type_param_updates);
        self.pop_type_parameters(contextual_signature_type_param_updates);
        self.pop_type_parameters(type_param_updates);
        self.pop_type_parameters(enclosing_type_param_updates);

        return_with_cleanup!(function_type)
    }
}

#[cfg(test)]
mod tests;
