//! Destructuring pattern type resolution and validation.

use crate::query_boundaries::state_checking as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        let pattern_kind = pattern_node.kind;
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

            let element_type = if parent_type == TypeId::ANY {
                TypeId::ANY
            } else {
                self.get_binding_element_type(pattern_kind, i, parent_type, element_data)
            };

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
                    && self.is_only_undefined_or_null(element_type)
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

        while let Some((curr_pattern_idx, curr_source_type, curr_kind, base_path)) = stack.pop() {
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

                if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                {
                    let nested_source_type =
                        self.get_binding_element_type(curr_kind, i, curr_source_type, element_data);
                    stack.push((
                        element_data.name,
                        nested_source_type,
                        name_node.kind,
                        property_name,
                    ));
                }
            }
        }
    }

    /// Get the expected type for a binding element from its parent type.
    pub(crate) fn get_binding_element_type(
        &mut self,
        pattern_kind: u16,
        element_index: usize,
        parent_type: TypeId,
        element_data: &tsz_parser::parser::node::BindingElementData,
    ) -> TypeId {
        // Resolve Application/Lazy types to their concrete form so that
        // union members, object shapes, and tuple elements are accessible.
        let parent_type = self.evaluate_type_for_assignability(parent_type);

        // Array binding patterns use the element position.
        if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            if parent_type == TypeId::UNKNOWN || parent_type == TypeId::ERROR {
                return parent_type;
            }

            // For union types of tuples/arrays, resolve element type from each member
            if let Some(members) = query::union_members(self.ctx.types, parent_type) {
                let mut elem_types = Vec::new();
                let factory = self.ctx.types.factory();
                for member in members {
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
                        elem_types.push(elem);
                    } else if let Some(elems) = query::tuple_elements(self.ctx.types, member)
                        && let Some(e) = elems.get(element_index)
                    {
                        elem_types.push(e.type_id);
                    }
                }
                return if elem_types.is_empty() {
                    TypeId::ANY
                } else if elem_types.len() == 1 {
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
                elem
            } else if let Some(elems) = query::tuple_elements(self.ctx.types, array_like) {
                if let Some(e) = elems.get(element_index) {
                    e.type_id
                } else {
                    let has_rest_tail = elems.last().is_some_and(|element| element.rest);
                    if !has_rest_tail {
                        self.error_at_node(
                            element_data.name,
                            &format!(
                                "Tuple type of length '{}' has no element at index '{}'.",
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

        let property_optional_type = |property_type: TypeId, optional: bool| {
            if optional {
                self.ctx
                    .types
                    .factory()
                    .union(vec![property_type, TypeId::UNDEFINED])
            } else {
                property_type
            }
        };

        // Get the property name or index
        if element_data.property_name.is_some() {
            // For computed keys in object binding patterns (e.g. `{ [k]: v }`),
            // check index signatures when the key is not a simple identifier key.
            // This aligns with TS2537 behavior for destructuring from `{}`.
            let computed_expr = self
                .ctx
                .arena
                .get(element_data.property_name)
                .and_then(|prop_node| self.ctx.arena.get_computed_property(prop_node))
                .map(|computed| computed.expression);
            let computed_is_identifier = computed_expr
                .and_then(|expr_idx| {
                    self.ctx
                        .arena
                        .get(expr_idx)
                        .and_then(|expr_node| self.ctx.arena.get_identifier(expr_node))
                })
                .is_some();

            if !computed_is_identifier {
                let key_type =
                    computed_expr.map_or(TypeId::ANY, |expr_idx| self.get_type_of_node(expr_idx));
                let key_is_string = key_type == TypeId::STRING;
                let key_is_number = key_type == TypeId::NUMBER;

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
                        self.error_at_node(
                            element_data.property_name,
                            &message,
                            crate::diagnostics::diagnostic_codes::TYPE_HAS_NO_MATCHING_INDEX_SIGNATURE_FOR_TYPE,
                        );
                    }
                }
            }
        }

        let property_name = if element_data.property_name.is_some() {
            // { x: a } - property_name is "x"
            if let Some(prop_node) = self.ctx.arena.get(element_data.property_name) {
                self.ctx
                    .arena
                    .get_identifier(prop_node)
                    .map(|ident| ident.escaped_text.clone())
            } else {
                None
            }
        } else {
            // { x } - the name itself is the property name
            if let Some(name_node) = self.ctx.arena.get(element_data.name) {
                self.ctx
                    .arena
                    .get_identifier(name_node)
                    .map(|ident| ident.escaped_text.clone())
            } else {
                None
            }
        };

        if parent_type == TypeId::UNKNOWN {
            if let Some(prop_name_str) = property_name.as_deref() {
                let error_node = if element_data.property_name.is_some() {
                    element_data.property_name
                } else if element_data.name.is_some() {
                    element_data.name
                } else {
                    NodeIndex::NONE
                };
                self.error_property_not_exist_at(prop_name_str, parent_type, error_node);
            }
            return TypeId::UNKNOWN;
        }

        if let Some(ref prop_name_str) = property_name {
            // Look up the property type in the parent type.
            // For union types, resolve the property in each member and union the results.
            if let Some(members) = query::union_members(self.ctx.types, parent_type) {
                let mut prop_types = Vec::new();
                for member in members {
                    if let Some(prop) = tsz_solver::type_queries::find_property_in_object_by_str(
                        self.ctx.types,
                        member,
                        prop_name_str,
                    ) {
                        prop_types.push(property_optional_type(prop.type_id, prop.optional));
                    }
                }
                if prop_types.is_empty() {
                    TypeId::ANY
                } else {
                    tsz_solver::utils::union_or_single(self.ctx.types, prop_types)
                }
            } else if let Some(prop) = tsz_solver::type_queries::find_property_in_object_by_str(
                self.ctx.types,
                parent_type,
                prop_name_str,
            ) {
                property_optional_type(prop.type_id, prop.optional)
            } else {
                TypeId::ANY
            }
        } else {
            TypeId::ANY
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

    /// Check if a type consists only of `undefined` and/or `null`.
    /// Used for widening to `any` under `strict: false`.
    /// Returns true for: `undefined`, `null`, `undefined | null`
    fn is_only_undefined_or_null(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::UNDEFINED || type_id == TypeId::NULL {
            return true;
        }
        // Check for union of undefined/null
        if let Some(members) = query::union_members(self.ctx.types, type_id) {
            return members
                .iter()
                .all(|&m| m == TypeId::UNDEFINED || m == TypeId::NULL || m == type_id);
        }
        false
    }
}
