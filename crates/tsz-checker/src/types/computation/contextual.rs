//! Contextual sensitivity analysis for type inference.
//!
//! Determines whether an expression's type depends on its surrounding context
//! (e.g., arrow functions with unannotated params, object/array literals).
//! Used for two-pass generic type inference where contextually sensitive
//! arguments are deferred to Round 2.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::syntax::transform_utils::contains_this_reference;

fn callee_needs_contextual_return_type(state: &CheckerState, callee_idx: NodeIndex) -> bool {
    use tsz_parser::parser::syntax_kind_ext;

    let callee_idx = state
        .ctx
        .arena
        .skip_parenthesized_and_assertions(callee_idx);
    let Some(node) = state.ctx.arena.get(callee_idx) else {
        return false;
    };

    match node.kind {
        k if k == syntax_kind_ext::ARROW_FUNCTION || k == syntax_kind_ext::FUNCTION_EXPRESSION => {
            state
                .ctx
                .arena
                .get_function(node)
                .is_some_and(|func| function_body_needs_contextual_return_type(state, func.body))
        }
        _ => false,
    }
}

/// A node is contextually sensitive if its type cannot be fully determined
/// without an expected type from its parent. This includes:
/// - Arrow functions and function expressions
/// - Object literals (if ANY property is sensitive)
/// - Array literals (if ANY element is sensitive)
/// - Parenthesized expressions (pass through)
///
/// This is used for two-pass generic type inference, where contextually
/// sensitive arguments are deferred to Round 2 after non-contextual
/// arguments have been processed and type parameters have been partially inferred.
pub(crate) fn is_contextually_sensitive(state: &CheckerState, idx: NodeIndex) -> bool {
    use tsz_parser::parser::syntax_kind_ext;

    let Some(node) = state.ctx.arena.get(idx) else {
        return false;
    };

    match node.kind {
        // Methods (standalone, not as object literal element) follow the same rules
        // as arrow/function expressions for sensitivity.
        k if k == syntax_kind_ext::METHOD_DECLARATION => {
            if let Some(method) = state.ctx.arena.get_method_decl(node) {
                let has_unannotated_params = method.parameters.nodes.iter().any(|&param_idx| {
                    state
                        .ctx
                        .arena
                        .get(param_idx)
                        .and_then(|pn| state.ctx.arena.get_parameter(pn))
                        .is_some_and(|p| p.type_annotation.is_none())
                });
                let zero_param_contextual_this = method.parameters.nodes.is_empty()
                    && method.type_annotation.is_none()
                    && contains_this_reference(state.ctx.arena, method.body);
                has_unannotated_params
                    || zero_param_contextual_this
                    || (method.parameters.nodes.is_empty()
                        && method.type_annotation.is_none()
                        && function_body_needs_contextual_return_type(state, method.body))
            } else {
                true
            }
        }

        // Functions are sensitive ONLY if they have at least one parameter without a type annotation
        k if k == syntax_kind_ext::ARROW_FUNCTION || k == syntax_kind_ext::FUNCTION_EXPRESSION => {
            if let Some(func) = state.ctx.arena.get_function(node) {
                let has_unannotated_params = func.parameters.nodes.iter().any(|&param_idx| {
                    if let Some(param_node) = state.ctx.arena.get(param_idx)
                        && let Some(param) = state.ctx.arena.get_parameter(param_node)
                    {
                        return param.type_annotation.is_none();
                    }
                    false
                });

                has_unannotated_params
                    || (func.parameters.nodes.is_empty()
                        && func.type_annotation.is_none()
                        && function_body_needs_contextual_return_type(state, func.body))
            } else {
                false
            }
        }

        // Parentheses just pass through sensitivity
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
            if let Some(paren) = state.ctx.arena.get_parenthesized(node) {
                is_contextually_sensitive(state, paren.expression)
            } else {
                false
            }
        }

        // Conditional Expressions: Sensitive if either branch is sensitive
        k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
            if let Some(cond) = state.ctx.arena.get_conditional_expr(node) {
                is_contextually_sensitive(state, cond.when_true)
                    || is_contextually_sensitive(state, cond.when_false)
            } else {
                false
            }
        }

        // Nested calls/constructs: sensitive if any of their own arguments are sensitive.
        // This lets outer generic calls defer wrapper expressions like
        // `handler(type, state => state)` to Round 2, so the outer call can first
        // infer type arguments from non-contextual inputs and then provide a concrete
        // contextual return type to the inner generic call.
        k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
            state.ctx.arena.get_call_expr(node).is_some_and(|call| {
                call.arguments.as_ref().is_some_and(|args| {
                    args.nodes
                        .iter()
                        .any(|&arg| is_contextually_sensitive(state, arg))
                }) || callee_needs_contextual_return_type(state, call.expression)
            })
        }

        // Object Literals: Sensitive if any property is sensitive
        k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
            if let Some(obj) = state.ctx.arena.get_literal_expr(node) {
                for &element_idx in &obj.elements.nodes {
                    if let Some(element) = state.ctx.arena.get(element_idx) {
                        match element.kind {
                            // Standard property: check initializer
                            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                                if let Some(prop) = state.ctx.arena.get_property_assignment(element)
                                    && is_contextually_sensitive(state, prop.initializer)
                                {
                                    return true;
                                }
                            }
                            // Shorthand property: { x } refers to a variable, never sensitive
                            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                                // Variable references are not contextually sensitive
                                // (their type is already known from their declaration)
                            }
                            // Spread: check the expression being spread
                            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                                if let Some(spread) = state.ctx.arena.get_spread(element)
                                    && is_contextually_sensitive(state, spread.expression)
                                {
                                    return true;
                                }
                            }
                            // Methods: sensitive only if they have unannotated params
                            // (same rule as arrow/function expressions). Fully annotated
                            // methods should participate in Round 1 inference.
                            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                if let Some(method) = state.ctx.arena.get_method_decl(element) {
                                    let has_unannotated =
                                        method.parameters.nodes.iter().any(|&param_idx| {
                                            state
                                                .ctx
                                                .arena
                                                .get(param_idx)
                                                .and_then(|pn| state.ctx.arena.get_parameter(pn))
                                                .is_some_and(|p| p.type_annotation.is_none())
                                        });
                                    let zero_param_contextual_this = method
                                        .parameters
                                        .nodes
                                        .is_empty()
                                        && method.type_annotation.is_none()
                                        && contains_this_reference(state.ctx.arena, method.body);
                                    if has_unannotated
                                        || zero_param_contextual_this
                                        || (method.parameters.nodes.is_empty()
                                            && method.type_annotation.is_none()
                                            && function_body_needs_contextual_return_type(
                                                state,
                                                method.body,
                                            ))
                                    {
                                        return true;
                                    }
                                } else {
                                    return true;
                                }
                            }
                            // Accessors are always sensitive
                            k if k == syntax_kind_ext::GET_ACCESSOR
                                || k == syntax_kind_ext::SET_ACCESSOR =>
                            {
                                return true;
                            }
                            _ => {}
                        }
                    }
                }
            }
            false
        }

        // Array Literals: Sensitive if any element is sensitive
        k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
            if let Some(arr) = state.ctx.arena.get_literal_expr(node) {
                for &element_idx in &arr.elements.nodes {
                    if is_contextually_sensitive(state, element_idx) {
                        return true;
                    }
                }
            }
            false
        }

        // Spread Elements (in arrays)
        k if k == syntax_kind_ext::SPREAD_ELEMENT => {
            if let Some(spread) = state.ctx.arena.get_spread(node) {
                is_contextually_sensitive(state, spread.expression)
            } else {
                false
            }
        }

        _ => false,
    }
}

