//! Destructuring pattern type resolution and validation.

use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn report_unknown_empty_binding_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
    ) {
        if parent_type != TypeId::UNKNOWN {
            return;
        }

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };
        if !pattern_data.elements.nodes.is_empty() {
            return;
        }

        self.error_at_node(
            pattern_idx,
            "Object is of type 'unknown'.",
            crate::diagnostics::diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
        );
    }

    fn should_suppress_missing_property_for_literal_default(
        &self,
        pattern_idx: NodeIndex,
        element_data: &tsz_parser::parser::node::BindingElementData,
    ) -> bool {
        if element_data.initializer.is_none() {
            return false;
        }

        let Some(ext) = self.ctx.arena.get_extended(pattern_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };

        let source_expr = match parent_node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let Some(decl) = self.ctx.arena.get_variable_declaration(parent_node) else {
                    return false;
                };
                if decl.name != pattern_idx || decl.type_annotation.is_some() {
                    return false;
                }
                decl.initializer
            }
            syntax_kind_ext::PARAMETER => {
                let Some(param) = self.ctx.arena.get_parameter(parent_node) else {
                    return false;
                };
                if param.name != pattern_idx
                    || param.type_annotation.is_some()
                    || self.ctx.contextual_type.is_some()
                {
                    return false;
                }
                param.initializer
            }
            // Nested destructuring: `{ event: { params = {} } = {} }` — the inner
            // ObjectBindingPattern's parent is the outer BindingElement.  When that
            // BindingElement has an object-literal default, suppress TS2339 for the
            // inner pattern's properties (same as tsc).
            syntax_kind_ext::BINDING_ELEMENT => {
                let Some(be) = self.ctx.arena.get_binding_element(parent_node) else {
                    return false;
                };
                if be.name != pattern_idx {
                    return false;
                }
                be.initializer
            }
            _ => return false,
        };

        let source_expr = self.ctx.arena.skip_parenthesized(source_expr);
        self.ctx
            .arena
            .get(source_expr)
            .is_some_and(|expr| expr.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
    }

    fn binding_pattern_direct_source_is_this(&self, pattern_idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(pattern_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };

        let source_expr = if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            self.ctx
                .arena
                .get_variable_declaration(parent_node)
                .map(|decl| decl.initializer)
        } else {
            None
        };

        source_expr.is_some_and(|expr_idx| {
            let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
            self.is_this_expression(expr_idx)
        })
    }

    fn add_undefined_if_missing_for_destructuring(&self, ty: TypeId) -> TypeId {
        if tsz_solver::type_contains_undefined(self.ctx.types, ty) {
            ty
        } else {
            self.ctx.types.factory().union(vec![ty, TypeId::UNDEFINED])
        }
    }

    pub(crate) fn report_empty_array_destructuring_bounds(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) {
        let Some(init_node) = self.ctx.arena.get(initializer_idx) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return;
        }
        let Some(init_lit) = self.ctx.arena.get_literal_expr(init_node) else {
            return;
        };
        if !init_lit.elements.nodes.is_empty() {
            return;
        }

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        for (index, &element_idx) in pattern.elements.nodes.iter().enumerate() {
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
            if element_data.dot_dot_dot_token {
                break;
            }
            // TS doesn't report tuple out-of-bounds for empty array destructuring
            // when the element has a default value.
            if element_data.initializer.is_some() {
                continue;
            }

            self.error_at_node(
                element_data.name,
                &format!("Tuple type '[]' of length '0' has no element at index '{index}'."),
                crate::diagnostics::diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
            );
        }
    }

    /// Check binding pattern elements and their default values for type correctness.
    ///
    /// This function traverses a binding pattern (object or array destructuring) and verifies
    /// that any default values provided in binding elements are assignable to their expected types.
    /// Assign inferred types to binding element symbols (destructuring).
    ///
    /// The binder creates symbols for identifiers inside binding patterns (e.g., `const [x] = arr;`),
    /// but their `value_declaration` is the identifier node, not the enclosing variable declaration.
    /// We infer the binding element type from the destructured value type and cache it on the symbol.
    pub(crate) fn assign_binding_pattern_symbol_types(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
    ) {
        self.report_unknown_empty_binding_pattern(pattern_idx, parent_type);

        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        for (i, &element_idx) in pattern_data.elements.nodes.iter().enumerate() {
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

            let mut element_type = if parent_type == TypeId::ANY {
                TypeId::ANY
            } else {
                self.get_binding_element_type(pattern_idx, i, parent_type, element_data)
            };

            // If there's an initializer, the type incorporates it.
            // TypeScript widens the inferred type with the initializer type.
            // Set contextual type for function-like defaults so parameter types
            // are inferred from the expected element type (e.g., `{ f: id = arg => arg }: T`).
            if element_data.initializer.is_some() {
                // A default value guarantees the binding won't be undefined at runtime,
                // so strip `undefined` from the element type. This matches tsc behavior:
                // `{ name = "default" }: { name?: string }` gives `name` type `string`.
                if self.ctx.strict_null_checks()
                    && element_type != TypeId::ANY
                    && element_type != TypeId::UNKNOWN
                    && element_type != TypeId::ERROR
                {
                    element_type = tsz_solver::remove_undefined(self.ctx.types, element_type);
                }

                let prev_context = self.ctx.contextual_type;
                // Provide the element type as contextual type for the default
                // value expression. This is needed for:
                // - Arrow/function defaults: infers parameter types
                // - Array literal defaults: produces tuples instead of widened arrays
                //   e.g., `[b, {x}]=["abc", {x: 10}]` needs the default typed as
                //   a tuple `[string, {x: number}]`, not `(string | {x: number})[]`
                if element_type != TypeId::ANY
                    && element_type != TypeId::UNKNOWN
                    && element_type != TypeId::ERROR
                {
                    self.ctx.contextual_type = Some(element_type);
                }
                let init_type = self.get_type_of_node(element_data.initializer);
                self.ctx.contextual_type = prev_context;
                if element_type == TypeId::ANY || element_type == TypeId::UNKNOWN {
                    element_type = init_type;
                } else if !self.is_assignable_to(init_type, element_type) {
                    element_type = self
                        .ctx
                        .types
                        .factory()
                        .union(vec![element_type, init_type]);
                }
            }

            let Some(name_node) = self.ctx.arena.get(element_data.name) else {
                continue;
            };

            // Identifier binding: cache the inferred type on the symbol.
            if name_node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.ctx.binder.get_node_symbol(element_data.name)
            {
                // When strictNullChecks is off, undefined and null widen to any
                // for mutable destructured bindings (var/let).
                // This includes unions like `undefined | null`.
                let final_type = if !self.ctx.strict_null_checks()
                    && query::is_only_null_or_undefined(self.ctx.types, element_type)
                {
                    TypeId::ANY
                } else {
                    element_type
                };
                self.cache_symbol_type(sym_id, final_type);
            }

            // Nested binding patterns: check iterability for array patterns, then recurse
            if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                // Check iterability for nested array destructuring
                self.check_destructuring_iterability(
                    element_data.name,
                    element_type,
                    NodeIndex::NONE,
                );
                self.assign_binding_pattern_symbol_types(element_data.name, element_type);
            } else if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
                self.assign_binding_pattern_symbol_types(element_data.name, element_type);
            }
        }
    }

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

            // Get the property name for this binding element
            let prop_name = if element_data.property_name.is_some() {
                // Explicit property name: `{ foo: bar } = obj` — property is "foo"
                if let Some(prop_node) = self.ctx.arena.get(element_data.property_name) {
                    self.ctx
                        .arena
                        .get_identifier(prop_node)
                        .map(|ident| ident.escaped_text.clone())
                } else {
                    None
                }
            } else {
                // Shorthand: `{ bar } = obj` — property name is the identifier name
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
                // Nested: `{ nested: { a, b } } = obj` — recurse with dotted path prefix
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
    /// - symbol for `a`  → (obj, "nested.a")
    /// - symbol for `text` → (obj, "nested.b")
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

            // Get the property name for this binding element
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
                // Further nesting — recurse
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

    /// Get the expected type for a binding element from its parent type.
    pub(crate) fn get_binding_element_type(
        &mut self,
        pattern_idx: NodeIndex,
        element_index: usize,
        parent_type: TypeId,
        element_data: &tsz_parser::parser::node::BindingElementData,
    ) -> TypeId {
        let pattern_kind = self.ctx.arena.get(pattern_idx).map_or(0, |n| n.kind);
        // Resolve Application/Lazy types to their concrete form so that
        // union members, object shapes, and tuple elements are accessible.
        let parent_type = self.evaluate_type_for_assignability(parent_type);
        let defer_property_not_found = self
            .should_defer_property_not_found_for_contextual_destructuring(pattern_idx, parent_type);
        let suppress_missing_property_for_literal_default =
            self.should_suppress_missing_property_for_literal_default(pattern_idx, element_data);

        // Array binding patterns use the element position.
        if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            if parent_type == TypeId::UNKNOWN || parent_type == TypeId::ERROR {
                return parent_type;
            }

            // For union types of tuples/arrays, resolve element type from each member
            if let Some(members) = query::union_members(self.ctx.types, parent_type) {
                let mut elem_types = Vec::new();
                let factory = self.ctx.types.factory();
                for &member in &members {
                    let member = query::unwrap_readonly_deep(self.ctx.types, member);
                    if element_data.dot_dot_dot_token {
                        let elem_type = if let Some(elem) =
                            query::array_element_type(self.ctx.types, member)
                        {
                            factory.array(elem)
                        } else if let Some(elems) = query::tuple_elements(self.ctx.types, member) {
                            let rest_elem = elems
                                .iter()
                                .find(|e| e.rest)
                                .or_else(|| elems.last())
                                .map_or(TypeId::ANY, |e| e.type_id);
                            self.rest_binding_array_type(rest_elem)
                        } else {
                            continue;
                        };
                        elem_types.push(elem_type);
                    } else if let Some(elem) = query::array_element_type(self.ctx.types, member) {
                        let mut elem = elem;
                        if self.ctx.no_unchecked_indexed_access() {
                            elem = self.add_undefined_if_missing_for_destructuring(elem);
                        }
                        elem_types.push(elem);
                    } else if query::tuple_elements(self.ctx.types, member).is_some() {
                        let elem = self.get_element_access_type(
                            member,
                            TypeId::NUMBER,
                            Some(element_index),
                        );
                        if elem != TypeId::ERROR {
                            elem_types.push(elem);
                        }
                    }
                }
                if elem_types.is_empty() && !element_data.dot_dot_dot_token {
                    // All members are tuples that are out of bounds for this index.
                    // Emit TS2339 "Property 'N' does not exist on type 'X'".
                    let all_tuples_oob = members.iter().all(|&m| {
                        let m = query::unwrap_readonly_deep(self.ctx.types, m);
                        if let Some(elems) = query::tuple_elements(self.ctx.types, m) {
                            let has_rest = elems.iter().any(|e| e.rest);
                            !has_rest && element_index >= elems.len()
                        } else {
                            false
                        }
                    });
                    if all_tuples_oob {
                        let type_str = self.format_type(parent_type);
                        self.error_at_node(
                            element_data.name,
                            &format!(
                                "Property '{element_index}' does not exist on type '{type_str}'.",
                            ),
                            crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        );
                    }
                    return TypeId::ANY;
                }
                return if elem_types.len() == 1 {
                    elem_types[0]
                } else {
                    factory.union(elem_types)
                };
            }

            // Unwrap readonly wrappers for destructuring element access
            let array_like = query::unwrap_readonly_deep(self.ctx.types, parent_type);

            // Rest element: ...rest
            if element_data.dot_dot_dot_token {
                let elem_type =
                    if let Some(elem) = query::array_element_type(self.ctx.types, array_like) {
                        elem
                    } else if let Some(elems) = query::tuple_elements(self.ctx.types, array_like) {
                        // Best-effort: if the tuple has a rest element, use it; otherwise, fall back to last.
                        elems
                            .iter()
                            .find(|e| e.rest)
                            .or_else(|| elems.last())
                            .map_or(TypeId::ANY, |e| e.type_id)
                    } else {
                        TypeId::ANY
                    };
                return self.rest_binding_array_type(elem_type);
            }

            return if let Some(elem) = query::array_element_type(self.ctx.types, array_like) {
                if self.ctx.no_unchecked_indexed_access() {
                    self.add_undefined_if_missing_for_destructuring(elem)
                } else {
                    elem
                }
            } else if let Some(elems) = query::tuple_elements(self.ctx.types, array_like) {
                let elem =
                    self.get_element_access_type(array_like, TypeId::NUMBER, Some(element_index));
                if elem != TypeId::ERROR {
                    elem
                } else {
                    let has_rest_tail = elems.last().is_some_and(|element| element.rest);
                    // When a binding element has a default value (e.g., `[a, b = a] = [1]`),
                    // accessing beyond the tuple length is allowed — the default covers
                    // the missing element. tsc does not emit TS2493 in this case.
                    // Also skip when the index is in bounds — ERROR may just mean the
                    // element type itself is an error (e.g. from an unresolved property),
                    // not that the index is out of range.
                    if !has_rest_tail
                        && element_data.initializer.is_none()
                        && element_index >= elems.len()
                    {
                        let tuple_type_str = self.format_type(array_like);
                        self.error_at_node(
                            element_data.name,
                            &format!(
                                "Tuple type '{}' of length '{}' has no element at index '{}'.",
                                tuple_type_str,
                                elems.len(),
                                element_index
                            ),
                            crate::diagnostics::diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
                        );
                    }
                    TypeId::ANY
                }
            } else {
                TypeId::ANY
            };
        }

        let computed_expr = self
            .ctx
            .arena
            .get(element_data.property_name)
            .and_then(|prop_node| self.ctx.arena.get_computed_property(prop_node))
            .map(|computed| computed.expression);

        if let Some(computed_expr) = computed_expr {
            let key_type = self.get_binding_element_computed_key_type(pattern_idx, computed_expr);
            if let Some(property_type) = self.get_binding_element_literal_key_type(
                parent_type,
                key_type,
                element_data,
                defer_property_not_found,
                suppress_missing_property_for_literal_default,
            ) {
                return property_type;
            }
        }

        // Extract the static property name from binding element.
        // Handles: { x }, { x: a }, { 'b': a }, { ['b']: a }, { [ident]: a }.
        let property_name = self.extract_binding_property_name(element_data);

        // Unique symbol keys (e.g. `const s = Symbol(); { [s]: v }`) resolve to
        // `__unique_N` via `get_property_name_resolved`, but they should be treated
        // as dynamic keys for index type checking — tsc emits TS2538 ("Type 'unique
        // symbol' cannot be used as an index type") rather than TS2339.
        let is_unique_symbol_key = property_name
            .as_ref()
            .is_some_and(|n| n.starts_with("__unique_"));
        let property_name = if is_unique_symbol_key {
            None
        } else {
            property_name
        };

        // For computed keys in object binding patterns (e.g. `{ [k]: v }`),
        // check index signatures when the key resolves to a dynamic type
        // (string or number, not a literal matching a known property).
        if element_data.property_name.is_some() {
            // Only check index signatures for truly dynamic keys (not identifiers
            // or string/numeric literals that resolve to known properties).
            // Unique symbol keys are also treated as dynamic.
            if computed_expr.is_some() && property_name.is_none() {
                let key_type = computed_expr.map_or(TypeId::ANY, |expr_idx| {
                    self.get_binding_element_computed_key_type(pattern_idx, expr_idx)
                });
                let key_is_string = key_type == TypeId::STRING;
                let key_is_number = key_type == TypeId::NUMBER;

                // TS2538: Type cannot be used as an index type.
                // Use the strict validity check matching tsc's `isValidIndexType`:
                // only `string`, `number`, `bigint`, and their literal subtypes plus
                // template literals / string mappings are valid. `any`, `symbol`,
                // `unique symbol`, `unknown`, and structural types are rejected.
                // Note: ERROR types from failed expressions are treated as `any`
                // for this check — tsc cascades TS2538 after prior expression errors.
                if !key_is_string && !key_is_number && key_type != TypeId::NEVER {
                    let check_key = if key_type == TypeId::ERROR {
                        TypeId::ANY
                    } else {
                        self.resolve_lazy_type(key_type)
                    };
                    if let Some(invalid_member) =
                        query::invalid_index_type_member_strict(self.ctx.types, check_key)
                    {
                        let key_type_str = self.format_type(invalid_member);
                        let message = crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                            &[&key_type_str],
                        );
                        let error_node = computed_expr.unwrap_or(element_data.property_name);
                        self.error_at_node(
                            error_node,
                            &message,
                            crate::diagnostics::diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                        );
                    }
                }

                if key_is_string || key_is_number {
                    let has_matching_index = |ty: TypeId| {
                        query::object_shape(self.ctx.types, ty).is_some_and(|shape| {
                            if key_is_string {
                                shape.string_index.is_some()
                            } else {
                                shape.number_index.is_some() || shape.string_index.is_some()
                            }
                        })
                    };

                    let has_index_signature =
                        if let Some(members) = query::union_members(self.ctx.types, parent_type) {
                            members.into_iter().all(has_matching_index)
                        } else {
                            has_matching_index(parent_type)
                        };

                    if !has_index_signature
                        && parent_type != TypeId::ANY
                        && parent_type != TypeId::ERROR
                        && parent_type != TypeId::UNKNOWN
                    {
                        let mut formatter = self.ctx.create_type_formatter();
                        let object_str = formatter.format(parent_type);
                        let index_str = formatter.format(key_type);
                        let message = crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                            &[&object_str, &index_str],
                        );
                        let error_node = self
                            .ctx
                            .arena
                            .get(element_data.property_name)
                            .and_then(|prop_node| self.ctx.arena.get_computed_property(prop_node))
                            .map_or(element_data.property_name, |computed| computed.expression);
                        self.error_at_node(
                            error_node,
                            &message,
                            crate::diagnostics::diagnostic_codes::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                        );
                    }
                }
            }
        }

        if element_data.dot_dot_dot_token {
            if self.is_untyped_parameter_binding_pattern_without_context(pattern_idx) {
                return TypeId::ANY;
            }
            return self.compute_object_rest_type(pattern_idx, parent_type);
        }

        if parent_type == TypeId::UNKNOWN {
            let error_node = if element_data.property_name.is_some() {
                element_data.property_name
            } else if element_data.name.is_some() {
                element_data.name
            } else {
                NodeIndex::NONE
            };
            if let Some(prop_name_str) = property_name.as_deref() {
                if !defer_property_not_found && !suppress_missing_property_for_literal_default {
                    self.error_property_not_exist_at(prop_name_str, parent_type, error_node);
                }
            } else if element_data.initializer.is_none()
                && !defer_property_not_found
                && !suppress_missing_property_for_literal_default
            {
                self.error_at_node(
                    error_node,
                    "Object is of type 'unknown'.",
                    crate::diagnostics::diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                );
            }
            return TypeId::UNKNOWN;
        }

        if let Some(ref prop_name_str) = property_name {
            if self.binding_pattern_direct_source_is_this(pattern_idx)
                && self.ctx.function_depth == 0
                && let Some(class_info) = self.ctx.enclosing_class.as_ref()
                && class_info.in_constructor
                && let Some(declaring_class_name) =
                    self.find_abstract_property_declaring_class(class_info.class_idx, prop_name_str)
            {
                let error_node = if element_data.property_name.is_some() {
                    element_data.property_name
                } else if element_data.name.is_some() {
                    element_data.name
                } else {
                    NodeIndex::NONE
                };
                self.error_abstract_property_in_constructor(
                    prop_name_str,
                    &declaring_class_name,
                    error_node,
                );
            }

            use tsz_solver::operations::property::PropertyAccessResult;
            let prop_access_result =
                self.resolve_property_access_with_env(parent_type, prop_name_str);
            match prop_access_result {
                PropertyAccessResult::Success { type_id, .. } => {
                    // Check accessibility (TS2341/TS2445) — destructuring still
                    // respects private/protected modifiers.
                    let error_node = if element_data.property_name != NodeIndex::NONE {
                        element_data.property_name
                    } else if element_data.name != NodeIndex::NONE {
                        element_data.name
                    } else {
                        NodeIndex::NONE
                    };
                    self.check_property_accessibility(
                        NodeIndex::NONE, // no direct object expr in destructuring
                        prop_name_str,
                        error_node,
                        parent_type,
                    );
                    type_id
                }
                PropertyAccessResult::PropertyNotFound { .. } => {
                    let error_node = if element_data.property_name.is_some() {
                        element_data.property_name
                    } else if element_data.name.is_some() {
                        element_data.name
                    } else {
                        NodeIndex::NONE
                    };
                    if !defer_property_not_found && !suppress_missing_property_for_literal_default {
                        // In tsc, destructuring from `object` uses the apparent type `{}`
                        // in error messages (getApparentType(object) = {}).
                        if parent_type == TypeId::OBJECT {
                            self.error_property_not_exist_with_apparent_type(
                                prop_name_str,
                                "{}",
                                error_node,
                            );
                        } else {
                            self.error_property_not_exist_at(
                                prop_name_str,
                                parent_type,
                                error_node,
                            );
                        }
                    }
                    TypeId::ANY
                }
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    if !defer_property_not_found && !suppress_missing_property_for_literal_default {
                        let error_node = if element_data.property_name.is_some() {
                            element_data.property_name
                        } else if element_data.name.is_some() {
                            element_data.name
                        } else {
                            NodeIndex::NONE
                        };
                        self.error_property_not_exist_at(prop_name_str, parent_type, error_node);
                    }
                    property_type.unwrap_or(TypeId::ANY)
                }
                PropertyAccessResult::IsUnknown => TypeId::ANY,
            }
        } else {
            TypeId::ANY
        }
    }

    fn get_binding_element_literal_key_type(
        &mut self,
        parent_type: TypeId,
        key_type: TypeId,
        element_data: &tsz_parser::parser::node::BindingElementData,
        defer_property_not_found: bool,
        suppress_missing_property_for_literal_default: bool,
    ) -> Option<TypeId> {
        let (string_keys, number_keys) = self.get_literal_key_union_from_type(key_type)?;

        if let Some(members) = query::union_members(self.ctx.types, parent_type)
            && members.len() > 1
        {
            let mut member_types = Vec::new();
            for &member in &members {
                if let Some(member_type) = self.get_binding_element_literal_key_type_for_parent(
                    query::unwrap_readonly_deep(self.ctx.types, member),
                    &string_keys,
                    &number_keys,
                    member,
                    element_data,
                    defer_property_not_found,
                    suppress_missing_property_for_literal_default,
                ) {
                    member_types.push(member_type);
                }
            }

            return if member_types.is_empty() {
                None
            } else if member_types.len() == 1 {
                Some(member_types[0])
            } else {
                Some(self.ctx.types.factory().union(member_types))
            };
        }

        self.get_binding_element_literal_key_type_for_parent(
            query::unwrap_readonly_deep(self.ctx.types, parent_type),
            &string_keys,
            &number_keys,
            parent_type,
            element_data,
            defer_property_not_found,
            suppress_missing_property_for_literal_default,
        )
    }

    fn get_binding_element_literal_key_type_for_parent(
        &mut self,
        literal_parent_type: TypeId,
        string_keys: &[Atom],
        number_keys: &[f64],
        error_parent_type: TypeId,
        element_data: &tsz_parser::parser::node::BindingElementData,
        defer_property_not_found: bool,
        suppress_missing_property_for_literal_default: bool,
    ) -> Option<TypeId> {
        let mut key_types = Vec::with_capacity(
            usize::from(!string_keys.is_empty()) + usize::from(!number_keys.is_empty()),
        );
        let error_node = if element_data.property_name.is_some() {
            element_data.property_name
        } else if element_data.name.is_some() {
            element_data.name
        } else {
            NodeIndex::NONE
        };

        if !string_keys.is_empty() {
            let keys_result =
                self.get_element_access_type_for_literal_keys(literal_parent_type, string_keys);
            if let Some(result_type) = keys_result.result_type {
                key_types.push(result_type);
            }
            if !defer_property_not_found && !suppress_missing_property_for_literal_default {
                for key in keys_result.missing_keys {
                    self.error_property_not_exist_at(&key, error_parent_type, error_node);
                }
            }
        }

        if !number_keys.is_empty()
            && let Some(result_type) = self
                .get_element_access_type_for_literal_number_keys(literal_parent_type, number_keys)
        {
            key_types.push(result_type);
        }

        if key_types.is_empty() {
            None
        } else if key_types.len() == 1 {
            Some(key_types[0])
        } else {
            Some(self.ctx.types.factory().union(key_types))
        }
    }

    fn get_binding_element_computed_key_type(
        &mut self,
        pattern_idx: NodeIndex,
        expr_idx: NodeIndex,
    ) -> TypeId {
        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let prev_checking = self.ctx.checking_computed_property_name.take();
        self.ctx.checking_computed_property_name = Some(expr_idx);
        let mut key_type = self.get_type_of_node(expr_idx);
        self.ctx.checking_computed_property_name = prev_checking;
        self.ctx.preserve_literal_types = prev_preserve;

        let is_identifier = self
            .ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16);
        if is_identifier && let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) {
            let base_key_type = self
                .get_binding_identifier_initializer_key_type(sym_id)
                .unwrap_or(key_type);
            let mut key_types = vec![base_key_type];
            self.collect_enclosing_default_assignment_key_types(
                pattern_idx,
                sym_id,
                &mut key_types,
            );
            if key_types.len() > 1 {
                key_type = self.ctx.types.factory().union(key_types);
            } else {
                key_type = base_key_type;
            }
        }

        key_type
    }

    fn collect_enclosing_default_assignment_key_types(
        &mut self,
        pattern_idx: NodeIndex,
        sym_id: SymbolId,
        key_types: &mut Vec<TypeId>,
    ) {
        let mut current = pattern_idx;
        let mut visited = 0usize;

        while visited < 32 {
            visited += 1;
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }

            if let Some(parent_node) = self.ctx.arena.get(parent_idx)
                && parent_node.kind == syntax_kind_ext::BINDING_ELEMENT
                && let Some(binding) = self.ctx.arena.get_binding_element(parent_node)
                && binding.initializer.is_some()
            {
                self.collect_assignment_types_for_symbol(binding.initializer, sym_id, key_types);
            }

            current = parent_idx;
        }
    }

    fn collect_assignment_types_for_symbol(
        &mut self,
        expr_idx: NodeIndex,
        sym_id: SymbolId,
        key_types: &mut Vec<TypeId>,
    ) {
        let mut stack = vec![expr_idx];

        while let Some(current) = stack.pop() {
            let Some(node) = self.ctx.arena.get(current) else {
                continue;
            };

            match node.kind {
                k if k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    continue;
                }
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    if let Some(binary) = self.ctx.arena.get_binary_expr(node)
                        && binary.operator_token == SyntaxKind::EqualsToken as u16
                        && self.binding_assignment_target_matches_symbol(binary.left, sym_id)
                    {
                        let prev_preserve = self.ctx.preserve_literal_types;
                        self.ctx.preserve_literal_types = true;
                        let assigned_type = self.get_type_of_node(binary.right);
                        self.ctx.preserve_literal_types = prev_preserve;
                        key_types.push(assigned_type);
                    }
                }
                _ => {}
            }

            stack.extend(self.ctx.arena.get_children(current));
        }
    }

    fn binding_assignment_target_matches_symbol(
        &self,
        target_idx: NodeIndex,
        sym_id: SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(target_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        self.resolve_identifier_symbol(target_idx) == Some(sym_id)
    }

    fn get_binding_identifier_initializer_key_type(&mut self, sym_id: SymbolId) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let decl_node = self.ctx.arena.get(decl_idx)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        if var_decl.initializer.is_none() {
            return None;
        }

        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let init_type = self.get_type_of_node(var_decl.initializer);
        self.ctx.preserve_literal_types = prev_preserve;
        Some(init_type)
    }

    /// During contextual typing of unannotated callback parameters, inferred
    /// parameter types can remain as unresolved type parameters temporarily.
    /// Avoid emitting premature TS2339 from destructuring in that phase; final
    /// assignability diagnostics (e.g. TS2322/TS2345) should drive the error.
    fn should_defer_property_not_found_for_contextual_destructuring(
        &self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
    ) -> bool {
        if !query::is_type_parameter_like(self.ctx.types, parent_type) {
            return false;
        }

        let mut current = pattern_idx;
        for _ in 0..32 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == syntax_kind_ext::PARAMETER {
                let Some(param) = self.ctx.arena.get_parameter(parent_node) else {
                    return false;
                };
                if param.type_annotation.is_some() {
                    return false;
                }

                let Some(param_ext) = self.ctx.arena.get_extended(parent_idx) else {
                    return false;
                };
                let Some(func_node) = self.ctx.arena.get(param_ext.parent) else {
                    return false;
                };

                return func_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || func_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION;
            }

            current = parent_idx;
        }

        false
    }

    /// Compute the type for an object rest element: `{ a, b, ...rest } = obj`.
    ///
    /// The rest type is the parent type with all statically-named non-rest properties
    /// excluded (like `Omit<T, 'a' | 'b'>`). For union parent types, compute the rest
    /// for each member and union the results.
    fn compute_object_rest_type(&mut self, pattern_idx: NodeIndex, parent_type: TypeId) -> TypeId {
        // Collect the names of all non-rest sibling properties in this binding pattern.
        let excluded = self.collect_non_rest_property_names(pattern_idx);

        // For union types, compute rest type for each member and union them.
        if let Some(members) = query::union_members(self.ctx.types, parent_type) {
            let rest_types: Vec<TypeId> = members
                .iter()
                .map(|&m| self.omit_properties_from_type(m, &excluded))
                .collect();
            return if rest_types.len() == 1 {
                rest_types[0]
            } else {
                self.ctx.types.factory().union(rest_types)
            };
        }

        self.omit_properties_from_type(parent_type, &excluded)
    }

    /// Collect static property names from all non-rest sibling elements in
    /// an object binding pattern.
    fn collect_non_rest_property_names(&self, pattern_idx: NodeIndex) -> Vec<String> {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return Vec::new();
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return Vec::new();
        };

        let mut names = Vec::new();
        for &element_idx in pattern_data.elements.nodes.iter() {
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
            // Skip rest elements — they are the target, not excluded.
            if element_data.dot_dot_dot_token {
                continue;
            }
            // Extract the property name (same logic as the main property_name extraction).
            let prop_name = if element_data.property_name.is_some() {
                if let Some(prop_node) = self.ctx.arena.get(element_data.property_name) {
                    // Try identifier first
                    if let Some(ident) = self.ctx.arena.get_identifier(prop_node) {
                        Some(ident.escaped_text.clone())
                    } else if let Some(lit) = self.ctx.arena.get_literal(prop_node) {
                        // String literal property name: { 'b': renamed }
                        Some(lit.text.clone())
                    } else if let Some(computed) = self.ctx.arena.get_computed_property(prop_node) {
                        // Computed property with string literal: { ['b']: renamed }
                        self.ctx
                            .arena
                            .get(computed.expression)
                            .and_then(|expr| self.ctx.arena.get_literal(expr))
                            .map(|lit| lit.text.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                // Shorthand: { x } — the name itself is the property name.
                self.ctx
                    .arena
                    .get(element_data.name)
                    .and_then(|n| self.ctx.arena.get_identifier(n))
                    .map(|ident| ident.escaped_text.clone())
            };
            if let Some(name) = prop_name {
                names.push(name);
            }
        }
        names
    }

    /// Create a new object type from `type_id` with the given property names excluded.
    fn omit_properties_from_type(&mut self, type_id: TypeId, excluded: &[String]) -> TypeId {
        if matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return type_id;
        }

        let constraint = query::type_parameter_constraint(self.ctx.types, type_id);
        let shape = query::object_shape(self.ctx.types, type_id).or_else(|| {
            // For type parameters, use the constraint's shape so that
            // `{ a, ...rest } = obj` where `obj: T extends { a, b }` produces
            // rest without the excluded properties.  Without this, `rest` would
            // keep all of T's constraint properties and trigger false TS2783.
            let constraint = constraint?;
            query::object_shape(self.ctx.types, constraint)
        });

        // Object rest follows the same property-collection rules as object spread:
        // drop readonly, prototype members, private/protected members, and
        // compiler-only private-brand properties before excluding named siblings.
        let mut remaining_props = self.collect_object_spread_properties(type_id);
        if remaining_props.is_empty()
            && query::object_shape(self.ctx.types, type_id).is_none()
            && let Some(constraint) = constraint
        {
            remaining_props = self.collect_object_spread_properties(constraint);
        }

        let remaining_props: Vec<_> = remaining_props
            .iter()
            .filter(|prop| {
                let name = self.ctx.types.resolve_atom_ref(prop.name);
                !excluded.iter().any(|ex| ex == name.as_ref())
            })
            .cloned()
            .collect();

        let Some(shape) = shape else {
            return if !remaining_props.is_empty()
                || query::is_object_like_type(self.ctx.types, type_id)
            {
                self.ctx.types.factory().object(remaining_props)
            } else {
                type_id
            };
        };

        // Preserve index signatures and object flags for object-rest types.
        // Rest results are structural copies, so they must not retain the
        // source type's nominal symbol (e.g. class identity).
        if shape.string_index.is_some() || shape.number_index.is_some() {
            let mut rest_shape = shape.as_ref().clone();
            rest_shape.properties = remaining_props;
            rest_shape.symbol = None;
            self.ctx.types.factory().object_with_index(rest_shape)
        } else {
            self.ctx.types.factory().object_with_flags_and_symbol(
                remaining_props,
                shape.flags,
                None,
            )
        }
    }

    /// Rest bindings from tuple members should produce an array type.
    /// Variadic tuple members can already carry array types (`...T[]`), so avoid
    /// wrapping those into nested arrays.
    fn rest_binding_array_type(&self, tuple_member_type: TypeId) -> TypeId {
        let tuple_member_type = query::unwrap_readonly_deep(self.ctx.types, tuple_member_type);
        if query::array_element_type(self.ctx.types, tuple_member_type).is_some() {
            tuple_member_type
        } else {
            self.ctx.types.factory().array(tuple_member_type)
        }
    }

    /// Extract a static property name from a binding element.
    ///
    /// Handles the following patterns:
    /// - `{ x }` → "x" (shorthand, name is the property)
    /// - `{ x: a }` → "x" (identifier property name)
    /// - `{ 'b': a }` → "b" (string literal property name)
    /// - `{ ['b']: a }` → "b" (computed with string literal)
    /// - `{ [ident]: a }` → "ident" (computed with identifier)
    ///
    /// Returns None for truly dynamic computed keys (e.g., `{ [expr]: a }`).
    fn extract_binding_property_name(
        &mut self,
        element_data: &tsz_parser::parser::node::BindingElementData,
    ) -> Option<String> {
        if element_data.property_name.is_some() {
            self.get_property_name_resolved(element_data.property_name)
        } else {
            // Shorthand: { x } — the name itself is the property name
            let name_node = self.ctx.arena.get(element_data.name)?;
            self.ctx
                .arena
                .get_identifier(name_node)
                .map(|ident| ident.escaped_text.clone())
        }
    }

    fn is_untyped_parameter_binding_pattern_without_context(&self, pattern_idx: NodeIndex) -> bool {
        if self.ctx.contextual_type.is_some() {
            return false;
        }
        let Some(ext) = self.ctx.arena.get_extended(pattern_idx) else {
            return false;
        };
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return false;
        }
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::PARAMETER {
            return false;
        }
        let Some(param) = self.ctx.arena.get_parameter(parent_node) else {
            return false;
        };
        param.name == pattern_idx && param.type_annotation.is_none()
    }

    /// Build a contextual type from a binding pattern's structure.
    ///
    /// Used to provide contextual typing for array literals in destructuring
    /// initializers so that `var [a, b, c] = [1, "hello", true]` produces
    /// positional tuple types (a=number, b=string) instead of a widened union.
    ///
    /// - Array binding patterns → tuple types with `any` elements
    /// - Object binding patterns → object types with properties typed from defaults
    /// - Nested patterns → recursively structured contextual types
    ///
    /// When a binding element has a default initializer (e.g., `{ f = (x: string) => x.length }`),
    /// the default's type is used instead of `any`. This enables contextual typing to flow
    /// from binding pattern defaults into generic function return type seeding.
    pub(crate) fn build_contextual_type_from_pattern(
        &mut self,
        pattern_idx: NodeIndex,
    ) -> Option<TypeId> {
        let pattern_node = self.ctx.arena.get(pattern_idx)?;
        let pattern_data = self.ctx.arena.get_binding_pattern(pattern_node)?;
        let elem_indices: Vec<NodeIndex> = pattern_data.elements.nodes.clone();
        let pattern_kind = pattern_node.kind;
        let factory = self.ctx.types.factory();

        if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            let mut tuple_elements = Vec::new();
            for &elem_idx in &elem_indices {
                let elem_type = self
                    .ctx
                    .arena
                    .get(elem_idx)
                    .and_then(|elem_node| self.ctx.arena.get_binding_element(elem_node))
                    .and_then(|elem_data| {
                        let name_node = self.ctx.arena.get(elem_data.name)?;
                        if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        {
                            Some(elem_data.name)
                        } else {
                            None
                        }
                    });
                let elem_type = if let Some(pattern_name) = elem_type {
                    self.build_contextual_type_from_pattern(pattern_name)
                        .unwrap_or(TypeId::ANY)
                } else {
                    TypeId::ANY
                };
                tuple_elements.push(tsz_solver::TupleElement {
                    type_id: elem_type,
                    optional: false,
                    rest: false,
                    name: None,
                });
            }
            Some(factory.tuple(tuple_elements))
        } else if pattern_kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            let mut properties = Vec::new();
            for &elem_idx in &elem_indices {
                let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                    continue;
                };
                let Some(elem_data) = self.ctx.arena.get_binding_element(elem_node) else {
                    continue;
                };

                // Skip rest elements — `...rest` is not a named property in the contextual type.
                if elem_data.dot_dot_dot_token {
                    continue;
                }

                // Get the property name
                let prop_name = if elem_data.property_name.is_some() {
                    self.ctx
                        .arena
                        .get(elem_data.property_name)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .map(|id| id.escaped_text.clone())
                } else {
                    self.ctx
                        .arena
                        .get(elem_data.name)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .map(|id| id.escaped_text.clone())
                };

                let Some(name_str) = prop_name else {
                    continue;
                };

                let initializer = elem_data.initializer;
                let name_idx = elem_data.name;

                // For nested patterns, recursively build contextual type.
                // For elements with default initializers, use the default's type
                // instead of `any` so the contextual type carries useful info
                // (e.g., `{ f = (x: string) => x.length }` → f: (x: string) => number).
                let name_kind = self.ctx.arena.get(name_idx).map(|n| n.kind);
                let prop_type = if matches!(
                    name_kind,
                    Some(
                        k
                    ) if k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        || k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                ) {
                    self.build_contextual_type_from_pattern(name_idx)
                        .unwrap_or(TypeId::ANY)
                } else if initializer.is_some() {
                    let init_type = self.get_type_of_node(initializer);
                    if init_type != TypeId::ANY
                        && init_type != TypeId::UNKNOWN
                        && init_type != TypeId::ERROR
                    {
                        init_type
                    } else {
                        TypeId::ANY
                    }
                } else {
                    TypeId::ANY
                };

                let atom = self.ctx.types.intern_string(&name_str);
                properties.push(tsz_solver::PropertyInfo::new(atom, prop_type));
            }
            Some(factory.object(properties))
        } else {
            None
        }
    }
}
