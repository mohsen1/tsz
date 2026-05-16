//! Return statement validation, await expression checking, and mapped type
//! constraint validation.
//!
//! Extracted from `core.rs` to keep module size manageable.

use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_return_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(return_data) = self.ctx.arena.get_return_statement(node) else {
            return;
        };

        if self.find_enclosing_static_block(stmt_idx).is_some() {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                stmt_idx,
                "A 'return' statement cannot be used inside a class static block.",
                diagnostic_codes::A_RETURN_STATEMENT_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK,
            );
            return;
        }

        // TS1108: A 'return' statement can only be used within a function body.
        // In .d.ts files, TS1036 is emitted instead of TS1108.
        // Like TSC's grammarErrorOnFirstToken, suppress grammar errors when parse
        // errors are present — TSC checks hasParseDiagnostics(sourceFile) before
        // emitting TS1108 and other grammar errors.
        if self.current_return_type().is_none() {
            if !self.ctx.is_in_ambient_declaration_file && !self.has_syntax_parse_errors() {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node(
                    stmt_idx,
                    "A 'return' statement can only be used within a function body.",
                    diagnostic_codes::A_RETURN_STATEMENT_CAN_ONLY_BE_USED_WITHIN_A_FUNCTION_BODY,
                );
            }
            // Still type-check the return expression even when outside a function body.
            // In tsc, TS1108 is added to parseDiagnostics (via grammarErrorOnFirstToken),
            // making hasParseDiagnostics() true. This suppresses TS2304/TS7006 but NOT
            // TS1212 strict-mode reserved word checks. Simulate by temporarily setting
            // has_real_syntax_errors during expression checking.
            if return_data.expression.is_some() {
                let prev_real = self.ctx.has_real_syntax_errors;
                let prev_syntax = self.ctx.has_syntax_parse_errors;
                self.ctx.has_real_syntax_errors = true;
                self.ctx.has_syntax_parse_errors = true;
                self.get_type_of_node(return_data.expression);
                self.ctx.has_real_syntax_errors = prev_real;
                self.ctx.has_syntax_parse_errors = prev_syntax;
            }
            return;
        }

        // TS2408: Setters cannot return a value.
        if return_data.expression.is_some()
            && let Some(enclosing_fn_idx) = self.find_enclosing_function(stmt_idx)
            && let Some(enclosing_fn_node) = self.ctx.arena.get(enclosing_fn_idx)
            && enclosing_fn_node.kind == syntax_kind_ext::SET_ACCESSOR
        {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                stmt_idx,
                "Setters cannot return a value.",
                diagnostic_codes::SETTERS_CANNOT_RETURN_A_VALUE,
            );
            return;
        }

        // Get the expected return type from the function context
        let expected_type = self.current_return_type().unwrap_or(TypeId::UNKNOWN);

        let mut return_mismatch_already_reported = false;

        // Get the type of the return expression (if any)
        let return_type = if return_data.expression.is_some() {
            // TS1359: Check for await expressions outside async function
            self.check_await_expression(return_data.expression);
            let return_expr_diag_snap =
                crate::context::speculation::DiagnosticSpeculationSnapshot::new(&self.ctx);

            let contextual_expected_type = if expected_type != TypeId::ANY
                && expected_type != TypeId::UNKNOWN
                && !self.type_contains_error(expected_type)
            {
                self.contextual_type_for_expression(expected_type)
            } else {
                expected_type
            };
            let should_contextualize =
                self.ctx
                    .arena
                    .get(return_data.expression)
                    .is_some_and(|expr_node| {
                        expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16
                    });
            let request = if should_contextualize
                && contextual_expected_type != TypeId::ANY
                && !self.type_contains_error(contextual_expected_type)
            {
                let use_async_promise_union_context = self.ctx.in_async_context()
                    && contextual_expected_type != TypeId::UNKNOWN
                    && contextual_expected_type != TypeId::NEVER
                    && !crate::query_boundaries::common::is_union_type(
                        self.ctx.types,
                        contextual_expected_type,
                    )
                    && !self.is_promise_type(contextual_expected_type)
                    && self
                        .ctx
                        .arena
                        .get(return_data.expression)
                        .is_some_and(|expr_node| {
                            matches!(
                                expr_node.kind,
                                syntax_kind_ext::CALL_EXPRESSION
                                    | syntax_kind_ext::NEW_EXPRESSION
                                    | syntax_kind_ext::AWAIT_EXPRESSION
                            )
                        });
                // For async functions, the return type has been unwrapped from Promise<T>
                // to T. But return expressions like `return new Promise(resolve => ...)`
                // need Promise<T> in the contextual type for generic constructor inference.
                // Transform T → T | PromiseLike<T> | Promise<T> (matching await behavior).
                // Only apply when expected_type is the unwrapped T (not a union or Promise).
                // If expected_type is already a union or Promise-like, the transformation
                // would create nonsensical nested types.
                let ctx_type = if use_async_promise_union_context {
                    let promise_like_t = self.get_promise_like_type(contextual_expected_type);
                    let promise_t = self.get_promise_type(contextual_expected_type);
                    let mut members = vec![contextual_expected_type, promise_like_t];
                    if let Some(pt) = promise_t {
                        members.push(pt);
                    }
                    self.ctx.types.factory().union(members)
                } else {
                    contextual_expected_type
                };
                // Targeted invalidation: the return expression will be re-evaluated
                // with a non-empty request (contextual return type), which bypasses
                // node_types for non-audited types and uses request_node_types for
                // audited types.  Only the expression node and its contextually-
                // sensitive immediate subtree need clearing.
                self.invalidate_expression_for_contextual_retry(return_data.expression);
                TypingRequest::with_contextual_type(ctx_type)
            } else {
                TypingRequest::NONE
            };
            let unwrapped_return_expr = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(return_data.expression);
            let preserve_literal_return = self.ctx.in_async_context()
                && request.contextual_type.is_some()
                && self
                    .ctx
                    .arena
                    .get(unwrapped_return_expr)
                    .is_some_and(|expr_node| {
                        matches!(
                            expr_node.kind,
                            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        )
                    });
            let prev_preserve_literals = self.ctx.preserve_literal_types;
            if preserve_literal_return {
                self.ctx.preserve_literal_types = true;
            }
            let mut return_type =
                self.get_type_of_node_with_request(return_data.expression, &request);
            if let Some(contextual_type) = request.contextual_type
                && self
                    .ctx
                    .arena
                    .get(return_data.expression)
                    .is_some_and(|expr_node| expr_node.kind == syntax_kind_ext::NEW_EXPRESSION)
                && (self
                    .contextual_application_recovers_unknown_result(return_type, contextual_type)
                    || self.contextual_application_recovers_type_param_result(
                        return_type,
                        contextual_type,
                    )
                    || (crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        return_type,
                    ) && self
                        .ctx
                        .arena
                        .get_call_expr_at(return_data.expression)
                        .is_some_and(|new_expr| {
                            self.contextual_application_matches_new_target(
                                new_expr.expression,
                                contextual_type,
                            )
                        })))
                && self.is_assignable_to(contextual_type, expected_type)
            {
                return_type = contextual_type;
            }
            self.ctx.preserve_literal_types = prev_preserve_literals;
            if self.ctx.in_async_context() {
                // Use unwrap_async_return_type_for_body which handles unions
                // by unwrapping Promise from each member individually.
                // This is needed for cases like:
                //   async function f(): Promise<T> {
                //     return cond ? getPromise<T>() : plainValue;
                //   }
                // where the conditional expression type is Promise<T> | PlainValue.
                // Each Promise member must be unwrapped before checking against T.
                return_type = self.unwrap_async_return_type_for_body(return_type);
            }
            // A contextual async return can shape inline literals, but fixed call
            // arguments like identifiers keep their declared widened type in tsc's
            // TS2322 source display.
            if request.contextual_type.is_some()
                && self
                    .async_contextual_return_call_has_only_fixed_arguments(return_data.expression)
                && !self.is_assignable_to(return_type, expected_type)
            {
                self.invalidate_expression_for_contextual_retry(return_data.expression);
                let mut raw_return_type = self
                    .get_type_of_node_with_request(return_data.expression, &TypingRequest::NONE);
                raw_return_type = self.unwrap_async_return_type_for_body(raw_return_type);
                return_expr_diag_snap.rollback(&mut self.ctx);
                let source_str = self
                    .object_literal_source_type_display(return_data.expression, Some(expected_type))
                    .unwrap_or_else(|| self.format_type_diagnostic_widened(raw_return_type));
                let target_str = self.format_type_diagnostic(expected_type);
                let message = crate::diagnostics::format_message(
                    crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                self.error_at_node(
                    stmt_idx,
                    &message,
                    crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
                return_mismatch_already_reported = true;
                return_type = raw_return_type;
            }
            return_type
        } else {
            // `return;` without expression returns undefined
            TypeId::UNDEFINED
        };

        // Ensure relation preconditions for the return expression before assignability.
        // The assignability gateway already prepares `expected_type`, so doing it here
        // as well duplicates expensive lazy-ref/application traversals on hot return paths.
        self.ensure_relation_input_ready(return_type);

        // Check if the return type is assignable to the expected type.
        let is_in_constructor = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| c.in_constructor);

        // Use the return expression as the source anchor for failure analysis so
        // branch/literal elaboration can drill into nested expressions, but keep
        // the `return` statement as the fallback diagnostic anchor when no
        // elaboration is available.
        let source_error_node = if return_data.expression.is_some() {
            return_data.expression
        } else {
            stmt_idx
        };
        let fallback_error_node = stmt_idx;

        // In constructors, bare `return;` (without expression) is always allowed — TSC
        // doesn't check assignability for void returns in constructors.
        //
        // In JS files, tsc additionally suppresses the constructor return-type
        // assignability check entirely. JavaScript constructors can return any
        // object — returning an object from a constructor replaces `this` at
        // runtime, so `return a` (where `a` is some unrelated type) is
        // idiomatic and not an error in `--checkJs` mode. Mirrors tsc's
        // `isJavaScriptFile`-gated bypass in `checkReturnStatement`.
        let skip_assignability = is_in_constructor
            && (return_data.expression.is_none() || self.is_js_file())
            || (return_data.expression.is_none()
                && self.type_references_unresolved_import(expected_type));

        // Track whether assignability check passed — when it fails, the solver's
        // failure reason already emits the appropriate diagnostic (including TS2353
        // for excess properties on fresh object literals).  Running the explicit
        // excess-property check again would produce a duplicate.
        //
        // When the return expression is a conditional expression and the combined
        // type fails assignability, check each branch separately and report
        // per-branch errors instead of the combined error.  This matches tsc's
        // behavior of drilling into conditional expression branches for return
        // statements.
        let unwrapped_expr = self.ctx.arena.skip_parenthesized(return_data.expression);
        let is_conditional_expr = unwrapped_expr.is_some()
            && self
                .ctx
                .arena
                .get(unwrapped_expr)
                .is_some_and(|e| e.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION);

        // tsc still reports TS2322 when the declared return type contains a
        // nested error (e.g. a class whose members reference unresolved
        // identifiers) but the surface structure is otherwise distinguishable
        // from the return value — `return null` against a non-nullable target
        // is still TS2322 because `null` is not the structural target.
        let return_is_nullish_literal =
            return_type == TypeId::NULL || return_type == TypeId::UNDEFINED;
        let target_is_top_level_error = expected_type == TypeId::ERROR;
        let expected_contains_error_nested =
            self.type_contains_error(expected_type) && !target_is_top_level_error;
        let allow_check_through_nested_error =
            return_is_nullish_literal && expected_contains_error_nested;
        let assignability_ok = if return_mismatch_already_reported {
            false
        } else if !skip_assignability
            && expected_type != TypeId::ANY
            && !target_is_top_level_error
            && (!expected_contains_error_nested || allow_check_through_nested_error)
        {
            if is_conditional_expr && !self.is_assignable_to(return_type, expected_type) {
                // Per-branch error elaboration for conditional expressions.
                // Instead of "Type '1 | 2' is not assignable to type '3'" at `return`,
                // emit "Type '1' is not assignable to type '3'" at each failing branch.
                self.check_conditional_return_branches_against_type(
                    unwrapped_expr,
                    expected_type,
                    self.ctx.in_async_context(),
                );
                if is_in_constructor {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        fallback_error_node,
                        diagnostic_messages::RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_MUST_BE_ASSIGNABLE_TO_THE_INSTANCE_TYPE_OF,
                        diagnostic_codes::RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_MUST_BE_ASSIGNABLE_TO_THE_INSTANCE_TYPE_OF,
                    );
                }
                false
            } else if self.should_report_primitive_to_generic_indexed_conditional_return(
                return_type,
                expected_type,
            ) {
                self.error_type_not_assignable_generic_at(
                    return_type,
                    expected_type,
                    fallback_error_node,
                );
                false
            } else {
                let ok = self.check_assignable_or_report_at_exact_anchor(
                    return_type,
                    expected_type,
                    source_error_node,
                    fallback_error_node,
                );
                if !ok {
                    // TS2409: In constructors, also emit the constructor-specific diagnostic
                    // alongside the TS2322 already emitted by check_assignable_or_report.
                    if is_in_constructor {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            fallback_error_node,
                            diagnostic_messages::RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_MUST_BE_ASSIGNABLE_TO_THE_INSTANCE_TYPE_OF,
                            diagnostic_codes::RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_MUST_BE_ASSIGNABLE_TO_THE_INSTANCE_TYPE_OF,
                        );
                    }
                }
                ok
            }
        } else {
            true
        };

        // Only run explicit excess-property check when the assignability check
        // passed (types are structurally compatible but may have excess props)
        // or was skipped.  When assignability failed, the solver already
        // emitted the correct TS2353/TS2322 via the failure reason.
        if assignability_ok
            && expected_type != TypeId::ANY
            && expected_type != TypeId::UNKNOWN
            && !self.type_contains_error(expected_type)
            && return_data.expression.is_some()
            && let Some(expr_node) = self.ctx.arena.get(return_data.expression)
        {
            if expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                self.check_object_literal_excess_properties(
                    return_type,
                    expected_type,
                    return_data.expression,
                );
            } else if crate::query_boundaries::common::is_fresh_object_type(
                self.ctx.types,
                return_type,
            ) {
                // Fresh type from non-literal expression (e.g., `return obj = { x: 1, y: 2 }`).
                // Walk through binary assignment expressions to find the object literal.
                let literal_idx = self.find_rhs_object_literal(return_data.expression);
                self.check_object_literal_excess_properties(
                    return_type,
                    expected_type,
                    literal_idx.unwrap_or(return_data.expression),
                );
            } else if expr_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION {
                self.check_conditional_return_branches_against_type(
                    return_data.expression,
                    expected_type,
                    self.ctx.in_async_context(),
                );
            }
        }
    }

    fn type_references_unresolved_import(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::collect_all_types(self.ctx.types, type_id)
            .into_iter()
            .any(|ty| {
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, ty)
                    .and_then(|def_id| self.ctx.def_to_symbol_id(def_id))
                    .is_some_and(|sym_id| self.is_unresolved_import_symbol_id(sym_id))
            })
    }

    fn should_report_primitive_to_generic_indexed_conditional_return(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !matches!(
            source,
            TypeId::NUMBER | TypeId::STRING | TypeId::BOOLEAN | TypeId::BIGINT | TypeId::SYMBOL
        ) {
            return false;
        }
        let Some((base, args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, target)
        else {
            return false;
        };
        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, base)
        else {
            return false;
        };
        let Some(def) = self.ctx.definition_store.get(def_id) else {
            return false;
        };
        if def.kind != tsz_solver::def::DefKind::TypeAlias
            || !def.body.is_some_and(|body| {
                crate::query_boundaries::common::is_conditional_type(self.ctx.types, body)
            })
        {
            return false;
        }
        args.iter().any(|&arg| {
            crate::query_boundaries::common::is_index_access_type(self.ctx.types, arg)
                && crate::query_boundaries::common::contains_type_parameters(self.ctx.types, arg)
        })
    }

    fn async_contextual_return_call_has_only_fixed_arguments(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION
            && node.kind != syntax_kind_ext::NEW_EXPRESSION
        {
            return false;
        }
        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };
        call.arguments.as_ref().is_some_and(|args| {
            !args.nodes.is_empty()
                && args
                    .nodes
                    .iter()
                    .all(|&arg_idx| !self.argument_needs_contextual_type(arg_idx))
        })
    }

    // --- Await Expression Validation ---

    /// Check if current compiler options support top-level await.
    ///
    /// Routes through the environment capability boundary — the module + target
    /// requirements for top-level `await` are identical to top-level `await using`.
    const fn supports_top_level_await(&self) -> bool {
        self.ctx.capabilities.top_level_await_using_supported
    }

    /// Check an await expression for async context.
    ///
    /// Validates that await expressions are only used within async functions,
    /// recursively checking child expressions for nested await usage.
    ///
    /// ## Parameters:
    /// - `expr_idx`: The expression node index to check
    ///
    /// ## Validation:
    /// - Emits TS1308 if await is used outside async function
    /// - Iteratively checks child expressions for await expressions (no recursion)
    pub(crate) fn check_await_expression(&mut self, expr_idx: NodeIndex) {
        // Use iterative approach with explicit stack to handle deeply nested expressions.
        // This prevents stack overflow for expressions like `0 + 0 + 0 + ... + 0` (50K+ deep).
        let mut stack = vec![expr_idx];

        while let Some(current_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(current_idx) else {
                continue;
            };

            if Self::await_expression_traversal_boundary(node.kind) {
                continue;
            }

            match node.kind {
                syntax_kind_ext::AWAIT_EXPRESSION => {
                    // Validate await expression context.
                    // tsc suppresses these grammar checks when the file has parse errors
                    // (e.g., `@dec await 1` — the decorator error suppresses TS1378).
                    if !self.ctx.in_async_context() && !self.ctx.has_syntax_parse_errors {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

                        // Check if we're at top level of a module
                        let at_top_level = self.ctx.function_depth == 0;

                        if at_top_level {
                            // TS1378: Top-level await requires ES2022+/ESNext module and ES2017+ target
                            if !self.supports_top_level_await() {
                                self.error_at_node(
                                    current_idx,
                                    diagnostic_messages::TOP_LEVEL_AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES,
                                    diagnostic_codes::TOP_LEVEL_AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ES,
                                );
                            } else if !self.ctx.is_external_module_file() {
                                self.error_at_node(
                                    current_idx,
                                    diagnostic_messages::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS,
                                    diagnostic_codes::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_FILE_WHEN_THAT_FILE_IS,
                                );
                            }
                        } else {
                            // TS1308: 'await' expressions are only allowed within async functions
                            self.error_at_node(
                                current_idx,
                                diagnostic_messages::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS,
                                diagnostic_codes::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS,
                            );
                        }
                    }
                }
                _ => {
                    for child in self.ctx.arena.get_children(current_idx) {
                        if child.is_some() {
                            stack.push(child);
                        }
                    }
                }
            }
        }
    }

    const fn await_expression_traversal_boundary(kind: u16) -> bool {
        matches!(
            kind,
            syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CONSTRUCTOR
        )
    }

    // --- Variable Statement Validation ---

    /// Check a for-await statement for async context and module/target support.
    ///
    /// Validates that for-await loops are only used within async functions or at top level
    /// with appropriate compiler options.
    ///
    /// ## Parameters:
    /// - `stmt_idx`: The for-await statement node index to check
    ///
    /// ## Notes:
    /// tsc 6.0 no longer emits TS1103/TS1431/TS1432 for `for await` statements.
    /// Top-level await and `for await` in non-async functions are now accepted
    /// without error.  Only TS18038 (`for await` in class static blocks) is
    /// still emitted.
    pub(crate) fn check_for_await_statement(&mut self, stmt_idx: NodeIndex) {
        if !self.ctx.in_async_context()
            && self.ctx.function_depth > 0
            && self.find_enclosing_static_block(stmt_idx).is_some()
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            // TS18038: 'for await' loops cannot be used inside a class static block.
            // TSC anchors this error at the `await` keyword, not the `for` keyword.
            if let Some(stmt_node) = self.ctx.arena.get(stmt_idx) {
                let await_pos = stmt_node.pos + 4; // skip "for "
                let await_len = 5u32; // "await"
                self.error(
                    await_pos,
                    await_len,
                    diagnostic_messages::FOR_AWAIT_LOOPS_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK
                        .to_string(),
                    diagnostic_codes::FOR_AWAIT_LOOPS_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK,
                );
            } else {
                self.error_at_node(
                    stmt_idx,
                    diagnostic_messages::FOR_AWAIT_LOOPS_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK,
                    diagnostic_codes::FOR_AWAIT_LOOPS_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK,
                );
            }
        }
    }

    /// TS6133: Check for unused `infer` type parameters in conditional types.
    pub(super) fn check_unused_infer_type_params_in_conditional(
        &mut self,
        cond: &tsz_parser::parser::node::ConditionalTypeData,
    ) {
        let mut infer_params: Vec<(String, NodeIndex)> = Vec::new();
        let mut stack: Vec<NodeIndex> = vec![cond.extends_type];
        while let Some(idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::INFER_TYPE
                && let Some(infer_data) = self.ctx.arena.get_infer_type(node)
                && let Some(tp_node) = self.ctx.arena.get(infer_data.type_parameter)
                && let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node)
                && let Some(name_node) = self.ctx.arena.get(tp_data.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                // Use the InferType node (idx) for positioning, not the identifier (tp_data.name)
                // TSC spans the diagnostic across `infer U`, not just `U`
                infer_params.push((ident.escaped_text.clone(), idx));
            }
            for child in self.ctx.arena.get_children(idx) {
                stack.push(child);
            }
        }
        if infer_params.is_empty() {
            return;
        }
        for (name, name_idx) in &infer_params {
            let mut found = false;
            for &branch in &[cond.true_type, cond.false_type] {
                if self.type_node_references_name(branch, name) {
                    found = true;
                    break;
                }
            }
            if !found {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                self.error_at_node(
                    *name_idx,
                    &format_message(
                        diagnostic_messages::IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ,
                        &[name],
                    ),
                    diagnostic_codes::IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ,
                );
            }
        }
    }

    fn type_node_references_name(&self, root: NodeIndex, name: &str) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::TYPE_REFERENCE
                && let Some(tr) = self.ctx.arena.get_type_ref(node)
                && let Some(tn) = self.ctx.arena.get(tr.type_name)
                && let Some(ident) = self.ctx.arena.get_identifier(tn)
                && ident.escaped_text == name
            {
                return true;
            }
            for child in self.ctx.arena.get_children(idx) {
                stack.push(child);
            }
        }
        false
    }

    /// TS2322: Check that a mapped type's constraint is assignable to `string | number | symbol`.
    pub(super) fn check_mapped_type_constraint(&mut self, mapped_node_idx: NodeIndex) {
        use tsz_parser::parser::NodeIndex as ParserNodeIndex;

        let Some(node) = self.ctx.arena.get(mapped_node_idx) else {
            return;
        };
        let Some(data) = self.ctx.arena.get_mapped_type(node) else {
            return;
        };

        // When an `as` clause is present (e.g., `[Key in T as ...]`), the constraint
        // type T doesn't need to be a key type directly — the keys are produced by the
        // `as` clause. TSC skips this validation for mapped types with name remapping.
        if data.name_type != NodeIndex::NONE {
            return;
        }

        // Get the constraint node from the mapped type's type parameter.
        let Some(tp_node) = self.ctx.arena.get(data.type_parameter) else {
            return;
        };
        let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node) else {
            return;
        };
        if tp_data.constraint == ParserNodeIndex::NONE {
            return;
        }
        let Some(constraint_node) = self.ctx.arena.get(tp_data.constraint) else {
            return;
        };
        let constraint_pos = constraint_node.pos;
        let constraint_end = constraint_node.end;
        let Some(param_node) = self.ctx.arena.get(data.type_parameter) else {
            return;
        };
        let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
            return;
        };
        let Some(name_node) = self.ctx.arena.get(param.name) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };
        let name = ident.escaped_text.clone();
        let atom = self.ctx.types.intern_string(&name);
        let provisional_type_id = self
            .ctx
            .types
            .factory()
            .type_param(tsz_solver::TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            });
        let previous = self
            .ctx
            .type_parameter_scope
            .insert(name.clone(), provisional_type_id);

        // Resolve just the constraint type node (e.g., Date, T, keyof T)
        // rather than the whole mapped type, to avoid side effects.
        let mut constraint_type = self.get_type_from_type_node(tp_data.constraint);
        if constraint_type == TypeId::ERROR {
            self.ctx.type_parameter_scope.remove(&name);
            if let Some(prev_type) = previous {
                self.ctx.type_parameter_scope.insert(name, prev_type);
            }
            return;
        }

        // Nested mapped types like `{ [Q in P]: ... }` rely on the outer mapped key
        // parameter `P` carrying its `keyof T` constraint. Resolve bare type-parameter
        // references through the current scoped bindings before validating them.
        let scoped_name = self
            .ctx
            .arena
            .get(tp_data.constraint)
            .and_then(|n| self.ctx.arena.get_type_ref(n).map(|tr| tr.type_name))
            .or(Some(tp_data.constraint))
            .and_then(|idx| self.ctx.arena.get(idx))
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|ident| ident.escaped_text.clone());
        if let Some(ref scoped_name) = scoped_name
            && scoped_name.as_str() != name.as_str()
            && let Some(&scoped_type) = self.ctx.type_parameter_scope.get(scoped_name.as_str())
            && crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, scoped_type)
        {
            constraint_type = scoped_type;
        }

        let is_direct_self_constraint = if constraint_type == provisional_type_id {
            true
        } else {
            crate::query_boundaries::common::type_param_info(self.ctx.types, constraint_type)
                .is_some_and(|info| {
                    self.ctx.types.resolve_atom(info.name).as_str() == name.as_str()
                        && self
                            .type_parameter_identity_matches(constraint_type, provisional_type_id)
                })
        };
        // Also check if the constraint is a type parameter whose own constraint is
        // circular (set to UNKNOWN). This handles mapped types like `{ [P in T]: number }`
        // where T has been detected as having a circular constraint. In tsc, resolving
        // P's base constraint would recurse into T and detect the same circularity.
        let constraint_is_circular_type_param = !is_direct_self_constraint
            && crate::query_boundaries::common::type_param_info(self.ctx.types, constraint_type)
                .is_some_and(|info| info.constraint == Some(TypeId::UNKNOWN));

        if is_direct_self_constraint || constraint_is_circular_type_param {
            let message = format!("Type parameter '{name}' has a circular constraint.");
            self.ctx.error(
                constraint_pos,
                constraint_end - constraint_pos,
                message,
                2313,
            );
            self.ctx.type_parameter_scope.remove(&name);
            if let Some(prev_type) = previous {
                self.ctx.type_parameter_scope.insert(name, prev_type);
            }
            return;
        }

        // Check if the constraint references a circular type alias
        // (e.g., `type Recurse = { [K in keyof Recurse]: ... }` — K's constraint
        // is `keyof Recurse` which circularly references the enclosing alias).
        // During the checking pass, `circular_type_aliases` has been populated by
        // the resolution pass, so we check it instead of the resolution stack.
        let constraint_refs_circular_alias = {
            let mut refs_to_check = vec![constraint_type];
            if let Some(inner) =
                crate::query_boundaries::common::keyof_inner_type(self.ctx.types, constraint_type)
            {
                refs_to_check.push(inner);
            }
            refs_to_check.iter().any(|&ref_type| {
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, ref_type)
                    .and_then(|def_id| self.ctx.def_to_symbol.borrow().get(&def_id).copied())
                    .is_some_and(|target_sym| self.ctx.circular_type_aliases.contains(&target_sym))
            })
        };

        // Skip TS2313 for valid mapped type key patterns like `keyof T` where T
        // is the type parameter being constrained.
        let is_keyof_parent_type_param =
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, constraint_type)
                .is_some_and(|inner| {
                    crate::query_boundaries::common::type_param_info(self.ctx.types, inner)
                        .is_some_and(|info| info.name == atom)
                });
        if constraint_refs_circular_alias && !is_keyof_parent_type_param {
            let message = format!("Type parameter '{name}' has a circular constraint.");
            self.ctx.error(
                constraint_pos,
                constraint_end - constraint_pos,
                message,
                2313,
            );
            self.ctx.type_parameter_scope.remove(&name);
            if let Some(prev_type) = previous {
                self.ctx.type_parameter_scope.insert(name, prev_type);
            }
            return;
        }

        // Evaluate to resolve Lazy/Application types before checking validity.
        let evaluated = self.evaluate_type_with_env(constraint_type);

        // Use the solver's is_valid_mapped_type_key_type which handles type
        // parameters (checks constraint), unions, keyof, literals, etc.
        // Index-access constraints like AB[K] are accepted when K is known to be
        // constrained to the object's key space.
        let constraint_is_deferred = crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            constraint_type,
        ) || crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            evaluated,
        );
        let is_valid = if constraint_is_deferred {
            crate::query_boundaries::common::is_valid_mapped_type_key_type(
                self.ctx.types,
                evaluated,
            ) || crate::query_boundaries::common::is_valid_mapped_type_key_type(
                self.ctx.types,
                constraint_type,
            )
        } else {
            let evaluator =
                crate::query_boundaries::common::new_binary_op_evaluator(self.ctx.types);
            evaluator.is_valid_computed_property_name_type(evaluated)
                || evaluator.is_valid_computed_property_name_type(constraint_type)
        };
        let is_deferred_index_access =
            crate::query_boundaries::common::index_access_types(self.ctx.types, evaluated)
                .is_some_and(|(object_type, index_type)| {
                    crate::query_boundaries::common::type_parameter_constraint(
                        self.ctx.types,
                        index_type,
                    )
                    .is_none_or(|constraint| {
                        self.is_assignable_to(
                            constraint,
                            self.ctx.types.evaluate_keyof(object_type),
                        )
                    })
                });

        // When the original constraint is an indexed access type (e.g., `AB[S]`),
        // evaluation may eagerly resolve it (e.g., to `"a"`) by substituting the
        // type parameter's constraint, which can mask the fact that the index
        // constraint exceeds the object's key space (e.g., S extends 'a'|'b'|'extra'
        // but AB only has keys 'a'|'b'). Check the PRE-evaluation indexed access
        // for this pattern and override validity when the index constraint is invalid.
        let has_invalid_index_constraint =
            crate::query_boundaries::common::index_access_types(self.ctx.types, constraint_type)
                .is_some_and(|(object_type, index_type)| {
                    crate::query_boundaries::common::type_parameter_constraint(
                        self.ctx.types,
                        index_type,
                    )
                    .is_some_and(|constraint| {
                        let keyof_object = self.ctx.types.evaluate_keyof(object_type);
                        !self.is_assignable_to(constraint, keyof_object)
                    })
                });

        // Check if the constraint contains a self-reference to the type parameter.
        // Use a shallow check that does NOT walk into other type parameters' constraints,
        // because those constraints are separate scopes.
        //
        // Skip this check for mapped type constraints like `T extends { [K in keyof T]: T[K] }`.
        // In tsc, a mapped type constraint that references T via keyof T (or in property types)
        // is NOT circular — it means "T must conform to this mapped shape". This is a common
        // and valid TypeScript pattern.
        let constraint_is_mapped =
            crate::query_boundaries::common::is_mapped_type(self.ctx.types, constraint_type)
                || crate::query_boundaries::common::is_mapped_type(self.ctx.types, evaluated);
        if !constraint_is_mapped
            && self.contains_type_parameter_identity_shallow(constraint_type, provisional_type_id)
        {
            let message = format!("Type parameter '{name}' has a circular constraint.");
            self.ctx.error(
                constraint_pos,
                constraint_end - constraint_pos,
                message,
                2313,
            );
            self.ctx.type_parameter_scope.remove(&name);
            if let Some(prev_type) = previous {
                self.ctx.type_parameter_scope.insert(name, prev_type);
            }
            return;
        }
        if is_deferred_index_access {
            self.ctx.type_parameter_scope.remove(&name);
            if let Some(prev_type) = previous {
                self.ctx.type_parameter_scope.insert(name, prev_type);
            }
            return;
        }
        let references_enclosing_mapped_key = scoped_name.as_ref().is_some_and(|constraint_name| {
            let mut current = self
                .ctx
                .arena
                .get_extended(mapped_node_idx)
                .and_then(|ext| (ext.parent != NodeIndex::NONE).then_some(ext.parent));
            while let Some(parent_idx) = current {
                let Some(parent) = self.ctx.arena.get(parent_idx) else {
                    break;
                };
                if parent.kind == syntax_kind_ext::MAPPED_TYPE
                    && let Some(parent_mapped) = self.ctx.arena.get_mapped_type(parent)
                    && let Some(parent_tp_node) = self.ctx.arena.get(parent_mapped.type_parameter)
                    && let Some(parent_tp) = self.ctx.arena.get_type_parameter(parent_tp_node)
                    && let Some(parent_name_node) = self.ctx.arena.get(parent_tp.name)
                    && let Some(parent_ident) = self.ctx.arena.get_identifier(parent_name_node)
                    && &parent_ident.escaped_text == constraint_name
                {
                    return true;
                }
                current = self
                    .ctx
                    .arena
                    .get_extended(parent_idx)
                    .and_then(|ext| (ext.parent != NodeIndex::NONE).then_some(ext.parent));
            }
            false
        });

        if (!is_valid || has_invalid_index_constraint) && !references_enclosing_mapped_key {
            let constraint_name = {
                let mut formatter = self.ctx.create_type_formatter();
                formatter.format(constraint_type)
            };
            let message = format!(
                "Type '{constraint_name}' is not assignable to type 'string | number | symbol'."
            );
            self.ctx.error(
                constraint_pos,
                constraint_end - constraint_pos,
                message,
                2322,
            );
        }

        let constrained_type_id = self
            .ctx
            .types
            .factory()
            .type_param(tsz_solver::TypeParamInfo {
                name: atom,
                constraint: Some(constraint_type),
                default: None,
                is_const: false,
            });
        self.ctx
            .type_parameter_scope
            .insert(name.clone(), constrained_type_id);

        self.ctx.type_parameter_scope.remove(&name);
        if let Some(prev_type) = previous {
            self.ctx.type_parameter_scope.insert(name, prev_type);
        }
    }
}
