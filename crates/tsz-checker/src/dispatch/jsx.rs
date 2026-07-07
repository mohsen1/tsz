//! Dispatcher for JSX element type computation.
//!
//! Extracted from `dispatch.rs` to keep that file under the §19 hard limit of 2000 lines.
//! Handles `JSX_ELEMENT` children synthesis, contextual typing, and component-type resolution.

use crate::context::TypingRequest;
use crate::query_boundaries::checkers::jsx as jsx_query;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::ExpressionDispatcher;

impl<'a, 'b> ExpressionDispatcher<'a, 'b> {
    /// Compute the type of a `JSX_ELEMENT` node.
    ///
    /// Processes children for contextual typing (arrows/functions in child
    /// positions), synthesizes the `children` prop type from child types, checks
    /// the closing tag, and delegates component/intrinsic type resolution to
    /// `get_type_of_jsx_opening_element_with_children`.
    pub(super) fn dispatch_jsx_element(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let Some(node) = self.checker.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        if let Some(jsx) = self.checker.ctx.arena.get_jsx_element(node) {
            // Extract contextual type for children from the component's
            // `children` prop BEFORE processing children, so arrow functions
            // and other expressions get contextual parameter typing.
            let children_ctx_type = if !jsx.children.nodes.is_empty() {
                self.checker
                    .get_jsx_children_contextual_type(jsx.opening_element)
            } else {
                None
            };
            let normal_request = request.read().normal_origin().contextual_opt(None);
            let children_request = request
                .read()
                .normal_origin()
                .contextual_opt(children_ctx_type);
            let child_diagnostics_start = self.checker.ctx.diagnostics.len();
            // Collect children types for children prop synthesis.
            // tsc synthesizes a `children` prop from JSX element body children
            // and validates it against the component's `children` prop type.
            let mut child_types: Vec<TypeId> = Vec::new();
            let mut has_text_child = false;
            let mut text_child_indices: Vec<NodeIndex> = Vec::new();
            let mut has_spread_child = false;
            for &child in &jsx.children.nodes {
                if let Some(child_node) = self.checker.ctx.arena.get(child) {
                    // Skip trivial whitespace JsxText — tsc ignores whitespace-only
                    // text that contains newlines (formatting indentation). But
                    // same-line whitespace (e.g., `<A />  <B />`) is preserved.
                    if child_node.kind == SyntaxKind::JsxText as u16
                        && let Some(text) = self.checker.ctx.arena.get_jsx_text(child_node)
                    {
                        let is_all_whitespace = text.text.chars().all(|c| c.is_ascii_whitespace());
                        let has_newline = text.text.contains('\n');
                        if is_all_whitespace && has_newline {
                            continue;
                        }
                    }
                    // Skip empty JSX expressions (e.g., {/* comment */})
                    // — tsc does not count these as children.
                    if child_node.kind == syntax_kind_ext::JSX_EXPRESSION
                        && let Some(expr_data) =
                            self.checker.ctx.arena.get_jsx_expression(child_node)
                        && expr_data.expression == NodeIndex::NONE
                    {
                        continue;
                    }
                }
                let child_type = if let Some(child_node) = self.checker.ctx.arena.get(child)
                    && child_node.kind == syntax_kind_ext::JSX_EXPRESSION
                    && let Some(expr_data) = self.checker.ctx.arena.get_jsx_expression(child_node)
                    && expr_data.dot_dot_dot_token
                {
                    has_spread_child = true;
                    let spread_type = self
                        .checker
                        .get_type_of_node_with_request(expr_data.expression, &children_request);
                    self.checker
                        .normalize_jsx_spread_child_type(child, spread_type)
                } else if let Some(child_node) = self.checker.ctx.arena.get(child)
                    && child_node.kind == syntax_kind_ext::JSX_EXPRESSION
                    && let Some(expr_data) = self.checker.ctx.arena.get_jsx_expression(child_node)
                    && expr_data.expression.is_some()
                    && self
                        .checker
                        .ctx
                        .arena
                        .get(expr_data.expression)
                        .is_some_and(|expr| {
                            matches!(
                                expr.kind,
                                syntax_kind_ext::ARROW_FUNCTION
                                    | syntax_kind_ext::FUNCTION_EXPRESSION
                            )
                        })
                {
                    let has_function_context =
                        children_request.contextual_type.is_some_and(|ctx_type| {
                            let ctx_type = self.checker.resolve_type_for_property_access(ctx_type);
                            crate::query_boundaries::common::function_shape_for_type(
                                self.checker.ctx.types,
                                ctx_type,
                            )
                            .is_some()
                                || crate::query_boundaries::common::call_signatures_for_type(
                                    self.checker.ctx.types,
                                    ctx_type,
                                )
                                .is_some_and(|sigs| !sigs.is_empty())
                        });
                    if has_function_context {
                        self.checker
                            .ctx
                            .implicit_any_contextual_closures
                            .insert(expr_data.expression);
                        self.checker
                            .ctx
                            .implicit_any_checked_closures
                            .insert(expr_data.expression);
                        self.checker
                            .invalidate_function_like_for_contextual_retry(expr_data.expression);
                        self.checker
                            .get_type_of_node_with_request(expr_data.expression, &children_request)
                    } else {
                        // Still process the arrow/function expression to emit
                        // diagnostics (e.g., TS7006 for unannotated parameters).
                        // Without a callable contextual type, the result is ANY,
                        // but we must evaluate the function to trigger TS7006.
                        self.checker.get_type_of_node(child);
                        TypeId::ANY
                    }
                } else if let Some(child_node) = self.checker.ctx.arena.get(child)
                    && matches!(
                        child_node.kind,
                        syntax_kind_ext::JSX_ELEMENT
                            | syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                            | syntax_kind_ext::JSX_FRAGMENT
                    )
                {
                    self.checker
                        .get_type_of_node_with_request(child, &normal_request)
                } else {
                    self.checker
                        .get_type_of_node_with_request(child, &children_request)
                };
                if let Some(child_node) = self.checker.ctx.arena.get(child)
                    && child_node.kind == SyntaxKind::JsxText as u16
                {
                    has_text_child = true;
                    text_child_indices.push(child);
                }
                child_types.push(child_type);
            }
            // Synthesize the children type:
            // - 0 children → None (no children prop synthesized)
            // - 1 child → the child's type directly
            // - 2+ children without spread → fixed tuple of child types
            // - 2+ children with spread → array of union of child types
            let children_ctx = if !child_types.is_empty() {
                let child_count = child_types.len();
                let synthesized_type = if child_types.len() == 1 && !has_spread_child {
                    child_types[0]
                } else if !has_spread_child {
                    jsx_query::tuple_type_from_required_element_types(
                        self.checker.ctx.types,
                        child_types.to_vec(),
                    )
                } else {
                    // Multiple children: synthesize as an array type.
                    // tsc uses the union of all child types as the element type.
                    let element_type = jsx_query::union_type_from_members(
                        self.checker.ctx.types,
                        child_types.clone(),
                    );
                    jsx_query::array_type_from_element(self.checker.ctx.types, element_type)
                };
                let normalized_child_count = if has_spread_child {
                    child_count.max(2)
                } else {
                    child_count
                };
                Some(crate::checkers_domain::JsxChildrenContext {
                    child_count: normalized_child_count,
                    has_text_child,
                    contextual_type: children_ctx_type,
                    synthesized_type,
                    text_child_indices,
                })
            } else {
                None
            };
            // Check closing element for TS7026 (tsc emits for both opening and closing tags)
            self.checker
                .check_jsx_closing_element_for_implicit_any(jsx.closing_element);
            // tsc also re-checks the closing tag's name identifier, so an
            // unresolved component name emits TS2304 at both the opening
            // and closing positions. Route the closing tag_name through
            // type resolution so the standard identifier-lookup path
            // produces the diagnostic.
            if let Some(closing_node) = self.checker.ctx.arena.get(jsx.closing_element)
                && let Some(closing_data) = self.checker.ctx.arena.get_jsx_closing(closing_node)
            {
                let tag_name_idx = closing_data.tag_name;
                if let Some(tag_name_node) = self.checker.ctx.arena.get(tag_name_idx)
                    && tag_name_node.kind == SyntaxKind::Identifier as u16
                    && let Some(ident) = self.checker.ctx.arena.get_identifier(tag_name_node)
                    && ident
                        .escaped_text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_uppercase())
                {
                    let _ = self
                        .checker
                        .get_type_of_node_with_request(tag_name_idx, request);
                }
                if let Some(tag_name_node) = self.checker.ctx.arena.get(tag_name_idx)
                    && tag_name_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                {
                    let mut root_idx = tag_name_idx;
                    while let Some(root_node) = self.checker.ctx.arena.get(root_idx)
                        && root_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        && let Some(access) = self.checker.ctx.arena.get_access_expr(root_node)
                    {
                        root_idx = access.expression;
                    }
                    if let Some(root_node) = self.checker.ctx.arena.get(root_idx)
                        && root_node.kind == SyntaxKind::Identifier as u16
                        && let Some(ident) = self.checker.ctx.arena.get_identifier(root_node)
                        && ident
                            .escaped_text
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_uppercase())
                    {
                        let _ = self
                            .checker
                            .get_type_of_node_with_request(root_idx, request);
                    }
                }
            }
            let opening_element = jsx.opening_element;
            let result = self.checker.get_type_of_jsx_opening_element_with_children(
                opening_element,
                request,
                children_ctx,
            );
            let opening_span = self.checker.get_node_span(opening_element);
            let has_parent_overload_failure = self
                .checker
                .ctx
                .diagnostics
                .iter()
                .skip(child_diagnostics_start)
                .any(|diagnostic| {
                    diagnostic.code
                        == crate::diagnostics::diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL
                        && opening_span.is_none_or(|(start, end)| {
                            diagnostic.start >= start && diagnostic.start <= end
                        })
                });
            if has_parent_overload_failure {
                let child_spans: Vec<(u32, u32)> = jsx
                    .children
                    .nodes
                    .iter()
                    .filter(|&&child_idx| self.jsx_child_is_intrinsic_element(child_idx))
                    .filter_map(|&child_idx| self.checker.get_node_span(child_idx))
                    .collect();
                let mut diagnostic_index = 0usize;
                self.checker.ctx.diagnostics.retain(|diagnostic| {
                    let current_index = diagnostic_index;
                    diagnostic_index += 1;
                    let is_missing_property = matches!(
                        diagnostic.code,
                        crate::diagnostics::diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                            | crate::diagnostics::diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
                    );
                    let is_child_diagnostic = child_spans
                        .iter()
                        .any(|&(start, end)| diagnostic.start >= start && diagnostic.start <= end);
                    current_index < child_diagnostics_start
                        || !is_missing_property
                        || !is_child_diagnostic
                });
            }
            result
        } else {
            TypeId::ERROR
        }
    }

    fn jsx_child_is_intrinsic_element(&self, child_idx: NodeIndex) -> bool {
        let Some(child_node) = self.checker.ctx.arena.get(child_idx) else {
            return false;
        };
        let opening_idx = match child_node.kind {
            syntax_kind_ext::JSX_ELEMENT => {
                let Some(jsx) = self.checker.ctx.arena.get_jsx_element(child_node) else {
                    return false;
                };
                jsx.opening_element
            }
            syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => child_idx,
            _ => return false,
        };
        let Some(opening_node) = self.checker.ctx.arena.get(opening_idx) else {
            return false;
        };
        let Some(opening) = self.checker.ctx.arena.get_jsx_opening(opening_node) else {
            return false;
        };
        let Some(tag_name_node) = self.checker.ctx.arena.get(opening.tag_name) else {
            return false;
        };
        if tag_name_node.kind == SyntaxKind::Identifier as u16 {
            return self
                .checker
                .ctx
                .arena
                .get_identifier(tag_name_node)
                .is_some_and(|ident| {
                    ident
                        .escaped_text
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_lowercase())
                });
        }
        tag_name_node.kind == syntax_kind_ext::JSX_NAMESPACED_NAME
    }
}
