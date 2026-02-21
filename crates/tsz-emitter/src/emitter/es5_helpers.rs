use super::Printer;
use std::sync::Arc;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{MethodDeclData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Segment of an array literal for ES5 spread transformation
pub(super) enum ArraySegment<'a> {
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

    pub(super) fn is_spread_element(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            || node.kind == syntax_kind_ext::SPREAD_ELEMENT
    }

    pub(super) fn emit_spread_expression(&mut self, node: &Node) {
        // Get the expression inside the spread element
        if let Some(spread) = self.arena.get_spread(node) {
            self.emit(spread.expression);
        }
    }

    pub(super) fn emit_spread_expression_with_read(&mut self, node: &Node, wrap_with_read: bool) {
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
        if func.name.is_some() {
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
        if func.name.is_some() {
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
}
