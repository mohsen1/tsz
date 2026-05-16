//! Binding Pattern Emission Module
//!
//! This module handles emission of destructuring binding patterns.
//! Includes object binding patterns, array binding patterns, and binding elements.

use super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;

/// Represents how a property name should appear in the __rest exclude list.
enum ExcludedProp {
    /// Identifier property: emit as `"name"`
    Identifier(String),
    /// String literal property: emit as `'name'`
    StringLiteral(String),
    /// Dynamic computed property: emit as `typeof _temp === "symbol" ? _temp : _temp + ""`
    /// Stores the temp variable name assigned to the key expression.
    Dynamic(String),
}

enum ArrayObjectRestDeferred {
    ObjectRest {
        pattern: NodeIndex,
        temp: String,
        initializer: NodeIndex,
    },
    BindingPattern {
        pattern: NodeIndex,
        temp: String,
        initializer: NodeIndex,
    },
    SimpleDefault {
        name: NodeIndex,
        temp: String,
        initializer: NodeIndex,
    },
}

impl<'a> Printer<'a> {
    // =========================================================================
    // Binding Patterns
    // =========================================================================

    /// Emit an object binding pattern: { x, y }
    pub(super) fn emit_object_binding_pattern(&mut self, node: &Node) {
        let Some(pattern) = self.arena.get_binding_pattern(node) else {
            return;
        };

        if pattern.elements.nodes.is_empty() {
            self.write("{}");
            return;
        }

        let has_trailing_comma = self.has_trailing_comma_in_source(node, &pattern.elements.nodes);
        self.write("{ ");
        self.emit_comma_separated(&pattern.elements.nodes);
        if has_trailing_comma {
            self.write(",");
        }
        self.write(" }");
    }

    /// Emit an array binding pattern: [x, y]
    pub(super) fn emit_array_binding_pattern(&mut self, node: &Node) {
        let Some(pattern) = self.arena.get_binding_pattern(node) else {
            return;
        };

        let has_trailing_comma = self.has_trailing_comma_in_source(node, &pattern.elements.nodes);
        self.write("[");
        // Emit any inline comments between `[` and the first element
        // (e.g., `[/*comment*/ a]` in catch destructuring)
        if let Some(&first_elem) = pattern.elements.nodes.first()
            && let Some(elem_node) = self.arena.get(first_elem)
        {
            self.emit_comments_before_pos(elem_node.pos);
            if self.pending_block_comment_space {
                self.write_space();
                self.pending_block_comment_space = false;
            }
        }
        self.emit_comma_separated(&pattern.elements.nodes);
        if has_trailing_comma {
            self.write(",");
        }
        self.write("]");
    }

    /// Emit a binding element: x or x = default or propertyName: x
    pub(super) fn emit_binding_element(&mut self, node: &Node) {
        let Some(elem) = self.arena.get_binding_element(node) else {
            return;
        };

        // Rest element: ...x
        if elem.dot_dot_dot_token {
            self.write("...");
            if let Some(name_node) = self.arena.get(elem.name) {
                self.emit_comments_after_dot_dot_dot(node.pos, name_node.pos, false);
            }
        }

        // propertyName: name  or just name
        // When the source explicitly wrote `{ x: x }`, the parser sets
        // `property_name` even though it matches `name`. TSC always preserves
        // the explicit form, so we must emit `property_name: name` whenever
        // `property_name` is set — never collapse to shorthand.
        if elem.property_name.is_some() {
            self.emit(elem.property_name);
            self.write(": ");
            self.emit_decl_name(elem.name);
        } else if self.binding_name_requires_property_assignment(elem.name) {
            self.emit(elem.name);
            self.write(": ");
        } else {
            self.emit_decl_name(elem.name);
        }

        // Default value: = expr
        if elem.initializer.is_some() {
            self.write(" = ");
            self.emit(elem.initializer);
        }
    }

