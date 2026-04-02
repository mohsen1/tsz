//! Helper methods for duplicate identifier checking.
//!
//! Extracted from `duplicate_identifiers.rs` to keep that module under 2000 LOC.
//! All methods here are `impl CheckerState` helpers called from
//! `check_duplicate_identifiers` or its sub-routines.

use super::duplicate_identifiers::{DuplicateDeclarationOrigin, OuterDeclResult};
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(super) fn find_visible_outer_declarations_for_block_function(
        &self,
        decl_idx: NodeIndex,
        current_sym_id: tsz_binder::SymbolId,
        name: &str,
    ) -> OuterDeclResult {
        let containing_scope_id = self.get_containing_scope_id(decl_idx)?;
        let mut scope_id = self
            .ctx
            .binder
            .scopes
            .get(containing_scope_id.0 as usize)?
            .parent;

        while scope_id.is_some() {
            let scope = self.ctx.binder.scopes.get(scope_id.0 as usize)?;
            if let Some(sym_id) = scope.table.get(name) {
                if sym_id == current_sym_id {
                    return None;
                }

                let local_decls = self.local_declarations_for_symbol(sym_id, name);
                if local_decls.is_empty() {
                    return None;
                }

                let non_catch_local_decls: Vec<(NodeIndex, u32)> = local_decls
                    .into_iter()
                    .filter(|(outer_decl_idx, _)| {
                        !self.is_catch_clause_variable_declaration(*outer_decl_idx)
                    })
                    .collect();
                if !non_catch_local_decls.is_empty() {
                    return Some((sym_id, non_catch_local_decls));
                }
            }
            scope_id = scope.parent;
        }

        None
    }

    fn get_containing_scope_id(&self, decl_idx: NodeIndex) -> Option<tsz_binder::ScopeId> {
        let mut current = decl_idx;

        loop {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            if let Some(&scope_id) = self.ctx.binder.node_scope_ids.get(&parent.0) {
                return Some(scope_id);
            }
            current = parent;
        }
    }

    fn local_declarations_for_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
        expected_name: &str,
    ) -> Vec<(NodeIndex, u32)> {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return Vec::new();
        };

        let mut declarations = Vec::new();
        let mut seen = FxHashSet::default();

        for &decl_idx in &symbol.declarations {
            let mut push_local_decl = |arena: &tsz_parser::parser::node::NodeArena| {
                if !std::ptr::eq(arena, self.ctx.arena) {
                    return;
                }
                if !seen.insert(decl_idx) || !self.declaration_name_matches(decl_idx, expected_name)
                {
                    return;
                }
                if let Some(flags) = self.declaration_symbol_flags(arena, decl_idx) {
                    declarations.push((decl_idx, flags));
                }
            };

            if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena_arc in arenas {
                    push_local_decl(arena_arc.as_ref());
                }
            } else {
                push_local_decl(self.ctx.arena);
            }
        }

        declarations.sort_by_key(|(decl_idx, _)| {
            self.ctx
                .arena
                .get(*decl_idx)
                .map_or(u32::MAX, |node| node.pos)
        });
        declarations
    }

    pub(crate) fn get_enclosing_namespace(&self, decl_idx: NodeIndex) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = decl_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return NodeIndex::NONE;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return NodeIndex::NONE;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return NodeIndex::NONE;
            };
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                return parent;
            }
            if parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                return NodeIndex::NONE;
            }
            current = parent;
        }
    }

    /// Get the SymbolId of the enclosing namespace for a declaration.
    /// Returns `SymbolId::NONE` for file/global scope declarations.
    /// Unlike `get_enclosing_namespace` (which returns a `NodeIndex`), this resolves
    /// to the namespace's symbol, ensuring that separate `namespace M { }` blocks
    /// with the same name map to the same key.
    pub(super) fn get_enclosing_namespace_symbol(
        &self,
        decl_idx: NodeIndex,
    ) -> tsz_binder::SymbolId {
        let ns_node = self.get_enclosing_namespace(decl_idx);
        if ns_node.is_none() {
            return tsz_binder::SymbolId::NONE;
        }
        // Look up the symbol for this MODULE_DECLARATION node
        self.ctx
            .binder
            .node_symbols
            .get(&ns_node.0)
            .copied()
            .unwrap_or(tsz_binder::SymbolId::NONE)
    }

    pub(super) fn module_augmentation_conflict_declarations_for_current_file(
        &self,
        name: &str,
    ) -> Vec<(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)> {
        let mut declarations = Vec::new();
        let mut seen = FxHashSet::default();

        let mut push_remote_decl =
            |file_idx: usize, decl_idx: NodeIndex, flags: u32, is_exported: bool| {
                if seen.insert((file_idx, decl_idx.0)) {
                    declarations.push((
                        decl_idx,
                        flags,
                        false,
                        is_exported,
                        DuplicateDeclarationOrigin::TargetedModuleAugmentation,
                    ));
                }
            };

        let mut consider_augmentation =
            |module_spec: &str,
             augmenting_file_idx: usize,
             augmentation: &tsz_binder::ModuleAugmentation| {
                if augmentation.name != name {
                    return;
                }
                let Some(target_idx) = self
                    .ctx
                    .resolve_import_target_from_file(augmenting_file_idx, module_spec)
                else {
                    return;
                };

                if target_idx == self.ctx.current_file_idx && augmenting_file_idx != target_idx {
                    let arena = augmentation
                        .arena
                        .as_deref()
                        .unwrap_or_else(|| self.ctx.get_arena_for_file(augmenting_file_idx as u32));
                    let Some(flags) = self.declaration_symbol_flags(arena, augmentation.node)
                    else {
                        return;
                    };
                    let is_exported = self.is_declaration_exported(arena, augmentation.node);
                    push_remote_decl(augmenting_file_idx, augmentation.node, flags, is_exported);
                    return;
                }

                if augmenting_file_idx == self.ctx.current_file_idx {
                    for (decl_idx, flags, is_exported) in
                        self.export_surface_declarations_in_file(target_idx, name)
                    {
                        push_remote_decl(target_idx, decl_idx, flags, is_exported);
                    }
                }
            };

        let augmentation_owner_file_idx = |augmentation: &tsz_binder::ModuleAugmentation| {
            augmentation
                .arena
                .as_deref()
                .and_then(|arena| self.ctx.get_file_idx_for_arena(arena))
                .unwrap_or(self.ctx.current_file_idx)
        };

        for (module_spec, augmentations) in &self.ctx.binder.module_augmentations {
            for augmentation in augmentations {
                consider_augmentation(
                    module_spec,
                    augmentation_owner_file_idx(augmentation),
                    augmentation,
                );
            }
        }

        if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
            for (module_spec, entries) in aug_index.iter() {
                for (augmenting_file_idx, augmentation) in entries {
                    if *augmenting_file_idx == self.ctx.current_file_idx {
                        continue;
                    }
                    consider_augmentation(module_spec, *augmenting_file_idx, augmentation);
                }
            }
        } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
            for (augmenting_file_idx, binder) in all_binders.iter().enumerate() {
                if augmenting_file_idx == self.ctx.current_file_idx {
                    continue;
                }
                for (module_spec, augmentations) in &binder.module_augmentations {
                    for augmentation in augmentations {
                        consider_augmentation(module_spec, augmenting_file_idx, augmentation);
                    }
                }
            }
        }

        declarations
    }

    pub(crate) fn export_surface_declarations_in_file(
        &self,
        file_idx: usize,
        name: &str,
    ) -> Vec<(NodeIndex, u32, bool)> {
        let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
            return Vec::new();
        };
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let file_name = arena
            .source_files
            .first()
            .map(|sf| sf.file_name.as_str())
            .unwrap_or_default();
        let mut export_keys = vec![file_name.to_string()];
        if let Some(stripped) = file_name.strip_prefix("./") {
            export_keys.push(stripped.to_string());
        } else if !file_name.starts_with("../") && !file_name.starts_with('/') {
            export_keys.push(format!("./{file_name}"));
        }

        let sym_id = binder
            .file_locals
            .get(name)
            .or_else(|| {
                arena.source_files.first().and_then(|source_file| {
                    source_file.statements.nodes.iter().find_map(|stmt_idx| {
                        let stmt_node = arena.get(*stmt_idx)?;
                        if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                            return None;
                        }
                        let export_decl = arena.get_export_decl(stmt_node)?;
                        let clause_node = arena.get(export_decl.export_clause)?;
                        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                            return None;
                        }
                        let named_exports = arena.get_named_imports(clause_node)?;
                        named_exports.elements.nodes.iter().find_map(|spec_idx| {
                            let spec_node = arena.get(*spec_idx)?;
                            let spec = arena.get_specifier(spec_node)?;
                            let export_name = arena
                                .get(spec.property_name)
                                .and_then(|n| arena.get_identifier(n))
                                .or_else(|| {
                                    arena.get(spec.name).and_then(|n| arena.get_identifier(n))
                                })?;
                            (export_name.escaped_text == name)
                                .then(|| binder.get_node_symbol(*spec_idx))
                                .flatten()
                        })
                    })
                })
            })
            .or_else(|| {
                export_keys.iter().find_map(|key| {
                    self.module_exports_for_file(binder, key)
                        .and_then(|exports| self.resolve_export_from_table(binder, exports, name))
                })
            })
            .or_else(|| {
                binder
                    .get_symbols()
                    .find_all_by_name(name)
                    .iter()
                    .find_map(|candidate_id| {
                        let symbol = binder.get_symbol(*candidate_id)?;
                        if !symbol.is_exported {
                            return None;
                        }
                        symbol
                            .declarations
                            .iter()
                            .any(|decl_idx| {
                                if let Some(arenas) =
                                    binder.declaration_arenas.get(&(*candidate_id, *decl_idx))
                                {
                                    arenas
                                        .iter()
                                        .any(|decl_arena| std::ptr::eq(decl_arena.as_ref(), arena))
                                } else {
                                    true
                                }
                            })
                            .then_some(*candidate_id)
                    })
            });
        let Some(sym_id) = sym_id else {
            let Some(owner_binder) = self.ctx.get_binder_for_file(file_idx) else {
                return Vec::new();
            };
            let owner_arena = self.ctx.get_arena_for_file(file_idx as u32);
            let Some(owner_file_name) = owner_arena
                .source_files
                .first()
                .map(|sf| sf.file_name.clone())
            else {
                return Vec::new();
            };
            if let Some(exports) = self.module_exports_for_file(owner_binder, &owner_file_name)
                && let Some(resolved_sym_id) =
                    self.resolve_export_from_table(owner_binder, exports, name)
            {
                let Some(symbol) = owner_binder.get_symbol(resolved_sym_id) else {
                    return Vec::new();
                };

                let mut declarations = Vec::new();
                let mut seen = FxHashSet::default();

                for &decl_idx in &symbol.declarations {
                    if let Some(arenas) = owner_binder
                        .declaration_arenas
                        .get(&(resolved_sym_id, decl_idx))
                    {
                        for decl_arena in arenas {
                            if !std::ptr::eq(decl_arena.as_ref(), owner_arena)
                                || !seen.insert(decl_idx.0)
                            {
                                continue;
                            }
                            if let Some(flags) =
                                self.declaration_symbol_flags(decl_arena.as_ref(), decl_idx)
                            {
                                let is_exported =
                                    self.is_declaration_exported(decl_arena.as_ref(), decl_idx);
                                declarations.push((decl_idx, flags, is_exported));
                            }
                        }
                    } else if seen.insert(decl_idx.0)
                        && let Some(flags) = self.declaration_symbol_flags(owner_arena, decl_idx)
                    {
                        let is_exported = self.is_declaration_exported(owner_arena, decl_idx);
                        declarations.push((decl_idx, flags, is_exported));
                    }
                }

                return declarations;
            }

            return Vec::new();
        };
        let Some(symbol) = binder.get_symbol(sym_id) else {
            return Vec::new();
        };

        let mut declarations = Vec::new();
        let mut seen = FxHashSet::default();

        for &decl_idx in &symbol.declarations {
            if let Some(arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for decl_arena in arenas {
                    if !std::ptr::eq(decl_arena.as_ref(), arena) || !seen.insert(decl_idx.0) {
                        continue;
                    }
                    if let Some(flags) =
                        self.declaration_symbol_flags(decl_arena.as_ref(), decl_idx)
                    {
                        let is_exported =
                            self.is_declaration_exported(decl_arena.as_ref(), decl_idx);
                        declarations.push((decl_idx, flags, is_exported));
                    } else if name == "default"
                        && let Some(target_sym_id) = self
                            .default_export_alias_target_symbol_in_file(
                                binder,
                                decl_arena.as_ref(),
                                decl_idx,
                            )
                    {
                        self.push_export_surface_symbol_declarations_in_file(
                            binder,
                            arena,
                            target_sym_id,
                            Some(true),
                            &mut declarations,
                            &mut seen,
                        );
                    }
                }
            } else if seen.insert(decl_idx.0)
                && let Some(flags) = self.declaration_symbol_flags(arena, decl_idx)
            {
                let is_exported = self.is_declaration_exported(arena, decl_idx);
                declarations.push((decl_idx, flags, is_exported));
            } else if name == "default"
                && let Some(target_sym_id) =
                    self.default_export_alias_target_symbol_in_file(binder, arena, decl_idx)
            {
                self.push_export_surface_symbol_declarations_in_file(
                    binder,
                    arena,
                    target_sym_id,
                    Some(true),
                    &mut declarations,
                    &mut seen,
                );
            }
        }

        if declarations.is_empty()
            && let Some(resolved_sym_id) = export_keys.iter().find_map(|key| {
                self.module_exports_for_file(binder, key)
                    .and_then(|exports| self.resolve_export_from_table(binder, exports, name))
            })
        {
            let mut resolved_seen = FxHashSet::default();
            self.push_export_surface_symbol_declarations_in_file(
                binder,
                arena,
                resolved_sym_id,
                Some(true),
                &mut declarations,
                &mut resolved_seen,
            );
        }

        declarations
    }

    fn default_export_alias_target_symbol_in_file(
        &self,
        binder: &tsz_binder::BinderState,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let export_decl_idx = self.resolve_duplicate_decl_node(arena, decl_idx)?;
        let export_node = arena.get(export_decl_idx)?;
        if export_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
            return None;
        }
        let export_decl = arena.get_export_decl(export_node)?;
        if !export_decl.is_default_export {
            return None;
        }
        let clause_node = arena.get(export_decl.export_clause)?;
        if clause_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let ident = arena.get_identifier(clause_node)?;
        binder.file_locals.get(&ident.escaped_text)
    }

    fn push_export_surface_symbol_declarations_in_file(
        &self,
        binder: &tsz_binder::BinderState,
        arena: &tsz_parser::parser::node::NodeArena,
        sym_id: tsz_binder::SymbolId,
        exported_override: Option<bool>,
        declarations: &mut Vec<(NodeIndex, u32, bool)>,
        seen: &mut FxHashSet<u32>,
    ) {
        let Some(symbol) = binder.get_symbol(sym_id) else {
            return;
        };

        for &decl_idx in &symbol.declarations {
            if let Some(arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for decl_arena in arenas {
                    if !std::ptr::eq(decl_arena.as_ref(), arena) || !seen.insert(decl_idx.0) {
                        continue;
                    }
                    if let Some(flags) =
                        self.declaration_symbol_flags(decl_arena.as_ref(), decl_idx)
                    {
                        let is_exported = exported_override.unwrap_or_else(|| {
                            self.is_declaration_exported(decl_arena.as_ref(), decl_idx)
                        });
                        declarations.push((decl_idx, flags, is_exported));
                    }
                }
            } else if seen.insert(decl_idx.0)
                && let Some(flags) = self.declaration_symbol_flags(arena, decl_idx)
            {
                let is_exported = exported_override
                    .unwrap_or_else(|| self.is_declaration_exported(arena, decl_idx));
                declarations.push((decl_idx, flags, is_exported));
            }
        }
    }

    fn module_augmentation_targets_current_file_export(
        &self,
        augmenting_file_idx: usize,
        module_spec: &str,
        export_name: &str,
    ) -> bool {
        let Some(target_idx) = self
            .ctx
            .resolve_import_target_from_file(augmenting_file_idx, module_spec)
        else {
            return false;
        };

        if target_idx == self.ctx.current_file_idx {
            return true;
        }

        let mut visited = FxHashSet::default();
        self.resolve_export_in_file(target_idx, export_name, &mut visited)
            .is_some_and(|(_, owner_idx)| owner_idx == self.ctx.current_file_idx)
    }

    pub(super) fn same_name_top_level_script_declarations_for_current_file(
        &self,
        name: &str,
    ) -> Vec<(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)> {
        let Some(all_arenas) = self.ctx.all_arenas.as_ref() else {
            return Vec::new();
        };
        if self.ctx.binder.is_external_module() {
            return Vec::new();
        }

        let mut declarations = Vec::new();

        for (file_idx, arena) in all_arenas.iter().enumerate() {
            if file_idx == self.ctx.current_file_idx {
                continue;
            }
            let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
                continue;
            };
            if binder.is_external_module() {
                continue;
            }

            declarations.extend(self.top_level_script_declarations_in_arena(arena.as_ref(), name));
        }

        declarations
    }

    pub(super) fn is_namespace_export_declaration_name_in_current_file(
        &self,
        decl_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION {
            return true;
        }
        let Some(ext) = self.ctx.arena.get_extended(decl_idx) else {
            return false;
        };
        if !ext.parent.is_some() {
            return false;
        }
        self.ctx
            .arena
            .get(ext.parent)
            .is_some_and(|parent| parent.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION)
    }

    pub(super) fn is_block_scoped_global_augmentation_value_decl_in_current_file(
        &self,
        decl_idx: NodeIndex,
        flags: u32,
    ) -> bool {
        use tsz_parser::parser::node_flags;

        if (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0 || (flags & symbol_flags::VALUE) == 0
        {
            return false;
        }

        let mut current = decl_idx;
        for _ in 0..32 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if !ext.parent.is_some() {
                return false;
            }
            let parent_idx = ext.parent;
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent.kind == syntax_kind_ext::MODULE_DECLARATION {
                let is_global_augmentation =
                    (u32::from(parent.flags) & node_flags::GLOBAL_AUGMENTATION) != 0
                        || self
                            .ctx
                            .arena
                            .get_module(parent)
                            .and_then(|module| self.ctx.arena.get(module.name))
                            .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                            .is_some_and(|ident| ident.escaped_text == "global");
                if is_global_augmentation {
                    return true;
                }
            }
            current = parent_idx;
        }

        false
    }

    fn top_level_script_declarations_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        name: &str,
    ) -> Vec<(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)> {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(source_file) = arena.source_files.first() else {
            return Vec::new();
        };

        let mut declarations = Vec::new();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            let decl_idx = if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                let Some(export_decl) = arena.get_export_decl(stmt_node) else {
                    continue;
                };
                export_decl.export_clause
            } else {
                stmt_idx
            };
            let Some(decl_node) = arena.get(decl_idx) else {
                continue;
            };

            let matches_name = match decl_node.kind {
                syntax_kind_ext::FUNCTION_DECLARATION => arena
                    .get_function(decl_node)
                    .and_then(|decl| arena.get_identifier_at(decl.name))
                    .is_some_and(|ident| ident.escaped_text == name),
                syntax_kind_ext::CLASS_DECLARATION => arena
                    .get_class(decl_node)
                    .and_then(|decl| arena.get_identifier_at(decl.name))
                    .is_some_and(|ident| ident.escaped_text == name),
                syntax_kind_ext::INTERFACE_DECLARATION => arena
                    .get_interface(decl_node)
                    .and_then(|decl| arena.get_identifier_at(decl.name))
                    .is_some_and(|ident| ident.escaped_text == name),
                syntax_kind_ext::TYPE_ALIAS_DECLARATION => arena
                    .get_type_alias(decl_node)
                    .and_then(|decl| arena.get_identifier_at(decl.name))
                    .is_some_and(|ident| ident.escaped_text == name),
                syntax_kind_ext::ENUM_DECLARATION => arena
                    .get_enum(decl_node)
                    .and_then(|decl| arena.get_identifier_at(decl.name))
                    .is_some_and(|ident| ident.escaped_text == name),
                _ => false,
            };
            if !matches_name {
                continue;
            }

            let Some(flags) = self.declaration_symbol_flags(arena, decl_idx) else {
                continue;
            };
            let is_exported = self.is_declaration_exported(arena, decl_idx);
            declarations.push((
                decl_idx,
                flags,
                false,
                is_exported,
                DuplicateDeclarationOrigin::SymbolDeclaration,
            ));
        }

        declarations
    }

    /// Detect cross-file global scope conflicts between UMD namespace exports
    /// (`export as namespace X`) and `declare global { const/let X }` across
    /// external module files. Returns remote declarations that conflict with
    /// the given name from the current file's global contributions.
    ///
    /// Scans other files' ASTs directly because `all_binders` stores merged
    /// global augmentations (identical for every file), not per-file data.
    pub(super) fn global_scope_conflict_declarations_for_current_file(
        &self,
        name: &str,
    ) -> Vec<(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)> {
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(all_arenas) = self.ctx.all_arenas.as_ref() else {
            return Vec::new();
        };

        let mut declarations = Vec::new();

        for (file_idx, arena) in all_arenas.iter().enumerate() {
            if file_idx == self.ctx.current_file_idx {
                continue;
            }

            let Some(source_file_node) = arena.source_files.first() else {
                continue;
            };

            for &stmt_idx in &source_file_node.statements.nodes {
                let Some(stmt_node) = arena.get(stmt_idx) else {
                    continue;
                };

                // Check for `export as namespace X` (UMD namespace export)
                if stmt_node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION {
                    if let Some(export) = arena.get_export_decl(stmt_node)
                        && let Some(ident) = arena.get_identifier_at(export.export_clause)
                        && ident.escaped_text == name
                    {
                        // Assign flags directly — declaration_symbol_flags cannot
                        // resolve the identifier to its NAMESPACE_EXPORT_DECLARATION
                        // parent because that kind isn't in resolve_duplicate_decl_node.
                        let flags = symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::ALIAS;
                        declarations.push((
                            export.export_clause,
                            flags,
                            false,
                            true,
                            DuplicateDeclarationOrigin::GlobalScopeConflict,
                        ));
                    }
                    continue;
                }

                // Check for `declare global { ... }` containing variable declarations
                if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    let is_global = (u32::from(stmt_node.flags) & node_flags::GLOBAL_AUGMENTATION)
                        != 0
                        || arena
                            .get_module(stmt_node)
                            .and_then(|m| arena.get(m.name))
                            .and_then(|n| arena.get_identifier(n))
                            .is_some_and(|ident| ident.escaped_text == "global");
                    if !is_global {
                        continue;
                    }
                    let Some(module) = arena.get_module(stmt_node) else {
                        continue;
                    };
                    // Scan the body for variable declarations matching `name`
                    self.scan_global_block_for_variable(
                        arena.as_ref(),
                        module.body,
                        name,
                        &mut declarations,
                    );
                }
            }
        }

        declarations
    }

    /// Scan a `declare global { ... }` block body for variable declarations
    /// with the given name. Uses `get_variable` for `VariableStatement` access
    /// and `get_variable_declaration` for individual declarations.
    fn scan_global_block_for_variable(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        body_idx: NodeIndex,
        name: &str,
        declarations: &mut Vec<(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)>,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(body_node) = arena.get(body_idx) else {
            return;
        };
        // The body of a ModuleDeclaration is a ModuleBlock
        let stmts = if body_node.kind == syntax_kind_ext::MODULE_BLOCK {
            if let Some(block) = arena.get_module_block(body_node)
                && let Some(ref stmts) = block.statements
            {
                &stmts.nodes[..]
            } else {
                return;
            }
        } else {
            return;
        };

        for &inner_stmt in stmts {
            let Some(inner_node) = arena.get(inner_stmt) else {
                continue;
            };
            if inner_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            let Some(var_data) = arena.get_variable(inner_node) else {
                continue;
            };
            for &decl_list_idx in &var_data.declarations.nodes {
                let Some(decl_list_node) = arena.get(decl_list_idx) else {
                    continue;
                };
                let Some(decl_list_data) = arena.get_variable(decl_list_node) else {
                    continue;
                };
                for &decl_idx in &decl_list_data.declarations.nodes {
                    let Some(decl_node) = arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                        continue;
                    }
                    let Some(var_decl) = arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    if let Some(ident) = arena.get_identifier_at(var_decl.name)
                        && ident.escaped_text == name
                        && let Some(flags) = self.declaration_symbol_flags(arena, decl_idx)
                    {
                        declarations.push((
                            decl_idx,
                            flags,
                            false,
                            false,
                            DuplicateDeclarationOrigin::GlobalScopeConflict,
                        ));
                    }
                }
            }
        }
    }

    /// Get the `NodeIndex` of the nearest enclosing block scope for a declaration.
    /// Returns the first Block, `CaseBlock`, `ForStatement`, etc. ancestor.
    /// Returns `NodeIndex::NONE` if the declaration is directly in a function/module scope.
    pub(super) fn get_enclosing_block_scope(&self, decl_idx: NodeIndex) -> NodeIndex {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = decl_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return NodeIndex::NONE;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return NodeIndex::NONE;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return NodeIndex::NONE;
            };
            match parent_node.kind {
                // Block-creating scopes - return this as the enclosing scope,
                // but only if the block is NOT a function body.
                syntax_kind_ext::BLOCK => {
                    // A function body's block is the function scope itself,
                    // not a block scope. Only blocks inside control flow
                    // (if/catch/try/for/etc.) create true block scopes.
                    if let Some(block_ext) = self.ctx.arena.get_extended(parent)
                        && let Some(grandparent_node) = self.ctx.arena.get(block_ext.parent)
                    {
                        match grandparent_node.kind {
                            syntax_kind_ext::FUNCTION_DECLARATION
                            | syntax_kind_ext::FUNCTION_EXPRESSION
                            | syntax_kind_ext::ARROW_FUNCTION
                            | syntax_kind_ext::METHOD_DECLARATION
                            | syntax_kind_ext::CONSTRUCTOR => {
                                // This block is a function body — not a block scope
                                return NodeIndex::NONE;
                            }
                            _ => return parent,
                        }
                    }
                    return parent;
                }
                syntax_kind_ext::CASE_BLOCK
                | syntax_kind_ext::FOR_STATEMENT
                | syntax_kind_ext::FOR_IN_STATEMENT
                | syntax_kind_ext::FOR_OF_STATEMENT => {
                    return parent;
                }
                // Function/module boundaries - no enclosing block scope
                syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::SOURCE_FILE => {
                    return NodeIndex::NONE;
                }
                _ => {}
            }
            current = parent;
        }
    }

    /// Check diagnostics specific to merged enum declarations.
    ///
    /// - TS2432: In an enum with multiple declarations, only one declaration can
    ///   omit an initializer for its first enum element.
    /// - TS2300: Duplicate enum member names across different enum declarations.
    pub(super) fn check_merged_enum_declaration_diagnostics(
        &mut self,
        declarations: &[(NodeIndex, u32)],
    ) {
        use crate::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;

        let enum_declarations: Vec<NodeIndex> = declarations
            .iter()
            .filter(|&(_decl_idx, flags)| (flags & symbol_flags::ENUM) != 0)
            .map(|(decl_idx, _flags)| *decl_idx)
            .collect();

        if enum_declarations.len() <= 1 {
            return;
        }

        let mut first_member_without_initializer = Vec::new();
        let mut first_member_by_name: FxHashMap<String, (NodeIndex, NodeIndex, bool)> =
            FxHashMap::default();

        for &enum_decl_idx in &enum_declarations {
            let Some(enum_decl_node) = self.ctx.arena.get(enum_decl_idx) else {
                continue;
            };
            let Some(enum_decl) = self.ctx.arena.get_enum(enum_decl_node) else {
                continue;
            };

            if let Some(&first_member_idx) = enum_decl.members.nodes.first()
                && let Some(first_member_node) = self.ctx.arena.get(first_member_idx)
                && let Some(first_member) = self.ctx.arena.get_enum_member(first_member_node)
                && first_member.initializer.is_none()
            {
                first_member_without_initializer.push(first_member_idx);
            }

            for &member_idx in &enum_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };
                let Some(member_name_node) = self.ctx.arena.get(member.name) else {
                    continue;
                };

                let member_name =
                    if let Some(ident) = self.ctx.arena.get_identifier(member_name_node) {
                        ident.escaped_text.clone()
                    } else if let Some(literal) = self.ctx.arena.get_literal(member_name_node) {
                        literal.text.clone()
                    } else {
                        continue;
                    };

                if let Some((first_member_idx, first_decl_idx, first_reported)) =
                    first_member_by_name.get_mut(&member_name)
                {
                    if *first_decl_idx != enum_decl_idx {
                        if !*first_reported {
                            let first_name_idx = self
                                .ctx
                                .arena
                                .get(*first_member_idx)
                                .and_then(|node| self.ctx.arena.get_enum_member(node))
                                .map(|member| member.name)
                                .unwrap_or(*first_member_idx);
                            self.error_at_node_msg(
                                first_name_idx,
                                diagnostic_codes::DUPLICATE_IDENTIFIER,
                                &[&member_name],
                            );
                            *first_reported = true;
                        }
                        self.error_at_node_msg(
                            member.name,
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                            &[&member_name],
                        );
                    }
                } else {
                    first_member_by_name
                        .insert(member_name.clone(), (member_idx, enum_decl_idx, false));
                }
            }
        }

        if first_member_without_initializer.len() > 1 {
            // The first declaration that omits an initializer is allowed;
            // only subsequent ones get TS2432.
            for &member_idx in &first_member_without_initializer[1..] {
                self.error_at_node_msg(
                    member_idx,
                    diagnostic_codes::IN_AN_ENUM_WITH_MULTIPLE_DECLARATIONS_ONLY_ONE_DECLARATION_CAN_OMIT_AN_INITIALIZ,
                    &[],
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CheckerState;
    use crate::context::{CheckerOptions, ScriptTarget};
    use crate::module_resolution::build_module_resolution_maps;
    use crate::query_boundaries::common::TypeInterner;
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    fn with_checker(
        files: &[(&str, &str)],
        entry_file: &str,
        f: impl FnOnce(&mut CheckerState<'_>, usize, usize),
    ) {
        let mut arenas = Vec::with_capacity(files.len());
        let mut binders = Vec::with_capacity(files.len());
        let mut roots = Vec::with_capacity(files.len());
        let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

        for (name, source) in files {
            let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
            let root = parser.parse_source_file();
            let mut binder = BinderState::new();
            binder.bind_source_file(parser.get_arena(), root);
            arenas.push(Arc::new(parser.get_arena().clone()));
            binders.push(Arc::new(binder));
            roots.push(root);
        }

        let entry_idx = file_names
            .iter()
            .position(|name| name == entry_file)
            .expect("entry file should exist");
        let index_idx = file_names
            .iter()
            .position(|name| name == "/index.d.ts")
            .expect("index.d.ts should exist");
        let a_idx = file_names
            .iter()
            .position(|name| name == "/a.d.ts")
            .expect("a.d.ts should exist");
        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

        let all_arenas = Arc::new(arenas);
        let all_binders = Arc::new(binders);
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            all_arenas[entry_idx].as_ref(),
            all_binders[entry_idx].as_ref(),
            &types,
            file_names[entry_idx].clone(),
            CheckerOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );

        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(entry_idx);
        checker
            .ctx
            .set_resolved_module_paths(Arc::new(resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules);
        checker.check_source_file(roots[entry_idx]);
        f(&mut checker, a_idx, index_idx);
    }

    #[test]
    fn module_augmentation_conflict_helper_sees_target_export_from_augmentation_file() {
        let files = [
            (
                "/a.d.ts",
                r#"
import "./index";
declare module "./index" {
    type Row2 = { a: string };
}
"#,
            ),
            (
                "/index.d.ts",
                r#"
export type { Row2 } from "./common";
"#,
            ),
            (
                "/common.d.ts",
                r#"
export interface Row2 { b: string }
"#,
            ),
        ];

        with_checker(&files, "/a.d.ts", |checker, _a_idx, index_idx| {
            let conflicts =
                checker.module_augmentation_conflict_declarations_for_current_file("Row2");

            assert!(
                !conflicts.is_empty(),
                "Expected the augmentation file to see the target export surface as a duplicate partner"
            );
            assert!(
                conflicts.iter().all(|(_, _, is_local, _, _)| !*is_local),
                "Expected augmentation conflicts to be recorded as remote declarations: {conflicts:#?}"
            );
            let index_arena = checker.ctx.get_arena_for_file(index_idx as u32);
            assert!(
                conflicts.iter().any(|(decl_idx, _, _, _, _)| {
                    index_arena.get(*decl_idx).is_some_and(|node| {
                        node.kind == tsz_parser::parser::syntax_kind_ext::EXPORT_SPECIFIER
                    })
                }),
                "Expected the duplicate partner to be the local export binding in index.d.ts: {conflicts:#?}"
            );
        });
    }

    #[test]
    fn module_augmentation_conflict_helper_sees_augmentation_from_target_file() {
        let files = [
            (
                "/a.d.ts",
                r#"
import "./index";
declare module "./index" {
    type Row2 = { a: string };
}
"#,
            ),
            (
                "/index.d.ts",
                r#"
export type { Row2 } from "./common";
"#,
            ),
            (
                "/common.d.ts",
                r#"
export interface Row2 { b: string }
"#,
            ),
        ];

        with_checker(&files, "/index.d.ts", |checker, a_idx, _index_idx| {
            let conflicts =
                checker.module_augmentation_conflict_declarations_for_current_file("Row2");

            assert!(
                !conflicts.is_empty(),
                "Expected the target file to see the augmentation declaration as a duplicate partner"
            );
            let a_arena = checker.ctx.get_arena_for_file(a_idx as u32);
            assert!(
                conflicts.iter().any(|(decl_idx, _, _, _, _)| {
                    a_arena.get(*decl_idx).is_some_and(|node| {
                        node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    })
                }),
                "Expected the duplicate partner to be the augmentation type alias in a.d.ts: {conflicts:#?}"
            );
        });
    }

    #[test]
    fn module_augmentation_conflict_helper_skips_importing_consumer_file() {
        let files = [
            (
                "/main.ts",
                r#"
import { Row2 } from "./index";
const x: Row2 = {};
"#,
            ),
            (
                "/a.d.ts",
                r#"
import "./index";
declare module "./index" {
    type Row2 = { a: string };
}
"#,
            ),
            (
                "/index.d.ts",
                r#"
export type { Row2 } from "./common";
"#,
            ),
            (
                "/common.d.ts",
                r#"
export interface Row2 { b: string }
"#,
            ),
        ];

        with_checker(&files, "/main.ts", |checker, _a_idx, _index_idx| {
            let conflicts =
                checker.module_augmentation_conflict_declarations_for_current_file("Row2");

            assert!(
                conflicts.is_empty(),
                "Importing consumers should not be treated as module augmentation duplicate partners: {conflicts:#?}"
            );
        });
    }

    #[test]
    fn importing_consumer_row2_alias_stays_local_to_main() {
        let files = [
            (
                "/main.ts",
                r#"
import { Row2 } from "./index";
const x: Row2 = {};
"#,
            ),
            (
                "/a.d.ts",
                r#"
import "./index";
declare module "./index" {
    type Row2 = { a: string };
}
"#,
            ),
            (
                "/index.d.ts",
                r#"
export type { Row2 } from "./common";
"#,
            ),
            (
                "/common.d.ts",
                r#"
export interface Row2 { b: string }
"#,
            ),
        ];

        with_checker(&files, "/main.ts", |checker, _a_idx, _index_idx| {
            let sym_id = checker
                .ctx
                .binder
                .file_locals
                .get("Row2")
                .expect("main import alias should exist");
            let symbol = checker
                .ctx
                .binder
                .get_symbol(sym_id)
                .expect("symbol should exist");

            let remote_decl_count = symbol
                .declarations
                .iter()
                .filter_map(|&decl_idx| {
                    checker
                        .ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                })
                .flat_map(|arenas| arenas.iter())
                .filter(|arena| !std::ptr::eq(arena.as_ref(), checker.ctx.arena))
                .count();

            assert_eq!(
                remote_decl_count, 0,
                "Imported consumer alias should not carry remote declarations: {symbol:#?}"
            );
        });
    }

    #[test]
    fn export_surface_declarations_follow_export_equals_members_to_real_interface_decls() {
        let files = [
            (
                "/a.d.ts",
                r#"
import * as e from "express";
declare module "express" {
    interface Request {
        id: number;
    }
}
"#,
            ),
            (
                "/index.d.ts",
                r#"
declare namespace Express {
    export interface Request { }
}

declare module "express" {
    function e(): e.Express;
    namespace e {
        interface Request extends Express.Request {
            get(name: string): string;
        }
        interface Express {
            createApplication(): Application;
        }
        interface Application {}
        export = e;
    }
}
"#,
            ),
        ];

        with_checker(&files, "/a.d.ts", |checker, _a_idx, index_idx| {
            let decls = checker.export_surface_declarations_in_file(index_idx, "Request");

            assert!(
                !decls.is_empty(),
                "Expected Request to resolve through export= surface to real declarations"
            );
            assert!(
                decls
                    .iter()
                    .any(|(_, flags, _)| (flags & tsz_binder::symbol_flags::INTERFACE) != 0),
                "Expected export surface to include interface flags, got: {decls:#?}"
            );
        });
    }
}
