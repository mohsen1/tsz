//! Function type resolution helpers: JSDoc type predicates, enclosing type
//! parameter resolution, arguments object detection, contextual rest
//! parameter evaluation, and async/return completeness checks.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{TypeId, TypeParamInfo};

impl<'a> CheckerState<'a> {
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

    pub(crate) fn contextual_type_params_from_expected(
        &self,
        expected: TypeId,
    ) -> Option<Vec<TypeParamInfo>> {
        tsz_solver::type_queries::extract_contextual_type_params(self.ctx.types, expected)
    }

    pub(crate) fn push_contextual_type_parameter_infos(
        &mut self,
        type_params: &[TypeParamInfo],
    ) -> Vec<(String, Option<TypeId>, bool)> {
        let mut updates = Vec::with_capacity(type_params.len());
        let factory = self.ctx.types.factory();

        for info in type_params {
            let name = self.ctx.types.resolve_atom_ref(info.name).to_string();
            let mut shadowed_class_param = false;
            if let Some(ref mut c) = self.ctx.enclosing_class
                && let Some(pos) = c.type_param_names.iter().position(|x| *x == name)
            {
                c.type_param_names.remove(pos);
                shadowed_class_param = true;
            }

            let type_id = factory.type_param(*info);
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous, shadowed_class_param));
        }

        updates
    }

    /// Check if a function body references the `arguments` object.
    /// Walks the AST recursively but stops at nested function boundaries.
    /// Used by JS files to determine if a function needs an implicit rest parameter.
    pub(crate) fn body_has_arguments_reference(&self, body: NodeIndex) -> bool {
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
    pub(crate) fn push_enclosing_type_parameters(
        &mut self,
        func_idx: NodeIndex,
    ) -> Vec<(String, Option<TypeId>, bool)> {
        use tsz_parser::parser::syntax_kind_ext;

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
    pub(crate) fn evaluate_contextual_rest_param_applications(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
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
        new_params
            .last_mut()
            .expect("new_params cloned from non-empty shape.params")
            .type_id = evaluated_rest;

        let new_shape = tsz_solver::FunctionShape {
            type_params: shape.type_params.clone(),
            params: new_params,
            this_type: shape.this_type,
            return_type: shape.return_type,
            type_predicate: shape.type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        };

        self.ctx.types.function(new_shape)
    }

    /// TS2705/TS2468: Check that the Promise constructor is available for async functions.
    /// Emits TS2468 (program-level) and TS2705 when Promise is missing from loaded libs.
    pub(crate) fn check_async_promise_constructor_availability(
        &mut self,
        is_async: bool,
        is_generator: bool,
        is_function_declaration: bool,
        has_type_annotation: bool,
        async_node_idx: NodeIndex,
        func_idx: NodeIndex,
    ) {
        if !is_async || is_generator {
            return;
        }
        // Only check for Promise constructor availability when targeting pre-ES2015,
        // where Promise is not a native global.
        if self.ctx.compiler_options.target.supports_es2015() {
            return;
        }
        let should_check_promise_constructor = !is_function_declaration || has_type_annotation;
        let missing_promise = !self.ctx.has_promise_constructor_in_scope();
        if !(should_check_promise_constructor && missing_promise) {
            return;
        }

        // Find the `async` keyword position for error anchoring.
        // For async arrow functions (no name node), the node `pos` starts at
        // the first parameter, not the `async` keyword. We scan backward
        // in the source to locate the keyword.
        let async_keyword_span = if async_node_idx.is_none() {
            // Arrow function — scan backward from node start to find `async`
            self.ctx.arena.get(func_idx).and_then(|n| {
                let sf = self.ctx.arena.source_files.first()?;
                let text = sf.text.as_bytes();
                let node_pos = n.pos as usize;
                // Scan backward over whitespace to find end of `async`
                let mut end = node_pos;
                while end > 0 && text.get(end - 1).copied() == Some(b' ') {
                    end -= 1;
                }
                // Check that the 5 chars before `end` are "async"
                if end >= 5 && &text[end - 5..end] == b"async" {
                    Some((end as u32 - 5, 5u32))
                } else {
                    None
                }
            })
        } else {
            None
        };

        // TS2468: Cannot find global value 'Promise'.
        // tsc emits this as a program-level diagnostic (no file location).
        if !is_function_declaration {
            let message =
                format_message(diagnostic_messages::CANNOT_FIND_GLOBAL_VALUE, &["Promise"]);
            self.error_program_level(message, diagnostic_codes::CANNOT_FIND_GLOBAL_VALUE);
        }

        // TS2705: anchored at the `async` keyword
        if let Some((start, length)) = async_keyword_span {
            self.error_at_position(
                start,
                length,
                diagnostic_messages::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
                diagnostic_codes::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
            );
        } else {
            let diagnostic_node = if async_node_idx.is_none() {
                func_idx
            } else {
                async_node_idx
            };
            self.error_at_node(
                diagnostic_node,
                diagnostic_messages::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
                diagnostic_codes::AN_ASYNC_FUNCTION_OR_METHOD_IN_ES5_REQUIRES_THE_PROMISE_CONSTRUCTOR_MAKE_SURE_YO,
            );
        }
    }

    /// TS2705/TS1055/TS1064: Check that an async function's return type annotation is Promise.
    /// Emits TS1055 (ES5) or TS1064 (ES6+) when the declared return type is not Promise<T>.
    pub(crate) fn check_async_return_type_is_promise(
        &mut self,
        has_type_annotation: bool,
        is_async: bool,
        is_generator: bool,
        return_type: TypeId,
        type_annotation: NodeIndex,
    ) {
        if !has_type_annotation || !is_async || is_generator {
            return;
        }
        use tsz_scanner::SyntaxKind;
        let should_emit = if self.is_global_promise_type(return_type) {
            // Return type is exactly the global Promise<T> - OK
            false
        } else if self.is_promise_type_through_alias(return_type) {
            // Return type is a type alias application that resolves to Promise
            // (e.g., `type MyPromise<T> = Promise<T>` with `declare var MyPromise: typeof Promise`).
            // The merged symbol prevents is_global_promise_type from recognizing it.
            false
        } else if self.is_non_promise_application_type(return_type) {
            // Return type is an Application with a non-Promise base (e.g., MyPromise<T>).
            // TSC requires exactly Promise<T>, not subclasses.
            true
        } else if return_type != TypeId::ERROR {
            // Return type evaluated to a non-Application form (e.g., Object).
            // Fall back to strict syntactic check: only suppress TS1064 if the
            // annotation literally says `Promise<...>`. TSC uses `isReferenceToType`
            // which requires exactly the global Promise — not subclasses like
            // `MyPromise`, not qualified names like `X.MyPromise`, not type aliases.
            !self.return_type_annotation_is_exactly_promise(type_annotation)
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
        if !should_emit {
            return;
        }
        use crate::context::ScriptTarget;
        // For ES5/ES3 targets, emit TS1055 instead of TS2705
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

    /// Check if a type is a type alias application that resolves to Promise.
    ///
    /// For example, `type PromiseAlias<T> = Promise<T>; async function f(): PromiseAlias<void>`
    /// -- the return type `PromiseAlias<void>` is an Application whose base is a type alias.
    /// This method resolves the alias body and checks if it references the global Promise type.
    ///
    /// This handles tsc's `isReferenceToType` semantics for TS1064, where type aliases
    /// that ultimately resolve to Promise<T> are accepted as valid async return types.
    /// It also handles merged symbols (e.g., `type MyPromise<T> = Promise<T>` combined
    /// with `declare var MyPromise: typeof Promise`) by finding the type alias declaration
    /// among the symbol's declarations.
    pub(crate) fn is_promise_type_through_alias(&mut self, type_id: TypeId) -> bool {
        use crate::query_boundaries::checkers::promise as query;
        use tsz_binder::symbol_flags;

        // Must be an Application type
        let query::PromiseTypeKind::Application { base, .. } =
            query::classify_promise_type(self.ctx.types, type_id)
        else {
            return false;
        };

        // Check if the base is a Lazy(DefId) pointing to a type alias
        let def_id = match query::classify_promise_type(self.ctx.types, base) {
            query::PromiseTypeKind::Lazy(def_id) => def_id,
            _ => return false,
        };

        let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Only handle type aliases (not classes/interfaces)
        if symbol.flags & symbol_flags::TYPE_ALIAS == 0 {
            return false;
        }

        // Get the alias body type using type_reference_symbol_type_with_params which
        // correctly handles merged symbols (e.g., `type MyPromise<T> = Promise<T>`
        // merged with `declare var MyPromise: typeof Promise`). It finds the type
        // alias declaration in the symbol's declarations list.
        let (body_type, _params) = self.type_reference_symbol_type_with_params(sym_id);
        if self.is_global_promise_type(body_type) {
            return true;
        }

        // The body might itself be an Application (e.g., `Promise<T>`)
        // Check if the Application base refers to the global Promise type
        if let query::PromiseTypeKind::Application {
            base: body_base, ..
        } = query::classify_promise_type(self.ctx.types, body_type)
        {
            // Check if the body's base is Promise
            return self.is_global_promise_type(body_base)
                || match query::classify_promise_type(self.ctx.types, body_base) {
                    query::PromiseTypeKind::Lazy(body_def_id) => {
                        if let Some(body_sym_id) = self.ctx.def_to_symbol_id(body_def_id)
                            && let Some(body_symbol) = self.ctx.binder.get_symbol(body_sym_id)
                        {
                            body_symbol.escaped_name == "Promise"
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
        }

        false
    }

    /// TS2366/TS2355/TS7030: Check that all code paths return a value when required.
    /// For function expressions and arrow functions with return type annotations.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_function_return_completeness(
        &mut self,
        is_function_declaration: bool,
        body: NodeIndex,
        func_idx: NodeIndex,
        annotated_return_type: Option<TypeId>,
        return_type: TypeId,
        has_type_annotation: bool,
        type_annotation: NodeIndex,
        function_is_generator: bool,
        name_node: Option<NodeIndex>,
        idx: NodeIndex,
    ) {
        if is_function_declaration || body.is_none() {
            return;
        }
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };
        // Class methods and constructors have their return completeness checked
        // by ambient_signature_checks.rs during the class checking phase, where
        // enclosing_class is properly set. Skip them here to avoid false
        // positives during the type building phase when enclosing_class is not
        // yet available (needed for `this.method()` never-returning call detection).
        if node.kind == syntax_kind_ext::METHOD_DECLARATION
            || node.kind == syntax_kind_ext::CONSTRUCTOR
        {
            return;
        }
        // Determine if this is an async function or generator
        let (is_async, is_generator) = if let Some(func) = self.ctx.arena.get_function(node) {
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
            if !has_return {
                self.error_at_node(
                    type_annotation,
                    "A function whose declared type is neither 'undefined', 'void', nor 'any' must return a value.",
                    diagnostic_codes::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_V,
                );
            } else {
                // TS2366: always emit when return type doesn't include undefined
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
            if !self.should_skip_no_implicit_return_check(ts7030_check_type, has_type_annotation) {
                // TSC points TS7030 to: return type annotation > function name > node itself
                let error_node = if has_type_annotation {
                    type_annotation
                } else if let Some(nn) = name_node {
                    nn
                } else {
                    idx
                };
                self.error_at_node(
                    error_node,
                    diagnostic_messages::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                    diagnostic_codes::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
                );
            }
        }
    }

    /// Check if a return context type is or references a const type parameter.
    /// Used to propagate const context into callback bodies during generic inference.
    pub(crate) fn return_context_has_const_type_param(&self, ret_ctx: TypeId) -> bool {
        // Direct check: is the return context itself a const type parameter?
        if let Some(tp_info) = tsz_solver::type_param_info(self.ctx.types, ret_ctx)
            && tp_info.is_const
        {
            return true;
        }

        // General check: does the return context reference any const type parameter?
        let referenced = tsz_solver::collect_referenced_types(self.ctx.types, ret_ctx);
        referenced.into_iter().any(|ty| {
            tsz_solver::type_param_info(self.ctx.types, ty).is_some_and(|info| info.is_const)
        })
    }
}
