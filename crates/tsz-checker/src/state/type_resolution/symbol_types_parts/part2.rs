impl<'a> CheckerState<'a> {
    /// Like `type_reference_symbol_type` but also returns the type parameters used.
    /// Body and params must come from the SAME `push_type_parameters` call so `TypeId`s match during substitution.
    pub(crate) fn type_reference_symbol_type_with_params(
        &mut self,
        sym_id: SymbolId,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        use tsz_lowering::TypeLowering;

        let local_alias_symbol = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .filter(|symbol| symbol.has_any_flags(symbol_flags::ALIAS));

        if local_alias_symbol.is_none()
            && let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && self.should_delegate_dynamic_type_alias_owner(sym_id, file_idx)
        {
            if let Some(result) = self.delegate_cross_arena_symbol_resolution(sym_id) {
                return result;
            }
            if let Some(result) =
                self.direct_source_file_type_alias_result(sym_id, Some(file_idx), true)
            {
                return result;
            }
        }

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            tracing::debug!(
                sym_id = sym_id.0,
                name = %symbol.escaped_name,
                flags = symbol.flags,
                num_decls = symbol.declarations.len(),
                has_value_decl = symbol.value_declaration.is_some(),
                "type_reference_symbol_type_with_params: ENTRY"
            );
        }

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            if symbol.has_any_flags(symbol_flags::ALIAS) {
                if self
                    .ctx
                    .resolve_symbol_file_index(sym_id)
                    .is_some_and(|file_idx| {
                        file_idx != self.ctx.current_file_idx
                            && self.should_delegate_dynamic_type_alias_owner(sym_id, file_idx)
                    })
                    && self
                        .get_cross_file_symbol(sym_id)
                        .is_some_and(|target| target.has_any_flags(symbol_flags::TYPE_ALIAS))
                    && let Some((alias_type, params)) =
                        self.delegate_cross_arena_symbol_resolution(sym_id)
                    && alias_type != TypeId::UNKNOWN
                    && alias_type != TypeId::ERROR
                {
                    return (alias_type, params);
                }

                let mut visited = AliasCycleTracker::new();
                let resolved_alias = self.resolve_alias_symbol(sym_id, &mut visited);
                let alias_target = match resolved_alias {
                    Some(target_sym_id) if target_sym_id != sym_id => Some(target_sym_id),
                    _ => self.resolve_import_alias_cross_file(sym_id),
                };
                if let Some(target_sym_id) = alias_target {
                    let target_flags = self
                        .get_symbol_from_registered_file_target(target_sym_id)
                        .or_else(|| self.get_cross_file_symbol(target_sym_id))
                        .map(|s| s.flags)
                        .unwrap_or(0);
                    if target_flags
                        & (symbol_flags::CLASS
                            | symbol_flags::INTERFACE
                            | symbol_flags::TYPE_ALIAS
                            | symbol_flags::ENUM
                            | symbol_flags::TYPE_PARAMETER)
                        != 0
                    {
                        if target_flags & symbol_flags::CLASS != 0
                            && self
                                .ctx
                                .resolve_symbol_file_index(target_sym_id)
                                .is_some_and(|file_idx| file_idx != self.ctx.current_file_idx)
                            && let Some(result) =
                                self.delegate_cross_arena_class_instance_type(target_sym_id)
                        {
                            return result;
                        }
                        if target_sym_id != sym_id {
                            return self.type_reference_symbol_type_with_params(target_sym_id);
                        }
                        if self
                            .ctx
                            .resolve_symbol_file_index(target_sym_id)
                            .is_some_and(|file_idx| file_idx != self.ctx.current_file_idx)
                            && let Some(result) =
                                self.delegate_cross_arena_symbol_resolution(target_sym_id)
                        {
                            return result;
                        }
                    }
                }
            }

            // For classes, use class_instance_type_with_params_from_symbol which
            // returns both the instance type AND the type params used to build it.
            // class+interface merges (with or without namespace blocks) take this
            // path too: `get_class_instance_type_inner` already merges interface
            // declarations into the instance shape, so routing through the
            // interface branch would drop class instance members.
            if symbol.has_any_flags(symbol_flags::CLASS)
                && let Some((instance_type, params)) =
                    self.class_instance_type_with_params_from_symbol(sym_id)
            {
                // Store type parameters for DefId-based resolution
                if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                    self.ctx.insert_def_type_params(def_id, params.clone());
                }
                return (instance_type, params);
            }

            // When a symbol has both TYPE_ALIAS and INTERFACE flags (e.g., local
            // `type Request<T> = ...` merged with lib's `interface Request`), the
            // local type alias should take precedence. Check whether the TYPE_ALIAS
            // declaration lives in the current arena and skip the INTERFACE path if so.
            let prefer_type_alias_over_interface = symbol.has_any_flags(symbol_flags::TYPE_ALIAS)
                && symbol.has_any_flags(symbol_flags::INTERFACE)
                && symbol.declarations.iter().any(|&d| {
                    self.ctx
                        .arena
                        .get(d)
                        .and_then(|n| {
                            if n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                                let type_alias = self.ctx.arena.get_type_alias(n)?;
                                let name = self.ctx.arena.get_identifier_text(type_alias.name)?;
                                Some(name == symbol.escaped_name.as_str())
                            } else {
                                Some(false)
                            }
                        })
                        .unwrap_or(false)
                });

            // For interfaces, lower with type parameters and return both
            if symbol.has_any_flags(symbol_flags::INTERFACE)
                && !symbol.declarations.is_empty()
                && !prefer_type_alias_over_interface
            {
                // Build per-declaration arena pairs for multi-arena support
                // (e.g. Promise has declarations in lib.es5.d.ts, lib.es2018.promise.d.ts, etc.)
                let fallback_arena: &NodeArena = self
                    .ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .map_or(self.ctx.arena, |arena| arena.as_ref());

                // Detect whether any declaration arena entry points to an arena other
                // than the current file's arena. Previously the per-file
                // `declaration_arenas` map was pre-filtered to only contain such
                // non-local entries, so a simple `contains_key` was enough. The
                // program-wide `Arc`-shared map now contains entries for purely-local
                // declarations too, so we must check the arena contents explicitly.
                let has_declaration_arenas = symbol.declarations.iter().any(|&decl_idx| {
                    self.ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .is_some_and(|arenas| {
                            arenas
                                .iter()
                                .any(|a| !std::ptr::eq(a.as_ref(), self.ctx.arena))
                        })
                });
                let needs_text_based_resolution =
                    has_declaration_arenas || !std::ptr::eq(fallback_arena, self.ctx.arena);

                let decls_with_arenas: Vec<(NodeIndex, &NodeArena)> = symbol
                    .declarations
                    .iter()
                    .flat_map(|&decl_idx| {
                        if let Some(arenas) =
                            self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                        {
                            arenas
                                .iter()
                                .map(|arc| (decl_idx, arc.as_ref()))
                                .collect::<Vec<_>>()
                        } else if has_declaration_arenas {
                            // This symbol has lib declarations (with declaration_arenas
                            // entries) but THIS declaration has no entry — it was added
                            // during user-file binding and lives in the user arena.
                            vec![(decl_idx, self.ctx.arena)]
                        } else {
                            vec![(decl_idx, fallback_arena)]
                        }
                    })
                    .collect();

                // Get type parameters from first declaration that has them,
                // along with the arena they came from (needed for lib interfaces).
                let type_params_with_arena: Option<(tsz_parser::parser::NodeList, &NodeArena)> =
                    decls_with_arenas.iter().find_map(|(decl_idx, arena)| {
                        arena
                            .get(*decl_idx)
                            .and_then(|node| arena.get_interface(node))
                            .and_then(|iface| {
                                iface.type_parameters.clone().map(|tpl| (tpl, *arena))
                            })
                    });
                let type_params_list = type_params_with_arena.as_ref().map(|(tpl, _)| tpl.clone());
                let namespace_prefix = decls_with_arenas.iter().find_map(|(decl_idx, arena)| {
                    let node = arena.get(*decl_idx)?;
                    arena.get_interface(node)?;

                    let mut parent = arena
                        .get_extended(*decl_idx)
                        .map_or(NodeIndex::NONE, |info| info.parent);
                    let mut prefixes = Vec::new();
                    while parent.is_some() {
                        let parent_node = arena.get(parent)?;
                        if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                            && let Some(module) = arena.get_module(parent_node)
                            && let Some(name_node) = arena.get(module.name)
                            && name_node.kind == SyntaxKind::Identifier as u16
                            && let Some(name_ident) = arena.get_identifier(name_node)
                        {
                            prefixes.push(name_ident.escaped_text.clone());
                        }
                        parent = arena
                            .get_extended(parent)
                            .map_or(NodeIndex::NONE, |info| info.parent);
                    }

                    (!prefixes.is_empty())
                        .then(|| prefixes.into_iter().rev().collect::<Vec<_>>().join("."))
                });

                // Pre-compute computed property names for declarations in the
                // current arena. This handles cases like `[FOO_SYMBOL]?: number`
                // inside `declare global { interface Promise<T> { ... } }`, where
                // TypeLowering alone can't resolve the computed expression.
                let computed_names =
                    self.precompute_computed_property_names_in_arenas(&decls_with_arenas);
                let computed_symbol_names = self
                    .precompute_symbol_named_computed_property_names_in_arenas(&decls_with_arenas);

                // Push type params, lower interface, pop type params.
                // push_type_parameters uses self.ctx.arena (user arena) to read
                // type param nodes. For lib interfaces the nodes are in a lib arena,
                // so push_type_parameters may return empty params. In that case,
                // extract params directly from the lib arena.
                let (mut params, updates) = self.push_type_parameters(&type_params_list);
                if params.is_empty() {
                    // For lib/multi-arena interfaces, local push_type_parameters may fail
                    // to read type parameter nodes from self.ctx.arena. Reuse canonical
                    // type-parameter extraction so defaults/constraints are preserved.
                    let canonical_params = self.get_type_params_for_symbol(sym_id);
                    if !canonical_params.is_empty() {
                        params = canonical_params;
                    }
                }

                let type_param_bindings = self.get_type_param_bindings();

                let mut prewarmed_lazy_type_params = rustc_hash::FxHashMap::default();
                for (decl_idx, decl_arena) in &decls_with_arenas {
                    let mut stack = vec![*decl_idx];
                    while let Some(node_idx) = stack.pop() {
                        let Some(node) = decl_arena.get(node_idx) else {
                            continue;
                        };
                        if node.kind == syntax_kind_ext::TYPE_REFERENCE
                            && let Some(type_ref) = decl_arena.get_type_ref(node)
                            && let Some(name_node) = decl_arena.get(type_ref.type_name)
                            && name_node.kind == SyntaxKind::Identifier as u16
                            && let Some(name) = decl_arena.get_identifier_text(type_ref.type_name)
                        {
                            self.prime_lib_type_params(name);
                            if let Some(sym_id) = resolve_name_to_lib_symbol(
                                name,
                                self.ctx.binder,
                                self.ctx.global_file_locals_index.as_deref(),
                                self.ctx
                                    .all_binders
                                    .as_ref()
                                    .map(|binders| binders.as_ref().as_slice()),
                                &self.ctx.lib_contexts,
                            ) {
                                let def_id = self.ctx.get_or_create_def_id(sym_id);
                                if let Some(params) = self.ctx.get_def_type_params(def_id)
                                    && !params.is_empty()
                                {
                                    prewarmed_lazy_type_params.insert(def_id, params);
                                }
                            }
                        }
                        stack.extend(decl_arena.get_children(node_idx));
                    }
                }
                let binder = &self.ctx.binder;
                let lib_binders = self.get_lib_binders();
                // For multi-arena interfaces (e.g. PromiseConstructor declared in
                // lib.es2015.promise.d.ts AND lib.es2015.iterable.d.ts), the resolver
                // must look up identifier text from ALL declaration arenas, not just
                // self.ctx.arena. NodeIndices from different arenas may collide, so
                // using self.ctx.arena alone could resolve to the wrong node.
                let multi_arena_resolve = |node_idx: NodeIndex| -> Option<SymbolId> {
                    // Use checker-accessible compiler-managed type detection helper.

                    // Try each declaration arena to find the identifier text
                    let ident_name = decls_with_arenas
                        .iter()
                        .find_map(|(_, arena)| arena.get_identifier_text(node_idx))
                        .or_else(|| fallback_arena.get_identifier_text(node_idx))?;
                    if is_compiler_managed_type(ident_name) {
                        return None;
                    }
                    let sym_id = resolve_name_to_lib_symbol(
                        ident_name,
                        binder,
                        self.ctx.global_file_locals_index.as_deref(),
                        self.ctx
                            .all_binders
                            .as_ref()
                            .map(|binders| binders.as_ref().as_slice()),
                        &self.ctx.lib_contexts,
                    )?;
                    let symbol = binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                    symbol.has_any_flags(symbol_flags::TYPE).then_some(sym_id)
                };

                let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
                    if needs_text_based_resolution {
                        multi_arena_resolve(node_idx).map(|s| s.0)
                    } else {
                        self.resolve_type_symbol_for_lowering(node_idx)
                    }
                };
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);

                // Stable-identity helper for DefId-based resolution
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
                    if needs_text_based_resolution {
                        decls_with_arenas
                            .iter()
                            .find_map(|(_, arena)| arena.get_identifier_text(node_idx))
                            .or_else(|| fallback_arena.get_identifier_text(node_idx))
                            .and_then(|name| {
                                namespace_prefix
                                    .as_ref()
                                    .and_then(|prefix| {
                                        let mut scoped =
                                            String::with_capacity(prefix.len() + 1 + name.len());
                                        scoped.push_str(prefix);
                                        scoped.push('.');
                                        scoped.push_str(name);
                                        self.resolve_entity_name_text_to_def_id_for_lowering(
                                            &scoped,
                                        )
                                    })
                                    .or_else(|| {
                                        self.resolve_entity_name_text_to_def_id_for_lowering(name)
                                    })
                                    .or_else(|| {
                                        resolve_name_to_lib_symbol(
                                            name,
                                            self.ctx.binder,
                                            self.ctx.global_file_locals_index.as_deref(),
                                            self.ctx
                                                .all_binders
                                                .as_ref()
                                                .map(|binders| binders.as_ref().as_slice()),
                                            &self.ctx.lib_contexts,
                                        )
                                        .map(|sym_id| {
                                            self.ctx.get_canonical_lib_def_id(name, sym_id)
                                        })
                                    })
                            })
                            .or_else(|| {
                                multi_arena_resolve(node_idx)
                                    .map(|sym_id| self.ctx.get_or_create_def_id(sym_id))
                            })
                    } else {
                        self.resolve_def_id_for_lowering(node_idx)
                    }
                };
                let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
                    namespace_prefix
                        .as_ref()
                        .and_then(|prefix| {
                            let mut scoped =
                                String::with_capacity(prefix.len() + 1 + type_name.len());
                            scoped.push_str(prefix);
                            scoped.push('.');
                            scoped.push_str(type_name);
                            self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                        })
                        .or_else(|| {
                            self.resolve_actual_lib_name_to_def_id_for_cross_arena(type_name)
                        })
                        .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
                        .or_else(|| {
                            resolve_name_to_lib_symbol(
                                type_name,
                                self.ctx.binder,
                                self.ctx.global_file_locals_index.as_deref(),
                                self.ctx
                                    .all_binders
                                    .as_ref()
                                    .map(|binders| binders.as_ref().as_slice()),
                                &self.ctx.lib_contexts,
                            )
                            .map(|sym_id| self.ctx.get_canonical_lib_def_id(type_name, sym_id))
                        })
                };

                // Arena-aware resolvers: the key is (NodeIndex, arena_ptr). In
                // lower_merged_interface_declarations_with_symbol, each declaration
                // is lowered with its own NodeArena via TypeLowering::with_arena(),
                // so self.arena at resolver call time IS the correct decl_arena.
                let computed_name_resolver_with_arena =
                    |expr_idx: NodeIndex,
                     arena: *const tsz_parser::parser::node::NodeArena|
                     -> Option<tsz_common::Atom> {
                        computed_names.get(&(expr_idx, arena as usize)).copied()
                    };
                let computed_symbol_name_resolver_with_arena =
                    |expr_idx: NodeIndex,
                     arena: *const tsz_parser::parser::node::NodeArena|
                     -> bool {
                        computed_symbol_names.contains(&(expr_idx, arena as usize))
                    };
                let lazy_type_params_resolver = |def_id: tsz_solver::def::DefId| {
                    prewarmed_lazy_type_params
                        .get(&def_id)
                        .cloned()
                        .or_else(|| self.ctx.get_def_type_params(def_id))
                };
                let lowering = TypeLowering::with_hybrid_resolver(
                    fallback_arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings)
                .with_lazy_type_params_resolver(&lazy_type_params_resolver)
                .with_name_def_id_resolver(&name_resolver)
                .with_computed_name_resolver_with_arena(&computed_name_resolver_with_arena)
                .with_computed_symbol_name_resolver_with_arena(
                    &computed_symbol_name_resolver_with_arena,
                )
                .with_preferred_self_reference(
                    symbol.escaped_name.clone(),
                    self.ctx.get_or_create_def_id(sym_id),
                );
                let lowering = if needs_text_based_resolution {
                    lowering.prefer_name_def_id_resolution()
                } else {
                    lowering
                };

                // Use merged interface lowering for multi-arena declarations
                let has_multi_arenas = has_declaration_arenas;
                let interface_type = if has_multi_arenas {
                    let (ty, _merged_params) = lowering
                        .lower_merged_interface_declarations_with_symbol(
                            &decls_with_arenas,
                            Some(sym_id),
                        );
                    ty
                } else {
                    lowering.lower_interface_declarations_with_symbol(&symbol.declarations, sym_id)
                };
                // First try the standard heritage merge (works for user-arena interfaces).
                let mut merged =
                    self.merge_interface_heritage_types(&symbol.declarations, interface_type);
                // If standard merge didn't propagate lib-arena heritage, fall
                // back to the lib-aware heritage merge. Namespaced lib interfaces
                // (e.g. `Temporal.RoundingOptionsWithLargestUnit`) resolve their
                // own symbol and their base interfaces through the enclosing
                // namespace, so the lib-aware merge needs the qualified name; the
                // bare name fails to resolve the namespaced symbol and drops every
                // inherited member.
                if merged == interface_type {
                    let name = match &namespace_prefix {
                        Some(prefix) => format!("{prefix}.{}", symbol.escaped_name),
                        None => symbol.escaped_name.clone(),
                    };
                    merged = self.merge_lib_interface_heritage(merged, &name).0;
                }
                self.pop_type_parameters(updates);
                if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                    let canonical_params = self.get_type_params_for_symbol(sym_id);
                    if !canonical_params.is_empty() {
                        self.ctx.insert_def_type_params(def_id, canonical_params);
                    } else {
                        self.ctx.insert_def_type_params(def_id, params.clone());
                    }
                }
                return (merged, params);
            }

            if symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
                if self
                    .ctx
                    .resolve_symbol_file_index(sym_id)
                    .is_some_and(|file_idx| {
                        file_idx != self.ctx.current_file_idx
                            && self.should_delegate_dynamic_type_alias_owner(sym_id, file_idx)
                    })
                    && let Some((alias_type, params)) =
                        self.delegate_cross_arena_symbol_resolution(sym_id)
                    && alias_type != TypeId::UNKNOWN
                    && alias_type != TypeId::ERROR
                {
                    return (alias_type, params);
                }

                // When a type alias name collides with a global value declaration
                // (e.g., user-defined `type Proxy<T>` vs global `declare var Proxy`),
                // the merged symbol's value_declaration points to the var decl, not the
                // type alias. We must search declarations[] to find the actual type alias.
                let decl_idx = symbol
                    .declarations
                    .iter()
                    .copied()
                    .find(|&d| {
                        self.ctx
                            .arena
                            .get(d)
                            .and_then(|n| {
                                if n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                                    // Verify name matches to prevent NodeIndex collisions
                                    let type_alias = self.ctx.arena.get_type_alias(n)?;
                                    let name =
                                        self.ctx.arena.get_identifier_text(type_alias.name)?;
                                    Some(name == symbol.escaped_name.as_str())
                                } else {
                                    Some(false)
                                }
                            })
                            .unwrap_or(false)
                    })
                    .unwrap_or_else(|| symbol.primary_declaration().unwrap_or(NodeIndex::NONE));

                if decl_idx.is_some() {
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && let Some(type_alias) = self.ctx.arena.get_type_alias(node)
                        && self
                            .ctx
                            .arena
                            .get_identifier_text(type_alias.name)
                            .is_some_and(|n| n == symbol.escaped_name.as_str())
                    {
                        let (params, updates) =
                            self.push_type_parameters(&type_alias.type_parameters);
                        self.prime_type_reference_params_in_alias_body(
                            self.ctx.arena,
                            type_alias.type_node,
                        );
                        let alias_type = self.get_type_from_type_node(type_alias.type_node);
                        self.pop_type_parameters(updates);
                        if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                            self.ctx.register_def_auto_params_in_envs(
                                def_id,
                                alias_type,
                                params.clone(),
                            );
                        }
                        return (alias_type, params);
                    }

                    let lib_arena = self
                        .ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        .map(std::convert::AsRef::as_ref)
                        .or_else(|| {
                            self.ctx
                                .binder
                                .symbol_arenas
                                .get(&sym_id)
                                .map(std::convert::AsRef::as_ref)
                        });

                    if let Some(lib_arena) = lib_arena
                        && let Some(node) = lib_arena.get(decl_idx)
                        && let Some(type_alias) = lib_arena.get_type_alias(node)
                    {
                        self.prime_type_reference_params_in_alias_body(
                            lib_arena,
                            type_alias.type_node,
                        );
                        let type_param_bindings = self.get_type_param_bindings();
                        let binder = &self.ctx.binder;
                        let lib_binders = self.get_lib_binders();
                        let namespace_prefix = {
                            let mut parent = lib_arena
                                .get_extended(decl_idx)
                                .map_or(NodeIndex::NONE, |info| info.parent);
                            let mut prefixes = Vec::new();
                            while parent.is_some() {
                                let Some(parent_node) = lib_arena.get(parent) else {
                                    break;
                                };
                                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                                    && let Some(module) = lib_arena.get_module(parent_node)
                                    && let Some(name_node) = lib_arena.get(module.name)
                                    && name_node.kind == SyntaxKind::Identifier as u16
                                    && let Some(name_ident) = lib_arena.get_identifier(name_node)
                                {
                                    prefixes.push(name_ident.escaped_text.clone());
                                }
                                parent = lib_arena
                                    .get_extended(parent)
                                    .map_or(NodeIndex::NONE, |info| info.parent);
                            }
                            (!prefixes.is_empty())
                                .then(|| prefixes.into_iter().rev().collect::<Vec<_>>().join("."))
                        };
                        let resolve_type_name = |name: &str| -> Option<SymbolId> {
                            namespace_prefix
                                .as_ref()
                                .and_then(|prefix| {
                                    let mut scoped =
                                        String::with_capacity(prefix.len() + 1 + name.len());
                                    scoped.push_str(prefix);
                                    scoped.push('.');
                                    scoped.push_str(name);
                                    self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                                        .and_then(|def_id| {
                                            self.ctx.def_to_symbol_id_with_fallback(def_id)
                                        })
                                })
                                .or_else(|| {
                                    self.resolve_entity_name_text_to_def_id_for_lowering(name)
                                        .and_then(|def_id| {
                                            self.ctx.def_to_symbol_id_with_fallback(def_id)
                                        })
                                })
                        };

                        let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
                            let type_name = entity_name_text_in_arena(lib_arena, node_idx)?;
                            if is_compiler_managed_type(&type_name) {
                                return None;
                            }
                            let sym_id = resolve_type_name(&type_name)?;
                            let symbol = binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                            symbol.has_any_flags(symbol_flags::TYPE).then_some(sym_id.0)
                        };
                        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
                            self.resolve_value_symbol_for_lowering(node_idx)
                        };
                        let def_id_resolver =
                            |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
                                let type_name = entity_name_text_in_arena(lib_arena, node_idx)?;
                                if is_compiler_managed_type(&type_name) {
                                    return None;
                                }
                                let sym_id = resolve_type_name(&type_name)?;
                                let symbol = binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                                symbol
                                    .has_any_flags(symbol_flags::TYPE)
                                    .then(|| self.ctx.get_or_create_def_id(sym_id))
                            };
                        let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
                            namespace_prefix
                                .as_ref()
                                .and_then(|prefix| {
                                    let mut scoped =
                                        String::with_capacity(prefix.len() + 1 + type_name.len());
                                    scoped.push_str(prefix);
                                    scoped.push('.');
                                    scoped.push_str(type_name);
                                    self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                                })
                                .or_else(|| {
                                    self.resolve_actual_lib_name_to_def_id_for_cross_arena(
                                        type_name,
                                    )
                                })
                                .or_else(|| {
                                    self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
                                })
                        };

                        let lazy_type_params_resolver =
                            |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);
                        let lowering = TypeLowering::with_hybrid_resolver(
                            lib_arena,
                            self.ctx.types,
                            &type_resolver,
                            &def_id_resolver,
                            &value_resolver,
                        )
                        .with_type_param_bindings(type_param_bindings)
                        .with_lazy_type_params_resolver(&lazy_type_params_resolver)
                        .with_name_def_id_resolver(&name_resolver);
                        let (alias_type, params) =
                            lowering.lower_type_alias_declaration(type_alias);
                        if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
                            self.ctx.register_def_auto_params_in_envs(
                                def_id,
                                alias_type,
                                params.clone(),
                            );
                        }
                        return (alias_type, params);
                    }
                }
            }
        }

        // Fallback: get type of symbol and params separately
        let body_type = self.get_type_of_symbol(sym_id);
        let type_params = self.get_type_params_for_symbol(sym_id);
        (body_type, type_params)
    }
}
