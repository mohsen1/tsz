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

        // Get the type of the return expression (if any)
        let return_type = if return_data.expression.is_some() {
            // TS1359: Check for await expressions outside async function
            self.check_await_expression(return_data.expression);

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
                    && !tsz_solver::is_union_type(self.ctx.types, contextual_expected_type)
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
            let mut return_type =
                self.get_type_of_node_with_request(return_data.expression, &request);
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
        let skip_assignability = is_in_constructor && return_data.expression.is_none();

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

        let assignability_ok = if !skip_assignability
            && expected_type != TypeId::ANY
            && !self.type_contains_error(expected_type)
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
            } else {
                let ok = self.check_assignable_or_report_at(
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
            } else if expr_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION {
                self.check_conditional_return_branches_against_type(
                    return_data.expression,
                    expected_type,
                    self.ctx.in_async_context(),
                );
            }
        }
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
        // Use iterative approach with explicit stack to handle deeply nested expressions
        // This prevents stack overflow for expressions like `0 + 0 + 0 + ... + 0` (50K+ deep)
        let mut stack = vec![expr_idx];

        while let Some(current_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(current_idx) else {
                continue;
            };

            // Push child expressions onto stack for iterative processing
            match node.kind {
                syntax_kind_ext::BINARY_EXPRESSION => {
                    if let Some(bin_expr) = self.ctx.arena.get_binary_expr(node) {
                        if bin_expr.right.is_some() {
                            stack.push(bin_expr.right);
                        }
                        if bin_expr.left.is_some() {
                            stack.push(bin_expr.left);
                        }
                    }
                }
                syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                | syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => {
                    if let Some(unary_expr) = self.ctx.arena.get_unary_expr_ex(node)
                        && unary_expr.expression.is_some()
                    {
                        stack.push(unary_expr.expression);
                    }
                }
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
                    if let Some(unary_expr) = self.ctx.arena.get_unary_expr_ex(node)
                        && unary_expr.expression.is_some()
                    {
                        stack.push(unary_expr.expression);
                    }
                }
                syntax_kind_ext::CALL_EXPRESSION => {
                    if let Some(call_expr) = self.ctx.arena.get_call_expr(node) {
                        // Check arguments (push in reverse order for correct traversal)
                        if let Some(ref args) = call_expr.arguments {
                            for &arg in args.nodes.iter().rev() {
                                if arg.is_some() {
                                    stack.push(arg);
                                }
                            }
                        }
                        if call_expr.expression.is_some() {
                            stack.push(call_expr.expression);
                        }
                    }
                }
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                    if let Some(access_expr) = self.ctx.arena.get_access_expr(node)
                        && access_expr.expression.is_some()
                    {
                        stack.push(access_expr.expression);
                    }
                }
                syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren_expr) = self.ctx.arena.get_parenthesized(node)
                        && paren_expr.expression.is_some()
                    {
                        stack.push(paren_expr.expression);
                    }
                }
                _ => {
                    // For other expression types, don't recurse into children
                    // to avoid infinite recursion or performance issues
                }
            }
        }
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
                infer_params.push((ident.escaped_text.clone(), tp_data.name));
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
            tsz_solver::type_queries::get_type_parameter_info(self.ctx.types, constraint_type)
                .is_some_and(|info| {
                    self.ctx.types.resolve_atom(info.name).as_str() == name.as_str()
                })
        };
        if is_direct_self_constraint {
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
            if let Some(inner) = tsz_solver::keyof_inner_type(self.ctx.types, constraint_type) {
                refs_to_check.push(inner);
            }
            refs_to_check.iter().any(|&ref_type| {
                tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, ref_type)
                    .and_then(|def_id| self.ctx.def_to_symbol.borrow().get(&def_id).copied())
                    .is_some_and(|target_sym| self.ctx.circular_type_aliases.contains(&target_sym))
            })
        };
        if constraint_refs_circular_alias {
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
        let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
        let is_valid = evaluator.is_valid_mapped_type_key_type(evaluated);
        let is_deferred_index_access =
            tsz_solver::type_queries::get_index_access_types(self.ctx.types, evaluated)
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
        // Check if the constraint contains a self-reference to the mapped type parameter.
        // Use a shallow check that does NOT walk into other type parameters' constraints,
        // because those constraints are separate scopes. For example, in
        // `T extends { [K in keyof T]: T[K] }`, `K`'s constraint is `keyof T`.
        // Although `T`'s own constraint contains `K`, that doesn't make `K`'s constraint
        // circular — `keyof T` itself doesn't contain `K` at the surface level.
        if tsz_solver::contains_type_parameter_named_shallow(self.ctx.types, constraint_type, atom)
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

        if !is_valid && !references_enclosing_mapped_key {
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
        if data.type_node != ParserNodeIndex::NONE {
            self.check_type_node(data.type_node);
        }

        self.ctx.type_parameter_scope.remove(&name);
        if let Some(prev_type) = previous {
            self.ctx.type_parameter_scope.insert(name, prev_type);
        }
    }
}
