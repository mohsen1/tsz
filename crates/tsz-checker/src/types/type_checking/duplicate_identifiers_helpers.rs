//! Helper methods for duplicate identifier checking.
//!
//! Extracted from `duplicate_identifiers.rs` to keep that module under 2000 LOC.
//! All methods here are `impl CheckerState` helpers called from
//! `check_duplicate_identifiers` or its sub-routines.

use super::duplicate_identifiers::{DuplicateDeclarationOrigin, OuterDeclResult};
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use rustc_hash::FxHashSet;
use tsz_binder::{ContainerKind, SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(super) fn check_global_augmentation_const_enum_rebind_diagnostics(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if !self.ctx.binder.is_external_module() {
            return;
        }

        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return;
        };
        let statements: Vec<NodeIndex> = source_file.statements.nodes.clone();

        for stmt_idx in statements {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            if !stmt_node.is_global_augmentation()
                && self
                    .ctx
                    .arena
                    .get_module(stmt_node)
                    .and_then(|module| self.ctx.arena.get(module.name))
                    .and_then(|name| self.ctx.arena.get_identifier(name))
                    .is_none_or(|ident| ident.escaped_text != "global")
            {
                continue;
            }

            let Some(module) = self.ctx.arena.get_module(stmt_node) else {
                continue;
            };
            let Some(body_node) = self.ctx.arena.get(module.body) else {
                continue;
            };
            if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
                continue;
            }
            let Some(block) = self.ctx.arena.get_module_block(body_node) else {
                continue;
            };
            let Some(statements) = block.statements.as_ref() else {
                continue;
            };
            let inner_statements: Vec<NodeIndex> = statements.nodes.clone();

            for enum_decl_idx in inner_statements {
                let Some(enum_node) = self.ctx.arena.get(enum_decl_idx) else {
                    continue;
                };
                if enum_node.kind != syntax_kind_ext::ENUM_DECLARATION {
                    continue;
                }
                let Some(enum_decl) = self.ctx.arena.get_enum(enum_node) else {
                    continue;
                };
                if !self
                    .ctx
                    .arena
                    .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
                {
                    continue;
                }

                for &member_idx in &enum_decl.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                        continue;
                    };
                    let Some(member_name) = self.ctx.arena.get(member.name).and_then(|name_node| {
                        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                            Some(ident.escaped_text.clone())
                        } else {
                            self.ctx
                                .arena
                                .get_literal(name_node)
                                .map(|literal| literal.text.clone())
                        }
                    }) else {
                        continue;
                    };
                    self.error_at_node_msg(
                        member.name,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                        &[&member_name],
                    );
                }

                if let Some(&first_member_idx) = enum_decl.members.nodes.first()
                    && let Some(first_member_node) = self.ctx.arena.get(first_member_idx)
                    && let Some(first_member) = self.ctx.arena.get_enum_member(first_member_node)
                    && first_member.initializer.is_none()
                {
                    self.error_at_node(
                        first_member.name,
                        diagnostic_messages::IN_AN_ENUM_WITH_MULTIPLE_DECLARATIONS_ONLY_ONE_DECLARATION_CAN_OMIT_AN_INITIALIZ,
                        diagnostic_codes::IN_AN_ENUM_WITH_MULTIPLE_DECLARATIONS_ONLY_ONE_DECLARATION_CAN_OMIT_AN_INITIALIZ,
                    );
                }
            }
        }
    }

    pub(super) fn extend_duplicate_symbol_ids_with_local_augmentation_decls(
        &self,
        symbol_ids: &mut FxHashSet<tsz_binder::SymbolId>,
    ) {
        for augmentations in self.ctx.binder.module_augmentations.values() {
            for augmentation in augmentations {
                let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
                if !std::ptr::eq(arena, self.ctx.arena) {
                    continue;
                }
                if let Some(sym_id) = self.local_augmentation_decl_symbol_id(augmentation.node) {
                    symbol_ids.insert(sym_id);
                }
            }
        }

        for augmentations in self.ctx.binder.global_augmentations.values() {
            for augmentation in augmentations {
                let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
                if !std::ptr::eq(arena, self.ctx.arena) {
                    continue;
                }
                if let Some(sym_id) = self.local_augmentation_decl_symbol_id(augmentation.node) {
                    symbol_ids.insert(sym_id);
                }
            }
        }
    }

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

    /// Check if a declaration is inside a `declare namespace` (identifier-named)
    /// but NOT inside a `declare module "..."` (string-literal-named).
    ///
    /// This distinction matters for TS2395: tsc suppresses the "individual
    /// declarations must be all exported or all local" check inside pure ambient
    /// namespaces but still emits it inside ambient module declarations.
    pub(super) fn is_in_ambient_namespace_not_module(&self, decl_idx: NodeIndex) -> bool {
        // Interfaces and type aliases are implicitly "ambient" (no runtime code),
        // but for TS2395 they should NOT be treated as "in ambient namespace" when
        // they are at module scope. We need to check if the declaration is ACTUALLY
        // inside a `declare namespace` (not just implicitly ambient due to its kind).
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        let is_implicitly_ambient = decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
            || decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION;
        if !is_implicitly_ambient && !self.ctx.arena.is_in_ambient_context(decl_idx) {
            return false;
        }
        // Walk up to find an enclosing `declare namespace` (identifier-named).
        // If we only find SOURCE_FILE or `declare module "..."`, the declaration
        // is NOT in a pure ambient namespace.
        let mut current = decl_idx;
        let mut found_ambient_namespace = false;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                // Check if the module name is a string literal
                if let Some(module_data) = self.ctx.arena.get_module(parent_node)
                    && let Some(name_node) = self.ctx.arena.get(module_data.name)
                    && name_node.is_string_literal()
                {
                    // declare module "..." -- NOT a pure namespace
                    return false;
                }
                // Found an identifier-named module declaration (namespace).
                // Only count it as "ambient" if it has the `declare` keyword or
                // is in an ambient context (.d.ts / global declare block).
                if self.ctx.arena.is_in_ambient_context(parent) {
                    found_ambient_namespace = true;
                }
            }
            if parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                break;
            }
            current = parent;
        }
        // For implicitly ambient declarations (interfaces/type aliases),
        // only return true if actually inside an ambient namespace.
        // For explicitly ambient declarations (.d.ts, declare keyword),
        // reaching the source file without finding a string-literal module
        // means we're in ambient/namespace context.
        if is_implicitly_ambient {
            found_ambient_namespace
        } else {
            true
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
            |file_idx: usize,
             decl_idx: NodeIndex,
             flags: u32,
             is_exported: bool,
             origin: DuplicateDeclarationOrigin| {
                if seen.insert((file_idx, decl_idx.0)) {
                    declarations.push((decl_idx, flags, false, is_exported, origin));
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
                    push_remote_decl(
                        augmenting_file_idx,
                        augmentation.node,
                        flags,
                        is_exported,
                        DuplicateDeclarationOrigin::TargetedModuleAugmentation,
                    );
                    return;
                }

                if augmenting_file_idx == self.ctx.current_file_idx {
                    for (decl_idx, flags, is_exported) in
                        self.export_surface_declarations_in_file(target_idx, name)
                    {
                        push_remote_decl(
                            target_idx,
                            decl_idx,
                            flags,
                            is_exported,
                            DuplicateDeclarationOrigin::CurrentFileAugmentationTargetExport(
                                target_idx,
                            ),
                        );
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

        for (module_spec, augmentations) in self.ctx.binder.module_augmentations.iter() {
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
                for (module_spec, augmentations) in binder.module_augmentations.iter() {
                    for augmentation in augmentations {
                        consider_augmentation(module_spec, augmenting_file_idx, augmentation);
                    }
                }
            }
        }

        declarations
    }

    pub(super) fn normalize_duplicate_conflict_flags(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: NodeIndex,
        flags: u32,
    ) -> u32 {
        let Some(resolved_decl_idx) = self.resolve_duplicate_decl_node(arena, decl_idx) else {
            return flags;
        };
        let Some(node) = arena.get(resolved_decl_idx) else {
            return flags;
        };
        if node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return flags;
        }
        // Import-equals aliases can carry namespace flags from their targets.
        // For duplicate-name checks, they should still participate as aliases.
        (flags | symbol_flags::ALIAS)
            & !(symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
    }

    pub(super) fn default_export_identifier_named_requires_alias_conflict(
        &self,
        name: &str,
    ) -> bool {
        if self
            .current_file_default_export_identifier_named(name)
            .is_none()
        {
            return false;
        }

        let has_ambient_value_default_export =
            self.ambient_external_module_default_export_has_value_sibling_named(name);

        let sym_id = self.ctx.binder.file_locals.get(name).or_else(|| {
            self.ctx
                .binder
                .module_exports
                .values()
                .find_map(|exports| exports.get(name))
        });
        let Some(sym_id) = sym_id else {
            return has_ambient_value_default_export;
        };

        if self.symbol_is_type_only(sym_id, Some(name))
            || self.alias_resolves_to_uninstantiated_namespace(sym_id)
        {
            return true;
        }

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let concrete_value = symbol_flags::VARIABLE
            | symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::ENUM;
        if symbol.has_any_flags(concrete_value) {
            return has_ambient_value_default_export;
        }

        (symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0
            && !self.symbol_has_runtime_value_in_binder(self.ctx.binder, sym_id)
    }

    pub(super) fn current_file_default_export_identifier_named(
        &self,
        name: &str,
    ) -> Option<NodeIndex> {
        let source_file = self.ctx.arena.source_files.first()?;
        for &stmt_idx in &source_file.statements.nodes {
            if let Some(node) =
                self.find_default_export_identifier_named_in_statement(stmt_idx, name, 0)
            {
                return Some(node);
            }
        }
        None
    }

    fn find_default_export_identifier_named_in_statement(
        &self,
        stmt_idx: NodeIndex,
        name: &str,
        depth: u8,
    ) -> Option<NodeIndex> {
        if depth > 12 {
            return None;
        }
        let stmt_node = self.ctx.arena.get(stmt_idx)?;

        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let export_decl = self.ctx.arena.get_export_decl(stmt_node)?;
            if !export_decl.is_default_export {
                return None;
            }
            return self
                .ctx
                .arena
                .get_identifier_at(export_decl.export_clause)
                .is_some_and(|ident| ident.escaped_text == name)
                .then_some(export_decl.export_clause);
        }

        if stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
            let export_assign = self.ctx.arena.get_export_assignment(stmt_node)?;
            if export_assign.is_export_equals {
                return None;
            }
            return self
                .ctx
                .arena
                .get_identifier_at(export_assign.expression)
                .is_some_and(|ident| ident.escaped_text == name)
                .then_some(export_assign.expression);
        }

        if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            let module_decl = self.ctx.arena.get_module(stmt_node)?;
            let body_node = self.ctx.arena.get(module_decl.body)?;
            if body_node.kind == syntax_kind_ext::MODULE_BLOCK {
                let block = self.ctx.arena.get_module_block(body_node)?;
                let statements = block.statements.as_ref()?;
                for &inner_idx in &statements.nodes {
                    if let Some(found) = self.find_default_export_identifier_named_in_statement(
                        inner_idx,
                        name,
                        depth + 1,
                    ) {
                        return Some(found);
                    }
                }
                return None;
            }
            if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                return self.find_default_export_identifier_named_in_statement(
                    module_decl.body,
                    name,
                    depth + 1,
                );
            }
        }

        None
    }

    pub(super) fn jsx_runtime_conflict_declarations_for_current_file(
        &mut self,
        name: &str,
    ) -> Vec<(NodeIndex, u32, bool, bool, DuplicateDeclarationOrigin)> {
        use tsz_common::checker_options::JsxMode;

        if name != "JSX" {
            return Vec::new();
        }

        let effective_mode = self.effective_jsx_mode();
        let pragma_source = self.extract_jsx_import_source_pragma();
        let uses_automatic_runtime =
            matches!(effective_mode, JsxMode::ReactJsx | JsxMode::ReactJsxDev)
                || pragma_source.is_some()
                || !self.ctx.compiler_options.jsx_import_source.is_empty();
        if !uses_automatic_runtime {
            return Vec::new();
        }

        let Some(local_alias_decl_idx) = self.first_current_file_global_import_equals_named(name)
        else {
            return Vec::new();
        };

        let source = if let Some(pragma) = pragma_source {
            pragma
        } else if self.ctx.compiler_options.jsx_import_source.is_empty() {
            "react".to_string()
        } else {
            self.ctx.compiler_options.jsx_import_source.clone()
        };
        let runtime_suffix = if effective_mode == JsxMode::ReactJsxDev {
            "jsx-dev-runtime"
        } else {
            "jsx-runtime"
        };
        let runtime_module = format!("{source}/{runtime_suffix}");

        let jsx_sym_id = self
            .resolve_cross_file_export_from_file(
                &runtime_module,
                "JSX",
                Some(self.ctx.current_file_idx),
            )
            .or_else(|| self.resolve_jsx_runtime_export_fallback(&runtime_module))
            .or_else(|| self.resolve_jsx_namespace_from_factory());
        let remote_decl_idx = jsx_sym_id
            .map(|sym_id| {
                let resolved_sym_id = self
                    .resolve_alias_symbol(sym_id, &mut AliasCycleTracker::new())
                    .unwrap_or(sym_id);
                self.get_cross_file_symbol(resolved_sym_id)
                    .and_then(|sym| sym.declarations.first().copied())
                    .or_else(|| {
                        let lib_binders = self.get_lib_binders();
                        self.ctx
                            .binder
                            .get_symbol_with_libs(resolved_sym_id, &lib_binders)
                            .and_then(|sym| sym.declarations.first().copied())
                    })
                    .unwrap_or(local_alias_decl_idx)
            })
            .unwrap_or(local_alias_decl_idx);

        vec![(
            remote_decl_idx,
            symbol_flags::ALIAS,
            false,
            false,
            DuplicateDeclarationOrigin::GlobalScopeConflict,
        )]
    }

    fn first_current_file_global_import_equals_named(&self, name: &str) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let source_file = self.ctx.arena.source_files.first()?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }
            let Some(module_decl) = self.ctx.arena.get_module(stmt_node) else {
                continue;
            };
            let is_global_augmentation = stmt_node.is_global_augmentation()
                || self
                    .ctx
                    .arena
                    .get(module_decl.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_some_and(|ident| ident.escaped_text == "global");
            if !is_global_augmentation {
                continue;
            }
            let Some(body_node) = self.ctx.arena.get(module_decl.body) else {
                continue;
            };
            if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
                continue;
            }
            let Some(block) = self.ctx.arena.get_module_block(body_node) else {
                continue;
            };
            let Some(statements) = &block.statements else {
                continue;
            };

            for &inner_idx in &statements.nodes {
                let Some(inner_node) = self.ctx.arena.get(inner_idx) else {
                    continue;
                };
                let decl_idx = if inner_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                    let Some(export_decl) = self.ctx.arena.get_export_decl(inner_node) else {
                        continue;
                    };
                    export_decl.export_clause
                } else {
                    inner_idx
                };
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                    continue;
                }
                let Some(import_eq) = self.ctx.arena.get_import_decl(decl_node) else {
                    continue;
                };
                if self
                    .ctx
                    .arena
                    .get_identifier_at(import_eq.import_clause)
                    .is_some_and(|ident| ident.escaped_text == name)
                {
                    return Some(decl_idx);
                }
            }
        }

        None
    }

    /// Scan a `declare global { ... }` block body for variable declarations
    /// with the given name. Uses `get_variable` for `VariableStatement` access
    /// and `get_variable_declaration` for individual declarations.
    pub(super) fn scan_global_block_for_variable(
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
        use rustc_hash::{FxHashMap, FxHashSet};

        // Enum declarations of this symbol, paired with their const-ness, in
        // source (binding) order. Order matters: tsc's binder keeps the first
        // declaration's const-ness as the merged symbol's kind and splits every
        // later declaration that disagrees into its own symbol.
        let mut enum_declarations: Vec<(NodeIndex, bool)> = declarations
            .iter()
            .filter(|&(_decl_idx, flags)| (flags & symbol_flags::ENUM) != 0)
            .map(|&(decl_idx, flags)| (decl_idx, (flags & symbol_flags::CONST_ENUM) != 0))
            .collect();

        if enum_declarations.len() <= 1 {
            return;
        }

        enum_declarations
            .sort_by_key(|&(decl_idx, _)| self.ctx.arena.get(decl_idx).map_or(0, |node| node.pos));

        // TS2567: a `const enum` and a non-const `enum` of the same name cannot
        // merge. The merged symbol takes the first declaration's const-ness (the
        // "primary" group); any later declaration whose const-ness differs is
        // reported together with every primary declaration bound before it
        // (matching tsc's binder, which splits the conflicting declaration into a
        // fresh symbol at the point of conflict). Only the primary group actually
        // merges, so the initializer / member-duplicate checks below run over it
        // rather than over the split-off declarations.
        let primary_is_const = enum_declarations[0].1;
        let mut primary_group: Vec<NodeIndex> = Vec::new();
        let mut reported: FxHashSet<NodeIndex> = FxHashSet::default();
        for &(decl_idx, is_const) in &enum_declarations {
            if is_const == primary_is_const {
                primary_group.push(decl_idx);
                continue;
            }
            // const-ness conflict: report it together with every primary
            // declaration bound before it (output diagnostics are sorted by
            // position, so emission order here is irrelevant).
            for &target in primary_group.iter().chain(std::iter::once(&decl_idx)) {
                if reported.insert(target)
                    && let Some(name_idx) = self
                        .ctx
                        .arena
                        .get(target)
                        .and_then(|node| self.ctx.arena.get_enum(node))
                        .map(|enum_decl| enum_decl.name)
                {
                    self.error_at_node_msg(
                        name_idx,
                        diagnostic_codes::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
                        &[],
                    );
                }
            }
        }

        let mut first_member_without_initializer = Vec::new();
        let mut first_member_by_name: FxHashMap<String, (NodeIndex, NodeIndex, bool)> =
            FxHashMap::default();

        for &enum_decl_idx in &primary_group {
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

    /// TS2300 for an enum member whose name collides with an export of a
    /// namespace it merges with.
    ///
    /// When an enum and a same-named namespace merge (a valid merge) and the
    /// enum declares member `A` while the namespace `export`s `A`, both `A`s
    /// occupy one slot in the merged symbol's export table and `tsc` reports
    /// "Duplicate identifier" on each. The collision rule is the binder's own
    /// exclusion set: an enum member excludes everything in `VALUE | TYPE`
    /// (`ENUM_MEMBER_EXCLUDES`), so any *exported* namespace member that carries
    /// a value or type meaning conflicts (`const`/`var`, `function`, `class`,
    /// nested namespace, `type`, `interface`). Non-exported namespace locals
    /// live in a different table and never collide, and distinct names merge
    /// cleanly.
    pub(super) fn check_enum_namespace_export_collisions(
        &mut self,
        declarations: &[(NodeIndex, u32)],
    ) {
        use crate::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;

        // One pass over the merged symbol's declarations: collect the namespace
        // blocks and map each enum member name to its name node.
        let mut module_decls: Vec<NodeIndex> = Vec::new();
        let mut enum_member_name_nodes: FxHashMap<String, NodeIndex> = FxHashMap::default();
        for &(decl_idx, flags) in declarations {
            if (flags & symbol_flags::MODULE) != 0 {
                module_decls.push(decl_idx);
                continue;
            }
            if (flags & symbol_flags::ENUM) == 0 {
                continue;
            }
            let Some(enum_decl) = self
                .ctx
                .arena
                .get(decl_idx)
                .and_then(|node| self.ctx.arena.get_enum(node))
            else {
                continue;
            };
            for &member_idx in &enum_decl.members.nodes {
                let Some(name_node) = self
                    .ctx
                    .arena
                    .get(member_idx)
                    .and_then(|node| self.ctx.arena.get_enum_member(node))
                    .map(|member| member.name)
                else {
                    continue;
                };
                let Some(name_data) = self.ctx.arena.get(name_node) else {
                    continue;
                };
                let name = if let Some(ident) = self.ctx.arena.get_identifier(name_data) {
                    ident.escaped_text.clone()
                } else if let Some(literal) = self.ctx.arena.get_literal(name_data) {
                    literal.text.clone()
                } else {
                    continue;
                };
                enum_member_name_nodes.entry(name).or_insert(name_node);
            }
        }
        if module_decls.is_empty() || enum_member_name_nodes.is_empty() {
            return;
        }

        for module_decl_idx in module_decls {
            // Snapshot only the namespace exports whose name matches an enum
            // member, so the `scopes` borrow is released before we emit.
            let candidates: Vec<(String, SymbolId)> =
                match self.ctx.binder.scopes.iter().find(|scope| {
                    scope.kind == ContainerKind::Module && scope.container_node == module_decl_idx
                }) {
                    Some(scope) => scope
                        .table
                        .iter()
                        .filter(|(name, _)| enum_member_name_nodes.contains_key(name.as_str()))
                        .map(|(name, &sym_id)| (name.clone(), sym_id))
                        .collect(),
                    None => continue,
                };
            for (name, sym_id) in candidates {
                let enum_name_node = enum_member_name_nodes[&name];
                // An enum member excludes everything in `VALUE | TYPE`, so only
                // an *exported* namespace member that carries a value or type
                // meaning collides; the guard skips non-exported locals.
                let export_decl = match self.ctx.binder.get_symbol(sym_id) {
                    Some(sym)
                        if sym.is_exported
                            && (sym.flags & symbol_flags::ENUM_MEMBER_EXCLUDES) != 0 =>
                    {
                        if sym.value_declaration.is_some() {
                            sym.value_declaration
                        } else if let Some(&decl) = sym.declarations.first() {
                            decl
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                let export_name_node = self
                    .get_declaration_name_node(export_decl)
                    .unwrap_or(export_decl);
                // `error_at_node` dedupes by (start, code), so repeating the
                // enum member node across several colliding exports is harmless.
                self.error_at_node_msg(
                    enum_name_node,
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                    &[&name],
                );
                self.error_at_node_msg(
                    export_name_node,
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                    &[&name],
                );
            }
        }
    }

    /// Check if a declaration node is an export specifier inside a re-export
    /// (`export { X } from "module"`). Re-export specifiers in tsc create
    /// symbols in the file's exports table rather than in file locals, so
    /// they should not conflict with import specifiers that share the same
    /// name.
    pub(super) fn is_reexport_specifier(&self, decl_idx: NodeIndex) -> bool {
        let node = match self.ctx.arena.get(decl_idx) {
            Some(n) => n,
            None => return false,
        };
        if node.kind != syntax_kind_ext::EXPORT_SPECIFIER {
            return false;
        }
        // Walk up: ExportSpecifier -> NamedExports -> ExportDeclaration
        let named_exports_idx = match self.ctx.arena.get_extended(decl_idx) {
            Some(ext) if ext.parent.is_some() => ext.parent,
            _ => return false,
        };
        let export_decl_idx = match self.ctx.arena.get_extended(named_exports_idx) {
            Some(ext) if ext.parent.is_some() => ext.parent,
            _ => return false,
        };
        let export_decl_node = match self.ctx.arena.get(export_decl_idx) {
            Some(n) => n,
            None => return false,
        };
        // Check if the ExportDeclaration has a module specifier (i.e., `from "mod"`)
        self.ctx
            .arena
            .get_export_decl(export_decl_node)
            .is_some_and(|data| data.module_specifier.is_some())
    }

    /// Check if a declaration node is an import alias (import specifier,
    /// import clause, or namespace import). These create ALIAS symbols
    /// that reference a declaration in another file. In tsc, import
    /// aliases are separate symbols and never conflict with the original
    /// declaration. Our binder sometimes merges them, so we use this
    /// check to suppress false duplicate diagnostics.
    pub(super) fn is_import_alias_node(&self, decl_idx: NodeIndex) -> bool {
        self.ctx.arena.get(decl_idx).is_some_and(|node| {
            matches!(
                node.kind,
                syntax_kind_ext::IMPORT_SPECIFIER
                    | syntax_kind_ext::IMPORT_CLAUSE
                    | syntax_kind_ext::NAMESPACE_IMPORT
            )
        })
    }

    pub(super) fn node_is_import_alias(&self, flags: u32, idx: NodeIndex) -> bool {
        (flags & symbol_flags::ALIAS) != 0 && self.is_import_alias_node(idx)
    }

    /// Emit duplicate-identifier diagnostics, defaulting to local-anchored
    /// errors but redirecting to a remote anchor when tsc's plain-JS
    /// `addDuplicateLocations` filter (checker.ts ~L2782-L2783) applies.
    pub(super) fn emit_duplicate_identifier_diagnostics(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        declarations: &[(
            NodeIndex,
            u32,
            bool,
            bool,
            super::duplicate_identifiers::DuplicateDeclarationOrigin,
        )],
        conflicts: &FxHashSet<NodeIndex>,
        code: u32,
        message: &str,
    ) {
        if self.try_redirect_dup_id_to_non_plain_js_remote(
            sym_id,
            declarations,
            conflicts,
            code,
            message,
        ) {
            return;
        }
        self.emit_remote_default_lib_duplicate_identifier_diagnostics(
            sym_id,
            declarations,
            conflicts,
            code,
            message,
        );
        let default_export_alias_anchor = self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
            let name = &symbol.escaped_name;
            let has_remote_default_import_alias_conflict = code
                == crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER
                && declarations.iter().any(|(_, flags, is_local, _, origin)| {
                    !*is_local
                        && *origin == DuplicateDeclarationOrigin::GlobalScopeConflict
                        && (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
                        && (flags & symbol_flags::ALIAS) != 0
                });
            (has_remote_default_import_alias_conflict
                && self.ambient_external_module_default_export_has_value_sibling_named(name))
            .then(|| self.current_file_default_export_identifier_named(name))
            .flatten()
        });
        for (decl_idx, _decl_flags, is_local, _, _) in declarations {
            if *is_local && conflicts.contains(decl_idx) {
                let error_node = default_export_alias_anchor
                    .or_else(|| self.get_declaration_name_node(*decl_idx))
                    .unwrap_or(*decl_idx);
                self.error_at_node(error_node, message, code);
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

    #[test]
    fn module_block_scoped_conflict_detects_global_vs_module_let() {
        // Simulates typeReferenceDirectives7.ts:
        // Script file declares `let $` (global, block-scoped)
        // Module file declares `export let $` (module, block-scoped)
        // Expected: the helper finds the module file's `$` as a conflict
        let files = [
            (
                "/a.d.ts",
                // Script file (no import/export) — global `let $`
                "declare let $: { x: number }\n",
            ),
            (
                "/index.d.ts",
                // Module file (has export) — module-scoped `let $`
                "export let $ = 1;\nexport let x: typeof $;\n",
            ),
        ];

        with_checker(&files, "/a.d.ts", |checker, _a_idx, _index_idx| {
            let conflicts = checker
                .module_file_block_scoped_conflict_declarations_for_current_file(
                    "$",
                    tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE,
                );

            assert!(
                !conflicts.is_empty(),
                "Expected to find module file's `$` as a block-scoped conflict"
            );
            assert!(
                conflicts.iter().all(|(_, _, is_local, _, _)| !*is_local),
                "All conflict declarations should be remote: {conflicts:#?}"
            );
        });
    }
}
