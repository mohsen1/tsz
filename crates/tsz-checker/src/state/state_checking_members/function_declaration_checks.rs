//! Function declaration checking logic extracted from the statement callback bridge.
//!
//! This module implements the comprehensive checking of function declarations,
//! including parameter validation, return type checking, generator/async semantics,
//! JSDoc integration, and overload compatibility.

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, node::NodeAccess, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn collect_untyped_this_references_in_function_body(
        &self,
        node_idx: NodeIndex,
        refs: &mut Vec<NodeIndex>,
        is_root: bool,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if !is_root
            && matches!(
                node.kind,
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION
            )
        {
            return;
        }

        if node.kind == SyntaxKind::ThisKeyword as u16 {
            refs.push(node_idx);
            return;
        }

        for child in self.ctx.arena.get_children(node_idx) {
            self.collect_untyped_this_references_in_function_body(child, refs, false);
        }
    }

    /// Comprehensive function declaration checking, used as the callback implementation
    /// for `StatementCheckCallbacks::check_function_declaration`.
    ///
    /// Covers: declaration-level checks, parameter validation (TS2300/TS2369/TS2370/TS7006),
    /// return type analysis (TS2355/TS2366/TS2534/TS7010/TS7011), async/generator semantics
    /// (TS2505/TS2705/TS7055), JSDoc integration (TS8024/TS8030), and overload compatibility
    /// (TS2394).
    pub(crate) fn check_function_declaration_callback(&mut self, func_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };

        // Delegate to DeclarationChecker for function declaration-specific checks
        // (only for actual function declarations, not expressions/arrows)
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
            checker.check_function_declaration(func_idx);
        }

        // TS2394: Check overload compatibility for function declarations with a body.
        // When a function has overload signatures followed by an implementation,
        // verify the implementation signature is compatible with all overloads.
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = self.ctx.arena.get_function(node)
            && func.body.is_some()
        {
            self.check_overload_compatibility(func_idx);
        }

        // Validate indexed access types in the return type annotation of ambient
        // (declare) function declarations. This catches TS2536 for patterns like
        // `T[keyof T]["foo"]` in return types. Limited to declare functions to
        // avoid triggering side effects from type evaluation in function bodies.
        if let Some(func) = self.ctx.arena.get_function(node)
            && self
                .ctx
                .arena
                .has_modifier(&func.modifiers, tsz_scanner::SyntaxKind::DeclareKeyword)
            && func.type_annotation != tsz_parser::parser::NodeIndex::NONE
        {
            self.check_type_node(func.type_annotation);
        }

        // TS8030: In JS files, if a function declaration has a @type tag that doesn't
        // resolve to a callable type, emit "The type of a function declaration must match
        // the function's signature." TSC points the error at the type expression inside
        // the @type tag (e.g. at "MyClass" in `@type {MyClass}`).
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION && self.is_js_file()
            && let Some(jsdoc) = self.get_jsdoc_for_function(func_idx)
                && let Some(type_expr) = Self::jsdoc_extract_type_tag_expr(&jsdoc)
                // Skip types that are syntactically callable (arrow functions, function types,
                // or generic signatures) — these may not resolve but are valid function types.
                && !Self::is_syntactically_callable_type(&type_expr)
                && self
                    .jsdoc_callable_type_annotation_for_function(func_idx)
                    .is_none()
        {
            // Find the position of the type expression inside the JSDoc comment
            if let Some(sf) = self.source_file_data_for_node(func_idx) {
                let source_text = sf.text.to_string();
                let comments = sf.comments.clone();
                if let Some((_, jsdoc_start)) =
                    self.try_jsdoc_with_ancestor_walk_and_pos(func_idx, &comments, &source_text)
                {
                    // Find "@type {expr}" in the source text starting from jsdoc_start
                    let jsdoc_text = &source_text[jsdoc_start as usize..];
                    if let Some(type_tag_off) = jsdoc_text.find("@type") {
                        let after_type = &jsdoc_text[type_tag_off + 5..];
                        if let Some(brace_off) = after_type.find('{') {
                            let expr_start =
                                jsdoc_start + type_tag_off as u32 + 5 + brace_off as u32 + 1;
                            // Find matching close brace
                            let after_brace = &after_type[brace_off + 1..];
                            let expr_end = after_brace
                                .find('}')
                                .map_or(type_expr.len() as u32, |i| i as u32);
                            self.ctx.error(
                                    expr_start,
                                    expr_end,
                                    "The type of a function declaration must match the function's signature.".to_string(),
                                    8030,
                                );
                        }
                    }
                }
            }
        }

        // Re-get node after DeclarationChecker borrows ctx
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };

        let Some(func) = self.ctx.arena.get_function(node) else {
            return;
        };

        // TS1212/TS1213/TS1214: Reserved word used as function name in strict mode
        // Skip in ambient contexts (declare namespace/module/global).
        if self.is_strict_mode_for_node(func_idx)
            && !self.ctx.is_ambient_declaration(func_idx)
            && func.name.is_some()
            && let Some(func_name_node) = self.ctx.arena.get(func.name)
            && let Some(ident) = self.ctx.arena.get_identifier(func_name_node)
            && crate::state_checking::is_strict_mode_reserved_name(&ident.escaped_text)
        {
            self.emit_strict_mode_reserved_word_error(func.name, &ident.escaped_text, true);
        }

        // Error 1183: An implementation cannot be declared in ambient contexts
        // Check if function has 'declare' modifier but also has a body
        // Point error at the body (opening brace) to match tsc
        if func.body.is_some() && self.has_declare_modifier(&func.modifiers) {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                func.body,
                "An implementation cannot be declared in ambient contexts.",
                diagnostic_codes::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
            );
        }

        // Check for missing Promise global type when function is async (TS2318)
        // TSC emits this at the start of the file when Promise is not available
        // Only check for non-generator async functions (async generators use AsyncGenerator, not Promise)
        if func.is_async && !func.asterisk_token {
            self.check_global_promise_available();
        }

        // TS1221 / TS1222
        // TSC anchors these errors at the `*` asterisk token, not the whole function node.
        // Suppressed when file has parse errors (tsc's grammarErrorOnNode).
        if func.asterisk_token && !self.has_syntax_parse_errors() {
            use crate::diagnostics::diagnostic_codes;
            let is_ambient = self.has_declare_modifier(&func.modifiers)
                || self.ctx.is_declaration_file()
                || self.is_ambient_declaration(func_idx);

            if is_ambient {
                self.emit_generator_error_at_asterisk(
                    func.name,
                    func_idx,
                    "Generators are not allowed in an ambient context.",
                    diagnostic_codes::GENERATORS_ARE_NOT_ALLOWED_IN_AN_AMBIENT_CONTEXT,
                );
            } else if func.body.is_none() {
                self.emit_generator_error_at_asterisk(
                    func.name,
                    func_idx,
                    "An overload signature cannot be declared as a generator.",
                    diagnostic_codes::AN_OVERLOAD_SIGNATURE_CANNOT_BE_DECLARED_AS_A_GENERATOR,
                );
            }
        }

        let (_type_params, type_param_updates) = self.push_type_parameters(&func.type_parameters);

        self.check_duplicate_type_parameters(&func.type_parameters);
        self.check_type_parameters_for_missing_names(&func.type_parameters);

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&func.type_parameters, func_idx);
        if func.type_parameters.is_none() {
            self.check_unused_jsdoc_template_type_params(func_idx);
        }

        // TS7008: Check type parameter constraints for implicit any members
        // e.g., `function f<T extends { x, y }>(t: T)` — members x, y need type annotations
        if let Some(ref type_params) = func.type_parameters {
            for &param_idx in &type_params.nodes {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_type_parameter(param_node)
                    && param.constraint.is_some()
                {
                    self.check_type_for_parameter_properties(param.constraint);
                }
            }
        }

        // TS1212/TS1213/TS1214: Reserved word used as type parameter name in strict mode
        if !self.ctx.is_ambient_declaration(func_idx) {
            self.check_strict_mode_reserved_type_parameter_names(
                &func.type_parameters,
                func_idx,
                false,
            );
        }

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors
        self.check_parameter_properties(&func.parameters.nodes);

        // Check for duplicate parameter names (TS2300)
        self.check_duplicate_parameters(&func.parameters, func.body.is_some());
        if !self.ctx.is_ambient_declaration(func_idx) {
            self.check_strict_mode_reserved_parameter_names(
                &func.parameters.nodes,
                func_idx,
                false,
            );
        }

        // Check for required parameters following optional parameters (TS1016)
        self.check_parameter_ordering(&func.parameters, Some(func_idx));
        self.check_binding_pattern_optionality(
            &func.parameters.nodes,
            func.body.is_some(),
            Some(func_idx),
        );

        // Check that rest parameters have array types (TS2370)
        self.check_rest_parameter_types(&func.parameters.nodes);

        // Check return type annotation for parameter properties in function types
        if func.type_annotation.is_some() {
            self.check_type_for_parameter_properties(func.type_annotation);
            // Check for undefined type names in return type
            self.check_type_for_missing_names(func.type_annotation);
        }

        // Check parameter type annotations for parameter properties
        for &param_idx in &func.parameters.nodes {
            if let Some(param_node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_parameter(param_node)
                && param.type_annotation.is_some()
            {
                self.check_type_for_parameter_properties(param.type_annotation);
                // Check for undefined type names in parameter type
                self.check_type_for_missing_names(param.type_annotation);
            }
        }

        // Extract JSDoc for function declarations to suppress TS7006/TS7010 in JS files
        let func_decl_jsdoc = self.get_jsdoc_for_function(func_idx);

        // TS7006: Check parameters for implicit any.
        // For closures (function expressions and arrow functions), TS7006 is already
        // handled by get_type_of_function which has contextual type information.
        // Only check here for actual function declarations.
        let is_closure = matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION
        );
        if !is_closure {
            self.check_function_parameter_implicit_any(
                func_idx,
                &func.parameters.nodes,
                &func_decl_jsdoc,
            );
        }

        // TS8024: Check that JSDoc @param tag names match actual function parameters.
        // Only for JS files (tsc does not emit TS8024 for TS files).
        // Only for non-closures: arrow functions/function expressions in nested positions
        // may find JSDoc from a parent function via parent chain walking.
        let should_check_closure_jsdoc_param_names = is_closure
            && self.is_js_file()
            && func.parameters.nodes.is_empty()
            && self.body_has_arguments_reference(func.body);
        if self.is_js_file()
            && let Some(ref jsdoc) = self.find_jsdoc_for_function(func_idx)
            && !jsdoc.contains("@callback")
            && (!is_closure || should_check_closure_jsdoc_param_names)
        {
            self.check_jsdoc_param_tag_names(jsdoc, &func.parameters.nodes, func_idx);
            if !is_closure {
                self.check_jsdoc_param_function_types_missing_return_type(jsdoc, func_idx);
            }
        }
        if !is_closure && self.is_js_file() {
            self.check_jsdoc_overload_implicit_any_return(func_idx);
        }

        // Check parameter initializer placement for implementation vs signature (TS2371)
        self.check_non_impl_parameter_initializers(
            &func.parameters.nodes,
            self.has_declare_modifier(&func.modifiers),
            func.body.is_some(),
        );

        // Check function body if present
        let has_type_annotation = func.type_annotation.is_some();
        // For JS files without explicit type annotations, check for JSDoc @type
        // providing a function type. If found, extract its return type so that
        // return statements are checked against it (TS2322/TS2355).
        let jsdoc_return_type = if !has_type_annotation && !is_closure && self.is_js_file() {
            self.jsdoc_type_annotation_for_node(func_idx)
                .and_then(|jsdoc_func_type| {
                    crate::query_boundaries::assignability::get_function_return_type(
                        self.ctx.types,
                        jsdoc_func_type,
                    )
                })
        } else {
            None
        };
        let has_jsdoc_return_type = jsdoc_return_type.is_some();
        if func.body.is_some() {
            self.check_function_body(
                func_idx,
                func,
                node,
                is_closure,
                has_type_annotation,
                has_jsdoc_return_type,
                jsdoc_return_type,
                &func_decl_jsdoc,
            );
        } else if self.ctx.no_implicit_any() && !has_type_annotation {
            let is_ambient =
                self.has_declare_modifier(&func.modifiers) || self.ctx.is_declaration_file();
            if let Some(func_name) = self.get_function_name_from_node(func_idx) {
                let name_node = func.name.into_option();
                if is_ambient {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        name_node.unwrap_or(func_idx),
                        diagnostic_codes::WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                        &[&func_name, "any"],
                    );
                } else {
                    // TS7010 for bodyless declaration signatures (TS2391 sibling error)
                    // in non-ambient contexts.
                    self.maybe_report_implicit_any_return(
                        Some(func_name),
                        name_node,
                        TypeId::ANY,
                        false,
                        false,
                        func_idx,
                    );
                }
            }
        }

        // Check overload compatibility: implementation must be assignable to all overloads
        // This is the function implementation validation (TS2394)
        if func.body.is_some() {
            // Only check for implementations (functions with bodies)
            self.check_overload_modifier_consistency(func_idx);
            self.check_overload_compatibility(func_idx);
        }

        self.pop_type_parameters(type_param_updates);
    }

    /// Check parameters for implicit any (TS7006) in function declarations.
    fn check_function_parameter_implicit_any(
        &mut self,
        func_idx: NodeIndex,
        params: &[NodeIndex],
        func_decl_jsdoc: &Option<String>,
    ) {
        // Pre-extract ordered @param names for positional matching with binding patterns
        let jsdoc_param_names: Vec<String> = func_decl_jsdoc
            .as_ref()
            .map(|jsdoc| {
                Self::extract_jsdoc_param_names(jsdoc)
                    .into_iter()
                    .map(|(name, _)| name)
                    .collect()
            })
            .unwrap_or_default();
        for (pi, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            // Check if JSDoc provides a @param type for this parameter,
            // or if the parameter has an inline /** @type {T} */ annotation,
            // or if the function has a @type tag declaring its full type.
            let has_callable_jsdoc_type = func_decl_jsdoc
                .as_ref()
                .is_some_and(|jsdoc| Self::jsdoc_type_tag_declares_callable(jsdoc))
                || self
                    .jsdoc_callable_type_annotation_for_function(func_idx)
                    .is_some();
            let has_jsdoc_param = if param.type_annotation.is_none() {
                let from_func_jsdoc = if let Some(jsdoc) = func_decl_jsdoc {
                    let pname = self.effective_jsdoc_param_name(param.name, &jsdoc_param_names, pi);
                    Self::jsdoc_has_param_type(jsdoc, &pname)
                        || has_callable_jsdoc_type
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
            self.maybe_report_implicit_any_parameter(param, has_jsdoc_param, pi);
        }
    }

    /// Check the body of a function declaration, including return type inference,
    /// generator/async semantics, and return path analysis.
    #[allow(clippy::too_many_arguments)]
    fn check_function_body(
        &mut self,
        func_idx: NodeIndex,
        func: &tsz_parser::parser::node::FunctionData,
        _node: &tsz_parser::parser::node::Node,
        is_closure: bool,
        has_type_annotation: bool,
        has_jsdoc_return_type: bool,
        jsdoc_return_type: Option<TypeId>,
        func_decl_jsdoc: &Option<String>,
    ) {
        let mut return_type = if has_type_annotation {
            self.get_type_from_type_node(func.type_annotation)
        } else if let Some(jsdoc_ret) = jsdoc_return_type {
            jsdoc_ret
        } else {
            // Use UNKNOWN to enforce strict checking
            TypeId::UNKNOWN
        };

        // Extract this type from explicit `this` parameter EARLY
        // so that infer_return_type_from_body has the correct `this` context
        // (prevents false TS2683 during return type inference)
        let mut pushed_this_type = false;
        if let Some(&first_param) = func.parameters.nodes.first()
            && let Some(param_node) = self.ctx.arena.get(first_param)
            && let Some(param) = self.ctx.arena.get_parameter(param_node)
        {
            // Check if parameter name is "this"
            // Must check both ThisKeyword and Identifier("this") to match parser behavior
            let is_this = if let Some(name_node) = self.ctx.arena.get(param.name) {
                if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                    true
                } else if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    ident.escaped_text == "this"
                } else {
                    false
                }
            } else {
                false
            };
            if is_this && param.type_annotation.is_some() {
                let this_type = self.get_type_from_type_node(param.type_annotation);
                self.ctx.this_type_stack.push(this_type);
                self.ctx.function_owned_this_stack.push(func_idx);
                pushed_this_type = true;
            }
        }
        if !pushed_this_type
            && self.is_js_file()
            && let Some(jsdoc_callable_type) =
                self.jsdoc_callable_type_annotation_for_function(func_idx)
        {
            let ctx_helper = tsz_solver::ContextualTypeContext::with_expected_and_options(
                self.ctx.types,
                jsdoc_callable_type,
                self.ctx.compiler_options.no_implicit_any,
            );
            if let Some(this_type) = ctx_helper.get_this_type() {
                self.ctx.this_type_stack.push(this_type);
                self.ctx.function_owned_this_stack.push(func_idx);
                pushed_this_type = true;
            }
        }

        let owns_untyped_this_binding = matches!(
            _node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION | syntax_kind_ext::FUNCTION_EXPRESSION
        ) && !pushed_this_type
            && !self.enclosing_function_has_contextual_this_type(func_idx)
            && !self.is_js_file();
        let masked_outer_this = if owns_untyped_this_binding && self.current_this_type().is_some() {
            self.ctx.this_type_stack.pop()
        } else {
            None
        };

        // Cache parameter types from annotations (so for-of binding uses correct types)
        // and then infer for any remaining unknown parameters using contextual information.
        // For closures (function expressions / arrow functions), parameter types are
        // already properly cached by get_type_of_function with contextual typing.
        // Calling cache_parameter_types(None) here would overwrite contextually-typed
        // parameters (e.g., `data` in `() => data => data.map(s => ...)`) with ANY,
        // causing downstream callback contextual typing to break (false TS7006).
        if !is_closure {
            self.cache_parameter_types(&func.parameters.nodes, None);
        }
        self.infer_parameter_types_from_context(&func.parameters.nodes);

        // Check that parameter default values are assignable to declared types (TS2322)
        self.check_parameter_initializers(&func.parameters.nodes);

        // Check binding element defaults in destructuring parameters (TS2322)
        // e.g., function f({ show: x = v => v }: Show) — validate x's default
        self.check_parameter_binding_pattern_defaults(&func.parameters.nodes);

        if !has_type_annotation && !has_jsdoc_return_type {
            // Suppress definite assignment errors during return type inference.
            // The function body will be checked again below, and that's when
            // we want to emit TS2454 errors to avoid duplicates.
            let prev_suppress = self.ctx.suppress_definite_assignment_errors;
            self.ctx.suppress_definite_assignment_errors = true;
            return_type = self.infer_return_type_from_body(func_idx, func.body, None);
            self.ctx.suppress_definite_assignment_errors = prev_suppress;

            self.maybe_report_non_serializable_inferred_declaration_type(
                func_idx,
                func.name,
                self.get_function_name_from_node(func_idx)
                    .as_deref()
                    .unwrap_or(""),
                return_type,
            );
        }

        // TS7010/TS7011 (implicit any return) for function declarations.
        // For closures (function expressions and arrow functions), TS7010/TS7011
        // is already handled by get_type_of_function which has contextual return
        // type information. Only check here for actual function declarations.
        if !is_closure {
            self.check_function_implicit_any_return(
                func_idx,
                func,
                func_decl_jsdoc,
                has_type_annotation,
                return_type,
            );
        }

        // TS2677: Check that a type predicate's type is assignable to its parameter's type.
        if has_type_annotation {
            self.check_function_decl_type_predicate_assignability(func_idx, func);
        }

        // TS2705: Async function must return Promise
        self.check_async_return_type_is_promise(
            has_type_annotation,
            func.is_async,
            func.asterisk_token,
            return_type,
            func.type_annotation,
        );

        // Enter async context for await expression checking
        if func.is_async {
            self.ctx.enter_async_context();
        }

        // For generator functions with explicit return type (Generator<Y, R, N> or AsyncGenerator<Y, R, N>),
        // return statements should be checked against TReturn (R), not the full Generator type.
        // This matches TypeScript's behavior where `return x` in a generator checks `x` against TReturn.
        let is_generator = func.asterisk_token;
        let has_declared_return = has_type_annotation || has_jsdoc_return_type;
        let body_return_type = if is_generator && has_type_annotation {
            self.compute_generator_body_return_type(func, return_type)
        } else if func.is_async && has_type_annotation {
            // Unwrap Promise<T> to T for async function return type checking.
            // The function body returns T, which gets auto-wrapped in a Promise.
            self.unwrap_promise_type(return_type).unwrap_or(return_type)
        } else if has_declared_return {
            return_type
        } else {
            // When the return type was purely inferred from the body (no
            // annotation), push ANY so that check_return_statement skips
            // the circular assignability check.  Checking a return expression
            // against its own inferred type can produce false positives when
            // contextual typing widens inner types differently.
            TypeId::ANY
        };

        self.push_return_type(body_return_type);

        // For generator functions, push the contextual yield type so that
        // yield expressions can contextually type their operand.
        let contextual_yield_type = if is_generator && has_type_annotation {
            self.get_generator_yield_type_argument(return_type)
        } else {
            None
        };
        self.ctx.push_yield_type(contextual_yield_type);

        // Push the generator next type for yield result typing.
        let contextual_next_type = if is_generator && has_type_annotation {
            self.get_generator_next_type_argument(return_type)
        } else {
            None
        };
        self.ctx.push_generator_next_type(contextual_next_type);

        // Save and reset control flow context (function body creates new context)
        let saved_cf_context = (
            self.ctx.iteration_depth,
            self.ctx.switch_depth,
            self.ctx.label_stack.len(),
            self.ctx.had_outer_loop,
        );
        // If we were in a loop/switch, or already had an outer loop, mark it
        if self.ctx.iteration_depth > 0 || self.ctx.switch_depth > 0 || self.ctx.had_outer_loop {
            self.ctx.had_outer_loop = true;
        }
        self.ctx.iteration_depth = 0;
        self.ctx.switch_depth = 0;
        self.ctx.function_depth += 1;
        // Note: we don't truncate label_stack here - labels remain visible
        // but function_depth is used to detect crosses over function boundary

        // Save outer generator's yield collection state (for nested generators)
        let saved_yield_collection = std::mem::take(&mut self.ctx.generator_yield_operand_types);
        let saved_had_ts7057 = std::mem::replace(&mut self.ctx.generator_had_ts7057, false);

        self.check_statement_with_request(func.body, &TypingRequest::NONE);

        if masked_outer_this.is_some() && self.ctx.no_implicit_this() {
            let mut refs = Vec::new();
            self.collect_untyped_this_references_in_function_body(func.body, &mut refs, true);
            for this_idx in refs {
                let already_reported = self.ctx.diagnostics.iter().any(|diag| {
                    diag.code
                        == crate::diagnostics::diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION
                        && self
                            .ctx
                            .arena
                            .get(this_idx)
                            .is_some_and(|node| diag.start == node.pos && diag.length == node.end - node.pos)
                });
                if !already_reported {
                    self.error_at_node(
                        this_idx,
                        crate::diagnostics::diagnostic_messages::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                        crate::diagnostics::diagnostic_codes::THIS_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION,
                    );
                }
            }
        }

        // For annotated generators, check that Generator<TYield, any, any>
        // is assignable to the declared return type.
        if is_generator && has_type_annotation {
            self.check_generator_return_type_assignability(
                func.is_async,
                contextual_yield_type,
                return_type,
                func.type_annotation,
            );
        }

        // For unannotated generators, emit TS7055 if yield type is implicit 'any'
        // (suppressed when TS7057 was already emitted — tsc emits one, not both).
        if is_generator && !has_type_annotation {
            self.check_generator_implicit_yield_type(func_idx, func);
        }

        // Restore outer generator's yield collection state
        self.ctx.generator_yield_operand_types = saved_yield_collection;
        self.ctx.generator_had_ts7057 = saved_had_ts7057;

        // Restore control flow context
        self.ctx.iteration_depth = saved_cf_context.0;
        self.ctx.switch_depth = saved_cf_context.1;
        self.ctx.function_depth -= 1;
        self.ctx.label_stack.truncate(saved_cf_context.2);
        self.ctx.had_outer_loop = saved_cf_context.3;

        // Check return path analysis (TS2355/TS2366/TS2534/TS7030)
        self.check_function_return_paths(
            func_idx,
            func,
            return_type,
            has_type_annotation,
            has_declared_return,
        );

        self.pop_return_type();
        self.ctx.pop_yield_type();
        self.ctx.pop_generator_next_type();

        // Exit async context
        if func.is_async {
            self.ctx.exit_async_context();
        }

        if pushed_this_type {
            self.ctx.this_type_stack.pop();
            self.ctx.function_owned_this_stack.pop();
        }
        if let Some(outer_this) = masked_outer_this {
            self.ctx.this_type_stack.push(outer_this);
        }
    }

    /// TS2677: Check that a type predicate's type is assignable to its parameter's type
    /// for function declarations (not function type nodes).
    fn check_function_decl_type_predicate_assignability(
        &mut self,
        _func_idx: NodeIndex,
        func: &tsz_parser::parser::node::FunctionData,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        if func.type_annotation.is_none() {
            return;
        }

        // Find the TypePredicate node in the return type annotation
        let type_ann_idx = func.type_annotation;
        let Some(type_ann_node) = self.ctx.arena.get(type_ann_idx) else {
            return;
        };

        // The return type annotation should be a TypePredicate node
        if type_ann_node.kind != syntax_kind_ext::TYPE_PREDICATE {
            return;
        }

        let Some(pred_data) = self.ctx.arena.get_type_predicate(type_ann_node) else {
            return;
        };

        if pred_data.type_node.is_none() {
            return;
        }

        let Some(pred_name_node) = self.ctx.arena.get(pred_data.parameter_name) else {
            return;
        };
        let Some(pred_name_ident) = self.ctx.arena.get_identifier(pred_name_node) else {
            return;
        };
        let predicate_name = pred_name_ident.escaped_text.clone();

        // Resolve the predicate type
        let mut predicate_type = self.get_type_from_type_node(pred_data.type_node);

        // When the predicate type was parsed from `?T` (prefix ?), the parser recovers
        // just `T` but tsc semantically treats it as `T | null | undefined`. Detect this
        // by checking if the type node's position matches a nullable-type parse error.
        // Only `?`-related errors trigger widening; `!`-related errors should not.
        if let Some(type_node) = self.ctx.arena.get(pred_data.type_node) {
            let type_pos = type_node.pos;
            if self
                .ctx
                .nullable_type_parse_error_positions
                .contains(&type_pos)
            {
                predicate_type = self.ctx.types.factory().union(vec![
                    predicate_type,
                    TypeId::NULL,
                    TypeId::UNDEFINED,
                ]);
            }
        }

        // Find the parameter type
        let mut param_type = None;
        for &param_idx in &func.parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            let param_name_matches = self
                .ctx
                .arena
                .get(param_data.name)
                .and_then(|n| self.ctx.arena.get_identifier(n))
                .is_some_and(|ident| ident.escaped_text == predicate_name);
            if param_name_matches {
                if param_data.type_annotation.is_some() {
                    param_type = Some(self.get_type_from_type_node(param_data.type_annotation));
                }
                break;
            }
        }

        let Some(param_type) = param_type else {
            return;
        };

        if !self.is_assignable_to(predicate_type, param_type) {
            if let Some(type_node) = self.ctx.arena.get(pred_data.type_node) {
                self.ctx.error(
                    type_node.pos,
                    type_node.end - type_node.pos,
                    "A type predicate's type must be assignable to its parameter's type."
                        .to_string(),
                    2677,
                );
            }
        }
    }

    /// Check TS7010/TS7011 (implicit any return) for function declarations.
    fn check_function_implicit_any_return(
        &mut self,
        func_idx: NodeIndex,
        func: &tsz_parser::parser::node::FunctionData,
        func_decl_jsdoc: &Option<String>,
        has_type_annotation: bool,
        return_type: TypeId,
    ) {
        let has_jsdoc_return = func_decl_jsdoc
            .as_ref()
            .is_some_and(|j| Self::jsdoc_has_type_annotations(j));
        if func.is_async || has_jsdoc_return {
            return;
        }
        let func_name = self.get_function_name_from_node(func_idx);
        let name_node = func.name.into_option();
        let has_wrapped_circular_return = !has_type_annotation
            && return_type == TypeId::ANY
            && self.function_has_wrapped_self_call_in_return_expression(func_idx, func.body);
        if has_wrapped_circular_return
            && self.ctx.no_implicit_any()
            && !self.has_syntax_parse_errors()
            && !self.is_js_file()
        {
            use crate::diagnostics::diagnostic_codes;

            if let Some(name) = func_name {
                self.error_at_node_msg(
                    name_node.unwrap_or(func_idx),
                    diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                    &[&name],
                );
            } else {
                self.error_at_node_msg(
                    func_idx,
                    diagnostic_codes::FUNCTION_IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_A,
                    &[],
                );
            }
        } else {
            self.maybe_report_implicit_any_return(
                func_name,
                name_node,
                return_type,
                has_type_annotation,
                false,
                func_idx,
            );
        }
    }

    /// Compute the body return type for generator functions, handling TS2505 and
    /// generator protocol compatibility.
    fn compute_generator_body_return_type(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        return_type: TypeId,
    ) -> TypeId {
        // TS2505: A generator cannot have a 'void' type annotation.
        // When void is used, emit this specific error and skip the generator
        // protocol check to avoid cascading TS2322 errors.
        if return_type == TypeId::VOID {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                func.type_annotation,
                "A generator cannot have a 'void' type annotation.",
                diagnostic_codes::A_GENERATOR_CANNOT_HAVE_A_VOID_TYPE_ANNOTATION,
            );
            TypeId::ANY // Use ANY to suppress return statement checks
        } else {
            // Ensure the annotated return type is actually compatible with the Generator protocol.
            let generator_base = if func.is_async {
                self.resolve_lib_type_by_name("AsyncGenerator")
                    .unwrap_or(TypeId::ERROR)
            } else {
                self.resolve_lib_type_by_name("Generator")
                    .unwrap_or(TypeId::ERROR)
            };
            if generator_base != TypeId::ERROR {
                let any_gen = self
                    .ctx
                    .types
                    .factory()
                    .application(generator_base, vec![TypeId::ANY, TypeId::ANY, TypeId::ANY]);

                // Fast path: if the return type is already recognized as a valid generator type,
                // we don't need to do the complex structural subtyping check that fails due to overloads.
                // If it is not (e.g. `number`), we run the check to emit the TS2322 assignability error.
                let has_direct_builtin_generator_annotation = self
                    .ctx
                    .arena
                    .get(func.type_annotation)
                    .and_then(|node| self.ctx.arena.get_type_ref(node))
                    .and_then(|type_ref| self.node_text(type_ref.type_name))
                    .is_some_and(|name| {
                        matches!(
                            name.as_str(),
                            "Generator"
                                | "AsyncGenerator"
                                | "Iterator"
                                | "AsyncIterator"
                                | "IterableIterator"
                                | "AsyncIterableIterator"
                        )
                    });
                if !has_direct_builtin_generator_annotation
                    && self
                        .get_generator_return_type_argument(return_type)
                        .is_none()
                {
                    self.check_assignable_or_report(any_gen, return_type, func.type_annotation);
                }
            }

            self.get_generator_return_type_argument(return_type)
                .unwrap_or(return_type)
        }
    }

    /// Check for implicit yield type in unannotated generators (TS7055).
    fn check_generator_implicit_yield_type(
        &mut self,
        func_idx: NodeIndex,
        func: &tsz_parser::parser::node::FunctionData,
    ) {
        let yield_types = std::mem::take(&mut self.ctx.generator_yield_operand_types);

        let inferred_yield = if yield_types.is_empty() {
            TypeId::NEVER // No yields -> never
        } else {
            self.ctx.types.factory().union(yield_types)
        };

        let widened = self.widen_literal_type(inferred_yield);
        // When strictNullChecks is off, tsc widens null/undefined yield types
        // to `any` — but NOT when the yield type is purely `undefined` (from
        // bare `yield;` or `yield undefined`).  In that case the generator's
        // yield type is `void`, which is intentional and must not trigger
        // TS7055.
        let final_yield = if !self.ctx.strict_null_checks()
            && tsz_solver::type_queries::is_only_null_or_undefined(self.ctx.types, widened)
            && inferred_yield != TypeId::UNDEFINED
            && widened != TypeId::UNDEFINED
        {
            TypeId::ANY
        } else {
            widened
        };

        if final_yield == TypeId::ANY
            && self.ctx.no_implicit_any()
            && !self.is_js_file()
            && !self.ctx.generator_had_ts7057
        {
            // TS7055: Named generator's yield type is implicitly 'any'
            use crate::diagnostics::diagnostic_codes;
            if let Some(func_name) = self.get_function_name_from_node(func_idx) {
                self.error_at_node_msg(
                    func.name,
                    diagnostic_codes::WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_YIELD_TYPE,
                    &[&func_name, "any"],
                );
            } else {
                // TS7025: Unnamed generator expression (unlikely for function declarations)
                self.error_at_node_msg(
                    func_idx,
                    diagnostic_codes::GENERATOR_IMPLICITLY_HAS_YIELD_TYPE_CONSIDER_SUPPLYING_A_RETURN_TYPE_ANNOTATION,
                    &["any"],
                );
            }
        }
    }

    /// Check return path analysis: TS2355, TS2366, TS2534, TS7030.
    fn check_function_return_paths(
        &mut self,
        func_idx: NodeIndex,
        func: &tsz_parser::parser::node::FunctionData,
        return_type: TypeId,
        has_type_annotation: bool,
        has_declared_return: bool,
    ) {
        let is_async = func.is_async;
        let is_generator = func.asterisk_token;
        let mut check_return_type =
            self.return_type_for_implicit_return_check(return_type, is_async, is_generator);
        // For async functions, if we couldn't unwrap Promise<T> (e.g. lib files not loaded),
        // fall back to the annotation syntax. If it looks like Promise<...>, suppress TS2355
        // since we can't verify the inner type anyway.
        if is_async
            && check_return_type == return_type
            && has_type_annotation
            && self.return_type_annotation_looks_like_promise(func.type_annotation)
        {
            check_return_type = TypeId::VOID;
        }
        let check_explicit_return_paths = has_declared_return;
        let requires_return = if check_explicit_return_paths {
            self.requires_return_value(check_return_type)
        } else {
            false
        };
        let check_no_implicit_returns = self.ctx.no_implicit_returns();
        let need_return_flow_scan =
            (check_explicit_return_paths && requires_return) || check_no_implicit_returns;
        let (has_return, falls_through) = if need_return_flow_scan {
            (
                self.body_has_return_with_value(func.body),
                self.function_body_falls_through(func.body),
            )
        } else {
            (false, false)
        };

        // TS2534: A function returning 'never' cannot have a reachable end point.
        // This must be checked before TS2355/TS2366 because `never` return type
        // causes `requires_return` to be false (never doesn't require a return VALUE,
        // but it does require the function to never complete normally).
        if has_declared_return
            && check_return_type == TypeId::NEVER
            && self.function_body_falls_through(func.body)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            let error_node = if has_type_annotation {
                func.type_annotation
            } else if func.name.is_some() {
                func.name
            } else {
                func_idx
            };
            self.error_at_node(
                error_node,
                diagnostic_messages::A_FUNCTION_RETURNING_NEVER_CANNOT_HAVE_A_REACHABLE_END_POINT,
                diagnostic_codes::A_FUNCTION_RETURNING_NEVER_CANNOT_HAVE_A_REACHABLE_END_POINT,
            );
        }

        if check_explicit_return_paths && requires_return && falls_through {
            // For JSDoc @type, the error node is the function name/node
            // (there's no separate type annotation node in the AST).
            let error_node = if has_type_annotation {
                func.type_annotation
            } else {
                // JSDoc: use function name if available, otherwise function itself
                if func.name.is_some() {
                    func.name
                } else {
                    func_idx
                }
            };
            if !has_return {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    error_node,
                    "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                    diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                );
            } else {
                // TS2366: always emit when return type doesn't include undefined
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    error_node,
                    diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                    diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                );
            }
        } else if check_no_implicit_returns
            && has_return
            && falls_through
            && !self.should_skip_no_implicit_return_check(check_return_type, has_declared_return)
        {
            // TS7030: noImplicitReturns - not all code paths return a value
            // TSC points TS7030 to: return type annotation > function name > node itself
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            let error_node = if func.type_annotation.is_some() {
                func.type_annotation
            } else if func.name.is_some() {
                func.name
            } else {
                func.body
            };
            self.error_at_node(
                error_node,
                diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
            );
        }
    }
}
