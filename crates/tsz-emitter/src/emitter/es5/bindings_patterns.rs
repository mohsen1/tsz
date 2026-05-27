//! ES5 destructuring - binding element patterns and parameter bindings.

use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{BindingElementData, Node, NodeAccess};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

pub(in crate::emitter) enum ES5RestProp {
    Static(NodeIndex),
    Dynamic(String),
}

/// Work deferred from an array-binding element's "head" emit phase to a later
/// pass over its siblings - see [`Printer::emit_es5_array_binding_element_head`].
pub(in crate::emitter) enum DeferredArrayElement {
    /// A nested binding pattern; emit `_temp.x` / `_temp[i]` decompositions
    /// once the current array-binding run has emitted its non-deferred work.
    NestedDecomposition { pattern: NodeIndex, temp: String },
    /// A simple binding with a default expression; emit the
    /// `name = _temp === void 0 ? init : _temp` assignment after sibling reads.
    SimpleWithDefault {
        name: NodeIndex,
        temp: String,
        initializer: NodeIndex,
    },
}

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn get_binding_element_property_key(
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

    fn emit_es5_empty_binding_element_value(
        &mut self,
        elem: &BindingElementData,
        key_idx: NodeIndex,
        temp_name: &str,
        computed_key_temp: Option<&str>,
        first: Option<&mut bool>,
    ) {
        if let Some(first) = first {
            if !*first {
                self.write(", ");
            }
            *first = false;
        } else {
            self.write(", ");
        }

        let is_empty_array = self
            .arena
            .get(elem.name)
            .is_some_and(|node| node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN);

        let value_name = self.get_temp_var_name();
        self.write(&value_name);
        self.write(" = ");
        if is_empty_array && self.ctx.options.downlevel_iteration && elem.initializer.is_none() {
            self.write_helper("__read");
            self.write("(");
            self.emit_assignment_target_es5_with_computed(key_idx, temp_name, computed_key_temp);
            self.write(", 0)");
        } else {
            self.emit_assignment_target_es5_with_computed(key_idx, temp_name, computed_key_temp);
        }

        if elem.initializer.is_some() {
            let defaulted_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&defaulted_name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);

            let empty_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&empty_name);
            self.write(" = ");
            if is_empty_array && self.ctx.options.downlevel_iteration {
                self.write_helper("__read");
                self.write("(");
                self.write(&defaulted_name);
                self.write(", 0)");
            } else {
                self.write(&defaulted_name);
            }
        }
    }

    /// Emit a single binding element for ES5 object destructuring
    pub(in crate::emitter) fn emit_es5_binding_element(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
    ) -> Option<ES5RestProp> {
        let elem_node = self.arena.get(elem_idx)?;
        let elem = self.arena.get_binding_element(elem_node)?;
        if elem.dot_dot_dot_token {
            return None;
        }

        let key_idx = self.get_binding_element_property_key(elem)?;

        // Check if key is computed and save to temp if needed
        let computed_key_temp = self.emit_computed_key_temp_if_needed(key_idx);
        let rest_prop = self.es5_rest_prop_for_key(key_idx, computed_key_temp.as_deref());

        if self.is_binding_pattern(elem.name) {
            if self.binding_pattern_is_empty(elem.name) {
                self.emit_es5_empty_binding_element_value(
                    elem,
                    key_idx,
                    temp_name,
                    computed_key_temp.as_deref(),
                    None,
                );
                return Some(rest_prop);
            }

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
            return Some(rest_prop);
        }

        if !self.has_identifier_text(elem.name) {
            return Some(rest_prop);
        }

        if elem.initializer.is_none() {
            // Emit: , bindingName = temp.propName
            self.write(", ");
            self.write_binding_identifier_text(elem.name);
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
            self.write_binding_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }

        Some(rest_prop)
    }

    /// If `key_idx` is a computed property, emit a temp variable assignment and return the temp name
    /// Returns None if not computed
    pub(in crate::emitter) fn emit_computed_key_temp_if_needed(
        &mut self,
        key_idx: NodeIndex,
    ) -> Option<String> {
        let key_node = self.arena.get(key_idx)?;

        if key_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(key_node)
        {
            let has_inner_class_temp =
                self.reserve_es5_computed_key_inner_class_temps(computed.expression);
            let temp_name = if has_inner_class_temp {
                self.make_unique_name_fresh()
            } else {
                self.get_temp_var_name()
            };
            self.write(", ");
            self.write(&temp_name);
            self.write(" = ");
            self.emit(computed.expression);
            return Some(temp_name);
        }

        None
    }

    /// Emit a single binding element for ES5 array destructuring
    pub(in crate::emitter) fn emit_es5_array_binding_element(
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
            self.write_binding_identifier_text(elem.name);
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
            self.write_binding_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }
    }

    /// Emit the "head" of a single array binding element — the reads of
    /// `source[index]` (and any default-value substitution) — and return any
    /// work that should be deferred until after sibling elements have done
    /// their own reads.
    ///
    /// Paired with [`emit_es5_array_deferred_element`]; together they
    /// implement tsc's two-phase emit for arrays containing nested patterns,
    /// so that all element reads land in source order before any decomposition
    /// or defaulted-binding assignment.
    pub(in crate::emitter) fn emit_es5_array_binding_element_head(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        index: usize,
    ) -> Option<DeferredArrayElement> {
        let elem_node = self.arena.get(elem_idx)?;
        let elem = self.arena.get_binding_element(elem_node)?;

        if elem.dot_dot_dot_token {
            if self.is_binding_pattern(elem.name) {
                let value_name = self.get_temp_var_name();
                self.write(", ");
                self.write(&value_name);
                self.write(" = ");
                self.write(temp_name);
                self.write(".slice(");
                self.write_usize(index);
                self.write(")");
                return Some(DeferredArrayElement::NestedDecomposition {
                    pattern: elem.name,
                    temp: value_name,
                });
            }
            self.emit_es5_array_rest_element(elem.name, temp_name, index);
            return None;
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

            return Some(DeferredArrayElement::NestedDecomposition {
                pattern: elem.name,
                temp: pattern_temp,
            });
        }

        if !self.has_identifier_text(elem.name) {
            return None;
        }

        if elem.initializer.is_none() {
            self.write(", ");
            self.write_binding_identifier_text(elem.name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
            None
        } else {
            // Read into a temp now; emit the defaulted assignment in the
            // deferred pass so it runs after sibling reads (and after any
            // earlier element's nested decomposition has bound its names).
            let value_name = self.get_temp_var_name();
            self.write(", ");
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
            Some(DeferredArrayElement::SimpleWithDefault {
                name: elem.name,
                temp: value_name,
                initializer: elem.initializer,
            })
        }
    }

    /// Direct-variant counterpart of
    /// [`Self::emit_es5_array_binding_element_head`] for the case where the
    /// source is a bare identifier rather than a freshly-introduced temp. The
    /// caller threads `first` so the very first emission has no leading `", "`.
    pub(in crate::emitter) fn emit_es5_array_binding_element_head_direct(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        index: usize,
        first: &mut bool,
    ) -> Option<DeferredArrayElement> {
        let elem_node = self.arena.get(elem_idx)?;
        let elem = self.arena.get_binding_element(elem_node)?;

        if elem.dot_dot_dot_token {
            if !self.has_identifier_text(elem.name) && !self.is_binding_pattern(elem.name) {
                return None;
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
                return Some(DeferredArrayElement::NestedDecomposition {
                    pattern: elem.name,
                    temp: value_name,
                });
            } else {
                self.write_binding_identifier_text(elem.name);
                self.write(" = ");
                self.write(temp_name);
                self.write(".slice(");
                self.write_usize(index);
                self.write(")");
            }
            return None;
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

            return Some(DeferredArrayElement::NestedDecomposition {
                pattern: elem.name,
                temp: pattern_temp,
            });
        }

        if !self.has_identifier_text(elem.name) {
            return None;
        }

        if elem.initializer.is_none() {
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write_binding_identifier_text(elem.name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
            None
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
            Some(DeferredArrayElement::SimpleWithDefault {
                name: elem.name,
                temp: value_name,
                initializer: elem.initializer,
            })
        }
    }

    /// Emit the "tail" half deferred from [`emit_es5_array_binding_element_head`].
    pub(in crate::emitter) fn emit_es5_array_deferred_element(
        &mut self,
        deferred: DeferredArrayElement,
    ) {
        match deferred {
            DeferredArrayElement::NestedDecomposition { pattern, temp } => {
                self.emit_es5_destructuring_pattern_idx(pattern, &temp);
            }
            DeferredArrayElement::SimpleWithDefault {
                name,
                temp,
                initializer,
            } => {
                self.write(", ");
                self.write_binding_identifier_text(name);
                self.write(" = ");
                self.write(&temp);
                self.write(" === void 0 ? ");
                self.emit_expression(initializer);
                self.write(" : ");
                self.write(&temp);
            }
        }
    }

    /// Like `emit_es5_binding_element` but with first flag for separator control
    pub(in crate::emitter) fn emit_es5_binding_element_direct(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        first: &mut bool,
    ) -> Option<ES5RestProp> {
        let elem_node = self.arena.get(elem_idx)?;
        let elem = self.arena.get_binding_element(elem_node)?;
        if elem.dot_dot_dot_token {
            return None;
        }

        let key_idx = self.get_binding_element_property_key(elem)?;

        // Check if key is computed and save to temp if needed
        let computed_key_temp = self.emit_computed_key_temp_for_direct(key_idx, first);
        let rest_prop = self.es5_rest_prop_for_key(key_idx, computed_key_temp.as_deref());

        if self.is_binding_pattern(elem.name) {
            if self.binding_pattern_is_empty(elem.name) {
                self.emit_es5_empty_binding_element_value(
                    elem,
                    key_idx,
                    temp_name,
                    computed_key_temp.as_deref(),
                    Some(first),
                );
                return Some(rest_prop);
            }

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
            return Some(rest_prop);
        }

        if !self.has_identifier_text(elem.name) {
            return Some(rest_prop);
        }

        if elem.initializer.is_none() {
            if !*first {
                self.write(", ");
            }
            *first = false;
            self.write_binding_identifier_text(elem.name);
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
            self.write_binding_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }

        Some(rest_prop)
    }

    /// Similar to `emit_computed_key_temp_if_needed` but handles the first flag for direct destructuring
    pub(in crate::emitter) fn emit_computed_key_temp_for_direct(
        &mut self,
        key_idx: NodeIndex,
        first: &mut bool,
    ) -> Option<String> {
        let key_node = self.arena.get(key_idx)?;

        if key_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(key_node)
        {
            let has_inner_class_temp =
                self.reserve_es5_computed_key_inner_class_temps(computed.expression);
            let temp_name = if has_inner_class_temp {
                self.make_unique_name_fresh()
            } else {
                self.get_temp_var_name()
            };
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

    pub(super) fn reserve_es5_computed_key_inner_class_temps(
        &mut self,
        expression: NodeIndex,
    ) -> bool {
        let count = self.count_es5_static_class_expression_temps(expression);
        if count == 0 {
            return false;
        }
        self.preallocate_temp_names(count);
        true
    }

    fn count_es5_static_class_expression_temps(&self, expression: NodeIndex) -> usize {
        if !self.ctx.target_es5 || self.ctx.options.use_define_for_class_fields {
            return 0;
        }
        let Some(node) = self.arena.get(expression) else {
            return 0;
        };

        if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            return if self
                .arena
                .get_class(node)
                .is_some_and(|class| !self.es5_static_class_expression_elements(class).is_empty())
            {
                1
            } else {
                0
            };
        }

        if self.arena.get_function(node).is_some()
            || node.kind == syntax_kind_ext::CLASS_DECLARATION
        {
            return 0;
        }

        if let Some(access) = self.arena.get_access_expr(node) {
            return self.count_es5_static_class_expression_temps(access.expression)
                + self.count_es5_static_class_expression_temps(access.name_or_argument);
        }

        if let Some(binary) = self.arena.get_binary_expr(node) {
            return self.count_es5_static_class_expression_temps(binary.left)
                + self.count_es5_static_class_expression_temps(binary.right);
        }

        if let Some(call) = self.arena.get_call_expr(node) {
            let callee_count = self.count_es5_static_class_expression_temps(call.expression);
            let args_count = call.arguments.as_ref().map_or(0, |args| {
                args.nodes
                    .iter()
                    .copied()
                    .map(|arg| self.count_es5_static_class_expression_temps(arg))
                    .sum()
            });
            return callee_count + args_count;
        }

        if let Some(paren) = self.arena.get_parenthesized(node) {
            return self.count_es5_static_class_expression_temps(paren.expression);
        }

        if let Some(assertion) = self.arena.get_type_assertion(node) {
            return self.count_es5_static_class_expression_temps(assertion.expression);
        }

        if let Some(unary) = self.arena.get_unary_expr(node) {
            return self.count_es5_static_class_expression_temps(unary.operand);
        }

        if let Some(unary) = self.arena.get_unary_expr_ex(node) {
            return self.count_es5_static_class_expression_temps(unary.expression);
        }

        if let Some(cond) = self.arena.get_conditional_expr(node) {
            return self.count_es5_static_class_expression_temps(cond.condition)
                + self.count_es5_static_class_expression_temps(cond.when_true)
                + self.count_es5_static_class_expression_temps(cond.when_false);
        }

        if let Some(literal) = self.arena.get_literal_expr(node) {
            return literal
                .elements
                .nodes
                .iter()
                .copied()
                .map(|element| self.count_es5_static_class_expression_temps(element))
                .sum();
        }

        self.arena
            .get_children(expression)
            .into_iter()
            .map(|child| self.count_es5_static_class_expression_temps(child))
            .sum()
    }

    /// Like `emit_es5_array_binding_element` but with first flag for separator control
    pub(in crate::emitter) fn emit_es5_array_binding_element_direct(
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
                self.write_binding_identifier_text(elem.name);
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
            self.write_binding_identifier_text(elem.name);
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
            self.write_binding_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        }
    }

    pub(in crate::emitter) fn emit_es5_destructuring_pattern(
        &mut self,
        pattern_node: &Node,
        temp_name: &str,
    ) {
        if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
                return;
            };
            let mut rest_props = Vec::new();
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
                } else if let Some(rest_prop) = self.emit_es5_binding_element(elem_idx, temp_name) {
                    rest_props.push(rest_prop);
                }
            }
        } else if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            && let Some(pattern) = self.arena.get_binding_pattern(pattern_node)
        {
            if self.array_pattern_needs_deferred_elements(pattern) {
                self.emit_es5_array_binding_elements_with_deferred_object_rest(pattern, temp_name);
            } else {
                for (i, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                    self.emit_es5_array_binding_element(elem_idx, temp_name, i);
                }
            }
        }
    }

    fn emit_es5_array_binding_elements_with_deferred_object_rest(
        &mut self,
        pattern: &tsz_parser::parser::node::BindingPatternData,
        temp_name: &str,
    ) {
        let mut pending: Vec<DeferredArrayElement> = Vec::new();
        let mut has_deferred_prior = false;

        for (i, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
            if self.array_element_needs_deferred_emit(elem_idx, has_deferred_prior) {
                has_deferred_prior = true;
                if let Some(deferred) =
                    self.emit_es5_array_binding_element_head(elem_idx, temp_name, i)
                {
                    pending.push(deferred);
                }
            } else {
                self.emit_es5_array_binding_element(elem_idx, temp_name, i);
            }
        }

        for deferred in pending {
            self.emit_es5_array_deferred_element(deferred);
        }
    }

    fn emit_es5_array_binding_elements_direct_with_deferred_object_rest(
        &mut self,
        pattern: &tsz_parser::parser::node::BindingPatternData,
        ident_name: &str,
        first: &mut bool,
    ) {
        let mut pending: Vec<DeferredArrayElement> = Vec::new();
        let mut has_deferred_prior = false;

        for (i, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
            if self.array_element_needs_deferred_emit(elem_idx, has_deferred_prior) {
                has_deferred_prior = true;
                if let Some(deferred) =
                    self.emit_es5_array_binding_element_head_direct(elem_idx, ident_name, i, first)
                {
                    pending.push(deferred);
                }
            } else {
                self.emit_es5_array_binding_element_direct(elem_idx, ident_name, i, first);
            }
        }

        for deferred in pending {
            self.emit_es5_array_deferred_element(deferred);
        }
    }

    /// Returns true when an array binding pattern contains an object-rest
    /// binding that must be split out into a deferred binding assignment.
    fn array_pattern_needs_deferred_elements(
        &self,
        pattern: &tsz_parser::parser::node::BindingPatternData,
    ) -> bool {
        for &elem_idx in &pattern.elements.nodes {
            if self.array_element_contains_object_rest(elem_idx) {
                return true;
            }
        }
        false
    }

    fn array_element_needs_deferred_emit(
        &self,
        elem_idx: NodeIndex,
        has_deferred_prior: bool,
    ) -> bool {
        if self.array_element_contains_object_rest(elem_idx) {
            return true;
        }

        has_deferred_prior && !self.is_simple_array_binding_element(elem_idx)
    }

    fn array_element_contains_object_rest(&self, elem_idx: NodeIndex) -> bool {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return false;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return false;
        };

        self.binding_target_contains_object_rest(elem.name)
    }

    pub(super) fn binding_target_contains_object_rest(&self, target_idx: NodeIndex) -> bool {
        let Some(target_node) = self.arena.get(target_idx) else {
            return false;
        };

        match target_node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                let Some(pattern) = self.arena.get_binding_pattern(target_node) else {
                    return false;
                };
                for &elem_idx in &pattern.elements.nodes {
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };
                    let Some(elem) = self.arena.get_binding_element(elem_node) else {
                        continue;
                    };
                    if elem.dot_dot_dot_token || self.binding_target_contains_object_rest(elem.name)
                    {
                        return true;
                    }
                }
                false
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                let Some(pattern) = self.arena.get_binding_pattern(target_node) else {
                    return false;
                };
                pattern
                    .elements
                    .nodes
                    .iter()
                    .any(|&elem_idx| self.array_element_contains_object_rest(elem_idx))
            }
            _ => false,
        }
    }

    fn is_simple_array_binding_element(&self, elem_idx: NodeIndex) -> bool {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return true;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return true;
        };

        self.is_simple_binding_element(elem)
    }

    fn is_simple_binding_element(&self, elem: &BindingElementData) -> bool {
        if let Some(property_name) = self.get_binding_element_property_key(elem)
            && !self.is_literal_property_name(property_name)
        {
            return false;
        }

        if elem.initializer.is_some() && !self.is_simple_inlineable_expression(elem.initializer) {
            return false;
        }

        self.is_simple_binding_target(elem.name)
    }

    fn is_simple_binding_target(&self, target_idx: NodeIndex) -> bool {
        if target_idx.is_none() {
            return true;
        }

        let Some(target_node) = self.arena.get(target_idx) else {
            return true;
        };

        match target_node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                let Some(pattern) = self.arena.get_binding_pattern(target_node) else {
                    return true;
                };
                pattern.elements.nodes.iter().all(|&elem_idx| {
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        return true;
                    };
                    let Some(elem) = self.arena.get_binding_element(elem_node) else {
                        return true;
                    };
                    self.is_simple_binding_element(elem)
                })
            }
            k if k == SyntaxKind::Identifier as u16 => true,
            _ => false,
        }
    }

    pub(super) fn is_literal_property_name(&self, property_name: NodeIndex) -> bool {
        let Some(property_node) = self.arena.get(property_name) else {
            return true;
        };
        matches!(
            property_node.kind,
            k if k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        )
    }

    fn is_simple_inlineable_expression(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        let Some(kind) = SyntaxKind::try_from_u16(expr_node.kind) else {
            return false;
        };

        matches!(
            kind,
            SyntaxKind::StringLiteral
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::NumericLiteral
                | SyntaxKind::BigIntLiteral
        ) || (kind >= SyntaxKind::FIRST_KEYWORD && kind <= SyntaxKind::LAST_KEYWORD)
    }

    /// Like `emit_es5_destructuring_pattern` but handles the `first` flag for the first
    /// non-omitted element, allowing it to be emitted without a `, ` prefix.
    /// Used when the initializer is a simple identifier and no temp variable is needed.
    pub(in crate::emitter) fn emit_es5_destructuring_pattern_direct(
        &mut self,
        pattern_node: &Node,
        ident_name: &str,
        first: &mut bool,
    ) {
        if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
                return;
            };
            let mut rest_props = Vec::new();
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
                } else if let Some(rest_prop) =
                    self.emit_es5_binding_element_direct(elem_idx, ident_name, first)
                {
                    rest_props.push(rest_prop);
                }
            }
        } else if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            && let Some(pattern) = self.arena.get_binding_pattern(pattern_node)
        {
            if self.array_pattern_needs_deferred_elements(pattern) {
                self.emit_es5_array_binding_elements_direct_with_deferred_object_rest(
                    pattern, ident_name, first,
                );
            } else {
                for (i, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                    self.emit_es5_array_binding_element_direct(elem_idx, ident_name, i, first);
                }
            }
        }
    }
}
