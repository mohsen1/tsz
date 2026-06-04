impl<'a> Printer<'a> {
    // =========================================================================
    // Assignment destructuring lowering (ES5)
    // Lowers: [, nameA] = expr  →  nameA = expr[1]
    //         { name: nameA } = expr  →  nameA = expr.name
    // =========================================================================

    fn emit_assignment_computed_key_temp_if_needed(
        &mut self,
        name_idx: NodeIndex,
        first: &mut bool,
    ) -> Option<String> {
        if !self.assignment_property_name_is_dynamic_computed(name_idx) {
            return None;
        }

        let key_temp = self.make_unique_name_hoisted_assignment();
        self.emit_assignment_separator(first);
        self.write(&key_temp);
        self.write(" = ");
        self.emit_assignment_computed_property_expression(name_idx);
        Some(key_temp)
    }

    fn assignment_rest_prop_for_key(
        &self,
        name_idx: NodeIndex,
        computed_key_temp: Option<&str>,
    ) -> Option<AssignmentRestProp> {
        if let Some(temp) = computed_key_temp {
            return Some(AssignmentRestProp::Dynamic(temp.to_string()));
        }

        self.get_property_key_text(name_idx)
            .filter(|key| !key.is_empty())
            .map(AssignmentRestProp::Static)
    }

    fn emit_assignment_object_key_access(
        &mut self,
        source: &str,
        inline_source: Option<NodeIndex>,
        name_idx: NodeIndex,
        computed_key_temp: Option<&str>,
    ) {
        if let Some(temp) = computed_key_temp {
            self.emit_assignment_source(source, inline_source);
            self.write("[");
            self.write(temp);
            self.write("]");
            return;
        }

        let key = self.get_property_key_text(name_idx).unwrap_or_default();
        if let Some(inline_src) = inline_source {
            self.emit(inline_src);
            if is_valid_identifier_name(&key) {
                self.write(".");
                self.write(&key);
            } else {
                self.write("[\"");
                self.write(&key.replace('\\', "\\\\").replace('\"', "\\\""));
                self.write("\"]");
            }
        } else {
            self.emit_object_key_access(source, &key);
        }
    }

    fn emit_assignment_source(&mut self, source: &str, inline_source: Option<NodeIndex>) {
        if let Some(inline_src) = inline_source {
            self.emit(inline_src);
        } else {
            self.write(source);
        }
    }

    fn emit_assignment_rest_exclude_list(&mut self, props: &[AssignmentRestProp]) {
        self.write("[");
        for (i, prop) in props.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_assignment_rest_excluded_prop(prop);
        }
        self.write("]");
    }

    fn emit_assignment_rest_excluded_prop(&mut self, prop: &AssignmentRestProp) {
        match prop {
            AssignmentRestProp::Static(key) => {
                self.write("\"");
                self.write(&key.replace('\\', "\\\\").replace('"', "\\\""));
                self.write("\"");
            }
            AssignmentRestProp::Dynamic(temp) => {
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

    fn assignment_property_name_is_dynamic_computed(&self, name_idx: NodeIndex) -> bool {
        self.arena
            .get(name_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
            && self.get_property_key_text(name_idx).is_none()
    }

    fn emit_assignment_computed_property_expression(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(name_node)
        {
            self.emit(computed.expression);
        }
    }

    /// Helper to emit nested destructuring from a source name.
    pub(in crate::emitter) fn emit_assignment_nested_destructuring(
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
                    self.emit_assignment_object_destructuring(
                        &lit.elements.nodes,
                        source,
                        first,
                        None,
                    );
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    self.emit_assignment_array_destructuring(
                        &pattern.elements.nodes,
                        source,
                        first,
                        None,
                    );
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    self.emit_assignment_object_destructuring(
                        &pattern.elements.nodes,
                        source,
                        first,
                        None,
                    );
                }
            }
            _ => {}
        }
    }

    pub(in crate::emitter) fn emit_object_key_access(&mut self, source: &str, key: &str) {
        if is_valid_identifier_name(key) {
            self.write(source);
            self.write(".");
            self.write(key);
        } else {
            self.write(source);
            self.write("[\"");
            self.write(&key.replace('\\', "\\\\").replace('\"', "\\\""));
            self.write("\"]");
        }
    }

    pub(in crate::emitter) fn get_binding_or_literal_elements(
        &self,
        node: &Node,
    ) -> Option<Vec<NodeIndex>> {
        self.arena
            .get_literal_expr(node)
            .map(|lit| lit.elements.nodes.to_vec())
            .or_else(|| {
                self.arena
                    .get_binding_pattern(node)
                    .map(|pattern| pattern.elements.nodes.to_vec())
            })
    }

    /// Emit separator for assignment destructuring (`, ` between parts).
    pub(in crate::emitter) fn emit_assignment_separator(&mut self, first: &mut bool) {
        if !*first {
            self.write(", ");
        }
        *first = false;
    }

    /// Get property key text from a property name node.
    pub(in crate::emitter) fn get_property_key_text(&self, name_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(name_idx)?;
        if node.is_identifier() {
            Some(crate::transforms::emit_utils::identifier_text_or_empty(
                self.arena, name_idx,
            ))
        } else if node.is_string_literal() {
            // For string keys like { "name": value }
            self.get_string_literal_text(name_idx)
        } else if node.is_numeric_literal() {
            self.get_numeric_literal_text(name_idx)
        } else if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(node)?;
            let expr_node = self.arena.get(computed.expression)?;
            self.arena
                .get_literal(expr_node)
                .map(|literal| literal.text.clone())
        } else {
            None
        }
    }

    pub(in crate::emitter) fn get_string_literal_text(&self, idx: NodeIndex) -> Option<String> {
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

    pub(in crate::emitter) fn get_numeric_literal_text(&self, idx: NodeIndex) -> Option<String> {
        let source = self.source_text?;
        let node = self.arena.get(idx)?;
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let end = node.end as usize;
        Some(source[start..end].to_string())
    }
}
