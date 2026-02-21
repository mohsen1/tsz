use super::Printer;
use tracing::debug;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Represents a segment of assignment destructuring output.
/// When the right-hand side is a simple identifier, we access properties/elements directly.
/// When complex, we create a temp variable first.
impl<'a> Printer<'a> {
    pub(super) fn emit_variable_declaration_list_es5(&mut self, node: &Node) {
        let Some(decl_list) = self.arena.get_variable(node) else {
            return;
        };

        // Pre-register all variable names in this declaration list to handle shadowing.
        // For let/const: use register_variable (renames for any scope conflict including current)
        // For var: use register_var_declaration (only renames for parent scope conflicts,
        // allowing same-scope redeclarations like `var cl; var cl = Point();`)
        let flags = node.flags as u32;
        let is_block_scoped = (flags & tsz_parser::parser::node_flags::LET != 0)
            || (flags & tsz_parser::parser::node_flags::CONST != 0);
        if is_block_scoped {
            for &decl_idx in &decl_list.declarations.nodes {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    self.pre_register_binding_name(decl.name);
                }
            }
        } else {
            for &decl_idx in &decl_list.declarations.nodes {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    self.pre_register_var_binding_name(decl.name);
                }
            }
        }

        self.write("var");

        let mut first = true;
        for &decl_idx in &decl_list.declarations.nodes {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };

            if self.is_binding_pattern(decl.name) && decl.initializer.is_some() {
                if first {
                    self.write(" ");
                }
                self.emit_es5_destructuring(decl_idx, &mut first);
            } else {
                if first {
                    self.write(" ");
                }
                if !first {
                    self.write(", ");
                }
                first = false;
                self.emit(decl_idx);
            }
        }
    }

    /// Count effective (non-omitted) bindings in a destructuring pattern
    pub(super) fn count_effective_bindings(&self, pattern_node: &Node) -> (usize, bool) {
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return (0, false);
        };
        let mut count = 0;
        let mut has_rest = false;
        for &elem_idx in &pattern.elements.nodes {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };
            if elem.dot_dot_dot_token {
                has_rest = true;
            } else {
                count += 1;
            }
        }
        (count, has_rest)
    }

    /// For single-binding array patterns with complex expressions,
    /// find the single effective binding's index and emit inline.
    pub(super) fn emit_single_array_binding_inline(
        &mut self,
        pattern_node: &Node,
        initializer: NodeIndex,
        first: &mut bool,
    ) -> bool {
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return false;
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return false;
        };

        // TypeScript only inlines when the single binding is at index 0
        // (no preceding omitted elements). For [, x] it uses a temp.
        let first_elem = pattern.elements.nodes.first().copied();
        let Some(first_elem_idx) = first_elem else {
            return false;
        };
        if first_elem_idx.is_none() {
            return false; // First element is omitted, can't inline
        }
        let Some(first_elem_node) = self.arena.get(first_elem_idx) else {
            return false;
        };
        let Some(first_elem_data) = self.arena.get_binding_element(first_elem_node) else {
            return false;
        };
        if first_elem_data.dot_dot_dot_token {
            return false;
        }
        if self.is_binding_pattern(first_elem_data.name) {
            return false;
        }
        if !self.has_identifier_text(first_elem_data.name) {
            return false;
        }

        let binding_idx = Some((first_elem_idx, 0usize, first_elem_data.initializer));
        let binding_array_index = 0;

        let Some((_elem_idx, _idx, initializer_default)) = binding_idx else {
            return false;
        };

        // Find the binding element data again
        let elem_idx = pattern
            .elements
            .nodes
            .iter()
            .enumerate()
            .find(|(i, n)| *i == binding_array_index && n.is_some())
            .map(|(_, &n)| n);
        let Some(elem_idx) = elem_idx else {
            return false;
        };
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return false;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return false;
        };

        if initializer_default.is_none() {
            // Simple case: name = expr[index]
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.emit(initializer);
            self.write("[");
            self.write_usize(binding_array_index);
            self.write("]");
        } else {
            // Default value case: _a = expr[index], name = _a === void 0 ? default : _a
            let value_name = self.get_temp_var_name();
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write(&value_name);
            self.write(" = ");
            self.emit(initializer);
            self.write("[");
            self.write_usize(binding_array_index);
            self.write("]");
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(initializer_default);
            self.write(" : ");
            self.write(&value_name);
        }
        true
    }

    /// For rest-only array patterns [...rest] = expr, emit: rest = expr.slice(0)
    /// TypeScript inlines this without a temp variable for any expression type.
    pub(super) fn emit_rest_only_array_inline(
        &mut self,
        pattern_node: &Node,
        initializer: NodeIndex,
        first: &mut bool,
    ) -> bool {
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return false;
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return false;
        };

        // Find the rest element (should be the only element)
        let mut rest_name_idx = NodeIndex::NONE;
        for &elem_idx in &pattern.elements.nodes {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };
            if elem.dot_dot_dot_token {
                rest_name_idx = elem.name;
                break;
            }
        }

        if rest_name_idx.is_none() {
            return false;
        }
        if !self.has_identifier_text(rest_name_idx) {
            return false;
        }

        // Emit: rest = expr.slice(0)
        if !*first {
            self.write(", ");
        }
        *first = false;
        self.write_identifier_text(rest_name_idx);
        self.write(" = ");
        self.emit(initializer);
        self.write(".slice(0)");
        true
    }

    /// Inline a single-element array pattern at index 0 from a string expression.
    /// [x] from expr → x = expr[0]
    pub(super) fn try_emit_single_inline_from_expr(
        &mut self,
        pattern_node: &Node,
        expr: &str,
        first: &mut bool,
    ) -> bool {
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return false;
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return false;
        };
        // Must be first element, not omitted
        let first_elem = pattern.elements.nodes.first().copied();
        let Some(first_elem_idx) = first_elem else {
            return false;
        };
        if first_elem_idx.is_none() {
            return false;
        }
        let Some(first_elem_node) = self.arena.get(first_elem_idx) else {
            return false;
        };
        let Some(first_elem_data) = self.arena.get_binding_element(first_elem_node) else {
            return false;
        };
        if first_elem_data.dot_dot_dot_token || self.is_binding_pattern(first_elem_data.name) {
            return false;
        }
        if !self.has_identifier_text(first_elem_data.name) {
            return false;
        }

        if first_elem_data.initializer.is_none() {
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write_identifier_text(first_elem_data.name);
            self.write(" = ");
            self.write(expr);
            self.write("[0]");
        } else {
            let value_name = self.get_temp_var_name();
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write(&value_name);
            self.write(" = ");
            self.write(expr);
            self.write("[0]");
            self.write(", ");
            self.write_identifier_text(first_elem_data.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(first_elem_data.initializer);
            self.write(" : ");
            self.write(&value_name);
        }
        true
    }

    /// Inline a rest-only array pattern from a string expression.
    /// [...rest] from expr → rest = expr.slice(0)
    pub(super) fn try_emit_rest_only_from_expr(
        &mut self,
        pattern_node: &Node,
        expr: &str,
        first: &mut bool,
    ) -> bool {
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return false;
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return false;
        };
        for &elem_idx in &pattern.elements.nodes {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };
            if elem.dot_dot_dot_token && self.has_identifier_text(elem.name) {
                if !*first {
                    self.write(", ");
                }
                *first = false;
                self.write_identifier_text(elem.name);
                self.write(" = ");
                self.write(expr);
                self.write(".slice(0)");
                return true;
            }
        }
        false
    }

    pub(super) fn unwrap_parenthesized_binding_pattern(
        &self,
        mut pattern_idx: NodeIndex,
    ) -> NodeIndex {
        while let Some(node) = self.arena.get(pattern_idx) {
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                break;
            }
            let Some(paren) = self.arena.get_parenthesized(node) else {
                break;
            };
            if paren.expression.is_none() {
                break;
            }
            pattern_idx = paren.expression;
        }
        pattern_idx
    }

    pub(super) fn is_binding_pattern_array_shape(&self, pattern_node: &Node) -> bool {
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return false;
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return false;
        };
        pattern.elements.nodes.iter().all(|&elem_idx| {
            if elem_idx.is_none() {
                return true;
            }
            let Some(elem_node) = self.arena.get(elem_idx) else {
                return false;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                return false;
            };
            if elem.dot_dot_dot_token {
                return true;
            }
            elem.property_name.is_none()
        })
    }

    pub(super) fn binding_pattern_non_rest_count(&self, pattern_node: &Node) -> usize {
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return 0;
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return 0;
        };
        let mut count = 0;
        for &elem_idx in &pattern.elements.nodes {
            if elem_idx.is_none() {
                count += 1;
                continue;
            }

            let Some(node) = self.arena.get(elem_idx) else {
                count += 1;
                continue;
            };
            let Some(element) = self.arena.get_binding_element(node) else {
                count += 1;
                continue;
            };
            if element.dot_dot_dot_token {
                break;
            }
            count += 1;
        }
        count
    }

    /// Emit ES5 destructuring: { x, y } = obj → _a = obj, x = _a.x, y = _a.y
    /// When the initializer is a simple identifier, TypeScript skips the temp variable
    /// and uses the identifier directly: var [, name] = robot → var name = robot[1]
    pub(super) fn emit_es5_destructuring(&mut self, decl_idx: NodeIndex, first: &mut bool) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };
        let Some(pattern_node) = self.arena.get(decl.name) else {
            return;
        };

        let is_simple_ident = self
            .arena
            .get(decl.initializer)
            .is_some_and(|n| n.kind == SyntaxKind::Identifier as u16);

        if is_simple_ident {
            let ident_text = self.get_identifier_text(decl.initializer);
            self.emit_es5_destructuring_pattern_direct(pattern_node, &ident_text, first);
            return;
        }

        if self.ctx.options.downlevel_iteration
            && pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
        {
            self.emit_es5_destructuring_with_read_node(decl.name, decl.initializer, first);
            return;
        }

        self.emit_es5_destructuring_fallback(pattern_node, decl.initializer, first, true);
    }

    pub(super) fn emit_es5_destructuring_fallback(
        &mut self,
        pattern_node: &Node,
        initializer: NodeIndex,
        first: &mut bool,
        allow_expression_emit: bool,
    ) {
        let (effective_count, has_rest) = self.count_effective_bindings(pattern_node);
        if effective_count == 1
            && !has_rest
            && self.emit_single_object_binding_inline_nested(
                pattern_node,
                initializer,
                first,
                allow_expression_emit,
            )
        {
            return;
        }
        if effective_count == 1
            && !has_rest
            && self.emit_single_object_binding_inline_simple(pattern_node, initializer, first)
        {
            return;
        }
        if effective_count == 1
            && !has_rest
            && self.emit_single_array_binding_inline(pattern_node, initializer, first)
        {
            return;
        }

        if effective_count == 0
            && has_rest
            && self.emit_rest_only_array_inline(pattern_node, initializer, first)
        {
            return;
        }

        let temp_name = self.get_temp_var_name();
        if !*first {
            self.write(", ");
        }
        *first = false;
        self.write(&temp_name);
        self.write(" = ");
        if allow_expression_emit {
            self.emit(initializer);
        } else {
            self.emit_expression(initializer);
        }

        self.emit_es5_destructuring_pattern(pattern_node, &temp_name);
    }

    // ES5 parity: for a single object binding with an identifier key, inline source access.
    // Example: var { x } = { x: 1 } -> var x = { x: 1 }.x
    // Default initializer still uses a value temp:
    // var { z = "" } = { z: undefined } -> var _a = { z: undefined }.z, z = _a === void 0 ? "" : _a
    pub(super) fn emit_single_object_binding_inline_simple(
        &mut self,
        pattern_node: &Node,
        initializer: NodeIndex,
        first: &mut bool,
    ) -> bool {
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return false;
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return false;
        };

        let mut elems = pattern
            .elements
            .nodes
            .iter()
            .copied()
            .filter(|n| n.is_some());
        let Some(elem_idx) = elems.next() else {
            return false;
        };
        if elems.next().is_some() {
            return false;
        }

        let Some(elem_node) = self.arena.get(elem_idx) else {
            return false;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return false;
        };
        if elem.dot_dot_dot_token
            || self.is_binding_pattern(elem.name)
            || !self.has_identifier_text(elem.name)
        {
            return false;
        }

        let key_idx = if elem.property_name.is_some() {
            elem.property_name
        } else {
            elem.name
        };
        let Some(key_node) = self.arena.get(key_idx) else {
            return false;
        };
        if key_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let key_text = self.get_identifier_text(key_idx);

        if !*first {
            self.write(", ");
        }
        *first = false;

        if elem.initializer.is_none() {
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.emit(initializer);
            self.write(".");
            self.write(&key_text);
        } else {
            let value_name = self.get_temp_var_name();
            self.write(&value_name);
            self.write(" = ");
            self.emit(initializer);
            self.write(".");
            self.write(&key_text);
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }

        true
    }

    pub(super) fn emit_single_object_binding_inline_nested(
        &mut self,
        pattern_node: &Node,
        initializer: NodeIndex,
        first: &mut bool,
        allow_expression_emit: bool,
    ) -> bool {
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return false;
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return false;
        };

        let mut elem_idx = NodeIndex::NONE;
        for idx in &pattern.elements.nodes {
            if idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.arena.get(*idx) else {
                continue;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };
            if elem.dot_dot_dot_token {
                continue;
            }
            elem_idx = *idx;
            break;
        }

        if elem_idx.is_none() {
            return false;
        }
        if pattern.elements.nodes.len() > 1
            && pattern
                .elements
                .nodes
                .iter()
                .filter(|&&idx| {
                    if idx.is_none() {
                        return false;
                    }
                    let Some(node) = self.arena.get(idx) else {
                        return false;
                    };
                    let Some(element) = self.arena.get_binding_element(node) else {
                        return false;
                    };
                    !element.dot_dot_dot_token
                })
                .count()
                > 1
        {
            return false;
        }

        let Some(elem_node) = self.arena.get(elem_idx) else {
            return false;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return false;
        };
        if elem.dot_dot_dot_token || !self.is_binding_pattern(elem.name) {
            return false;
        }

        let pattern_name = self.unwrap_parenthesized_binding_pattern(elem.name);
        let Some(_pattern_name_node) = self.arena.get(pattern_name) else {
            return false;
        };
        if !self.is_binding_pattern(pattern_name) {
            return false;
        }

        let key_idx = if elem.property_name.is_some() {
            elem.property_name
        } else {
            elem.name
        };
        let Some(key_node) = self.arena.get(key_idx) else {
            return false;
        };
        if key_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        if !*first {
            self.write(", ");
        }
        *first = false;

        let Some(pattern_node) = self.arena.get(pattern_name) else {
            return false;
        };
        let is_array_shape = self.is_binding_pattern_array_shape(pattern_node);

        if elem.initializer.is_none() {
            if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                let child_count = self.binding_pattern_non_rest_count(pattern_node);
                if is_array_shape {
                    let read_name = self.get_temp_var_name();
                    self.write(&read_name);
                    self.write(" = __read(");
                    if allow_expression_emit {
                        self.emit(initializer);
                    } else {
                        self.emit_expression(initializer);
                    }
                    self.write(".");
                    self.write_identifier_text(key_idx);
                    if child_count > 0 {
                        self.write(", ");
                        self.write(&child_count.to_string());
                    }
                    self.write(")");
                    self.emit_es5_destructuring_pattern_idx(pattern_name, &read_name);
                } else {
                    let value_name = self.get_temp_var_name();
                    self.write(", ");
                    self.write(&value_name);
                    self.write(" = ");
                    if allow_expression_emit {
                        self.emit(initializer);
                    } else {
                        self.emit_expression(initializer);
                    }
                    self.write(".");
                    self.write_identifier_text(key_idx);

                    if child_count > 0 {
                        let read_name = self.get_temp_var_name();
                        self.write(", ");
                        self.write(&read_name);
                        self.write(" = __read(");
                        self.write(&value_name);
                        self.write(", ");
                        self.write(&child_count.to_string());
                        self.write(")");
                        self.emit_es5_destructuring_pattern_idx(pattern_name, &read_name);
                    } else {
                        self.emit_es5_destructuring_pattern_idx(pattern_name, &value_name);
                    }
                }
                return true;
            }

            if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                && self.emit_single_object_binding_inline_nested_object_node(
                    pattern_name,
                    initializer,
                    key_idx,
                    allow_expression_emit,
                )
            {
                return true;
            }

            let value_name = self.get_temp_var_name();
            self.write(&value_name);
            self.write(" = ");
            if allow_expression_emit {
                self.emit(initializer);
            } else {
                self.emit_expression(initializer);
            }
            self.write(".");
            self.write_identifier_text(key_idx);
            self.emit_es5_destructuring_pattern_idx(pattern_name, &value_name);
            return true;
        }

        let value_name = self.get_temp_var_name();
        self.write(&value_name);
        self.write(" = ");
        if allow_expression_emit {
            self.emit(initializer);
        } else {
            self.emit_expression(initializer);
        }
        self.write(".");
        self.write_identifier_text(key_idx);
        let defaulted_name = self.get_temp_var_name();
        self.write(", ");
        self.write(&defaulted_name);
        self.write(" = ");
        self.write(&value_name);
        self.write(" === void 0 ? ");
        self.emit_expression(elem.initializer);
        self.write(" : ");
        self.write(&value_name);

        let child_count = self.binding_pattern_non_rest_count(pattern_node);
        match pattern_node.kind {
            syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                if is_array_shape {
                    self.emit_es5_destructuring_pattern_idx(pattern_name, &defaulted_name);
                } else if child_count > 0 {
                    let read_name = self.get_temp_var_name();
                    self.write(", ");
                    self.write(&read_name);
                    self.write(" = __read(");
                    self.write(&defaulted_name);
                    self.write(", ");
                    self.write(&child_count.to_string());
                    self.write(")");
                    self.emit_es5_destructuring_pattern_idx(pattern_name, &read_name);
                } else {
                    self.emit_es5_destructuring_pattern_idx(pattern_name, &defaulted_name);
                }
            }
            syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if !self
                    .emit_single_object_binding_inline_nested_object(pattern_name, &defaulted_name)
                {
                    self.emit_es5_destructuring_pattern_idx(pattern_name, &defaulted_name);
                }
            }
            _ => {
                self.emit_es5_destructuring_pattern_idx(pattern_name, &defaulted_name);
            }
        }
        true
    }

    pub(super) fn emit_single_object_binding_inline_nested_object(
        &mut self,
        pattern_node: NodeIndex,
        source_name: &str,
    ) -> bool {
        let Some(pattern_ast) = self.arena.get(pattern_node) else {
            return false;
        };
        let Some(pattern) = self.arena.get_binding_pattern(pattern_ast) else {
            return false;
        };
        if pattern.elements.nodes.is_empty() {
            return false;
        }

        let mut child = NodeIndex::NONE;
        let mut non_rest = 0;
        for &elem_idx in &pattern.elements.nodes {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.arena.get(elem_idx) else {
                return false;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                return false;
            };
            if elem.dot_dot_dot_token {
                return false;
            }
            child = elem_idx;
            non_rest += 1;
            if non_rest > 1 {
                return false;
            }
        }
        if child.is_none() {
            return false;
        }

        let Some(child_node) = self.arena.get(child) else {
            return false;
        };
        let Some(child_elem) = self.arena.get_binding_element(child_node) else {
            return false;
        };
        if self.is_binding_pattern(child_elem.name) || !self.has_identifier_text(child_elem.name) {
            return false;
        }

        let key_idx = if child_elem.property_name.is_some() {
            child_elem.property_name
        } else {
            child_elem.name
        };
        let Some(key_node) = self.arena.get(key_idx) else {
            return false;
        };
        if key_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        self.write(", ");
        self.write_identifier_text(child_elem.name);
        self.write(" = ");
        self.write(source_name);
        self.write(".");
        self.write_identifier_text(key_idx);
        if child_elem.initializer.is_some() {
            self.write(" === void 0 ? ");
            self.emit_expression(child_elem.initializer);
            self.write(" : ");
            self.write(source_name);
            self.write(".");
            self.write_identifier_text(key_idx);
        }
        true
    }

    pub(super) fn emit_single_object_binding_inline_nested_object_node(
        &mut self,
        pattern_node: NodeIndex,
        initializer: NodeIndex,
        key_idx: NodeIndex,
        allow_expression_emit: bool,
    ) -> bool {
        let Some(pattern_ast) = self.arena.get(pattern_node) else {
            return false;
        };
        let Some(pattern) = self.arena.get_binding_pattern(pattern_ast) else {
            return false;
        };
        if pattern.elements.nodes.is_empty() {
            return false;
        }

        let mut child = NodeIndex::NONE;
        let mut non_rest = 0;
        for &elem_idx in &pattern.elements.nodes {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.arena.get(elem_idx) else {
                return false;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                return false;
            };
            if elem.dot_dot_dot_token {
                return false;
            }
            child = elem_idx;
            non_rest += 1;
            if non_rest > 1 {
                return false;
            }
        }
        if child.is_none() {
            return false;
        }

        let Some(child_node) = self.arena.get(child) else {
            return false;
        };
        let Some(child_elem) = self.arena.get_binding_element(child_node) else {
            return false;
        };
        if self.is_binding_pattern(child_elem.name) || !self.has_identifier_text(child_elem.name) {
            return false;
        }

        let child_key_idx = if child_elem.property_name.is_some() {
            child_elem.property_name
        } else {
            child_elem.name
        };
        let Some(child_key_node) = self.arena.get(child_key_idx) else {
            return false;
        };
        if child_key_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        let value_name = self.get_temp_var_name();
        self.write(&value_name);
        self.write(" = ");
        if allow_expression_emit {
            self.emit(initializer);
        } else {
            self.emit_expression(initializer);
        }
        self.write(".");
        self.write_identifier_text(key_idx);
        self.write(".");
        self.write_identifier_text(child_key_idx);

        if child_elem.initializer.is_none() {
            self.write(", ");
            self.write_identifier_text(child_elem.name);
            self.write(" = ");
            self.write(&value_name);
        } else {
            self.write(", ");
            self.write_identifier_text(child_elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(child_elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }
        true
    }

    pub(super) fn emit_es5_destructuring_from_value(
        &mut self,
        pattern_idx: NodeIndex,
        result_name: &str,
        first: &mut bool,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };

        let temp_name = self.get_temp_var_name();

        if !*first {
            self.write(", ");
        }
        *first = false;
        self.write(&temp_name);
        self.write(" = ");
        self.write(result_name);
        self.write(".value");

        self.emit_es5_destructuring_pattern(pattern_node, &temp_name);
    }

    /// Emit ES5 destructuring using __read helper for downlevelIteration
    /// Transforms: `[a = 0, b = 1] = expr`
    /// Into: `_d = __read(expr, 2), _e = _d[0], a = _e === void 0 ? 0 : _e, _f = _d[1], b = _f === void 0 ? 1 : _f`
    pub(super) fn emit_es5_destructuring_with_read_node(
        &mut self,
        pattern_idx: NodeIndex,
        source_expr: NodeIndex,
        _first: &mut bool,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        if std::env::var_os("TSZ_DEBUG_EMIT").is_some() {
            debug!("emit_es5_destructuring_with_read_node entered");
        }

        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };

        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            let temp_name = self.get_temp_var_name();
            self.write(&temp_name);
            self.write(" = ");
            self.emit(source_expr);
            self.emit_es5_destructuring_pattern(pattern_node, &temp_name);
            return;
        }

        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        let element_count = pattern
            .elements
            .nodes
            .iter()
            .filter(|&&elem_idx| {
                self.arena
                    .get(elem_idx)
                    .and_then(|n| self.arena.get_binding_element(n))
                    .is_some_and(|e| !e.dot_dot_dot_token)
            })
            .count();

        let read_temp = self.get_temp_var_name();
        self.write(&read_temp);
        self.write(" = __read(");
        self.destructuring_read_depth += 1;
        self.emit(source_expr);
        self.destructuring_read_depth -= 1;
        if element_count > 0 {
            self.write(", ");
            self.write(&element_count.to_string());
        }
        self.write(")");

        for (index, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };
            if elem.name.is_none() {
                continue;
            }

            if elem.dot_dot_dot_token {
                if self.is_binding_pattern(elem.name) {
                    let rest_temp = self.get_temp_var_name();
                    self.write(", ");
                    self.write(&rest_temp);
                    self.write(" = ");
                    self.write(&read_temp);
                    self.write(".slice(");
                    self.write(&index.to_string());
                    self.write(")");
                    self.emit_es5_destructuring_pattern_idx(elem.name, &rest_temp);
                } else if self.has_identifier_text(elem.name) {
                    self.write(", ");
                    self.emit_expression(elem.name);
                    self.write(" = ");
                    self.write(&read_temp);
                    self.write(".slice(");
                    self.write(&index.to_string());
                    self.write(")");
                }
                continue;
            }

            let unwrapped_name = self.unwrap_parenthesized_binding_pattern(elem.name);
            #[cfg(not(target_arch = "wasm32"))]
            if std::env::var_os("TSZ_DEBUG_EMIT").is_some() {
                let elem_kind = self.arena.get(elem.name).map(|n| n.kind).unwrap_or(0);
                debug!(
                    "downlevel-bp-element index={} elem_name={:?} unwrapped={:?} kind={}",
                    index, elem.name, unwrapped_name, elem_kind
                );
                debug!(
                    "downlevel-bp-kind-bytes: elem={} unwrapped={}",
                    self.arena.get(unwrapped_name).map(|n| n.kind).unwrap_or(0),
                    SyntaxKind::Identifier as u16
                );
            }
            if let Some(name_node) = self.arena.get(unwrapped_name) {
                if name_node.kind == SyntaxKind::Identifier as u16 {
                    let elem_source = format!("{read_temp}[{index}]");
                    if elem.initializer.is_none() {
                        self.write(", ");
                        self.emit_expression(elem.name);
                        self.write(" = ");
                        self.write(&elem_source);
                    } else {
                        let value_name = self.get_temp_var_name();
                        self.write(", ");
                        self.write(&value_name);
                        self.write(" = ");
                        self.write(&elem_source);
                        self.write(", ");
                        self.emit_expression(elem.name);
                        self.write(" = ");
                        self.write(&value_name);
                        self.write(" === void 0 ? ");
                        self.emit_expression(elem.initializer);
                        self.write(" : ");
                        self.write(&value_name);
                    }
                } else if self.is_binding_pattern(unwrapped_name) {
                    let Some(unwrapped_node) = self.arena.get(unwrapped_name) else {
                        continue;
                    };
                    let elem_source = format!("{read_temp}[{index}]");
                    if unwrapped_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                        #[cfg(not(target_arch = "wasm32"))]
                        if std::env::var_os("TSZ_DEBUG_EMIT").is_some() {
                            debug!(
                                "downlevel-nested-array index={} unwrapped={} source={}",
                                index, unwrapped_name.0, elem_source
                            );
                        }
                        self.write(", ");
                        let source_expr = if elem.initializer.is_none() {
                            elem_source
                        } else {
                            let defaulted = self.get_temp_var_name();
                            self.write(&defaulted);
                            self.write(" = ");
                            self.write(&elem_source);
                            self.write(" === void 0 ? ");
                            self.emit_expression(elem.initializer);
                            self.write(" : ");
                            self.write(&elem_source);
                            defaulted
                        };

                        let element_count = self.binding_pattern_non_rest_count(unwrapped_node);
                        let nested_temp = self.get_temp_var_name();
                        self.write(&nested_temp);
                        self.write(" = __read(");
                        self.write(&source_expr);
                        if element_count > 0 {
                            self.write(", ");
                            self.write(&element_count.to_string());
                        }
                        self.write(")");
                        self.emit_es5_destructuring_with_read_tail(unwrapped_name, &nested_temp);
                    } else {
                        let pattern_temp = self.get_temp_var_name();
                        self.write(", ");
                        self.write(&pattern_temp);
                        self.write(" = ");
                        self.write(&elem_source);

                        let target_temp = if elem.initializer.is_some() {
                            let defaulted = self.get_temp_var_name();
                            self.write(", ");
                            self.write(&defaulted);
                            self.write(" = ");
                            self.write(&pattern_temp);
                            self.write(" === void 0 ? ");
                            self.emit_expression(elem.initializer);
                            self.write(" : ");
                            self.write(&pattern_temp);
                            defaulted
                        } else {
                            pattern_temp
                        };

                        self.emit_es5_destructuring_pattern_idx(unwrapped_name, &target_temp);
                    }
                } else {
                    // no-op
                }
            }
        }
    }

    pub(super) fn emit_es5_destructuring_with_read_tail(
        &mut self,
        pattern_idx: NodeIndex,
        source_expr: &str,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return;
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        for (index, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };

            if elem.name.is_none() || elem.dot_dot_dot_token {
                continue;
            }

            let elem_source = format!("{source_expr}[{index}]");
            let Some(elem_node) = self.arena.get(elem.name) else {
                continue;
            };

            if elem_node.kind == SyntaxKind::Identifier as u16 {
                self.write(", ");
                self.emit(elem.name);
                self.write(" = ");
                if elem.initializer.is_some() {
                    let value_name = self.get_temp_var_name();
                    self.write(&value_name);
                    self.write(" = ");
                    self.write(&elem_source);
                    self.write(", ");
                    self.emit(elem.name);
                    self.write(" = ");
                    self.write(&value_name);
                    self.write(" === void 0 ? ");
                    self.emit_expression(elem.initializer);
                    self.write(" : ");
                    self.write(&value_name);
                } else {
                    self.write(&elem_source);
                }
            } else if self.is_binding_pattern(elem.name) {
                let nested_name = self.unwrap_parenthesized_binding_pattern(elem.name);
                let Some(nested_node) = self.arena.get(nested_name) else {
                    continue;
                };

                if nested_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                    let nested_count = self.binding_pattern_non_rest_count(nested_node);
                    let nested_temp = self.get_temp_var_name();
                    self.write(", ");
                    self.write(&nested_temp);
                    self.write(" = __read(");
                    self.write(&elem_source);
                    if nested_count > 0 {
                        self.write(", ");
                        self.write(&nested_count.to_string());
                    }
                    self.write(")");
                    self.emit_es5_destructuring_with_read_tail(nested_name, &nested_temp);
                } else {
                    let pattern_temp = self.get_temp_var_name();
                    self.write(", ");
                    self.write(&pattern_temp);
                    self.write(" = ");
                    self.write(&elem_source);

                    let target_temp = if elem.initializer.is_some() {
                        let defaulted = self.get_temp_var_name();
                        self.write(", ");
                        self.write(&defaulted);
                        self.write(" = ");
                        self.write(&pattern_temp);
                        self.write(" === void 0 ? ");
                        self.emit_expression(elem.initializer);
                        self.write(" : ");
                        self.write(&pattern_temp);
                        defaulted
                    } else {
                        pattern_temp
                    };
                    self.emit_es5_destructuring_pattern_idx(nested_name, &target_temp);
                }
            }
        }
    }

    pub(super) fn emit_es5_destructuring_with_read(
        &mut self,
        pattern_idx: NodeIndex,
        source_expr: &str,
        _first: &mut bool,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };

        // Only handle array binding patterns for now
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            let temp_name = self.get_temp_var_name();
            if !*_first {
                self.write(", ");
            }
            *_first = false;
            self.write(&temp_name);
            self.write(" = ");
            self.write(source_expr);
            self.emit_es5_destructuring_pattern(pattern_node, &temp_name);
            return;
        }

        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        // Count non-rest elements to pass to __read
        let element_count = pattern
            .elements
            .nodes
            .iter()
            .filter(|&&elem_idx| {
                self.arena
                    .get(elem_idx)
                    .and_then(|n| self.arena.get_binding_element(n))
                    .is_some_and(|e| !e.dot_dot_dot_token)
            })
            .count();

        // Emit: _d = __read(expr, N)
        let read_temp = self.get_temp_var_name();
        // Note: caller has already handled the comma and set first=false
        self.write(&read_temp);
        self.write(" = __read(");
        self.write(source_expr);
        self.write(", ");
        self.write(&element_count.to_string());
        self.write(")");

        // Now emit each element binding
        for (index, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };

            // Skip elided elements
            if elem.name.is_none() {
                continue;
            }

            // Handle rest elements (not applicable for for-of with __read, but included for completeness)
            if elem.dot_dot_dot_token {
                continue;
            }

            // Get element from array: _e = _d[0]
            let elem_temp = self.get_temp_var_name();
            self.write(", ");
            self.write(&elem_temp);
            self.write(" = ");
            self.write(&read_temp);
            self.write("[");
            self.write(&index.to_string());
            self.write("]");

            // If there's a default value, emit: a = _e === void 0 ? 0 : _e
            // If no default, emit: a = _e
            if let Some(name_node) = self.arena.get(elem.name) {
                if name_node.kind == SyntaxKind::Identifier as u16 {
                    self.write(", ");
                    self.emit(elem.name);
                    self.write(" = ");
                    if elem.initializer.is_some() {
                        self.write(&elem_temp);
                        self.write(" === void 0 ? ");
                        self.emit_expression(elem.initializer);
                        self.write(" : ");
                        self.write(&elem_temp);
                    } else {
                        self.write(&elem_temp);
                    }
                } else if self.is_binding_pattern(elem.name) {
                    // Nested binding pattern - handle recursively
                    let nested_temp = if elem.initializer.is_some() {
                        let defaulted = self.get_temp_var_name();
                        self.write(", ");
                        self.write(&defaulted);
                        self.write(" = ");
                        self.write(&elem_temp);
                        self.write(" === void 0 ? ");
                        self.emit_expression(elem.initializer);
                        self.write(" : ");
                        self.write(&elem_temp);
                        defaulted
                    } else {
                        elem_temp
                    };
                    let nested_node = self.unwrap_parenthesized_binding_pattern(elem.name);
                    if let Some(nested_pattern_node) = self.arena.get(nested_node)
                        && nested_pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    {
                        let mut first = false;
                        self.emit_es5_destructuring_with_read(
                            nested_node,
                            &nested_temp,
                            &mut first,
                        );
                    } else {
                        self.emit_es5_destructuring_pattern_idx(elem.name, &nested_temp);
                    }
                }
            }
        }
    }

    // Binding element patterns + param bindings → es5_bindings_patterns.rs
    // For-of array + assignment destructuring → es5_bindings_assignment.rs
}
