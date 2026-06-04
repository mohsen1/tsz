impl<'a> CheckerState<'a> {
    /// Resolve multiple members from the same remote interface with one child checker.
    ///
    /// Interface compatibility and module augmentation checks often walk every
    /// property/method in a remote declaration. Batching keeps the same target
    /// arena/binder semantics as the single-member path without constructing a
    /// child checker per member.
    pub(crate) fn delegate_cross_arena_interface_member_simple_types(
        &mut self,
        interface_idx: NodeIndex,
        member_indices: &[NodeIndex],
        interface_arena: &tsz_parser::NodeArena,
        type_args: Option<&[TypeId]>,
    ) -> Option<rustc_hash::FxHashMap<NodeIndex, TypeId>> {
        if std::ptr::eq(interface_arena, self.ctx.arena) {
            return None;
        }
        if member_indices.is_empty() {
            return Some(rustc_hash::FxHashMap::default());
        }

        // O(1) via global_arena_index (replaces O(N) position scan)
        let delegate_file_idx = self.ctx.get_file_idx_for_arena(interface_arena);
        let delegate_binder_arc = delegate_file_idx
            .and_then(|file_idx| self.ctx.all_binders.as_ref()?.get(file_idx).cloned());
        let delegate_binder = delegate_binder_arc.as_deref().unwrap_or(self.ctx.binder);

        let mut results = rustc_hash::FxHashMap::default();
        let mut misses = Vec::new();
        if type_args.is_none()
            && let Some(file_idx) = delegate_file_idx
        {
            for &member_idx in member_indices {
                if let Some(cached_type) = self.ctx.cached_cross_file_interface_member_simple_type(
                    interface_idx,
                    member_idx,
                    file_idx as u32,
                ) {
                    tsz_common::perf_counters::record_delegate_cross_arena_cache_hit_cross_file();
                    results.insert(member_idx, cached_type);
                } else {
                    misses.push(member_idx);
                }
            }
        } else {
            misses.extend_from_slice(member_indices);
        }

        if misses.is_empty() {
            return Some(results);
        }

        if let Some(direct_results) = self.direct_cross_file_interface_member_simple_types(
            interface_idx,
            &misses,
            interface_arena,
            delegate_binder,
            type_args,
            false,
        ) {
            if type_args.is_none()
                && let Some(file_idx) = delegate_file_idx
            {
                for (&member_idx, &member_type) in direct_results.iter() {
                    self.ctx.cache_cross_file_interface_member_simple_type(
                        interface_idx,
                        member_idx,
                        file_idx as u32,
                        member_type,
                    );
                }
            }
            results.extend(direct_results);
            misses.retain(|member_idx| !results.contains_key(member_idx));
            if misses.is_empty() {
                return Some(results);
            }
        }

        if !Self::enter_cross_arena_delegation() {
            return if results.is_empty() {
                None
            } else {
                Some(results)
            };
        }
        if !self.ctx.enter_recursion() {
            Self::leave_cross_arena_delegation();
            return if results.is_empty() {
                None
            } else {
                Some(results)
            };
        }

        let delegate_file_name = interface_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());

        // PERF: see the matching block in `delegate_cross_arena_class_instance_type`.
        // Cache check above returned None → about to do real work.
        tsz_common::perf_counters::record_delegate_cross_arena_miss();
        let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();

        let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
            interface_arena,
            delegate_binder,
            self.ctx.types,
            delegate_file_name,
            self.ctx.compiler_options.clone(),
            self,
            tsz_common::perf_counters::CheckerCreationReason::DelegateCrossArenaOther,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        self.ctx.copy_symbol_file_targets_to_attributed(
            &mut checker.ctx,
            tsz_common::perf_counters::CheckerCreationReason::DelegateCrossArenaOther,
        );
        checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);
        let parent_is_declaration_file = self.ctx.file_name.ends_with(".d.ts")
            || self.ctx.file_name.ends_with(".d.cts")
            || self.ctx.file_name.ends_with(".d.mts");
        let delegate_is_declaration_file = interface_arena
            .source_files
            .first()
            .is_some_and(|source_file| source_file.is_declaration_file);
        if parent_is_declaration_file && !delegate_is_declaration_file {
            checker
                .ctx
                .type_resolution_fuel
                .set(crate::state::MAX_TYPE_RESOLUTION_OPS);
            crate::state_domain::type_environment::lazy::reset_global_resolution_fuel();
        }
        // DefId ↔ SymbolId mappings are resolved via DefinitionStore fallback
        // on cache miss — no parent-to-child copy needed.

        let interface_type_params = checker
            .ctx
            .arena
            .get(interface_idx)
            .and_then(|node| checker.ctx.arena.get_interface(node))
            .and_then(|iface| iface.type_parameters.clone());
        let (interface_params, interface_updates) = interface_type_params
            .as_ref()
            .map(|type_parameters| checker.push_type_parameters(&Some(type_parameters.clone())))
            .unwrap_or_default();

        let substitution = type_args
            .filter(|type_args| {
                !interface_params.is_empty() && type_args.len() <= interface_params.len()
            })
            .and_then(|type_args| {
                crate::query_boundaries::type_defaults::fill_application_defaults(
                    checker.ctx.types,
                    type_args,
                    &interface_params,
                )
            })
            .map(|type_args| {
                crate::query_boundaries::common::TypeSubstitution::from_args(
                    checker.ctx.types,
                    &interface_params,
                    &type_args,
                )
            });

        for member_idx in misses {
            let mut result = checker.get_type_of_interface_member_simple(member_idx);
            if let Some(substitution) = substitution.as_ref() {
                result = crate::query_boundaries::common::instantiate_type(
                    checker.ctx.types,
                    result,
                    substitution,
                );
            }
            if result != TypeId::UNKNOWN && result != TypeId::ERROR {
                if type_args.is_none()
                    && let Some(file_idx) = delegate_file_idx
                {
                    self.ctx.cache_cross_file_interface_member_simple_type(
                        interface_idx,
                        member_idx,
                        file_idx as u32,
                        result,
                    );
                }
                results.insert(member_idx, result);
            }
        }
        checker.pop_type_parameters(interface_updates);

        self.ctx.leave_recursion();
        Self::leave_cross_arena_delegation();

        Some(results)
    }

    /// Detect and record cross-file `SymbolIds`.
    ///
    /// In multi-file mode, the driver copies target file's `module_exports` into
    /// the local binder, so `SymbolIds` may be from another file's binder. We
    /// detect this by checking if the `SymbolId` maps to a symbol with the expected
    /// name in the current binder. If not, we search `all_binders` to find the
    /// correct source file.
    pub(crate) fn record_cross_file_symbol_if_needed(
        &self,
        sym_id: SymbolId,
        expected_name: &str,
        module_name: &str,
    ) {
        // Skip if already recorded
        if self.ctx.has_symbol_file_index(sym_id) {
            return;
        }

        // Try resolve_import_target first (most reliable). This avoids SymbolId
        // collision issues: after lib_symbols_merged, different files' binders share
        // the same base_offset, so binder.get_symbol(sym_id) can return the WRONG
        // symbol from the current file that happens to share the same index offset.
        if let Some(target_file_idx) = self.ctx.resolve_import_target(module_name) {
            if target_file_idx != self.ctx.current_file_idx {
                self.ctx
                    .register_symbol_file_target(sym_id, target_file_idx);
            }
            return;
        }

        // resolve_import_target didn't work (the module specifier may be relative
        // to a different file). Fall back to the binder locality check.
        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.escaped_name.as_str() == expected_name
        {
            return;
        }

        // Fast-path: use global_file_locals_index for O(1) name→binder lookup.
        // Only covers top-level file_locals symbols; nested symbols (class members,
        // namespace exports) fall through to the O(N) scan below.
        if let Some(entries) = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(expected_name))
            && let Some(binders) = &self.ctx.all_binders
        {
            for &(file_idx, _) in entries {
                if let Some(binder) = binders.get(file_idx)
                    && let Some(symbol) = binder.get_symbol(sym_id)
                    && symbol.escaped_name.as_str() == expected_name
                {
                    self.ctx.register_symbol_file_target(sym_id, file_idx);
                    return;
                }
            }
        }
        // Full fallback: the symbol may be nested (not in file_locals).
        if let Some(binders) = &self.ctx.all_binders {
            for (idx, binder) in binders.iter().enumerate() {
                if let Some(symbol) = binder.get_symbol(sym_id)
                    && symbol.escaped_name.as_str() == expected_name
                {
                    self.ctx.register_symbol_file_target(sym_id, idx);
                    return;
                }
            }
            // For ambient module `export =` entries, the exports table key is
            // "export=" but the actual symbol has a different escaped_name (e.g.,
            // "passport"). Fall back to matching by SymbolId alone when the name
            // didn't match — this is safe because SymbolId uniquely identifies the
            // symbol within its owning binder.
            if expected_name == "export=" {
                for (idx, binder) in binders.iter().enumerate() {
                    if binder.get_symbol(sym_id).is_some() {
                        self.ctx.register_symbol_file_target(sym_id, idx);
                        return;
                    }
                }
            }
        }
    }

    /// Lower a single interface declaration from a cross-file arena.
    ///
    /// When an interface is declared across multiple files (e.g., global script
    /// interface merging), each cross-file declaration lives in a different
    /// `NodeArena`. This method creates a `TypeLowering` bound to the source arena
    /// and uses name-based resolution via `file_locals` to resolve type references.
    pub(crate) fn lower_cross_file_interface_decl(
        &self,
        arena: &std::sync::Arc<tsz_parser::parser::node::NodeArena>,
        decl_idx: NodeIndex,
        sym_id: SymbolId,
    ) -> TypeId {
        use tsz_lowering::TypeLowering;
        use tsz_solver::is_compiler_managed_type;

        let arena_ref: &tsz_parser::parser::node::NodeArena = arena.as_ref();
        let lib_binders = self.get_lib_binders();

        // Cross-file type resolver: reads identifier text from the cross-file
        // arena, then resolves by name in the current binder's file_locals
        // (which includes merged global symbols from all files).
        let cross_type_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let node = arena_ref.get(node_idx)?;
            let ident = arena_ref.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if is_compiler_managed_type(name) {
                return None;
            }
            let sym = self.ctx.binder.file_locals.get(name)?;
            let symbol = self.ctx.binder.get_symbol_with_libs(sym, &lib_binders)?;
            if symbol.has_any_flags(symbol_flags::TYPE) {
                return Some(sym.0);
            }
            None
        };

        let cross_def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
            let node = arena_ref.get(node_idx)?;
            let ident = arena_ref.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if is_compiler_managed_type(name) {
                return None;
            }
            let sym = self.ctx.binder.file_locals.get(name)?;
            let symbol = self.ctx.binder.get_symbol_with_libs(sym, &lib_binders)?;
            if symbol.has_any_flags(symbol_flags::TYPE) {
                Some(self.ctx.get_or_create_def_id(sym))
            } else {
                None
            }
        };

        let cross_value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let node = arena_ref.get(node_idx)?;
            let ident = arena_ref.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            let sym = self.ctx.binder.file_locals.get(name)?;
            let symbol = self.ctx.binder.get_symbol_with_libs(sym, &lib_binders)?;
            if (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0 {
                Some(sym.0)
            } else {
                None
            }
        };

        let type_param_bindings = self.get_type_param_bindings();
        let lowering = TypeLowering::with_hybrid_resolver(
            arena_ref,
            self.ctx.types,
            &cross_type_resolver,
            &cross_def_id_resolver,
            &cross_value_resolver,
        )
        .with_type_param_bindings(type_param_bindings);

        lowering.lower_interface_declarations_with_symbol(&[decl_idx], sym_id)
    }

    /// Merge heritage types from cross-file interface declarations.
    ///
    /// `merge_interface_heritage_types` uses `self.ctx.arena` to read heritage
    /// clauses, so it silently skips cross-file declarations. This method handles
    /// those skipped declarations by reading from the source arena and resolving
    /// base types via `file_locals` name lookup.
    pub(crate) fn merge_cross_file_heritage(
        &mut self,
        declarations: &[NodeIndex],
        sym_id: SymbolId,
        mut derived_type: TypeId,
    ) -> TypeId {
        use tsz_scanner::SyntaxKind;

        for &decl_idx in declarations {
            let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) else {
                continue;
            };
            for arena in arenas.iter() {
                // Skip the local arena (already processed by merge_interface_heritage_types)
                if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                    continue;
                }
                let Some(node) = arena.get(decl_idx) else {
                    continue;
                };
                let Some(interface) = arena.get_interface(node) else {
                    continue;
                };
                let Some(ref heritage_clauses) = interface.heritage_clauses else {
                    continue;
                };

                for &clause_idx in &heritage_clauses.nodes {
                    let Some(clause_node) = arena.get(clause_idx) else {
                        continue;
                    };
                    let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                        continue;
                    };
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }

                    for &type_idx in &heritage.types.nodes {
                        let Some(type_node) = arena.get(type_idx) else {
                            continue;
                        };

                        let (expr_idx, type_arguments) =
                            if let Some(expr) = arena.get_expr_type_args(type_node) {
                                (expr.expression, expr.type_arguments.as_ref())
                            } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                                if let Some(type_ref) = arena.get_type_ref(type_node) {
                                    (type_ref.type_name, type_ref.type_arguments.as_ref())
                                } else {
                                    (type_idx, None)
                                }
                            } else {
                                (type_idx, None)
                            };

                        let Some(name) = expression_name_text_in_arena(arena, expr_idx) else {
                            continue;
                        };
                        let Some(base_sym_id) = self.resolve_cross_file_global_type_symbol(&name)
                        else {
                            continue;
                        };

                        let mut base_type = self.get_type_of_symbol(base_sym_id);
                        if base_type == TypeId::ERROR || base_type == TypeId::UNKNOWN {
                            continue;
                        }
                        if let Some(type_arguments) = type_arguments {
                            let base_params = self.get_type_params_for_symbol(base_sym_id);
                            if !base_params.is_empty() {
                                let mut type_args = Vec::with_capacity(type_arguments.nodes.len());
                                for &arg_idx in &type_arguments.nodes {
                                    type_args.push(
                                        self.resolve_cross_file_heritage_type_arg(arena, arg_idx),
                                    );
                                }
                                while type_args.len() < base_params.len() {
                                    let param = &base_params[type_args.len()];
                                    type_args.push(
                                        param
                                            .default
                                            .or(param.constraint)
                                            .unwrap_or(TypeId::UNKNOWN),
                                    );
                                }
                                if type_args.len() > base_params.len() {
                                    type_args.truncate(base_params.len());
                                }
                                let substitution =
                                    crate::query_boundaries::common::TypeSubstitution::from_args(
                                        self.ctx.types,
                                        &base_params,
                                        &type_args,
                                    );
                                base_type = crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    base_type,
                                    &substitution,
                                );
                            }
                        }

                        derived_type = self.merge_interface_types(derived_type, base_type);
                    }
                }
            }
        }

        derived_type
    }
}
