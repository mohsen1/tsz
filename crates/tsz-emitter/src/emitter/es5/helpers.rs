use super::super::Printer;
use crate::transforms::emit_utils;
use std::sync::Arc;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Segment of an array literal for ES5 spread transformation
pub(in crate::emitter) enum ArraySegment<'a> {
    /// Non-spread elements: [1, 2, 3]
    Elements(&'a [NodeIndex]),
    /// Spread element: ...arr
    Spread(NodeIndex),
}

impl<'a> Printer<'a> {
    /// Emit an array literal with ES5 spread transformation.
    /// Uses TypeScript's __spreadArray helper for exact tsc matching.
    /// Pattern: [...a] -> __spreadArray([], a, true)
    /// Pattern: [...a, 1] -> __spreadArray(__spreadArray([], a, true), [1], false)
    /// Pattern: [1, ...a] -> __spreadArray([1], a, true)
    /// Pattern: [1, ...a, 2] -> __spreadArray(__spreadArray([1], a, true), [2], false)
    pub(in crate::emitter) fn emit_array_literal_es5(&mut self, elements: &[NodeIndex]) {
        if let Some(flattened) = self.flatten_single_spread_array_literal(elements) {
            self.write("[");
            self.emit_comma_separated(&flattened);
            self.write("]");
            return;
        }

        if elements.is_empty() {
            self.write("[]");
            return;
        }

        let wrap_spread_with_read = self.ctx.target_es5 && self.ctx.options.downlevel_iteration;

        // Split array into segments by spread elements
        let mut segments: Vec<ArraySegment> = Vec::new();
        let mut current_start = 0;

        for (i, &elem_idx) in elements.iter().enumerate() {
            if emit_utils::is_spread_element(self.arena, elem_idx) {
                // Add non-spread segment before this spread
                if current_start < i {
                    segments.push(ArraySegment::Elements(&elements[current_start..i]));
                }
                // Add the spread element
                segments.push(ArraySegment::Spread(elem_idx));
                current_start = i + 1;
            }
        }

        // Add remaining elements after last spread
        if current_start < elements.len() {
            segments.push(ArraySegment::Elements(&elements[current_start..]));
        }

        // Emit using __spreadArray for exact tsc matching.
        // tsc uses nested __spreadArray calls for multi-segment arrays:
        //   [1, ...a, 2, ...b] -> __spreadArray(__spreadArray(__spreadArray([1], a, true), [2], false), b, true)
        if segments.is_empty() {
            self.write("[]");
        } else if segments.len() == 1 {
            match &segments[0] {
                ArraySegment::Elements(elems) => {
                    // No spreads, emit normally
                    self.write("[");
                    self.emit_comma_separated(elems);
                    self.write("]");
                }
                ArraySegment::Spread(spread_idx) => {
                    // Only a spread element: [...a] -> __spreadArray([], a, true)
                    // When __read wraps the spread, the pack arg is false because
                    // __read already produces an array.
                    self.write_helper("__spreadArray");
                    self.write("([], ");
                    if let Some(spread_node) = self.arena.get(*spread_idx) {
                        self.emit_spread_expression_with_read(spread_node, wrap_spread_with_read);
                    }
                    if wrap_spread_with_read {
                        self.write(", false)");
                    } else {
                        self.write(", true)");
                    }
                }
            }
        } else {
            // Multiple segments: use nested __spreadArray calls.
            // Open __spreadArray( for all pairs (segments.len() - 1 calls).
            for _ in 0..segments.len() - 1 {
                self.write_helper("__spreadArray");
                self.write("(");
            }

            // Emit the first segment as the innermost base.
            match &segments[0] {
                ArraySegment::Elements(elems) => {
                    self.write("[");
                    self.emit_comma_separated(elems);
                    self.write("]");
                }
                ArraySegment::Spread(spread_idx) => {
                    // First segment is spread: base is __spreadArray([], spread, true)
                    // unless __read already packed the spread source.
                    self.write_helper("__spreadArray");
                    self.write("([], ");
                    if let Some(spread_node) = self.arena.get(*spread_idx) {
                        self.emit_spread_expression_with_read(spread_node, wrap_spread_with_read);
                    }
                    if wrap_spread_with_read {
                        self.write(", false)");
                    } else {
                        self.write(", true)");
                    }
                }
            }

            // Emit remaining segments, each closing one __spreadArray call.
            for segment in &segments[1..] {
                match segment {
                    ArraySegment::Elements(elems) => {
                        self.write(", [");
                        self.emit_comma_separated(elems);
                        self.write("], false)");
                    }
                    ArraySegment::Spread(spread_idx) => {
                        self.write(", ");
                        if let Some(spread_node) = self.arena.get(*spread_idx) {
                            self.emit_spread_expression_with_read(
                                spread_node,
                                wrap_spread_with_read,
                            );
                        }
                        if wrap_spread_with_read {
                            self.write(", false)");
                        } else {
                            self.write(", true)");
                        }
                    }
                }
            }
        }
    }