fn function_body_needs_contextual_return_type(state: &CheckerState, body_idx: NodeIndex) -> bool {
    use tsz_parser::parser::syntax_kind_ext;

    let Some(body_node) = state.ctx.arena.get(body_idx) else {
        return false;
    };

    if body_node.kind != syntax_kind_ext::BLOCK {
        return expression_needs_contextual_return_type(state, body_idx);
    }

    // For block bodies, use the stricter `is_contextually_sensitive` check on return
    // expressions rather than the broader `expression_needs_contextual_return_type`.
    //
    // tsc's `hasContextSensitiveReturnExpression` returns false for all block bodies.
    // We can't go that far because our inference pipeline needs the two-pass flow
    // for block-bodied functions returning context-sensitive expressions (e.g.,
    // `() => { return a => a + 1 }` where `a` needs contextual type from outer generic).
    //
    // But `expression_needs_contextual_return_type` is too broad — it flags ALL object
    // literals, array literals, and call expressions, even non-sensitive ones. This
    // incorrectly makes methods like `state() { return { bar2: 1 }; }` context-sensitive,
    // preventing them from contributing to Round 1 generic inference.
    //
    // The stricter check only flags truly context-sensitive return expressions
    // (those with unannotated params, sensitive nested objects, etc.).
    let Some(block) = state.ctx.arena.get_block(body_node) else {
        return false;
    };

    block.statements.nodes.iter().any(|&stmt_idx| {
        let Some(stmt_node) = state.ctx.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
            return false;
        }
        state
            .ctx
            .arena
            .get_return_statement(stmt_node)
            .is_some_and(|ret| {
                ret.expression.is_some() && is_contextually_sensitive(state, ret.expression)
            })
    })
}

pub(crate) fn expression_needs_contextual_return_type(
    state: &CheckerState,
    expr_idx: NodeIndex,
) -> bool {
    use tsz_parser::parser::syntax_kind_ext;

    if is_contextually_sensitive(state, expr_idx) {
        return true;
    }

    let Some(node) = state.ctx.arena.get(expr_idx) else {
        return false;
    };

    match node.kind {
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => state
            .ctx
            .arena
            .get_parenthesized(node)
            .is_some_and(|paren| expression_needs_contextual_return_type(state, paren.expression)),
        k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => state
            .ctx
            .arena
            .get_conditional_expr(node)
            .is_some_and(|cond| {
                expression_needs_contextual_return_type(state, cond.when_true)
                    || expression_needs_contextual_return_type(state, cond.when_false)
            }),
        k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || k == syntax_kind_ext::CALL_EXPRESSION
            || k == syntax_kind_ext::NEW_EXPRESSION
            || k == syntax_kind_ext::YIELD_EXPRESSION
            || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
        {
            true
        }
        _ => false,
    }
}
