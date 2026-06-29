//! Destructured binding source and group recording helpers.

use super::*;

impl<'a> CheckerState<'a> {
    /// Record source expression info for destructured bindings.
    /// Maps each binding element symbol to `(source_expression, property_name)` so that
    /// flow narrowing can check if the source's property has been narrowed by a condition.
    /// For example, `const { bar } = aFoo` records `bar -> (aFoo_node, "bar")`.
    pub(crate) fn record_destructured_binding_sources(
        &mut self,
        pattern_idx: NodeIndex,
        source_expr: NodeIndex,
    ) {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        for &element_idx in &pattern_data.elements.nodes {
            if element_idx.is_none() {
                continue;
            }
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(element_data.name) else {
                continue;
            };

            let prop_name = if element_data.property_name.is_some() {
                // Explicit property name: `{ foo: bar } = obj` — property is "foo".
                if let Some(prop_node) = self.ctx.arena.get(element_data.property_name) {
                    self.ctx
                        .arena
                        .get_identifier(prop_node)
                        .map(|ident| ident.escaped_text.clone())
                } else {
                    None
                }
            } else {
                // Shorthand: `{ bar } = obj` — property name is the identifier name.
                self.ctx
                    .arena
                    .get_identifier(name_node)
                    .map(|ident| ident.escaped_text.clone())
            };

            let Some(prop_atom) = prop_name else {
                continue;
            };

            if name_node.kind == SyntaxKind::Identifier as u16 {
                if let Some(sym_id) = self.ctx.binder.get_node_symbol(element_data.name) {
                    self.ctx
                        .destructured_binding_sources
                        .insert(sym_id, (source_expr, prop_atom));
                }
            } else if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
                // Nested: `{ nested: { a, b } } = obj` — recurse with dotted path prefix.
                self.record_nested_destructured_binding_sources(
                    element_data.name,
                    source_expr,
                    &prop_atom,
                );
            }
        }
    }

    /// Record destructured binding sources for nested object destructuring patterns.
    ///
    /// For `{ nested: { a, b: text } } = obj`, records:
    /// - symbol for `a` -> (obj, "nested.a")
    /// - symbol for `text` -> (obj, "nested.b")
    fn record_nested_destructured_binding_sources(
        &mut self,
        pattern_idx: NodeIndex,
        source_expr: NodeIndex,
        prefix: &str,
    ) {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        for &element_idx in &pattern_data.elements.nodes {
            if element_idx.is_none() {
                continue;
            }
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(element_data.name) else {
                continue;
            };

            let prop_name = if element_data.property_name.is_some() {
                if let Some(prop_node) = self.ctx.arena.get(element_data.property_name) {
                    self.ctx
                        .arena
                        .get_identifier(prop_node)
                        .map(|ident| ident.escaped_text.clone())
                } else {
                    None
                }
            } else {
                self.ctx
                    .arena
                    .get_identifier(name_node)
                    .map(|ident| ident.escaped_text.clone())
            };

            let Some(prop_atom) = prop_name else {
                continue;
            };

            let dotted_path = format!("{prefix}.{prop_atom}");

            if name_node.kind == SyntaxKind::Identifier as u16 {
                if let Some(sym_id) = self.ctx.binder.get_node_symbol(element_data.name) {
                    self.ctx
                        .destructured_binding_sources
                        .insert(sym_id, (source_expr, dotted_path));
                }
            } else if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
                self.record_nested_destructured_binding_sources(
                    element_data.name,
                    source_expr,
                    &dotted_path,
                );
            }
        }
    }

    /// Record destructured binding group information for correlated narrowing.
    /// When `const { data, isSuccess } = useQuery()`, this records that both `data` and
    /// `isSuccess` come from the same union source and can be used for correlated narrowing.
    pub(crate) fn record_destructured_binding_group(
        &mut self,
        pattern_idx: NodeIndex,
        source_type: TypeId,
        is_const: bool,
        pattern_kind: u16,
    ) {
        use crate::context::DestructuredBindingInfo;

        let group_id = self.ctx.next_binding_group_id;
        self.ctx.next_binding_group_id += 1;

        let mut stack: Vec<(NodeIndex, TypeId, u16, String)> =
            vec![(pattern_idx, source_type, pattern_kind, String::new())];

        while let Some((curr_pattern_idx, _curr_source_type, curr_kind, base_path)) = stack.pop() {
            let Some(curr_pattern_node) = self.ctx.arena.get(curr_pattern_idx) else {
                continue;
            };
            let Some(curr_pattern_data) = self.ctx.arena.get_binding_pattern(curr_pattern_node)
            else {
                continue;
            };

            let curr_is_object = curr_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN;

            for (i, &element_idx) in curr_pattern_data.elements.nodes.iter().enumerate() {
                if element_idx.is_none() {
                    continue;
                }
                let Some(element_node) = self.ctx.arena.get(element_idx) else {
                    continue;
                };
                if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                    continue;
                }
                let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(element_data.name) else {
                    continue;
                };

                let path_segment = if curr_is_object {
                    if element_data.property_name.is_some() {
                        if let Some(prop_node) = self.ctx.arena.get(element_data.property_name) {
                            self.ctx
                                .arena
                                .get_identifier(prop_node)
                                .map(|ident| ident.escaped_text.clone())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        }
                    } else {
                        self.ctx
                            .arena
                            .get_identifier(name_node)
                            .map(|ident| ident.escaped_text.clone())
                            .unwrap_or_default()
                    }
                } else {
                    String::new()
                };

                let property_name = if curr_is_object {
                    if base_path.is_empty() {
                        path_segment
                    } else if path_segment.is_empty() {
                        base_path.clone()
                    } else {
                        format!("{base_path}.{path_segment}")
                    }
                } else {
                    String::new()
                };

                if name_node.kind == SyntaxKind::Identifier as u16 {
                    if let Some(sym_id) = self.ctx.binder.get_node_symbol(element_data.name) {
                        self.ctx.destructured_bindings.insert(
                            sym_id,
                            DestructuredBindingInfo {
                                source_type,
                                property_name: property_name.clone(),
                                element_index: i as u32,
                                group_id,
                                is_const,
                                is_rest: element_data.dot_dot_dot_token,
                            },
                        );
                    }
                    continue;
                }

                // Keep correlated narrowing scoped to direct siblings from the same
                // destructuring layer. TypeScript does not correlate nested aliases
                // like `const { resp: { data }, type } = value`.
            }
        }
    }
}
