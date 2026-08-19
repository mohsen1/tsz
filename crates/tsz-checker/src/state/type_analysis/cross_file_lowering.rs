//! Cross-file interface lowering and heritage merge, plus cross-file symbol
//! recording. Split from `cross_file.rs` to respect the 2000-line file cap;
//! child module of the same `CheckerState` impl surface.
use crate::state::CheckerState;
use crate::symbols_domain::name_text::expression_name_text_in_arena;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl CheckerState<'_> {
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

    /// Merge an interface symbol's cross-file declaration groups into the
    /// locally lowered `interface_type`.
    ///
    /// tsc resolves a merged interface's calls with the LATER declaration
    /// group's signatures tried first (`reorderCandidates`) while display
    /// keeps forward order; a merge that always puts the checking file's
    /// group first bakes in whichever file computed the symbol type first
    /// (#17646 cross-file follow-up). So when every cross-arena declaration
    /// belongs to a program file, the groups (local + cross) are merged in
    /// forward program order with `declaration_group` stamps that let call
    /// resolution try later groups first. Any lib/unknown arena in the set
    /// keeps the legacy local-first merge (the lib augmentation order is
    /// owned by the lib resolution paths, not this loop).
    pub(crate) fn merge_cross_file_interface_declarations(
        &mut self,
        sym_id: SymbolId,
        declarations: &[NodeIndex],
        interface_type: TypeId,
    ) -> TypeId {
        let mut interface_type = interface_type;
        let mut cross_decls: Vec<(std::sync::Arc<tsz_parser::parser::NodeArena>, NodeIndex)> =
            Vec::new();
        for &decl_idx in declarations {
            let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) else {
                continue;
            };
            for arena in arenas.iter() {
                // Skip the local arena — already lowered by the caller.
                if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                    continue;
                }
                if let Some(node) = arena.get(decl_idx)
                    && arena.get_interface(node).is_some()
                {
                    cross_decls.push((std::sync::Arc::clone(arena), decl_idx));
                }
            }
        }
        // Program-file position of each cross arena; `None` means a lib or
        // otherwise unregistered arena.
        let cross_file_orders: Vec<Option<usize>> = cross_decls
            .iter()
            .map(|(arena, _)| {
                self.ctx.all_arenas.as_ref().and_then(|all| {
                    all.iter()
                        .position(|candidate| std::sync::Arc::ptr_eq(candidate, arena))
                })
            })
            .collect();
        if !cross_decls.is_empty() && cross_file_orders.iter().all(Option::is_some) {
            // All declaration groups belong to program files: merge them in
            // forward program order, stamping each later file's
            // `declaration_group` above the earlier ones so overload
            // resolution tries later groups first.
            let mut blocks: Vec<(usize, TypeId)> = Vec::new();
            if interface_type != TypeId::ERROR {
                blocks.push((self.ctx.current_file_idx, interface_type));
            }
            for ((arena, decl_idx), file_order) in cross_decls.iter().zip(&cross_file_orders) {
                let cross_type = self.lower_cross_file_interface_decl(arena, *decl_idx, sym_id);
                if cross_type != TypeId::ERROR
                    && let Some(order) = file_order
                {
                    blocks.push((*order, cross_type));
                }
            }
            // Stable: same-file re-opens keep binder declaration order.
            blocks.sort_by_key(|&(order, _)| order);
            let mut merged = TypeId::ERROR;
            for (_, block) in blocks {
                merged = if merged == TypeId::ERROR {
                    block
                } else {
                    self.merge_interface_types_cross_file_declaration(merged, block)
                };
            }
            merged
        } else {
            for (arena, decl_idx) in &cross_decls {
                let cross_type = self.lower_cross_file_interface_decl(arena, *decl_idx, sym_id);
                if cross_type != TypeId::ERROR {
                    // With no local declarations the local lowering is ERROR;
                    // the first cross-file lowering becomes the base.
                    interface_type = if interface_type == TypeId::ERROR {
                        cross_type
                    } else {
                        self.merge_interface_types(interface_type, cross_type)
                    };
                }
            }
            interface_type
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
        use crate::query_boundaries::type_predicates::is_compiler_managed_type;
        use tsz_lowering::TypeLowering;

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
        .with_nonstrict_nullish_union_reduction(self.ctx.compiler_options.strict_null_checks)
        .with_type_param_bindings(type_param_bindings);

        lowering.lower_interface_declarations_with_symbol(&[decl_idx], sym_id)
    }

    /// Lower one bodiless cross-file function declaration into a call
    /// signature.
    ///
    /// The function-symbol analog of [`Self::lower_cross_file_interface_decl`]:
    /// when one function symbol has bodiless declarations in multiple program
    /// files, each foreign declaration's signature is lowered against its own
    /// source arena while identifiers resolve by name through the current
    /// binder's merged `file_locals`. Bodiless declarations only — an
    /// implementation signature is never an externally visible overload, and a
    /// body would require inference this path deliberately avoids.
    fn lower_cross_file_function_overload_signature(
        &self,
        arena: &std::sync::Arc<tsz_parser::parser::node::NodeArena>,
        decl_idx: NodeIndex,
    ) -> Option<tsz_solver::CallSignature> {
        use crate::query_boundaries::type_predicates::is_compiler_managed_type;
        use tsz_lowering::TypeLowering;

        let arena_ref: &tsz_parser::parser::node::NodeArena = arena.as_ref();
        let lib_binders = self.get_lib_binders();

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
        .with_nonstrict_nullish_union_reduction(self.ctx.compiler_options.strict_null_checks)
        .with_type_param_bindings(type_param_bindings);

        let function_type = lowering.lower_signature_from_declaration(decl_idx, None);
        if function_type == TypeId::ERROR {
            return None;
        }
        let shape = crate::query_boundaries::type_computation::complex::get_function_shape(
            self.ctx.types,
            function_type,
        )?;
        Some(
            crate::query_boundaries::construct_signatures::call_signature_from_function_shape(
                (*shape).clone(),
                false,
            ),
        )
    }

    /// Collect a function symbol's externally visible overload signatures with
    /// tsc's `reorderCandidates` declaration-group boundaries stamped, merging
    /// bodiless declarations from every program file that re-declares the
    /// symbol.
    ///
    /// tsc keys a signature's declaration group on `signature.declaration
    /// .parent`: same-parent declarations (one `SourceFile`, one module block)
    /// form one group in source order, and groups fold in forward program
    /// order. The stored order stays forward — display renders merged sets in
    /// declaration order — while the solver's overload reorder tries later
    /// groups first at call sites. Any foreign declaration living in a lib or
    /// otherwise unregistered arena keeps the legacy local-only overload set
    /// (lib augmentation order is owned by the lib resolution paths).
    ///
    /// Returns the merged overload list plus the local implementation
    /// declaration (the last bodied local declaration, `NONE` when absent),
    /// mirroring what the local-only collection previously produced.
    pub(crate) fn merged_function_overload_signatures(
        &mut self,
        sym_id: SymbolId,
        declarations: &[NodeIndex],
    ) -> (Vec<tsz_solver::CallSignature>, NodeIndex) {
        // Origin of one bodiless signature: program-file order, then the
        // declaration parent node that delimits its group within that file.
        struct OverloadOrigin {
            file_order: usize,
            parent: NodeIndex,
            signature: tsz_solver::CallSignature,
        }

        // Foreign bodiless declarations first (immutable borrows only): the
        // owning `Arc` is cloned so signature building below can take `&mut
        // self`.
        let mut cross_decls: Vec<(
            usize,
            std::sync::Arc<tsz_parser::parser::NodeArena>,
            NodeIndex,
        )> = Vec::new();
        let mut has_unordered_cross_decl = false;
        for &decl_idx in declarations {
            let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) else {
                continue;
            };
            for arena in arenas.iter() {
                if arena.as_ref().shares_node_storage_with(self.ctx.arena) {
                    continue;
                }
                let Some(func) = arena.get_function_at(decl_idx) else {
                    continue;
                };
                if func.body.is_some() {
                    continue;
                }
                let file_order = self.ctx.all_arenas.as_ref().and_then(|all| {
                    all.iter()
                        .position(|candidate| std::sync::Arc::ptr_eq(candidate, arena))
                });
                match file_order {
                    Some(order) => {
                        cross_decls.push((order, std::sync::Arc::clone(arena), decl_idx));
                    }
                    None => has_unordered_cross_decl = true,
                }
            }
        }
        if has_unordered_cross_decl {
            cross_decls.clear();
        }

        let mut entries: Vec<OverloadOrigin> = Vec::new();
        let mut implementation_decl = NodeIndex::NONE;
        for &decl_idx in declarations {
            // A declaration index recorded for a foreign arena can collide
            // with an unrelated node in the local arena; binder provenance
            // decides locality, not index validity.
            if !self
                .ctx
                .declaration_is_local_to_current_arena(sym_id, decl_idx)
            {
                continue;
            }
            let Some(func) = self.ctx.arena.get_function_at(decl_idx) else {
                continue;
            };
            if func.body.is_none() {
                let signature = self.call_signature_from_function(func, decl_idx);
                entries.push(OverloadOrigin {
                    file_order: self.ctx.current_file_idx,
                    parent: self
                        .ctx
                        .arena
                        .parent_of(decl_idx)
                        .unwrap_or(NodeIndex::NONE),
                    signature,
                });
            } else {
                implementation_decl = decl_idx;
            }
        }

        for (file_order, arena, decl_idx) in &cross_decls {
            let Some(signature) =
                self.lower_cross_file_function_overload_signature(arena, *decl_idx)
            else {
                continue;
            };
            entries.push(OverloadOrigin {
                file_order: *file_order,
                parent: arena.parent_of(*decl_idx).unwrap_or(NodeIndex::NONE),
                signature,
            });
        }

        // Stable: same-file declarations keep binder declaration order.
        entries.sort_by_key(|entry| entry.file_order);

        let mut overloads = Vec::with_capacity(entries.len());
        let mut group: u32 = 0;
        let mut last_key: Option<(usize, NodeIndex)> = None;
        for entry in entries {
            let key = (entry.file_order, entry.parent);
            if last_key.is_some() && last_key != Some(key) {
                group += 1;
            }
            last_key = Some(key);
            let mut signature = entry.signature;
            signature.declaration_group = group;
            overloads.push(signature);
        }
        (overloads, implementation_decl)
    }

    /// Resolve a cross-file interface's heritage-base name in the binder scope of
    /// the arena that owns the `extends` clause, registering the owning file index
    /// so downstream `get_type_of_symbol` / `get_type_params_for_symbol` take the
    /// cross-arena delegation path.
    ///
    /// `resolve_cross_file_global_type_symbol` resolves names in the *current*
    /// (importing) file's locals + globals. A base declared and exported only by
    /// the declaring module (e.g. `interface MutationObserverOptions extends
    /// MutationOptions`, where the consuming file imports `MutationObserverOptions`
    /// but never `MutationOptions`) is module-scoped, not global, so it is unseen
    /// there and every inherited member is dropped. Resolve the name in the
    /// declaring arena's binder `file_locals` instead — that scope contains the
    /// module's own top-level declarations.
    ///
    /// The returned `SymbolId` belongs to the declaring binder. Because raw
    /// `SymbolId`s are per-binder (a foreign id passed to `get_type_of_symbol`
    /// silently resolves the wrong symbol in the current arena), register its
    /// owning file index in the cross-file overlay before returning. The overlay
    /// is what `get_type_of_symbol` / `get_type_params_for_symbol` consult to
    /// delegate resolution to the declaring arena's checker, so registration is
    /// required for correctness, not just an optimization.
    fn resolve_heritage_base_symbol_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        name: &str,
    ) -> Option<SymbolId> {
        use crate::query_boundaries::type_predicates::is_compiler_managed_type;

        if is_compiler_managed_type(name) {
            return None;
        }
        let owner_binder = self.ctx.get_binder_for_arena(arena)?;
        let owner_file_idx = self.ctx.get_file_idx_for_arena(arena)?;
        // The declaring module's own top-level scope. Resolving here (rather than
        // in the importing file's locals/globals) is what reaches a module-scoped,
        // non-imported base.
        let base_sym_id = owner_binder.file_locals.get(name)?;
        let symbol = owner_binder.get_symbol(base_sym_id)?;
        if !symbol.has_any_flags(symbol_flags::TYPE) {
            return None;
        }
        // Pin the foreign symbol to its declaring file so the type / type-param
        // queries below delegate to the right arena instead of mis-resolving the
        // raw id against the current binder.
        if owner_file_idx != self.ctx.current_file_idx {
            self.ctx
                .register_symbol_file_target(base_sym_id, owner_file_idx);
        }
        Some(base_sym_id)
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
                        // Prefer the importing file's locals/globals resolution to
                        // preserve every base that already resolved there (changing
                        // those would perturb downstream relation shapes). Only when a
                        // base is unresolvable here — a module-scoped base declared and
                        // exported by the declaring module but never imported into the
                        // consuming file (e.g. `MutationObserverOptions extends
                        // MutationOptions`, where the consumer imports
                        // `MutationObserverOptions` but never `MutationOptions`) — fall
                        // back to resolving the name in the DECLARING arena's binder
                        // scope, where the module's own top-level declarations live.
                        // Without this fallback the loop `continue`s and drops every
                        // inherited member. `resolve_heritage_base_symbol_in_arena`
                        // registers the owning file index so the `get_type_of_symbol` /
                        // `get_type_params_for_symbol` calls below take the proper
                        // cross-arena delegation path (rather than mis-resolving a
                        // foreign-binder SymbolId in the current arena).
                        let Some(base_sym_id) = self
                            .resolve_cross_file_global_type_symbol(&name)
                            .or_else(|| self.resolve_heritage_base_symbol_in_arena(arena, &name))
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

                        derived_type = self.merge_interface_types_heritage(derived_type, base_type);
                    }
                }
            }
        }

        derived_type
    }
}