    fn binding_name_requires_property_assignment(&self, name: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name) else {
            return false;
        };
        if name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            return self
                .arena
                .get_identifier(name_node)
                .and_then(|ident| tsz_scanner::text_to_keyword(&ident.escaped_text))
                .is_some_and(tsz_scanner::token_is_reserved_word);
        }

        name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
            || name_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
    }

    // =========================================================================
    // Binding Pattern Utilities
    // =========================================================================

    /// Get the next temporary variable name.
    /// Uses the unified `make_unique_name` to ensure collision-free names
    /// across both destructuring and for-of lowering.
    pub(super) fn get_temp_var_name(&mut self) -> String {
        self.make_unique_name()
    }

    /// Check if a node is a binding pattern
    pub(super) fn is_binding_pattern(&self, idx: NodeIndex) -> bool {
        self.arena.get(idx).is_some_and(|n| n.is_binding_pattern())
    }

    // =========================================================================
    // ES2018 Object Rest Lowering
    // =========================================================================
    // For targets < ES2018, object rest patterns `{ a, ...rest }` must be
    // lowered to `__rest()` calls. Unlike full ES5 destructuring, the non-rest
    // part of the binding is preserved as-is.
    //
    // Examples:
    //   var { a, ...rest } = obj;
    //     → var { a } = obj, rest = __rest(obj, ["a"]);
    //
    //   var { ...clone } = obj;
    //     → var clone = __rest(obj, []);
    //
    //   var { a: { b, ...nested }, ...rest } = obj;
    //     → var _a = obj.a, { b } = _a, nested = __rest(_a, ["b"]),
    //       rest = __rest(obj, ["a"]);

    /// Check if a variable declaration's binding pattern contains an object
    /// rest element (at any nesting level).
    pub(super) fn decl_has_object_rest(&self, decl_idx: NodeIndex) -> bool {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        self.pattern_has_object_rest(decl.name)
    }

    /// Check if a binding pattern (recursively) contains an object rest element.
    pub(super) fn pattern_has_object_rest(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            let Some(pattern) = self.arena.get_binding_pattern(node) else {
                return false;
            };
            for &elem_idx in &pattern.elements.nodes {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    continue;
                };
                if let Some(elem) = self.arena.get_binding_element(elem_node) {
                    if elem.dot_dot_dot_token {
                        return true;
                    }
                    // Check nested patterns
                    if self.pattern_has_object_rest(elem.name) {
                        return true;
                    }
                }
            }
        } else if node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            let Some(pattern) = self.arena.get_binding_pattern(node) else {
                return false;
            };
            for &elem_idx in &pattern.elements.nodes {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    continue;
                };
                if let Some(elem) = self.arena.get_binding_element(elem_node) {
                    // Check nested patterns (array rest doesn't need __rest,
                    // but nested object patterns within arrays might)
                    if self.pattern_has_object_rest(elem.name) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if a function parameter has an object rest pattern.
    pub(super) fn param_has_object_rest(&self, param_idx: NodeIndex) -> bool {
        let Some(param_node) = self.arena.get(param_idx) else {
            return false;
        };
        let Some(param) = self.arena.get_parameter(param_node) else {
            return false;
        };
        self.pattern_has_object_rest(param.name)
    }

    /// Emit a variable declaration that has been identified as having object rest
    /// patterns. Splits the rest element into a separate `__rest()` call.
    ///
    /// Input:  `{ a, b: renamed, ...rest } = obj`
    /// Output: `{ a, b: renamed } = obj, rest = __rest(obj, ["a", "b"])`
    pub(super) fn emit_var_decl_with_object_rest(&mut self, decl_idx: NodeIndex) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };

        let Some(name_node) = self.arena.get(decl.name) else {
            return;
        };

        match name_node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                self.emit_object_rest_var_decl(decl.name, decl.initializer, None);
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                if decl.initializer.is_some() {
                    self.emit_array_object_rest_var_decl_from_initializer(
                        decl.name,
                        decl.initializer,
                    );
                } else {
                    self.emit_decl_name(decl.name);
                }
            }
            _ => {
                self.emit_decl_name(decl.name);
                if decl.initializer.is_some() {
                    self.write(" = ");
                    self.emit_expression(decl.initializer);
                }
            }
        }
    }

    /// Core: emit an object binding pattern with rest lowered.
    /// `source_expr` is the expression on the RHS (or a temp var name).
    /// `source_temp` is an optional already-assigned temp variable name.
    ///
    /// Emits: `{ nonRest1, nonRest2 } = SOURCE, restName = __rest(SOURCE, ["nonRest1", "nonRest2"])`
    /// For nested patterns with rest, introduces temps as needed.
    pub(super) fn emit_object_rest_var_decl(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
        source_temp: Option<&str>,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        // Collect non-rest elements and find the rest element
        let mut non_rest_elements: Vec<NodeIndex> = Vec::new();
        let mut rest_element: Option<NodeIndex> = None;
        let mut excluded_props: Vec<ExcludedProp> = Vec::new();
        // Track which non-rest elements have nested object rest
        let mut nested_rest_indices: Vec<usize> = Vec::new();
        // Whether any element has a dynamic computed property name
        let mut has_dynamic_computed = false;
        let mut saw_invalid_nonlast_rest = false;

        for (index, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };

            if elem.dot_dot_dot_token {
                let has_later_element = pattern
                    .elements
                    .nodes
                    .iter()
                    .skip(index + 1)
                    .any(|idx| !idx.is_none());
                if has_later_element {
                    saw_invalid_nonlast_rest = true;
                    let invalid_rest_name = self.get_identifier_text(elem.name);
                    if !invalid_rest_name.is_empty() {
                        excluded_props.push(ExcludedProp::Identifier(invalid_rest_name));
                    }
                    continue;
                }
                rest_element = Some(elem_idx);
                continue;
            }

            // Get the property name for the exclude list
            let (prop_name, is_static_computed) =
                self.get_binding_element_property_name_info(elem_idx);

            // Check if this element's name has a nested object rest
            let has_nested_rest = self.pattern_has_object_rest(elem.name);
            if has_nested_rest {
                nested_rest_indices.push(non_rest_elements.len());
            }

            if prop_name.is_empty() && self.has_computed_property_name(elem_idx) {
                // Dynamic computed property — will need temp var for key
                has_dynamic_computed = true;
                // Placeholder — actual temp will be assigned during emission
                excluded_props.push(ExcludedProp::Dynamic(String::new()));
            } else if !prop_name.is_empty() {
                let is_string_literal_prop =
                    is_static_computed || self.is_string_literal_property_name(elem_idx);
                if is_string_literal_prop {
                    excluded_props.push(ExcludedProp::StringLiteral(prop_name));
                } else {
                    excluded_props.push(ExcludedProp::Identifier(prop_name));
                }
            }

            non_rest_elements.push(elem_idx);
        }

        // If any element has a dynamic computed property, we must fully destructure
        // manually (no `{ }` pattern syntax).
        if has_dynamic_computed && rest_element.is_some() {
            self.emit_object_rest_with_dynamic_computed(
                &non_rest_elements,
                rest_element,
                initializer_idx,
                source_temp,
            );
            return;
        }

        if rest_element.is_none() && nested_rest_indices.is_empty() {
            if !non_rest_elements.is_empty() {
                if let Some(temp) = source_temp {
                    self.emit_object_pattern_without_rest(&non_rest_elements);
                    self.write(" = ");
                    self.write(temp);
                } else if initializer_idx.is_some() {
                    let can_reuse_initializer = self
                        .arena
                        .get(initializer_idx)
                        .is_some_and(|n| n.kind == tsz_scanner::SyntaxKind::Identifier as u16);
                    if saw_invalid_nonlast_rest && !can_reuse_initializer {
                        let source_name = self.get_temp_var_name();
                        self.write(&source_name);
                        self.write(" = ");
                        self.emit_expression(initializer_idx);
                        self.write(", ");
                        self.emit_object_pattern_without_rest(&non_rest_elements);
                        self.write(" = ");
                        self.write(&source_name);
                    } else {
                        self.emit_object_pattern_without_rest(&non_rest_elements);
                        self.write(" = ");
                        self.emit_expression(initializer_idx);
                    }
                } else {
                    self.emit_object_pattern_without_rest(&non_rest_elements);
                }
            }
            return;
        }

        // Determine if we need a temp variable for the source. We need one whenever
        // anything below the outer pattern requires the source: either a rest element
        // at this level, or a nested element whose own pattern carries an object rest
        // (e.g. `{ f: { a, ...spread } } = value` — outer has no rest, but the nested
        // pattern under `f:` does, so the lowering still needs to thread `value` /
        // `value.f` through `__rest`).
        let needs_temp =
            (rest_element.is_some() || !nested_rest_indices.is_empty()) && source_temp.is_none();
        let source_name: String;
        let mut emitted_prefix = false;

        if needs_temp {
            // Check if initializer is a simple identifier we can reuse
            let can_reuse = initializer_idx.is_some()
                && self
                    .arena
                    .get(initializer_idx)
                    .is_some_and(|n| n.kind == tsz_scanner::SyntaxKind::Identifier as u16);

            if can_reuse {
                // Reuse the identifier name
                source_name = self.get_identifier_text(initializer_idx);
                if !non_rest_elements.is_empty() && !nested_rest_indices.is_empty() {
                    self.emit_object_rest_with_nested(
                        &non_rest_elements,
                        &nested_rest_indices,
                        &source_name,
                    );
                    emitted_prefix = true;
                } else if !non_rest_elements.is_empty() {
                    self.emit_object_pattern_without_rest(&non_rest_elements);
                    self.write(" = ");
                    self.emit_expression(initializer_idx);
                    emitted_prefix = true;
                }
            } else {
                // Need a temp variable
                if non_rest_elements.is_empty() && initializer_idx.is_some() {
                    // Only rest: no temp is needed because the initializer can be
                    // passed directly into __rest().
                    source_name = String::new();
                } else if non_rest_elements.is_empty() {
                    // Only rest but no initializer: keep recovery output
                    // syntactically valid by avoiding an empty assignment RHS.
                    source_name = self.get_temp_var_name();
                    self.write(&source_name);
                    self.write(" = void 0");
                    emitted_prefix = true;
                } else if !nested_rest_indices.is_empty() {
                    source_name = self.get_temp_var_name();
                    self.write(&source_name.clone());
                    self.write(" = ");
                    self.emit_expression(initializer_idx);
                    self.write(", ");
                    self.emit_object_rest_with_nested(
                        &non_rest_elements,
                        &nested_rest_indices,
                        &source_name,
                    );
                    emitted_prefix = true;
                } else {
                    source_name = self.get_temp_var_name();
                    // Emit: temp = initializer, { nonRest } = temp
                    self.write(&source_name);
                    self.write(" = ");
                    self.emit_expression(initializer_idx);
                    self.write(", ");
                    self.emit_object_pattern_without_rest(&non_rest_elements);
                    self.write(" = ");
                    self.write(&source_name);
                    emitted_prefix = true;
                }
            }
        } else if let Some(temp) = source_temp {
            source_name = temp.to_string();
            if !non_rest_elements.is_empty() && !nested_rest_indices.is_empty() {
                self.emit_object_rest_with_nested(
                    &non_rest_elements,
                    &nested_rest_indices,
                    &source_name,
                );
                emitted_prefix = true;
            } else if !non_rest_elements.is_empty() {
                self.emit_object_pattern_without_rest(&non_rest_elements);
                self.write(" = ");
                self.write(&source_name);
                emitted_prefix = true;
            }
        } else {
            // No rest element - shouldn't happen but handle gracefully
            self.emit_decl_name(pattern_idx);
            if initializer_idx.is_some() {
                self.write(" = ");
                self.emit_expression(initializer_idx);
            }
            return;
        }

        // Emit the rest call: , restName = __rest(source, ["excluded1", "excluded2"])
        if let Some(rest_idx) = rest_element {
            let Some(rest_node) = self.arena.get(rest_idx) else {
                return;
            };
            let Some(rest_elem) = self.arena.get_binding_element(rest_node) else {
                return;
            };
            let rest_name = self.get_identifier_text(rest_elem.name);
            if !rest_name.is_empty() {
                if emitted_prefix {
                    self.write(", ");
                }
                self.write(&rest_name);
                self.write(" = ");
                self.write_helper("__rest");
                self.write("(");
                if source_name.is_empty() && initializer_idx.is_some() && source_temp.is_none() {
                    self.emit_expression(initializer_idx);
                } else {
                    self.write(&source_name);
                }
                self.write(", [");
                self.emit_excluded_props_list(&excluded_props);
                self.write("])");
            }
        }
    }

    /// Emit the exclude list items for a `__rest()` call.
    fn emit_excluded_props_list(&mut self, props: &[ExcludedProp]) {
        for (i, prop) in props.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            match prop {
                ExcludedProp::Identifier(name) => {
                    self.write("\"");
                    self.write(name);
                    self.write("\"");
                }
                ExcludedProp::StringLiteral(name) => {
                    self.write("'");
                    self.write(name);
                    self.write("'");
                }
                ExcludedProp::Dynamic(temp) => {
                    // typeof _temp === "symbol" ? _temp : _temp + ""
                    self.write("typeof ");
                    self.write(temp);
                    self.write(" === \"symbol\" ? ");
                    self.write(temp);
                    self.write(" : ");
                    self.write(temp);
                    self.write(" + \"\"");
                }
            }
        }
    }

    /// Emit fully manual destructuring for patterns with dynamic computed keys.
    /// `let { [k1]: a1, [k2]: a2, ...rest } = obj`
    /// → `let _a = obj, _b = k1, a1 = _a[_b], _c = k2, a2 = _a[_c],
    ///    rest = __rest(_a, [typeof _b === "symbol" ? _b : _b + "", ...])`
    fn emit_object_rest_with_dynamic_computed(
        &mut self,
        non_rest_elements: &[NodeIndex],
        rest_element: Option<NodeIndex>,
        initializer_idx: NodeIndex,
        source_temp: Option<&str>,
    ) {
        // For dynamic computed patterns, tsc always assigns the source to a temp,
        // even if it's a simple identifier. This ensures consistent naming.
        let source_name = if let Some(temp) = source_temp {
            temp.to_string()
        } else {
            let temp = self.get_temp_var_name();
            self.write(&temp);
            self.write(" = ");
            self.emit_expression(initializer_idx);
            if !non_rest_elements.is_empty() || rest_element.is_some() {
                self.write(", ");
            }
            temp
        };

        let mut excluded_props: Vec<ExcludedProp> = Vec::new();

        // Emit each non-rest element as manual property access
        for (i, &elem_idx) in non_rest_elements.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }

            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                continue;
            };

            let (static_name, is_static_computed) =
                self.get_binding_element_property_name_info(elem_idx);
            let is_dynamic = static_name.is_empty() && self.has_computed_property_name(elem_idx);

            if is_dynamic {
                // Dynamic computed key: assign key expr to temp, then access
                let key_temp = self.get_temp_var_name();
                self.write(&key_temp);
                self.write(" = ");
                // Emit the computed expression
                self.emit_computed_key_expression(elem_idx);
                self.write(", ");

                // Emit: varName = source[keyTemp]
                let var_name = self.get_identifier_text(elem.name);
                let value_name = if elem.initializer.is_some() {
                    self.get_temp_var_name()
                } else {
                    var_name.clone()
                };
                self.write(&value_name);
                self.write(" = ");
                self.write(&source_name);
                self.write("[");
                self.write(&key_temp);
                self.write("]");

                if self.is_binding_pattern(elem.name) {
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
                    excluded_props.push(ExcludedProp::Dynamic(key_temp));
                    continue;
                }

                if elem.initializer.is_some() {
                    self.write(", ");
                    self.write(&var_name);
                    self.write(" = ");
                    self.write(&value_name);
                    self.write(" === void 0 ? ");
                    self.emit_expression(elem.initializer);
                    self.write(" : ");
                    self.write(&value_name);
                }

                excluded_props.push(ExcludedProp::Dynamic(key_temp));
            } else {
                // Static property: emit as manual property access
                let var_name = self.get_identifier_text(elem.name);
                let prop_name = if static_name.is_empty() {
                    var_name.clone()
                } else {
                    static_name.clone()
                };

                let value_name = if elem.initializer.is_some() || self.is_binding_pattern(elem.name)
                {
                    self.get_temp_var_name()
                } else {
                    var_name.clone()
                };
                self.write(&value_name);
                self.write(" = ");
                self.write(&source_name);
                self.write(".");
                self.write(&prop_name);

                if self.is_binding_pattern(elem.name) {
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
                    let is_str_lit =
                        is_static_computed || self.is_string_literal_property_name(elem_idx);
                    if is_str_lit {
                        excluded_props.push(ExcludedProp::StringLiteral(prop_name));
                    } else {
                        excluded_props.push(ExcludedProp::Identifier(prop_name));
                    }
                    continue;
                }

                if elem.initializer.is_some() {
                    self.write(", ");
                    self.write(&var_name);
                    self.write(" = ");
                    self.write(&value_name);
                    self.write(" === void 0 ? ");
                    self.emit_expression(elem.initializer);
                    self.write(" : ");
                    self.write(&value_name);
                }

                let is_str_lit =
                    is_static_computed || self.is_string_literal_property_name(elem_idx);
                if is_str_lit {
                    excluded_props.push(ExcludedProp::StringLiteral(prop_name));
                } else {
                    excluded_props.push(ExcludedProp::Identifier(prop_name));
                }
            }
        }

        // Emit rest
        if let Some(rest_idx) = rest_element {
            let Some(rest_node) = self.arena.get(rest_idx) else {
                return;
            };
            let Some(rest_elem) = self.arena.get_binding_element(rest_node) else {
                return;
            };
            let rest_name = self.get_identifier_text(rest_elem.name);
            if !rest_name.is_empty() {
                if !non_rest_elements.is_empty() {
                    self.write(", ");
                }
                self.write(&rest_name);
                self.write(" = ");
                self.write_helper("__rest");
                self.write("(");
                self.write(&source_name);
                self.write(", [");
                self.emit_excluded_props_list(&excluded_props);
                self.write("])");
            }
        }
    }

    /// Emit an object binding pattern but skip the rest element.
    /// Used when lowering: `{ a, b, ...rest } = x` → `{ a, b } = x`
    pub(in crate::emitter) fn emit_object_pattern_without_rest(&mut self, elements: &[NodeIndex]) {
        if elements.is_empty() {
            self.write("{}");
            return;
        }
        self.write("{ ");
        for (i, &elem_idx) in elements.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            // Emit the binding element normally (but it won't have rest since
            // we filtered those out)
            self.emit(elem_idx);
        }
        self.write(" }");
    }

    /// Handle the case where non-rest elements have nested object rest.
    /// For example: `{ a: { b, ...nested }, c } = obj`
    /// We need to introduce temps for the nested parts.
    fn emit_object_rest_with_nested(
        &mut self,
        non_rest_elements: &[NodeIndex],
        nested_rest_indices: &[usize],
        source_name: &str,
    ) {
        // Emit non-rest elements that DON'T have nested rest normally
        let mut simple_elements: Vec<NodeIndex> = Vec::new();
        let mut first_extra = true;

        for (i, &elem_idx) in non_rest_elements.iter().enumerate() {
            if nested_rest_indices.contains(&i) {
                // This element has nested rest - emit the simple ones first if any
                if !simple_elements.is_empty() {
                    if !first_extra {
                        self.write(", ");
                    }
                    self.emit_object_pattern_without_rest(&simple_elements);
                    self.write(" = ");
                    self.write(source_name);
                    simple_elements.clear();
                    first_extra = false;
                }

                // Now handle the nested rest
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    continue;
                };
                let Some(elem) = self.arena.get_binding_element(elem_node) else {
                    continue;
                };

                // Get the property name to access on source
                let prop_name = self.get_binding_element_property_name_text(elem_idx);

                if !first_extra {
                    self.write(", ");
                }

                if self
                    .arena
                    .get(elem.name)
                    .is_some_and(|n| n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                {
                    let source = format!("{source_name}.{prop_name}");
                    self.emit_array_object_rest_var_decl_from_pattern(elem.name, &source);
                    first_extra = false;
                    continue;
                }

                // Create a temp for the nested source
                let nested_temp = self.get_temp_var_name();
                self.write(&nested_temp.clone());
                self.write(" = ");
                self.write(source_name);
                self.write(".");
                self.write(&prop_name);

                // Now emit the nested object rest pattern using the temp
                self.write(", ");
                self.emit_object_rest_var_decl_from_pattern(elem.name, &nested_temp);
                first_extra = false;
            } else {
                simple_elements.push(elem_idx);
            }
        }

        // Emit remaining simple elements
        if !simple_elements.is_empty() {
            if !first_extra {
                self.write(", ");
            }
            self.emit_object_pattern_without_rest(&simple_elements);
            self.write(" = ");
            self.write(source_name);
        }
    }

    /// Emit an object rest lowering for a pattern that's already assigned to a temp.
    fn emit_object_rest_var_decl_from_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        source_temp: &str,
    ) {
        let Some(node) = self.arena.get(pattern_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            self.emit_object_rest_var_decl(pattern_idx, NodeIndex::NONE, Some(source_temp));
        } else {
            // Not an object pattern - emit as-is
            self.emit_decl_name(pattern_idx);
            self.write(" = ");
            self.write(source_temp);
        }
    }

    /// Emit an array binding pattern that contains nested object rest.
    /// `[{ ...rest }, ...tail] = source` -> `[_a, ...tail] = source, rest = __rest(_a, [])`.
    fn emit_array_object_rest_var_decl_from_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        source: &str,
    ) {
        self.emit_array_object_rest_var_decl_from_source(pattern_idx, |printer| {
            printer.write(source);
        });
    }

    fn emit_array_object_rest_var_decl_from_initializer(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) {
        self.emit_array_object_rest_var_decl_from_source(pattern_idx, |printer| {
            printer.emit_expression(initializer_idx);
        });
    }

    fn emit_array_object_rest_var_decl_from_source(
        &mut self,
        pattern_idx: NodeIndex,
        emit_source: impl FnOnce(&mut Self),
    ) {
        let Some(node) = self.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern) = self.arena.get_binding_pattern(node) else {
            return;
        };

        let elements = pattern.elements.nodes.clone();
        let mut deferred = Vec::new();
        let mut has_deferred_prior = false;

        self.write("[");
        for (i, &elem_idx) in elements.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }

            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                self.emit(elem_idx);
                continue;
            };

            let contains_object_rest = self.pattern_has_object_rest(elem.name);
            let default_needs_deferred = elem.initializer.is_some()
                && !self.array_object_rest_is_simple_inlineable_expression(elem.initializer);
            let needs_deferred_after_prior = has_deferred_prior
                && (default_needs_deferred || self.is_binding_pattern(elem.name));

            if !elem.dot_dot_dot_token && (contains_object_rest || needs_deferred_after_prior) {
                let temp = self.get_temp_var_name();
                self.write(&temp);
                if contains_object_rest {
                    has_deferred_prior = true;
                    deferred.push(ArrayObjectRestDeferred::ObjectRest {
                        pattern: elem.name,
                        temp,
                        initializer: elem.initializer,
                    });
                } else if self.is_binding_pattern(elem.name) {
                    deferred.push(ArrayObjectRestDeferred::BindingPattern {
                        pattern: elem.name,
                        temp,
                        initializer: elem.initializer,
                    });
                } else {
                    deferred.push(ArrayObjectRestDeferred::SimpleDefault {
                        name: elem.name,
                        temp,
                        initializer: elem.initializer,
                    });
                }
            } else {
                self.emit(elem_idx);
            }
        }
        self.write("] = ");
        emit_source(self);

        for item in deferred {
            self.emit_array_object_rest_deferred(item);
        }
    }

    fn emit_array_object_rest_deferred(&mut self, item: ArrayObjectRestDeferred) {
        match item {
            ArrayObjectRestDeferred::ObjectRest {
                pattern,
                temp,
                initializer,
            } => {
                let source = self.emit_array_object_rest_default_temp(temp, initializer);
                self.write(", ");
                self.emit_object_rest_var_decl_from_pattern(pattern, &source);
            }
            ArrayObjectRestDeferred::BindingPattern {
                pattern,
                temp,
                initializer,
            } => {
                let source = self.emit_array_object_rest_default_temp(temp, initializer);
                self.write(", ");
                self.emit_decl_name(pattern);
                self.write(" = ");
                self.write(&source);
            }
            ArrayObjectRestDeferred::SimpleDefault {
                name,
                temp,
                initializer,
            } => {
                self.write(", ");
                self.write_identifier_text(name);
                self.write(" = ");
                self.write(&temp);
                self.write(" === void 0 ? ");
                self.emit_expression(initializer);
                self.write(" : ");
                self.write(&temp);
            }
        }
    }

    fn emit_array_object_rest_default_temp(
        &mut self,
        temp: String,
        initializer: NodeIndex,
    ) -> String {
        if initializer.is_none() {
            return temp;
        }

        let defaulted = self.get_temp_var_name();
        self.write(", ");
        self.write(&defaulted);
        self.write(" = ");
        self.write(&temp);
        self.write(" === void 0 ? ");
        self.emit_expression(initializer);
        self.write(" : ");
        self.write(&temp);
        defaulted
    }

    fn array_object_rest_is_simple_inlineable_expression(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        let Some(kind) = tsz_scanner::SyntaxKind::try_from_u16(expr_node.kind) else {
            return false;
        };

        matches!(
            kind,
            tsz_scanner::SyntaxKind::StringLiteral
                | tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral
                | tsz_scanner::SyntaxKind::NumericLiteral
                | tsz_scanner::SyntaxKind::BigIntLiteral
        ) || (kind >= tsz_scanner::SyntaxKind::FIRST_KEYWORD
            && kind <= tsz_scanner::SyntaxKind::LAST_KEYWORD)
    }

    /// Get the property name text from a binding element (for __rest exclude list).
    /// Returns a tuple of (name, `is_computed`) where `is_computed` means the name
    /// came from a computed property like `['b']`.
    fn get_binding_element_property_name_info(&self, elem_idx: NodeIndex) -> (String, bool) {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return (String::new(), false);
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return (String::new(), false);
        };

        // If there's an explicit property name, use it
        if elem.property_name.is_some() {
            if let Some(prop_node) = self.arena.get(elem.property_name) {
                if let Some(ident) = self.arena.get_identifier(prop_node) {
                    return (ident.escaped_text.clone(), false);
                }
                if let Some(lit) = self.arena.get_literal(prop_node) {
                    return (lit.text.clone(), false);
                }
                // Handle computed property name: [expr]
                if prop_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                    && let Some(computed) = self.arena.get_computed_property(prop_node)
                    && let Some(expr_node) = self.arena.get(computed.expression)
                {
                    // Static string literal: ['b'] → "b"
                    if let Some(lit) = self.arena.get_literal(expr_node) {
                        return (lit.text.clone(), true);
                    }
                    // Static numeric literal: [0] → "0"
                    if expr_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
                        && let Some(lit) = self.arena.get_literal(expr_node)
                    {
                        return (lit.text.clone(), true);
                    }
                }
            }
            return (String::new(), false);
        }

        // Otherwise, the name IS the property name (shorthand)
        (self.get_identifier_text(elem.name), false)
    }

    /// Get the property name text from a binding element (for __rest exclude list).
    fn get_binding_element_property_name_text(&self, elem_idx: NodeIndex) -> String {
        self.get_binding_element_property_name_info(elem_idx).0
    }

    /// Check if a binding element has a computed property name (e.g., `{ [key]: x }`).
    fn has_computed_property_name(&self, elem_idx: NodeIndex) -> bool {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return false;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return false;
        };
        if elem.property_name.is_none() {
            return false;
        }
        let Some(prop_node) = self.arena.get(elem.property_name) else {
            return false;
        };
        prop_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
    }

    /// Emit the expression inside a computed property name for a binding element.
    /// For `{ [expr]: x }`, emits `expr`.
    fn emit_computed_key_expression(&mut self, elem_idx: NodeIndex) {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };
        let Some(prop_node) = self.arena.get(elem.property_name) else {
            return;
        };
        if prop_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(prop_node)
        {
            self.emit_expression(computed.expression);
        }
    }

    /// Check if a binding element's property name is a string literal (e.g., `{ 'a': x }`).
    fn is_string_literal_property_name(&self, elem_idx: NodeIndex) -> bool {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return false;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return false;
        };
        if elem.property_name.is_none() {
            return false;
        }
        let Some(prop_node) = self.arena.get(elem.property_name) else {
            return false;
        };
        prop_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
    }

    /// Get the text of an identifier node.
    pub(super) fn get_identifier_text(&self, idx: NodeIndex) -> String {
        if idx.is_none() {
            return String::new();
        }
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        if let Some(ident) = self.arena.get_identifier(node) {
            return ident.escaped_text.clone();
        }
        String::new()
    }

    // =========================================================================
    // ES2018 Function Parameter Object Rest Lowering
    // =========================================================================

    /// Check if any function parameters have object rest patterns that need lowering.
    pub(super) fn any_param_has_object_rest(&self, params: &[NodeIndex]) -> bool {
        params.iter().any(|&idx| self.param_has_object_rest(idx))
    }
}
