//! Yield and generator expression type computation for the expression dispatcher.

use crate::context::TypingRequest;
use crate::dispatch::ExpressionDispatcher;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a, 'b> ExpressionDispatcher<'a, 'b> {
    fn get_expected_yield_type(&mut self, idx: NodeIndex) -> Option<TypeId> {
        let enclosing_fn_idx = self.checker.find_enclosing_function(idx)?;
        let fn_node = self.checker.ctx.arena.get(enclosing_fn_idx)?;
        let declared_return_type_node =
            if let Some(func) = self.checker.ctx.arena.get_function(fn_node) {
                if !func.asterisk_token || func.type_annotation.is_none() {
                    return None;
                }
                func.type_annotation
            } else if let Some(method) = self.checker.ctx.arena.get_method_decl(fn_node) {
                if !method.asterisk_token || method.type_annotation.is_none() {
                    return None;
                }
                method.type_annotation
            } else {
                return None;
            };

        // Prefer syntactic extraction from the explicit annotation first.
        // This preserves `TYield` exactly as written (e.g. `IterableIterator<number>`
        // => `number`) even if semantic base resolution currently widens it.
        let declared_return_node = self.checker.ctx.arena.get(declared_return_type_node)?;
        if declared_return_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            let declared_return_type = self
                .checker
                .get_type_from_type_node(declared_return_type_node);
            return self
                .checker
                .get_generator_yield_type_argument(declared_return_type);
        }
        let type_ref = self.checker.ctx.arena.get_type_ref(declared_return_node)?;
        let type_name_node = self.checker.ctx.arena.get(type_ref.type_name)?;
        let type_name = self
            .checker
            .ctx
            .arena
            .get_identifier(type_name_node)
            .map(|ident| ident.escaped_text.as_str())?;
        if !matches!(
            type_name,
            "Generator"
                | "AsyncGenerator"
                | "Iterator"
                | "AsyncIterator"
                | "IterableIterator"
                | "AsyncIterableIterator"
        ) {
            let declared_return_type = self
                .checker
                .get_type_from_type_node(declared_return_type_node);
            return self
                .checker
                .get_generator_yield_type_argument(declared_return_type);
        }
        if let Some(first_arg) = type_ref
            .type_arguments
            .as_ref()
            .and_then(|args| args.nodes.first().copied())
        {
            return Some(self.checker.get_type_from_type_node(first_arg));
        }
        let declared_return_type = self
            .checker
            .get_type_from_type_node(declared_return_type_node);
        self.checker
            .get_generator_yield_type_argument(declared_return_type)
    }

    fn type_node_includes_undefined(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.checker.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind == SyntaxKind::UndefinedKeyword as u16 {
            return true;
        }
        if node.kind == syntax_kind_ext::UNION_TYPE
            && let Some(composite) = self.checker.ctx.arena.get_composite_type(node)
        {
            return composite
                .types
                .nodes
                .iter()
                .copied()
                .any(|member| self.type_node_includes_undefined(member));
        }
        false
    }

    fn explicit_generator_yield_allows_undefined(&mut self, idx: NodeIndex) -> Option<bool> {
        let enclosing_fn_idx = self.checker.find_enclosing_function(idx)?;
        let fn_node = self.checker.ctx.arena.get(enclosing_fn_idx)?;

        let declared_return_type_node =
            if let Some(func) = self.checker.ctx.arena.get_function(fn_node) {
                if !func.asterisk_token || func.type_annotation.is_none() {
                    return None;
                }
                func.type_annotation
            } else if let Some(method) = self.checker.ctx.arena.get_method_decl(fn_node) {
                if !method.asterisk_token || method.type_annotation.is_none() {
                    return None;
                }
                method.type_annotation
            } else {
                return None;
            };
        let declared_return_node = self.checker.ctx.arena.get(declared_return_type_node)?;
        if declared_return_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.checker.ctx.arena.get_type_ref(declared_return_node)?;
        let type_name_node = self.checker.ctx.arena.get(type_ref.type_name)?;
        let type_name = self
            .checker
            .ctx
            .arena
            .get_identifier(type_name_node)
            .map(|ident| ident.escaped_text.as_str())?;
        if !matches!(
            type_name,
            "Generator"
                | "AsyncGenerator"
                | "Iterator"
                | "AsyncIterator"
                | "IterableIterator"
                | "AsyncIterableIterator"
        ) {
            return None;
        }

        let first_arg = type_ref.type_arguments.as_ref()?.nodes.first().copied()?;
        Some(self.type_node_includes_undefined(first_arg))
    }

    /// Get the declared generator type for the enclosing generator function.
    fn get_expected_generator_type(&mut self, idx: NodeIndex) -> Option<TypeId> {
        let enclosing_fn_idx = self.checker.find_enclosing_function(idx)?;
        let fn_node = self.checker.ctx.arena.get(enclosing_fn_idx)?;

        let declared_return_type_node =
            if let Some(func) = self.checker.ctx.arena.get_function(fn_node) {
                if !func.asterisk_token || func.type_annotation.is_none() {
                    return None;
                }
                func.type_annotation
            } else if let Some(method) = self.checker.ctx.arena.get_method_decl(fn_node) {
                if !method.asterisk_token || method.type_annotation.is_none() {
                    return None;
                }
                method.type_annotation
            } else {
                return None;
            };

        // Get the declared generator type
        let declared_return_type = self
            .checker
            .get_type_from_type_node(declared_return_type_node);
        Some(declared_return_type)
    }

    pub(crate) fn get_type_of_yield_expression(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let Some(node) = self.checker.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        let Some(yield_expr) = self.checker.ctx.arena.get_unary_expr_ex(node) else {
            return TypeId::ERROR;
        };

        // If yield is outside a generator function, the parser already emitted TS1163.
        // Return ANY without evaluating the operand to avoid cascading TS2304 errors.
        let is_in_generator = self
            .checker
            .find_enclosing_function(idx)
            .and_then(|fn_idx| self.checker.ctx.arena.get(fn_idx))
            .is_some_and(|fn_node| {
                if let Some(func) = self.checker.ctx.arena.get_function(fn_node) {
                    func.asterisk_token
                } else if let Some(method) = self.checker.ctx.arena.get_method_decl(fn_node) {
                    method.asterisk_token
                } else {
                    false
                }
            });
        if !is_in_generator {
            return TypeId::ANY;
        }

        // TS2523: 'yield' expressions cannot be used in a parameter initializer.
        // Only emit when there are no nearby parse errors (to avoid cascading diagnostics
        // after parser recovery, e.g. `function * foo(a = yield => yield) {}`).
        if self.checker.is_in_default_parameter(idx)
            && !self.checker.node_has_nearby_parse_error(idx)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.checker.error_at_node(
                idx,
                diagnostic_messages::YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
                diagnostic_codes::YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
            );
        }

        // For yield*, tracks the delegated iterator's return type.
        // The yield* expression result is TReturn of the delegated iterator, NOT TNext
        // of the containing generator (which is what regular yield returns).
        let mut yield_star_return_type: Option<TypeId> = None;
        let yielded_type = if yield_expr.expression.is_none() {
            TypeId::UNDEFINED
        } else {
            let is_async_generator = self
                .checker
                .find_enclosing_function(idx)
                .and_then(|fn_idx| self.checker.ctx.arena.get(fn_idx))
                .is_some_and(|fn_node| {
                    if let Some(func) = self.checker.ctx.arena.get_function(fn_node) {
                        func.is_async && func.asterisk_token
                    } else if let Some(method) = self.checker.ctx.arena.get_method_decl(fn_node) {
                        self.checker.has_async_modifier(&method.modifiers) && method.asterisk_token
                    } else {
                        false
                    }
                });
            // Set contextual type for yield expression from the generator's yield type.
            // This allows `yield (num) => ...` to contextually type arrow params.
            // For `yield *expr`, the expression is an iterable of the yield type,
            // so wrap the contextual type in Array<T> to contextually type array elements.
            let outer_contextual = request.contextual_type;
            let mut contextual_yield_star_return = None;
            let yield_request = if let Some(yield_ctx) = self
                .checker
                .ctx
                .current_yield_type()
                .or_else(|| self.get_expected_yield_type(idx))
            {
                let ctx_type = if yield_expr.asterisk_token {
                    self.checker
                        .ctx
                        .arena
                        .get(yield_expr.expression)
                        .map(|n| n.kind)
                        .and_then(|kind| {
                            // Only direct call expressions (e.g. `yield* gen()`) should
                            // receive a generator contextual type. Await expressions
                            // (e.g. `yield* await promise.then(fn)`) must receive no
                            // contextual type at all here: `await` propagates its
                            // contextual type into the operand, and that would
                            // over-constrain `.then()` callback inference, producing
                            // spurious generic mismatches like TS2345/TS2504.
                            if kind == syntax_kind_ext::AWAIT_EXPRESSION {
                                return Some(TypeId::UNKNOWN);
                            }
                            if kind != syntax_kind_ext::CALL_EXPRESSION {
                                return None;
                            }
                            let expected_generator = self.get_expected_generator_type(idx)?;
                            let result_ctx = outer_contextual.unwrap_or(TypeId::UNKNOWN);
                            contextual_yield_star_return = Some(result_ctx);
                            let generator_ctx = tsz_solver::ContextualTypeContext::with_expected(
                                self.checker.ctx.types,
                                expected_generator,
                            );
                            let next_ctx = generator_ctx
                                .get_generator_next_type()
                                .unwrap_or(TypeId::UNKNOWN);
                            let generator_name = if is_async_generator {
                                "AsyncGenerator"
                            } else {
                                "Generator"
                            };
                            let lib_binders = self.checker.get_lib_binders();
                            let generator_sym = self
                                .checker
                                .ctx
                                .binder
                                .get_global_type_with_libs(generator_name, &lib_binders)?;
                            let generator_def =
                                self.checker.ctx.get_or_create_def_id(generator_sym);
                            let generator_base =
                                self.checker.ctx.types.factory().lazy(generator_def);
                            Some(
                                self.checker.ctx.types.factory().application(
                                    generator_base,
                                    vec![yield_ctx, result_ctx, next_ctx],
                                ),
                            )
                        })
                        .unwrap_or_else(|| {
                            // yield *[x => ...] needs Array<TYield> as contextual type
                            // so each array element gets TYield as its contextual type
                            self.checker.ctx.types.factory().array(yield_ctx)
                        })
                } else {
                    yield_ctx
                };
                self.checker
                    .clear_type_cache_recursive(yield_expr.expression);
                request.read().normal_origin().contextual(ctx_type)
            } else {
                request.read().normal_origin().contextual_opt(None)
            };
            let expression_type = self
                .checker
                .get_type_of_node_with_request(yield_expr.expression, &yield_request);
            if yield_expr.asterisk_token {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                if is_async_generator {
                    let is_iterable = self.checker.is_async_iterable_type(expression_type)
                        || self.checker.is_iterable_type(expression_type);
                    if !is_iterable {
                        let type_str = self.checker.format_type(expression_type);
                        let message = format_message(
                            diagnostic_messages::TYPE_MUST_HAVE_A_SYMBOL_ASYNCITERATOR_METHOD_THAT_RETURNS_AN_ASYNC_ITERATOR,
                            &[&type_str],
                        );
                        self.checker.error_at_node(
                            yield_expr.expression,
                            &message,
                            diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ASYNCITERATOR_METHOD_THAT_RETURNS_AN_ASYNC_ITERATOR,
                        );
                    }
                } else {
                    let is_iterable = self.checker.is_iterable_type(expression_type);
                    if !is_iterable {
                        let type_str = self.checker.format_type(expression_type);
                        let message = format_message(
                            diagnostic_messages::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR,
                            &[&type_str],
                        );
                        self.checker.error_at_node(
                            yield_expr.expression,
                            &message,
                            diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR,
                        );
                    }
                }

                if is_async_generator {
                    if self
                        .checker
                        .async_iterator_has_invalid_thenable_next_result(expression_type)
                    {
                        self.checker.error_at_node(
                            yield_expr.expression,
                            diagnostic_messages::TYPE_OF_AWAIT_OPERAND_MUST_EITHER_BE_A_VALID_PROMISE_OR_MUST_NOT_CONTAIN_A_CALLA,
                            diagnostic_codes::TYPE_OF_AWAIT_OPERAND_MUST_EITHER_BE_A_VALID_PROMISE_OR_MUST_NOT_CONTAIN_A_CALLA,
                        );
                    }
                    let async_info = tsz_solver::operations::get_iterator_info(
                        self.checker.ctx.types,
                        expression_type,
                        true,
                    );
                    let element = async_info.as_ref().map_or_else(
                        || {
                            tsz_solver::operations::get_async_iterable_element_type(
                                self.checker.ctx.types,
                                expression_type,
                            )
                        },
                        |i| i.yield_type,
                    );
                    // Capture the delegated iterator's return type for yield* expression result.
                    // Try get_iterator_info first (structural), then fall back to
                    // get_generator_return_type_argument (direct Application arg extraction)
                    // which handles Generator/AsyncGenerator types from lib.d.ts.
                    if let Some(ref i) = async_info {
                        yield_star_return_type = Some(i.return_type);
                    }
                    if yield_star_return_type.is_none() {
                        yield_star_return_type = self
                            .checker
                            .get_generator_return_type_argument(expression_type);
                    }
                    if yield_star_return_type
                        .is_none_or(|ty| ty == TypeId::UNKNOWN || ty == TypeId::ANY)
                        && let Some(ctx_return) = contextual_yield_star_return
                    {
                        yield_star_return_type = Some(ctx_return);
                    }
                    // Collect yield* element type for unannotated generators when resolvable
                    // (skip when async iterator info is None/fallback ANY).
                    // Always collect regardless of contextual yield type — the final
                    // generator yield type must come from actual body yields, not context
                    // (see function_type.rs comment on final_generator_yield_type).
                    if async_info.is_some() {
                        self.checker.ctx.generator_yield_operand_types.push(element);
                        // When yield* delegates to an async iterable with `any` element
                        // type, suppress TS7055 at the function level (see sync path).
                        if element == TypeId::ANY {
                            self.checker.ctx.generator_had_ts7057 = true;
                        }
                    }
                    element
                } else {
                    let info = tsz_solver::operations::get_iterator_info(
                        self.checker.ctx.types,
                        expression_type,
                        false,
                    );
                    // Capture the delegated iterator's return type for yield* expression result.
                    // Try get_iterator_info first (structural), then fall back to
                    // get_generator_return_type_argument (direct Application arg extraction).
                    if let Some(ref i) = info {
                        yield_star_return_type = Some(i.return_type);
                    }
                    if yield_star_return_type.is_none() {
                        yield_star_return_type = self
                            .checker
                            .get_generator_return_type_argument(expression_type);
                    }
                    // Collect yield* element type for unannotated generators when resolvable
                    // (skip when get_iterator_info returns None/fallback ANY).
                    // Always collect regardless of contextual yield type — the final
                    // generator yield type must come from actual body yields, not context
                    // (see function_type.rs comment on final_generator_yield_type).
                    if let Some(ref i) = info {
                        self.checker
                            .ctx
                            .generator_yield_operand_types
                            .push(i.yield_type);
                        // When yield* delegates to an iterable with `any` element type
                        // (e.g. `any[]`), suppress TS7055 at the function level.
                        // tsc considers the `any` yield type to be "explained" by
                        // the delegated iterable's type, not requiring a function-level
                        // implicit-any warning. Set the flag to suppress TS7055.
                        if i.yield_type == TypeId::ANY {
                            self.checker.ctx.generator_had_ts7057 = true;
                        }
                    }
                    info.map_or(TypeId::ANY, |i| i.yield_type)
                }
            } else {
                expression_type
            }
        };

        // Collect yield operand type for unannotated generators.
        // After body check, the union determines the inferred yield type for
        // TS7055/TS7025 vs TS7057 discrimination.
        // Always collect regardless of contextual yield type — the final
        // generator yield type must come from actual body yields, not context
        // (see function_type.rs comment on final_generator_yield_type).
        // Only collect for regular `yield expr` (not yield*), and skip when the
        // operand is itself a yield expression — its `any` result type is the TNext
        // fallback, not a real yielded value (e.g. `yield yield` should not make
        // TYield = any).
        if !yield_expr.asterisk_token {
            let operand_is_yield = yield_expr.expression.is_some()
                && self
                    .checker
                    .ctx
                    .arena
                    .get(yield_expr.expression)
                    .is_some_and(|n| n.kind == syntax_kind_ext::YIELD_EXPRESSION);
            if !operand_is_yield {
                self.checker
                    .ctx
                    .generator_yield_operand_types
                    .push(yielded_type);
            }
        }
        if let Some(expected_yield_type) = self.get_expected_yield_type(idx) {
            let error_node = if yield_expr.expression.is_none() {
                idx
            } else {
                yield_expr.expression
            };

            self.checker.ensure_relation_input_ready(yielded_type);
            self.checker
                .ensure_relation_input_ready(expected_yield_type);
            let resolved_expected_yield_type = self.checker.resolve_lazy_type(expected_yield_type);
            let syntactic_yield_allows_undefined = self
                .explicit_generator_yield_allows_undefined(idx)
                .unwrap_or(false);
            let bare_yield_requires_error = yield_expr.expression.is_none()
                && expected_yield_type != TypeId::ANY
                && expected_yield_type != TypeId::UNKNOWN
                && expected_yield_type != TypeId::ERROR
                && expected_yield_type != TypeId::VOID  // Allow bare yield for void
                && !syntactic_yield_allows_undefined
                && !tsz_solver::type_queries::type_includes_undefined(self.checker.ctx.types, expected_yield_type)
                && !tsz_solver::type_queries::type_includes_undefined(self.checker.ctx.types, resolved_expected_yield_type);

            // For yield*, check that the delegated iterable's element type is
            // assignable to the containing generator's expected yield type.
            // e.g. `yield * [new Baz]` in `function* g(): IterableIterator<Foo>`
            // checks Baz assignable to Foo → TS2741 if Baz is missing props from Foo.
            if yield_expr.asterisk_token {
                if !self.checker.type_contains_error(expected_yield_type)
                    && yielded_type != TypeId::ANY
                    && expected_yield_type != TypeId::ANY
                    && expected_yield_type != TypeId::UNKNOWN
                {
                    self.checker.check_assignable_or_report(
                        yielded_type,
                        expected_yield_type,
                        yield_expr.expression,
                    );
                }
                return yield_star_return_type.unwrap_or(TypeId::ANY);
            }
            if bare_yield_requires_error {
                self.checker.check_assignable_or_report(
                    yielded_type,
                    expected_yield_type,
                    error_node,
                );
            } else if !self.checker.type_contains_error(expected_yield_type)
                && !self.checker.check_assignable_or_report(
                    yielded_type,
                    expected_yield_type,
                    error_node,
                )
            {
                // Diagnostic emitted by check_assignable_or_report.
            }
        }

        // For yield*, the expression result type is the RETURN type of the delegated
        // iterator (TReturn), not the TNext of the containing generator. This applies
        // regardless of whether the containing generator has a return type annotation.
        // e.g., `const x = yield* gen` where gen: Generator<Y, R, N> → x has type R.
        if yield_expr.asterisk_token {
            if let Some(ret_type) = yield_star_return_type {
                return ret_type;
            }
            return TypeId::ANY;
        }

        // TypeScript models `yield` result type as the value received by `.next(...)` (TNext).
        // Extract TNext from Generator<TYield, TReturn, TNext> or AsyncGenerator<TYield, TReturn, TNext>.
        // First try the checker-side extraction which handles heritage resolution
        // (e.g., `interface I1 extends Iterator<0, 1, 2> {}` → TNext = 2).
        if let Some(generator_type) = self.get_expected_generator_type(idx) {
            if let Some(next_type) = self
                .checker
                .get_generator_next_type_argument(generator_type)
            {
                return next_type;
            }
            // Fallback to solver's contextual extraction for direct Application types
            let ctx = tsz_solver::ContextualTypeContext::with_expected(
                self.checker.ctx.types,
                generator_type,
            );
            if let Some(next_type) = ctx.get_generator_next_type() {
                return next_type;
            }
        }

        // Check the contextual generator next type stack — this is populated when
        // a generator function is contextually typed (e.g., assigned to a variable
        // with a Generator<Y, R, N> type or passed as a callback with explicit
        // type arguments). The next type from the contextual Generator type tells
        // us what `.next()` will pass, so we don't need the explicit annotation.
        if let Some(next_type) = self.checker.ctx.current_generator_next_type() {
            return next_type;
        }

        // Fallback to `any` if no generator context is available.
        // Emit TS7057 when noImplicitAny is enabled, the generator lacks a return type,
        // and the yield result is consumed (not discarded).
        if self.checker.ctx.no_implicit_any() && !self.expression_result_is_unused(idx) {
            let yield_type = self.checker.ctx.current_yield_type();
            let contextual = request.contextual_type;
            // Suppress TS7057 when:
            // - yield_type is Some(ANY): the yield type itself is any, so TS7055/7025 covers it
            // - contextual type provides a concrete non-any, non-type-parameter type
            //   (a type parameter like T doesn't provide meaningful context — the yield
            //   result will still be inferred as any)
            // - yield is the initializer of a destructuring variable declaration (TSC derives
            //   a contextual type from the binding pattern, suppressing TS7057)
            let contextual_is_concrete = contextual.is_some_and(|t| {
                if t == TypeId::ANY {
                    return false;
                }
                // A type parameter from a call argument (e.g. f2<T>(yield) where param is T)
                // doesn't provide meaningful context — T gets inferred as any from the yield.
                // But a type parameter from a variable annotation (e.g. const a: T = yield 0)
                // IS a valid contextual type that suppresses TS7057.
                if tsz_solver::type_queries::is_type_parameter_like(self.checker.ctx.types, t)
                    && self.yield_is_direct_call_argument(idx)
                {
                    return false;
                }
                true
            });
            if yield_type != Some(TypeId::ANY)
                && !contextual_is_concrete
                && !self.yield_is_in_binding_pattern_initializer(idx)
                && !self.yield_is_direct_dynamic_import_argument(idx)
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.checker.error_at_node(
                    idx,
                    diagnostic_messages::YIELD_EXPRESSION_IMPLICITLY_RESULTS_IN_AN_ANY_TYPE_BECAUSE_ITS_CONTAINING_GENERA,
                    diagnostic_codes::YIELD_EXPRESSION_IMPLICITLY_RESULTS_IN_AN_ANY_TYPE_BECAUSE_ITS_CONTAINING_GENERA,
                );
                // Track that TS7057 was emitted so TS7055 is suppressed at the
                // function level (tsc emits one or the other, not both).
                self.checker.ctx.generator_had_ts7057 = true;
            }
        }
        TypeId::ANY
    }

    /// Check if an expression's result value is unused (discarded).
    fn expression_result_is_unused(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        loop {
            let Some(ext) = self.checker.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            let Some(parent) = self.checker.ctx.arena.get(parent_idx) else {
                return false;
            };

            // Walk up through parenthesized expressions
            if parent.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                current = parent_idx;
                continue;
            }
            // Expression statement: result is unused
            if parent.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
                return true;
            }
            // Void expression: result is unused.
            // Our parser models `void expr` as PREFIX_UNARY_EXPRESSION with VoidKeyword operator.
            if parent.kind == syntax_kind_ext::VOID_EXPRESSION {
                return true;
            }
            if parent.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && let Some(unary) = self.checker.ctx.arena.get_unary_expr(parent)
                && unary.operator == SyntaxKind::VoidKeyword as u16
            {
                return true;
            }
            // For statement: initializer and incrementor results are unused
            if parent.kind == syntax_kind_ext::FOR_STATEMENT {
                if let Some(loop_data) = self.checker.ctx.arena.get_loop(parent)
                    && (loop_data.initializer == current || loop_data.incrementor == current)
                {
                    return true;
                }
                return false;
            }
            // Decorator expression: the result of a decorator is applied to the
            // decorated declaration but is not "used" as an expression result.
            // This suppresses false TS7057 for `@(yield 0) class C {}`.
            if parent.kind == syntax_kind_ext::DECORATOR {
                return true;
            }
            // Binary comma expression: left side is always unused;
            // right side is unused if the parent comma expression is unused
            if parent.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.checker.ctx.arena.get_binary_expr(parent)
                && bin.operator_token == SyntaxKind::CommaToken as u16
            {
                if current == bin.left {
                    return true;
                }
                // Right side: walk up to check if parent is unused
                current = parent_idx;
                continue;
            }

            return false;
        }
    }

    /// Check if a yield expression is in a binding pattern initializer (suppresses TS7057).
    fn yield_is_in_binding_pattern_initializer(&self, idx: NodeIndex) -> bool {
        let Some(ext) = self.checker.ctx.arena.get_extended(idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        let Some(var_decl) = self
            .checker
            .ctx
            .arena
            .get(parent_idx)
            .and_then(|p| self.checker.ctx.arena.get_variable_declaration(p))
        else {
            return false;
        };
        // Check if the yield is the direct initializer
        if var_decl.initializer != idx {
            return false;
        }
        // Check if the variable name is a binding pattern
        self.checker
            .ctx
            .arena
            .get(var_decl.name)
            .is_some_and(|name_node| {
                name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            })
    }

    /// Check if a yield expression is a direct argument of a call or new expression.
    fn yield_is_direct_call_argument(&self, idx: NodeIndex) -> bool {
        let Some(ext) = self.checker.ctx.arena.get_extended(idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        let Some(parent) = self.checker.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent.kind == syntax_kind_ext::CALL_EXPRESSION
            || parent.kind == syntax_kind_ext::NEW_EXPRESSION
        {
            // Check if yield is in the arguments list (not the callee expression)
            if let Some(call) = self.checker.ctx.arena.get_call_expr(parent)
                && let Some(ref args) = call.arguments
            {
                return args.nodes.contains(&idx);
            }
        }
        false
    }

    /// Check if a yield expression is inside a dynamic import argument (suppresses TS7057).
    fn yield_is_direct_dynamic_import_argument(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        let mut guard = 0u32;
        while current.is_some() {
            guard += 1;
            if guard > 4096 {
                return false;
            }
            let Some(node) = self.checker.ctx.arena.get(current) else {
                return false;
            };
            if let Some(call) = self.checker.ctx.arena.get_call_expr(node)
                && self.checker.is_dynamic_import(call)
                && let Some(args) = call.arguments.as_ref()
                && args
                    .nodes
                    .iter()
                    .any(|&arg_idx| self.node_contains_descendant(arg_idx, idx))
            {
                return true;
            }
            let Some(ext) = self.checker.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
        }
        false
    }

    fn node_contains_descendant(&self, ancestor: NodeIndex, mut descendant: NodeIndex) -> bool {
        let mut guard = 0u32;
        while descendant.is_some() {
            if descendant == ancestor {
                return true;
            }
            guard += 1;
            if guard > 4096 {
                return false;
            }
            let Some(ext) = self.checker.ctx.arena.get_extended(descendant) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            descendant = ext.parent;
        }
        false
    }
}
