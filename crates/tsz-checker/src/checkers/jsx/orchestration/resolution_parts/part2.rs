impl<'a> CheckerState<'a> {
    pub(in crate::checkers_domain::jsx) fn resolve_symbol_id_from_origin(
        &mut self,
        sym_id: SymbolId,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        if visited.contains(&sym_id) {
            return None;
        }
        visited.push(sym_id);

        let source_file_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .unwrap_or(self.ctx.current_file_idx);

        let (import_module, import_name, escaped_name, decl_idx) =
            if let Some(symbol) = self.get_cross_file_symbol(sym_id) {
                if !symbol.has_any_flags(symbol_flags::ALIAS) {
                    return Some(sym_id);
                }
                (
                    symbol.import_module.clone(),
                    symbol.import_name.clone(),
                    symbol.escaped_name.clone(),
                    symbol.primary_declaration()?,
                )
            } else {
                let lib_binders = self.get_lib_binders();
                let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                if !symbol.has_any_flags(symbol_flags::ALIAS) {
                    return Some(sym_id);
                }
                (
                    symbol.import_module.clone(),
                    symbol.import_name.clone(),
                    symbol.escaped_name.clone(),
                    symbol.primary_declaration()?,
                )
            };

        if let Some(module_name) = import_module.as_deref() {
            let export_name = import_name.as_deref().unwrap_or(escaped_name.as_str());
            let target_sym_id = self.resolve_cross_file_export_from_file(
                module_name,
                export_name,
                Some(source_file_idx),
            )?;
            return self.resolve_symbol_id_from_origin(target_sym_id, visited);
        }

        let arena = self.ctx.get_arena_for_file(source_file_idx as u32);
        let decl_node = arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return Some(sym_id);
        }
        let import = arena.get_import_decl(decl_node)?;
        let entity_name = Self::entity_name_text_in_arena(arena, import.module_specifier)?;
        let target_sym_id =
            self.resolve_entity_name_from_file(source_file_idx, &entity_name, visited)?;
        Some(target_sym_id)
    }

    pub(in crate::checkers_domain::jsx) fn resolve_entity_name_from_file(
        &mut self,
        file_idx: usize,
        name: &str,
        visited: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let binder = self.ctx.get_binder_for_file(file_idx)?;
        let mut segments = name.split('.');
        let root_name = segments.next()?;
        let mut current_sym = binder.file_locals.get(root_name)?;
        self.ctx.register_symbol_file_target(current_sym, file_idx);
        current_sym = self
            .resolve_symbol_id_from_origin(current_sym, visited)
            .unwrap_or(current_sym);

        for segment in segments {
            let current_file_idx = self
                .ctx
                .resolve_symbol_file_index(current_sym)
                .unwrap_or(file_idx);
            let member_sym_id = if let Some(symbol) = self.get_cross_file_symbol(current_sym) {
                symbol
                    .exports
                    .as_ref()
                    .and_then(|exports| exports.get(segment))
                    .or_else(|| {
                        symbol
                            .members
                            .as_ref()
                            .and_then(|members| members.get(segment))
                    })?
            } else {
                let lib_binders = self.get_lib_binders();
                let symbol = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(current_sym, &lib_binders)?;
                symbol
                    .exports
                    .as_ref()
                    .and_then(|exports| exports.get(segment))
                    .or_else(|| {
                        symbol
                            .members
                            .as_ref()
                            .and_then(|members| members.get(segment))
                    })?
            };
            self.ctx
                .register_symbol_file_target(member_sym_id, current_file_idx);
            current_sym = self
                .resolve_symbol_id_from_origin(member_sym_id, visited)
                .unwrap_or(member_sym_id);
        }

        Some(current_sym)
    }

    pub(in crate::checkers_domain::jsx) fn entity_name_text_in_arena(
        arena: &NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        entity_name_text_in_arena(arena, idx)
    }

    pub(in crate::checkers_domain::jsx) fn get_intrinsic_elements_symbol_id(
        &mut self,
    ) -> Option<SymbolId> {
        if let Some(cached) = self.ctx.jsx_intrinsic_elements_symbol_cache {
            return cached;
        }
        let resolved = self.get_jsx_namespace_export_symbol_id("IntrinsicElements");
        self.ctx.jsx_intrinsic_elements_symbol_cache = Some(resolved);
        resolved
    }

    /// Get the JSX.IntrinsicElements interface type (maps tag names to prop types).
    pub(crate) fn get_intrinsic_elements_type(&mut self) -> Option<TypeId> {
        if let Some(cached) = self.ctx.jsx_intrinsic_elements_type_cache {
            return cached;
        }
        let mut intrinsic_element_symbols = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        if let Some(primary) = self.get_intrinsic_elements_symbol_id()
            && seen.insert(primary)
        {
            intrinsic_element_symbols.push(primary);
        }
        for sym_id in self.get_jsx_namespace_export_symbol_ids("IntrinsicElements") {
            if seen.insert(sym_id) {
                intrinsic_element_symbols.push(sym_id);
            }
        }
        let resolved = if intrinsic_element_symbols.is_empty() {
            None
        } else {
            let mut merged = TypeId::ERROR;
            for intrinsic_elements_sym_id in intrinsic_element_symbols {
                let ty = self.type_reference_symbol_type(intrinsic_elements_sym_id);
                if matches!(ty, TypeId::ERROR | TypeId::UNKNOWN) {
                    continue;
                }
                merged = if merged == TypeId::ERROR {
                    ty
                } else {
                    self.merge_interface_types(merged, ty)
                };
            }
            (merged != TypeId::ERROR).then_some(merged)
        };
        self.ctx.jsx_intrinsic_elements_type_cache = Some(resolved);
        resolved
    }

    /// Get the JSX.IntrinsicAttributes type (e.g. `{ key?: string }` in React).
    pub(in crate::checkers_domain::jsx) fn get_intrinsic_attributes_type(
        &mut self,
    ) -> Option<TypeId> {
        let ia_sym_id = self.get_jsx_namespace_export_symbol_id("IntrinsicAttributes")?;
        let ty = self.type_reference_symbol_type(ia_sym_id);
        let evaluated = self.evaluate_type_with_env(ty);
        if evaluated == TypeId::ANY || evaluated == TypeId::ERROR || evaluated == TypeId::UNKNOWN {
            return None;
        }
        Some(evaluated)
    }

    /// Get the JSX.Element type for fragments.
    ///
    /// Rule #36: Fragments resolve to JSX.Element type.
    pub(crate) fn get_jsx_element_type(&mut self, node_idx: NodeIndex) -> TypeId {
        self.check_jsx_factory_in_scope(node_idx);
        self.check_jsx_fragment_factory(node_idx);
        self.check_jsx_import_source(node_idx);

        // Try to resolve JSX.Element from the JSX namespace
        if let Some(element_sym_id) = self.get_jsx_namespace_export_symbol_id("Element") {
            return self.type_reference_symbol_type(element_sym_id);
        }
        // Note: tsc 6.0 never emits TS7026 about "JSX.Element" (0 occurrences).
        // TS7026 is only emitted about "JSX.IntrinsicElements" for intrinsic elements.
        // For fragments, tsc emits TS17016 (missing jsxFragmentFactory) instead.
        TypeId::ANY
    }

    /// Get JSX.Element type for return type checking (no factory diagnostics).
    pub(crate) fn get_jsx_element_type_for_check(&mut self) -> Option<TypeId> {
        let element_sym_id = self.get_jsx_namespace_export_symbol_id("Element")?;
        Some(self.type_reference_symbol_type(element_sym_id))
    }

    /// Get JSX.ElementClass type for class component return type checking.
    pub(in crate::checkers_domain::jsx) fn get_jsx_element_class_type(&mut self) -> Option<TypeId> {
        let element_class_sym_id = self.get_jsx_namespace_export_symbol_id("ElementClass")?;
        Some(self.type_reference_symbol_type(element_class_sym_id))
    }

    pub(in crate::checkers_domain::jsx) fn get_jsx_children_prop_name(&mut self) -> String {
        use tsz_common::checker_options::JsxMode;

        if matches!(
            self.effective_jsx_mode(),
            JsxMode::ReactJsx | JsxMode::ReactJsxDev
        ) {
            return "children".to_string();
        }

        let Some(eca_sym_id) = self.get_jsx_namespace_export_symbol_id("ElementChildrenAttribute")
        else {
            return "children".to_string();
        };

        let eca_type = self.type_reference_symbol_type(eca_sym_id);
        let evaluated = self.evaluate_type_with_env(eca_type);
        if evaluated == TypeId::UNKNOWN || evaluated == TypeId::ERROR {
            return "children".to_string();
        }

        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated)
        else {
            return "children".to_string();
        };

        shape
            .properties
            .first()
            .map(|prop| self.ctx.types.resolve_atom(prop.name))
            .unwrap_or_else(|| "children".to_string())
    }

    pub(crate) fn get_jsx_children_contextual_type(
        &mut self,
        opening_element_idx: NodeIndex,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(opening_element_idx)?;
        let jsx_opening = self.ctx.arena.get_jsx_opening(node)?;
        let tag_name_idx = jsx_opening.tag_name;
        let tag_name_node = self.ctx.arena.get(tag_name_idx)?;

        // Determine if intrinsic (lowercase) or component (uppercase/property access)
        let is_intrinsic = if tag_name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            self.ctx
                .arena
                .get_identifier(tag_name_node)
                .map(|id| id.escaped_text.as_str())
                .is_some_and(|n| n.chars().next().is_some_and(|c| c.is_ascii_lowercase()))
        } else {
            tag_name_node.kind == syntax_kind_ext::JSX_NAMESPACED_NAME
        };

        let props_type = if is_intrinsic {
            let tag_name = if tag_name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                self.ctx
                    .arena
                    .get_identifier(tag_name_node)
                    .map(|id| id.escaped_text.as_str().to_string())
            } else {
                // Namespaced tag
                self.ctx
                    .arena
                    .get_jsx_namespaced_name(tag_name_node)
                    .and_then(|ns| {
                        let ns_id = self.ctx.arena.get(ns.namespace)?;
                        let ns_text = self.ctx.arena.get_identifier(ns_id)?.escaped_text.as_str();
                        let name_id = self.ctx.arena.get(ns.name)?;
                        let name_text = self
                            .ctx
                            .arena
                            .get_identifier(name_id)?
                            .escaped_text
                            .as_str();
                        Some(format!("{ns_text}:{name_text}"))
                    })
            }?;
            let props =
                self.get_jsx_intrinsic_props_for_tag(opening_element_idx, &tag_name, false)?;
            if props == TypeId::ERROR {
                return None;
            }
            props
        } else {
            // Component: resolve tag name to get component type, extract props
            let component_type = self.compute_type_of_node(tag_name_idx);
            let resolved_component_type =
                self.normalize_jsx_component_type_for_resolution(component_type);
            if let Some(tag) = self.get_jsx_specific_string_literal_component_tag_name(
                tag_name_idx,
                resolved_component_type,
            ) && let Some(props) =
                self.get_jsx_intrinsic_props_for_tag(opening_element_idx, &tag, false)
                && props != TypeId::ERROR
            {
                props
            } else if let Some((props, _raw_has_type_params)) = self
                .recover_jsx_component_props_type(
                    jsx_opening.attributes,
                    component_type,
                    None,
                    &TypingRequest::NONE,
                )
            {
                self.narrow_jsx_props_union_from_attributes(jsx_opening.attributes, props)
            } else if self.is_generic_jsx_component(resolved_component_type) {
                // Generic component: return ANY to avoid false implicit-any
                // diagnostics for callback and destructuring children.
                return Some(TypeId::ANY);
            } else {
                return None;
            }
        };

        let child_count = self
            .get_jsx_body_child_nodes(jsx_opening.attributes)
            .map_or(0, |children| children.len());

        self.get_jsx_children_prop_type(props_type)
            .map(|children_type| {
                self.jsx_children_contextual_type_for_body_shape(children_type, child_count)
            })
    }

    /// Extract the attribute name from a JSX attribute name node.
    ///
    /// Handles both simple identifiers (`name`) and namespaced names (`ns:name`).
    /// Returns `None` if the node is neither.
    pub(crate) fn get_jsx_attribute_name(
        &self,
        name_node: &tsz_parser::parser::node::Node,
    ) -> Option<String> {
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            Some(ident.escaped_text.as_str().to_string())
        } else if let Some(ns) = self.ctx.arena.get_jsx_namespaced_name(name_node) {
            let ns_id = self.ctx.arena.get(ns.namespace)?;
            let ns_text = self.ctx.arena.get_identifier(ns_id)?.escaped_text.as_str();
            let name_id = self.ctx.arena.get(ns.name)?;
            let name_text = self
                .ctx
                .arena
                .get_identifier(name_id)?
                .escaped_text
                .as_str();
            Some(format!("{ns_text}:{name_text}"))
        } else {
            None
        }
    }

    /// Check if a specific attribute name exists as an EXPLICIT JSX attribute
    /// (not from a spread). Used for TS2710 double-specification detection.
    pub(in crate::checkers_domain::jsx) fn has_explicit_jsx_attribute(
        &self,
        attributes_idx: NodeIndex,
        name: &str,
    ) -> bool {
        self.find_explicit_jsx_attribute(attributes_idx, name)
            .is_some()
    }

    /// Find an explicit JSX attribute by name, returning the attribute's name node index.
    pub(in crate::checkers_domain::jsx) fn find_explicit_jsx_attribute(
        &self,
        attributes_idx: NodeIndex,
        name: &str,
    ) -> Option<NodeIndex> {
        let attrs_node = self.ctx.arena.get(attributes_idx)?;
        let attrs = self.ctx.arena.get_jsx_attributes(attrs_node)?;
        for &attr_idx in &attrs.properties.nodes {
            let attr_node = self.ctx.arena.get(attr_idx)?;
            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                let attr_data = self.ctx.arena.get_jsx_attribute(attr_node)?;
                let name_node = self.ctx.arena.get(attr_data.name)?;
                if let Some(attr_name) = self.get_jsx_attribute_name(name_node)
                    && attr_name == name
                {
                    return Some(attr_data.name);
                }
            }
        }
        None
    }

    pub(super) fn instantiate_jsx_component_with_type_args(
        &mut self,
        component_type: TypeId,
        type_args: &[TypeId],
    ) -> TypeId {
        if let Some(instantiated) =
            crate::query_boundaries::common::instantiate_function_with_type_args(
                self.ctx.types,
                component_type,
                type_args,
            )
        {
            return instantiated;
        }

        // Fallback: create Application for class components, type aliases,
        // and overloaded SFCs (Callable types)
        self.ctx
            .types
            .application(component_type, type_args.to_vec())
    }
}
