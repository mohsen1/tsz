use super::is_valid_identifier_name;
use super::{ParamTransform, ParamTransformPlan, Printer, RestParamTransform};
use crate::transform_context::TransformDirective;
use crate::transforms::ClassES5Emitter;
use std::sync::Arc;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{MethodDeclData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Segment of an array literal for ES5 spread transformation
enum ArraySegment<'a> {
    /// Non-spread elements: [1, 2, 3]
    Elements(&'a [NodeIndex]),
    /// Spread element: ...arr
    Spread(NodeIndex),
}

/// Segment of an object literal for ES5 spread transformation
enum ObjectSegment<'a> {
    /// Non-spread elements: regular and computed properties
    Elements(&'a [NodeIndex]),
    /// Spread element: ...obj
    Spread(NodeIndex),
}

impl<'a> Printer<'a> {
    /// Emit an array literal with ES5 spread transformation.
    /// Uses TypeScript's __spreadArray helper for exact tsc matching.
    /// Pattern: [...a] -> __spreadArray([], a, true)
    /// Pattern: [...a, 1] -> __spreadArray(a, [1], false)
    /// Pattern: [1, ...a] -> __spreadArray([1], a, false)
    /// Pattern: [1, ...a, 2] -> __spreadArray([1], a, false).concat([2])
    pub(super) fn emit_array_literal_es5(&mut self, elements: &[NodeIndex]) {
        if elements.is_empty() {
            self.write("[]");
            return;
        }

        let wrap_spread_with_read = false;

        // Split array into segments by spread elements
        let mut segments: Vec<ArraySegment> = Vec::new();
        let mut current_start = 0;

        for (i, &elem_idx) in elements.iter().enumerate() {
            if self.is_spread_element(elem_idx) {
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

        // Emit using __spreadArray for exact tsc matching
        match segments.as_slice() {
            [] => {
                // Should not happen due to empty check above
                self.write("[]");
            }
            [ArraySegment::Elements(elems)] => {
                // No spreads, emit normally
                self.write("[");
                self.emit_comma_separated(elems);
                self.write("]");
            }
            [ArraySegment::Spread(spread_idx)] => {
                // Only a spread element: [...a] -> __spreadArray([], a, true)
                self.write("__spreadArray([], ");
                if let Some(spread_node) = self.arena.get(*spread_idx) {
                    self.emit_spread_expression_with_read(spread_node, wrap_spread_with_read);
                }
                self.write(", true)");
            }
            [
                ArraySegment::Spread(spread_idx),
                ArraySegment::Elements(elems),
            ] => {
                // Spread first, then elements: [...a, 1, 2] -> __spreadArray(a, [1, 2], false)
                self.write("__spreadArray(");
                if let Some(spread_node) = self.arena.get(*spread_idx) {
                    self.emit_spread_expression_with_read(spread_node, wrap_spread_with_read);
                }
                self.write(", ");
                self.write("[");
                self.emit_comma_separated(elems);
                self.write("]");
                self.write(", false)");
            }
            [
                ArraySegment::Elements(elems),
                ArraySegment::Spread(spread_idx),
            ] => {
                // Elements first, then spread: [1, 2, ...a] -> __spreadArray([1, 2], a, false)
                self.write("__spreadArray(");
                self.write("[");
                self.emit_comma_separated(elems);
                self.write("]");
                self.write(", ");
                if let Some(spread_node) = self.arena.get(*spread_idx) {
                    self.emit_spread_expression_with_read(spread_node, wrap_spread_with_read);
                }
                self.write(", false)");
            }
            [
                ArraySegment::Elements(prefix_elems),
                ArraySegment::Spread(spread_idx),
                ArraySegment::Elements(suffix_elems),
            ] => {
                // Elements, spread, elements: [1, ...a, 2] -> __spreadArray([1], a, false).concat([2])
                self.write("__spreadArray(");
                self.write("[");
                self.emit_comma_separated(prefix_elems);
                self.write("]");
                self.write(", ");
                if let Some(spread_node) = self.arena.get(*spread_idx) {
                    self.emit_spread_expression_with_read(spread_node, wrap_spread_with_read);
                }
                self.write(", false)");
                // Append suffix with concat
                if !suffix_elems.is_empty() {
                    self.write(".concat(");
                    self.write("[");
                    self.emit_comma_separated(suffix_elems);
                    self.write("]");
                    self.write(")");
                }
            }
            [first, rest @ ..] => {
                // Fallback for more complex patterns: use concat chain
                self.emit_array_segment(first);
                for segment in rest {
                    self.write(".concat(");
                    match segment {
                        ArraySegment::Elements(elems) => {
                            self.write("[");
                            self.emit_comma_separated(elems);
                            self.write("]");
                        }
                        ArraySegment::Spread(spread_idx) => {
                            if let Some(spread_node) = self.arena.get(*spread_idx) {
                                self.emit_spread_expression_with_read(
                                    spread_node,
                                    wrap_spread_with_read,
                                );
                            }
                        }
                    }
                    self.write(")");
                }
            }
        }
    }

    fn is_spread_element(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            || node.kind == syntax_kind_ext::SPREAD_ELEMENT
    }

    fn emit_spread_expression(&mut self, node: &Node) {
        // Get the expression inside the spread element
        if let Some(spread) = self.arena.get_spread(node) {
            self.emit(spread.expression);
        }
    }

    fn emit_spread_expression_with_read(&mut self, node: &Node, wrap_with_read: bool) {
        if let Some(spread) = self.arena.get_spread(node) {
            if wrap_with_read {
                self.write("__read(");
                self.emit(spread.expression);
                self.write(")");
            } else {
                self.emit(spread.expression);
            }
        }
    }

    fn emit_array_segment(&mut self, segment: &ArraySegment) {
        match segment {
            ArraySegment::Elements(elems) => {
                self.write("[");
                self.emit_comma_separated(elems);
                self.write("]");
            }
            ArraySegment::Spread(_) => {
                // This should not happen as the first segment
                // [...a] case is handled differently
                self.write("[]");
            }
        }
    }

    pub(super) fn emit_object_literal_entries_es5(&mut self, elements: &[NodeIndex]) {
        if elements.is_empty() {
            self.write("{}");
            return;
        }

        if elements.len() > 1 {
            self.write("{");
            self.write_line();
            self.increase_indent();
            for (i, &prop) in elements.iter().enumerate() {
                self.emit_object_literal_member_es5(prop);
                if i < elements.len() - 1 {
                    self.write(",");
                }
                self.write_line();
            }
            self.decrease_indent();
            self.write("}");
        } else {
            self.write("{ ");
            self.emit_object_literal_member_es5(elements[0]);
            self.write(" }");
        }
    }

    pub(super) fn emit_object_literal_member_es5(&mut self, prop_idx: NodeIndex) {
        let Some(node) = self.arena.get(prop_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(shorthand) = self.arena.get_shorthand_property(node) {
                    self.emit(shorthand.name);
                    self.write(": ");
                    self.emit(shorthand.name);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    self.emit(method.name);
                    self.write(": ");
                    self.emit_object_literal_method_value_es5(method);
                }
            }
            _ => self.emit(prop_idx),
        }
    }

    pub(super) fn emit_object_literal_method_value_es5(&mut self, method: &MethodDeclData) {
        if method.body.is_none() {
            self.write("function () { }");
            return;
        }

        let is_async = self.has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword as u16);
        if is_async {
            self.emit_async_function_es5_body("", &method.parameters.nodes, method.body, "this");
            return;
        }

        self.write("function");
        if method.asterisk_token {
            self.write("*");
        }
        self.write(" (");
        let param_transforms = self.emit_function_parameters_es5(&method.parameters.nodes);
        self.write(") ");
        if param_transforms.has_transforms() {
            self.emit_block_with_param_prologue(method.body, &param_transforms);
        } else {
            self.emit(method.body);
        }
        self.pop_temp_scope();
    }

    /// Check if a property member has a computed property name
    pub(super) fn is_computed_property_member(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        let name_idx = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                self.arena.get_property_assignment(node).map(|p| p.name)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.arena.get_method_decl(node).map(|m| m.name)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.arena.get_accessor(node).map(|a| a.name)
            }
            _ => None,
        };

        if let Some(name_idx) = name_idx
            && let Some(name_node) = self.arena.get(name_idx)
        {
            return name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME;
        }
        false
    }

    /// Emit ES5-compatible object literal with computed properties and spread
    /// Uses TypeScript's __assign helper for exact tsc matching.
    ///
    /// Spread patterns:
    /// - { ...a } → __assign({}, a)
    /// - { a: 1, ...b } → __assign({ a: 1 }, b)
    /// - { ...a, b: 1 } → __assign(__assign({}, a), { b: 1 })
    /// - { a: 1, ...b, c: 2 } → __assign(__assign({ a: 1 }, b), { c: 2 })
    ///
    /// Computed properties (without spread):
    /// - { [k]: v } → (_a = {}, _a[k] = v, _a)
    /// - { a: 1, [k]: v } → (_a = { a: 1 }, _a[k] = v, _a)
    ///
    /// Mixed computed and spread:
    /// - { [k]: v, ...a } → __assign((_a = {}, _a[k] = v, _a), a)
    pub(super) fn emit_object_literal_es5(
        &mut self,
        elements: &[NodeIndex],
        source_range: Option<(u32, u32)>,
    ) {
        if elements.is_empty() {
            self.write("{}");
            return;
        }

        // Check if we have any spread elements
        let has_spread = elements.iter().any(|&idx| self.is_spread_element(idx));

        if !has_spread {
            // No spread - use the old computed property logic
            self.emit_object_literal_without_spread_es5(elements, source_range);
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
    ) {
        let first_computed_idx = elements
            .iter()
            .position(|&idx| self.is_computed_property_member(idx))
            .unwrap_or(elements.len());

        if first_computed_idx == elements.len() {
            self.emit_object_literal_entries_es5(elements);
            return;
        }

        // Get hoisted temp variable name
        let temp_var = self.make_unique_name_hoisted();

        let source_is_multiline = source_range.is_some_and(|(pos, end)| {
            self.source_text.is_some_and(|text| {
                let start = pos as usize;
                let end = end as usize;
                start < end && end <= text.len() && text[start..end].contains('\n')
            })
        });
        let single_computed_only = first_computed_idx == 0 && elements.len() == 1;
        if single_computed_only && source_is_multiline {
            self.write("(");
            self.increase_indent();
            self.write(&temp_var);
            self.write(" = ");
        } else {
            self.write("(");
            self.write(&temp_var);
            self.write(" = ");
        }

        // Emit initial non-computed properties as the object literal
        if first_computed_idx > 0 {
            self.emit_object_literal_entries_es5(&elements[..first_computed_idx]);
        } else {
            self.write("{}");
        }

        // Emit remaining properties as assignments
        for prop_idx in elements.iter().skip(first_computed_idx) {
            if single_computed_only && source_is_multiline {
                self.write(",");
                self.write_line();
            } else {
                self.write(", ");
            }
            self.emit_property_assignment_es5(*prop_idx, &temp_var);
        }

        // Return the temp variable
        if single_computed_only && source_is_multiline {
            self.write(",");
            self.write_line();
            self.write(&temp_var);
            self.decrease_indent();
            self.write(")");
        } else {
            self.write(", ");
            self.write(&temp_var);
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
            if self.is_spread_element(elem_idx) {
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
                    .any(|&idx| self.is_computed_property_member(idx));
                if has_computed {
                    self.emit_object_literal_without_spread_es5(elems, source_range);
                } else {
                    self.emit_object_literal_entries_es5(elems);
                }
            }
            [ObjectSegment::Spread(spread_idx)] => {
                // Only a spread element: { ...a } → __assign({}, a)
                self.write("__assign({}, ");
                if let Some(spread_node) = self.arena.get(*spread_idx) {
                    self.emit_spread_expression(spread_node);
                }
                self.write(")");
            }
            [
                ObjectSegment::Elements(elems),
                ObjectSegment::Spread(spread_idx),
            ] => {
                // Elements then spread: { a: 1, ...b } → __assign({ a: 1 }, b)
                let has_computed = elems
                    .iter()
                    .any(|&idx| self.is_computed_property_member(idx));
                if has_computed {
                    // Need temp var for computed properties
                    let temp_var = self.make_unique_name_hoisted();
                    self.write("__assign((");
                    self.write(&temp_var);
                    self.write(" = ");
                    self.emit_object_literal_entries_es5(elems);
                    self.write(", ");
                    self.write(&temp_var);
                    self.write("), ");
                } else {
                    self.write("__assign(");
                    self.emit_object_literal_entries_es5(elems);
                    self.write(", ");
                }
                if let Some(spread_node) = self.arena.get(*spread_idx) {
                    self.emit_spread_expression(spread_node);
                }
                if has_computed {
                    self.write(")");
                }
                self.write(")");
            }
            [
                ObjectSegment::Spread(spread_idx),
                ObjectSegment::Elements(elems),
            ] => {
                // Spread then elements: { ...a, b: 1 } → __assign(__assign({}, a), { b: 1 })
                self.write("__assign(__assign({}, ");
                if let Some(spread_node) = self.arena.get(*spread_idx) {
                    self.emit_spread_expression(spread_node);
                }
                self.write("), ");
                self.emit_object_literal_entries_es5(elems);
                self.write(")");
            }
            [first, rest @ ..] => {
                // Complex pattern: use Prefix-Wrap strategy for proper nested __assign
                // Example: { a: 1, ...b, c: 2, ...d }
                // Result: __assign(__assign(__assign({ a: 1 }, b), { c: 2 }), d)

                let total_segments = 1 + rest.len();
                let first_is_spread = matches!(first, ObjectSegment::Spread(_));

                // 1. Emit the necessary number of __assign( calls
                let num_assigns = if first_is_spread {
                    total_segments
                } else {
                    total_segments - 1
                };

                for _ in 0..num_assigns {
                    self.write("__assign(");
                }

                // 2. Handle the first segment
                match first {
                    ObjectSegment::Elements(elems) => {
                        let has_computed = elems
                            .iter()
                            .any(|&idx| self.is_computed_property_member(idx));
                        if has_computed {
                            // Use temp var for computed properties
                            let temp_var = self.ctx.destructuring_state.next_temp_var();
                            self.write("(");
                            self.write(&temp_var);
                            self.write(" = ");
                            self.emit_object_literal_entries_es5(elems);
                            for elem in *elems {
                                if self.is_computed_property_member(*elem) {
                                    self.write(", ");
                                    self.emit_property_assignment_es5(*elem, &temp_var);
                                }
                            }
                            self.write(", ");
                            self.write(&temp_var);
                            self.write(")");
                        } else {
                            self.emit_object_literal_entries_es5(elems);
                        }
                    }
                    ObjectSegment::Spread(spread_idx) => {
                        self.write("{}, ");
                        if let Some(spread_node) = self.arena.get(*spread_idx) {
                            self.emit_spread_expression(spread_node);
                        }
                        self.write(")");
                    }
                }

                // 3. Handle subsequent segments
                for segment in rest {
                    self.write(", ");
                    match segment {
                        ObjectSegment::Elements(elems) => {
                            let has_computed = elems
                                .iter()
                                .any(|&idx| self.is_computed_property_member(idx));
                            if has_computed {
                                let temp_var = self.ctx.destructuring_state.next_temp_var();
                                self.write("(");
                                self.write(&temp_var);
                                self.write(" = ");
                                self.emit_object_literal_entries_es5(elems);
                                for elem in *elems {
                                    if self.is_computed_property_member(*elem) {
                                        self.write(", ");
                                        self.emit_property_assignment_es5(*elem, &temp_var);
                                    }
                                }
                                self.write(", ");
                                self.write(&temp_var);
                                self.write(")");
                            } else if !elems.is_empty() {
                                self.emit_object_literal_entries_es5(elems);
                            } else {
                                self.write("{}");
                            }
                        }
                        ObjectSegment::Spread(spread_idx) => {
                            if let Some(spread_node) = self.arena.get(*spread_idx) {
                                self.emit_spread_expression(spread_node);
                            }
                        }
                    }
                    self.write(")");
                }
            }
        }
    }

    /// Emit a property assignment in ES5 computed property transform
    pub(super) fn emit_property_assignment_es5(&mut self, prop_idx: NodeIndex, temp_var: &str) {
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
                    self.write_identifier_text(shorthand.name);
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    self.emit_assignment_target_es5(method.name, temp_var);
                    self.write(" = ");
                    self.emit_object_literal_method_value_es5(method);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    self.write("Object.defineProperty(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit_property_key_string(accessor.name);
                    self.write(", { get: function () ");
                    self.emit(accessor.body);
                    self.write(", enumerable: false, configurable: true })");
                }
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    self.write("Object.defineProperty(");
                    self.write(temp_var);
                    self.write(", ");
                    self.emit_property_key_string(accessor.name);
                    self.write(", { set: function (");
                    self.emit_function_parameters_js(&accessor.parameters.nodes);
                    self.write(") ");
                    self.emit(accessor.body);
                    self.write(", enumerable: false, configurable: true })");
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
    pub(super) fn emit_assignment_target_es5(&mut self, name_idx: NodeIndex, temp_var: &str) {
        self.emit_assignment_target_es5_with_computed(name_idx, temp_var, None);
    }

    /// Emit assignment target for ES5 computed property transform with optional computed temp
    /// For computed: _a[_temp] (if `computed_temp` is Some)
    /// For regular: _a.name
    pub(super) fn emit_assignment_target_es5_with_computed(
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
    pub(super) fn emit_property_key_string(&mut self, name_idx: NodeIndex) {
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

    /// Emit ES5-compatible function expression for arrow function
    /// Arrow: (x) => x + 1  →  function (x) { return x + 1; }
    ///
    /// When `class_alias` is Some (arrow in static member), use class alias capture:
    /// var _a = Vector; _a.foo = () => _a;
    ///
    /// Otherwise use IIFE capture:
    /// (function (_this) { return _this.x; })(this)
    pub(super) fn emit_arrow_function_es5(
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
            // Arrow functions don't have their own `this`. In ES5 lowering:
            // - If body uses `this`: capture with `_this` and pass to __awaiter
            // - If body doesn't use `this`: pass `void 0` to __awaiter
            let this_expr = if _captures_this { "_this" } else { "void 0" };
            // TSC wraps async arrow→function conversions inline:
            // function () { return __awaiter(void 0, ..., function () { ... }); };
            self.emit_async_arrow_es5_inline(func, this_expr);
        } else {
            self.write("function (");
            let param_transforms = self.emit_function_parameters_es5(&func.parameters.nodes);
            self.write(") ");

            // If body is not a block (concise arrow), wrap with return
            let body_node = self.arena.get(func.body);
            let is_block = body_node.is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
            let needs_param_prologue = param_transforms.has_transforms();

            if is_block {
                // Check if it's a simple single-return block
                if let Some(block_node) = self.arena.get(func.body) {
                    if let Some(block) = self.arena.get_block(block_node) {
                        if block.statements.nodes.is_empty() {
                            self.write("{");
                            self.write_line();
                            self.write("}");
                            return;
                        }
                        if !needs_param_prologue
                            && block.statements.nodes.len() == 1
                            && self.is_simple_return_statement(block.statements.nodes[0])
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
                if needs_parens {
                    self.write("return (");
                    self.emit(func.body);
                    self.write(");");
                } else {
                    self.write("return ");
                    self.emit(func.body);
                    self.write(";");
                }
                self.write_line();
                self.decrease_indent();
                self.write("}");
            } else {
                // Concise body: (x) => x + 1  →  function (x) { return x + 1; }
                // If the body is (or resolves to) an object literal, wrap in parens
                // to disambiguate from a block: () => ({})  →  function () { return ({}); }
                let needs_parens = self.concise_body_needs_parens(func.body);
                if needs_parens {
                    self.write("{ return (");
                    self.emit(func.body);
                    self.write("); }");
                } else {
                    self.write("{ return ");
                    self.emit(func.body);
                    self.write("; }");
                }
            }
            self.pop_temp_scope();
        }
    }

    /// Check if a concise arrow body resolves to an object literal expression
    /// and needs wrapping in parens. Returns false if already parenthesized
    /// (to avoid double-parens). Unwraps through type assertions and as-expressions.
    pub(super) fn concise_body_needs_parens(&self, body_idx: NodeIndex) -> bool {
        let mut idx = body_idx;
        loop {
            let Some(node) = self.arena.get(idx) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => return true,
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION =>
                {
                    if let Some(ta) = self.arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return false;
                    }
                }
                // Already parenthesized — the emitter will preserve the parens
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => return false,
                _ => return false,
            }
        }
    }

    pub(super) fn emit_function_expression_es5_params(&mut self, node: &Node) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        self.write("function");

        if func.asterisk_token {
            self.write("*");
        }

        // Name (if any)
        if !func.name.is_none() {
            self.write_space();
            self.emit(func.name);
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

        if param_transforms.has_transforms() {
            self.emit_block_with_param_prologue(func.body, &param_transforms);
        } else if is_simple_body {
            self.emit_single_line_block(func.body);
        } else {
            self.emit(func.body);
        }
        self.pop_temp_scope();
    }

    pub(super) fn emit_function_declaration_es5_params(&mut self, node: &Node) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        // Skip ambient declarations (declare function)
        if self.has_declare_modifier(&func.modifiers) {
            return;
        }

        // For JavaScript emit: skip declaration-only functions (no body)
        if func.body.is_none() {
            return;
        }

        self.write("function");

        if func.asterisk_token {
            self.write("*");
        }

        // Name
        if !func.name.is_none() {
            self.write_space();
            self.emit(func.name);
        }

        // Parameters - only emit names, not types for JavaScript
        self.write("(");
        let param_transforms = self.emit_function_parameters_es5(&func.parameters.nodes);
        self.write(")");

        // No return type for JavaScript

        self.write_space();
        if param_transforms.has_transforms() {
            self.emit_block_with_param_prologue(func.body, &param_transforms);
        } else {
            self.emit(func.body);
        }
        self.pop_temp_scope();
    }

    /// Emit an ES5 async arrow function with inline body wrapping.
    /// TSC format: `function () { return __awaiter(void 0, ..., function () { ... }); };`
    fn emit_async_arrow_es5_inline(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        this_expr: &str,
    ) {
        self.push_temp_scope();
        self.write("function (");
        // ES5: apply destructuring/default transforms
        let param_transforms = self.emit_function_parameters_es5(&func.parameters.nodes);
        let has_param_transforms = param_transforms.has_transforms();

        if has_param_transforms {
            // If parameters need transforms (destructuring, defaults), fall back to
            // multi-line format since we need prologue statements
            self.write(") {");
            self.write_line();
            self.increase_indent();
            self.emit_param_prologue(&param_transforms);
        }

        // Build the __generator body
        let mut async_emitter = crate::transforms::async_es5::AsyncES5Emitter::new(self.arena);
        async_emitter.set_indent_level(self.writer.indent_level() + 1);
        if let Some(text) = self.source_text_for_map() {
            async_emitter.set_source_map_context(text, self.writer.current_source_index());
        }
        async_emitter.set_lexical_this(this_expr != "this");

        let body_has_await = async_emitter.body_contains_await(func.body);
        let generator_body = if body_has_await {
            async_emitter.emit_generator_body_with_await(func.body)
        } else {
            async_emitter.emit_simple_generator_body(func.body)
        };
        let generator_mappings = async_emitter.take_mappings();

        if has_param_transforms {
            // Multi-line path (with param prologue)
            self.write("return __awaiter(");
            self.write(this_expr);
            self.write(", void 0, void 0, function () {");
            self.write_line();
            self.increase_indent();
            self.write(&generator_body);
            self.decrease_indent();
            self.write_line();
            self.write("});");
            self.write_line();
            self.decrease_indent();
            self.write("}");
        } else {
            // Inline path: function () { return __awaiter(...); };
            self.write(") { return __awaiter(");
            self.write(this_expr);
            self.write(", void 0, void 0, function () {");
            self.write_line();
            self.increase_indent();
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
        self.pop_temp_scope();
    }

    /// Emit an async function transformed to ES5 __awaiter/__generator pattern
    pub(super) fn emit_async_function_es5(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        func_name: &str,
        this_expr: &str,
    ) {
        self.emit_async_function_es5_body(func_name, &func.parameters.nodes, func.body, this_expr);
    }

    pub(super) fn emit_async_function_es5_body(
        &mut self,
        func_name: &str,
        params: &[NodeIndex],
        body: NodeIndex,
        this_expr: &str,
    ) {
        // For ES2015/ES2016 targets, use function* + yield pattern
        // For ES5, use function + __generator state machine pattern
        let use_native_generators = !self.ctx.target_es5;
        let params_have_top_level_await = params
            .iter()
            .copied()
            .any(|p| self.param_initializer_has_top_level_await(p));
        let move_params_to_generator = use_native_generators && params_have_top_level_await;
        let es5_await_param_recovery = !use_native_generators
            && params_have_top_level_await
            && self.block_is_empty(body)
            && self.first_await_default_param_name(params).is_some();

        // function name(params) { ... } or function (params) { ... }
        if func_name.is_empty() {
            self.write("function (");
        } else {
            self.write("function ");
            self.write(func_name);
            self.write("(");
        }
        if use_native_generators {
            // ES2015: when a parameter initializer starts with `await`, match tsc
            // by moving parameters to the inner generator and forwarding `arguments`.
            if !move_params_to_generator {
                self.emit_function_parameters_js(params);
            }
        } else {
            if es5_await_param_recovery {
                self.write(") {");
                self.write_line();
                self.increase_indent();

                self.write("return __awaiter(");
                self.write(this_expr);
                self.write(", arguments, void 0, function (");
                self.emit_function_parameter_names_only(params);
                self.write(") {");
                self.write_line();
                self.increase_indent();

                if let Some(param_name) = self.first_await_default_param_name(params) {
                    self.write("if (");
                    self.write(&param_name);
                    self.write(" === void 0) { ");
                    self.write(&param_name);
                    self.write(" = _a.sent(); }");
                    self.write_line();
                }

                self.write("return __generator(this, function (_a) {");
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
                return;
            }

            // ES5: apply destructuring/default transforms
            let param_transforms = self.emit_function_parameters_es5(params);
            self.write(") {");
            self.write_line();
            self.increase_indent();
            self.emit_param_prologue(&param_transforms);

            // ES5 path: __awaiter + __generator state machine
            let mut async_emitter = crate::transforms::async_es5::AsyncES5Emitter::new(self.arena);
            async_emitter.set_indent_level(self.writer.indent_level() + 1);
            if let Some(text) = self.source_text_for_map() {
                async_emitter.set_source_map_context(text, self.writer.current_source_index());
            }
            async_emitter.set_lexical_this(this_expr != "this");

            let body_has_await = async_emitter.body_contains_await(body);
            let hoist_function_decls_only =
                !body_has_await && self.block_has_only_function_decls(body);
            if hoist_function_decls_only {
                self.write("return __awaiter(");
                self.write(this_expr);
                self.write(", void 0, void 0, function () {");
                self.write_line();
                self.increase_indent();

                if let Some(body_node) = self.arena.get(body)
                    && let Some(block) = self.arena.get_block(body_node)
                {
                    for &stmt in &block.statements.nodes {
                        if let Some(stmt_node) = self.arena.get(stmt) {
                            let actual_start =
                                self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                            self.emit_comments_before_pos(actual_start);
                        }
                        self.emit(stmt);
                        self.write_line();
                    }
                }

                self.write("return __generator(this, function (_a) {");
                self.write_line();
                self.increase_indent();
                self.write("return [2 /*return*/];");
                self.write_line();
                self.decrease_indent();
                self.write("});");
                self.decrease_indent();
                self.write_line();
                self.write("});");
                self.write_line();
                self.decrease_indent();
                self.write("}");
                self.pop_temp_scope();
                return;
            }
            if !body_has_await
                && let Some(body_node) = self.arena.get(body)
                && let Some(block) = self.arena.get_block(body_node)
                && let Some(&first_stmt_idx) = block.statements.nodes.first()
                && let Some(first_stmt_node) = self.arena.get(first_stmt_idx)
                && first_stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            {
                let actual_start =
                    self.skip_trivia_forward(first_stmt_node.pos, first_stmt_node.end);
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].end <= actual_start
                {
                    self.comment_emit_idx += 1;
                }
            }

            let generator_body = if body_has_await {
                async_emitter.emit_generator_body_with_await(body)
            } else {
                async_emitter.emit_simple_generator_body(body)
            };
            let generator_mappings = async_emitter.take_mappings();

            // Write with surrounding __awaiter wrapper
            self.write("return __awaiter(");
            self.write(this_expr);
            self.write(", void 0, void 0, function () {");
            self.write_line();
            self.increase_indent();
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
            self.pop_temp_scope();
            return;
        }

        // ES2015 path: __awaiter + function* with yield

        // Check if the body is empty and was single-line in source for compact formatting
        let body_is_empty_single_line = self
            .arena
            .get(body)
            .and_then(|n| {
                let block = self.arena.get_block(n)?;
                if block.statements.nodes.is_empty() {
                    Some(self.is_single_line(n))
                } else {
                    None
                }
            })
            .unwrap_or(false);

        self.write(") {");
        self.write_line();
        self.increase_indent();

        // return __awaiter(this, void 0, void 0, function* () {
        self.write("return __awaiter(");
        self.write(this_expr);
        if move_params_to_generator {
            self.write(", arguments, void 0, function* (");
            let saved = self.ctx.emit_await_as_yield;
            self.ctx.emit_await_as_yield = true;
            self.emit_function_parameters_js(params);
            self.ctx.emit_await_as_yield = saved;
            if body_is_empty_single_line {
                self.write(") { });");
            } else {
                self.write(") {");
            }
        } else if body_is_empty_single_line {
            self.write(", void 0, void 0, function* () { });");
        } else {
            self.write(", void 0, void 0, function* () {");
        }

        if body_is_empty_single_line {
            self.write_line();
            self.decrease_indent();
            self.write("}");
            return;
        }

        self.write_line();
        self.increase_indent();

        // Emit function body with await→yield substitution
        self.ctx.emit_await_as_yield = true;
        // Emit the block body's statements directly
        if let Some(body_node) = self.arena.get(body)
            && let Some(block) = self.arena.get_block(body_node)
        {
            for &stmt in &block.statements.nodes {
                if let Some(stmt_node) = self.arena.get(stmt) {
                    let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                    self.emit_comments_before_pos(actual_start);
                }
                self.emit(stmt);
                self.write_line();
            }
        }
        self.ctx.emit_await_as_yield = false;

        self.decrease_indent();
        self.write("});");
        self.write_line();
        self.decrease_indent();
        self.write("}");
    }

    fn block_has_only_function_decls(&self, body: NodeIndex) -> bool {
        let Some(body_node) = self.arena.get(body) else {
            return false;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return false;
        };
        if block.statements.nodes.is_empty() {
            return false;
        }
        block.statements.nodes.iter().all(|&stmt_idx| {
            self.arena
                .get(stmt_idx)
                .is_some_and(|stmt_node| stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION)
        })
    }

    fn param_initializer_has_top_level_await(&self, param_idx: NodeIndex) -> bool {
        let Some(param_node) = self.arena.get(param_idx) else {
            return false;
        };
        let Some(param) = self.arena.get_parameter(param_node) else {
            return false;
        };
        if param.initializer.is_none() {
            return false;
        }
        let Some(init_node) = self.arena.get(param.initializer) else {
            return false;
        };
        init_node.kind == syntax_kind_ext::AWAIT_EXPRESSION
    }

    fn first_await_default_param_name(&self, params: &[NodeIndex]) -> Option<String> {
        for &param_idx in params {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if param.initializer.is_none() {
                continue;
            }
            let Some(init_node) = self.arena.get(param.initializer) else {
                continue;
            };
            if init_node.kind != syntax_kind_ext::AWAIT_EXPRESSION {
                continue;
            }
            let Some(name_node) = self.arena.get(param.name) else {
                continue;
            };
            if name_node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let name = self.get_identifier_text(param.name);
            if !name.is_empty() {
                return Some(name);
            }
        }
        None
    }

    fn emit_function_parameter_names_only(&mut self, params: &[NodeIndex]) {
        let mut first = true;
        for &param_idx in params {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if !first {
                self.write(", ");
            }
            first = false;
            if param.dot_dot_dot_token {
                self.write("...");
            }
            self.emit(param.name);
        }
    }

    fn block_is_empty(&self, body: NodeIndex) -> bool {
        let Some(body_node) = self.arena.get(body) else {
            return false;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return false;
        };
        block.statements.nodes.is_empty()
    }

    pub(super) fn emit_function_parameters_es5(
        &mut self,
        params: &[NodeIndex],
    ) -> ParamTransformPlan {
        // Push a fresh temp scope for this function.
        // Each function has its own temp naming starting from _a.
        // Caller MUST call pop_temp_scope() after emitting the function body.
        self.push_temp_scope();

        let mut plan = ParamTransformPlan::default();
        let mut first = true;

        for (index, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            if param.dot_dot_dot_token {
                let rest_target = param.name;
                let rest_is_pattern = self.is_binding_pattern(rest_target);
                let rest_name = if rest_is_pattern {
                    self.get_temp_var_name()
                } else {
                    self.get_identifier_text(rest_target)
                };

                if !rest_name.is_empty() {
                    plan.rest = Some(RestParamTransform {
                        name: rest_name,
                        pattern: rest_is_pattern.then_some(rest_target),
                        index,
                    });
                }
                break;
            }

            if !first {
                self.write(", ");
            }
            first = false;

            if self.is_binding_pattern(param.name) {
                let temp_name = self.get_temp_var_name();
                self.write(&temp_name);
                plan.params.push(ParamTransform {
                    name: temp_name,
                    pattern: Some(param.name),
                    initializer: if param.initializer.is_none() {
                        None
                    } else {
                        Some(param.initializer)
                    },
                });
            } else {
                self.emit(param.name);
                if !param.initializer.is_none() {
                    let name = self.get_identifier_text(param.name);
                    if !name.is_empty() {
                        plan.params.push(ParamTransform {
                            name,
                            pattern: None,
                            initializer: Some(param.initializer),
                        });
                    }
                }
            }
        }

        plan
    }

    /// Emit an ES5-compatible class expression by wrapping the class IIFE in an expression.
    pub(super) fn emit_class_expression_es5(&mut self, class_node: NodeIndex) {
        let Some(node) = self.arena.get(class_node) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(node) else {
            return;
        };

        let mut es5_emitter = ClassES5Emitter::new(self.arena);
        es5_emitter.set_indent_level(0);
        // Pass transform directives to the ClassES5Emitter
        es5_emitter.set_transforms(self.transforms.clone());
        if let Some(text) = self.source_text_for_map() {
            if self.writer.has_source_map() {
                es5_emitter.set_source_map_context(text, self.writer.current_source_index());
            } else {
                es5_emitter.set_source_text(text);
            }
        }

        let (class_name, es5_output) = if !class_data.name.is_none() {
            let candidate = self.get_identifier_text(class_data.name);
            if candidate.is_empty() || !is_valid_identifier_name(&candidate) {
                let temp_name = self.get_temp_var_name();
                let output = es5_emitter.emit_class_with_name(class_node, &temp_name);
                (temp_name, output)
            } else {
                let output = es5_emitter.emit_class(class_node);
                (candidate, output)
            }
        } else {
            let temp_name = self.get_temp_var_name();
            let output = es5_emitter.emit_class_with_name(class_node, &temp_name);
            (temp_name, output)
        };
        let es5_mappings = es5_emitter.take_mappings();

        self.write("(function () {");
        self.write_line();
        self.increase_indent();

        if !es5_mappings.is_empty() && self.writer.has_source_map() {
            let base_line = self.writer.current_line();
            let column_offset = self.writer.indent_width();
            self.writer.add_mappings_with_line_column_offset(
                base_line,
                column_offset,
                &es5_mappings,
            );
        }

        for line in es5_output.lines() {
            if !line.is_empty() {
                self.write(line);
            }
            self.write_line();
        }

        self.write("return ");
        self.write(&class_name);
        self.write(";");
        self.write_line();

        self.decrease_indent();
        self.write("})()");
    }

    pub(super) fn has_es5_transforms(&self) -> bool {
        self.transforms
            .iter()
            .any(|(_, directive)| Self::directive_has_es5(directive))
    }

    pub(super) fn directive_has_es5(directive: &TransformDirective) -> bool {
        match directive {
            TransformDirective::ES5Class { .. }
            | TransformDirective::ES5ClassExpression { .. }
            | TransformDirective::ES5Namespace { .. }
            | TransformDirective::ES5Enum { .. }
            | TransformDirective::ES5ArrowFunction { .. }
            | TransformDirective::ES5AsyncFunction { .. }
            | TransformDirective::ES5ForOf { .. }
            | TransformDirective::ES5ObjectLiteral { .. }
            | TransformDirective::ES5VariableDeclarationList { .. }
            | TransformDirective::ES5FunctionParameters { .. }
            | TransformDirective::ES5TemplateLiteral { .. }
            | TransformDirective::CommonJSExportDefaultClassES5 { .. } => true,
            TransformDirective::CommonJSExport { inner, .. } => Self::directive_has_es5(inner),
            TransformDirective::Chain(directives) => directives.iter().any(Self::directive_has_es5),
            _ => false,
        }
    }

    pub(super) fn tagged_template_var_name(&self, idx: NodeIndex) -> String {
        format!("__templateObject_{}", idx.0)
    }

    pub(super) fn collect_tagged_template_vars(&self) -> Vec<String> {
        if self.transforms.helpers_populated() {
            return self.collect_tagged_template_vars_from_transforms();
        }

        let mut vars = Vec::new();
        for (idx, node) in self.arena.nodes.iter().enumerate() {
            if node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
                vars.push(self.tagged_template_var_name(NodeIndex(idx as u32)));
            }
        }
        vars
    }

    pub(super) fn collect_tagged_template_vars_from_transforms(&self) -> Vec<String> {
        let mut vars = Vec::new();
        for (&idx, directive) in self.transforms.iter() {
            if !matches!(directive, TransformDirective::ES5TemplateLiteral { .. }) {
                continue;
            }

            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
                vars.push(self.tagged_template_var_name(idx));
            }
        }
        vars
    }

    /// Emit a call expression with spread arguments transformed for ES5
    ///
    /// Examples:
    /// - `foo(...arr)` -> `foo.apply(void 0, arr)`
    /// - `foo(...arr, 1, 2)` -> `foo.apply(void 0, __spreadArray(__spreadArray([], arr, false), [1, 2], false))`
    /// - `obj.method(...arr)` -> `obj.method.apply(obj, arr)`
    pub(super) fn emit_call_expression_es5_spread(&mut self, node: &Node) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        let optional_call_token =
            self.has_optional_call_token_in_spread(node, call.expression, call.arguments.as_ref());

        let Some(ref args) = call.arguments else {
            // No arguments - shouldn't happen if we detected spread
            self.emit(call.expression);
            self.write("()");
            return;
        };

        // Check if this is a method call (property access)
        let callee_node = self.arena.get(call.expression);
        let is_method_call =
            callee_node.is_some_and(|n| n.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION);

        if is_method_call {
            if optional_call_token {
                self.emit_optional_method_call_with_spread(call.expression, args, true);
            } else {
                self.emit_method_call_with_spread(call.expression, args);
            }
        } else if optional_call_token {
            self.emit_optional_function_call_with_spread(call.expression, args);
        } else {
            self.emit_function_call_with_spread(call.expression, args);
        }
    }

    fn has_optional_call_token_in_spread(
        &self,
        node: &Node,
        callee: NodeIndex,
        args: Option<&tsz_parser::parser::NodeList>,
    ) -> bool {
        let Some(source) = self.source_text_for_map() else {
            let Some(callee_node) = self.arena.get(callee) else {
                return false;
            };
            return self.arena.get_access_expr(callee_node).is_none();
        };

        let Some(callee_node) = self.arena.get(callee) else {
            return false;
        };
        let Some(open_paren) = self.find_open_paren_position_optional_call(node, args) else {
            return false;
        };
        let bytes = source.as_bytes();
        let mut i = std::cmp::min(open_paren as usize, source.len());
        let start = std::cmp::min(callee_node.pos as usize, source.len());

        while i > start {
            if i == 0 {
                break;
            }
            match bytes[i - 1] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    i -= 1;
                }
                b'/' if i >= 2 && bytes[i - 2] == b'/' => {
                    while i > start && bytes[i - 1] != b'\n' {
                        i -= 1;
                    }
                    if i > start {
                        i -= 1;
                    }
                }
                b'/' if i >= 2 && bytes[i - 2] == b'*' => {
                    if i >= 2 {
                        i -= 2;
                    }
                    while i >= 2 && !(bytes[i - 2] == b'*' && bytes[i - 1] == b'/') {
                        i -= 1;
                    }
                    if i >= 2 {
                        i -= 2;
                    }
                }
                b'?' if i >= 2 && bytes[i - 2] == b'.' => {
                    return true;
                }
                b'.' if i >= 2 && bytes[i - 2] == b'?' && bytes[i - 1] == b'.' => {
                    return true;
                }
                _ => return false,
            }
        }

        false
    }

    fn find_open_paren_position_optional_call(
        &self,
        node: &Node,
        args: Option<&tsz_parser::parser::NodeList>,
    ) -> Option<u32> {
        let text = self.source_text_for_map()?;
        let bytes = text.as_bytes();
        let start = std::cmp::min(node.pos as usize, bytes.len());
        let mut end = std::cmp::min(node.end as usize, bytes.len());
        if let Some(args) = args
            && let Some(first_arg) = args.nodes.first()
            && let Some(first_node) = self.arena.get(*first_arg)
        {
            end = std::cmp::min(first_node.pos as usize, end);
        }
        (start..end)
            .position(|i| bytes[i] == b'(')
            .map(|offset| (start + offset) as u32)
    }

    fn emit_optional_function_call_with_spread(
        &mut self,
        callee_idx: NodeIndex,
        args: &tsz_parser::parser::NodeList,
    ) {
        let temp = self.get_temp_var_name();
        self.write("(");
        self.write(&temp);
        self.write(" = ");
        self.emit(callee_idx);
        self.write(")");
        self.write(" === null || ");
        self.write(&temp);
        self.write(" === void 0 ? void 0 : ");
        self.write(&temp);
        self.write(".apply(void 0, ");
        self.emit_spread_args_array(&args.nodes);
        self.write(")");
    }

    fn emit_optional_method_call_with_spread(
        &mut self,
        access_idx: NodeIndex,
        args: &tsz_parser::parser::NodeList,
        has_optional_call_token: bool,
    ) {
        // obj.method?.(...args) -> obj.method.call.apply(obj, [args]) with optional checks
        let Some(access_node) = self.arena.get(access_idx) else {
            return;
        };
        let Some(access) = self.arena.get_access_expr(access_node) else {
            return;
        };

        if !has_optional_call_token {
            let this_temp = self.get_temp_var_name();
            self.write("(");
            self.write(&this_temp);
            self.write(" = ");
            self.emit(access.expression);
            self.write(")");
            if access.question_dot_token {
                self.write(" === null || ");
                self.write(&this_temp);
                self.write(" === void 0 ? void 0 : ");
            }

            if access_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.write(".");
                self.emit(access.name_or_argument);
            } else {
                self.write("[");
                self.emit(access.name_or_argument);
                self.write("]");
            }
            self.write(".apply(");
            self.write(&this_temp);
            self.write(", ");
            self.emit_spread_args_array(&args.nodes);
            self.write(")");
            return;
        }

        let this_temp = self.get_temp_var_name();
        let method_temp = self.get_temp_var_name();

        self.write("(");
        self.write(&method_temp);
        self.write(" = ");
        self.write("(");
        self.write(&this_temp);
        self.write(" = ");
        self.emit(access.expression);
        self.write(")");
        if access.question_dot_token {
            self.write(" === null || ");
            self.write(&this_temp);
            self.write(" === void 0 ? void 0 : ");
        }
        if access_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if access.question_dot_token {
                self.write(&this_temp);
            }
            self.write(".");
            self.emit(access.name_or_argument);
        } else {
            if access.question_dot_token {
                self.write(&this_temp);
            }
            self.write("[");
            self.emit(access.name_or_argument);
            self.write("]");
        }
        self.write(") === null || ");
        self.write(&method_temp);
        self.write(" === void 0 ? void 0 : ");
        self.write(&method_temp);
        self.write(".call.apply(");
        self.write(&method_temp);
        self.write(", ");
        self.write("__spreadArray([");
        self.write(&this_temp);
        self.write("], ");
        self.emit_spread_args_array(&args.nodes);
        self.write(", false)");
        self.write(")");
    }

    fn emit_function_call_with_spread(
        &mut self,
        callee_idx: NodeIndex,
        args: &tsz_parser::parser::NodeList,
    ) {
        // foo(...args) -> foo.apply(void 0, args_array)
        self.emit(callee_idx);
        self.write(".apply(void 0, ");
        self.emit_spread_args_array(&args.nodes);
        self.write(")");
    }

    fn emit_method_call_with_spread(
        &mut self,
        access_idx: NodeIndex,
        args: &tsz_parser::parser::NodeList,
    ) {
        // obj.method(...args) -> obj.method.apply(obj, args_array)
        let Some(access_node) = self.arena.get(access_idx) else {
            return;
        };
        let Some(access) = self.arena.get_access_expr(access_node) else {
            return;
        };

        // Emit: obj.method.apply(obj, args_array)
        self.emit(access.expression);
        self.write(".");
        self.emit(access.name_or_argument);
        self.write(".apply(");
        self.emit(access.expression);
        self.write(", ");
        self.emit_spread_args_array(&args.nodes);
        self.write(")");
    }

    fn emit_spread_args_array(&mut self, args: &[NodeIndex]) {
        // Build arguments array using __spreadArray for spread elements
        if args.is_empty() {
            self.write("[]");
            return;
        }

        // Check if there are any spread elements
        let has_spread = args.iter().any(|&arg_idx| self.is_spread_element(arg_idx));

        if !has_spread {
            // No spreads, just emit an array literal
            self.write("[");
            self.emit_comma_separated(args);
            self.write("]");
            return;
        }

        // Build segments by grouping consecutive non-spread and spread elements
        let mut segments: Vec<ArraySegment> = Vec::new();
        let mut current_start = 0;

        for (i, &arg_idx) in args.iter().enumerate() {
            if self.is_spread_element(arg_idx) {
                // Add non-spread segment before this spread
                if current_start < i {
                    segments.push(ArraySegment::Elements(&args[current_start..i]));
                }
                // Add the spread element
                segments.push(ArraySegment::Spread(arg_idx));
                current_start = i + 1;
            }
        }

        // Add remaining elements after last spread
        if current_start < args.len() {
            segments.push(ArraySegment::Elements(&args[current_start..]));
        }

        // Emit using nested __spreadArray calls
        self.emit_spread_segments(&segments);
    }

    fn emit_spread_segments(&mut self, segments: &[ArraySegment]) {
        if segments.is_empty() {
            self.write("[]");
            return;
        }

        if segments.len() == 1 {
            match &segments[0] {
                ArraySegment::Spread(spread_idx) => {
                    // Just a single spread with no other arguments:
                    // TypeScript optimization - pass the array directly without __spreadArray
                    // Example: foo(...args) -> foo.apply(void 0, args)
                    // NOT: foo.apply(void 0, __spreadArray([], args, false))
                    if let Some(spread_node) = self.arena.get(*spread_idx) {
                        self.emit_spread_expression(spread_node);
                    }
                }
                ArraySegment::Elements(elems) => {
                    // Just elements: [1, 2, 3]
                    self.write("[");
                    self.emit_comma_separated(elems);
                    self.write("]");
                }
            }
            return;
        }

        // Multiple segments: build nested __spreadArray calls
        // Pattern: __spreadArray(__spreadArray(base, segment1, false), segment2, false)

        // Open __spreadArray calls for all but the last segment
        for _ in 0..segments.len() - 1 {
            self.write("__spreadArray(");
        }

        // Emit the first segment as a complete unit
        match &segments[0] {
            ArraySegment::Elements(elems) => {
                self.write("[");
                self.emit_comma_separated(elems);
                self.write("]");
            }
            ArraySegment::Spread(spread_idx) => {
                // First segment is spread: emit as __spreadArray([], spread, false)
                self.write("__spreadArray([], ");
                if let Some(spread_node) = self.arena.get(*spread_idx) {
                    self.emit_spread_expression(spread_node);
                }
                self.write(", false)");
            }
        }

        // Emit remaining segments - each closes one __spreadArray call
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
                        self.emit_spread_expression(spread_node);
                    }
                    self.write(", false)");
                }
            }
        }
    }
}
