use super::{ParamTransformPlan, Printer};
use tracing::debug;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{BindingElementData, BindingPatternData, ForInOfData, Node};
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

            if self.is_binding_pattern(decl.name) && !decl.initializer.is_none() {
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
    fn count_effective_bindings(&self, pattern_node: &Node) -> (usize, bool) {
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
    fn emit_single_array_binding_inline(
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
            .find(|(i, n)| *i == binding_array_index && !n.is_none())
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
    fn emit_rest_only_array_inline(
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
    fn try_emit_single_inline_from_expr(
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
    fn try_emit_rest_only_from_expr(
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

    fn unwrap_parenthesized_binding_pattern(&self, mut pattern_idx: NodeIndex) -> NodeIndex {
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

    fn is_binding_pattern_array_shape(&self, pattern_node: &Node) -> bool {
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

    fn binding_pattern_non_rest_count(&self, pattern_node: &Node) -> usize {
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

    fn emit_es5_destructuring_fallback(
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
    fn emit_single_object_binding_inline_simple(
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
            .filter(|n| !n.is_none());
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

        let key_idx = if !elem.property_name.is_none() {
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

    fn emit_single_object_binding_inline_nested(
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

        let key_idx = if !elem.property_name.is_none() {
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

    fn emit_single_object_binding_inline_nested_object(
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

        let key_idx = if !child_elem.property_name.is_none() {
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
        if !child_elem.initializer.is_none() {
            self.write(" === void 0 ? ");
            self.emit_expression(child_elem.initializer);
            self.write(" : ");
            self.write(source_name);
            self.write(".");
            self.write_identifier_text(key_idx);
        }
        true
    }

    fn emit_single_object_binding_inline_nested_object_node(
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

        let child_key_idx = if !child_elem.property_name.is_none() {
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

    fn emit_es5_destructuring_from_value(
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
    fn emit_es5_destructuring_with_read_node(
        &mut self,
        pattern_idx: NodeIndex,
        source_expr: NodeIndex,
        _first: &mut bool,
    ) {
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
                    let elem_source = format!("{}[{}]", read_temp, index);
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
                    let elem_source = format!("{}[{}]", read_temp, index);
                    if unwrapped_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
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

                        let target_temp = if !elem.initializer.is_none() {
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

    fn emit_es5_destructuring_with_read_tail(&mut self, pattern_idx: NodeIndex, source_expr: &str) {
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

            let elem_source = format!("{}[{}]", source_expr, index);
            let Some(elem_node) = self.arena.get(elem.name) else {
                continue;
            };

            if elem_node.kind == SyntaxKind::Identifier as u16 {
                self.write(", ");
                self.emit(elem.name);
                self.write(" = ");
                if !elem.initializer.is_none() {
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

                    let target_temp = if !elem.initializer.is_none() {
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

    fn emit_es5_destructuring_with_read(
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
                    if !elem.initializer.is_none() {
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
                    let nested_temp = if !elem.initializer.is_none() {
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

    fn get_binding_element_property_key(&self, elem: &BindingElementData) -> Option<NodeIndex> {
        let key_idx = if !elem.property_name.is_none() {
            elem.property_name
        } else {
            elem.name
        };
        let key_node = self.arena.get(key_idx)?;
        match key_node.kind {
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                || k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16 =>
            {
                Some(key_idx)
            }
            _ => None,
        }
    }

    /// Emit a single binding element for ES5 object destructuring
    fn emit_es5_binding_element(&mut self, elem_idx: NodeIndex, temp_name: &str) {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };
        if elem.dot_dot_dot_token {
            return;
        }

        let Some(key_idx) = self.get_binding_element_property_key(elem) else {
            return;
        };

        // Check if key is computed and save to temp if needed
        let computed_key_temp = self.emit_computed_key_temp_if_needed(key_idx);

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5_with_computed(
                key_idx,
                temp_name,
                computed_key_temp.as_deref(),
            );

            // When there's a default, create a NEW temp for the defaulted value
            let pattern_temp = if !elem.initializer.is_none() {
                let defaulted_name = self.get_temp_var_name();
                self.write(", ");
                self.write(&defaulted_name);
                self.write(" = ");
                self.write(&value_name);
                self.write(" === void 0 ? ");
                self.emit_expression(elem.initializer);
                self.write(" : ");
                self.write(&value_name);
                defaulted_name
            } else {
                value_name
            };

            self.emit_es5_destructuring_pattern_idx(elem.name, &pattern_temp);
            return;
        }

        if !self.has_identifier_text(elem.name) {
            return;
        }

        if elem.initializer.is_none() {
            // Emit: , bindingName = temp.propName
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.emit_assignment_target_es5_with_computed(
                key_idx,
                temp_name,
                computed_key_temp.as_deref(),
            );
        } else {
            let value_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5_with_computed(
                key_idx,
                temp_name,
                computed_key_temp.as_deref(),
            );
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }
    }

    /// If key_idx is a computed property, emit a temp variable assignment and return the temp name
    /// Returns None if not computed
    fn emit_computed_key_temp_if_needed(&mut self, key_idx: NodeIndex) -> Option<String> {
        let key_node = self.arena.get(key_idx)?;

        if key_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(key_node)
        {
            let temp_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&temp_name);
            self.write(" = ");
            self.emit(computed.expression);
            return Some(temp_name);
        }

        None
    }

    /// Emit a single binding element for ES5 array destructuring
    fn emit_es5_array_binding_element(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        index: usize,
    ) {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };

        if elem.dot_dot_dot_token {
            self.emit_es5_array_rest_element(elem.name, temp_name, index);
            return;
        }

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");

            // When there's a default, create a NEW temp for the defaulted value
            let pattern_temp = if !elem.initializer.is_none() {
                let defaulted_name = self.get_temp_var_name();
                self.write(", ");
                self.write(&defaulted_name);
                self.write(" = ");
                self.write(&value_name);
                self.write(" === void 0 ? ");
                self.emit_expression(elem.initializer);
                self.write(" : ");
                self.write(&value_name);
                defaulted_name
            } else {
                value_name
            };

            self.emit_es5_destructuring_pattern_idx(elem.name, &pattern_temp);
            return;
        }

        if !self.has_identifier_text(elem.name) {
            return;
        }

        if elem.initializer.is_none() {
            // Emit: , bindingName = temp[index]
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
        } else {
            let value_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }
    }

    /// Like emit_es5_binding_element but with first flag for separator control
    fn emit_es5_binding_element_direct(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        first: &mut bool,
    ) {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };
        if elem.dot_dot_dot_token {
            return;
        }

        let Some(key_idx) = self.get_binding_element_property_key(elem) else {
            return;
        };

        // Check if key is computed and save to temp if needed
        let computed_key_temp = self.emit_computed_key_temp_for_direct(key_idx, first);

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5_with_computed(
                key_idx,
                temp_name,
                computed_key_temp.as_deref(),
            );

            // When there's a default, create a NEW temp for the defaulted value
            let pattern_temp = if !elem.initializer.is_none() {
                let defaulted_name = self.get_temp_var_name();
                self.write(", ");
                self.write(&defaulted_name);
                self.write(" = ");
                self.write(&value_name);
                self.write(" === void 0 ? ");
                self.emit_expression(elem.initializer);
                self.write(" : ");
                self.write(&value_name);
                defaulted_name
            } else {
                value_name
            };

            self.emit_es5_destructuring_pattern_idx(elem.name, &pattern_temp);
            return;
        }

        if !self.has_identifier_text(elem.name) {
            return;
        }

        if elem.initializer.is_none() {
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.emit_assignment_target_es5_with_computed(
                key_idx,
                temp_name,
                computed_key_temp.as_deref(),
            );
        } else {
            let value_name = self.get_temp_var_name();
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5_with_computed(
                key_idx,
                temp_name,
                computed_key_temp.as_deref(),
            );
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }
    }

    /// Similar to emit_computed_key_temp_if_needed but handles the first flag for direct destructuring
    fn emit_computed_key_temp_for_direct(
        &mut self,
        key_idx: NodeIndex,
        first: &mut bool,
    ) -> Option<String> {
        let key_node = self.arena.get(key_idx)?;

        if key_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(key_node)
        {
            let temp_name = self.get_temp_var_name();
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write(&temp_name);
            self.write(" = ");
            self.emit(computed.expression);
            return Some(temp_name);
        }

        None
    }

    /// Like emit_es5_array_binding_element but with first flag for separator control
    fn emit_es5_array_binding_element_direct(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        index: usize,
        first: &mut bool,
    ) {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };

        if elem.dot_dot_dot_token {
            // Rest element: , restName = temp.slice(index)
            if !self.has_identifier_text(elem.name) && !self.is_binding_pattern(elem.name) {
                return;
            }
            if !*first {
                self.write(", ");
            }
            *first = false;
            if self.is_binding_pattern(elem.name) {
                let value_name = self.get_temp_var_name();
                self.write(&value_name);
                self.write(" = ");
                self.write(temp_name);
                self.write(".slice(");
                self.write_usize(index);
                self.write(")");
                self.emit_es5_destructuring_pattern_idx(elem.name, &value_name);
            } else {
                self.write_identifier_text(elem.name);
                self.write(" = ");
                self.write(temp_name);
                self.write(".slice(");
                self.write_usize(index);
                self.write(")");
            }
            return;
        }

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");

            // When there's a default, create a NEW temp for the defaulted value
            let pattern_temp = if !elem.initializer.is_none() {
                let defaulted_name = self.get_temp_var_name();
                self.write(", ");
                self.write(&defaulted_name);
                self.write(" = ");
                self.write(&value_name);
                self.write(" === void 0 ? ");
                self.emit_expression(elem.initializer);
                self.write(" : ");
                self.write(&value_name);
                defaulted_name
            } else {
                value_name
            };

            self.emit_es5_destructuring_pattern_idx(elem.name, &pattern_temp);
            return;
        }

        if !self.has_identifier_text(elem.name) {
            return;
        }

        if elem.initializer.is_none() {
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
        } else {
            let value_name = self.get_temp_var_name();
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }
    }

    fn emit_es5_destructuring_pattern(&mut self, pattern_node: &Node, temp_name: &str) {
        if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
                return;
            };
            let rest_props = self.collect_object_rest_props(pattern);
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
                    self.emit_es5_object_rest_element(elem, &rest_props, temp_name);
                } else {
                    self.emit_es5_binding_element(elem_idx, temp_name);
                }
            }
        } else if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            && let Some(pattern) = self.arena.get_binding_pattern(pattern_node)
        {
            for (i, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                self.emit_es5_array_binding_element(elem_idx, temp_name, i);
            }
        }
    }

    /// Like emit_es5_destructuring_pattern but handles the `first` flag for the first
    /// non-omitted element, allowing it to be emitted without a `, ` prefix.
    /// Used when the initializer is a simple identifier and no temp variable is needed.
    fn emit_es5_destructuring_pattern_direct(
        &mut self,
        pattern_node: &Node,
        ident_name: &str,
        first: &mut bool,
    ) {
        if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
                return;
            };
            let rest_props = self.collect_object_rest_props(pattern);
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
                    if !*first {
                        // rest element always needs separator
                    }
                    self.emit_es5_object_rest_element(elem, &rest_props, ident_name);
                    *first = false;
                } else {
                    self.emit_es5_binding_element_direct(elem_idx, ident_name, first);
                }
            }
        } else if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            && let Some(pattern) = self.arena.get_binding_pattern(pattern_node)
        {
            for (i, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                self.emit_es5_array_binding_element_direct(elem_idx, ident_name, i, first);
            }
        }
    }

    pub(super) fn emit_param_prologue(&mut self, transforms: &ParamTransformPlan) {
        for param in &transforms.params {
            if let Some(initializer) = param.initializer {
                if let Some(pattern) = param.pattern {
                    // Has both default and binding pattern: use ternary in a single var statement.
                    // TypeScript: var _b = _a === void 0 ? default : _a, _c = _b[1], ...
                    let mut started = false;
                    let temp = self.get_temp_var_name();
                    self.emit_param_assignment_prefix(&mut started);
                    self.write(&temp);
                    self.write(" = ");
                    self.write(&param.name);
                    self.write(" === void 0 ? ");
                    self.emit_expression(initializer);
                    self.write(" : ");
                    self.write(&param.name);

                    self.emit_param_binding_assignments(pattern, &temp, &mut started);
                    if started {
                        self.write(";");
                        self.write_line();
                    }
                } else {
                    // Only default, no pattern: use if statement
                    self.emit_param_default_assignment(&param.name, initializer);
                }
            } else if let Some(pattern) = param.pattern {
                let mut started = false;
                self.emit_param_binding_assignments(pattern, &param.name, &mut started);
                if started {
                    self.write(";");
                    self.write_line();
                }
            }
        }

        if let Some(rest) = &transforms.rest {
            if !rest.name.is_empty() {
                self.write("var ");
                self.write(&rest.name);
                self.write(" = [];");
                self.write_line();

                let iter_name = self.get_temp_var_name();
                self.write("for (var ");
                self.write(&iter_name);
                self.write(" = ");
                self.write_usize(rest.index);
                self.write("; ");
                self.write(&iter_name);
                self.write(" < arguments.length; ");
                self.write(&iter_name);
                self.write("++) ");
                self.write(&rest.name);
                self.write("[");
                self.write(&iter_name);
                self.write(" - ");
                self.write_usize(rest.index);
                self.write("] = arguments[");
                self.write(&iter_name);
                self.write("];");
                self.write_line();
            }

            if let Some(pattern) = rest.pattern {
                let mut started = false;
                self.emit_param_binding_assignments(pattern, &rest.name, &mut started);
                if started {
                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    fn emit_param_default_assignment(&mut self, name: &str, initializer: NodeIndex) {
        if name.is_empty() {
            return;
        }
        self.write("if (");
        self.write(name);
        self.write(" === void 0) { ");
        self.write(name);
        self.write(" = ");
        self.emit_expression(initializer);
        self.write("; }");
        self.write_line();
    }

    fn emit_param_binding_assignments(
        &mut self,
        pattern_idx: NodeIndex,
        temp_name: &str,
        started: &mut bool,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };

        match pattern_node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(pattern_node) {
                    let rest_props = self.collect_object_rest_props(pattern);
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
                            self.emit_param_object_rest_element(
                                elem,
                                &rest_props,
                                temp_name,
                                started,
                            );
                        } else {
                            self.emit_param_object_binding_element(elem_idx, temp_name, started);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(pattern_node) {
                    for (i, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                        self.emit_param_array_binding_element(elem_idx, temp_name, i, started);
                    }
                }
            }
            _ => {}
        }
    }

    fn emit_param_object_binding_element(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        started: &mut bool,
    ) {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };

        if elem.dot_dot_dot_token {
            return;
        }

        let Some(key_idx) = self.get_binding_element_property_key(elem) else {
            return;
        };

        // Check if key is computed and save to temp if needed
        let computed_key_temp = self.emit_computed_key_temp_for_param(key_idx, started);

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            self.emit_param_assignment_prefix(started);
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5_with_computed(
                key_idx,
                temp_name,
                computed_key_temp.as_deref(),
            );

            if !elem.initializer.is_none() {
                self.write(", ");
                self.write(&value_name);
                self.write(" = ");
                self.write(&value_name);
                self.write(" === void 0 ? ");
                self.emit_expression(elem.initializer);
                self.write(" : ");
                self.write(&value_name);
            }

            self.emit_param_binding_assignments(elem.name, &value_name, started);
            return;
        }

        if !self.has_identifier_text(elem.name) {
            return;
        }

        self.emit_param_assignment_prefix(started);
        if !elem.initializer.is_none() {
            let value_name = self.get_temp_var_name();
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5_with_computed(
                key_idx,
                temp_name,
                computed_key_temp.as_deref(),
            );
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        } else {
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.emit_assignment_target_es5_with_computed(
                key_idx,
                temp_name,
                computed_key_temp.as_deref(),
            );
        }
    }

    /// Similar to emit_computed_key_temp_if_needed but handles started flag for param destructuring
    fn emit_computed_key_temp_for_param(
        &mut self,
        key_idx: NodeIndex,
        started: &mut bool,
    ) -> Option<String> {
        let key_node = self.arena.get(key_idx)?;

        if key_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(key_node)
        {
            let temp_name = self.get_temp_var_name();
            self.emit_param_assignment_prefix(started);
            self.write(&temp_name);
            self.write(" = ");
            self.emit(computed.expression);
            return Some(temp_name);
        }

        None
    }

    fn emit_param_array_binding_element(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        index: usize,
        started: &mut bool,
    ) {
        if elem_idx.is_none() {
            return;
        }
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };

        if elem.dot_dot_dot_token {
            self.emit_param_array_rest_element(elem.name, temp_name, index, started);
            return;
        }

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            self.emit_param_assignment_prefix(started);
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");

            let source_name = if !elem.initializer.is_none() {
                // Allocate a NEW temp for the defaulted value
                let default_name = self.get_temp_var_name();
                self.write(", ");
                self.write(&default_name);
                self.write(" = ");
                self.write(&value_name);
                self.write(" === void 0 ? ");
                self.emit_expression(elem.initializer);
                self.write(" : ");
                self.write(&value_name);
                default_name
            } else {
                value_name
            };

            self.emit_param_binding_assignments(elem.name, &source_name, started);
            return;
        }

        if !self.has_identifier_text(elem.name) {
            return;
        }

        self.emit_param_assignment_prefix(started);
        if !elem.initializer.is_none() {
            let value_name = self.get_temp_var_name();
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        } else {
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
        }
    }

    fn emit_param_object_rest_element(
        &mut self,
        elem: &BindingElementData,
        rest_props: &[NodeIndex],
        temp_name: &str,
        started: &mut bool,
    ) {
        let rest_target = elem.name;
        let is_pattern = self.is_binding_pattern(rest_target);
        let rest_temp = is_pattern.then(|| self.get_temp_var_name());

        self.emit_param_assignment_prefix(started);
        if let Some(ref name) = rest_temp {
            self.write(name);
        } else {
            self.emit(rest_target);
        }
        self.write(" = __rest(");
        self.write(temp_name);
        self.write(", ");
        self.emit_rest_exclude_list(rest_props);
        self.write(")");

        if let Some(ref name) = rest_temp {
            self.emit_param_binding_assignments(rest_target, name, started);
        }
    }

    fn emit_param_array_rest_element(
        &mut self,
        rest_target: NodeIndex,
        temp_name: &str,
        index: usize,
        started: &mut bool,
    ) {
        let is_pattern = self.is_binding_pattern(rest_target);
        let rest_temp = is_pattern.then(|| self.get_temp_var_name());

        self.emit_param_assignment_prefix(started);
        if let Some(ref name) = rest_temp {
            self.write(name);
        } else {
            if !self.has_identifier_text(rest_target) {
                return;
            }
            self.write_identifier_text(rest_target);
        }
        self.write(" = ");
        self.write(temp_name);
        self.write(".slice(");
        self.write_usize(index);
        self.write(")");

        if let Some(ref name) = rest_temp {
            self.emit_param_binding_assignments(rest_target, name, started);
        }
    }

    fn emit_param_assignment_prefix(&mut self, started: &mut bool) {
        if !*started {
            self.write("var ");
            *started = true;
        } else {
            self.write(", ");
        }
    }

    fn emit_es5_object_rest_element(
        &mut self,
        elem: &BindingElementData,
        rest_props: &[NodeIndex],
        temp_name: &str,
    ) {
        let rest_target = elem.name;
        let is_pattern = self.is_binding_pattern(rest_target);
        let rest_temp = is_pattern.then(|| self.get_temp_var_name());

        self.write(", ");
        if let Some(ref name) = rest_temp {
            self.write(name);
        } else {
            self.emit(rest_target);
        }
        self.write(" = __rest(");
        self.write(temp_name);
        self.write(", ");
        self.emit_rest_exclude_list(rest_props);
        self.write(")");

        if let Some(ref name) = rest_temp {
            self.emit_es5_destructuring_pattern_idx(rest_target, name);
        }
    }

    fn emit_es5_array_rest_element(
        &mut self,
        rest_target: NodeIndex,
        temp_name: &str,
        index: usize,
    ) {
        let is_pattern = self.is_binding_pattern(rest_target);
        let rest_temp = is_pattern.then(|| self.get_temp_var_name());

        self.write(", ");
        if let Some(ref name) = rest_temp {
            self.write(name);
        } else {
            if !self.has_identifier_text(rest_target) {
                return;
            }
            self.write_identifier_text(rest_target);
        }
        self.write(" = ");
        self.write(temp_name);
        self.write(".slice(");
        self.write_usize(index);
        self.write(")");

        if let Some(ref name) = rest_temp {
            self.emit_es5_destructuring_pattern_idx(rest_target, name);
        }
    }

    fn emit_es5_destructuring_pattern_idx(&mut self, pattern_idx: NodeIndex, temp_name: &str) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        self.emit_es5_destructuring_pattern(pattern_node, temp_name);
    }

    fn collect_object_rest_props(&self, pattern: &BindingPatternData) -> Vec<NodeIndex> {
        let mut props = Vec::new();
        for &elem_idx in &pattern.elements.nodes {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };
            if elem.dot_dot_dot_token {
                continue;
            }
            let key_idx = if !elem.property_name.is_none() {
                elem.property_name
            } else {
                elem.name
            };
            if let Some(key_node) = self.arena.get(key_idx)
                && (key_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || key_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
            {
                continue;
            }
            props.push(key_idx);
        }
        props
    }

    fn emit_rest_exclude_list(&mut self, props: &[NodeIndex]) {
        self.write("[");
        let mut first = true;
        for &prop_idx in props {
            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_rest_property_key(prop_idx);
        }
        self.write("]");
    }

    fn emit_rest_property_key(&mut self, key_idx: NodeIndex) {
        let Some(key_node) = self.arena.get(key_idx) else {
            return;
        };

        if key_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(key_node) {
                self.emit_expression(computed.expression);
            }
            return;
        }

        if let Some(ident) = self.arena.get_identifier(key_node) {
            self.write("\"");
            self.write(&ident.escaped_text);
            self.write("\"");
            return;
        }

        if let Some(lit) = self.arena.get_literal(key_node) {
            self.write("\"");
            self.write(&lit.text);
            self.write("\"");
            return;
        }

        self.emit_expression(key_idx);
    }

    pub(super) fn emit_for_of_statement_es5(
        &mut self,
        for_of_idx: NodeIndex,
        for_in_of: &ForInOfData,
    ) {
        // Check if downlevelIteration is enabled
        if self.ctx.options.downlevel_iteration {
            self.emit_for_of_statement_es5_iterator(for_of_idx, for_in_of);
        } else {
            self.emit_for_of_statement_es5_array_indexing(for_in_of);
        }
    }

    /// Emit for-of using full iterator protocol (--downlevelIteration enabled)
    ///
    /// Transforms:
    /// ```typescript
    /// for (const item of iterable) { body }
    /// ```
    /// Into:
    /// ```javascript
    /// var e_1, _a, e_1_1;
    /// try {
    ///     for (e_1 = __values(iterable), _a = e_1.next(); !_a.done; _a = e_1.next()) {
    ///         var item = _a.value;
    ///         body
    ///     }
    /// }
    /// catch (e_1_1) { e_1 = { error: e_1_1 }; }
    /// finally {
    ///     try {
    ///         if (_a && !_a.done && (_a = e_1["return"])) _a.call(e_1);
    ///     }
    ///     finally { if (e_1) throw e_1.error; }
    /// }
    /// ```
    fn emit_for_of_statement_es5_iterator(
        &mut self,
        for_of_idx: NodeIndex,
        for_in_of: &ForInOfData,
    ) {
        let counter = self.ctx.destructuring_state.for_of_counter;

        // TypeScript's variable naming pattern:
        // Top-level: e_N (error container), _a (temp for return function)
        // For loop: _b (iterator), _c (result)
        // Catch: e_N_1 (error value, not pre-declared)
        let error_container_name = format!("e_{}", counter + 1);
        let return_temp_name = self
            .reserved_iterator_return_temps
            .remove(&for_of_idx)
            .unwrap_or_else(|| self.get_temp_var_name()); // _a, _b, ...
        let is_nested_iterator_for_of = self.iterator_for_of_depth > 0;
        self.iterator_for_of_depth += 1;

        // Reserve return temps for nested iterator for-of loops in this body before
        // allocating this loop's iterator/result temps.
        self.preallocate_nested_iterator_return_temps(for_in_of.statement);

        let loop_iterator_name = self.get_temp_var_name(); // _b
        let loop_result_name = self.get_temp_var_name(); // _c
        let catch_error_name = format!("e_{}_1", counter + 1);

        self.ctx.destructuring_state.for_of_counter += 1;

        // Hoist error container + return temp to the top of the source file scope.
        // This matches tsc's combined var preamble shape when multiple transformed for-of
        // loops appear in the same file.
        self.hoisted_for_of_temps.push(error_container_name.clone());
        self.hoisted_for_of_temps.push(return_temp_name.clone());

        // try block
        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Leading comments for downlevel for-of are deferred by statement emitters
        // and emitted here so they stay attached to the transformed loop body.
        if let Some(for_of_node) = self.arena.get(for_of_idx) {
            let actual_start = self.skip_whitespace_forward(for_of_node.pos, for_of_node.end);
            self.emit_comments_before_pos(actual_start);
        }

        // for loop with iterator protocol, using NEW temp vars
        self.write("for (var ");
        self.write(&loop_iterator_name);
        self.write(" = ");
        if is_nested_iterator_for_of {
            self.write("(");
            self.write(&error_container_name);
            self.write(" = void 0, __values(");
            self.emit_expression(for_in_of.expression);
            self.write(")), ");
        } else {
            self.write("__values(");
            self.emit_expression(for_in_of.expression);
            self.write("), ");
        }
        self.write(&loop_result_name);
        self.write(" = ");
        self.write(&loop_iterator_name);
        self.write(".next(); !");
        self.write(&loop_result_name);
        self.write(".done; ");
        self.write(&loop_result_name);
        self.write(" = ");
        self.write(&loop_iterator_name);
        self.write(".next()) {");
        self.write_line();
        self.increase_indent();

        // Enter a new scope for the loop body to track variable shadowing
        self.ctx.block_scope_state.enter_scope();

        // Pre-register loop variables before emitting (needed for shadowing)
        // Note: We only pre-register for VARIABLE_DECLARATION_LIST nodes, not assignment targets
        self.pre_register_for_of_loop_variable(for_in_of.initializer);

        // Emit the value binding: var item = _c.value;
        self.emit_for_of_value_binding_iterator_es5(for_in_of.initializer, &loop_result_name);
        self.write_line();

        // Emit the loop body
        self.emit_for_of_body(for_in_of.statement);

        // Exit the loop body scope
        self.ctx.block_scope_state.exit_scope();

        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.decrease_indent();
        self.write("}");
        self.write_line();

        // catch block
        self.write("catch (");
        self.write(&catch_error_name);
        self.write(") { ");
        self.write(&error_container_name);
        self.write(" = { error: ");
        self.write(&catch_error_name);
        self.write(" }; }");
        self.write_line();

        // finally block
        self.write("finally {");
        self.write_line();
        self.increase_indent();

        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Cleanup: if (_c && !_c.done && (_a = _b.return)) _a.call(_b);
        self.write("if (");
        self.write(&loop_result_name);
        self.write(" && !");
        self.write(&loop_result_name);
        self.write(".done && (");
        self.write(&return_temp_name);
        self.write(" = ");
        self.write(&loop_iterator_name);
        self.write(".return)) ");
        self.write(&return_temp_name);
        self.write(".call(");
        self.write(&loop_iterator_name);
        self.write(");");

        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.write("finally { if (");
        self.write(&error_container_name);
        self.write(") throw ");
        self.write(&error_container_name);
        self.write(".error; }");

        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.iterator_for_of_depth = self.iterator_for_of_depth.saturating_sub(1);
    }

    fn preallocate_nested_iterator_return_temps(&mut self, stmt_idx: NodeIndex) {
        self.visit_for_of_return_temp_prealloc(stmt_idx);
    }

    fn visit_for_of_return_temp_prealloc(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::FOR_OF_STATEMENT {
            if let Some(for_in_of) = self.arena.get_for_in_of(node)
                && !for_in_of.await_modifier
            {
                if !self.reserved_iterator_return_temps.contains_key(&idx) {
                    let temp = self.get_temp_var_name();
                    self.reserved_iterator_return_temps.insert(idx, temp);
                }
                self.visit_for_of_return_temp_prealloc(for_in_of.statement);
            }
            return;
        }

        match node.kind {
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.visit_for_of_return_temp_prealloc(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(node) {
                    self.visit_for_of_return_temp_prealloc(if_stmt.then_statement);
                    self.visit_for_of_return_temp_prealloc(if_stmt.else_statement);
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.arena.get_try(node) {
                    self.visit_for_of_return_temp_prealloc(try_stmt.try_block);
                    self.visit_for_of_return_temp_prealloc(try_stmt.catch_clause);
                    self.visit_for_of_return_temp_prealloc(try_stmt.finally_block);
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_clause) = self.arena.get_catch_clause(node) {
                    self.visit_for_of_return_temp_prealloc(catch_clause.block);
                }
            }
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    self.visit_for_of_return_temp_prealloc(loop_data.statement);
                } else if let Some(for_in_of) = self.arena.get_for_in_of(node) {
                    self.visit_for_of_return_temp_prealloc(for_in_of.statement);
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(sw) = self.arena.get_switch(node) {
                    self.visit_for_of_return_temp_prealloc(sw.case_block);
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(clause) = self.arena.get_case_clause(node) {
                    for &stmt in &clause.statements.nodes {
                        self.visit_for_of_return_temp_prealloc(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.arena.get_labeled_statement(node) {
                    self.visit_for_of_return_temp_prealloc(labeled.statement);
                }
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_stmt) = self.arena.get_with_statement(node) {
                    self.visit_for_of_return_temp_prealloc(with_stmt.then_statement);
                }
            }
            _ => {}
        }
    }

    /// Emit for-of using simple array indexing (default, --downlevelIteration disabled)
    ///
    /// Transforms:
    /// ```typescript
    /// for (const item of arr) { body }
    /// ```
    /// Into:
    /// ```javascript
    /// for (var _i = 0, arr_1 = arr; _i < arr_1.length; _i++) {
    ///     var item = arr_1[_i];
    ///     body
    /// }
    /// ```
    /// Note: This only works for arrays, not for Sets, Maps, Strings, or Generators.
    fn emit_for_of_statement_es5_array_indexing(&mut self, for_in_of: &ForInOfData) {
        // Simple array indexing pattern (default, no --downlevelIteration):
        // for (var _i = 0, arr_1 = arr; _i < arr_1.length; _i++) {
        //     var v = arr_1[_i];
        //     <body>
        // }
        //
        // TypeScript uses a single global name generator:
        // - First for-of gets `_i` as index name (special case)
        // - All other temp names come from the global counter (_a, _b, _c, ...)
        // - Named arrays use `<name>_1` (doesn't consume from counter)
        // - Names are checked against all identifiers in the source file

        // CRITICAL: Pre-register the loop variable BEFORE emitting the initialization expression
        // This ensures that references to shadowed variables in the array initializer get renamed.
        // For example: `for (let v of [v])` where inner v shadows outer v
        // We need to register inner v as v_1 BEFORE emitting [v] so the reference becomes [v_1]
        self.ctx.block_scope_state.enter_scope();
        self.pre_register_for_of_loop_variable(for_in_of.initializer);

        // Generate index name: first for-of gets `_i`, subsequent ones use global counter
        let index_name = if !self.first_for_of_emitted {
            self.first_for_of_emitted = true;
            let candidate = "_i".to_string();
            if self.file_identifiers.contains(&candidate)
                || self.generated_temp_names.contains(&candidate)
            {
                let name = self.make_unique_name();
                self.ctx.block_scope_state.reserve_name(name.clone());
                name
            } else {
                self.generated_temp_names.insert(candidate.clone());
                self.ctx.block_scope_state.reserve_name(candidate.clone());
                candidate
            }
        } else {
            let name = self.make_unique_name();
            self.ctx.block_scope_state.reserve_name(name.clone());
            name
        };

        // For assignment-pattern for-of with object/array literals, tsc allocates
        // destructuring temps before choosing the array temp in the loop header.
        // Reserve those temps now so later lowering reuses them in order.
        let reserve_count = self.estimate_for_of_assignment_temp_reserve(for_in_of.initializer);
        if reserve_count > 0 {
            self.preallocate_temp_names(reserve_count);
        }

        // Derive array name from expression:
        // - Simple identifier `arr` -> `arr_1`, `arr_2`, etc. (doesn't consume counter)
        // - Complex expression -> `_a`, `_b`, etc. (from global counter)
        let array_name = if let Some(expr_node) = self.arena.get(for_in_of.expression) {
            if expr_node.kind == SyntaxKind::Identifier as u16 {
                if let Some(ident) = self.arena.get_identifier(expr_node) {
                    let name = self.arena.resolve_identifier_text(ident).to_string();
                    // Try incrementing suffixes: name_1, name_2, name_3, ...
                    let mut found = None;
                    for suffix in 1..=100 {
                        let candidate = format!("{}_{}", name, suffix);
                        if !self.file_identifiers.contains(&candidate)
                            && !self.generated_temp_names.contains(&candidate)
                        {
                            found = Some(candidate);
                            break;
                        }
                    }
                    if let Some(candidate) = found {
                        self.generated_temp_names.insert(candidate.clone());
                        // Reserve this name in block scope state to prevent variable shadowing conflicts
                        self.ctx.block_scope_state.reserve_name(candidate.clone());
                        candidate
                    } else {
                        let name = self.make_unique_name_fresh();
                        self.ctx.block_scope_state.reserve_name(name.clone());
                        name
                    }
                } else {
                    let name = self.make_unique_name_fresh();
                    self.ctx.block_scope_state.reserve_name(name.clone());
                    name
                }
            } else {
                let name = self.make_unique_name_fresh();
                self.ctx.block_scope_state.reserve_name(name.clone());
                name
            }
        } else {
            let name = self.make_unique_name_fresh();
            self.ctx.block_scope_state.reserve_name(name.clone());
            name
        };

        self.write("for (var ");
        self.write(&index_name);
        self.write(" = 0, ");
        self.write(&array_name);
        self.write(" = ");
        self.emit_expression(for_in_of.expression);
        self.write("; ");
        self.write(&index_name);
        self.write(" < ");
        self.write(&array_name);
        self.write(".length; ");
        self.write(&index_name);
        self.write("++) ");

        self.write("{");
        self.write_line();
        self.increase_indent();

        // Scope was already entered above (before emitting the initialization expression)

        self.emit_for_of_value_binding_array_es5(for_in_of.initializer, &array_name, &index_name);
        self.write_line();

        // Emit the loop body
        self.emit_for_of_body(for_in_of.statement);

        // Exit the loop body scope
        self.ctx.block_scope_state.exit_scope();

        self.decrease_indent();
        self.write("}");
    }

    fn estimate_for_of_assignment_temp_reserve(&self, initializer: NodeIndex) -> usize {
        let Some(init_node) = self.arena.get(initializer) else {
            return 0;
        };
        match init_node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(init_node)
                    && lit.elements.nodes.len() > 1
                {
                    // One extracted source temp + per-property default temps.
                    let mut defaults = 0usize;
                    for &elem_idx in &lit.elements.nodes {
                        let Some(elem_node) = self.arena.get(elem_idx) else {
                            continue;
                        };
                        if elem_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                            && let Some(prop) = self.arena.get_property_assignment(elem_node)
                            && let Some(value_node) = self.arena.get(prop.initializer)
                            && value_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                            && let Some(bin) = self.arena.get_binary_expr(value_node)
                            && bin.operator_token == SyntaxKind::EqualsToken as u16
                        {
                            defaults += 1;
                        }
                    }
                    return 1 + defaults;
                }
                0
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(init_node)
                    && lit.elements.nodes.len() > 1
                {
                    // One extracted source temp. Additional nested/default temps are emitted
                    // later and use normal allocation.
                    return 1;
                }
                0
            }
            _ => 0,
        }
    }

    /// Emit the for-of loop body (common logic for both array and iterator modes)
    fn emit_for_of_body(&mut self, statement: NodeIndex) {
        if let Some(stmt_node) = self.arena.get(statement) {
            if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::BLOCK {
                // If body is a block, emit its statements directly (unwrap the block)
                if let Some(block) = self.arena.get_block(stmt_node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.emit(stmt_idx);
                        self.write_line();
                    }
                }
            } else {
                self.emit(statement);
                self.write_line();
            }
        }
    }

    /// Emit value binding for iterator protocol: `var item = _a.value;`
    fn emit_for_of_value_binding_iterator_es5(
        &mut self,
        initializer: NodeIndex,
        result_name: &str,
    ) {
        if initializer.is_none() {
            return;
        }

        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };

        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            self.write("var ");
            if let Some(decl_list) = self.arena.get_variable(init_node) {
                let mut first = true;
                for &decl_idx in &decl_list.declarations.nodes {
                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    {
                        if !first {
                            self.write(", ");
                        }
                        first = false;

                        // Check if name is a binding pattern (array or object destructuring)
                        if self.is_binding_pattern(decl.name) {
                            // For downlevelIteration with binding patterns, use __read
                            // Transform: var [a = 0, b = 1] = _c.value
                            // Into: var _d = __read(_c.value, 2), _e = _d[0], a = _e === void 0 ? 0 : _e, ...
                            self.emit_es5_destructuring_with_read(
                                decl.name,
                                &format!("{}.value", result_name),
                                &mut first,
                            );
                        } else {
                            // Simple identifier binding
                            self.emit(decl.name);
                            self.write(" = ");
                            self.write(result_name);
                            self.write(".value");
                        }
                    }
                }
            }
            self.write_semicolon();
        } else if self.is_binding_pattern(initializer) {
            self.write("var ");
            let mut first = true;
            self.emit_es5_destructuring_from_value(
                initializer,
                &format!("{}.value", result_name),
                &mut first,
            );
            self.write_semicolon();
        } else {
            self.emit_expression(initializer);
            self.write(" = ");
            self.write(result_name);
            self.write(".value");
            self.write_semicolon();
        }
    }

    /// Pre-register loop variables before emitting the for-of initialization expression.
    /// This ensures that references to outer variables with the same name get properly renamed.
    ///
    /// For example: `for (let v of [v])` where inner v shadows outer v
    /// We register inner v as v_1, so when we emit [v], it becomes [v_1]
    ///
    /// Note: Only registers variables from VARIABLE_DECLARATION_LIST nodes (e.g., `for (let v of ...)`).
    /// Bare identifiers (e.g., `for (v of ...)`) are assignment targets, not declarations, so they don't
    /// create new variables and shouldn't be pre-registered.
    fn pre_register_for_of_loop_variable(&mut self, initializer: NodeIndex) {
        if initializer.is_none() {
            return;
        }

        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };

        // Only handle variable declaration list: `for (let v of ...)`
        // Do NOT handle bare identifiers: `for (v of ...)` - those are assignments, not declarations
        // Note: Pre-register for both var and let/const in for-of loops because loop
        // temporaries (e.g., a_1 for array copy) create naming conflicts that must be avoided.
        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && let Some(decl_list) = self.arena.get_variable(init_node)
        {
            for &decl_idx in &decl_list.declarations.nodes {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    self.pre_register_binding_name(decl.name);
                }
            }
        }
        // Note: We explicitly do NOT pre-register for the else case (bare identifiers or patterns)
        // because those are assignment targets, not declarations
    }

    /// Pre-register a binding name (identifier or pattern) in the current scope
    fn pre_register_binding_name(&mut self, name_idx: NodeIndex) {
        if name_idx.is_none() {
            return;
        }

        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        // Simple identifier: register it directly
        if name_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                let original_name = self.arena.resolve_identifier_text(ident);
                self.ctx.block_scope_state.register_variable(original_name);
            }
        }
        // Destructuring patterns: extract all binding identifiers
        else if matches!(
            name_node.kind,
            syntax_kind_ext::ARRAY_BINDING_PATTERN | syntax_kind_ext::OBJECT_BINDING_PATTERN
        ) && let Some(pattern) = self.arena.get_binding_pattern(name_node)
        {
            for &elem_idx in &pattern.elements.nodes {
                if let Some(elem_node) = self.arena.get(elem_idx)
                    && let Some(elem) = self.arena.get_binding_element(elem_node)
                {
                    self.pre_register_binding_name(elem.name);
                }
            }
        }
    }

    /// Pre-register a var binding name. Uses `register_var_declaration` which allows
    /// same-scope redeclarations but renames for parent-scope conflicts.
    fn pre_register_var_binding_name(&mut self, name_idx: NodeIndex) {
        if name_idx.is_none() {
            return;
        }

        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                let original_name = self.arena.resolve_identifier_text(ident);
                self.ctx
                    .block_scope_state
                    .register_var_declaration(original_name);
            }
        } else if matches!(
            name_node.kind,
            syntax_kind_ext::ARRAY_BINDING_PATTERN | syntax_kind_ext::OBJECT_BINDING_PATTERN
        ) {
            if let Some(pattern) = self.arena.get_binding_pattern(name_node) {
                for &elem_idx in &pattern.elements.nodes {
                    if let Some(elem_node) = self.arena.get(elem_idx)
                        && let Some(elem) = self.arena.get_binding_element(elem_node)
                    {
                        self.pre_register_var_binding_name(elem.name);
                    }
                }
            }
            if let Some(pattern) = self.arena.get_binding_pattern(name_node) {
                for &elem_idx in &pattern.elements.nodes {
                    if let Some(elem_node) = self.arena.get(elem_idx)
                        && let Some(elem) = self.arena.get_binding_element(elem_node)
                    {
                        self.pre_register_var_binding_name(elem.name);
                    }
                }
            }
        }
    }

    /// Emit variable binding for array-indexed for-of pattern:
    /// `var v = _a[_i];`
    fn emit_for_of_value_binding_array_es5(
        &mut self,
        initializer: NodeIndex,
        array_name: &str,
        index_name: &str,
    ) {
        if initializer.is_none() {
            return;
        }

        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };

        let element_expr = format!("{}[{}]", array_name, index_name);

        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            self.write("var ");
            if let Some(decl_list) = self.arena.get_variable(init_node) {
                let mut first = true;
                for &decl_idx in &decl_list.declarations.nodes {
                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    {
                        if self.is_binding_pattern(decl.name) {
                            if let Some(pattern_node) = self.arena.get(decl.name) {
                                // Object patterns: for single-property patterns, use element_expr
                                // directly. For multi-property, create a temp.
                                if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
                                    let (obj_count, obj_rest) =
                                        self.count_effective_bindings(pattern_node);
                                    if obj_count <= 1 && !obj_rest {
                                        // Single property: var nameA = robots_1[_i].name
                                        self.emit_es5_destructuring_pattern_direct(
                                            pattern_node,
                                            &element_expr,
                                            &mut first,
                                        );
                                    } else {
                                        // Multi property: var _p = robots_1[_o], nameA = _p.name, skillA = _p.skill
                                        let temp_name = self.get_temp_var_name();
                                        if !first {
                                            self.write(", ");
                                        }
                                        first = false;
                                        self.write(&temp_name);
                                        self.write(" = ");
                                        self.write(&element_expr);
                                        self.emit_es5_destructuring_pattern(
                                            pattern_node,
                                            &temp_name,
                                        );
                                    }
                                    continue;
                                }

                                let (effective_count, has_rest) =
                                    self.count_effective_bindings(pattern_node);

                                // Single element at index 0: inline as name = arr[idx][0]
                                if effective_count == 1
                                    && !has_rest
                                    && self.try_emit_single_inline_from_expr(
                                        pattern_node,
                                        &element_expr,
                                        &mut first,
                                    )
                                {
                                    continue;
                                }

                                // Rest-only: inline as name = arr[idx].slice(0)
                                if effective_count == 0
                                    && has_rest
                                    && self.try_emit_rest_only_from_expr(
                                        pattern_node,
                                        &element_expr,
                                        &mut first,
                                    )
                                {
                                    continue;
                                }

                                // Multi-binding or complex: create temp and lower
                                // e.g., var [, nameA] = robots_1[_i] → var _a = robots_1[_i], nameA = _a[1]
                                let temp_name = self.get_temp_var_name();
                                if !first {
                                    self.write(", ");
                                }
                                first = false;
                                self.write(&temp_name);
                                self.write(" = ");
                                self.write(&element_expr);
                                self.emit_es5_destructuring_pattern(pattern_node, &temp_name);
                            }
                        } else {
                            if !first {
                                self.write(", ");
                            }
                            first = false;

                            // Handle variable shadowing: get the pre-registered renamed name
                            // (variable was already registered in pre_register_for_of_loop_variable)
                            if let Some(ident_node) = self.arena.get(decl.name) {
                                if ident_node.kind == SyntaxKind::Identifier as u16 {
                                    if let Some(ident) = self.arena.get_identifier(ident_node) {
                                        let original_name =
                                            self.arena.resolve_identifier_text(ident);
                                        let emitted_name = self
                                            .ctx
                                            .block_scope_state
                                            .get_emitted_name(original_name)
                                            .unwrap_or_else(|| original_name.to_string());
                                        self.write(&emitted_name);
                                    } else {
                                        self.emit(decl.name);
                                    }
                                } else {
                                    self.emit(decl.name);
                                }
                            } else {
                                self.emit(decl.name);
                            }

                            self.write(" = ");
                            self.write(&element_expr);
                        }
                    }
                }
            }
            self.write_semicolon();
        } else if init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            // Assignment destructuring pattern in for-of: {name: nameA} or [, nameA]
            // Lower to: nameA = element_expr.name or nameA = element_expr[1]
            let mut first = true;
            match init_node.kind {
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                    if let Some(lit) = self.arena.get_literal_expr(init_node) {
                        let elem_count = lit.elements.nodes.len();
                        if elem_count > 1 {
                            // Multi-element: need temp
                            let temp = self.make_unique_name_hoisted();
                            self.write(&temp);
                            self.write(" = ");
                            self.write(&element_expr);
                            first = false;
                            self.emit_assignment_array_destructuring(
                                &lit.elements.nodes,
                                &temp,
                                &mut first,
                                None,
                            );
                        } else {
                            // Single element: inline
                            self.emit_assignment_array_destructuring(
                                &lit.elements.nodes,
                                &element_expr,
                                &mut first,
                                None,
                            );
                        }
                    }
                }
                _ => {
                    // Object pattern
                    if let Some(lit) = self.arena.get_literal_expr(init_node) {
                        let elem_count = lit.elements.nodes.len();
                        if elem_count > 1 {
                            let temp = self.make_unique_name_hoisted();
                            self.write(&temp);
                            self.write(" = ");
                            self.write(&element_expr);
                            first = false;
                            self.emit_assignment_object_destructuring(
                                &lit.elements.nodes,
                                &temp,
                                &mut first,
                            );
                        } else {
                            self.emit_assignment_object_destructuring(
                                &lit.elements.nodes,
                                &element_expr,
                                &mut first,
                            );
                        }
                    }
                }
            }
            self.write_semicolon();
        } else {
            self.emit_expression(initializer);
            self.write(" = ");
            self.write(&element_expr);
            self.write_semicolon();
        }
    }

    // =========================================================================
    // Assignment destructuring lowering (ES5)
    // Lowers: [, nameA] = expr  →  nameA = expr[1]
    //         { name: nameA } = expr  →  nameA = expr.name
    // =========================================================================

    /// Count the total number of elements (including holes) in an array destructuring pattern.
    /// TypeScript creates a temp for non-identifier sources when there are 2+ elements
    /// (including holes). With exactly 1 element (no holes), it inlines the source.
    fn count_array_destructuring_elements(&self, elements: &[NodeIndex]) -> usize {
        elements.len()
    }

    /// Lower an assignment destructuring pattern to ES5.
    /// Called from emit_binary_expression when left side is array/object literal.
    pub(super) fn emit_assignment_destructuring_es5(
        &mut self,
        left_node: &Node,
        right_idx: NodeIndex,
    ) {
        // Determine if right side is a simple identifier (can be accessed directly)
        let is_simple = self
            .arena
            .get(right_idx)
            .is_some_and(|n| n.kind == SyntaxKind::Identifier as u16);

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
                _ => 2, // object patterns always need temp for now
            }
        };

        // For complex sources (function calls, array literals), we only need a temp
        // if the pattern requires multiple accesses. Single-access patterns can
        // inline the source expression directly.
        let needs_temp = !is_simple && element_count > 1;

        let source_name = if is_simple {
            self.get_identifier_text(right_idx)
        } else if needs_temp {
            let temp = self.make_unique_name_hoisted();
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
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(left_node) {
                    self.emit_assignment_object_destructuring(
                        &lit.elements.nodes,
                        &source_name,
                        &mut first,
                    );
                }
            }
            _ => {
                // Fallback: emit as-is
                self.emit_node_default(left_node, right_idx);
            }
        }
    }

    /// Emit lowered array assignment destructuring.
    /// `[, nameA, [primaryB, secondaryB]] = source` →
    /// `nameA = source[1], _a = source[2], primaryB = _a[0], secondaryB = _a[1]`
    ///
    /// When `inline_source` is Some, the source expression is emitted inline
    /// instead of using the `source` string. Used when only one access is needed.
    fn emit_assignment_array_destructuring(
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
                        {
                            // Nested destructuring on rest
                            let temp = self.make_unique_name_hoisted();
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
                            self.emit(spread.expression);
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
                });

                if is_nested {
                    let extract_temp = self.make_unique_name_hoisted();
                    let default_temp = self.make_unique_name_hoisted();
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
                    let temp = self.make_unique_name_hoisted();
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
                    self.emit(bin.left);
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
            {
                let temp = self.make_unique_name_hoisted();
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
            self.emit(elem_idx);
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
    fn emit_assignment_object_destructuring(
        &mut self,
        elements: &[NodeIndex],
        source: &str,
        first: &mut bool,
    ) {
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
                        let key_text = self.get_property_key_text(prop.name);
                        let key = key_text.unwrap_or_default();

                        // Check if value is a nested pattern
                        let value_node = self.arena.get(prop.initializer);
                        let is_nested = value_node.is_some_and(|n| {
                            n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                || n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        });

                        if is_nested {
                            let temp = self.make_unique_name_hoisted();
                            self.emit_assignment_separator(first);
                            self.write(&temp);
                            self.write(" = ");
                            self.write(source);
                            self.write(".");
                            self.write(&key);
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
                                let temp = self.make_unique_name_hoisted();
                                self.emit_assignment_separator(first);
                                self.write(&temp);
                                self.write(" = ");
                                self.write(source);
                                self.write(".");
                                self.write(&key);
                                self.write(", ");
                                self.emit(bin.left);
                                self.write(" = ");
                                self.write(&temp);
                                self.write(" === void 0 ? ");
                                self.emit(bin.right);
                                self.write(" : ");
                                self.write(&temp);
                                continue;
                            }
                            self.emit_assignment_separator(first);
                            self.emit(prop.initializer);
                            self.write(" = ");
                            self.write(source);
                            self.write(".");
                            self.write(&key);
                        }
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    // { name } → name = source.name
                    if let Some(shorthand) = self.arena.get_shorthand_property(elem_node) {
                        let name = self.get_identifier_text(shorthand.name);
                        self.emit_assignment_separator(first);
                        self.write(&name);
                        self.write(" = ");
                        self.write(source);
                        self.write(".");
                        self.write(&name);
                    }
                }
                k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                    // { ...rest } → rest = __rest(source, ["prop1", "prop2"])
                    if let Some(spread) = self.arena.get_spread(elem_node) {
                        self.emit_assignment_separator(first);
                        self.emit(spread.expression);
                        self.write(" = __rest(");
                        self.write(source);
                        self.write(", [");
                        // Collect non-rest property names
                        let mut prop_first = true;
                        for &other_idx in elements {
                            if other_idx == elem_idx {
                                continue;
                            }
                            if let Some(other_node) = self.arena.get(other_idx) {
                                let key = match other_node.kind {
                                    k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                                        .arena
                                        .get_property_assignment(other_node)
                                        .and_then(|p| self.get_property_key_text(p.name)),
                                    k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                                        self.arena
                                            .get_shorthand_property(other_node)
                                            .map(|s| self.get_identifier_text(s.name))
                                    }
                                    _ => None,
                                };
                                if let Some(k) = key {
                                    if !prop_first {
                                        self.write(", ");
                                    }
                                    self.write("\"");
                                    self.write(&k);
                                    self.write("\"");
                                    prop_first = false;
                                }
                            }
                        }
                        self.write("])");
                    }
                }
                _ => {}
            }
        }
    }

    /// Helper to emit nested destructuring from a source name.
    fn emit_assignment_nested_destructuring(
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
                    self.emit_assignment_object_destructuring(&lit.elements.nodes, source, first);
                }
            }
            _ => {}
        }
    }

    /// Emit separator for assignment destructuring (`, ` between parts).
    fn emit_assignment_separator(&mut self, first: &mut bool) {
        if !*first {
            self.write(", ");
        }
        *first = false;
    }

    /// Get property key text from a property name node.
    fn get_property_key_text(&self, name_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(name_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            Some(self.get_identifier_text(name_idx))
        } else if node.kind == SyntaxKind::StringLiteral as u16 {
            // For string keys like { "name": value }
            self.get_string_literal_text(name_idx)
        } else if node.kind == SyntaxKind::NumericLiteral as u16 {
            self.get_numeric_literal_text(name_idx)
        } else {
            None
        }
    }

    fn get_string_literal_text(&self, idx: NodeIndex) -> Option<String> {
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

    fn get_numeric_literal_text(&self, idx: NodeIndex) -> Option<String> {
        let source = self.source_text?;
        let node = self.arena.get(idx)?;
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let end = node.end as usize;
        Some(source[start..end].to_string())
    }
}