    fn flatten_single_spread_array_literal(
        &self,
        elements: &[NodeIndex],
    ) -> Option<Vec<NodeIndex>> {
        let [spread_idx] = elements else {
            return None;
        };
        let spread_node = self.arena.get(*spread_idx)?;
        let spread = self.arena.get_spread(spread_node)?;
        let expr_node = self.arena.get(spread.expression)?;
        if expr_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let literal = self.arena.get_literal_expr(expr_node)?;
        self.flatten_array_literal_elements(&literal.elements.nodes)
    }

    fn flatten_array_literal_elements(&self, elements: &[NodeIndex]) -> Option<Vec<NodeIndex>> {
        let mut flattened = Vec::new();
        for &elem_idx in elements {
            if elem_idx == NodeIndex::NONE {
                return None;
            }
            let elem_node = self.arena.get(elem_idx)?;
            if emit_utils::is_spread_element(self.arena, elem_idx) {
                let spread = self.arena.get_spread(elem_node)?;
                let expr_node = self.arena.get(spread.expression)?;
                if expr_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                    return None;
                }
                let literal = self.arena.get_literal_expr(expr_node)?;
                flattened.extend(self.flatten_array_literal_elements(&literal.elements.nodes)?);
            } else {
                flattened.push(elem_idx);
            }
        }
        Some(flattened)
    }

    pub(in crate::emitter) fn emit_spread_expression(&mut self, node: &Node) {
        if let Some(spread) = self.arena.get_spread(node) {
            self.emit(spread.expression);
        }
    }
    pub(in crate::emitter) fn emit_property_assignment_es5(
        &mut self,
        prop_idx: NodeIndex,
        temp_var: &str,
    ) {
        let Some(node) = self.arena.get(prop_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.arena.get_property_assignment(node) {
                    self.emit_assignment_target_es5(prop.name, temp_var);
                    self.write(" = ");
                    self.emit(prop.initializer);
                }
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(shorthand) = self.arena.get_shorthand_property(node) {
                    self.write(temp_var);
                    self.write(".");
                    self.write_identifier_text(shorthand.name);
                    self.write(" = ");
                    self.emit(shorthand.name);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    self.emit_assignment_target_es5(method.name, temp_var);
                    self.write(" = ");
                    self.emit_object_literal_method_value_es5(node, method);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    self.write("Object.defineProperty(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit_property_key_string(accessor.name);
                    self.write(", {");
                    self.write_line();
                    self.increase_indent();
                    self.write("get: function () ");
                    let prev_fb = self.emitting_function_body_block;
                    self.emitting_function_body_block = true;
                    let saved_temps = std::mem::take(&mut self.hoisted_assignment_temps);
                    let previous_new_target_capture = self
                        .function_like_contains_new_target(
                            accessor.body,
                            &accessor.parameters.nodes,
                        )
                        .then(|| self.push_new_target_capture_for_initializer("void 0".into()));
                    self.emit(accessor.body);
                    if let Some(previous) = previous_new_target_capture {
                        self.restore_new_target_capture(previous);
                    }
                    self.hoisted_assignment_temps = saved_temps;
                    self.emitting_function_body_block = prev_fb;
                    if let Some(body_node) = self.arena.get(accessor.body) {
                        self.emit_accessor_descriptor_trailing(body_node.pos, body_node.end);
                    }
                }
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    self.write("Object.defineProperty(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit_property_key_string(accessor.name);
                    self.write(", {");
                    self.write_line();
                    self.increase_indent();
                    self.write("set: function (");
                    let needs_param_transforms =
                        accessor.parameters.nodes.iter().any(|&param_idx| {
                            self.arena
                                .get(param_idx)
                                .and_then(|param_node| self.arena.get_parameter(param_node))
                                .is_some_and(|param| {
                                    param.dot_dot_dot_token
                                        || param.initializer.is_some()
                                        || self.is_binding_pattern(param.name)
                                })
                        });
                    let param_transforms = if needs_param_transforms {
                        let transforms =
                            self.emit_function_parameters_es5(&accessor.parameters.nodes);
                        Some(transforms)
                    } else {
                        self.emit_function_parameters_js(&accessor.parameters.nodes);
                        None
                    };
                    self.write(") ");
                    let prev_fb = self.emitting_function_body_block;
                    self.emitting_function_body_block = true;
                    let saved_temps = std::mem::take(&mut self.hoisted_assignment_temps);
                    let previous_new_target_capture = self
                        .function_like_contains_new_target(
                            accessor.body,
                            &accessor.parameters.nodes,
                        )
                        .then(|| self.push_new_target_capture_for_initializer("void 0".into()));
                    if let Some(transforms) = &param_transforms {
                        if transforms.has_transforms() {
                            self.emit_block_with_param_prologue(accessor.body, transforms);
                        } else {
                            self.emit(accessor.body);
                        }
                    } else {
                        self.emit(accessor.body);
                    }
                    if let Some(previous) = previous_new_target_capture {
                        self.restore_new_target_capture(previous);
                    }
                    self.hoisted_assignment_temps = saved_temps;
                    self.emitting_function_body_block = prev_fb;
                    if param_transforms.is_some() {
                        self.pop_temp_scope();
                    }
                    if let Some(body_node) = self.arena.get(accessor.body) {
                        self.emit_accessor_descriptor_trailing(body_node.pos, body_node.end);
                    }
                }
            }
            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                // Spread: { ...x } → Object.assign(_a, x)
                if let Some(spread) = self.arena.get_spread(node) {
                    self.write("Object.assign(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit(spread.expression);
                    self.write(")");
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT => {
                // Spread: { ...x } → Object.assign(_a, x)
                if let Some(spread) = self.arena.unary_exprs_ex.get(node.data_index as usize) {
                    self.write("Object.assign(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit_expression(spread.expression);
                    self.write(")");
                }
            }
            _ => {}
        }
    }

    /// Emit assignment target for ES5 computed property transform
    /// For computed: _a[expr]
    /// For regular: _a.name
    pub(in crate::emitter) fn emit_assignment_target_es5(
        &mut self,
        name_idx: NodeIndex,
        temp_var: &str,
    ) {
        self.emit_assignment_target_es5_with_computed(name_idx, temp_var, None);
    }

    /// Emit assignment target for ES5 computed property transform with optional computed temp
    /// For computed: _a[_temp] (if `computed_temp` is Some)
    /// For regular: _a.name
    pub(in crate::emitter) fn emit_assignment_target_es5_with_computed(
        &mut self,
        name_idx: NodeIndex,
        temp_var: &str,
        computed_temp: Option<&str>,
    ) {
        self.write(temp_var);

        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            // Computed property: use the temp variable if provided, otherwise emit expression
            if let Some(temp) = computed_temp {
                self.write("[");
                self.write(temp);
                self.write("]");
            } else if let Some(computed) = self.arena.get_computed_property(name_node) {
                self.write("[");
                self.emit(computed.expression);
                self.write("]");
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            // Regular identifier: _a.name
            self.write(".");
            self.write_identifier_text(name_idx);
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            // String literal: _a["name"]
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("[\"");
                self.write(&lit.text);
                self.write("\"]");
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16 {
            // Numeric literal: _a[123]
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("[");
                self.write(&lit.text);
                self.write("]");
            }
        }
    }

    /// Emit property key as a string for Object.defineProperty
    pub(in crate::emitter) fn emit_property_key_string(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            // Computed property: emit the expression directly
            if let Some(computed) = self.arena.get_computed_property(name_node) {
                self.emit(computed.expression);
            }
        } else if name_node.kind == SyntaxKind::Identifier as u16 {
            self.write("\"");
            self.write_identifier_text(name_idx);
            self.write("\"");
        } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
            if let Some(lit) = self.arena.get_literal(name_node) {
                self.write("\"");
                self.write(&lit.text);
                self.write("\"");
            }
        } else if name_node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.arena.get_literal(name_node)
        {
            self.write(&lit.text);
        }
    }

    /// Emit the trailing portion of an `Object.defineProperty` accessor descriptor.
    fn emit_accessor_descriptor_trailing(&mut self, body_pos: u32, body_end: u32) {
        // body_end includes trailing trivia; find the actual `}` position first.
        let token_end = self.find_token_end_before_trivia(body_pos, body_end);
        // Check for a trailing `// ...` comment between `}` and the next newline.
        let trailing_comment = self.extract_trailing_line_comment(token_end);
        if let Some(ref comment) = trailing_comment {
            self.write(" ");
            self.write(comment);
            self.write_line();
            self.write(",");
        } else {
            self.write(",");
        }
        self.write_line();
        self.write("enumerable: false,");
        self.write_line();
        self.write("configurable: true");
        self.write_line();
        self.decrease_indent();
        self.write("})");
    }

    /// Extract a trailing `// ...` comment at source position `pos`, and advance
    /// `comment_emit_idx` past it so the main comment system won't re-emit it.
    fn extract_trailing_line_comment(&mut self, pos: u32) -> Option<String> {
        if self.ctx.options.remove_comments {
            return None;
        }
        let source = self.source_text?;
        let start = pos as usize;
        if start >= source.len() {
            return None;
        }
        let rest = &source[start..];
        let trimmed = rest.trim_start_matches([' ', '\t']);
        if !trimmed.starts_with("//") {
            return None;
        }
        let line_end = trimmed.find('\n').unwrap_or(trimmed.len());
        let comment_text = trimmed[..line_end].trim_end().to_string();

        // Advance comment_emit_idx past this comment so the main comment
        // system does not emit it again at statement level.
        let comment_start = start + (rest.len() - trimmed.len());
        while self.comment_emit_idx < self.all_comments.len() {
            let c = &self.all_comments[self.comment_emit_idx];
            if (c.pos as usize) >= comment_start && (c.pos as usize) < comment_start + line_end {
                self.comment_emit_idx += 1;
                break;
            } else if (c.pos as usize) > comment_start + line_end {
                break;
            }
            self.comment_emit_idx += 1;
        }

        Some(comment_text)
    }

    /// Emit ES5-compatible function expression for arrow function
    /// Arrow: (x) => x + 1  →  function (x) { return x + 1; }
    ///
    /// When `class_alias` is Some (arrow in static member), use class alias capture:
    /// var _a = Vector; _a.foo = () => _a;
    ///
    /// Otherwise use IIFE capture:
    /// (function (_this) { return _this.x; })(this)
    pub(in crate::emitter) fn emit_arrow_function_es5(
        &mut self,
        _node: &Node,
        func: &tsz_parser::parser::node::FunctionData,
        _captures_this: bool,
        _captures_arguments: bool,
        _class_alias: &Option<Arc<str>>,
    ) {
        // Arrow functions are transformed to regular function expressions.
        // `this` capture is handled by `var _this = this;` at the enclosing
        // function scope (inserted during block emission). The lowering pass
        // marks `this` references with SubstituteThis to emit `_this` instead.

        if func.is_async {
            // Arrow functions don't have their own `this`. In ES5 lowering,
            // the lowering directive asks for `_this` both when the body
            // spells `this` and when an async arrow needs a lexical thisArg
            // passed into `__awaiter`.
            let this_expr = if _captures_this { "_this" } else { "void 0" };
            // TSC wraps async arrow→function conversions inline:
            // function () { return __awaiter(<lexical-this>, ..., function () { ... }); };
            self.emit_async_arrow_es5_inline(func, this_expr);
        } else {
            // Emit any leading comments before the arrow function's `(`.
            if let Some(&first_param_idx) = func.parameters.nodes.first()
                && let Some(first_param) = self.arena.get(first_param_idx)
                && let Some(source) = self.source_text
            {
                let bytes = source.as_bytes();
                let mut pos = first_param.pos as usize;
                while pos > 0 {
                    pos -= 1;
                    if bytes[pos] == b'(' {
                        break;
                    }
                }
                if bytes.get(pos) == Some(&b'(') && self.has_pending_comment_before(pos as u32) {
                    self.emit_comments_before_pos(pos as u32);
                    self.pending_block_comment_space = false;
                    self.write(" ");
                }
            }
            self.write("function (");
            let param_transforms = self.emit_function_parameters_es5(&func.parameters.nodes);
            self.write(") ");

            // If body is not a block (concise arrow), wrap with return
            let body_node = self.arena.get(func.body);
            let is_block = body_node.is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
            let needs_param_prologue = param_transforms.has_transforms();
            let prev_emitting_function_body_block = self.emitting_function_body_block;
            self.emitting_function_body_block = true;
            self.prepare_logical_assignment_value_temps(func.body);

            if is_block {
                // Check if it's a simple single-return block
                if let Some(block_node) = self.arena.get(func.body) {
                    if let Some(block) = self.arena.get_block(block_node) {
                        if block.statements.nodes.is_empty() {
                            if needs_param_prologue {
                                self.emit_block_with_param_prologue(func.body, &param_transforms);
                            } else if self.is_single_line(block_node) {
                                self.write("{ }");
                            } else {
                                self.write("{");
                                self.write_line();
                                self.write("}");
                            }
                            self.emitting_function_body_block = prev_emitting_function_body_block;
                            self.pop_temp_scope();
                            return;
                        }
                        if !needs_param_prologue
                            && !self.has_pending_new_target_capture()
                            && block.statements.nodes.len() == 1
                            && self.is_simple_return_statement(block.statements.nodes[0])
                            && self.is_single_line(block_node)
                        {
                            self.emit_single_line_block(func.body);
                        } else if needs_param_prologue {
                            self.emit_block_with_param_prologue(func.body, &param_transforms);
                        } else {
                            self.emit(func.body);
                        }
                    } else if needs_param_prologue {
                        self.emit_block_with_param_prologue(func.body, &param_transforms);
                    } else {
                        self.emit(func.body);
                    }
                } else if needs_param_prologue {
                    self.emit_block_with_param_prologue(func.body, &param_transforms);
                } else {
                    self.emit(func.body);
                }
            } else if needs_param_prologue {
                let needs_parens = self.concise_body_needs_parens(func.body);
                self.write("{");
                self.write_line();
                self.increase_indent();
                self.emit_param_prologue(&param_transforms);
                let comments_before_return =
                    self.es5_arrow_concise_body_needs_multiline_return(func.body);
                self.emit_es5_arrow_concise_return(func.body, needs_parens, comments_before_return);
                self.write_line();
                self.decrease_indent();
                self.write("}");
            } else {
                // Concise body: (x) => x + 1  →  function (x) { return x + 1; }
                // If the body is (or resolves to) an object literal, wrap in parens
                // to disambiguate from a block: () => ({})  →  function () { return ({}); }
                let needs_parens = self.concise_body_needs_parens(func.body);
                if self.es5_arrow_concise_body_needs_multiline_return(func.body) {
                    self.write("{");
                    self.write_line();
                    self.increase_indent();
                    self.emit_es5_arrow_concise_return(func.body, needs_parens, true);
                    self.write_line();
                    self.decrease_indent();
                    self.write("}");
                } else {
                    self.write("{ ");
                    self.emit_es5_arrow_concise_return(func.body, needs_parens, false);
                    self.write(" }");
                }
            }
            self.emitting_function_body_block = prev_emitting_function_body_block;
            self.pop_temp_scope();
        }
    }

    fn es5_arrow_concise_body_needs_multiline_return(&self, body: NodeIndex) -> bool {
        self.arena.get(body).is_some_and(|body_node| {
            self.pending_comment_before_pos_starts_after_newline(body_node.pos)
        })
    }

    fn emit_es5_arrow_concise_return(
        &mut self,
        body: NodeIndex,
        needs_parens: bool,
        comments_before_return: bool,
    ) {
        if comments_before_return && let Some(body_node) = self.arena.get(body) {
            self.emit_comments_before_pos(body_node.pos);
        }
        if needs_parens {
            self.write("return (");
        } else {
            self.write("return ");
        }
        if !comments_before_return && let Some(body_node) = self.arena.get(body) {
            self.emit_comments_before_pos(body_node.pos);
        }
        self.emit(body);
        if needs_parens {
            self.write(");");
        } else {
            self.write(";");
        }
    }

    /// Check if a concise arrow body resolves to an object literal expression
    /// and needs wrapping in parens. Returns false if already parenthesized
    /// (to avoid double-parens). Unwraps through type assertions and as-expressions.
    pub(in crate::emitter) fn concise_body_needs_parens(&self, body_idx: NodeIndex) -> bool {
        let mut idx = body_idx;
        loop {
            let Some(node) = self.arena.get(idx) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => return true,
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    if let Some(ta) = self.arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
                {
                    return self.erased_object_literal_access_chain_needs_parens(idx);
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION => {
                    return self.erased_object_literal_access_chain_needs_parens(idx);
                }
                // Already parenthesized — the emitter will preserve the parens
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => return false,
                _ => return false,
            }
        }
    }

    fn erased_object_literal_access_chain_needs_parens(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                self.arena.get_access_expr(node).is_some_and(|access| {
                    self.erased_object_literal_access_chain_needs_parens(access.expression)
                })
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                self.arena.get_call_expr(node).is_some_and(|call| {
                    self.erased_object_literal_access_chain_needs_parens(call.expression)
                })
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.arena.get_parenthesized(node).is_some_and(|paren| {
                    self.erased_object_literal_access_chain_needs_parens(paren.expression)
                })
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                self.type_assertion_wraps_object_literal(idx)
            }
            _ => false,
        }
    }

    pub(in crate::emitter) fn emit_function_expression_es5_params(
        &mut self,
        node: &Node,
        idx: NodeIndex,
    ) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        // Consume the paren flag for TSC-style IIFE parenthesization
        let self_paren = self.ctx.flags.paren_leftmost_function_or_object;
        if self_paren {
            self.ctx.flags.paren_leftmost_function_or_object = false;
            self.write("(");
        }

        self.write("function");

        if func.asterisk_token {
            self.write("*");
        }

        let needs_new_target_capture = self.function_body_contains_new_target(func);
        let function_name =
            self.function_expression_emit_name(idx, func.name, needs_new_target_capture);

        // Name (if any)
        if let Some(name) = function_name.as_deref() {
            self.write_space();
            self.write(name);
        } else {
            // Space before ( only for anonymous functions: function (x) vs function name(x)
            self.write(" ");
        }

        // Parameters (without types for JavaScript)
        self.write("(");
        let param_transforms = self.emit_function_parameters_es5(&func.parameters.nodes);
        self.write(") ");

        // Emit body - check if it's a simple single-statement body
        let body_node = self.arena.get(func.body);
        let is_simple_body = if let Some(body) = body_node {
            if let Some(block) = self.arena.get_block(body) {
                // Single return statement = simple body
                block.statements.nodes.len() == 1
                    && self.is_simple_return_statement(block.statements.nodes[0])
            } else {
                false
            }
        } else {
            false
        };

        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        self.prepare_logical_assignment_value_temps(func.body);
        let previous_new_target_capture = needs_new_target_capture.then(|| {
            self.push_new_target_capture_for_initializer(
                self.ordinary_function_new_target_initializer(function_name.as_deref()),
            )
        });
        if param_transforms.has_transforms() {
            self.emit_block_with_param_prologue(func.body, &param_transforms);
        } else if is_simple_body && !needs_new_target_capture {
            self.emit_single_line_block(func.body);
        } else {
            self.emit(func.body);
        }
        if let Some(previous) = previous_new_target_capture {
            self.restore_new_target_capture(previous);
        }
        self.emitting_function_body_block = prev_emitting_function_body_block;
        self.pop_temp_scope();
        if self_paren {
            self.write(")");
        }
    }

    pub(in crate::emitter) fn emit_function_declaration_es5_params(&mut self, node: &Node) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        // Skip ambient declarations (declare function)
        if self.arena.is_declare(&func.modifiers) {
            return;
        }

        // For JavaScript emit: skip declaration-only functions (no body)
        if func.body.is_none() {
            return;
        }

        let needs_new_target_capture = self.function_body_contains_new_target(func);
        let function_name = if func.name.is_some() {
            Some(self.get_identifier_text_idx(func.name))
        } else {
            needs_new_target_capture.then_some("_a".to_string())
        };

        self.write("function");

        if func.asterisk_token {
            self.write("*");
        }

        // Name
        if let Some(name) = function_name.as_deref() {
            self.write_space();
            self.write(name);
        }

        // Parameters - only emit names, not types for JavaScript
        self.write("(");
        let param_transforms = self.emit_function_parameters_es5(&func.parameters.nodes);
        self.write(")");

        // No return type for JavaScript

        self.write_space();
        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        self.prepare_logical_assignment_value_temps(func.body);
        let previous_new_target_capture = needs_new_target_capture.then(|| {
            self.push_new_target_capture_for_initializer(
                self.ordinary_function_new_target_initializer(function_name.as_deref()),
            )
        });
        if param_transforms.has_transforms() {
            self.emit_block_with_param_prologue(func.body, &param_transforms);
        } else {
            self.emit(func.body);
        }
        if let Some(previous) = previous_new_target_capture {
            self.restore_new_target_capture(previous);
        }
        self.emitting_function_body_block = prev_emitting_function_body_block;
        self.pop_temp_scope();
    }

    /// Emit an ES5 async arrow function with inline body wrapping.
    /// TSC format: `function () { return __awaiter(void 0, ..., function () { ... }); };`
    fn emit_async_arrow_es5_inline(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        this_expr: &str,
    ) {
        let await_param_recovery = func
            .parameters
            .nodes
            .iter()
            .copied()
            .any(|param_idx| self.param_initializer_has_top_level_await(param_idx))
            && crate::transforms::emit_utils::block_is_empty(self.arena, func.body)
            && crate::transforms::emit_utils::first_await_default_param_name(
                self.arena,
                &func.parameters.nodes,
            )
            .is_some();

        if await_param_recovery {
            self.emit_async_arrow_es5_await_param_recovery(func, this_expr);
            return;
        }

        let original_indent_level = self.writer.indent_level();
        let visual_indent_level = self.writer.current_line_visual_indent_level();
        let synced_visual_indent = original_indent_level == 0 && visual_indent_level > 0;
        if synced_visual_indent {
            self.writer.set_indent_level(visual_indent_level);
        }

        // Note: emit_function_parameters_es5 calls push_temp_scope() internally,
        // so we don't push here — the pop at the end of this function balances it.
        self.write("function (");
        // ES5: apply destructuring/default transforms
        let param_transforms = self.emit_function_parameters_es5(&func.parameters.nodes);
        let has_param_transforms = param_transforms.has_transforms();

        // Check if the body references `arguments`. If so, we capture it
        // before the __awaiter call: `var arguments_1 = arguments;`
        let body_captures_arguments =
            tsz_parser::syntax::transform_utils::contains_arguments_reference(
                self.arena, func.body,
            );
        let has_outer_param_prologue = param_transforms.rest.is_some() || body_captures_arguments;

        if has_param_transforms {
            self.write(") {");
            if has_outer_param_prologue {
                self.write_line();
                self.increase_indent();
                self.emit_rest_param_prologue(&param_transforms);
                if body_captures_arguments {
                    self.write("var arguments_1 = arguments;");
                    self.write_line();
                }
            } else {
                self.write(" ");
            }
        }

        // Build the __generator body
        let mut async_emitter = crate::transforms::async_es5::AsyncES5Emitter::new(self.arena);
        async_emitter.set_system_import_meta(self.in_system_execute_body);
        async_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
        async_emitter.set_downlevel_iteration(self.ctx.options.downlevel_iteration);
        // The generator body is nested inside `function () { ... }` in the __awaiter
        // callback, so render it at one extra indent level (matching tsc multi-line format).
        async_emitter.set_indent_level(self.writer.indent_level() + 1);
        if let Some(text) = self.source_text_for_map() {
            async_emitter.set_source_map_context(text, self.writer.current_source_index());
        }
        async_emitter.set_lexical_this(this_expr != "this");
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            async_emitter.set_tslib_prefix(true);
            async_emitter.set_tslib_import_binding(self.commonjs_tslib_import_binding.clone());
        }
        let blocked_disposable_names = self.blocked_disposable_names_for_transform();
        async_emitter
            .set_disposable_env_context(self.next_disposable_env_id, blocked_disposable_names);

        let body_has_await = async_emitter.body_contains_await(func.body);
        let body_is_single_line = self
            .arena
            .get(func.body)
            .is_some_and(|n| self.is_single_line(n));
        let promise_ctor = self.extract_awaiter_promise_constructor(func.type_annotation);
        let (generator_body, hoisted_var_groups, needs_lexical_this_capture) = if body_has_await {
            let (generator_body, hoisted_var_groups, _, needs_lexical_this_capture) =
                async_emitter.emit_generator_body_with_await_and_hoisted_var_groups(func.body);
            (
                generator_body,
                hoisted_var_groups,
                needs_lexical_this_capture,
            )
        } else {
            async_emitter.emit_simple_generator_body_with_hoisted_var_groups(func.body)
        };
        self.ctx.destructuring_state.temp_var_counter = async_emitter.temp_var_counter();
        self.next_disposable_env_id = async_emitter.disposable_env_counter();
        for generated_name in async_emitter.take_generated_disposable_env_names() {
            self.generated_temp_names.insert(generated_name);
        }
        let generator_mappings = async_emitter.take_mappings();

        if has_param_transforms {
            self.write("return ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_expr);
            self.write(", void 0, ");
            self.write_awaiter_promise_arg(&promise_ctor);
            self.write(", function () {");
            self.write_line();
            self.increase_indent();
            self.emit_async_arrow_hoisted_var_groups(
                &hoisted_var_groups,
                needs_lexical_this_capture,
            );
            self.emit_param_binding_prologue(&param_transforms);
            self.write(&generator_body);
            self.decrease_indent();
            self.write_line();
            self.write("});");
            if has_outer_param_prologue {
                self.write_line();
                self.decrease_indent();
                self.write("}");
            } else {
                self.write(" }");
            }
        } else if body_captures_arguments {
            // Arguments capture path: needs multi-line to emit var declaration
            // function () { var arguments_1 = arguments; return __awaiter(...); }
            self.write(") {");
            self.write_line();
            self.increase_indent();
            self.write("var arguments_1 = arguments;");
            self.write_line();
            self.write("return ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_expr);
            self.write(", void 0, ");
            self.write_awaiter_promise_arg(&promise_ctor);
            self.write(", function () {");
            self.write_line();
            self.increase_indent();
            self.emit_async_arrow_hoisted_var_groups(
                &hoisted_var_groups,
                needs_lexical_this_capture,
            );
            if !generator_mappings.is_empty() && self.writer.has_source_map() {
                self.writer.write("");
                let base_line = self.writer.current_line();
                let base_column = self.writer.current_column();
                self.writer
                    .add_offset_mappings(base_line, base_column, &generator_mappings);
                self.writer.write(&generator_body);
            } else {
                self.write(&generator_body);
            }
            self.decrease_indent();
            self.write_line();
            self.write("});");
            self.write_line();
            self.decrease_indent();
            self.write("}");
        } else {
            // Inline path: function () { return __awaiter(..., function () {
            //     return __generator(this, function (_a) { ... });
            // }); }
            self.write(") { return ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_expr);
            if hoisted_var_groups.is_empty() {
                let can_inline_wrapper = func.equals_greater_than_token
                    && body_is_single_line
                    && !body_has_await
                    && !needs_lexical_this_capture
                    && generator_mappings.is_empty();
                if can_inline_wrapper {
                    self.write(", void 0, ");
                    self.write_awaiter_promise_arg(&promise_ctor);
                    self.write(", function () { ");
                    self.write(&Self::inline_async_arrow_generator_body(&generator_body));
                    self.write(" }); }");
                    if synced_visual_indent {
                        self.writer.set_indent_level(original_indent_level);
                    }
                    self.pop_temp_scope();
                    return;
                }
                // Multi-line format (matches tsc): __generator on new line
                self.write(", void 0, ");
                self.write_awaiter_promise_arg(&promise_ctor);
                self.write(", function () {");
                self.write_line();
                self.increase_indent();
                self.emit_async_arrow_hoisted_var_groups(
                    &hoisted_var_groups,
                    needs_lexical_this_capture,
                );
                if !generator_mappings.is_empty() && self.writer.has_source_map() {
                    self.writer.write("");
                    let base_line = self.writer.current_line();
                    let base_column = self.writer.current_column();
                    self.writer
                        .add_offset_mappings(base_line, base_column, &generator_mappings);
                    self.writer.write(&generator_body);
                } else {
                    self.write(&generator_body);
                }
                self.decrease_indent();
                self.write_line();
                self.write("}); }");
            } else {
                // Multi-line format with hoisted vars
                self.write(", void 0, ");
                self.write_awaiter_promise_arg(&promise_ctor);
                self.write(", function () {");
                self.write_line();
                self.increase_indent();
                self.emit_async_arrow_hoisted_var_groups(
                    &hoisted_var_groups,
                    needs_lexical_this_capture,
                );
                if !generator_mappings.is_empty() && self.writer.has_source_map() {
                    self.writer.write("");
                    let base_line = self.writer.current_line();
                    let base_column = self.writer.current_column();
                    self.writer
                        .add_offset_mappings(base_line, base_column, &generator_mappings);
                    self.writer.write(&generator_body);
                } else {
                    self.write(&generator_body);
                }
                self.decrease_indent();
                self.write_line();
                self.write("}); }");
            }
        }
        if synced_visual_indent {
            self.writer.set_indent_level(original_indent_level);
        }
        self.pop_temp_scope();
    }

    fn inline_async_arrow_generator_body(generator_body: &str) -> String {
        let mut lines = generator_body.lines();
        let Some(first_line) = lines.next() else {
            return String::new();
        };

        let following_strip = 4;
        let mut output = String::from(first_line.trim_start());
        for line in lines {
            output.push('\n');
            output.push_str(line.get(following_strip..).unwrap_or(line).trim_end());
        }
        output
    }

    fn emit_async_arrow_hoisted_var_groups(
        &mut self,
        hoisted_var_groups: &[Vec<String>],
        needs_lexical_this_capture: bool,
    ) {
        for group in hoisted_var_groups {
            if group.is_empty() {
                continue;
            }
            self.write("var ");
            for (i, var_name) in group.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(var_name);
            }
            self.write(";");
            self.write_line();
        }

        if needs_lexical_this_capture {
            self.write("var _this = this;");
            self.write_line();
        }
    }

    fn emit_async_arrow_es5_await_param_recovery(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        this_expr: &str,
    ) {
        let Some(param_name) = crate::transforms::emit_utils::first_await_default_param_name(
            self.arena,
            &func.parameters.nodes,
        ) else {
            return;
        };
        let args_name = self.make_unique_name_from_base("args");

        self.write("function () {");
        self.write_line();
        self.increase_indent();
        self.write("var ");
        self.write(&args_name);
        self.write(" = [];");
        self.write_line();
        self.write("for (var _i = 0; _i < arguments.length; _i++) {");
        self.write_line();
        self.increase_indent();
        self.write(&args_name);
        self.write("[_i] = arguments[_i];");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.write("return ");
        self.write_helper("__awaiter");
        self.write("(");
        self.write(this_expr);
        self.write(", ");
        self.write_helper("__spreadArray");
        self.write("([], ");
        self.write(&args_name);
        self.write(", true), void 0, function (");
        self.emit_function_parameter_names_only(&func.parameters.nodes);
        self.write(") {");
        self.write_line();
        self.increase_indent();
        self.write("if (");
        self.write(&param_name);
        self.write(" === void 0) { ");
        self.write(&param_name);
        self.write(" = _a.sent(); }");
        self.write_line();
        self.write("return ");
        self.write_helper("__generator");
        self.write("(this, function (_a) {");
        self.write_line();
        self.increase_indent();
        self.write("switch (_a.label) {");
        self.write_line();
        self.increase_indent();
        self.write("case 0: return [4 /*yield*/, ];");
        self.write_line();
        self.write("case 1: return [2 /*return*/];");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.decrease_indent();
        self.write("});");
        self.write_line();
        self.decrease_indent();
        self.write("});");
        self.write_line();
        self.decrease_indent();
        self.write("}");
    }
}
