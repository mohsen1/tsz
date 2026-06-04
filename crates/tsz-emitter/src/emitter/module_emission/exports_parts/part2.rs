impl<'a> Printer<'a> {
    /// Emit an exported variable statement with destructuring binding patterns
    /// as a `CJS`/`AMD` comma expression that directly assigns to `exports.*`.
    pub(in crate::emitter) fn emit_cjs_destructuring_export(
        &mut self,
        clause_node: &tsz_parser::parser::node::Node,
    ) {
        let Some(var_stmt) = self.arena.get_variable(clause_node) else {
            return;
        };
        self.emit_comments_before_pos(clause_node.pos);

        // Walk through declaration lists to find the variable declaration
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };

                let Some(name_node) = self.arena.get(decl.name) else {
                    continue;
                };

                let is_binding_pattern = name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;

                if !is_binding_pattern {
                    // Simple identifier — shouldn't reach here, but handle gracefully
                    let name = self.get_identifier_text_idx(decl.name);
                    if !name.is_empty() {
                        self.write("exports.");
                        self.write(&name);
                        self.write(" = ");
                        self.emit(decl.initializer);
                        self.write(";");
                    }
                    continue;
                }

                if !(self.binding_pattern_has_export_names(decl.name)
                    || self.ctx.target_es5 && self.binding_pattern_is_empty(decl.name))
                {
                    self.emit_cjs_destructuring_export_without_bindings(
                        decl.name,
                        decl.initializer,
                    );
                    continue;
                }

                // Get binding pattern elements
                let Some(pattern) = self.arena.get_binding_pattern(name_node) else {
                    continue;
                };

                // Collect non-rest elements and rest element
                let pattern_is_array = name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;
                let mut non_rest_elems: Vec<DestructuringExportBinding> = Vec::new();
                let mut rest_elem: Option<String> = None;
                let mut excluded_props: Vec<String> = Vec::new();

                for (element_index, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };
                    if elem_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        continue;
                    }
                    let Some(elem) = self.arena.get_binding_element(elem_node) else {
                        continue;
                    };

                    if elem.dot_dot_dot_token {
                        // Rest element
                        let rest_name = self.get_identifier_text(elem.name);
                        rest_elem = Some(rest_name);
                        continue;
                    }

                    // Get the variable (export) name
                    let var_name = self.get_identifier_text(elem.name);

                    let access = if pattern_is_array {
                        DestructuringExportAccess::Element(element_index)
                    } else {
                        // Get the property name to access on the source object.
                        let prop_name = if elem.property_name.is_some() {
                            let pn = self.get_identifier_text_idx(elem.property_name);
                            if pn.is_empty() { var_name.clone() } else { pn }
                        } else {
                            var_name.clone()
                        };
                        excluded_props.push(prop_name.clone());
                        DestructuringExportAccess::Property(prop_name)
                    };

                    let leading_comment_pos = if elem.property_name.is_some() {
                        self.arena
                            .get(elem.name)
                            .map_or(elem_node.pos, |name_node| name_node.pos)
                    } else {
                        elem_node.pos
                    };

                    non_rest_elems.push(DestructuringExportBinding {
                        export_name: var_name,
                        access,
                        leading_comment_pos,
                    });
                }

                let is_empty = non_rest_elems.is_empty() && rest_elem.is_none();

                // Optimization: when there's exactly one binding (no rest), skip the
                // temp variable and emit `exports.x = (rhs).x` directly. tsc does this.
                if non_rest_elems.len() == 1 && rest_elem.is_none() {
                    let binding = &non_rest_elems[0];
                    // Check if RHS is a numeric literal — needs special formatting
                    // because `1.toString` is a JS parse error (`.` is decimal point).
                    // tsc emits `1..toString` (trailing dot on number, then prop access).
                    let init_is_numeric = decl.initializer.is_some()
                        && self
                            .arena
                            .get(decl.initializer)
                            .is_some_and(|n| n.is_numeric_literal())
                        && matches!(binding.access, DestructuringExportAccess::Property(_));
                    self.emit_comments_before_pos(binding.leading_comment_pos);
                    self.write("exports.");
                    self.write(&binding.export_name);
                    self.write(" = ");
                    self.emit(decl.initializer);
                    if init_is_numeric {
                        // Emit extra dot for numeric literal property access: 1..toString
                        self.write(".");
                    }
                    self.emit_destructuring_export_access(&binding.access);
                    self.write(";");
                    continue;
                }

                // Generate a hoisted temp var for the RHS.
                // CJS destructuring temps are placed BEFORE __esModule marker.
                let temp_name = self.make_unique_name_cjs_destructuring();

                if is_empty {
                    // Empty binding pattern
                    if self.ctx.target_es5 {
                        // es5: exports._b = _a = expr;
                        // _b is only used as export property name, no local var needed.
                        let export_temp = self.make_unique_name();
                        self.write("exports.");
                        self.write(&export_temp);
                        self.write(" = ");
                        self.write(&temp_name);
                        self.write(" = ");
                        self.emit(decl.initializer);
                        self.write(";");
                    } else {
                        // esnext: _a = expr;
                        self.write(&temp_name);
                        self.write(" = ");
                        self.emit(decl.initializer);
                        self.write(";");
                    }
                } else if self.ctx.target_es5 {
                    // es5 non-empty: exports.x = (_a = expr, _a).x, exports.rest = __rest(_a, ["x"]);
                    let mut first = true;
                    for binding in &non_rest_elems {
                        if !first {
                            self.write(", ");
                        }
                        self.write("exports.");
                        self.write(&binding.export_name);
                        self.write(" = (");
                        if first {
                            self.write(&temp_name);
                            self.write(" = ");
                            self.emit(decl.initializer);
                            self.write(", ");
                            self.write(&temp_name);
                        } else {
                            self.write(&temp_name);
                        }
                        self.write(")");
                        self.emit_destructuring_export_access(&binding.access);
                        first = false;
                    }
                    if let Some(rest_name) = &rest_elem {
                        if !first {
                            self.write(", ");
                        }
                        self.write("exports.");
                        self.write(rest_name);
                        self.write(" = ");
                        self.write_helper("__rest");
                        self.write("(");
                        if first {
                            // Only rest, no non-rest elements — assign temp first
                            self.write(&temp_name);
                            self.write(" = ");
                            self.emit(decl.initializer);
                            self.write(", ");
                            self.write(&temp_name);
                        } else {
                            self.write(&temp_name);
                        }
                        self.write(", [");
                        for (i, prop) in excluded_props.iter().enumerate() {
                            if i > 0 {
                                self.write(", ");
                            }
                            self.write("\"");
                            self.write(prop);
                            self.write("\"");
                        }
                        self.write("])");
                    }
                    self.write(";");
                } else {
                    // esnext non-empty: _a = expr, exports.x = _a.x, exports.rest = __rest(_a, ["x"]);
                    self.write(&temp_name);
                    self.write(" = ");
                    self.emit(decl.initializer);

                    for binding in &non_rest_elems {
                        self.write(", ");
                        self.write("exports.");
                        self.write(&binding.export_name);
                        self.write(" = ");
                        self.write(&temp_name);
                        self.emit_destructuring_export_access(&binding.access);
                    }

                    if let Some(rest_name) = &rest_elem {
                        self.write(", ");
                        self.write("exports.");
                        self.write(rest_name);
                        self.write(" = ");
                        self.write_helper("__rest");
                        self.write("(");
                        self.write(&temp_name);
                        self.write(", [");
                        for (i, prop) in excluded_props.iter().enumerate() {
                            if i > 0 {
                                self.write(", ");
                            }
                            self.write("\"");
                            self.write(prop);
                            self.write("\"");
                        }
                        self.write("])");
                    }

                    self.write(";");
                }
            }
        }
    }

    fn binding_pattern_has_export_names(&self, pattern_idx: NodeIndex) -> bool {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return false;
        };

        if self.has_identifier_text(pattern_idx) {
            return true;
        }

        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
            && pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
        {
            return false;
        }

        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return false;
        };

        pattern.elements.nodes.iter().any(|&elem_idx| {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                return false;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                return false;
            };
            self.binding_pattern_has_export_names(elem.name)
        })
    }

    pub(in crate::emitter) fn binding_pattern_is_empty(&self, pattern_idx: NodeIndex) -> bool {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return false;
        };
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
            && pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
        {
            return false;
        }
        self.arena
            .get_binding_pattern(pattern_node)
            .is_some_and(|pattern| pattern.elements.nodes.is_empty())
    }

    fn emit_cjs_destructuring_export_without_bindings(
        &mut self,
        pattern_idx: NodeIndex,
        initializer: NodeIndex,
    ) {
        let temp_name = self.make_unique_name_cjs_destructuring();
        self.write(&temp_name);
        self.write(" = ");
        if initializer.is_none() {
            self.write("void 0");
        } else {
            self.emit(initializer);
        }
        self.emit_cjs_empty_binding_pattern_tails(pattern_idx, &temp_name);
        self.write(";");
    }

    fn emit_cjs_empty_binding_pattern_tails(&mut self, pattern_idx: NodeIndex, source: &str) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
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
            let Some(name_node) = self.arena.get(elem.name) else {
                continue;
            };
            if name_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
                && name_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                continue;
            }

            let nested_temp = self.make_unique_name_cjs_destructuring();
            self.write(", ");
            self.write(&nested_temp);
            self.write(" = ");
            self.write(source);
            if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                self.write("[");
                self.write(&index.to_string());
                self.write("]");
            } else {
                let prop_name = if elem.property_name.is_some() {
                    self.get_identifier_text_idx(elem.property_name)
                } else {
                    self.get_identifier_text_idx(elem.name)
                };
                self.write(".");
                self.write(&prop_name);
            }
            self.emit_cjs_empty_binding_pattern_tails(elem.name, &nested_temp);
        }
    }
}
