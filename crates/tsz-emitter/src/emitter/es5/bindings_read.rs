use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_es5_destructuring_from_value(
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

    /// Emit ES5 destructuring using `__read` helper for downlevelIteration.
    /// Transforms: `[a = 0, b = 1] = expr`.
    /// Into: `_d = __read(expr, 2), _e = _d[0], a = _e === void 0 ? 0 : _e, _f = _d[1], b = _f === void 0 ? 1 : _f`.
    pub(in crate::emitter) fn emit_es5_destructuring_with_read_node(
        &mut self,
        pattern_idx: NodeIndex,
        source_expr: NodeIndex,
        _first: &mut bool,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        if std::env::var_os("TSZ_DEBUG_EMIT").is_some() {
            tracing::debug!("emit_es5_destructuring_with_read_node entered");
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

        let read_limit = self.binding_pattern_read_limit(pattern_node);

        let read_temp = self.get_temp_var_name();
        self.write(&read_temp);
        self.write(" = ");
        self.write_helper("__read");
        self.write("(");
        self.destructuring_read_depth += 1;
        self.emit(source_expr);
        self.destructuring_read_depth -= 1;
        if let Some(element_count) = read_limit
            && element_count > 0
        {
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
            #[cfg(not(target_arch = "wasm32"))]
            if std::env::var_os("TSZ_DEBUG_EMIT").is_some() {
                let elem_kind = self.arena.kind_at(elem.name).unwrap_or(0);
                tracing::debug!(
                    "downlevel-bp-element index={} elem_name={:?} unwrapped={:?} kind={}",
                    index,
                    elem.name,
                    unwrapped_name,
                    elem_kind
                );
                tracing::debug!(
                    "downlevel-bp-kind-bytes: elem={} unwrapped={}",
                    self.arena.kind_at(unwrapped_name).unwrap_or(0),
                    SyntaxKind::Identifier as u16
                );
            }
            if let Some(name_node) = self.arena.get(unwrapped_name) {
                if name_node.kind == SyntaxKind::Identifier as u16 {
                    let elem_source = format!("{read_temp}[{index}]");
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
                    let elem_source = format!("{read_temp}[{index}]");
                    if unwrapped_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                        #[cfg(not(target_arch = "wasm32"))]
                        if std::env::var_os("TSZ_DEBUG_EMIT").is_some() {
                            tracing::debug!(
                                "downlevel-nested-array index={} unwrapped={} source={}",
                                index,
                                unwrapped_name.0,
                                elem_source
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

                        let read_limit = self.binding_pattern_read_limit(unwrapped_node);
                        let nested_temp = self.get_temp_var_name();
                        self.write(&nested_temp);
                        self.write(" = ");
                        self.write_helper("__read");
                        self.write("(");
                        self.write(&source_expr);
                        if let Some(element_count) = read_limit
                            && element_count > 0
                        {
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

                        let target_temp = if elem.initializer.is_some() {
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

    pub(in crate::emitter) fn emit_es5_destructuring_with_read_tail(
        &mut self,
        pattern_idx: NodeIndex,
        source_expr: &str,
    ) {
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

            if elem.name.is_none() {
                continue;
            }

            if elem.dot_dot_dot_token {
                if self.is_binding_pattern(elem.name) {
                    let rest_temp = self.get_temp_var_name();
                    self.write(", ");
                    self.write(&rest_temp);
                    self.write(" = ");
                    self.write(source_expr);
                    self.write(".slice(");
                    self.write(&index.to_string());
                    self.write(")");
                    self.emit_es5_destructuring_pattern_idx(elem.name, &rest_temp);
                } else if self.has_identifier_text(elem.name) {
                    self.write(", ");
                    self.emit_expression(elem.name);
                    self.write(" = ");
                    self.write(source_expr);
                    self.write(".slice(");
                    self.write(&index.to_string());
                    self.write(")");
                }
                continue;
            }

            let elem_source = format!("{source_expr}[{index}]");
            let Some(elem_node) = self.arena.get(elem.name) else {
                continue;
            };

            if elem_node.kind == SyntaxKind::Identifier as u16 {
                self.write(", ");
                self.emit(elem.name);
                self.write(" = ");
                if elem.initializer.is_some() {
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
                    let read_limit = self.binding_pattern_read_limit(nested_node);
                    let nested_temp = self.get_temp_var_name();
                    self.write(", ");
                    self.write(&nested_temp);
                    self.write(" = ");
                    self.write_helper("__read");
                    self.write("(");
                    self.write(&elem_source);
                    if let Some(nested_count) = read_limit
                        && nested_count > 0
                    {
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

                    let target_temp = if elem.initializer.is_some() {
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

    pub(in crate::emitter) fn emit_es5_destructuring_with_read(
        &mut self,
        pattern_idx: NodeIndex,
        source_expr: &str,
        _first: &mut bool,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };

        // Only handle array binding patterns for now.
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

        let read_limit = self.binding_pattern_read_limit(pattern_node);

        let read_temp = self.get_temp_var_name();
        self.write(&read_temp);
        self.write(" = ");
        self.write_helper("__read");
        self.write("(");
        self.write(source_expr);
        if let Some(element_count) = read_limit {
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

            let elem_temp = self.get_temp_var_name();
            self.write(", ");
            self.write(&elem_temp);
            self.write(" = ");
            self.write(&read_temp);
            self.write("[");
            self.write(&index.to_string());
            self.write("]");

            if let Some(name_node) = self.arena.get(elem.name) {
                if name_node.kind == SyntaxKind::Identifier as u16 {
                    self.write(", ");
                    self.emit(elem.name);
                    self.write(" = ");
                    if elem.initializer.is_some() {
                        self.write(&elem_temp);
                        self.write(" === void 0 ? ");
                        self.emit_expression(elem.initializer);
                        self.write(" : ");
                        self.write(&elem_temp);
                    } else {
                        self.write(&elem_temp);
                    }
                } else if self.is_binding_pattern(elem.name) {
                    let nested_temp = if elem.initializer.is_some() {
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
}
