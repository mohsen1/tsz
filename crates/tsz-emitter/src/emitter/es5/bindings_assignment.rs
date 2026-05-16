//! ES5 destructuring - for-of array indexing and assignment destructuring.

use super::super::{Printer, is_valid_identifier_name};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

enum AssignmentRestProp {
    Static(String),
    Dynamic(String),
}

impl<'a> Printer<'a> {
    // =========================================================================
    // Assignment destructuring lowering (ES5)
    // Lowers: [, nameA] = expr  →  nameA = expr[1]
    //         { name: nameA } = expr  →  nameA = expr.name
    // =========================================================================

    /// Count the total number of elements (including holes) in an array destructuring pattern.
    /// TypeScript creates a temp for non-identifier sources when there are 2+ elements
    /// (including holes). With exactly 1 element (no holes), it inlines the source.
    const fn count_array_destructuring_elements(&self, elements: &[NodeIndex]) -> usize {
        elements.len()
    }

    /// Count elements in an object destructuring pattern for temp-variable optimization.
    const fn count_object_destructuring_elements(&self, elements: &[NodeIndex]) -> usize {
        elements.len()
    }

    fn emit_assignment_target(&mut self, target_idx: NodeIndex) {
        if self.emit_commonjs_live_export_assignment_target(target_idx) {
            return;
        }
        self.emit(target_idx);
    }

