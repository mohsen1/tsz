use std::rc::Rc;

use crate::state::CheckerState;
use crate::statements::StatementCheckCallbacks;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

/// Implementation of `StatementCheckCallbacks` for `CheckerState`.
///
/// This provides the actual implementation of statement checking operations
/// that `StatementChecker` delegates to. Each callback method calls the
/// corresponding method on `CheckerState`.
impl<'a> StatementCheckCallbacks for CheckerState<'a> {
    fn arena(&self) -> &tsz_parser::parser::node::NodeArena {
        self.ctx.arena
    }

    fn get_type_of_node(&mut self, idx: NodeIndex) -> TypeId {
        CheckerState::get_type_of_node(self, idx)
    }

    fn get_type_of_node_no_narrowing(&mut self, idx: NodeIndex) -> TypeId {
        let prev = self.ctx.skip_flow_narrowing;
        self.ctx.skip_flow_narrowing = true;
        let ty = CheckerState::get_type_of_node(self, idx);
        self.ctx.skip_flow_narrowing = prev;
        ty
    }

    fn check_variable_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_variable_statement(self, stmt_idx);
    }

    fn check_variable_declaration_list(&mut self, list_idx: NodeIndex) {
        CheckerState::check_variable_declaration_list(self, list_idx);
    }

    fn check_variable_declaration(&mut self, decl_idx: NodeIndex) {
        CheckerState::check_variable_declaration(self, decl_idx);
    }

    fn check_return_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_return_statement(self, stmt_idx);
    }

    fn check_function_implementations(&mut self, stmts: &[NodeIndex]) {
        CheckerState::check_function_implementations(self, stmts);
    }

    fn check_function_declaration(&mut self, func_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };

        // Delegate to DeclarationChecker for function declaration-specific checks
        // (only for actual function declarations, not expressions/arrows)
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
            checker.check_function_declaration(func_idx);
        }

        // Re-get node after DeclarationChecker borrows ctx
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };

        let Some(func) = self.ctx.arena.get_function(node) else {
            return;
        };

        // TS1100: 'arguments' and 'eval' are invalid in function names in strict contexts.
        if self.is_strict_mode_for_node(func_idx)
            && func.name.is_some()
            && let Some(func_name_node) = self.ctx.arena.get(func.name)
            && let Some(ident) = self.ctx.arena.get_identifier(func_name_node)
        {
            if ident.escaped_text == "arguments" || ident.escaped_text == "eval" {
                self.error_at_node_msg(
                    func.name,
                    crate::diagnostics::diagnostic_codes::INVALID_USE_OF_IN_STRICT_MODE,
                    &[&ident.escaped_text],
                );
            }

            // TS1212/TS1213/TS1214: Reserved word used as function name in strict mode
            if crate::state_checking::is_strict_mode_reserved_name(&ident.escaped_text) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                if self.ctx.enclosing_class.is_some() {
                    let message = format_message(
                        diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                        &[&ident.escaped_text],
                    );
                    self.error_at_node(
                        func.name,
                        &message,
                        diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                    );
                } else if self.ctx.binder.is_external_module() {
                    let message = format_message(
                        diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                        &[&ident.escaped_text],
                    );
                    self.error_at_node(
                        func.name,
                        &message,
                        diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                    );
                } else {
                    let message = format_message(
                        diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                        &[&ident.escaped_text],
                    );
                    self.error_at_node(
                        func.name,
                        &message,
                        diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                    );
                }
            }
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
        if func.asterisk_token {
            use crate::diagnostics::diagnostic_codes;
            let is_ambient = self.has_declare_modifier(&func.modifiers)
                || self.ctx.file_name.ends_with(".d.ts")
                || self.is_ambient_declaration(func_idx);

            if is_ambient {
                self.error_at_node(
                    func_idx,
                    "Generators are not allowed in an ambient context.",
                    diagnostic_codes::GENERATORS_ARE_NOT_ALLOWED_IN_AN_AMBIENT_CONTEXT,
                );
            } else if func.body.is_none() {
                self.error_at_node(
                    func_idx,
                    "An overload signature cannot be declared as a generator.",
                    diagnostic_codes::AN_OVERLOAD_SIGNATURE_CANNOT_BE_DECLARED_AS_A_GENERATOR,
                );
            }
        }

        let (_type_params, type_param_updates) = self.push_type_parameters(&func.type_parameters);

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&func.type_parameters, func_idx);

        // Check for parameter properties (error 2369)
        // Parameter properties are only allowed in constructors
        self.check_parameter_properties(&func.parameters.nodes);

        // Check for duplicate parameter names (TS2300)
        self.check_duplicate_parameters(&func.parameters, func.body.is_some());
        if !self.has_declare_modifier(&func.modifiers) && !self.ctx.file_name.ends_with(".d.ts") {
            self.check_strict_mode_reserved_parameter_names(
                &func.parameters.nodes,
                func_idx,
                false,
            );
        }

        // Check for required parameters following optional parameters (TS1016)
        self.check_parameter_ordering(&func.parameters);
        self.check_binding_pattern_optionality(&func.parameters.nodes, func.body.is_some());

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
            for (pi, &param_idx) in func.parameters.nodes.iter().enumerate() {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                // Check if JSDoc provides a @param type for this parameter,
                // or if the parameter has an inline /** @type {T} */ annotation,
                // or if the function has a @type tag declaring its full type.
                let has_jsdoc_param = if param.type_annotation.is_none() {
                    let from_func_jsdoc = if let Some(ref jsdoc) = func_decl_jsdoc {
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
                self.maybe_report_implicit_any_parameter(param, has_jsdoc_param, pi);
            }
        }

        // Check parameter initializer placement for implementation vs signature (TS2371)
        self.check_non_impl_parameter_initializers(
            &func.parameters.nodes,
            self.has_declare_modifier(&func.modifiers),
            func.body.is_some(),
        );

        // Check function body if present
        let has_type_annotation = func.type_annotation.is_some();
        if func.body.is_some() {
            let mut return_type = if has_type_annotation {
                self.get_type_from_type_node(func.type_annotation)
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
                    if name_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16 {
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
                    pushed_this_type = true;
                }
            }

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

            if !has_type_annotation {
                // Suppress definite assignment errors during return type inference.
                // The function body will be checked again below, and that's when
                // we want to emit TS2454 errors to avoid duplicates.
                let prev_suppress = self.ctx.suppress_definite_assignment_errors;
                self.ctx.suppress_definite_assignment_errors = true;
                return_type = self.infer_return_type_from_body(func_idx, func.body, None);
                self.ctx.suppress_definite_assignment_errors = prev_suppress;
            }

            // TS7010/TS7011 (implicit any return) for function declarations.
            // For closures (function expressions and arrow functions), TS7010/TS7011
            // is already handled by get_type_of_function which has contextual return
            // type information. Only check here for actual function declarations.
            if !is_closure {
                let has_jsdoc_return = func_decl_jsdoc
                    .as_ref()
                    .is_some_and(|j| Self::jsdoc_has_type_annotations(j));
                if !func.is_async && !has_jsdoc_return {
                    let func_name = self.get_function_name_from_node(func_idx);
                    let name_node = (func.name.is_some()).then_some(func.name);
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

            // TS2705: Async function must return Promise
            // Only check if there's an explicit return type annotation that is NOT Promise
            // Skip this check if the return type is ERROR or the annotation looks like Promise
            // Note: Async generators (async function*) return AsyncGenerator, not Promise
            if func.is_async && !func.asterisk_token && has_type_annotation {
                let should_emit_ts2705 = !self.is_promise_type(return_type)
                    && return_type != TypeId::ERROR
                    && !self.return_type_annotation_looks_like_promise(func.type_annotation);

                if should_emit_ts2705 {
                    use crate::context::ScriptTarget;
                    use crate::diagnostics::diagnostic_codes;

                    // For ES5/ES3 targets, emit TS1055 instead of TS2705
                    let is_es5_or_lower = matches!(
                        self.ctx.compiler_options.target,
                        ScriptTarget::ES3 | ScriptTarget::ES5
                    );

                    let type_name = self.format_type(return_type);
                    if is_es5_or_lower {
                        self.error_at_node_msg(
                            func.type_annotation,
                            diagnostic_codes::TYPE_IS_NOT_A_VALID_ASYNC_FUNCTION_RETURN_TYPE_IN_ES5_BECAUSE_IT_DOES_NOT_REFER,
                            &[&type_name],
                        );
                    } else {
                        // TS1064: For ES6+ targets, the return type must be Promise<T>
                        self.error_at_node_msg(
                            func.type_annotation,
                            diagnostic_codes::THE_RETURN_TYPE_OF_AN_ASYNC_FUNCTION_OR_METHOD_MUST_BE_THE_GLOBAL_PROMISE_T_TYPE,
                            &[&type_name],
                        );
                    }
                }
            }

            // Enter async context for await expression checking
            if func.is_async {
                self.ctx.enter_async_context();
            }

            // For generator functions with explicit return type (Generator<Y, R, N> or AsyncGenerator<Y, R, N>),
            // return statements should be checked against TReturn (R), not the full Generator type.
            // This matches TypeScript's behavior where `return x` in a generator checks `x` against TReturn.
            let is_generator = func.asterisk_token;
            let body_return_type = if is_generator && has_type_annotation {
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
                    if self
                        .get_generator_return_type_argument(return_type)
                        .is_none()
                    {
                        self.check_assignable_or_report(any_gen, return_type, func.type_annotation);
                    }
                }

                self.get_generator_return_type_argument(return_type)
                    .unwrap_or(return_type)
            } else if func.is_async && has_type_annotation {
                // Unwrap Promise<T> to T for async function return type checking.
                // The function body returns T, which gets auto-wrapped in a Promise.
                self.unwrap_promise_type(return_type).unwrap_or(return_type)
            } else {
                return_type
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

            // Save and reset control flow context (function body creates new context)
            let saved_cf_context = (
                self.ctx.iteration_depth,
                self.ctx.switch_depth,
                self.ctx.label_stack.len(),
                self.ctx.had_outer_loop,
            );
            // If we were in a loop/switch, or already had an outer loop, mark it
            if self.ctx.iteration_depth > 0 || self.ctx.switch_depth > 0 || self.ctx.had_outer_loop
            {
                self.ctx.had_outer_loop = true;
            }
            self.ctx.iteration_depth = 0;
            self.ctx.switch_depth = 0;
            self.ctx.function_depth += 1;
            // Note: we don't truncate label_stack here - labels remain visible
            // but function_depth is used to detect crosses over function boundary
            self.check_statement(func.body);
            // Restore control flow context
            self.ctx.iteration_depth = saved_cf_context.0;
            self.ctx.switch_depth = saved_cf_context.1;
            self.ctx.function_depth -= 1;
            self.ctx.label_stack.truncate(saved_cf_context.2);
            self.ctx.had_outer_loop = saved_cf_context.3;

            // Check for error 2355: function with return type must return a value
            // Only check if there's an explicit return type annotation
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
            let check_explicit_return_paths = has_type_annotation;
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

            if check_explicit_return_paths && requires_return && falls_through {
                if !has_return {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        func.type_annotation,
                        "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                        diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                    );
                } else if self.ctx.strict_null_checks() {
                    // TS2366: Only emit with strictNullChecks
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        func.type_annotation,
                        diagnostic_messages::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                        diagnostic_codes::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINE,
                    );
                }
            } else if check_no_implicit_returns
                && has_return
                && falls_through
                && !self
                    .should_skip_no_implicit_return_check(check_return_type, has_type_annotation)
            {
                // TS7030: noImplicitReturns - not all code paths return a value
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                let error_node = if func.name.is_some() {
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

            self.pop_return_type();
            self.ctx.pop_yield_type();

            // Exit async context
            if func.is_async {
                self.ctx.exit_async_context();
            }

            if pushed_this_type {
                self.ctx.this_type_stack.pop();
            }
        } else if self.ctx.no_implicit_any() && !has_type_annotation {
            let is_ambient =
                self.has_declare_modifier(&func.modifiers) || self.ctx.file_name.ends_with(".d.ts");
            if let Some(func_name) = self.get_function_name_from_node(func_idx) {
                let name_node = (func.name.is_some()).then_some(func.name);
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
            self.check_overload_compatibility(func_idx);
        }

        self.pop_type_parameters(type_param_updates);
    }

    fn check_class_declaration(&mut self, class_idx: NodeIndex) {
        // Note: DeclarationChecker::check_class_declaration handles TS2564 (property
        // initialization) but CheckerState::check_class_declaration also handles it
        // more comprehensively (with parameter properties, derived classes, etc.).
        // We skip the DeclarationChecker delegation for classes to avoid duplicate
        // TS2564 emissions. DeclarationChecker::check_class_declaration is tested
        // independently via its own test suite.
        CheckerState::check_class_declaration(self, class_idx);
    }

    fn check_interface_declaration(&mut self, iface_idx: NodeIndex) {
        // Delegate to DeclarationChecker first
        let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
        checker.check_interface_declaration(iface_idx);

        // Continue with comprehensive interface checking in CheckerState
        CheckerState::check_interface_declaration(self, iface_idx);
    }

    fn check_import_declaration(&mut self, import_idx: NodeIndex) {
        CheckerState::check_import_declaration(self, import_idx);
    }

    fn check_import_equals_declaration(&mut self, import_idx: NodeIndex) {
        CheckerState::check_import_equals_declaration(self, import_idx);
    }

    fn check_export_declaration(&mut self, export_idx: NodeIndex) {
        if let Some(export_decl) = self.ctx.arena.get_export_decl_at(export_idx) {
            if export_decl.is_default_export && self.is_inside_namespace_declaration(export_idx) {
                self.error_at_node(
                    export_idx,
                    crate::diagnostics::diagnostic_messages::A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE,
                    crate::diagnostics::diagnostic_codes::A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE,
                );
                // tsc does not further resolve the exported expression when
                // the export default is invalid in a namespace context.
                return;
            }

            // TS1194: `export { ... }` / `export ... from` forms are not valid inside namespaces.
            let is_reexport_syntax = export_decl.module_specifier.is_some()
                || self
                    .ctx
                    .arena
                    .get(export_decl.export_clause)
                    .is_some_and(|n| n.kind == syntax_kind_ext::NAMED_EXPORTS);
            if is_reexport_syntax && self.is_inside_namespace_declaration(export_idx) {
                let report_idx = if export_decl.module_specifier.is_some() {
                    export_decl.module_specifier
                } else {
                    export_idx
                };
                self.error_at_node(
                    report_idx,
                    crate::diagnostics::diagnostic_messages::EXPORT_DECLARATIONS_ARE_NOT_PERMITTED_IN_A_NAMESPACE,
                    crate::diagnostics::diagnostic_codes::EXPORT_DECLARATIONS_ARE_NOT_PERMITTED_IN_A_NAMESPACE,
                );
            }

            // TS2823: Import attributes require specific module options
            self.check_import_attributes_module_option(export_decl.attributes);

            // Check module specifier for unresolved modules (TS2792)
            if export_decl.module_specifier.is_some() {
                self.check_export_module_specifier(export_idx);
            }

            // Check the wrapped declaration
            if export_decl.export_clause.is_some() {
                let clause_idx = export_decl.export_clause;
                let mut expected_type = None;
                let mut prev_context = None;
                if export_decl.is_default_export {
                    expected_type = self.jsdoc_type_annotation_for_node(export_idx);
                    if let Some(et) = expected_type {
                        prev_context = self.ctx.contextual_type;
                        self.ctx.contextual_type = Some(et);
                    }
                }

                self.check_statement(clause_idx);

                if let Some(et) = expected_type {
                    let actual_type = self.get_type_of_node(clause_idx);
                    self.ctx.contextual_type = prev_context;
                    self.check_assignable_or_report(actual_type, et, clause_idx);
                    if let Some(expr_node) = self.ctx.arena.get(clause_idx)
                        && expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    {
                        self.check_object_literal_excess_properties(actual_type, et, clause_idx);
                    }
                }

                if export_decl.module_specifier.is_none()
                    && self
                        .ctx
                        .arena
                        .get(clause_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::NAMED_EXPORTS)
                {
                    self.check_local_named_exports(clause_idx);
                }
            }
        }
    }

    fn check_type_alias_declaration(&mut self, type_alias_idx: NodeIndex) {
        // Keep type-node validation and indexed-access diagnostics wired via CheckerState.
        CheckerState::check_type_alias_declaration(self, type_alias_idx);

        if let Some(node) = self.ctx.arena.get(type_alias_idx) {
            // Continue with comprehensive type alias checking
            if let Some(type_alias) = self.ctx.arena.get_type_alias(node) {
                // TS1212: Check type alias name for strict mode reserved words
                self.check_strict_mode_reserved_name_at(type_alias.name, type_alias_idx);

                // TS2457: Type alias name cannot be 'undefined'
                if let Some(name_node) = self.ctx.arena.get(type_alias.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text == "undefined"
                {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node(
                        type_alias.name,
                        "Type alias name cannot be 'undefined'.",
                        diagnostic_codes::TYPE_ALIAS_NAME_CANNOT_BE,
                    );
                }
                let (_params, updates) = self.push_type_parameters(&type_alias.type_parameters);
                // Check for unused type parameters (TS6133)
                self.check_unused_type_params(&type_alias.type_parameters, type_alias_idx);
                self.check_type_for_missing_names(type_alias.type_node);
                self.check_type_for_parameter_properties(type_alias.type_node);
                self.pop_type_parameters(updates);
            }
        }
    }
    fn check_enum_duplicate_members(&mut self, enum_idx: NodeIndex) {
        // TS1042: async modifier cannot be used on enum declarations
        if let Some(node) = self.ctx.arena.get(enum_idx)
            && let Some(enum_data) = self.ctx.arena.get_enum(node)
        {
            self.check_async_modifier_on_declaration(&enum_data.modifiers);
            // TS1212: Check enum name for strict mode reserved words
            self.check_strict_mode_reserved_name_at(enum_data.name, enum_idx);
        }

        // Delegate to DeclarationChecker first
        let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
        checker.check_enum_declaration(enum_idx);

        // Continue with enum duplicate members checking
        CheckerState::check_enum_duplicate_members(self, enum_idx);
    }

    fn check_module_declaration(&mut self, module_idx: NodeIndex) {
        if let Some(node) = self.ctx.arena.get(module_idx) {
            // Delegate to DeclarationChecker first
            let mut checker = crate::declarations::DeclarationChecker::new(&mut self.ctx);
            checker.check_module_declaration(module_idx);

            // Check module body and modifiers
            if let Some(module) = self.ctx.arena.get_module(node) {
                // TS1212: Check module/namespace name for strict mode reserved words
                self.check_strict_mode_reserved_name_at(module.name, module_idx);

                // TS1042: async modifier cannot be used on module/namespace declarations
                self.check_async_modifier_on_declaration(&module.modifiers);

                let is_ambient = self.has_declare_modifier(&module.modifiers);
                if module.body.is_some() {
                    self.check_module_body(module.body);
                }

                // TS1038: Check for 'declare' modifiers inside ambient module/namespace
                // TS1039: Check for initializers in ambient contexts
                // Even if we don't fully check the body, we still need to emit these errors
                if is_ambient && module.body.is_some() {
                    self.check_declare_modifiers_in_ambient_body(module.body);
                    self.check_initializers_in_ambient_body(module.body);

                    // TS2300/TS2309: Check for duplicate export assignments even in ambient modules
                    // TS2300: Check for duplicate import aliases even in ambient modules
                    // TS2303: Check for circular import aliases in ambient modules
                    // Need to extract statements from module body
                    if let Some(body_node) = self.ctx.arena.get(module.body)
                        && body_node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK
                        && let Some(block) = self.ctx.arena.get_module_block(body_node)
                        && let Some(ref statements) = block.statements
                    {
                        self.check_export_assignment(&statements.nodes);
                        self.check_import_alias_duplicates(&statements.nodes);
                        // Check import equals declarations for circular imports (TS2303)
                        for &stmt_idx in &statements.nodes {
                            if let Some(stmt_node) = self.ctx.arena.get(stmt_idx)
                                && stmt_node.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                                    self.check_import_equals_declaration(stmt_idx);
                                }
                        }
                    }
                }

                // TS2300: Check for duplicate import aliases in non-ambient modules too
                // This handles namespace { import X = ...; import X = ...; }
                if !is_ambient
                    && module.body.is_some()
                    && let Some(body_node) = self.ctx.arena.get(module.body)
                    && body_node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK
                    && let Some(block) = self.ctx.arena.get_module_block(body_node)
                    && let Some(ref statements) = block.statements
                {
                    self.check_import_alias_duplicates(&statements.nodes);
                }
            }
        }
    }

    fn check_await_expression(&mut self, expr_idx: NodeIndex) {
        CheckerState::check_await_expression(self, expr_idx);
    }

    fn check_for_await_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_for_await_statement(self, stmt_idx);
    }

    fn check_truthy_or_falsy(&mut self, node_idx: NodeIndex) {
        CheckerState::check_truthy_or_falsy(self, node_idx);
    }

    fn is_true_condition(&self, condition_idx: NodeIndex) -> bool {
        CheckerState::is_true_condition(self, condition_idx)
    }

    fn is_false_condition(&self, condition_idx: NodeIndex) -> bool {
        CheckerState::is_false_condition(self, condition_idx)
    }

    fn report_unreachable_statement(&mut self, stmt_idx: NodeIndex) {
        if !self.ctx.is_unreachable {
            return;
        }

        // Delegate to a helper that checks should_skip
        let should_skip = if let Some(node) = self.ctx.arena.get(stmt_idx) {
            node.kind == syntax_kind_ext::EMPTY_STATEMENT
                || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                || node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || node.kind == syntax_kind_ext::MODULE_DECLARATION
                || node.kind == syntax_kind_ext::BLOCK
                || CheckerState::is_var_without_initializer(self, stmt_idx, node)
        } else {
            false
        };

        if !should_skip && !self.ctx.has_reported_unreachable {
            if self.ctx.compiler_options.allow_unreachable_code != Some(false) {
                return;
            }
            self.error_at_node(
                stmt_idx,
                crate::diagnostics::diagnostic_messages::UNREACHABLE_CODE_DETECTED,
                crate::diagnostics::diagnostic_codes::UNREACHABLE_CODE_DETECTED,
            );
            self.ctx.has_reported_unreachable = true;
        }
    }

    fn check_for_in_expression_type(&mut self, expr_type: TypeId, expression: NodeIndex) {
        CheckerState::check_for_in_expression_type(self, expr_type, expression);
    }

    fn assign_for_in_of_initializer_types(
        &mut self,
        decl_list_idx: NodeIndex,
        loop_var_type: TypeId,
        is_for_in: bool,
    ) {
        CheckerState::assign_for_in_of_initializer_types(
            self,
            decl_list_idx,
            loop_var_type,
            is_for_in,
        );
    }

    fn for_of_element_type(&mut self, expr_type: TypeId) -> TypeId {
        CheckerState::for_of_element_type(self, expr_type)
    }

    fn check_for_of_iterability(
        &mut self,
        expr_type: TypeId,
        expr_idx: NodeIndex,
        await_modifier: bool,
    ) {
        CheckerState::check_for_of_iterability(self, expr_type, expr_idx, await_modifier);
    }

    fn check_for_in_of_expression_initializer(
        &mut self,
        initializer: NodeIndex,
        element_type: TypeId,
        is_for_of: bool,
        has_await_modifier: bool,
    ) {
        CheckerState::check_for_in_of_expression_initializer(
            self,
            initializer,
            element_type,
            is_for_of,
            has_await_modifier,
        );
    }

    fn check_for_in_destructuring_pattern(&mut self, initializer: NodeIndex) {
        CheckerState::check_for_in_destructuring_pattern(self, initializer);
    }

    fn check_for_in_expression_destructuring(&mut self, initializer: NodeIndex) {
        CheckerState::check_for_in_expression_destructuring(self, initializer);
    }

    fn check_statement(&mut self, stmt_idx: NodeIndex) {
        // This calls back to the main check_statement which will delegate to StatementChecker
        CheckerState::check_statement(self, stmt_idx);
    }

    fn check_switch_exhaustiveness(
        &mut self,
        _stmt_idx: NodeIndex,
        expression: NodeIndex,
        case_block: NodeIndex,
        has_default: bool,
    ) {
        // If there's a default clause, the switch is syntactically exhaustive
        if has_default {
            return;
        }

        // Get the discriminant type
        let discriminant_type = self.get_type_of_node(expression);

        // Create a FlowAnalyzer to check exhaustiveness
        let analyzer =
            crate::control_flow::FlowAnalyzer::new(self.ctx.arena, self.ctx.binder, self.ctx.types)
                .with_reference_match_cache(&self.ctx.flow_reference_match_cache)
                .with_type_environment(Rc::clone(&self.ctx.type_environment));

        // Create a narrowing context
        let narrowing = tsz_solver::NarrowingContext::new(self.ctx.types);

        // Calculate the "no-match" type (what type the discriminant would have
        // if none of the case clauses match)
        let _no_match_type = analyzer.narrow_by_default_switch_clause(
            discriminant_type,
            expression,
            case_block,
            expression, // target is the discriminant itself
            &narrowing,
        );

        // The no_match_type is used for narrowing within the flow analyzer.
        // The actual "not all code paths return" error (TS2366) should be
        // reported at the FUNCTION level in control flow analysis, not here.
        //
        // This is because:
        // 1. Code after the switch might handle missing cases
        // 2. The return type might accept undefined (e.g., number | undefined)
        // 3. Exhaustiveness must be checked in the context of the entire function
        //
        // The FlowAnalyzer uses no_match_type to correctly narrow types within
        // subsequent code blocks, but the error emission happens elsewhere.
    }

    fn check_switch_case_comparable(
        &mut self,
        switch_type: TypeId,
        case_type: TypeId,
        switch_expr: NodeIndex,
        case_expr: NodeIndex,
    ) {
        // Skip if either type is error/any/unknown to avoid cascade errors
        if switch_type == TypeId::ERROR
            || case_type == TypeId::ERROR
            || switch_type == TypeId::ANY
            || case_type == TypeId::ANY
            || switch_type == TypeId::UNKNOWN
            || case_type == TypeId::UNKNOWN
        {
            return;
        }

        // Use literal type for the switch expression if available, since
        // get_type_of_node widens literals (e.g., 12 -> number).
        // tsc's checkExpression preserves literal types for comparability checks.
        let effective_switch_type = self
            .literal_type_from_initializer(switch_expr)
            .unwrap_or(switch_type);

        // Use literal type for the case expression if available, since
        // get_type_of_node widens literals (e.g., "c" -> string).
        let effective_case_type = self
            .literal_type_from_initializer(case_expr)
            .unwrap_or(case_type);

        // Check if the types are comparable (assignable in either direction).
        // Types are comparable if they overlap — i.e., at least one direction works.
        // For example, "a" is comparable to "a" | "b" | "c" because "a" <: union.
        // TypeScript unconditionally allows 'null' and 'undefined' as the case type.
        let is_comparable = effective_case_type == tsz_solver::TypeId::NULL
            || effective_case_type == tsz_solver::TypeId::UNDEFINED
            || self.is_type_comparable_to(effective_case_type, effective_switch_type);

        if !is_comparable {
            // TS2678: Type 'X' is not comparable to type 'Y'
            if let Some(loc) = self.get_source_location(case_expr) {
                let case_str = self.format_type(effective_case_type);
                let switch_str = self.format_type(effective_switch_type);
                use crate::diagnostics::{
                    Diagnostic, diagnostic_codes, diagnostic_messages, format_message,
                };
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_COMPARABLE_TO_TYPE,
                    &[&case_str, &switch_str],
                );
                self.ctx.diagnostics.push(Diagnostic::error(
                    self.ctx.file_name.clone(),
                    loc.start,
                    loc.length(),
                    message,
                    diagnostic_codes::TYPE_IS_NOT_COMPARABLE_TO_TYPE,
                ));
            }
        }
    }

    fn check_with_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_with_statement(self, stmt_idx);
    }

    fn check_break_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_break_statement(self, stmt_idx);
    }

    fn check_continue_statement(&mut self, stmt_idx: NodeIndex) {
        CheckerState::check_continue_statement(self, stmt_idx);
    }

    fn is_unreachable(&self) -> bool {
        self.ctx.is_unreachable
    }

    fn set_unreachable(&mut self, value: bool) {
        self.ctx.is_unreachable = value;
    }

    fn has_reported_unreachable(&self) -> bool {
        self.ctx.has_reported_unreachable
    }

    fn set_reported_unreachable(&mut self, value: bool) {
        self.ctx.has_reported_unreachable = value;
    }

    fn statement_falls_through(&mut self, stmt_idx: NodeIndex) -> bool {
        CheckerState::statement_falls_through(self, stmt_idx)
    }

    fn enter_iteration_statement(&mut self) {
        self.ctx.iteration_depth += 1;
    }

    fn leave_iteration_statement(&mut self) {
        self.ctx.iteration_depth = self.ctx.iteration_depth.saturating_sub(1);
    }

    fn enter_switch_statement(&mut self) {
        self.ctx.switch_depth += 1;
    }

    fn leave_switch_statement(&mut self) {
        self.ctx.switch_depth = self.ctx.switch_depth.saturating_sub(1);
    }

    fn save_and_reset_control_flow_context(&mut self) -> (u32, u32, bool) {
        let saved = (
            self.ctx.iteration_depth,
            self.ctx.switch_depth,
            self.ctx.had_outer_loop,
        );
        // If we were in a loop/switch, or already had an outer loop, mark it
        if self.ctx.iteration_depth > 0 || self.ctx.switch_depth > 0 || self.ctx.had_outer_loop {
            self.ctx.had_outer_loop = true;
        }
        self.ctx.iteration_depth = 0;
        self.ctx.switch_depth = 0;
        saved
    }

    fn restore_control_flow_context(&mut self, saved: (u32, u32, bool)) {
        self.ctx.iteration_depth = saved.0;
        self.ctx.switch_depth = saved.1;
        self.ctx.had_outer_loop = saved.2;
    }

    fn enter_labeled_statement(&mut self, label: String, is_iteration: bool) {
        self.ctx.label_stack.push(crate::context::LabelInfo {
            name: label,
            is_iteration,
            function_depth: self.ctx.function_depth,
        });
    }

    fn leave_labeled_statement(&mut self) {
        self.ctx.label_stack.pop();
    }

    fn get_node_text(&self, idx: NodeIndex) -> Option<String> {
        // For identifiers (like label names), get the identifier data and resolve the text
        let ident = self.ctx.arena.get_identifier_at(idx)?;
        // Use the resolved text from the identifier data
        Some(self.ctx.arena.resolve_identifier_text(ident).to_string())
    }

    fn check_declaration_in_statement_position(&mut self, stmt_idx: NodeIndex) {
        use tsz_parser::parser::node_flags;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        // TS1156: '{0}' declarations can only be declared inside a block.
        // This fires when a const/let/interface/type declaration appears as
        // the body of a control flow statement (if/while/for) without braces.
        let decl_kind = match node.kind {
            syntax_kind_ext::INTERFACE_DECLARATION => Some("interface"),
            syntax_kind_ext::VARIABLE_STATEMENT => {
                // Check the VariableDeclarationList for const/let flags
                if let Some(var_data) = self.ctx.arena.get_variable(node) {
                    let list_idx = var_data
                        .declarations
                        .nodes
                        .first()
                        .copied()
                        .unwrap_or(NodeIndex::NONE);
                    if let Some(list_node) = self.ctx.arena.get(list_idx) {
                        let flags = list_node.flags as u32;
                        // Check USING first — AWAIT_USING (6) includes CONST bit
                        if (flags & node_flags::AWAIT_USING) == node_flags::AWAIT_USING {
                            Some("await using")
                        } else if flags & node_flags::USING != 0 {
                            Some("using")
                        } else if flags & node_flags::CONST != 0 {
                            Some("const")
                        } else if flags & node_flags::LET != 0 {
                            Some("let")
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(kind_name) = decl_kind {
            let msg = format!("'{kind_name}' declarations can only be declared inside a block.");
            self.error_at_node(
                stmt_idx,
                &msg,
                crate::diagnostics::diagnostic_codes::DECLARATIONS_CAN_ONLY_BE_DECLARED_INSIDE_A_BLOCK,
            );
        }
    }

    fn check_label_on_declaration(&mut self, label_idx: NodeIndex, statement_idx: NodeIndex) {
        // TS1344: In strict mode with target >= ES2015, a label is not allowed
        // before declaration statements or variable statements.
        // This matches TSC's checkStrictModeLabeledStatement in binder.ts.
        if !self.ctx.compiler_options.target.supports_es2015() {
            return;
        }
        if !self.is_strict_mode_for_node(label_idx) {
            return;
        }

        let Some(stmt_node) = self.ctx.arena.get(statement_idx) else {
            return;
        };

        // isDeclarationStatement || isVariableStatement
        let is_declaration_or_variable = matches!(
            stmt_node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::VARIABLE_STATEMENT
        );

        if is_declaration_or_variable {
            self.error_at_node(
                label_idx,
                "'A label is not allowed here.",
                crate::diagnostics::diagnostic_codes::A_LABEL_IS_NOT_ALLOWED_HERE,
            );
        }
    }
}
