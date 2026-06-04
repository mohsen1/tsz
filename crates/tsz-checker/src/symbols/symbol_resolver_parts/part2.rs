impl<'a> CheckerState<'a> {
    /// Resolve a simple or qualified type name through the merged checker binder.
    ///
    /// Cross-arena lowering cannot trust raw `NodeIndex` values because the same
    /// index may refer to unrelated nodes in different declaration arenas. This
    /// helper uses the text form (`A` or `A.B.C`) and walks the merged binder's
    /// export graph to recover the correct `DefId`.
    pub(crate) fn resolve_entity_name_text_to_def_id_for_lowering(
        &self,
        name: &str,
    ) -> Option<tsz_solver::def::DefId> {
        if !name.contains('.') && self.ctx.type_parameter_scope.contains_key(name) {
            return None;
        }

        if is_compiler_managed_type(name) {
            return None;
        }

        if let Some(cached) = self
            .ctx
            .lowering_entity_name_resolution_cache
            .borrow()
            .get(name)
            .copied()
        {
            // A miss recorded before lib contexts were attached is not stable
            // for child/cross-arena checkers. Retry once libs are available so
            // imported declaration files can resolve globals like `Error`.
            //
            // Likewise, a `None` cached for a qualified name like
            // `util.OmitKeys` may have been recorded by an earlier checker
            // state whose binder couldn't see the imported namespace
            // member yet. Retry such misses so a later checker state with
            // the merged binder graph can recover the correct `DefId`.
            // Without this retry, the first failed lookup poisons the cache
            // and silently strands the alias body's downstream consumers
            // (object spread, intersection reduction) on
            // `UnresolvedTypeName`.
            let retry_dotted_miss = cached.is_none() && name.contains('.');
            if cached.is_some() || (!self.ctx.has_lib_loaded() && !retry_dotted_miss) {
                return cached;
            }
        }

        let mut segments = name.split('.');
        let root_name = segments.next()?;
        let lib_binders = self.get_lib_binders();
        let mut current_sym = self
            .ctx
            .binder
            .file_locals
            .get(root_name)
            .or_else(|| {
                self.ctx
                    .binder
                    .get_global_type_with_libs(root_name, &lib_binders)
            })
            .or_else(|| {
                self.ctx
                    .global_file_locals_index
                    .as_ref()
                    .and_then(|idx| idx.get(root_name))
                    .and_then(|entries| entries.iter().max_by_key(|(_, sym)| sym.0))
                    .map(|&(_, sym)| sym)
            })
            .or_else(|| {
                lib_binders
                    .iter()
                    .find_map(|binder| binder.file_locals.get(root_name))
            })
            .or_else(|| self.resolve_global_augmentation_root_symbol(root_name, &lib_binders))?;

        for segment in segments {
            let mut visited_aliases = AliasCycleTracker::new();
            current_sym = self
                .resolve_alias_symbol(current_sym, &mut visited_aliases)
                .unwrap_or(current_sym);

            let Some(symbol) = self.get_cross_file_symbol(current_sym).or_else(|| {
                self.ctx
                    .binder
                    .get_symbol_with_libs(current_sym, &lib_binders)
            }) else {
                self.ctx
                    .lowering_entity_name_resolution_cache
                    .borrow_mut()
                    .insert(name.to_string(), None);
                return None;
            };

            if let Some(member_sym) = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(segment))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(segment))
                })
            {
                current_sym = member_sym;
                continue;
            }

            if let Some(ref module_specifier) = symbol.import_module {
                let mut visited_aliases = AliasCycleTracker::new();
                if let Some(member_sym) = self.resolve_reexported_member_symbol(
                    module_specifier,
                    segment,
                    &mut visited_aliases,
                ) {
                    current_sym = member_sym;
                    continue;
                }
            }

            if symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0
                && let Some(member_sym) = self.resolve_namespace_member_from_all_binders(
                    symbol.escaped_name.as_str(),
                    segment,
                )
            {
                current_sym = member_sym;
                continue;
            }

            self.ctx
                .lowering_entity_name_resolution_cache
                .borrow_mut()
                .insert(name.to_string(), None);
            return None;
        }

        let mut visited_aliases = AliasCycleTracker::new();
        let resolved_sym = self
            .resolve_alias_symbol(current_sym, &mut visited_aliases)
            .unwrap_or(current_sym);
        let canonical_name = name.rsplit('.').next().unwrap_or(name);
        let expected_name = self
            .get_cross_file_symbol(resolved_sym)
            .or_else(|| {
                self.ctx
                    .binder
                    .get_symbol_with_libs(resolved_sym, &lib_binders)
            })
            .map_or(canonical_name, |symbol| symbol.escaped_name.as_str());
        let def_id = if self.ctx.has_lib_loaded()
            && self.ctx.symbol_is_from_actual_or_cloned_lib(resolved_sym)
        {
            self.ctx
                .get_canonical_lib_def_id(expected_name, resolved_sym)
        } else {
            self.ctx
                .get_or_create_def_id_for_symbol_name(resolved_sym, expected_name)
        };
        self.ctx
            .lowering_entity_name_resolution_cache
            .borrow_mut()
            .insert(name.to_string(), Some(def_id));
        Some(def_id)
    }

    fn resolve_global_augmentation_root_symbol(
        &self,
        name: &str,
        lib_binders: &[std::sync::Arc<tsz_binder::BinderState>],
    ) -> Option<tsz_binder::SymbolId> {
        let from_binder = |binder: &tsz_binder::BinderState,
                           file_idx: Option<usize>|
         -> Option<tsz_binder::SymbolId> {
            let augmentations = binder.global_augmentations.get(name)?;
            for augmentation in augmentations {
                if let Some(sym_id) = binder.node_symbols.get(&augmentation.node.0).copied() {
                    if let Some(file_idx) = file_idx {
                        self.ctx.register_symbol_file_target(sym_id, file_idx);
                    }
                    return Some(sym_id);
                }
            }
            None
        };

        if let Some(sym_id) = from_binder(
            self.ctx.binder,
            (self.ctx.current_file_idx != usize::MAX).then_some(self.ctx.current_file_idx),
        ) {
            return Some(sym_id);
        }

        if let Some(all_binders) = self.ctx.all_binders.as_ref() {
            for (file_idx, binder) in all_binders.iter().enumerate() {
                if let Some(sym_id) = from_binder(binder, Some(file_idx)) {
                    return Some(sym_id);
                }
            }
        }

        for binder in lib_binders {
            if let Some(sym_id) = from_binder(binder, None) {
                return Some(sym_id);
            }
        }

        None
    }

    /// Resolve a type symbol for type lowering.
    ///
    /// Returns the symbol ID if the resolved symbol has the TYPE flag set.
    /// Returns None for built-in types that have special handling in `TypeLowering`.
    pub(crate) fn resolve_type_symbol_for_lowering(&self, idx: NodeIndex) -> Option<u32> {
        // Skip built-in types that have special handling in TypeLowering
        // These types use built-in TypeData representations instead of Refs
        if let Some(node) = self.ctx.arena.get(idx)
            && let Some(ident) = self.ctx.arena.get_identifier(node)
        {
            if is_compiler_managed_type(ident.escaped_text.as_str()) {
                let shadows_compiler_managed_type =
                    matches!(ident.escaped_text.as_str(), "Array" | "ReadonlyArray")
                        && self
                            .ctx
                            .file_local_type_shadow_for_lib_name(ident.escaped_text.as_str());
                if !shadows_compiler_managed_type {
                    return None;
                }
            }
            if node.kind == SyntaxKind::Identifier as u16
                && let TypeSymbolResolution::Type(sym_id) =
                    self.resolve_identifier_symbol_in_type_position(idx)
            {
                let lib_binders = self.get_lib_binders();
                if let Some(alias_symbol) =
                    self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                    && alias_symbol.has_any_flags(symbol_flags::ALIAS)
                    && alias_symbol.is_type_only
                    && let Some(module_name) = alias_symbol.import_module.as_ref()
                    && let Some(import_name) = alias_symbol.import_name.as_deref()
                {
                    let source_file_idx = self
                        .ctx
                        .resolve_symbol_file_index(sym_id)
                        .unwrap_or(self.ctx.current_file_idx);
                    if let Some(target_sym_id) = self.resolve_cross_file_export_from_file(
                        module_name,
                        import_name,
                        Some(source_file_idx),
                    ) {
                        let target_has_type = self
                            .get_cross_file_symbol(target_sym_id)
                            .or_else(|| {
                                self.ctx
                                    .binder
                                    .get_symbol_with_libs(target_sym_id, &lib_binders)
                            })
                            .is_some_and(|target_symbol| {
                                target_symbol.has_any_flags(symbol_flags::TYPE)
                            });
                        if target_has_type {
                            return Some(target_sym_id.0);
                        }
                    }
                }
                if let Some(symbol) = self
                    .get_cross_file_symbol(sym_id)
                    .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))
                {
                    if symbol.escaped_name != ident.escaped_text {
                        return self
                            .resolve_entity_name_text_to_def_id_for_lowering(
                                ident.escaped_text.as_str(),
                            )
                            .and_then(|def_id| {
                                self.ctx
                                    .def_symbol_identity(def_id)
                                    .map(|(sym_id, _)| sym_id.0)
                            });
                    }
                    if symbol.has_any_flags(symbol_flags::ALIAS) {
                        let mut visited_aliases = AliasCycleTracker::new();
                        if let Some(target_sym_id) =
                            self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                            && target_sym_id != sym_id
                            && self
                                .get_cross_file_symbol(target_sym_id)
                                .or_else(|| {
                                    self.ctx
                                        .binder
                                        .get_symbol_with_libs(target_sym_id, &lib_binders)
                                })
                                .is_some_and(|target_symbol| {
                                    target_symbol.has_any_flags(symbol_flags::TYPE)
                                })
                        {
                            return Some(target_sym_id.0);
                        }
                    }
                    if symbol.has_any_flags(symbol_flags::TYPE) {
                        return Some(sym_id.0);
                    }
                }
            }
        }

        let mut sym_id = match self.resolve_qualified_symbol_in_type_position(idx) {
            TypeSymbolResolution::Type(sym_id) => sym_id,
            _ => return None,
        };
        // Use get_cross_file_symbol to avoid SymbolId collisions across binders.
        // When resolving qualified names like `server.IWorkspace`, the SymbolId
        // belongs to server.ts's binder, not the current file's binder. Without
        // this, we'd look up the SymbolId in the wrong binder and potentially
        // get a different symbol with a colliding ID.
        let lib_binders = self.get_lib_binders();
        let mut symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))?;
        if symbol.has_any_flags(symbol_flags::ALIAS) {
            let mut visited_aliases = AliasCycleTracker::new();
            if let Some(target_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                && target_sym_id != sym_id
            {
                sym_id = target_sym_id;
                symbol = self
                    .get_cross_file_symbol(sym_id)
                    .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))?;
            }
        }
        symbol.has_any_flags(symbol_flags::TYPE).then_some(sym_id.0)
    }

    /// Resolve a value symbol for type lowering.
    ///
    /// Returns the symbol ID if the resolved symbol has VALUE or ALIAS flags set.
    pub(crate) fn resolve_value_symbol_for_lowering(&self, idx: NodeIndex) -> Option<u32> {
        if let Some(node) = self.ctx.arena.get(idx) {
            if node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.resolve_identifier_symbol(idx)
                && self.alias_resolves_to_type_only(sym_id)
            {
                return None;
            }
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let mut current = idx;
                while let Some(node) = self.ctx.arena.get(current) {
                    if node.kind == SyntaxKind::Identifier as u16 {
                        if let Some(sym_id) = self.resolve_identifier_symbol(current)
                            && self.alias_resolves_to_type_only(sym_id)
                        {
                            return None;
                        }
                        break;
                    }
                    if node.kind != syntax_kind_ext::QUALIFIED_NAME {
                        break;
                    }
                    let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                        break;
                    };
                    current = qn.left;
                }
            }
        }
        let sym_id = self.resolve_qualified_symbol(idx)?;
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        if symbol.is_type_only {
            return None;
        }
        if (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0 {
            return Some(sym_id.0);
        }

        // The initial resolution found a TYPE-only symbol (e.g., `interface Promise<T>`
        // from one lib file). But the VALUE declaration (`declare var Promise`) may
        // exist in a different lib file. Search all lib binders by name for a symbol
        // that has the VALUE flag. This handles declaration merging across lib files.
        let name = self
            .ctx
            .arena
            .get(idx)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|i| i.escaped_text.as_str());
        if let Some(name) = name {
            // Check file_locals first (may have merged value from lib)
            if let Some(val_sym_id) = self.ctx.binder.file_locals.get(name)
                && let Some(val_symbol) = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(val_sym_id, &lib_binders)
                && (val_symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0
                && !val_symbol.is_type_only
            {
                return Some(val_sym_id.0);
            }
            // Search lib binders directly for a value declaration
            for lib_binder in lib_binders.iter() {
                if let Some(val_sym_id) = lib_binder.file_locals.get(name)
                    && let Some(val_symbol) = lib_binder.get_symbol(val_sym_id)
                    && (val_symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0
                    && !val_symbol.is_type_only
                {
                    return Some(val_sym_id.0);
                }
            }
        }

        None
    }

    /// Resolve a `DefId` from a node index for type lowering.
    ///
    /// This is the canonical stable-identity helper for `def_id_resolver` closures.
    /// It encapsulates the common pattern:
    ///   `resolve_type_symbol_for_lowering(node_idx) → SymbolId → get_or_create_def_id`
    ///
    /// Use this instead of inlining the SymbolId wrapping + DefId creation at each
    /// lowering call site.
    pub(crate) fn resolve_def_id_for_lowering(
        &self,
        node_idx: NodeIndex,
    ) -> Option<tsz_solver::def::DefId> {
        self.resolve_type_symbol_for_lowering(node_idx)
            .map(|sym_id| {
                let sym_id = tsz_binder::SymbolId(sym_id);
                if let Some(node) = self.ctx.arena.get(node_idx)
                    && let Some(ident) = self.ctx.arena.get_identifier(node)
                {
                    // A same-arena NodeIndex may resolve to a namespace-local type
                    // whose bare name collides with a lib global (`Promise`, etc.).
                    // Only canonicalize to the lib DefId when the resolved symbol
                    // itself is from a lib context.
                    if !self
                        .ctx
                        .file_local_type_shadow_for_lib_name(&ident.escaped_text)
                        && (self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                            || self.ctx.symbol_is_from_lib(sym_id))
                        && let Some(def_id) =
                            self.resolve_actual_lib_name_to_def_id_for_lowering(&ident.escaped_text)
                    {
                        return def_id;
                    }
                    let expected_name = if let Some(symbol) = self.get_cross_file_symbol(sym_id) {
                        symbol.escaped_name.clone()
                    } else {
                        let lib_binders = self.get_lib_binders();
                        self.ctx
                            .binder
                            .get_symbol_with_libs(sym_id, &lib_binders)
                            .map_or_else(
                                || ident.escaped_text.clone(),
                                |symbol| symbol.escaped_name.clone(),
                            )
                    };
                    return self
                        .ctx
                        .get_or_create_def_id_for_symbol_name(sym_id, expected_name.as_str());
                }
                self.ctx.get_or_create_def_id(sym_id)
            })
    }
}
