impl<'a> CheckerState<'a> {
    fn preserve_actual_lib_namespace_binding_parent_type(
        &mut self,
        original_type: TypeId,
        evaluated_type: TypeId,
    ) -> TypeId {
        let lazy_def_id =
            crate::query_boundaries::common::lazy_def_id(self.ctx.types, original_type);
        let sym_id = lazy_def_id
            .and_then(|def_id| self.ctx.def_to_symbol_id(def_id))
            .or_else(|| {
                crate::query_boundaries::common::type_shape_symbol(self.ctx.types, original_type)
            });

        let (export_name, require_symbol_match) = if let Some(sym_id) = sym_id {
            let lib_binders = self.get_lib_binders();
            let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
                return evaluated_type;
            };
            if self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id) {
                (symbol.escaped_name.clone(), Some(sym_id))
            } else if symbol.parent.is_some()
                && self.ctx.symbol_is_from_actual_or_cloned_lib(symbol.parent)
            {
                (symbol.escaped_name.clone(), None)
            } else {
                return evaluated_type;
            }
        } else {
            let Some(def_id) = lazy_def_id else {
                return evaluated_type;
            };
            let Some(def_info) = self.ctx.definition_store.get(def_id) else {
                return evaluated_type;
            };
            let name = self.ctx.types.resolve_atom_ref(def_info.name).to_string();
            if name.is_empty() {
                return evaluated_type;
            }
            (name, None)
        };

        let namespace = "Intl";
        let Some(export_sym_id) = self.resolve_lib_namespace_export_symbol(namespace, &export_name)
        else {
            return evaluated_type;
        };
        if require_symbol_match.is_some_and(|sym_id| export_sym_id != sym_id) {
            return evaluated_type;
        }

        let cache_name = format!("{namespace}.{export_name}");
        self.ctx.lib_type_resolution_cache.remove(&cache_name);
        self.resolve_lib_interface_type_by_symbol(&cache_name, export_sym_id)
            .unwrap_or(evaluated_type)
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
            let keys_result = self.get_element_access_type_for_literal_keys(
                literal_parent_type,
                string_keys,
                false,
            );
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
            && let Some(result_type) = self.get_element_access_type_for_literal_number_keys(
                literal_parent_type,
                number_keys,
                false,
            )
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

    #[allow(dead_code)]
    fn get_binding_element_computed_key_type(
        &mut self,
        pattern_idx: NodeIndex,
        expr_idx: NodeIndex,
    ) -> TypeId {
        self.get_binding_element_computed_key_type_with_request(
            pattern_idx,
            expr_idx,
            &TypingRequest::NONE,
        )
    }

    fn get_binding_element_computed_key_type_with_request(
        &mut self,
        pattern_idx: NodeIndex,
        expr_idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let prev_checking = self.ctx.checking_computed_property_name.take();
        self.ctx.checking_computed_property_name = Some(expr_idx);
        let key_request = request.read().contextual_opt(None);
        let mut key_type = self.get_type_of_node_with_request(expr_idx, &key_request);
        self.ctx.checking_computed_property_name = prev_checking;
        self.ctx.preserve_literal_types = prev_preserve;

        let is_identifier = self
            .ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16);
        if is_identifier && let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) {
            let base_key_type = self
                .get_binding_identifier_initializer_key_type_with_request(sym_id, request)
                .unwrap_or(key_type);
            // When the identifier resolves to a unique symbol but the initializer
            // type is plain `symbol` (e.g. `const sa = Symbol()`), prefer the
            // identifier's type. The initializer `Symbol()` returns `symbol`, but
            // the const variable's type is narrowed to `typeof sa` (unique symbol)
            // which is more specific and correct for property key lookups.
            let effective_key = if base_key_type == TypeId::SYMBOL
                && crate::query_boundaries::common::is_unique_symbol_type(self.ctx.types, key_type)
            {
                key_type
            } else {
                base_key_type
            };
            let mut key_types = vec![effective_key];
            self.collect_enclosing_default_assignment_key_types(
                pattern_idx,
                sym_id,
                &mut key_types,
                request,
            );
            if key_types.len() > 1 {
                key_type = self.ctx.types.factory().union(key_types);
            } else {
                key_type = effective_key;
            }
        }

        key_type
    }

    fn collect_enclosing_default_assignment_key_types(
        &mut self,
        pattern_idx: NodeIndex,
        sym_id: SymbolId,
        key_types: &mut Vec<TypeId>,
        request: &TypingRequest,
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
                self.collect_assignment_types_for_symbol(
                    binding.initializer,
                    sym_id,
                    key_types,
                    request,
                );
            }

            current = parent_idx;
        }
    }

    fn collect_assignment_types_for_symbol(
        &mut self,
        expr_idx: NodeIndex,
        sym_id: SymbolId,
        key_types: &mut Vec<TypeId>,
        request: &TypingRequest,
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
                        let request = request.read().contextual_opt(None);
                        let assigned_type =
                            self.get_type_of_node_with_request(binary.right, &request);
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

    #[allow(dead_code)]
    fn get_binding_identifier_initializer_key_type(&mut self, sym_id: SymbolId) -> Option<TypeId> {
        self.get_binding_identifier_initializer_key_type_with_request(sym_id, &TypingRequest::NONE)
    }

    fn get_binding_identifier_initializer_key_type_with_request(
        &mut self,
        sym_id: SymbolId,
        request: &TypingRequest,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = symbol.primary_declaration()?;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        if var_decl.initializer.is_none() {
            return None;
        }

        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let request = request.read().contextual_opt(None);
        let init_type = self.get_type_of_node_with_request(var_decl.initializer, &request);
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

    fn is_untyped_parameter_binding_pattern_without_context(
        &self,
        pattern_idx: NodeIndex,
        request: &TypingRequest,
    ) -> bool {
        if request.contextual_type.is_some() {
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
    /// Array patterns → tuple with `any`; object patterns → typed properties.
    /// Default initializers (e.g., `{ f = (x: string) => x.length }`) seed
    /// property types instead of `any` to enable contextual typing.
    #[allow(dead_code)]
    pub(crate) fn build_contextual_type_from_pattern(
        &mut self,
        pattern_idx: NodeIndex,
    ) -> Option<TypeId> {
        self.build_contextual_type_from_pattern_with_request(pattern_idx, &TypingRequest::NONE)
    }

    pub(crate) fn build_contextual_type_from_pattern_with_request(
        &mut self,
        pattern_idx: NodeIndex,
        request: &TypingRequest,
    ) -> Option<TypeId> {
        let pattern_node = self.ctx.arena.get(pattern_idx)?;
        let pattern_data = self.ctx.arena.get_binding_pattern(pattern_node)?;
        let elem_indices: Vec<NodeIndex> = pattern_data.elements.nodes.clone();
        let pattern_kind = pattern_node.kind;
        let factory = self.ctx.types.factory();

        if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            let mut tuple_elements = Vec::new();
            for &elem_idx in &elem_indices {
                // Copy the Copy-able binding fields out so the immutable arena
                // borrow is released before the mutable `self` calls below.
                let binding = self
                    .ctx
                    .arena
                    .get(elem_idx)
                    .and_then(|elem_node| self.ctx.arena.get_binding_element(elem_node))
                    .map(|elem_data| {
                        (
                            elem_data.name,
                            elem_data.dot_dot_dot_token,
                            elem_data.initializer,
                        )
                    });
                let elem_type = match binding {
                    Some((name, _, _))
                        if matches!(
                            self.ctx.arena.kind_at(name),
                            Some(k) if k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                || k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        ) =>
                    {
                        self.build_contextual_type_from_pattern_with_request(name, request)
                            .unwrap_or(TypeId::ANY)
                    }
                    // A non-rest element default contributes its (literal) type as the
                    // contextual element type, mirroring tsc's `getTypeFromBindingElement`.
                    // This is what makes a fresh array-literal initializer preserve the
                    // positional literal element when the binding has a default
                    // (`const [first = 0] = [10, 20]` → element context `0`, so the
                    // source `10` stays `10` instead of widening to `number`). The
                    // default's literal type must be preserved (not widened to `number`)
                    // because tsc keeps the source literal only when its primitive kind
                    // matches the default's. Elements without a default keep `any`, so
                    // the source element widens as usual.
                    Some((_, false, initializer)) if initializer.is_some() => {
                        let init_request = request.read().contextual_opt(None);
                        let prev_preserve = self.ctx.preserve_literal_types;
                        self.ctx.preserve_literal_types = true;
                        let init_type =
                            self.get_type_of_node_with_request(initializer, &init_request);
                        self.ctx.preserve_literal_types = prev_preserve;
                        if init_type == TypeId::UNKNOWN || init_type == TypeId::ERROR {
                            TypeId::ANY
                        } else {
                            init_type
                        }
                    }
                    _ => TypeId::ANY,
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
            // Collect all binding element names for intra-binding-pattern reference detection.
            // When a binding element's default references another binding in the same pattern
            // (e.g., `{ fn1 = (x: number) => 0, fn2 = fn1 }`), the contextual type for that
            // property must be `any` to match tsc behavior and avoid circular contextual typing.
            let binding_names: Vec<Option<String>> = elem_indices
                .iter()
                .map(|&idx| {
                    self.ctx
                        .arena
                        .get(idx)
                        .and_then(|n| self.ctx.arena.get_binding_element(n))
                        .and_then(|elem| {
                            self.ctx
                                .arena
                                .get(elem.name)
                                .and_then(|n| self.ctx.arena.get_identifier(n))
                                .map(|id| id.escaped_text.clone())
                        })
                })
                .collect();

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

                // Check for intra-binding-pattern reference: if the initializer is an
                // identifier that references another binding in the same pattern, skip
                // this property in the contextual type entirely. This matches tsc behavior
                // (TypeScript#59177) where intra-binding references cause the contextual
                // type for that property to be absent, so arrow function parameters in the
                // RHS object literal don't get contextual types and TS7006 fires correctly.
                if initializer.is_some() {
                    let is_intra_binding_ref = self
                        .ctx
                        .arena
                        .get(initializer)
                        .filter(|init_node| init_node.kind == SyntaxKind::Identifier as u16)
                        .and_then(|init_node| self.ctx.arena.get_identifier(init_node))
                        .is_some_and(|init_id| {
                            binding_names.iter().any(|name| {
                                name.as_ref().is_some_and(|n| *n == init_id.escaped_text)
                            })
                        });
                    if is_intra_binding_ref {
                        continue;
                    }
                }

                // For nested patterns, recursively build contextual type.
                // For elements with default initializers, use the default's type
                // instead of `any` so the contextual type carries useful info
                // (e.g., `{ f = (x: string) => x.length }` → f: (x: string) => number).
                let name_kind = self.ctx.arena.kind_at(name_idx);
                let prop_type = if matches!(
                    name_kind,
                    Some(
                        k
                    ) if k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        || k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                ) {
                    self.build_contextual_type_from_pattern_with_request(name_idx, request)
                        .unwrap_or(TypeId::ANY)
                } else if initializer.is_some() {
                    let request = request.read().contextual_opt(None);
                    let init_type = self.get_type_of_node_with_request(initializer, &request);
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
