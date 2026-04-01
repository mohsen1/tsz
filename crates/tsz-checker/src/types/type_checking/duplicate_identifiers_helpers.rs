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
        let Some(_arenas) = self.ctx.all_arenas.as_ref() else {
            return Vec::new();
        };

        let mut declarations = Vec::new();

        for (module_spec, augmentations) in &self.ctx.binder.module_augmentations {
            for augmentation in augmentations {
                if augmentation.name != name {
                    continue;
                }

                let Some(arena) = augmentation.arena.as_deref() else {
                    continue;
                };
                let Some(source_file_idx) = self.ctx.get_file_idx_for_arena(arena) else {
                    continue;
                };
                if !self.module_augmentation_targets_current_file_export(
                    source_file_idx,
                    module_spec,
                    name,
                )
                {
                    continue;
                }

                let Some(flags) = self.declaration_symbol_flags(arena, augmentation.node) else {
                    continue;
                };
                let is_exported = self.is_declaration_exported(arena, augmentation.node);
                declarations.push((
                    augmentation.node,
                    flags,
                    false,
                    is_exported,
                    DuplicateDeclarationOrigin::TargetedModuleAugmentation,
                ));
            }
        }

        declarations
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
