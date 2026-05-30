//! Export-surface and script/global scope declaration helpers for duplicate
//! identifier checking.
//!
//! Extracted from `duplicate_identifiers_helpers.rs` to keep that module
//! under 2000 LOC. All methods here are `impl CheckerState` helpers called
//! from `check_duplicate_identifiers` or its sub-routines.

use super::duplicate_identifiers::DuplicateDeclarationOrigin;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(crate) fn export_surface_declarations_in_file(
        &self,
        file_idx: usize,
        name: &str,
    ) -> Vec<(NodeIndex, u32, bool)> {
        let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
            return self.commonjs_object_literal_export_declarations_in_file(file_idx, name);
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
                    self.ctx
                        .module_exports_for_module(binder, key)
                        .and_then(|exports| self.resolve_export_from_table(binder, exports, name))
                })
            })
            .or_else(|| {
                export_keys.iter().find_map(|key| {
                    binder
                        .resolve_import_with_reexports_type_only(key, name)
                        .map(|(resolved, _)| resolved)
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
            if let Some(exports) = self
                .ctx
                .module_exports_for_module(owner_binder, &owner_file_name)
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
                                let flags = self.normalize_export_surface_decl_flags(
                                    owner_binder,
                                    decl_arena.as_ref(),
                                    resolved_sym_id,
                                    decl_idx,
                                    flags,
                                );
                                let is_exported =
                                    self.is_declaration_exported(decl_arena.as_ref(), decl_idx);
                                declarations.push((decl_idx, flags, is_exported));
                            }
                        }
                    } else if seen.insert(decl_idx.0)
                        && let Some(flags) = self.declaration_symbol_flags(owner_arena, decl_idx)
                    {
                        let flags = self.normalize_export_surface_decl_flags(
                            owner_binder,
                            owner_arena,
                            resolved_sym_id,
                            decl_idx,
                            flags,
                        );
                        let is_exported = self.is_declaration_exported(owner_arena, decl_idx);
                        declarations.push((decl_idx, flags, is_exported));
                    }
                }

                if declarations.is_empty() {
                    return self
                        .commonjs_object_literal_export_declarations_in_file(file_idx, name);
                }
                return declarations;
            }

            return self.commonjs_object_literal_export_declarations_in_file(file_idx, name);
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
                        let flags = self.normalize_export_surface_decl_flags(
                            binder,
                            decl_arena.as_ref(),
                            sym_id,
                            decl_idx,
                            flags,
                        );
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
                let flags = self
                    .normalize_export_surface_decl_flags(binder, arena, sym_id, decl_idx, flags);
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
                self.ctx
                    .module_exports_for_module(binder, key)
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

        let alias_only = !declarations.is_empty()
            && declarations
                .iter()
                .all(|(_, flags, _)| (*flags & symbol_flags::ALIAS) != 0);
        if alias_only {
            let mut merged = declarations.clone();
            let push_unique =
                |decls: Vec<(NodeIndex, u32, bool)>, out: &mut Vec<(NodeIndex, u32, bool)>| {
                    for decl in decls {
                        if !out.contains(&decl) {
                            out.push(decl);
                        }
                    }
                };

            for &(decl_idx, _, _) in &declarations {
                if let Some(resolved) =
                    self.follow_reexport_specifier_to_source_declarations(file_idx, decl_idx)
                    && !resolved.is_empty()
                {
                    push_unique(resolved, &mut merged);
                }
            }

            let mut visited = FxHashSet::default();
            if let Some((resolved_sym_id, owner_idx)) =
                self.resolve_export_in_file(file_idx, name, &mut visited)
                && let Some(owner_binder) = self.ctx.get_binder_for_file(owner_idx)
            {
                let owner_arena = self.ctx.get_arena_for_file(owner_idx as u32);
                let mut resolved_decls = Vec::new();
                let mut resolved_seen = FxHashSet::default();
                self.push_export_surface_symbol_declarations_in_file(
                    owner_binder,
                    owner_arena,
                    resolved_sym_id,
                    Some(true),
                    &mut resolved_decls,
                    &mut resolved_seen,
                );
                if !resolved_decls.is_empty() {
                    push_unique(resolved_decls, &mut merged);
                }
            }

            return merged;
        }

        if declarations.is_empty() {
            return self.commonjs_object_literal_export_declarations_in_file(file_idx, name);
        }

        declarations
    }

    fn follow_reexport_specifier_to_source_declarations(
        &self,
        file_idx: usize,
        decl_idx: NodeIndex,
    ) -> Option<Vec<(NodeIndex, u32, bool)>> {
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let resolved_decl_idx = self.resolve_duplicate_decl_node(arena, decl_idx)?;
        let decl_node = arena.get(resolved_decl_idx)?;
        if decl_node.kind != syntax_kind_ext::EXPORT_SPECIFIER {
            return None;
        }
        let spec = arena.get_specifier(decl_node)?;
        let export_name_idx = if spec.property_name.is_some() {
            spec.property_name
        } else {
            spec.name
        };
        let export_name = arena
            .get(export_name_idx)
            .and_then(|node| arena.get_identifier(node))
            .map(|ident| ident.escaped_text.clone())?;

        let mut cursor = arena
            .get_extended(resolved_decl_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        while cursor.is_some() {
            let parent_node = arena.get(cursor)?;
            if parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                let export_decl = arena.get_export_decl(parent_node)?;
                if !export_decl.module_specifier.is_some() {
                    return None;
                }
                let module_node = arena.get(export_decl.module_specifier)?;
                let module_literal = arena.get_literal(module_node)?;
                let target_idx = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, &module_literal.text)?;
                if target_idx == file_idx {
                    return None;
                }
                return Some(self.export_surface_declarations_in_file(target_idx, &export_name));
            }
            cursor = arena
                .get_extended(cursor)
                .map(|ext| ext.parent)
                .unwrap_or(NodeIndex::NONE);
        }

        None
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
                        let flags = self.normalize_export_surface_decl_flags(
                            binder,
                            decl_arena.as_ref(),
                            sym_id,
                            decl_idx,
                            flags,
                        );
                        let is_exported = exported_override.unwrap_or_else(|| {
                            self.is_declaration_exported(decl_arena.as_ref(), decl_idx)
                        });
                        declarations.push((decl_idx, flags, is_exported));
                    }
                }
            } else if seen.insert(decl_idx.0)
                && let Some(flags) = self.declaration_symbol_flags(arena, decl_idx)
            {
                let flags = self
                    .normalize_export_surface_decl_flags(binder, arena, sym_id, decl_idx, flags);
                let is_exported = exported_override
                    .unwrap_or_else(|| self.is_declaration_exported(arena, decl_idx));
                declarations.push((decl_idx, flags, is_exported));
            }
        }
    }

    fn normalize_export_surface_decl_flags(
        &self,
        binder: &tsz_binder::BinderState,
        arena: &tsz_parser::parser::node::NodeArena,
        sym_id: tsz_binder::SymbolId,
        decl_idx: NodeIndex,
        flags: u32,
    ) -> u32 {
        if (flags & symbol_flags::ALIAS) == 0 {
            return flags;
        }
        let Some(resolved_decl_idx) = self.resolve_duplicate_decl_node(arena, decl_idx) else {
            return flags;
        };
        let Some(node) = arena.get(resolved_decl_idx) else {
            return flags;
        };
        if node.kind != syntax_kind_ext::EXPORT_SPECIFIER
            && node.kind != syntax_kind_ext::IMPORT_SPECIFIER
        {
            return flags;
        }

        // Export-surface checks should compare against the underlying declaration
        // kind, not the alias wrapper node. This matters for `export { A }`
        // where local `A` merges an imported type alias with a local value.
        if node.kind == syntax_kind_ext::IMPORT_SPECIFIER
            && let Some(symbol) = binder.get_symbol(sym_id)
            && let (Some(module_name), Some(export_name), Some(source_file_idx)) = (
                symbol.import_module.as_deref(),
                symbol.import_name.as_deref(),
                self.ctx.get_file_idx_for_arena(arena),
            )
            && let Some(target_sym_id) = self.resolve_cross_file_export_from_file(
                module_name,
                export_name,
                Some(source_file_idx),
            )
        {
            return self
                .get_cross_file_symbol(target_sym_id)
                .or_else(|| binder.get_symbol(target_sym_id))
                .map_or(flags, |sym| sym.flags);
        }

        let resolved_sym_id = self
            .resolve_alias_symbol(sym_id, &mut AliasCycleTracker::new())
            .unwrap_or(sym_id);
        self.ctx
            .binder
            .get_symbol(resolved_sym_id)
            .or_else(|| binder.get_symbol(resolved_sym_id))
            .map_or(flags, |sym| sym.flags)
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

    pub(super) fn symbol_is_current_file_top_level_script_declaration(
        &self,
        name: &str,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        use tsz_binder::ContainerKind;

        if self.ctx.binder.is_external_module() {
            return false;
        }

        if let Some(root_scope) = self.ctx.binder.scopes.first()
            && root_scope.kind == ContainerKind::SourceFile
        {
            return root_scope.table.get(name).is_some_and(|id| id == sym_id);
        }

        self.ctx
            .binder
            .file_locals
            .get(name)
            .is_some_and(|id| id == sym_id)
    }

    /// Detect conflicts between a global script's block-scoped variable declaration
    /// and same-named top-level declarations in module (external) files.
    ///
    /// In tsc, when a script file declares `let`/`const` in the global scope and a
    /// module file also has a top-level declaration with the same name, tsc reports
    /// TS2451 ("Cannot redeclare block-scoped variable") on the global declaration.
    /// This covers cases like:
    /// - `types/lib/index.d.ts`: `declare let $` (global) vs `app.ts`: `export let $` (module)
    /// - `@types/node/index.d.ts`: `declare const require` (global) vs module files using CommonJS
    pub(super) fn module_file_block_scoped_conflict_declarations_for_current_file(
        &self,
        name: &str,
        sym_flags: u32,
    ) -> Vec<(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)> {
        // Only applies when the current file is a script (non-module)
        if self.ctx.binder.is_external_module() {
            return Vec::new();
        }
        // Only check for block-scoped variables (let/const)
        if (sym_flags & symbol_flags::BLOCK_SCOPED_VARIABLE) == 0 {
            return Vec::new();
        }

        // Only applies to type declaration files (.d.ts). In tsc, global
        // block-scoped declarations from type root / @types .d.ts files
        // conflict with same-named module declarations. Regular .ts/.js
        // script files' block-scoped variables do NOT conflict with module
        // declarations because they are user code, not ambient type definitions.
        if !self.ctx.is_declaration_file() {
            return Vec::new();
        }

        let Some(all_arenas) = self.ctx.all_arenas.as_ref() else {
            return Vec::new();
        };

        let mut declarations = Vec::new();

        for (file_idx, arena) in all_arenas.iter().enumerate() {
            if file_idx == self.ctx.current_file_idx {
                continue;
            }
            let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
                continue;
            };
            // Only check module files (script files are handled by
            // same_name_top_level_script_declarations_for_current_file)
            if !binder.is_external_module() {
                continue;
            }

            let Some(source_file) = arena.source_files.first() else {
                continue;
            };

            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = arena.get(stmt_idx) else {
                    continue;
                };

                // Unwrap export declarations
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
                    syntax_kind_ext::VARIABLE_STATEMENT => {
                        // Check variable declarations inside the statement
                        if let Some(var_stmt) = arena.get_variable(decl_node) {
                            self.variable_statement_contains_name(
                                arena,
                                &var_stmt.declarations,
                                name,
                            )
                        } else {
                            false
                        }
                    }
                    syntax_kind_ext::FUNCTION_DECLARATION => arena
                        .get_function(decl_node)
                        .and_then(|decl| arena.get_identifier_at(decl.name))
                        .is_some_and(|ident| ident.escaped_text == name),
                    syntax_kind_ext::CLASS_DECLARATION => arena
                        .get_class(decl_node)
                        .and_then(|decl| arena.get_identifier_at(decl.name))
                        .is_some_and(|ident| ident.escaped_text == name),
                    _ => false,
                };

                if !matches_name {
                    continue;
                }

                // For variable statements, find the specific declaration with the name
                if decl_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                    if let Some(var_stmt) = arena.get_variable(decl_node) {
                        for &dl_idx in &var_stmt.declarations.nodes {
                            let Some(dl_node) = arena.get(dl_idx) else {
                                continue;
                            };
                            let Some(dl_data) = arena.get_variable(dl_node) else {
                                continue;
                            };
                            for &vd_idx in &dl_data.declarations.nodes {
                                let Some(vd_node) = arena.get(vd_idx) else {
                                    continue;
                                };
                                let Some(var_decl) = arena.get_variable_declaration(vd_node) else {
                                    continue;
                                };
                                if let Some(ident) = arena.get_identifier_at(var_decl.name)
                                    && ident.escaped_text == name
                                    && let Some(flags) =
                                        self.declaration_symbol_flags(arena, vd_idx)
                                {
                                    let is_exported = self.is_declaration_exported(arena, stmt_idx);
                                    declarations.push((
                                        vd_idx,
                                        flags,
                                        false,
                                        is_exported,
                                        DuplicateDeclarationOrigin::SymbolDeclaration,
                                    ));
                                }
                            }
                        }
                    }
                } else {
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
            }
        }

        declarations
    }

    /// Check if a variable statement (or declaration list) contains a declaration
    /// with the given name.
    fn variable_statement_contains_name(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_list: &tsz_parser::parser::NodeList,
        name: &str,
    ) -> bool {
        for &dl_idx in &decl_list.nodes {
            let Some(dl_node) = arena.get(dl_idx) else {
                continue;
            };
            if let Some(dl_data) = arena.get_variable(dl_node) {
                for &vd_idx in &dl_data.declarations.nodes {
                    let Some(vd_node) = arena.get(vd_idx) else {
                        continue;
                    };
                    if let Some(var_decl) = arena.get_variable_declaration(vd_node)
                        && arena
                            .get_identifier_at(var_decl.name)
                            .is_some_and(|ident| ident.escaped_text == name)
                    {
                        return true;
                    }
                }
            } else if let Some(var_decl) = arena.get_variable_declaration(dl_node)
                && arena
                    .get_identifier_at(var_decl.name)
                    .is_some_and(|ident| ident.escaped_text == name)
            {
                return true;
            }
        }
        false
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
                let is_global_augmentation = parent.is_global_augmentation()
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

            // Handle variable declarations (let/const/var) — these are wrapped in
            // VariableStatement → VariableDeclarationList → VariableDeclaration.
            if decl_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                if let Some(var_data) = arena.get_variable(decl_node) {
                    for &decl_list_idx in &var_data.declarations.nodes {
                        let Some(decl_list_node) = arena.get(decl_list_idx) else {
                            continue;
                        };
                        let Some(decl_list_data) = arena.get_variable(decl_list_node) else {
                            continue;
                        };
                        for &var_decl_idx in &decl_list_data.declarations.nodes {
                            let Some(var_decl_node) = arena.get(var_decl_idx) else {
                                continue;
                            };
                            if var_decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                                continue;
                            }
                            let Some(var_decl) = arena.get_variable_declaration(var_decl_node)
                            else {
                                continue;
                            };
                            if let Some(ident) = arena.get_identifier_at(var_decl.name)
                                && ident.escaped_text == name
                                && let Some(flags) =
                                    self.declaration_symbol_flags(arena, var_decl_idx)
                            {
                                let is_exported = self.is_declaration_exported(arena, var_decl_idx);
                                declarations.push((
                                    var_decl_idx,
                                    flags,
                                    false,
                                    is_exported,
                                    DuplicateDeclarationOrigin::SymbolDeclaration,
                                ));
                            }
                        }
                    }
                }
                continue;
            }

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
                    let is_global = stmt_node.is_global_augmentation()
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

    pub(super) fn default_import_alias_conflict_declarations_for_current_file(
        &self,
        name: &str,
    ) -> Vec<(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)> {
        // External modules have their own scope — default imports in other files
        // cannot create naming conflicts with declarations in the current module.
        // This check only matters for script files (e.g., ambient .d.ts without
        // module-level imports/exports) where declarations are global.
        if self.ctx.binder.is_external_module() {
            return Vec::new();
        }

        let Some(all_arenas) = self.ctx.all_arenas.as_ref() else {
            return Vec::new();
        };
        if !self.default_export_identifier_named_requires_alias_conflict(name) {
            return Vec::new();
        }

        let mut declarations = Vec::new();
        let mut seen = FxHashSet::default();
        let current_file_name = self
            .ctx
            .arena
            .source_files
            .first()
            .map(|sf| sf.file_name.replace('\\', "/"))
            .unwrap_or_default();

        let module_spec_targets_current_file = |module_spec: &str| {
            let module_spec = module_spec.replace('\\', "/");
            let mut module_candidates = vec![module_spec.clone()];
            if !module_spec.starts_with("@types/") {
                if let Some(scoped) = module_spec.strip_prefix('@') {
                    let mut parts = scoped.split('/');
                    if let (Some(scope), Some(package), None) =
                        (parts.next(), parts.next(), parts.next())
                        && !scope.is_empty()
                        && !package.is_empty()
                    {
                        module_candidates.push(format!("@types/{scope}__{package}"));
                    }
                } else if !module_spec.is_empty() {
                    module_candidates.push(format!("@types/{module_spec}"));
                }
            }

            module_candidates.into_iter().any(|candidate| {
                current_file_name.ends_with(&format!("/node_modules/{candidate}/index.d.ts"))
                    || current_file_name
                        .ends_with(&format!("/node_modules/{candidate}/index.d.mts"))
                    || current_file_name
                        .ends_with(&format!("/node_modules/{candidate}/index.d.cts"))
                    || current_file_name.ends_with(&format!("/node_modules/{candidate}.d.ts"))
                    || current_file_name.ends_with(&format!("/node_modules/{candidate}.d.mts"))
                    || current_file_name.ends_with(&format!("/node_modules/{candidate}.d.cts"))
            })
        };

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
                if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                    continue;
                }
                let Some(import_decl) = arena.get_import_decl(stmt_node) else {
                    continue;
                };
                if import_decl.import_clause.is_none() {
                    continue;
                }
                let Some(clause_node) = arena.get(import_decl.import_clause) else {
                    continue;
                };
                let Some(clause) = arena.get_import_clause(clause_node) else {
                    continue;
                };
                if clause.name.is_none() {
                    continue;
                }
                let Some(default_ident) = arena.get_identifier_at(clause.name) else {
                    continue;
                };
                if default_ident.escaped_text != name {
                    continue;
                }
                let Some(module_node) = arena.get(import_decl.module_specifier) else {
                    continue;
                };
                let Some(module_lit) = arena.get_literal(module_node) else {
                    continue;
                };
                let targets_current_file = if let Some(target_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, &module_lit.text)
                {
                    target_idx == self.ctx.current_file_idx
                } else {
                    module_spec_targets_current_file(&module_lit.text)
                };
                if !targets_current_file {
                    continue;
                }

                let decl_idx = clause.name;
                if !seen.insert((file_idx, decl_idx.0)) {
                    continue;
                }
                declarations.push((
                    decl_idx,
                    symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::ALIAS,
                    false,
                    true,
                    DuplicateDeclarationOrigin::GlobalScopeConflict,
                ));
            }
        }

        declarations
    }
}
