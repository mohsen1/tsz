use super::super::Printer;
use crate::transforms::emit_utils;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{MethodDeclData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Segment of an object literal for ES5 spread transformation
enum ObjectSegment<'a> {
    /// Non-spread elements: regular and computed properties
    Elements(&'a [NodeIndex]),
    /// Spread element: ...obj
    Spread(NodeIndex),
}

impl<'a> Printer<'a> {
    fn emit_spread_expr_from_idx(&mut self, spread_idx: NodeIndex) {
        if let Some(spread_node) = self.arena.get(spread_idx) {
            self.emit_spread_expression(spread_node);
        }
    }

    /// Returns true when the spread element at `spread_idx` wraps a simple object literal.
    ///
    /// tsc uses the object literal directly as the `__assign` target (instead of
    /// allocating a fresh `{}`) when the spread expression is an inline object
    /// literal without nested spreads. If the inner literal has spreads, it
    /// lowers to its own `__assign` chain and the outer spread must copy it
    /// through `{}` instead of mutating that intermediate result.
    fn spread_idx_is_simple_object_literal(&self, spread_idx: NodeIndex) -> bool {
        let Some(spread_node) = self.arena.get(spread_idx) else {
            return false;
        };
        let Some(spread) = self.arena.get_spread(spread_node) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(spread.expression) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(literal) = self.arena.get_literal_expr(expr_node) else {
            return false;
        };
        !literal.elements.nodes.iter().any(|&idx| {
            self.arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
        })
    }

    pub(in crate::emitter) fn emit_spread_expression_with_read(
        &mut self,
        node: &Node,
        wrap_with_read: bool,
    ) {
        if let Some(spread) = self.arena.get_spread(node) {
            if wrap_with_read {
                self.write_helper("__read");
                self.write("(");
                self.emit(spread.expression);
                self.write(")");
            } else {
                self.emit(spread.expression);
            }
        }
    }

    pub(in crate::emitter) fn emit_object_literal_entries_es5(&mut self, elements: &[NodeIndex]) {
        self.emit_object_literal_entries_es5_with_comments(elements, false, None, false);
    }

    fn emit_object_literal_assign_entries_es5(
        &mut self,
        elements: &[NodeIndex],
        source_range: Option<(u32, u32)>,
    ) {
        if self.object_literal_assign_segment_can_emit_compact(elements, source_range) {
            self.emit_object_literal_entries_es5_compact(elements);
        } else {
            self.emit_object_literal_entries_es5_with_comments(
                elements,
                false,
                source_range,
                false,
            );
        }
    }

    fn emit_object_literal_prefix_entries_es5(&mut self, elements: &[NodeIndex]) {
        self.emit_object_literal_entries_es5_with_comments(elements, false, None, true);
    }

    fn emit_object_literal_entries_es5_compact(&mut self, elements: &[NodeIndex]) {
        if elements.is_empty() {
            self.write("{}");
            return;
        }

        self.write("{ ");
        for (i, &prop) in elements.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_object_literal_member_es5(prop);
        }
        self.write(" }");
    }

    fn object_literal_assign_segment_can_emit_compact(
        &self,
        elements: &[NodeIndex],
        source_range: Option<(u32, u32)>,
    ) -> bool {
        if elements.is_empty() {
            return true;
        }

        if !elements.iter().all(|&idx| {
            self.arena.get(idx).is_some_and(|node| {
                node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    || node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            })
        }) {
            return false;
        }

        let Some(source) = self.source_text else {
            return true;
        };

        if self.object_literal_source_range_blocks_compact(source_range) {
            return false;
        }

        for pair in elements.windows(2) {
            let Some(curr) = self.arena.get(pair[0]) else {
                continue;
            };
            let Some(next) = self.arena.get(pair[1]) else {
                continue;
            };
            let curr_end = std::cmp::min(curr.end as usize, source.len());
            let next_pos = std::cmp::min(next.pos as usize, source.len());
            if curr_end >= next_pos {
                continue;
            }
            let gap = &source[curr_end..next_pos];
            if gap.contains('\n') || gap.contains("//") || gap.contains("/*") {
                return false;
            }
        }

        true
    }

    fn emit_object_literal_entries_es5_with_comments(
        &mut self,
        elements: &[NodeIndex],
        has_trailing_comma: bool,
        source_range: Option<(u32, u32)>,
        suppress_source_trailing_comma: bool,
    ) {
        if elements.is_empty() {
            self.write("{}");
            return;
        }

        let open_brace_end = source_range.and_then(|(start_pos, end_pos)| {
            self.source_text.and_then(|text| {
                let bytes = text.as_bytes();
                let start = start_pos as usize;
                let end = std::cmp::min(end_pos as usize, bytes.len());
                if start >= end {
                    return None;
                }
                bytes[start..end]
                    .iter()
                    .position(|&b| b == b'{')
                    .map(|off| (start + off + 1) as u32)
            })
        });
        let source_end = source_range.map(|(_, end)| end);

        if elements.len() > 1 {
            self.write("{");
            self.write_line();
            self.increase_indent();
            for (i, &prop) in elements.iter().enumerate() {
                let Some(prop_node) = self.arena.get(prop) else {
                    continue;
                };
                if i == 0
                    && let Some(open_brace_end) = open_brace_end
                {
                    let wrote_leading_newline =
                        self.emit_unemitted_comments_between(open_brace_end, prop_node.pos);
                    if !wrote_leading_newline
                        && self.object_literal_comment_range_ends_with_newline(
                            open_brace_end,
                            prop_node.pos,
                        )
                    {
                        self.write_line();
                    }
                }

                self.emit_object_literal_member_es5(prop);

                let token_end = self.object_literal_member_comment_start_es5(prop, prop_node);
                let source_bound = source_end.unwrap_or(prop_node.end);
                let next_pos = if i < elements.len() - 1 {
                    elements
                        .get(i + 1)
                        .and_then(|&next_prop| self.arena.get(next_prop))
                        .map_or(prop_node.end, |n| n.pos)
                } else {
                    source_bound
                };
                let comma_already_past = self.comma_immediately_before_pos(token_end);
                let comma_pos = if comma_already_past {
                    None
                } else {
                    self.find_comma_pos_after(token_end, next_pos)
                };
                let is_last = i == elements.len() - 1;
                let source_comma = comma_already_past || comma_pos.is_some();
                let needs_comma = if self.source_text.is_some() {
                    if is_last && suppress_source_trailing_comma {
                        has_trailing_comma
                    } else {
                        has_trailing_comma || source_comma
                    }
                } else {
                    i < elements.len() - 1 || has_trailing_comma
                };
                if needs_comma {
                    if let Some(comma_pos) = comma_pos {
                        self.emit_trailing_comments_before(token_end, comma_pos);
                    }
                    self.write(",");
                }

                if i < elements.len() - 1 {
                    let next_prop = elements[i + 1];
                    let has_same_line_comment = self.source_text.is_some_and(|text| {
                        let from = token_end as usize;
                        let to = std::cmp::min(next_pos as usize, text.len());
                        if from >= to {
                            return false;
                        }
                        let gap = &text[from..to];
                        if let Some(slash_pos) = gap.find("//") {
                            !gap[..slash_pos].contains('\n')
                        } else if let Some(block_pos) = gap.find("/*") {
                            !gap[..block_pos].contains('\n')
                        } else {
                            false
                        }
                    });
                    let same_line = self.are_on_same_line_in_source(prop, next_prop);
                    if has_same_line_comment {
                        self.write(" ");
                    } else if !same_line {
                        self.write_line();
                    }
                    let wrote_newline = self.emit_unemitted_comments_between(token_end, next_pos);
                    if !wrote_newline {
                        if same_line {
                            self.write(" ");
                        } else if has_same_line_comment {
                            self.write_line();
                        }
                    }
                } else {
                    self.emit_trailing_comments(token_end);
                    let wrote_newline =
                        self.emit_unemitted_comments_between(token_end, source_bound);
                    if !wrote_newline {
                        self.write_line();
                    }
                }
            }
            self.decrease_indent();
            self.write("}");
        } else {
            // For a single-element object literal, check if the source was multi-line.
            // tsc preserves the source formatting: multi-line source → multi-line output.
            // A single method with a multi-line body should be emitted multi-line.
            let use_multiline = self.es5_single_element_needs_multiline(elements[0])
                || self.object_literal_source_range_blocks_compact(source_range);
            if use_multiline {
                self.write("{");
                self.write_line();
                self.increase_indent();
                let prop = elements[0];
                if let Some(prop_node) = self.arena.get(prop) {
                    if let Some(open_brace_end) = open_brace_end {
                        let wrote_leading_newline =
                            self.emit_unemitted_comments_between(open_brace_end, prop_node.pos);
                        if !wrote_leading_newline
                            && self.object_literal_comment_range_ends_with_newline(
                                open_brace_end,
                                prop_node.pos,
                            )
                        {
                            self.write_line();
                        }
                    }
                    self.emit_object_literal_member_es5(prop);
                    let token_end = self.object_literal_member_comment_start_es5(prop, prop_node);
                    let source_bound = source_end.unwrap_or(prop_node.end);
                    if has_trailing_comma {
                        if let Some(comma_pos) = self.find_comma_pos_after(token_end, source_bound)
                        {
                            self.emit_trailing_comments_before(token_end, comma_pos);
                        }
                        self.write(",");
                    }
                    self.emit_trailing_comments(token_end);
                    let wrote_newline =
                        self.emit_unemitted_comments_between(token_end, source_bound);
                    if !wrote_newline {
                        self.write_line();
                    }
                } else if has_trailing_comma {
                    self.write(",");
                    self.write_line();
                }
                self.decrease_indent();
                self.write("}");
            } else {
                self.write("{ ");
                self.emit_object_literal_member_es5(elements[0]);
                if has_trailing_comma {
                    self.write(",");
                }
                self.write(" }");
            }
        }
    }

    fn object_literal_source_range_blocks_compact(&self, source_range: Option<(u32, u32)>) -> bool {
        let Some((start, end)) = source_range else {
            return false;
        };
        let Some(source) = self.source_text else {
            return false;
        };
        let start = std::cmp::min(start as usize, source.len());
        let end = std::cmp::min(end as usize, source.len());
        if start >= end {
            return false;
        }
        let literal_source = &source[start..end];
        literal_source.contains('\n')
            || self
                .all_comments
                .iter()
                .any(|comment| comment.pos as usize >= start && comment.end as usize <= end)
    }

    fn object_literal_member_comment_start_es5(&self, prop_idx: NodeIndex, node: &Node) -> u32 {
        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                self.arena.get_property_assignment(node).map_or_else(
                    || self.find_token_end_before_trivia(node.pos, node.end),
                    |prop| {
                        self.arena.get(prop.initializer).map_or_else(
                            || self.find_token_end_before_trivia(node.pos, node.end),
                            |init_node| {
                                self.find_token_end_before_trivia(init_node.pos, init_node.end)
                            },
                        )
                    },
                )
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                .arena
                .get_shorthand_property(node)
                .and_then(|shorthand| self.arena.get(shorthand.name))
                .map_or_else(
                    || self.find_token_end_before_trivia(node.pos, node.end),
                    |name_node| self.find_token_end_before_trivia(name_node.pos, name_node.end),
                ),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .arena
                .get_method_decl(node)
                .and_then(|method| self.arena.get(method.body))
                .map_or_else(
                    || self.find_token_end_before_trivia(node.pos, node.end),
                    |body_node| self.find_token_end_before_trivia(body_node.pos, body_node.end),
                ),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .arena
                .get_accessor(node)
                .and_then(|accessor| self.arena.get(accessor.body))
                .map_or_else(
                    || self.find_token_end_before_trivia(node.pos, node.end),
                    |body_node| self.find_token_end_before_trivia(body_node.pos, body_node.end),
                ),
            _ => self.arena.get(prop_idx).map_or(node.end, |member| {
                self.find_token_end_before_trivia(member.pos, member.end)
            }),
        }
    }

    pub(in crate::emitter) fn emit_object_literal_member_es5(&mut self, prop_idx: NodeIndex) {
        let Some(node) = self.arena.get(prop_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(shorthand) = self.arena.get_shorthand_property(node) {
                    self.emit_property_key_name(shorthand.name);
                    self.write(": ");
                    self.emit(shorthand.name);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    self.emit(method.name);
                    self.write(": ");
                    self.emit_object_literal_method_value_es5(node, method);
                }
            }
            _ => self.emit(prop_idx),
        }
    }

    /// Check if a single-element ES5 object literal should use multi-line formatting.
    /// tsc preserves the source layout: if the original method declaration spans multiple
    /// lines, the lowered `name: function(…) { … }` form must also be multi-line.
    fn es5_single_element_needs_multiline(&self, elem_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(elem_idx) else {
            return false;
        };
        // Only methods need this check — regular properties keep their original
        // formatting through the default emit path.
        if node.kind != syntax_kind_ext::METHOD_DECLARATION {
            return false;
        }
        // Check if the element node itself spans multiple lines in the source.
        self.source_text.is_some_and(|text| {
            let start = std::cmp::min(node.pos as usize, text.len());
            let end = std::cmp::min(node.end as usize, text.len());
            start < end && text[start..end].contains('\n')
        })
    }

    pub(in crate::emitter) fn emit_object_literal_method_value_es5(
        &mut self,
        node: &Node,
        method: &MethodDeclData,
    ) {
        if method.body.is_none() {
            self.write("function () { }");
            return;
        }

        let is_async = self
            .arena
            .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword);
        if is_async {
            if method.asterisk_token
                || crate::transforms::emit_utils::source_header_has_async_generator_asterisk(
                    self.source_text,
                    node.pos,
                    self.arena
                        .get(method.body)
                        .map_or(node.end, |body| body.pos),
                )
            {
                let property_name = crate::transforms::emit_utils::identifier_text_or_empty(
                    self.arena,
                    method.name,
                );
                self.emit_async_generator_es5_object_method_value(
                    &property_name,
                    &method.parameters.nodes,
                    method.body,
                );
                return;
            }

            self.emit_async_function_es5_body(
                "",
                &method.parameters.nodes,
                method.body,
                "this",
                method.type_annotation,
            );
            return;
        }

        self.write("function");
        if method.asterisk_token {
            self.write("*");
        }
        self.write(" (");
        let param_transforms = self.emit_function_parameters_es5(&method.parameters.nodes);
        self.write(") ");
        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        self.prepare_logical_assignment_value_temps(method.body);
        let previous_new_target_capture = self
            .function_like_contains_new_target(method.body, &method.parameters.nodes)
            .then(|| self.push_new_target_capture_for_initializer("void 0".into()));
        if param_transforms.has_transforms() {
            self.emit_block_with_param_prologue(method.body, &param_transforms);
        } else {
            self.emit(method.body);
        }
        if let Some(previous) = previous_new_target_capture {
            self.restore_new_target_capture(previous);
        }
        self.emitting_function_body_block = prev_emitting_function_body_block;
        self.pop_temp_scope();
    }

    /// Emit ES5-compatible object literal with computed properties and spread
    /// Uses TypeScript's __assign helper for exact tsc matching.
    ///
    /// Spread patterns (variable spread `v`, object-literal spread `{x}`):
    /// - { ...v }       → __assign({}, v)       (fresh target, variable spread)
    /// - { ...{x} }     → __assign({x})          (literal spread: use it as target directly)
    /// - { a: 1, ...b } → __assign({ a: 1 }, b)
    /// - { ...a, b: 1 } → __assign(__assign({}, a), { b: 1 })   (variable spread first)
    /// - { ...{x}, b: 1 } → __assign({x}, { b: 1 })             (literal spread first)
    /// - { a: 1, ...b, c: 2 } → __assign(__assign({ a: 1 }, b), { c: 2 })
    ///
    /// Computed properties (without spread):
    /// - { [k]: v } → (_a = {}, _a[k] = v, _a)
    /// - { a: 1, [k]: v } → (_a = { a: 1 }, _a[k] = v, _a)
    ///
    /// Mixed computed and spread:
    /// - { [k]: v, ...a } → __assign((_a = {}, _a[k] = v, _a), a)
    pub(in crate::emitter) fn emit_object_literal_es5(
        &mut self,
        elements: &[NodeIndex],
        source_range: Option<(u32, u32)>,
        has_trailing_comma: bool,
    ) {
        if elements.is_empty() {
            self.write("{}");
            return;
        }

        // Check if we have any spread elements
        let has_spread = elements
            .iter()
            .any(|&idx| emit_utils::is_spread_element(self.arena, idx));

        if !has_spread {
            // No spread - use the old computed property logic
            self.emit_object_literal_without_spread_es5(elements, source_range, has_trailing_comma);
            return;
        }

        // Has spread - use __assign pattern
        self.emit_object_literal_with_spread_es5(elements, source_range);
    }

    /// Emit object literal without spread (computed properties only)
    fn emit_object_literal_without_spread_es5(
        &mut self,
        elements: &[NodeIndex],
        source_range: Option<(u32, u32)>,
        has_trailing_comma: bool,
    ) {
        self.emit_object_literal_without_spread_es5_with_layout(
            elements,
            source_range,
            has_trailing_comma,
            true,
            true,
        );
    }

    pub(in crate::emitter) fn try_emit_object_literal_es5_return_expression(
        &mut self,
        expression: NodeIndex,
    ) -> bool {
        if !self.ctx.target_es5 {
            return false;
        }

        let Some(node) = self.arena.get(expression) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        let Some(literal) = self.arena.get_literal_expr(node) else {
            return false;
        };
        if literal
            .elements
            .nodes
            .iter()
            .any(|&idx| emit_utils::is_spread_element(self.arena, idx))
        {
            return false;
        }
        if !literal
            .elements
            .nodes
            .iter()
            .any(|&idx| emit_utils::is_computed_property_member(self.arena, idx))
        {
            return false;
        }

        self.emit_object_literal_without_spread_es5_with_layout(
            &literal.elements.nodes,
            Some((node.pos, node.end)),
            self.has_trailing_comma_in_source(node, &literal.elements.nodes),
            false,
            false,
        );
        true
    }

    pub(in crate::emitter) fn try_emit_object_literal_es5_inline_computed_expression(
        &mut self,
        expression: NodeIndex,
    ) -> bool {
        if !self.ctx.target_es5 {
            return false;
        }

        let Some(node) = self.arena.get(expression) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        let Some(literal) = self.arena.get_literal_expr(node) else {
            return false;
        };
        if literal
            .elements
            .nodes
            .iter()
            .any(|&idx| emit_utils::is_spread_element(self.arena, idx))
        {
            return false;
        }
        if !literal
            .elements
            .nodes
            .iter()
            .any(|&idx| emit_utils::is_computed_property_member(self.arena, idx))
        {
            return false;
        }

        self.emit_object_literal_without_spread_es5_with_layout(
            &literal.elements.nodes,
            Some((node.pos, node.end)),
            self.has_trailing_comma_in_source(node, &literal.elements.nodes),
            true,
            false,
        );
        true
    }

    fn emit_object_literal_without_spread_es5_with_layout(
        &mut self,
        elements: &[NodeIndex],
        source_range: Option<(u32, u32)>,
        has_trailing_comma: bool,
        wrap_in_parens: bool,
        use_multiline: bool,
    ) {
        let first_computed_idx = elements
            .iter()
            .position(|&idx| emit_utils::is_computed_property_member(self.arena, idx))
            .unwrap_or(elements.len());

        if first_computed_idx == elements.len() {
            self.emit_object_literal_entries_es5_with_comments(
                elements,
                has_trailing_comma,
                source_range,
                false,
            );
            return;
        }

        // Get hoisted temp variable name
        let temp_var = self.make_unique_name_hoisted();

        // Assignment-like expression contexts use the parenthesized multi-line
        // lowering. A return statement can own the comma expression directly,
        // and `tsc` keeps that form on one line.
        let _ = source_range;

        if wrap_in_parens {
            self.write("(");
        }
        if use_multiline {
            self.increase_indent();
        }
        self.write(&temp_var);
        self.write(" = ");

        // Emit initial non-computed properties as the object literal
        if first_computed_idx > 0 {
            self.emit_object_literal_prefix_entries_es5(&elements[..first_computed_idx]);
        } else {
            self.write("{}");
        }

        // Emit remaining properties as assignments
        for prop_idx in elements.iter().skip(first_computed_idx) {
            self.write(",");
            if use_multiline {
                self.write_line();
            } else {
                self.write(" ");
            }
            self.emit_property_assignment_es5(*prop_idx, &temp_var);
        }

        // Return the temp variable
        self.write(",");
        if use_multiline {
            self.write_line();
        } else {
            self.write(" ");
        }
        self.write(&temp_var);
        if use_multiline {
            self.decrease_indent();
        }
        if wrap_in_parens {
            self.write(")");
        }
    }

    /// Emit object literal with spread using __assign pattern
    fn emit_object_literal_with_spread_es5(
        &mut self,
        elements: &[NodeIndex],
        source_range: Option<(u32, u32)>,
    ) {
        // Split into segments
        let mut segments: Vec<ObjectSegment> = Vec::new();
        let mut current_start = 0;

        for (i, &elem_idx) in elements.iter().enumerate() {
            if emit_utils::is_spread_element(self.arena, elem_idx) {
                // Add non-spread segment before this spread
                if current_start < i {
                    segments.push(ObjectSegment::Elements(&elements[current_start..i]));
                }
                // Add the spread element
                segments.push(ObjectSegment::Spread(elem_idx));
                current_start = i + 1;
            }
        }

        // Add remaining elements after last spread
        if current_start < elements.len() {
            segments.push(ObjectSegment::Elements(&elements[current_start..]));
        }

        // Emit using __assign for exact tsc matching
        match segments.as_slice() {
            [] => {
                // Should not happen due to empty check above
                self.write("{}");
            }
            [ObjectSegment::Elements(elems)] => {
                // No spreads - emit without __assign
                // But check if we have computed properties
                let has_computed = elems
                    .iter()
                    .any(|&idx| emit_utils::is_computed_property_member(self.arena, idx));
                if has_computed {
                    self.emit_object_literal_without_spread_es5(elems, source_range, false);
                } else {
                    self.emit_object_literal_entries_es5(elems);
                }
            }
            [ObjectSegment::Spread(spread_idx)] => {
                // { ...a } → __assign({}, a)  (variable: fresh empty target)
                // { ...{x} } → __assign({x})  (object literal: use it as target directly)
                self.write_helper("__assign");
                self.write("(");
                if self.spread_idx_is_simple_object_literal(*spread_idx) {
                    self.emit_spread_expr_from_idx(*spread_idx);
                } else {
                    self.write("{}, ");
                    self.emit_spread_expr_from_idx(*spread_idx);
                }
                self.write(")");
            }
            [
                ObjectSegment::Elements(elems),
                ObjectSegment::Spread(spread_idx),
            ] => {
                // Elements then spread: { a: 1, ...b } → __assign({ a: 1 }, b)
                // Issue #3968: when the elements contain a computed property,
                // tsc lowers them with a comma-separated temp-var assignment
                // such as `(_a = {}, _a[k] = 1, _a)` so that ES5 has no
                // computed-property literal. Reuse
                // `emit_object_literal_without_spread_es5` which already
                // implements that lowering and writes its own outer parens.
                let has_computed = elems
                    .iter()
                    .any(|&idx| emit_utils::is_computed_property_member(self.arena, idx));
                self.write_helper("__assign");
                self.write("(");
                if has_computed {
                    self.emit_object_literal_without_spread_es5(elems, source_range, false);
                } else {
                    self.emit_object_literal_assign_entries_es5(elems, source_range);
                }
                self.write(", ");
                self.emit_spread_expr_from_idx(*spread_idx);
                self.write(")");
            }
            [
                ObjectSegment::Spread(spread_idx),
                ObjectSegment::Elements(elems),
            ] => {
                // { ...a, b: 1 }    → __assign(__assign({}, a), { b: 1 })  (variable)
                // { ...{x}, b: 1 }  → __assign({x}, { b: 1 })              (object literal)
                let has_computed = elems
                    .iter()
                    .any(|&idx| emit_utils::is_computed_property_member(self.arena, idx));
                self.write_helper("__assign");
                self.write("(");
                if self.spread_idx_is_simple_object_literal(*spread_idx) {
                    self.emit_spread_expr_from_idx(*spread_idx);
                } else {
                    self.write_helper("__assign");
                    self.write("({}, ");
                    self.emit_spread_expr_from_idx(*spread_idx);
                    self.write(")");
                }
                self.write(", ");
                if has_computed {
                    let temp_var = self.make_unique_name_hoisted();
                    self.write("(");
                    self.write(&temp_var);
                    self.write(" = ");
                    let first_computed = elems
                        .iter()
                        .position(|&idx| emit_utils::is_computed_property_member(self.arena, idx))
                        .unwrap_or(elems.len());
                    if first_computed > 0 {
                        self.emit_object_literal_prefix_entries_es5(&elems[..first_computed]);
                    } else {
                        self.write("{}");
                    }
                    for elem in &elems[first_computed..] {
                        self.write(", ");
                        self.emit_property_assignment_es5(*elem, &temp_var);
                    }
                    self.write(", ");
                    self.write(&temp_var);
                    self.write(")");
                } else {
                    self.emit_object_literal_assign_entries_es5(elems, source_range);
                }
                self.write(")");
            }
            [first, rest @ ..] => {
                // Complex pattern: use Prefix-Wrap strategy for proper nested __assign
                // Example: { a: 1, ...b, c: 2, ...d }
                // Result: __assign(__assign(__assign({ a: 1 }, b), { c: 2 }), d)
                //
                // When the first segment is a spread of an object literal, use it
                // directly as the innermost target (no extra `{}` wrapper):
                // { ...{a:1}, b:2, ...c } → __assign(__assign({a:1}, {b:2}), c)

                let total_segments = 1 + rest.len();
                let first_spread_is_obj_lit = match first {
                    ObjectSegment::Spread(idx) => self.spread_idx_is_simple_object_literal(*idx),
                    _ => false,
                };

                // 1. Emit the necessary number of __assign( calls
                let num_assigns =
                    if matches!(first, ObjectSegment::Spread(_)) && !first_spread_is_obj_lit {
                        total_segments
                    } else {
                        total_segments - 1
                    };

                for _ in 0..num_assigns {
                    self.write_helper("__assign");
                    self.write("(");
                }

                // 2. Handle the first segment
                match first {
                    ObjectSegment::Elements(elems) => {
                        let has_computed = elems
                            .iter()
                            .any(|&idx| emit_utils::is_computed_property_member(self.arena, idx));
                        if has_computed {
                            // Use temp var for computed properties.
                            // Emit properties before the first computed in the literal,
                            // then ALL properties from the first computed onward as assignments.
                            let temp_var = self.make_unique_name_hoisted();
                            self.write("(");
                            self.write(&temp_var);
                            self.write(" = ");
                            let first_computed = elems
                                .iter()
                                .position(|&idx| {
                                    emit_utils::is_computed_property_member(self.arena, idx)
                                })
                                .unwrap_or(elems.len());
                            if first_computed > 0 {
                                self.emit_object_literal_prefix_entries_es5(
                                    &elems[..first_computed],
                                );
                            } else {
                                self.write("{}");
                            }
                            for elem in &elems[first_computed..] {
                                self.write(", ");
                                self.emit_property_assignment_es5(*elem, &temp_var);
                            }
                            self.write(", ");
                            self.write(&temp_var);
                            self.write(")");
                        } else {
                            self.emit_object_literal_assign_entries_es5(elems, source_range);
                        }
                    }
                    ObjectSegment::Spread(spread_idx) => {
                        if first_spread_is_obj_lit {
                            self.emit_spread_expr_from_idx(*spread_idx);
                        } else {
                            self.write("{}, ");
                            self.emit_spread_expr_from_idx(*spread_idx);
                            self.write(")");
                        }
                    }
                }

                // 3. Handle subsequent segments
                for segment in rest {
                    self.write(", ");
                    match segment {
                        ObjectSegment::Elements(elems) => {
                            let has_computed = elems.iter().any(|&idx| {
                                emit_utils::is_computed_property_member(self.arena, idx)
                            });
                            if has_computed {
                                let temp_var = self.make_unique_name_hoisted();
                                self.write("(");
                                self.write(&temp_var);
                                self.write(" = ");
                                let first_computed = elems
                                    .iter()
                                    .position(|&idx| {
                                        emit_utils::is_computed_property_member(self.arena, idx)
                                    })
                                    .unwrap_or(elems.len());
                                if first_computed > 0 {
                                    self.emit_object_literal_prefix_entries_es5(
                                        &elems[..first_computed],
                                    );
                                } else {
                                    self.write("{}");
                                }
                                for elem in &elems[first_computed..] {
                                    self.write(", ");
                                    self.emit_property_assignment_es5(*elem, &temp_var);
                                }
                                self.write(", ");
                                self.write(&temp_var);
                                self.write(")");
                            } else if !elems.is_empty() {
                                self.emit_object_literal_assign_entries_es5(elems, source_range);
                            } else {
                                self.write("{}");
                            }
                        }
                        ObjectSegment::Spread(spread_idx) => {
                            self.emit_spread_expr_from_idx(*spread_idx);
                        }
                    }
                    self.write(")");
                }
            }
        }
    }
}
