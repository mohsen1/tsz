use super::{ParamTransformPlan, Printer};
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::parser::node::{BindingElementData, BindingPatternData, ForInOfData, Node};
use crate::scanner::SyntaxKind;

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

    /// Emit ES5 destructuring: { x, y } = obj → _a = obj, x = _a.x, y = _a.y
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

        // Get temp variable name
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

            self.emit_es5_destructuring_pattern_idx(elem.name, &value_name);
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

            self.emit_es5_destructuring_pattern_idx(elem.name, &value_name);
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
        } else if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            if let Some(pattern) = self.arena.get_binding_pattern(pattern_node) {
                for (i, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                    self.emit_es5_array_binding_element(elem_idx, temp_name, i);
                }
            }
        }
    }

    pub(super) fn emit_param_prologue(&mut self, transforms: &ParamTransformPlan) {
        for param in &transforms.params {
            if let Some(initializer) = param.initializer {
                self.emit_param_default_assignment(&param.name, initializer);
            }
            if let Some(pattern) = param.pattern {
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
            if let Some(key_node) = self.arena.get(key_idx) {
                if key_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || key_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                {
                    continue;
                }
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
        let error_name = self.get_temp_var_name();
        let return_name = self.get_temp_var_name();
        let iterator_name = self.get_temp_var_name();
        let result_name = self.get_temp_var_name();

        self.write("var ");
        self.write(&error_name);
        self.write(", ");
        self.write(&return_name);
        self.write_semicolon();
        self.write_line();

        self.write("try {");
        self.write_line();
        self.increase_indent();

        self.write("for (var ");
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
        self.emit_for_of_value_binding_es5(for_in_of.initializer, &result_name);
        self.write_line();
        self.emit(for_in_of.statement);
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.write("catch (");
        self.write(&error_name);
        self.write("_1) { ");
        self.write(&error_name);
        self.write(" = { error: ");
        self.write(&error_name);
        self.write("_1 }; }");
        self.write_line();

        self.write("finally {");
        self.write_line();
        self.increase_indent();

        self.write("try {");
        self.write_line();
        self.increase_indent();

        self.write("if (");
        self.write(&result_name);
        self.write(" && !");
        self.write(&result_name);
        self.write(".done && (");
        self.write(&return_name);
        self.write(" = ");
        self.write(&iterator_name);
        self.write(".return)) ");
        self.write(&return_name);
        self.write(".call(");
        self.write(&iterator_name);
        self.write(")");
        self.write_semicolon();
        self.write_line();

        self.decrease_indent();
        self.write("} finally {");
        self.write_line();
        self.increase_indent();

        self.write("if (");
        self.write(&error_name);
        self.write(") throw ");
        self.write(&error_name);
        self.write(".error");
        self.write_semicolon();
        self.write_line();

        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.decrease_indent();
        self.write("}");
    }

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
}
