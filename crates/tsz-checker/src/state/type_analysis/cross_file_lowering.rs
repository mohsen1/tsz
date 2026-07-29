//! Cross-file interface lowering and heritage merge, plus cross-file symbol
//! recording. Split from `cross_file.rs` to respect the 2000-line file cap;
//! child module of the same `CheckerState` impl surface.
use crate::state::CheckerState;
use crate::state_type_analysis::source_file_import_binding::source_file_import_binding_symbol;
use crate::symbols_domain::name_text::expression_name_text_in_arena;
use std::sync::Arc;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

#[derive(Clone, Copy)]
struct DeclarationScopeSymbol {
    sym_id: SymbolId,
    file_idx: Option<usize>,
}

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

    /// Lower interface declarations from one cross-file arena.
    ///
    /// When an interface is declared across multiple files (e.g., global script
    /// interface merging), each cross-file declaration lives in a different
    /// `NodeArena`. This method creates a `TypeLowering` bound to the source arena
    /// and uses name-based resolution via `file_locals` to resolve type references.
    pub(crate) fn lower_cross_file_interface_declarations(
        &self,
        arena: &Arc<NodeArena>,
        declarations: &[NodeIndex],
        sym_id: SymbolId,
    ) -> TypeId {
        let owner_binder = self
            .ctx
            .get_binder_for_arena(arena)
            .unwrap_or(self.ctx.binder);
        self.lower_cross_file_interface_declarations_with_binder(
            owner_binder,
            arena,
            declarations,
            sym_id,
        )
    }

    /// Lower declarations using the binder that owns `arena`.
    ///
    /// Cross-file lookup binders deliberately retain file-local symbol maps.
    /// Keeping the owning binder next to its arena preserves module-local and
    /// imported identities without exposing program-wide declaration
    /// provenance to every delegated checker query.
    pub(crate) fn lower_cross_file_interface_declarations_with_binder(
        &self,
        owner_binder: &BinderState,
        arena: &Arc<NodeArena>,
        declarations: &[NodeIndex],
        sym_id: SymbolId,
    ) -> TypeId {
        use crate::query_boundaries::type_predicates::is_compiler_managed_type;
        use tsz_lowering::TypeLowering;

        let arena_ref = arena.as_ref();
        let lib_binders = self.get_lib_binders();
        let resolve_name =
            |name: &str| self.resolve_declaration_scope_symbol(owner_binder, arena_ref, name);
        let resolved_symbol_has_flags = |resolved: DeclarationScopeSymbol, required_flags: u32| {
            resolved
                .file_idx
                .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
                .and_then(|binder| binder.get_symbol(resolved.sym_id))
                .or_else(|| owner_binder.get_symbol(resolved.sym_id))
                .or_else(|| self.get_cross_file_symbol(resolved.sym_id))
                .or_else(|| {
                    self.ctx
                        .binder
                        .get_symbol_with_libs(resolved.sym_id, &lib_binders)
                })
                .is_some_and(|symbol| symbol.has_any_flags(required_flags))
        };

        let cross_type_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let node = arena_ref.get(node_idx)?;
            let ident = arena_ref.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if is_compiler_managed_type(name) {
                return None;
            }
            let resolved = resolve_name(name)?;
            resolved_symbol_has_flags(resolved, symbol_flags::TYPE).then_some(resolved.sym_id.0)
        };

        let cross_def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
            let node = arena_ref.get(node_idx)?;
            let ident = arena_ref.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if is_compiler_managed_type(name) {
                return None;
            }
            let resolved = resolve_name(name)?;
            resolved_symbol_has_flags(resolved, symbol_flags::TYPE)
                .then(|| self.declaration_scope_symbol_def_id(resolved))
        };

        let cross_value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let node = arena_ref.get(node_idx)?;
            let ident = arena_ref.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            let resolved = resolve_name(name)?;
            resolved_symbol_has_flags(
                resolved,
                symbol_flags::VALUE
                    | symbol_flags::ALIAS
                    | symbol_flags::REGULAR_ENUM
                    | symbol_flags::CONST_ENUM,
            )
            .then_some(resolved.sym_id.0)
        };
        let name_resolver = |name: &str| -> Option<tsz_solver::def::DefId> {
            resolve_name(name)
                .filter(|&resolved| resolved_symbol_has_flags(resolved, symbol_flags::TYPE))
                .map(|resolved| self.declaration_scope_symbol_def_id(resolved))
                .or_else(|| {
                    (!self.ctx.file_local_type_shadow_for_lib_name(name))
                        .then(|| self.resolve_actual_lib_name_to_def_id_for_lowering(name))
                        .flatten()
                })
                .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(name))
        };

        let type_param_bindings = self.get_type_param_bindings();
        let lowering = TypeLowering::with_hybrid_resolver(
            arena_ref,
            self.ctx.types,
            &cross_type_resolver,
            &cross_def_id_resolver,
            &cross_value_resolver,
        )
        .with_type_param_bindings(type_param_bindings)
        .with_name_def_id_resolver(&name_resolver);
        let lowering = if let Some(symbol) = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))
        {
            lowering.with_preferred_self_reference(
                symbol.escaped_name.clone(),
                self.ctx.get_or_create_def_id(sym_id),
            )
        } else {
            lowering
        };

        lowering.lower_interface_declarations_with_symbol(declarations, sym_id)
    }

    fn resolve_declaration_scope_symbol(
        &self,
        owner_binder: &BinderState,
        arena: &NodeArena,
        name: &str,
    ) -> Option<DeclarationScopeSymbol> {
        let owner_file_idx = self.ctx.get_file_idx_for_arena(arena);
        let mut segments = name.split('.');
        let root_name = segments.next()?;
        let raw_sym_id = source_file_import_binding_symbol(arena, owner_binder, root_name)
            .or_else(|| owner_binder.file_locals.get(root_name))?;

        let mut resolved = if let Some(file_idx) = owner_file_idx
            && let Some(target) = self.source_file_import_alias_target_for_lowering(
                file_idx,
                owner_binder,
                raw_sym_id,
            ) {
            if let Some(target_file_idx) = target.file_idx {
                self.ctx
                    .register_symbol_file_target(target.sym_id, target_file_idx);
            }
            DeclarationScopeSymbol {
                sym_id: target.sym_id,
                file_idx: target.file_idx,
            }
        } else {
            let file_idx = if owner_binder.lib_symbol_ids.contains(&raw_sym_id) {
                None
            } else {
                owner_file_idx
            };
            DeclarationScopeSymbol {
                sym_id: raw_sym_id,
                file_idx,
            }
        };

        for segment in segments {
            let symbol = resolved
                .file_idx
                .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
                .and_then(|binder| binder.get_symbol(resolved.sym_id))
                .or_else(|| owner_binder.get_symbol(resolved.sym_id))
                .or_else(|| self.get_cross_file_symbol(resolved.sym_id))?;
            resolved.sym_id = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(segment))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(segment))
                })?;
            resolved.file_idx = self
                .ctx
                .resolve_symbol_file_index(resolved.sym_id)
                .or(resolved.file_idx);
        }

        if let Some(file_idx) = resolved.file_idx
            && file_idx != self.ctx.current_file_idx
        {
            self.ctx
                .register_symbol_file_target(resolved.sym_id, file_idx);
        }
        Some(resolved)
    }

    fn declaration_scope_symbol_def_id(
        &self,
        resolved: DeclarationScopeSymbol,
    ) -> tsz_solver::def::DefId {
        if let Some(file_idx) = resolved.file_idx
            && let Some(symbol) = self
                .ctx
                .get_binder_for_file(file_idx)
                .and_then(|binder| binder.get_symbol(resolved.sym_id))
            && let Some(def_id) = self.ctx.def_id_for_declaration_in_file(
                resolved.sym_id,
                file_idx,
                &symbol.escaped_name,
            )
        {
            return def_id;
        }
        self.ctx.get_or_create_def_id(resolved.sym_id)
    }

    fn declaration_scope_symbol_has_flags(
        &self,
        owner_binder: &BinderState,
        resolved: DeclarationScopeSymbol,
        required_flags: u32,
    ) -> bool {
        let lib_binders = self.get_lib_binders();
        resolved
            .file_idx
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .and_then(|binder| binder.get_symbol(resolved.sym_id))
            .or_else(|| owner_binder.get_symbol(resolved.sym_id))
            .or_else(|| self.get_cross_file_symbol(resolved.sym_id))
            .or_else(|| {
                self.ctx
                    .binder
                    .get_symbol_with_libs(resolved.sym_id, &lib_binders)
            })
            .is_some_and(|symbol| symbol.has_any_flags(required_flags))
    }

    fn resolve_cross_file_heritage_type_arg_in_scope(
        &mut self,
        owner_binder: &BinderState,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> TypeId {
        use crate::types_domain::queries::lib_resolution::keyword_syntax_to_type_id;

        let Some(node) = arena.get(node_idx) else {
            return TypeId::UNKNOWN;
        };
        if let Some(builtin) = keyword_syntax_to_type_id(node.kind) {
            return builtin;
        }

        let name = if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            arena
                .get_type_ref(node)
                .and_then(|type_ref| expression_name_text_in_arena(arena, type_ref.type_name))
        } else {
            expression_name_text_in_arena(arena, node_idx)
        };
        let Some(name) = name else {
            return TypeId::UNKNOWN;
        };
        if let Some(&type_id) = self.ctx.type_parameter_scope.get(&name) {
            return type_id;
        }
        if let Some(resolved) = self.resolve_declaration_scope_symbol(owner_binder, arena, &name)
            && self.declaration_scope_symbol_has_flags(owner_binder, resolved, symbol_flags::TYPE)
        {
            return self.get_type_of_symbol(resolved.sym_id);
        }
        if let Some(sym_id) = self.resolve_cross_file_global_type_symbol(&name) {
            return self.get_type_of_symbol(sym_id);
        }

        let atom = self.ctx.types.intern_string(&name);
        self.ctx.types.type_param(tsz_solver::TypeParamInfo {
            name: atom,
            constraint: None,
            default: None,
            is_const: false,
            origin: tsz_solver::TypeParamOrigin::User,
        })
    }

    /// Merge heritage for one exact `(binder, arena, declarations)` group.
    ///
    /// This is the provenance-preserving counterpart to
    /// [`Self::merge_cross_file_heritage`]. It does not re-query the current
    /// binder's declaration-arena map, which is intentionally empty on CLI
    /// lookup binders.
    pub(crate) fn merge_cross_file_heritage_for_declaration_group(
        &mut self,
        owner_binder: &BinderState,
        arena: &NodeArena,
        declarations: &[NodeIndex],
        mut derived_type: TypeId,
    ) -> TypeId {
        use tsz_scanner::SyntaxKind;

        for &decl_idx in declarations {
            let Some(node) = arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = arena.get_interface(node) else {
                continue;
            };
            let Some(heritage_clauses) = interface.heritage_clauses.as_ref() else {
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

                    let base_sym_id = self
                        .resolve_declaration_scope_symbol(owner_binder, arena, &name)
                        .filter(|&resolved| {
                            self.declaration_scope_symbol_has_flags(
                                owner_binder,
                                resolved,
                                symbol_flags::TYPE,
                            )
                        })
                        .map(|resolved| resolved.sym_id)
                        .or_else(|| self.resolve_cross_file_global_type_symbol(&name));
                    let Some(base_sym_id) = base_sym_id else {
                        continue;
                    };
                    let mut base_type = self.get_type_of_symbol(base_sym_id);
                    if matches!(base_type, TypeId::ERROR | TypeId::UNKNOWN) {
                        continue;
                    }

                    if let Some(type_arguments) = type_arguments {
                        let base_params = self.get_type_params_for_symbol(base_sym_id);
                        if !base_params.is_empty() {
                            let mut type_args = Vec::with_capacity(type_arguments.nodes.len());
                            for &arg_idx in &type_arguments.nodes {
                                type_args.push(self.resolve_cross_file_heritage_type_arg_in_scope(
                                    owner_binder,
                                    arena,
                                    arg_idx,
                                ));
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
                                crate::query_boundaries::type_rewrite::TypeSubstitution::from_args(
                                    self.ctx.types,
                                    &base_params,
                                    &type_args,
                                );
                            base_type = crate::query_boundaries::type_rewrite::instantiate_type(
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

        derived_type
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
            for arena in arenas {
                // Skip the local arena (already processed by
                // `merge_interface_heritage_types`). `NodeArena` wrappers may
                // differ while sharing the same parsed-node storage.
                if arena.as_ref().shares_node_storage_with(self.ctx.arena) {
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
                                    crate::query_boundaries::type_rewrite::TypeSubstitution::from_args(
                                        self.ctx.types,
                                        &base_params,
                                        &type_args,
                                    );
                                base_type = crate::query_boundaries::type_rewrite::instantiate_type(
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
