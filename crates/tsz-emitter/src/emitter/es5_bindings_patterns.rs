//! ES5 destructuring - binding element patterns and parameter bindings.

use super::{ParamTransformPlan, Printer};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{BindingElementData, BindingPatternData, ForInOfData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(super) fn get_binding_element_property_key(
        &self,
        elem: &BindingElementData,
    ) -> Option<NodeIndex> {
        let key_idx = if elem.property_name.is_some() {
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
    pub(super) fn emit_es5_binding_element(&mut self, elem_idx: NodeIndex, temp_name: &str) {
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
            let pattern_temp = if elem.initializer.is_some() {
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

    /// If `key_idx` is a computed property, emit a temp variable assignment and return the temp name
    /// Returns None if not computed
    pub(super) fn emit_computed_key_temp_if_needed(
        &mut self,
        key_idx: NodeIndex,
    ) -> Option<String> {
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
    pub(super) fn emit_es5_array_binding_element(
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
            let pattern_temp = if elem.initializer.is_some() {
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

    /// Like `emit_es5_binding_element` but with first flag for separator control
    pub(super) fn emit_es5_binding_element_direct(
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
            let pattern_temp = if elem.initializer.is_some() {
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

    /// Similar to `emit_computed_key_temp_if_needed` but handles the first flag for direct destructuring
    pub(super) fn emit_computed_key_temp_for_direct(
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

    /// Like `emit_es5_array_binding_element` but with first flag for separator control
    pub(super) fn emit_es5_array_binding_element_direct(
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
            let pattern_temp = if elem.initializer.is_some() {
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

    pub(super) fn emit_es5_destructuring_pattern(&mut self, pattern_node: &Node, temp_name: &str) {
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

    /// Like `emit_es5_destructuring_pattern` but handles the `first` flag for the first
    /// non-omitted element, allowing it to be emitted without a `, ` prefix.
    /// Used when the initializer is a simple identifier and no temp variable is needed.
    pub(super) fn emit_es5_destructuring_pattern_direct(
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

    pub(super) fn emit_param_default_assignment(&mut self, name: &str, initializer: NodeIndex) {
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

    pub(super) fn emit_param_binding_assignments(
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

    pub(super) fn emit_param_object_binding_element(
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

            if elem.initializer.is_some() {
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
        if elem.initializer.is_some() {
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

    /// Similar to `emit_computed_key_temp_if_needed` but handles started flag for param destructuring
    pub(super) fn emit_computed_key_temp_for_param(
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

    pub(super) fn emit_param_array_binding_element(
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

            let source_name = if elem.initializer.is_some() {
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
        if elem.initializer.is_some() {
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

    pub(super) fn emit_param_object_rest_element(
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

    pub(super) fn emit_param_array_rest_element(
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

    pub(super) fn emit_param_assignment_prefix(&mut self, started: &mut bool) {
        if !*started {
            self.write("var ");
            *started = true;
        } else {
            self.write(", ");
        }
    }

    pub(super) fn emit_es5_object_rest_element(
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

    pub(super) fn emit_es5_array_rest_element(
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

    pub(super) fn emit_es5_destructuring_pattern_idx(
        &mut self,
        pattern_idx: NodeIndex,
        temp_name: &str,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        self.emit_es5_destructuring_pattern(pattern_node, temp_name);
    }

    pub(super) fn collect_object_rest_props(&self, pattern: &BindingPatternData) -> Vec<NodeIndex> {
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
            let key_idx = if elem.property_name.is_some() {
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

    pub(super) fn emit_rest_exclude_list(&mut self, props: &[NodeIndex]) {
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

    pub(super) fn emit_rest_property_key(&mut self, key_idx: NodeIndex) {
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
        if for_in_of.await_modifier {
            self.emit_for_of_statement_es5_async_iterator(for_of_idx, for_in_of);
        } else if self.ctx.options.downlevel_iteration {
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
    pub(super) fn emit_for_of_statement_es5_iterator(
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

    /// Emit for-await-of using async iterator protocol (`__asyncValues`).
    ///
    /// Transforms:
    /// ```typescript
    /// for await (const item of iterable) { body }
    /// ```
    /// Into:
    /// ```javascript
    /// var e_1, _a, e_1_1;
    /// try {
    ///     for (var _c = true, iterable_1 = __asyncValues(iterable), iterable_1_1 = yield iterable_1.next(), _a = iterable_1_1.done, !_a; _c = true) {
    ///         var _d = iterable_1_1.value;
    ///         _c = false;
    ///         var item = _d;
    ///         body
    ///     }
    /// }
    /// catch (e_1_1) { e_1 = { error: e_1_1 }; }
    /// finally {
    ///     try {
    ///         if (!_c && !_a && (_b = iterable_1.return)) yield _b.call(iterable_1);
    ///     }
    ///     finally { if (e_1) throw e_1.error; }
    /// }
    /// ```
    pub(super) fn emit_for_of_statement_es5_async_iterator(
        &mut self,
        for_of_idx: NodeIndex,
        for_in_of: &ForInOfData,
    ) {
        let counter = self.ctx.destructuring_state.for_of_counter;

        // TypeScript's variable naming pattern:
        // Top-level: e_N (error container), _a (temp for return function)
        // For loop: _b (iterator), _c (result), _d (done), _e (guard)
        // Catch: e_N_1 (error value, not pre-declared)
        let error_container_name = format!("e_{}", counter + 1);
        let return_temp_name = self
            .reserved_iterator_return_temps
            .remove(&for_of_idx)
            .unwrap_or_else(|| self.get_temp_var_name()); // _a, _b, ...
        let is_nested_iterator_for_of = self.iterator_for_of_depth > 0;
        self.iterator_for_of_depth += 1;

        // Reserve return temps for nested iterator for-of loops in this body before
        // allocating this loop's iterator/result vars.
        self.preallocate_nested_iterator_return_temps(for_in_of.statement);

        let loop_iterator_name = self.get_temp_var_name(); // _b
        let loop_result_name = self.get_temp_var_name(); // _c
        let loop_done_name = self.get_temp_var_name(); // _d
        let loop_guard_name = self.get_temp_var_name(); // _e
        let catch_error_name = format!("e_{}_1", counter + 1);

        self.ctx.destructuring_state.for_of_counter += 1;

        // Hoist error container + return temp to the top of the source file scope.
        self.hoisted_for_of_temps.push(error_container_name.clone());
        self.hoisted_for_of_temps.push(return_temp_name.clone());

        // try block
        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Leading comments for downlevel for-await-of are deferred by statement emitters
        // and emitted here so they stay attached to the transformed loop body.
        if let Some(for_of_node) = self.arena.get(for_of_idx) {
            let actual_start = self.skip_whitespace_forward(for_of_node.pos, for_of_node.end);
            self.emit_comments_before_pos(actual_start);
        }

        // for (var _e = true, iterable_1 = __asyncValues(iterable), iterable_1_1 = [await/yield] iterable_1.next(), _d = iterable_1_1.done, !_d; _e = true) {
        let await_or_yield = if self.ctx.emit_await_as_yield {
            "yield"
        } else {
            "await"
        };
        self.write("for (var ");
        self.write(&loop_guard_name);
        self.write(" = true, ");
        self.write(&loop_iterator_name);
        self.write(" = ");
        if is_nested_iterator_for_of {
            self.write("(");
            self.write(&error_container_name);
            self.write(" = void 0, __asyncValues(");
            self.emit_expression(for_in_of.expression);
            self.write(")), ");
        } else {
            self.write("__asyncValues(");
            self.emit_expression(for_in_of.expression);
            self.write("), ");
        }
        self.write(&loop_result_name);
        self.write(" = ");
        self.write(await_or_yield);
        self.write(" ");
        self.write(&loop_iterator_name);
        self.write(".next(), ");
        self.write(&loop_done_name);
        self.write(" = ");
        self.write(&loop_result_name);
        self.write(".done; !");
        self.write(&loop_done_name);
        self.write("; ");
        self.write(&loop_guard_name);
        self.write(" = true) {");
        self.write_line();
        self.increase_indent();

        // Enter a new scope for the loop body to track variable shadowing
        self.ctx.block_scope_state.enter_scope();

        // Pre-register loop variables before emitting (needed for shadowing)
        // Note: We only pre-register for VARIABLE_DECLARATION_LIST nodes, not assignment targets
        self.pre_register_for_of_loop_variable(for_in_of.initializer);

        // Emit the value binding: var item = _c.value;
        self.emit_for_of_value_binding_iterator_es5_async(for_in_of.initializer, &loop_result_name);
        self.write_line();
        self.write(&loop_guard_name);
        self.write(" = false;");
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

        // Cleanup: if (!_e && !_d && (_a = _b.return)) [await/yield] _a.call(_b);
        self.write("if (!");
        self.write(&loop_guard_name);
        self.write(" && !");
        self.write(&loop_done_name);
        self.write(" && (");
        self.write(&return_temp_name);
        self.write(" = ");
        self.write(&loop_iterator_name);
        self.write(".return)) ");
        self.write(await_or_yield);
        self.write(" ");
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
}