    pub(in crate::emitter) fn assignment_pattern_has_commonjs_live_export_target(
        &self,
        pattern_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(pattern_idx) else {
            return false;
        };
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let name = crate::transforms::emit_utils::identifier_text_or_empty(
                    self.arena,
                    pattern_idx,
                );
                self.commonjs_live_export_assignment_target_name_needs_chain(&name)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.arena.get_binary_expr(node).is_some_and(|binary| {
                    binary.operator_token == SyntaxKind::EqualsToken as u16
                        && self.assignment_pattern_has_commonjs_live_export_target(binary.left)
                })
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT
                || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
            {
                self.arena.get_spread(node).is_some_and(|spread| {
                    self.assignment_pattern_has_commonjs_live_export_target(spread.expression)
                })
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                .arena
                .get_property_assignment(node)
                .is_some_and(|prop| {
                    self.assignment_pattern_has_commonjs_live_export_target(prop.initializer)
                }),
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                .arena
                .get_shorthand_property(node)
                .map(|shorthand| {
                    crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena,
                        shorthand.name,
                    )
                })
                .is_some_and(|name| {
                    self.commonjs_live_export_assignment_target_name_needs_chain(&name)
                }),
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || k == syntax_kind_ext::OBJECT_BINDING_PATTERN =>
            {
                self.get_binding_or_literal_elements(node)
                    .is_some_and(|elements| {
                        elements.into_iter().any(|element| {
                            !element.is_none()
                                && self.assignment_pattern_has_commonjs_live_export_target(element)
                        })
                    })
            }
            _ => false,
        }
    }

    /// Unwrap a chain of empty destructuring assignments to find the effective RHS.
    /// For example, `{} = [] = {} = a` reduces to just `a` since all patterns are empty.
    fn unwrap_empty_destructuring_chain(&self, idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.arena.get(idx) else {
            return idx;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return idx;
        }
        let Some(bin) = self.arena.get_binary_expr(node) else {
            return idx;
        };
        if bin.operator_token != SyntaxKind::EqualsToken as u16 {
            return idx;
        }
        let Some(left_node) = self.arena.get(bin.left) else {
            return idx;
        };
        // Check if the LHS is an empty destructuring pattern
        let is_empty = match left_node.kind {
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => self
                .arena
                .get_literal_expr(left_node)
                .is_some_and(|lit| lit.elements.nodes.is_empty()),
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => self
                .arena
                .get_literal_expr(left_node)
                .is_some_and(|lit| lit.elements.nodes.is_empty()),
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => self
                .arena
                .get_binding_pattern(left_node)
                .is_some_and(|p| p.elements.nodes.is_empty()),
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => self
                .arena
                .get_binding_pattern(left_node)
                .is_some_and(|p| p.elements.nodes.is_empty()),
            _ => false,
        };
        if is_empty {
            // Recursively unwrap in case of chained empty patterns
            self.unwrap_empty_destructuring_chain(bin.right)
        } else {
            idx
        }
    }

    /// Lower an assignment destructuring pattern to ES5.
    /// Called from `emit_binary_expression` when left side is array/object literal.
    pub(in crate::emitter) fn emit_assignment_destructuring_es5(
        &mut self,
        left_node: &Node,
        right_idx: NodeIndex,
    ) {
        // For empty patterns ({} = a, [] = a), just emit the right-hand side
        // so it's evaluated for side effects. This must be checked BEFORE creating
        // any temp variables, since the temp creation emits to the output buffer.
        let is_empty_pattern = match left_node.kind {
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => self
                .arena
                .get_literal_expr(left_node)
                .is_some_and(|lit| lit.elements.nodes.is_empty()),
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => self
                .arena
                .get_literal_expr(left_node)
                .is_some_and(|lit| lit.elements.nodes.is_empty()),
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => self
                .arena
                .get_binding_pattern(left_node)
                .is_some_and(|p| p.elements.nodes.is_empty()),
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => self
                .arena
                .get_binding_pattern(left_node)
                .is_some_and(|p| p.elements.nodes.is_empty()),
            _ => false,
        };
        if is_empty_pattern {
            self.emit(right_idx);
            return;
        }

        // Determine if right side is a simple identifier (can be accessed directly).
        // Also check if the RHS is a destructuring assignment with an empty pattern,
        // which reduces to just the inner RHS (e.g., `{} = a` evaluates to `a`).
        let effective_right_idx = self.unwrap_empty_destructuring_chain(right_idx);
        let mut is_simple = self
            .arena
            .get(effective_right_idx)
            .is_some_and(|n| n.is_identifier());

        // `({ foo, bar } = foo)` — when the LHS reassigns the same identifier
        // we'd be reading from on the RHS, the second access (`bar = foo.bar`)
        // sees the clobbered value. Force a temp so `_a = foo, foo = _a.foo,
        // bar = _a.bar` captures the original RHS first.
        if is_simple {
            let rhs_name = crate::transforms::emit_utils::identifier_text_or_empty(
                self.arena,
                effective_right_idx,
            );
            if !rhs_name.is_empty()
                && self.assignment_lhs_reassigns_identifier(left_node, &rhs_name)
            {
                is_simple = false;
            }
        }

        // Count elements to determine if we need a temp for complex sources.
        // TypeScript creates a temp for non-identifier sources when there are 2+ elements
        // (including holes). With exactly 1 element (no holes), it inlines the source.
        let element_count = if is_simple {
            0 // doesn't matter for identifiers
        } else {
            match left_node.kind {
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                    if let Some(lit) = self.arena.get_literal_expr(left_node) {
                        self.count_array_destructuring_elements(&lit.elements.nodes)
                    } else {
                        2 // fallback: assume needs temp
                    }
                }
                k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                    if let Some(pattern) = self.arena.get_binding_pattern(left_node) {
                        self.count_array_destructuring_elements(&pattern.elements.nodes)
                    } else {
                        2
                    }
                }
                k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                    if let Some(lit) = self.arena.get_literal_expr(left_node) {
                        self.count_object_destructuring_elements(&lit.elements.nodes)
                    } else {
                        2
                    }
                }
                k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => 2,
                _ => 2, // fallback: assume needs temp
            }
        };

        // For complex sources (function calls, array literals), we only need a temp
        // if the pattern requires multiple accesses. Single-access patterns can
        // inline the source expression directly.
        let needs_temp = !is_simple && element_count > 1;

        let source_name = if is_simple {
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, effective_right_idx)
        } else if needs_temp {
            let temp = self.make_unique_name_hoisted_assignment();
            self.write(&temp);
            self.write(" = ");
            self.emit(right_idx);
            temp
        } else {
            // Single access: use empty string as source_name marker,
            // and we'll inline the right_idx expression at the access point
            String::new()
        };

        let use_inline_source = !is_simple && !needs_temp;
        let mut first = !needs_temp;

        match left_node.kind {
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(left_node) {
                    self.emit_assignment_array_destructuring(
                        &lit.elements.nodes,
                        &source_name,
                        &mut first,
                        use_inline_source.then_some(right_idx),
                    );
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION =>
            {
                if let Some(elements) = self.get_binding_or_literal_elements(left_node) {
                    match left_node.kind {
                        k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                            self.emit_assignment_array_destructuring(
                                &elements,
                                &source_name,
                                &mut first,
                                use_inline_source.then_some(right_idx),
                            );
                        }
                        k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION =>
                        {
                            self.emit_assignment_object_destructuring(
                                &elements,
                                &source_name,
                                &mut first,
                                use_inline_source.then_some(right_idx),
                            );
                        }
                        _ => {}
                    }
                } else {
                    self.emit_node_default(left_node, right_idx);
                }
            }
            _ => {
                // Fallback: emit as-is
                self.emit_node_default(left_node, right_idx);
            }
        }
    }

    /// Walk the LHS of a destructuring assignment and return true if any
    /// assignment target is the identifier `name`. Used to detect cases like
    /// `({ foo, bar } = foo)` where reading `foo.bar` after assigning to
    /// `foo` would observe the clobbered value.
    fn assignment_lhs_reassigns_identifier(&self, lhs: &Node, name: &str) -> bool {
        // Object literal: `{ foo, bar }` or `{ x: foo }`
        if lhs.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let Some(lit) = self.arena.get_literal_expr(lhs) else {
                return false;
            };
            return lit.elements.nodes.iter().any(|&elem_idx| {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    return false;
                };
                match elem_node.kind {
                    k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                        .arena
                        .get_shorthand_property(elem_node)
                        .is_some_and(|sp| {
                            crate::transforms::emit_utils::identifier_text_or_empty(
                                self.arena, sp.name,
                            ) == name
                        }),
                    k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                        .arena
                        .get_property_assignment(elem_node)
                        .is_some_and(|prop| {
                            self.arena.get(prop.initializer).is_some_and(|init| {
                                self.assignment_lhs_reassigns_identifier(init, name)
                            })
                        }),
                    k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                        self.arena.get_spread(elem_node).is_some_and(|sp| {
                            crate::transforms::emit_utils::identifier_text_or_empty(
                                self.arena,
                                sp.expression,
                            ) == name
                        })
                    }
                    _ => false,
                }
            });
        }
        // Array literal: `[a, b]` or `[a = init, ...rest]`
        if lhs.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let Some(lit) = self.arena.get_literal_expr(lhs) else {
                return false;
            };
            return lit.elements.nodes.iter().any(|&elem_idx| {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    return false;
                };
                if elem_node.is_identifier() {
                    return crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena, elem_idx,
                    ) == name;
                }
                self.assignment_lhs_reassigns_identifier(elem_node, name)
            });
        }
        // Bare identifier target (rare in destructuring pattern position
        // but exhaustively covered).
        if lhs.is_identifier() {
            return false; // handled by parent walks
        }
        false
    }

    pub(in crate::emitter) fn assignment_pattern_has_object_rest(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
        {
            return self.assignment_pattern_has_object_rest(binary.left);
        }

        if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let Some(lit) = self.arena.get_literal_expr(node) else {
                return false;
            };
            return lit.elements.nodes.iter().any(|&elem_idx| {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    return false;
                };
                match elem_node.kind {
                    k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => true,
                    k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                        .arena
                        .get_property_assignment(elem_node)
                        .is_some_and(|prop| {
                            self.assignment_pattern_has_object_rest(prop.initializer)
                        }),
                    _ => false,
                }
            });
        }

        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let Some(lit) = self.arena.get_literal_expr(node) else {
                return false;
            };
            return lit.elements.nodes.iter().any(|&elem_idx| {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    return false;
                };
                if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                    && let Some(spread) = self.arena.get_spread(elem_node)
                {
                    return self.assignment_pattern_has_object_rest(spread.expression);
                }
                self.assignment_pattern_has_object_rest(elem_idx)
            });
        }

        false
    }

    fn assignment_pattern_has_dynamic_computed_property_name(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
        {
            return self.assignment_pattern_has_dynamic_computed_property_name(binary.left);
        }

        if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let Some(lit) = self.arena.get_literal_expr(node) else {
                return false;
            };
            return lit.elements.nodes.iter().any(|&elem_idx| {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    return false;
                };
                match elem_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                        .arena
                        .get_property_assignment(elem_node)
                        .is_some_and(|prop| {
                            self.assignment_property_name_is_dynamic_computed(prop.name)
                                || self.assignment_pattern_has_dynamic_computed_property_name(
                                    prop.initializer,
                                )
                        }),
                    k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                        self.arena.get_spread(elem_node).is_some_and(|spread| {
                            self.assignment_pattern_has_dynamic_computed_property_name(
                                spread.expression,
                            )
                        })
                    }
                    _ => false,
                }
            });
        }

        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let Some(lit) = self.arena.get_literal_expr(node) else {
                return false;
            };
            return lit.elements.nodes.iter().any(|&elem_idx| {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    return false;
                };
                if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                    && let Some(spread) = self.arena.get_spread(elem_node)
                {
                    return self
                        .assignment_pattern_has_dynamic_computed_property_name(spread.expression);
                }
                self.assignment_pattern_has_dynamic_computed_property_name(elem_idx)
            });
        }

        false
    }

    fn assignment_object_literal_is_rest_only(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        self.arena.get_literal_expr(node).is_some_and(|lit| {
            lit.elements.nodes.len() == 1
                && lit
                    .elements
                    .nodes
                    .first()
                    .copied()
                    .and_then(|idx| self.arena.get(idx))
                    .is_some_and(|elem| elem.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
        })
    }

    fn assignment_object_rest_default_pattern(
        &self,
        idx: NodeIndex,
    ) -> Option<(NodeIndex, NodeIndex)> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16
            || !self.assignment_pattern_has_object_rest(binary.left)
        {
            return None;
        }
        Some((binary.left, binary.right))
    }

    fn assignment_default_nested_pattern(&self, idx: NodeIndex) -> Option<(NodeIndex, NodeIndex)> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }
        let left_node = self.arena.get(binary.left)?;
        matches!(
            left_node.kind,
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || k == syntax_kind_ext::OBJECT_BINDING_PATTERN
        )
        .then_some((binary.left, binary.right))
    }

    /// Lower object-rest assignment for targets that do not support ES2018.
    /// `({ a, ...rest } = source)` -> `{ a } = source, rest = __rest(source, ["a"])`.
    pub(in crate::emitter) fn emit_assignment_object_rest_destructuring(
        &mut self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
    ) {
        let Some(left_node) = self.arena.get(left_idx) else {
            return;
        };

        if !self.assignment_pattern_has_object_rest(left_idx) {
            self.emit_node_default(left_node, right_idx);
            return;
        };

        if self.assignment_pattern_has_dynamic_computed_property_name(left_idx) {
            self.emit_assignment_destructuring_es5(left_node, right_idx);
            return;
        }

        let effective_right_idx = self.unwrap_empty_destructuring_chain(right_idx);
        let is_simple = self
            .arena
            .get(effective_right_idx)
            .is_some_and(|n| n.is_identifier());
        if !is_simple && self.assignment_object_literal_is_rest_only(left_idx) {
            self.emit_assignment_rest_only_object(left_idx, right_idx);
            return;
        }

        let mut first = true;
        let source_name = if is_simple {
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, effective_right_idx)
        } else {
            let temp = self.make_unique_name_hoisted_assignment();
            self.emit_assignment_separator(&mut first);
            self.write(&temp);
            self.write(" = ");
            // The RHS sits inside a comma expression `(_a = RHS, ...)` — never at
            // statement-leading position, so a paren wrapping a type-erased object
            // literal (e.g. `({ } as any)` -> after erasure -> `({})`) is redundant.
            // tsc emits `_a = {}` here, not `_a = ({})`. Peel through paren+type-
            // erasure if the unwrapped expression is an object literal so the strip
            // matches tsc's own placement decision.
            let emit_idx = self.peel_assign_rhs_object_literal_paren(right_idx);
            self.emit(emit_idx);
            temp
        };

        self.emit_assignment_pattern_with_object_rest(left_idx, &source_name, true, &mut first);
    }

    /// Peel a single layer of `(<TypeErasure>{...})` paren wrapping when the
    /// expression is the RHS of an assignment in a comma expression — that
    /// position never has the leading-`{` block-vs-object ambiguity, so the
    /// outer paren is redundant. Returns the inner expression if the shape
    /// matches; otherwise returns the original index unchanged.
    fn peel_assign_rhs_object_literal_paren(&self, idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.arena.get(idx) else {
            return idx;
        };
        if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return idx;
        }
        let Some(paren) = self.arena.get_parenthesized(node) else {
            return idx;
        };
        let Some(inner) = self.arena.get(paren.expression) else {
            return idx;
        };
        let is_type_erasure = inner.kind == syntax_kind_ext::TYPE_ASSERTION
            || inner.kind == syntax_kind_ext::AS_EXPRESSION
            || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
            || inner.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS;
        if !is_type_erasure {
            return idx;
        }
        match self.unwrap_type_assertion_kind(paren.expression) {
            Some(k) if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => paren.expression,
            _ => idx,
        }
    }

    pub(in crate::emitter) fn emit_assignment_object_rest_destructuring_from_source(
        &mut self,
        left_idx: NodeIndex,
        source: &str,
    ) {
        let mut first = true;
        self.emit_assignment_pattern_with_object_rest(left_idx, source, true, &mut first);
    }

    fn emit_assignment_rest_only_object(&mut self, left_idx: NodeIndex, right_idx: NodeIndex) {
        let Some(node) = self.arena.get(left_idx) else {
            return;
        };
        let Some(elements) = self.get_binding_or_literal_elements(node) else {
            return;
        };
        let Some(spread_idx) = elements.first().copied() else {
            return;
        };
        let Some(spread_node) = self.arena.get(spread_idx) else {
            return;
        };
        let Some(spread) = self.arena.get_spread(spread_node) else {
            return;
        };

        self.emit(spread.expression);
        self.write(" = ");
        self.write_helper("__rest");
        self.write("(");
        self.emit(right_idx);
        self.write(", [])");
    }

    fn emit_assignment_pattern_with_object_rest(
        &mut self,
        pattern_idx: NodeIndex,
        source: &str,
        source_simple: bool,
        first: &mut bool,
    ) {
        let Some(node) = self.arena.get(pattern_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.emit_assignment_object_rest_pattern(pattern_idx, source, source_simple, first);
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.emit_assignment_array_pattern_with_object_rest(pattern_idx, source, first);
            }
            _ => {}
        }
    }

    fn emit_assignment_object_rest_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        source: &str,
        source_simple: bool,
        first: &mut bool,
    ) {
        let Some(node) = self.arena.get(pattern_idx) else {
            return;
        };
        let Some(lit) = self.arena.get_literal_expr(node) else {
            return;
        };
        let elements = lit.elements.nodes.clone();
        let has_own_rest = elements.iter().any(|&elem_idx| {
            self.arena
                .get(elem_idx)
                .is_some_and(|elem| elem.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
        });

        let source_name;
        let source = if has_own_rest && !source_simple {
            source_name = self.make_unique_name_hoisted_assignment();
            self.emit_assignment_separator(first);
            self.write(&source_name);
            self.write(" = ");
            self.write(source);
            source_name.as_str()
        } else {
            source
        };

        let mut simple_elements = Vec::new();
        let mut excluded_props = Vec::new();
        let mut rest_element = None;

        for (index, &elem_idx) in elements.iter().enumerate() {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };

            match elem_node.kind {
                k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                    let has_later_element =
                        elements.iter().skip(index + 1).any(|idx| !idx.is_none());
                    if has_later_element {
                        continue;
                    }
                    rest_element = Some(elem_idx);
                }
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.arena.get_property_assignment(elem_node) else {
                        continue;
                    };
                    let key = self.get_property_key_text(prop.name).unwrap_or_default();
                    if !key.is_empty() {
                        excluded_props.push(key.clone());
                    }

                    if self.assignment_pattern_has_object_rest(prop.initializer) {
                        self.emit_assignment_object_pattern_without_rest(
                            &simple_elements,
                            source,
                            first,
                        );
                        simple_elements.clear();

                        if let Some((nested_pattern, default_expr)) =
                            self.assignment_object_rest_default_pattern(prop.initializer)
                        {
                            let extract_temp = self.make_unique_name_hoisted_assignment();
                            let default_temp = self.make_unique_name_hoisted_assignment();
                            self.emit_assignment_separator(first);
                            self.write(&extract_temp);
                            self.write(" = ");
                            self.emit_object_key_access(source, &key);
                            self.write(", ");
                            self.write(&default_temp);
                            self.write(" = ");
                            self.write(&extract_temp);
                            self.write(" === void 0 ? ");
                            self.emit(default_expr);
                            self.write(" : ");
                            self.write(&extract_temp);
                            self.emit_assignment_pattern_with_object_rest(
                                nested_pattern,
                                &default_temp,
                                true,
                                first,
                            );
                        } else {
                            let nested_source = self.object_key_access_text(source, &key);
                            let nested_simple = self.arena.get(prop.initializer).is_some_and(|n| {
                                n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            });
                            self.emit_assignment_pattern_with_object_rest(
                                prop.initializer,
                                &nested_source,
                                nested_simple,
                                first,
                            );
                        }
                    } else {
                        simple_elements.push(elem_idx);
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    if let Some(shorthand) = self.arena.get_shorthand_property(elem_node) {
                        let key = crate::transforms::emit_utils::identifier_text_or_empty(
                            self.arena,
                            shorthand.name,
                        );
                        if !key.is_empty() {
                            excluded_props.push(key);
                        }
                    }
                    simple_elements.push(elem_idx);
                }
                _ => {}
            }
        }

        self.emit_assignment_object_pattern_without_rest(&simple_elements, source, first);

        if let Some(rest_idx) = rest_element
            && let Some(rest_node) = self.arena.get(rest_idx)
            && let Some(spread) = self.arena.get_spread(rest_node)
        {
            self.emit_assignment_separator(first);
            self.emit(spread.expression);
            self.write(" = ");
            self.write_helper("__rest");
            self.write("(");
            self.write(source);
            self.write(", [");
            for (i, key) in excluded_props.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write("\"");
                self.write(key);
                self.write("\"");
            }
            self.write("])");
        }
    }

    fn emit_assignment_object_pattern_without_rest(
        &mut self,
        elements: &[NodeIndex],
        source: &str,
        first: &mut bool,
    ) {
        if elements.is_empty() {
            return;
        }

        self.emit_assignment_separator(first);
        self.write("{ ");
        for (i, &elem_idx) in elements.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit(elem_idx);
        }
        self.write(" } = ");
        self.write(source);
    }

    fn emit_assignment_array_pattern_with_object_rest(
        &mut self,
        pattern_idx: NodeIndex,
        source: &str,
        first: &mut bool,
    ) {
        let Some(node) = self.arena.get(pattern_idx) else {
            return;
        };
        let Some(lit) = self.arena.get_literal_expr(node) else {
            return;
        };
        let elements = lit.elements.nodes.clone();
        let mut nested_patterns = Vec::new();

        self.emit_assignment_separator(first);
        self.write("[");
        for (i, &elem_idx) in elements.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            if elem_idx.is_none() {
                continue;
            }

            if self.assignment_pattern_has_object_rest(elem_idx) {
                let temp = self.make_unique_name_hoisted_assignment();
                self.write(&temp);
                nested_patterns.push((elem_idx, temp));
            } else {
                self.emit(elem_idx);
            }
        }
        self.write("] = ");
        self.write(source);

        for (nested_idx, temp) in nested_patterns {
            let nested_pattern = self
                .arena
                .get(nested_idx)
                .and_then(|node| {
                    if node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                        self.arena.get_spread(node).map(|spread| spread.expression)
                    } else {
                        Some(nested_idx)
                    }
                })
                .unwrap_or(nested_idx);
            self.emit_assignment_pattern_with_object_rest(nested_pattern, &temp, true, first);
        }
    }

    fn object_key_access_text(&self, source: &str, key: &str) -> String {
        if is_valid_identifier_name(key) {
            format!("{source}.{key}")
        } else {
            format!(
                "{}[\"{}\"]",
                source,
                key.replace('\\', "\\\\").replace('"', "\\\"")
            )
        }
    }

    /// Emit lowered array assignment destructuring.
    /// `[, nameA, [primaryB, secondaryB]] = source` →
    /// `nameA = source[1], _a = source[2], primaryB = _a[0], secondaryB = _a[1]`
    ///
    /// When `inline_source` is Some, the source expression is emitted inline
    /// instead of using the `source` string. Used when only one access is needed.
    pub(in crate::emitter) fn emit_assignment_array_destructuring(
        &mut self,
        elements: &[NodeIndex],
        source: &str,
        first: &mut bool,
        inline_source: Option<NodeIndex>,
    ) {
        for (i, &elem_idx) in elements.iter().enumerate() {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };

            // Check for spread element: [...rest]
            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                if let Some(spread) = self.arena.get_spread(elem_node) {
                    self.emit_assignment_separator(first);
                    let target_node = self.arena.get(spread.expression);
                    if let Some(tn) = target_node {
                        if tn.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            || tn.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            || tn.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            || tn.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        {
                            // Nested destructuring on rest
                            let temp = self.make_unique_name_hoisted_assignment();
                            self.write(&temp);
                            self.write(" = ");
                            if let Some(inline_src) = inline_source {
                                self.emit(inline_src);
                            } else {
                                self.write(source);
                            }
                            self.write(".slice(");
                            self.write_usize(i);
                            self.write(")");
                            self.emit_assignment_nested_destructuring(
                                spread.expression,
                                &temp,
                                first,
                            );
                        } else {
                            self.emit_assignment_target(spread.expression);
                            self.write(" = ");
                            if let Some(inline_src) = inline_source {
                                self.emit(inline_src);
                            } else {
                                self.write(source);
                            }
                            self.write(".slice(");
                            self.write_usize(i);
                            self.write(")");
                        }
                    }
                }
                continue;
            }

            // Check if element has a default value (BinaryExpression with =)
            if elem_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.arena.get_binary_expr(elem_node)
                && bin.operator_token == SyntaxKind::EqualsToken as u16
            {
                // Element with default: target = source[i] === void 0 ? default : source[i]
                let target_node = self.arena.get(bin.left);
                let is_nested = target_node.is_some_and(|n| {
                    n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        || n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        || n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                });

                if is_nested {
                    let extract_temp = self.make_unique_name_hoisted_assignment();
                    let default_temp = self.make_unique_name_hoisted_assignment();
                    self.emit_assignment_separator(first);
                    self.write(&extract_temp);
                    self.write(" = ");
                    if let Some(inline_src) = inline_source {
                        self.emit(inline_src);
                    } else {
                        self.write(source);
                    }
                    self.write("[");
                    self.write_usize(i);
                    self.write("], ");
                    self.write(&default_temp);
                    self.write(" = ");
                    self.write(&extract_temp);
                    self.write(" === void 0 ? ");
                    self.emit(bin.right);
                    self.write(" : ");
                    self.write(&extract_temp);
                    self.emit_assignment_nested_destructuring(bin.left, &default_temp, first);
                } else {
                    let temp = self.make_unique_name_hoisted_assignment();
                    self.emit_assignment_separator(first);
                    self.write(&temp);
                    self.write(" = ");
                    if let Some(inline_src) = inline_source {
                        self.emit(inline_src);
                    } else {
                        self.write(source);
                    }
                    self.write("[");
                    self.write_usize(i);
                    self.write("], ");
                    self.emit_assignment_target(bin.left);
                    self.write(" = ");
                    self.write(&temp);
                    self.write(" === void 0 ? ");
                    self.emit(bin.right);
                    self.write(" : ");
                    self.write(&temp);
                }
                continue;
            }

            // Check for nested array/object destructuring
            if elem_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || elem_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || elem_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || elem_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            {
                let temp = self.make_unique_name_hoisted_assignment();
                self.emit_assignment_separator(first);
                self.write(&temp);
                self.write(" = ");
                if let Some(inline_src) = inline_source {
                    self.emit(inline_src);
                } else {
                    self.write(source);
                }
                self.write("[");
                self.write_usize(i);
                self.write("]");
                self.emit_assignment_nested_destructuring(elem_idx, &temp, first);
                continue;
            }

            // Simple identifier target
            self.emit_assignment_separator(first);
            self.emit_assignment_target(elem_idx);
            self.write(" = ");
            if let Some(inline_src) = inline_source {
                self.emit(inline_src);
            } else {
                self.write(source);
            }
            self.write("[");
            self.write_usize(i);
            self.write("]");
        }
    }

    /// Emit lowered object assignment destructuring.
    /// `{ name: nameA, skill: skillA } = source` →
    /// `nameA = source.name, skillA = source.skill`
    pub(in crate::emitter) fn emit_assignment_object_destructuring(
        &mut self,
        elements: &[NodeIndex],
        source: &str,
        first: &mut bool,
        inline_source: Option<NodeIndex>,
    ) {
        let mut rest_props = Vec::new();

        for &elem_idx in elements {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };

            match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    if let Some(prop) = self.arena.get_property_assignment(elem_node) {
                        let computed_key_temp =
                            self.emit_assignment_computed_key_temp_if_needed(prop.name, first);
                        if let Some(rest_prop) = self
                            .assignment_rest_prop_for_key(prop.name, computed_key_temp.as_deref())
                        {
                            rest_props.push(rest_prop);
                        }

                        // Check if value is a nested pattern
                        let value_node = self.arena.get(prop.initializer);
                        if let Some((nested_pattern, default_expr)) =
                            self.assignment_default_nested_pattern(prop.initializer)
                        {
                            let extract_temp = self.make_unique_name_hoisted_assignment();
                            let default_temp = self.make_unique_name_hoisted_assignment();
                            self.emit_assignment_separator(first);
                            self.write(&extract_temp);
                            self.write(" = ");
                            self.emit_assignment_object_key_access(
                                source,
                                inline_source,
                                prop.name,
                                computed_key_temp.as_deref(),
                            );
                            self.write(", ");
                            self.write(&default_temp);
                            self.write(" = ");
                            self.write(&extract_temp);
                            self.write(" === void 0 ? ");
                            self.emit(default_expr);
                            self.write(" : ");
                            self.write(&extract_temp);
                            self.emit_assignment_nested_destructuring(
                                nested_pattern,
                                &default_temp,
                                first,
                            );
                            continue;
                        }

                        let is_nested = value_node.is_some_and(|n| {
                            n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                || n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                || n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        });

                        if is_nested {
                            let temp = self.make_unique_name_hoisted_assignment();
                            self.emit_assignment_separator(first);
                            self.write(&temp);
                            self.write(" = ");
                            self.emit_assignment_object_key_access(
                                source,
                                inline_source,
                                prop.name,
                                computed_key_temp.as_deref(),
                            );
                            self.emit_assignment_nested_destructuring(
                                prop.initializer,
                                &temp,
                                first,
                            );
                        } else {
                            // Check for default value: { name: nameA = "default" }
                            let value_bin = value_node.and_then(|n| {
                                if n.kind == syntax_kind_ext::BINARY_EXPRESSION {
                                    self.arena.get_binary_expr(n)
                                } else {
                                    None
                                }
                            });
                            if let Some(bin) = value_bin
                                && bin.operator_token == SyntaxKind::EqualsToken as u16
                            {
                                let temp = self.make_unique_name_hoisted_assignment();
                                self.emit_assignment_separator(first);
                                self.write(&temp);
                                self.write(" = ");
                                self.emit_assignment_object_key_access(
                                    source,
                                    inline_source,
                                    prop.name,
                                    computed_key_temp.as_deref(),
                                );
                                self.write(", ");
                                self.emit_assignment_target(bin.left);
                                self.write(" = ");
                                self.write(&temp);
                                self.write(" === void 0 ? ");
                                self.emit(bin.right);
                                self.write(" : ");
                                self.write(&temp);
                                continue;
                            }
                            self.emit_assignment_separator(first);
                            self.emit_assignment_target(prop.initializer);
                            self.write(" = ");
                            self.emit_assignment_object_key_access(
                                source,
                                inline_source,
                                prop.name,
                                computed_key_temp.as_deref(),
                            );
                        }
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    // { name } → name = source.name
                    if let Some(shorthand) = self.arena.get_shorthand_property(elem_node) {
                        let name = crate::transforms::emit_utils::identifier_text_or_empty(
                            self.arena,
                            shorthand.name,
                        );
                        self.emit_assignment_separator(first);
                        if !self.emit_commonjs_live_export_assignment_target_name(&name) {
                            self.write(&name);
                        }
                        self.write(" = ");
                        self.emit_assignment_object_key_access(
                            source,
                            inline_source,
                            shorthand.name,
                            None,
                        );
                        if !name.is_empty() {
                            rest_props.push(AssignmentRestProp::Static(name));
                        }
                    }
                }
                k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                    // { ...rest } → rest = __rest(source, ["prop1", "prop2"])
                    if let Some(spread) = self.arena.get_spread(elem_node) {
                        self.emit_assignment_separator(first);
                        self.emit_assignment_target(spread.expression);
                        self.write(" = ");
                        self.write_helper("__rest");
                        self.write("(");
                        self.emit_assignment_source(source, inline_source);
                        self.write(", ");
                        self.emit_assignment_rest_exclude_list(&rest_props);
                        self.write(")");
                    }
                }
                _ => {}
            }
        }
    }

    fn emit_assignment_computed_key_temp_if_needed(
        &mut self,
        name_idx: NodeIndex,
        first: &mut bool,
    ) -> Option<String> {
        if !self.assignment_property_name_is_dynamic_computed(name_idx) {
            return None;
        }

        let key_temp = self.make_unique_name_hoisted_assignment();
        self.emit_assignment_separator(first);
        self.write(&key_temp);
        self.write(" = ");
        self.emit_assignment_computed_property_expression(name_idx);
        Some(key_temp)
    }

    fn assignment_rest_prop_for_key(
        &self,
        name_idx: NodeIndex,
        computed_key_temp: Option<&str>,
    ) -> Option<AssignmentRestProp> {
        if let Some(temp) = computed_key_temp {
            return Some(AssignmentRestProp::Dynamic(temp.to_string()));
        }

        self.get_property_key_text(name_idx)
            .filter(|key| !key.is_empty())
            .map(AssignmentRestProp::Static)
    }

    fn emit_assignment_object_key_access(
        &mut self,
        source: &str,
        inline_source: Option<NodeIndex>,
        name_idx: NodeIndex,
        computed_key_temp: Option<&str>,
    ) {
        if let Some(temp) = computed_key_temp {
            self.emit_assignment_source(source, inline_source);
            self.write("[");
            self.write(temp);
            self.write("]");
            return;
        }

        let key = self.get_property_key_text(name_idx).unwrap_or_default();
        if let Some(inline_src) = inline_source {
            self.emit(inline_src);
            if is_valid_identifier_name(&key) {
                self.write(".");
                self.write(&key);
            } else {
                self.write("[\"");
                self.write(&key.replace('\\', "\\\\").replace('\"', "\\\""));
                self.write("\"]");
            }
        } else {
            self.emit_object_key_access(source, &key);
        }
    }

    fn emit_assignment_source(&mut self, source: &str, inline_source: Option<NodeIndex>) {
        if let Some(inline_src) = inline_source {
            self.emit(inline_src);
        } else {
            self.write(source);
        }
    }

    fn emit_assignment_rest_exclude_list(&mut self, props: &[AssignmentRestProp]) {
        self.write("[");
        for (i, prop) in props.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_assignment_rest_excluded_prop(prop);
        }
        self.write("]");
    }

    fn emit_assignment_rest_excluded_prop(&mut self, prop: &AssignmentRestProp) {
        match prop {
            AssignmentRestProp::Static(key) => {
                self.write("\"");
                self.write(&key.replace('\\', "\\\\").replace('"', "\\\""));
                self.write("\"");
            }
            AssignmentRestProp::Dynamic(temp) => {
                self.write("typeof ");
                self.write(temp);
                self.write(" === \"symbol\" ? ");
                self.write(temp);
                self.write(" : ");
                self.write(temp);
                self.write(" + \"\"");
            }
        }
    }

    fn assignment_property_name_is_dynamic_computed(&self, name_idx: NodeIndex) -> bool {
        self.arena
            .get(name_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
            && self.get_property_key_text(name_idx).is_none()
    }

    fn emit_assignment_computed_property_expression(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(name_node)
        {
            self.emit(computed.expression);
        }
    }

    /// Helper to emit nested destructuring from a source name.
    pub(in crate::emitter) fn emit_assignment_nested_destructuring(
        &mut self,
        pattern_idx: NodeIndex,
        source: &str,
        first: &mut bool,
    ) {
        let Some(node) = self.arena.get(pattern_idx) else {
            return;
        };
        match node.kind {
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(node) {
                    self.emit_assignment_array_destructuring(
                        &lit.elements.nodes,
                        source,
                        first,
                        None,
                    );
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(node) {
                    self.emit_assignment_object_destructuring(
                        &lit.elements.nodes,
                        source,
                        first,
                        None,
                    );
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    self.emit_assignment_array_destructuring(
                        &pattern.elements.nodes,
                        source,
                        first,
                        None,
                    );
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    self.emit_assignment_object_destructuring(
                        &pattern.elements.nodes,
                        source,
                        first,
                        None,
                    );
                }
            }
            _ => {}
        }
    }

    pub(in crate::emitter) fn emit_object_key_access(&mut self, source: &str, key: &str) {
        if is_valid_identifier_name(key) {
            self.write(source);
            self.write(".");
            self.write(key);
        } else {
            self.write(source);
            self.write("[\"");
            self.write(&key.replace('\\', "\\\\").replace('\"', "\\\""));
            self.write("\"]");
        }
    }

    pub(in crate::emitter) fn get_binding_or_literal_elements(
        &self,
        node: &Node,
    ) -> Option<Vec<NodeIndex>> {
        self.arena
            .get_literal_expr(node)
            .map(|lit| lit.elements.nodes.to_vec())
            .or_else(|| {
                self.arena
                    .get_binding_pattern(node)
                    .map(|pattern| pattern.elements.nodes.to_vec())
            })
    }

    /// Emit separator for assignment destructuring (`, ` between parts).
    pub(in crate::emitter) fn emit_assignment_separator(&mut self, first: &mut bool) {
        if !*first {
            self.write(", ");
        }
        *first = false;
    }

    /// Get property key text from a property name node.
    pub(in crate::emitter) fn get_property_key_text(&self, name_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(name_idx)?;
        if node.is_identifier() {
            Some(crate::transforms::emit_utils::identifier_text_or_empty(
                self.arena, name_idx,
            ))
        } else if node.is_string_literal() {
            // For string keys like { "name": value }
            self.get_string_literal_text(name_idx)
        } else if node.is_numeric_literal() {
            self.get_numeric_literal_text(name_idx)
        } else if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(node)?;
            let expr_node = self.arena.get(computed.expression)?;
            self.arena
                .get_literal(expr_node)
                .map(|literal| literal.text.clone())
        } else {
            None
        }
    }

    pub(in crate::emitter) fn get_string_literal_text(&self, idx: NodeIndex) -> Option<String> {
        let source = self.source_text?;
        let node = self.arena.get(idx)?;
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let end = node.end as usize;
        let text = &source[start..end];
        // Strip quotes
        if text.len() >= 2 && (text.starts_with('"') || text.starts_with('\'')) {
            Some(text[1..text.len() - 1].to_string())
        } else {
            Some(text.to_string())
        }
    }

    pub(in crate::emitter) fn get_numeric_literal_text(&self, idx: NodeIndex) -> Option<String> {
        let source = self.source_text?;
        let node = self.arena.get(idx)?;
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let end = node.end as usize;
        Some(source[start..end].to_string())
    }
}
