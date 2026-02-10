use super::{ParamTransformPlan, Printer};
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

        self.write("var ");

        let mut first = true;
        for &decl_idx in &decl_list.declarations.nodes {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };

            if self.is_binding_pattern(decl.name) && !decl.initializer.is_none() {
                self.emit_es5_destructuring(decl_idx, &mut first);
            } else {
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

    /// Emit ES5 destructuring: { x, y } = obj → _a = obj, x = _a.x, y = _a.y
    /// When the initializer is a simple identifier, TypeScript skips the temp variable
    /// and uses the identifier directly: var [, name] = robot → var name = robot[1]
    fn emit_es5_destructuring(&mut self, decl_idx: NodeIndex, first: &mut bool) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };
        let Some(pattern_node) = self.arena.get(decl.name) else {
            return;
        };

        // Check if the initializer is a simple identifier - if so, skip temp variable
        let is_simple_ident = self
            .arena
            .get(decl.initializer)
            .map(|n| n.kind == SyntaxKind::Identifier as u16)
            .unwrap_or(false);

        if is_simple_ident {
            // Use the identifier directly without temp variable
            let ident_text = self.get_identifier_text(decl.initializer);
            self.emit_es5_destructuring_pattern_direct(pattern_node, &ident_text, first);
        } else {
            // For complex expressions: check if single binding at index 0 → inline
            // TypeScript only inlines [x] = expr → x = expr[0], not [, x] = expr
            let (effective_count, has_rest) = self.count_effective_bindings(pattern_node);
            if effective_count == 1
                && !has_rest
                && self.emit_single_array_binding_inline(pattern_node, decl.initializer, first)
            {
                return;
            }

            // Rest-only array pattern: [...rest] = expr → rest = expr.slice(0)
            // TypeScript inlines this without a temp variable for any expression
            if effective_count == 0
                && has_rest
                && self.emit_rest_only_array_inline(pattern_node, decl.initializer, first)
            {
                return;
            }

            // Complex expression with multiple bindings: need temp variable
            let temp_name = self.get_temp_var_name();

            // Emit temp variable assignment: _a = initializer
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write(&temp_name);
            self.write(" = ");
            self.emit(decl.initializer);

            self.emit_es5_destructuring_pattern(pattern_node, &temp_name);
        }
    }

    #[allow(dead_code)]
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

    fn get_binding_element_property_key(&self, elem: &BindingElementData) -> Option<NodeIndex> {
        let key_idx = if !elem.property_name.is_none() {
            elem.property_name
        } else {
            elem.name
        };
        let Some(key_node) = self.arena.get(key_idx) else {
            return None;
        };
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

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5(key_idx, temp_name);

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
            self.emit_assignment_target_es5(key_idx, temp_name);
        } else {
            let value_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5(key_idx, temp_name);
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

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5(key_idx, temp_name);

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
            self.emit_assignment_target_es5(key_idx, temp_name);
        } else {
            let value_name = self.get_temp_var_name();
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5(key_idx, temp_name);
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

        if self.is_binding_pattern(elem.name) {
            let value_name = self.get_temp_var_name();
            self.emit_param_assignment_prefix(started);
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5(key_idx, temp_name);

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
            self.emit_assignment_target_es5(key_idx, temp_name);
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
            self.emit_assignment_target_es5(key_idx, temp_name);
        }
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
        let rest_temp = if is_pattern {
            Some(self.get_temp_var_name())
        } else {
            None
        };

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
        let rest_temp = if is_pattern {
            Some(self.get_temp_var_name())
        } else {
            None
        };

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
        let rest_temp = if is_pattern {
            Some(self.get_temp_var_name())
        } else {
            None
        };

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
        let rest_temp = if is_pattern {
            Some(self.get_temp_var_name())
        } else {
            None
        };

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

    pub(super) fn emit_for_of_statement_es5(&mut self, for_in_of: &ForInOfData) {
        // Check if downlevelIteration is enabled
        if self.ctx.options.downlevel_iteration {
            self.emit_for_of_statement_es5_iterator(for_in_of);
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
    fn emit_for_of_statement_es5_iterator(&mut self, for_in_of: &ForInOfData) {
        let counter = self.ctx.destructuring_state.for_of_counter;

        // Generate variable names:
        // - iterator: e_1, e_2, e_3, ...
        // - result: _a, _b, _c, ... (temp var)
        // - error: e_1_1, e_2_1, e_3_1, ...
        let iterator_name = format!("e_{}", counter + 1);
        let result_name = self.get_temp_var_name();
        let error_name = format!("e_{}_1", counter + 1);

        self.ctx.destructuring_state.for_of_counter += 1;

        // Declare variables at the top
        self.write("var ");
        self.write(&iterator_name);
        self.write(", ");
        self.write(&result_name);
        self.write(", ");
        self.write(&error_name);
        self.write(";");
        self.write_line();

        // try block
        self.write("try ");
        self.write("{");
        self.write_line();
        self.increase_indent();

        // for loop with iterator protocol
        self.write("for (");
        self.write(&iterator_name);
        self.write(" = __values(");
        self.emit_expression(for_in_of.expression);
        self.write("), ");
        self.write(&result_name);
        self.write(" = ");
        self.write(&iterator_name);
        self.write(".next(); !");
        self.write(&result_name);
        self.write(".done; ");
        self.write(&result_name);
        self.write(" = ");
        self.write(&iterator_name);
        self.write(".next()) ");
        self.write("{");
        self.write_line();
        self.increase_indent();

        // Emit the value binding: var item = _a.value;
        self.emit_for_of_value_binding_iterator_es5(for_in_of.initializer, &result_name);
        self.write_line();

        // Emit the loop body
        self.emit_for_of_body(for_in_of.statement);

        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.decrease_indent();
        self.write("}");

        // catch block
        self.write(" catch (");
        self.write(&error_name);
        self.write(") { ");
        self.write(&iterator_name);
        self.write(" = { error: ");
        self.write(&error_name);
        self.write(" }; }");

        // finally block
        self.write(" finally ");
        self.write("{");
        self.write_line();
        self.increase_indent();

        self.write("try ");
        self.write("{");
        self.write_line();
        self.increase_indent();

        self.write("if (");
        self.write(&result_name);
        self.write(" && !");
        self.write(&result_name);
        self.write(".done && (");
        self.write(&result_name);
        self.write(" = ");
        self.write(&iterator_name);
        self.write("[\"return\"])) ");
        self.write(&result_name);
        self.write(".call(");
        self.write(&iterator_name);
        self.write(");");

        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.write("finally { if (");
        self.write(&iterator_name);
        self.write(") throw ");
        self.write(&iterator_name);
        self.write(".error; }");

        self.write_line();
        self.decrease_indent();
        self.write("}");
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

        // Generate index name: first for-of gets `_i`, subsequent ones use global counter
        let index_name = if !self.first_for_of_emitted {
            self.first_for_of_emitted = true;
            let candidate = "_i".to_string();
            if self.file_identifiers.contains(&candidate)
                || self.generated_temp_names.contains(&candidate)
            {
                self.make_unique_name()
            } else {
                self.generated_temp_names.insert(candidate.clone());
                candidate
            }
        } else {
            self.make_unique_name()
        };

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
                        candidate
                    } else {
                        self.make_unique_name()
                    }
                } else {
                    self.make_unique_name()
                }
            } else {
                self.make_unique_name()
            }
        } else {
            self.make_unique_name()
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
        self.emit_for_of_value_binding_array_es5(for_in_of.initializer, &array_name, &index_name);
        self.write_line();

        // Emit the loop body
        self.emit_for_of_body(for_in_of.statement);

        self.decrease_indent();
        self.write("}");
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
                        self.emit(decl.name);
                        self.write(" = ");
                        self.write(result_name);
                        self.write(".value");
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

    #[allow(dead_code)]
    fn emit_for_of_value_binding_es5(&mut self, initializer: NodeIndex, result_name: &str) {
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
                    self.emit_for_of_declaration_value_es5(decl_idx, result_name, &mut first);
                }
            }
            self.write_semicolon();
        } else if self.is_binding_pattern(initializer) {
            self.write("var ");
            let mut first = true;
            self.emit_es5_destructuring_from_value(initializer, result_name, &mut first);
            self.write_semicolon();
        } else {
            self.emit_expression(initializer);
            self.write(" = ");
            self.write(result_name);
            self.write(".value");
            self.write_semicolon();
        }
    }

    #[allow(dead_code)]
    fn emit_for_of_declaration_value_es5(
        &mut self,
        decl_idx: NodeIndex,
        result_name: &str,
        first: &mut bool,
    ) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };

        if self.is_binding_pattern(decl.name) {
            self.emit_es5_destructuring_from_value(decl.name, result_name, first);
            return;
        }

        if !*first {
            self.write(", ");
        }
        *first = false;
        self.emit(decl.name);
        self.write(" = ");
        self.write(result_name);
        self.write(".value");
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
                                if effective_count == 1 && !has_rest {
                                    if self.try_emit_single_inline_from_expr(
                                        pattern_node,
                                        &element_expr,
                                        &mut first,
                                    ) {
                                        continue;
                                    }
                                }

                                // Rest-only: inline as name = arr[idx].slice(0)
                                if effective_count == 0 && has_rest {
                                    if self.try_emit_rest_only_from_expr(
                                        pattern_node,
                                        &element_expr,
                                        &mut first,
                                    ) {
                                        continue;
                                    }
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
                            self.emit(decl.name);
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
            .map(|n| n.kind == SyntaxKind::Identifier as u16)
            .unwrap_or(false);

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
                        if use_inline_source {
                            Some(right_idx)
                        } else {
                            None
                        },
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
            if elem_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                if let Some(bin) = self.arena.get_binary_expr(elem_node) {
                    if bin.operator_token == SyntaxKind::EqualsToken as u16 {
                        // Element with default: target = source[i] === void 0 ? default : source[i]
                        let target_node = self.arena.get(bin.left);
                        let is_nested = target_node
                            .map(|n| {
                                n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                    || n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            })
                            .unwrap_or(false);

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
                            self.emit_assignment_nested_destructuring(
                                bin.left,
                                &default_temp,
                                first,
                            );
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
                }
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
                        let is_nested = value_node
                            .map(|n| {
                                n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                    || n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            })
                            .unwrap_or(false);

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
                            if let Some(bin) = value_bin {
                                if bin.operator_token == SyntaxKind::EqualsToken as u16 {
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
