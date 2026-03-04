//! Destructuring pattern type resolution and validation.

use crate::query_boundaries::state::checking as query;
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
                    let nested_source_type = self.get_binding_element_type(
                        curr_pattern_idx,
                        i,
                        curr_source_type,
                        element_data,
                    );
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
                        elem_types.push(elem);
                    } else if let Some(elems) = query::tuple_elements(self.ctx.types, member)
                        && let Some(e) = elems.get(element_index)
                    {
                        elem_types.push(e.type_id);
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
                elem
            } else if let Some(elems) = query::tuple_elements(self.ctx.types, array_like) {
                if let Some(e) = elems.get(element_index) {
                    e.type_id
                } else {
                    let has_rest_tail = elems.last().is_some_and(|element| element.rest);
                    if !has_rest_tail {
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

        // Extract the static property name from binding element.
        // Handles: { x }, { x: a }, { 'b': a }, { ['b']: a }, { [ident]: a }.
        let property_name = self.extract_binding_property_name(element_data);

        // For computed keys in object binding patterns (e.g. `{ [k]: v }`),
        // check index signatures when the key resolves to a dynamic type
        // (string or number, not a literal matching a known property).
        if element_data.property_name.is_some() {
            let computed_expr = self
                .ctx
                .arena
                .get(element_data.property_name)
                .and_then(|prop_node| self.ctx.arena.get_computed_property(prop_node))
                .map(|computed| computed.expression);

            // Only check index signatures for truly dynamic keys (not identifiers
            // or string/numeric literals that resolve to known properties).
            if computed_expr.is_some() && property_name.is_none() {
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
            if let Some(prop_name_str) = property_name.as_deref() {
                let error_node = if element_data.property_name.is_some() {
                    element_data.property_name
                } else if element_data.name.is_some() {
                    element_data.name
                } else {
                    NodeIndex::NONE
                };
                if element_data.initializer.is_none() {
                    self.error_property_not_exist_at(prop_name_str, parent_type, error_node);
                }
            }
            return TypeId::UNKNOWN;
        }

        if let Some(ref prop_name_str) = property_name {
            use tsz_solver::operations::property::PropertyAccessResult;
            let prop_access_result =
                self.resolve_property_access_with_env(parent_type, prop_name_str);
            match prop_access_result {
                PropertyAccessResult::Success { type_id, .. } => type_id,
                PropertyAccessResult::PropertyNotFound { .. } => {
                    let error_node = if element_data.property_name.is_some() {
                        element_data.property_name
                    } else if element_data.name.is_some() {
                        element_data.name
                    } else {
                        NodeIndex::NONE
                    };
                    if element_data.initializer.is_none() && !defer_property_not_found {
                        self.error_property_not_exist_at(prop_name_str, parent_type, error_node);
                    }
                    TypeId::ANY
                }
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    property_type.unwrap_or(TypeId::ANY)
                }
                PropertyAccessResult::IsUnknown => TypeId::ANY,
            }
        } else {
            TypeId::ANY
        }
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
    fn compute_object_rest_type(&self, pattern_idx: NodeIndex, parent_type: TypeId) -> TypeId {
        // Collect the names of all non-rest sibling properties in this binding pattern.
        let excluded = self.collect_non_rest_property_names(pattern_idx);
        if excluded.is_empty() {
            return parent_type;
        }

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
    fn omit_properties_from_type(&self, type_id: TypeId, excluded: &[String]) -> TypeId {
        let shape = query::object_shape(self.ctx.types, type_id).or_else(|| {
            // For type parameters, use the constraint's shape so that
            // `{ a, ...rest } = obj` where `obj: T extends { a, b }` produces
            // rest without the excluded properties.  Without this, `rest` would
            // keep all of T's constraint properties and trigger false TS2783.
            let constraint = query::type_parameter_constraint(self.ctx.types, type_id)?;
            query::object_shape(self.ctx.types, constraint)
        });
        let Some(shape) = shape else {
            return type_id;
        };

        let remaining_props: Vec<_> = shape
            .properties
            .iter()
            .filter(|prop| {
                let name = self.ctx.types.resolve_atom_ref(prop.name);
                !excluded.iter().any(|ex| ex == name.as_ref())
            })
            .cloned()
            .collect();

        self.ctx.types.factory().object(remaining_props)
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
        &self,
        element_data: &tsz_parser::parser::node::BindingElementData,
    ) -> Option<String> {
        if element_data.property_name.is_some() {
            let prop_node = self.ctx.arena.get(element_data.property_name)?;
            // Try identifier: { x: a }
            if let Some(ident) = self.ctx.arena.get_identifier(prop_node) {
                return Some(ident.escaped_text.clone());
            }
            // Try string/numeric literal: { 'b': a }
            if let Some(lit) = self.ctx.arena.get_literal(prop_node) {
                return Some(lit.text.clone());
            }
            // Try computed property with static literal value: { ['b']: a } or { [42]: a }
            // Note: { [ident]: a } is NOT static — the property depends on the runtime
            // value of `ident`, so we return None for computed-with-identifier.
            if let Some(computed) = self.ctx.arena.get_computed_property(prop_node) {
                let expr_node = self.ctx.arena.get(computed.expression)?;
                if let Some(lit) = self.ctx.arena.get_literal(expr_node) {
                    return Some(lit.text.clone());
                }
            }
            None
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
    /// - Object binding patterns → object types with `any` properties
    /// - Nested patterns → recursively structured contextual types
    pub(crate) fn build_contextual_type_from_pattern(
        &self,
        pattern_idx: NodeIndex,
    ) -> Option<TypeId> {
        let pattern_node = self.ctx.arena.get(pattern_idx)?;
        let pattern_data = self.ctx.arena.get_binding_pattern(pattern_node)?;
        let factory = self.ctx.types.factory();

        if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            let tuple_elements: Vec<tsz_solver::TupleElement> = pattern_data
                .elements
                .nodes
                .iter()
                .enumerate()
                .map(|(_, &elem_idx)| {
                    // For nested binding patterns, recursively build the contextual type.
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
                                self.build_contextual_type_from_pattern(elem_data.name)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(TypeId::ANY);

                    tsz_solver::TupleElement {
                        type_id: elem_type,
                        optional: false,
                        rest: false,
                        name: None,
                    }
                })
                .collect();
            Some(factory.tuple(tuple_elements))
        } else if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            let mut properties = Vec::new();
            for &elem_idx in &pattern_data.elements.nodes {
                let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                    continue;
                };
                let Some(elem_data) = self.ctx.arena.get_binding_element(elem_node) else {
                    continue;
                };

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

                // For nested patterns, recursively build contextual type
                let prop_type = self
                    .ctx
                    .arena
                    .get(elem_data.name)
                    .and_then(|name_node| {
                        if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        {
                            self.build_contextual_type_from_pattern(elem_data.name)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(TypeId::ANY);

                let atom = self.ctx.types.intern_string(&name_str);
                properties.push(tsz_solver::PropertyInfo::new(atom, prop_type));
            }
            Some(factory.object(properties))
        } else {
            None
        }
    }
}
