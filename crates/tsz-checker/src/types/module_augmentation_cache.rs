//! Owner-qualified cache publication for module-augmentation declarations.

use crate::query_boundaries::module_augmentation as module_augmentation_boundary;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::{BinderState, ModuleAugmentation, symbol_flags};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuntimeDeclarationMatch {
    DirectTarget,
    ReplayFallback,
    Foreign,
}

impl CheckerState<'_> {
    /// Return the declarations that may contribute an exact runtime export for
    /// `module_spec`.
    ///
    /// The general augmentation collector deliberately has broader semantics:
    /// it can replay declarations through re-export barrels and its candidate
    /// keys include raw relative specifiers. Two unrelated files can therefore
    /// both register `declare module "./index"` under the same raw key. That is
    /// useful discovery behavior for type/barrel merging, but an exact runtime
    /// query must not select the other file's concrete value.
    ///
    /// Qualify declarations reached through a direct candidate key against the
    /// key's owner file. Declarations reached only through the collector's
    /// re-export fallback have no direct candidate registration and remain
    /// eligible, preserving its program-order barrel semantics.
    pub(crate) fn module_augmentation_runtime_declarations(
        &self,
        module_spec: &str,
        export_name: &str,
    ) -> Vec<ModuleAugmentation> {
        self.module_augmentation_runtime_declarations_with_direct_value(module_spec, export_name)
            .0
    }

    /// Return augmentation declarations qualified to the exact resolved target,
    /// regardless of whether they contribute type or value meaning.
    pub(crate) fn exact_module_augmentation_declarations(
        &self,
        module_spec: &str,
        export_name: &str,
    ) -> Vec<ModuleAugmentation> {
        let declarations = self.get_module_augmentation_declarations(module_spec, export_name);
        let Some(target_file_idx) = self.module_augmentation_runtime_target_file(module_spec)
        else {
            return declarations;
        };
        let direct_candidates = self.module_augmentation_key_candidates(module_spec);
        let mut seen = FxHashSet::default();
        declarations
            .into_iter()
            .filter(|augmentation| {
                self.module_augmentation_runtime_declaration_match(
                    augmentation,
                    target_file_idx,
                    &direct_candidates,
                ) != RuntimeDeclarationMatch::Foreign
            })
            .filter(|augmentation| {
                let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
                let owner_file_idx = self
                    .ctx
                    .get_file_idx_for_arena(arena)
                    .unwrap_or(self.ctx.current_file_idx);
                seen.insert((owner_file_idx, augmentation.node))
            })
            .collect()
    }

    pub(crate) fn module_augmentation_runtime_declarations_with_direct_value(
        &self,
        module_spec: &str,
        export_name: &str,
    ) -> (Vec<ModuleAugmentation>, bool, bool) {
        let declarations = self.get_module_augmentation_declarations(module_spec, export_name);
        let Some(target_file_idx) = self.module_augmentation_runtime_target_file(module_spec)
        else {
            // Ambient and otherwise unresolved module names have no file
            // identity to qualify against. Preserve the established lookup.
            let has_direct_value = declarations.iter().any(|augmentation| {
                self.module_augmentation_declaration_contributes_value(augmentation)
            });
            return (declarations, has_direct_value, has_direct_value);
        };
        let direct_candidates = self.module_augmentation_key_candidates(module_spec);
        let mut has_direct_value = false;
        let mut has_runtime_value = false;

        let declarations = declarations
            .into_iter()
            .filter(|augmentation| {
                let declaration_match = self.module_augmentation_runtime_declaration_match(
                    augmentation,
                    target_file_idx,
                    &direct_candidates,
                );
                if declaration_match != RuntimeDeclarationMatch::Foreign
                    && self.module_augmentation_declaration_contributes_value(augmentation)
                {
                    has_runtime_value = true;
                    if declaration_match == RuntimeDeclarationMatch::DirectTarget {
                        has_direct_value = true;
                    }
                }
                declaration_match != RuntimeDeclarationMatch::Foreign
            })
            .collect();
        (declarations, has_direct_value, has_runtime_value)
    }

    /// Select one type-position augmentation declaration together with the
    /// binder file that owns its raw `SymbolId`, qualified against the exact
    /// target module.
    ///
    /// Type-position member resolution cannot carry a bare augmentation
    /// `SymbolId` across files: the same numeric id may denote an unrelated
    /// symbol in the target or consumer binder. Direct target declarations
    /// outrank replay-only barrel candidates, and declarations registered for a
    /// different resolved target are excluded.
    pub(crate) fn exact_module_augmentation_declaration_owner(
        &self,
        module_spec: &str,
        export_name: &str,
    ) -> Option<(ModuleAugmentation, usize)> {
        let declarations = self
            .get_module_augmentation_declarations(module_spec, export_name)
            .into_iter()
            .filter(|augmentation| {
                self.module_augmentation_declaration_has_type_meaning(augmentation)
            });
        let owner = |augmentation: &ModuleAugmentation| {
            let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
            self.ctx
                .get_file_idx_for_arena(arena)
                .unwrap_or(self.ctx.current_file_idx)
        };
        let Some(target_file_idx) = self.module_augmentation_runtime_target_file(module_spec)
        else {
            return declarations.into_iter().next().map(|augmentation| {
                let owner_file_idx = owner(&augmentation);
                (augmentation, owner_file_idx)
            });
        };
        let direct_candidates = self.module_augmentation_key_candidates(module_spec);
        let mut replay_fallback = None;

        for augmentation in declarations {
            let declaration_match = self.module_augmentation_runtime_declaration_match(
                &augmentation,
                target_file_idx,
                &direct_candidates,
            );
            match declaration_match {
                RuntimeDeclarationMatch::DirectTarget => {
                    let owner_file_idx = owner(&augmentation);
                    return Some((augmentation, owner_file_idx));
                }
                RuntimeDeclarationMatch::ReplayFallback => {
                    replay_fallback.get_or_insert_with(|| {
                        let owner_file_idx = owner(&augmentation);
                        (augmentation, owner_file_idx)
                    });
                }
                RuntimeDeclarationMatch::Foreign => {}
            }
        }

        replay_fallback
    }

    fn module_augmentation_declaration_has_type_meaning(
        &self,
        augmentation: &ModuleAugmentation,
    ) -> bool {
        let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
        let Some(node) = arena.get(augmentation.node) else {
            return false;
        };
        let owner_file_idx = self
            .ctx
            .get_file_idx_for_arena(arena)
            .unwrap_or(self.ctx.current_file_idx);
        if let Some(symbol) = self
            .ctx
            .get_binder_for_file(owner_file_idx)
            .and_then(|binder| binder.get_node_symbol(augmentation.node))
            .and_then(|symbol_id| {
                self.ctx
                    .get_binder_for_file(owner_file_idx)?
                    .get_symbol(symbol_id)
            })
            && symbol.has_any_flags(
                symbol_flags::TYPE | symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE,
            )
        {
            return true;
        }

        matches!(
            node.kind,
            syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::MODULE_DECLARATION
        )
    }

    fn module_augmentation_runtime_target_file(&self, module_spec: &str) -> Option<usize> {
        let trimmed = module_spec.trim().trim_matches('"').trim_matches('\'');
        trimmed
            .strip_prefix("file_idx:")
            .and_then(|file_idx| file_idx.parse::<usize>().ok())
            .or_else(|| self.ctx.resolve_import_target(module_spec))
            .or_else(|| self.ctx.resolve_import_target(trimmed))
            .or_else(|| {
                self.ctx.all_arenas.as_ref().and_then(|arenas| {
                    arenas.iter().enumerate().find_map(|(file_idx, arena)| {
                        arena
                            .source_files
                            .first()
                            .is_some_and(|source| {
                                source.file_name == module_spec || source.file_name == trimmed
                            })
                            .then_some(file_idx)
                    })
                })
            })
    }

    fn module_augmentation_runtime_declaration_match(
        &self,
        augmentation: &ModuleAugmentation,
        target_file_idx: usize,
        direct_candidates: &[String],
    ) -> RuntimeDeclarationMatch {
        let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
        let owner_file_idx = self
            .ctx
            .get_file_idx_for_arena(arena)
            .unwrap_or(self.ctx.current_file_idx);
        let Some(owner_binder) = self.ctx.get_binder_for_file(owner_file_idx) else {
            return RuntimeDeclarationMatch::ReplayFallback;
        };

        let mut saw_resolved_foreign_direct_registration = false;
        for (registered_specifier, registered_declarations) in
            owner_binder.module_augmentations.iter()
        {
            if !direct_candidates
                .iter()
                .any(|candidate| candidate == registered_specifier)
                || !registered_declarations.iter().any(|registered| {
                    registered.node == augmentation.node && registered.name == augmentation.name
                })
            {
                continue;
            }

            let trimmed = registered_specifier
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            let registered_target = trimmed
                .strip_prefix("file_idx:")
                .and_then(|file_idx| file_idx.parse::<usize>().ok())
                .or_else(|| {
                    self.ctx
                        .resolve_import_target_from_file(owner_file_idx, registered_specifier)
                })
                .or_else(|| {
                    self.ctx
                        .resolve_import_target_from_file(owner_file_idx, trimmed)
                });

            match registered_target {
                Some(registered_target) if registered_target == target_file_idx => {
                    return RuntimeDeclarationMatch::DirectTarget;
                }
                Some(_) => saw_resolved_foreign_direct_registration = true,
                // Do not suppress ambient/unresolved direct registrations.
                None => return RuntimeDeclarationMatch::DirectTarget,
            }
        }

        if saw_resolved_foreign_direct_registration {
            RuntimeDeclarationMatch::Foreign
        } else {
            RuntimeDeclarationMatch::ReplayFallback
        }
    }

    fn module_augmentation_declaration_contributes_value(
        &self,
        augmentation: &ModuleAugmentation,
    ) -> bool {
        let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
        let Some(node) = arena.get(augmentation.node) else {
            return false;
        };
        matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::VARIABLE_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
        ) || (node.kind == syntax_kind_ext::MODULE_DECLARATION
            && self.module_augmentation_namespace_is_value_instantiated(augmentation.node, arena))
    }

    /// Decide namespace instantiation in the declaration owner's binder.
    ///
    /// `NodeArena::is_namespace_instantiated` is intentionally conservative:
    /// every named export may be a value until binding has resolved it. Exact
    /// module-augmentation runtime publication must perform that resolution,
    /// including when the declaration lives in a foreign arena.
    pub(crate) fn module_augmentation_namespace_is_value_instantiated(
        &self,
        namespace_idx: NodeIndex,
        arena: &NodeArena,
    ) -> bool {
        if std::ptr::eq(arena, self.ctx.arena) {
            return self.is_namespace_declaration_value_instantiated(namespace_idx);
        }
        let Some(binder) = self.ctx.get_binder_for_arena(arena) else {
            return arena.is_namespace_instantiated(namespace_idx);
        };
        let mut visiting = FxHashSet::default();
        Self::namespace_is_value_instantiated_in_owner(arena, binder, namespace_idx, &mut visiting)
            .unwrap_or_else(|| arena.is_namespace_instantiated(namespace_idx))
    }

    fn namespace_is_value_instantiated_in_owner(
        arena: &NodeArena,
        binder: &BinderState,
        namespace_idx: NodeIndex,
        visiting: &mut FxHashSet<NodeIndex>,
    ) -> Option<bool> {
        if !visiting.insert(namespace_idx) {
            return Some(false);
        }
        let result = Self::namespace_is_value_instantiated_in_owner_inner(
            arena,
            binder,
            namespace_idx,
            visiting,
        );
        visiting.remove(&namespace_idx);
        result
    }

    fn namespace_is_value_instantiated_in_owner_inner(
        arena: &NodeArena,
        binder: &BinderState,
        namespace_idx: NodeIndex,
        visiting: &mut FxHashSet<NodeIndex>,
    ) -> Option<bool> {
        let node = arena.get(namespace_idx)?;
        if node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION {
            return Some(true);
        }
        if node.kind != syntax_kind_ext::MODULE_DECLARATION {
            return Some(false);
        }
        let module_decl = arena.get_module(node)?;
        if module_decl.body.is_none() {
            return Some(false);
        }
        let body_node = arena.get(module_decl.body)?;
        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            return Self::namespace_is_value_instantiated_in_owner(
                arena,
                binder,
                module_decl.body,
                visiting,
            );
        }
        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return Some(false);
        }
        let module_block = arena.get_module_block(body_node)?;
        let Some(statements) = module_block.statements.as_ref() else {
            return Some(false);
        };
        for &statement_idx in &statements.nodes {
            let Some(statement) = arena.get(statement_idx) else {
                continue;
            };
            if Self::namespace_statement_has_owner_value(
                arena,
                binder,
                statement,
                statement_idx,
                visiting,
            ) {
                return Some(true);
            }
        }
        Some(false)
    }

    fn namespace_statement_has_owner_value(
        arena: &NodeArena,
        binder: &BinderState,
        node: &tsz_parser::parser::node::Node,
        node_idx: NodeIndex,
        visiting: &mut FxHashSet<NodeIndex>,
    ) -> bool {
        match node.kind {
            kind if kind == syntax_kind_ext::INTERFACE_DECLARATION
                || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || kind == syntax_kind_ext::IMPORT_DECLARATION
                || kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION =>
            {
                false
            }
            kind if kind == syntax_kind_ext::MODULE_DECLARATION => {
                Self::namespace_is_value_instantiated_in_owner(arena, binder, node_idx, visiting)
                    .unwrap_or_else(|| arena.is_namespace_instantiated(node_idx))
            }
            kind if kind == syntax_kind_ext::EXPORT_DECLARATION => {
                let Some(export_decl) = arena.get_export_decl(node) else {
                    return false;
                };
                if export_decl.is_type_only {
                    return false;
                }
                if export_decl.module_specifier.is_some() {
                    return true;
                }
                let Some(clause) = arena.get(export_decl.export_clause) else {
                    return false;
                };
                match clause.kind {
                    kind if kind == syntax_kind_ext::VARIABLE_STATEMENT
                        || kind == syntax_kind_ext::FUNCTION_DECLARATION
                        || kind == syntax_kind_ext::CLASS_DECLARATION
                        || kind == syntax_kind_ext::ENUM_DECLARATION =>
                    {
                        true
                    }
                    kind if kind == syntax_kind_ext::MODULE_DECLARATION => {
                        Self::namespace_is_value_instantiated_in_owner(
                            arena,
                            binder,
                            export_decl.export_clause,
                            visiting,
                        )
                        .unwrap_or_else(|| {
                            arena.is_namespace_instantiated(export_decl.export_clause)
                        })
                    }
                    kind if kind == syntax_kind_ext::NAMED_EXPORTS => {
                        Self::owner_named_exports_have_value(arena, binder, clause, visiting)
                    }
                    _ => false,
                }
            }
            _ => true,
        }
    }

    fn owner_named_exports_have_value(
        arena: &NodeArena,
        binder: &BinderState,
        named_exports: &tsz_parser::parser::node::Node,
        visiting: &mut FxHashSet<NodeIndex>,
    ) -> bool {
        let Some(named) = arena.get_named_imports(named_exports) else {
            return true;
        };
        for &specifier_idx in &named.elements.nodes {
            let Some(specifier_node) = arena.get(specifier_idx) else {
                continue;
            };
            if specifier_node.kind != syntax_kind_ext::EXPORT_SPECIFIER {
                continue;
            }
            let Some(specifier) = arena.get_specifier(specifier_node) else {
                continue;
            };
            if specifier.is_type_only {
                continue;
            }
            let lookup_idx = if specifier.property_name.is_none() {
                specifier.name
            } else {
                specifier.property_name
            };
            if lookup_idx.is_none() {
                return true;
            }
            let Some(symbol_id) =
                Self::local_named_export_target_symbol_in_owner(arena, binder, lookup_idx)
            else {
                return true;
            };
            let Some(symbol) = binder.get_symbol(symbol_id) else {
                return true;
            };
            let value_mask = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
            if symbol.has_any_flags(value_mask) {
                return true;
            }
            if symbol.has_any_flags(symbol_flags::VALUE_MODULE)
                && symbol.declarations.iter().any(|&declaration| {
                    Self::namespace_is_value_instantiated_in_owner(
                        arena,
                        binder,
                        declaration,
                        visiting,
                    )
                    .unwrap_or_else(|| arena.is_namespace_instantiated(declaration))
                })
            {
                return true;
            }
            if symbol.has_any_flags(symbol_flags::ALIAS) {
                return true;
            }
        }
        false
    }

    pub(crate) fn local_named_export_target_symbol_in_owner(
        arena: &NodeArena,
        binder: &BinderState,
        name_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let directly_resolved =
            binder.resolve_identifier_with_filter(arena, name_idx, &[], |_| true);
        if let Some(symbol_id) = directly_resolved
            && binder.get_symbol(symbol_id).is_some_and(|symbol| {
                !symbol.has_any_flags(symbol_flags::ALIAS) || symbol.import_module().is_some()
            })
        {
            return Some(symbol_id);
        }

        let name = arena.get_identifier_text(name_idx)?;
        let enclosing_namespace = Self::enclosing_module_declaration(arena, name_idx)?;
        binder
            .get_symbols()
            .find_all_by_name(name)
            .iter()
            .copied()
            .filter(|candidate| Some(*candidate) != directly_resolved)
            .find(|&candidate| {
                binder.get_symbol(candidate).is_some_and(|symbol| {
                    symbol.declarations.iter().copied().any(|declaration| {
                        Self::declaration_is_within_module(arena, declaration, enclosing_namespace)
                    })
                })
            })
            .or(directly_resolved)
    }

    fn enclosing_module_declaration(
        arena: &NodeArena,
        mut node_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        while node_idx.is_some() {
            if arena
                .get(node_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::MODULE_DECLARATION)
            {
                return Some(node_idx);
            }
            node_idx = arena.get_extended(node_idx)?.parent;
        }
        None
    }

    fn declaration_is_within_module(
        arena: &NodeArena,
        mut declaration: NodeIndex,
        module: NodeIndex,
    ) -> bool {
        while declaration.is_some() {
            if declaration == module {
                return true;
            }
            let Some(extended) = arena.get_extended(declaration) else {
                return false;
            };
            declaration = extended.parent;
        }
        false
    }

    /// Return the declaration spaces supplied by the target module itself,
    /// before external module augmentations are considered.
    pub(crate) fn module_augmentation_target_native_spaces(
        &self,
        module_spec: &str,
        export_name: &str,
    ) -> Option<(bool, bool)> {
        let module_spec = module_spec.trim().trim_matches('"').trim_matches('\'');
        let target_file_idx = module_spec
            .strip_prefix("file_idx:")
            .and_then(|file_idx| file_idx.parse::<usize>().ok())
            .or_else(|| self.ctx.resolve_import_target(module_spec))
            .or_else(|| {
                self.ctx.all_arenas.as_ref().and_then(|arenas| {
                    arenas.iter().enumerate().find_map(|(file_idx, arena)| {
                        arena
                            .source_files
                            .first()
                            .is_some_and(|source| source.file_name == module_spec)
                            .then_some(file_idx)
                    })
                })
            })?;
        let mut visited = FxHashSet::default();
        let Some((symbol_id, owner_file_idx)) =
            self.resolve_export_in_file(target_file_idx, export_name, &mut visited)
        else {
            return Some((false, false));
        };
        let binder = self.ctx.get_binder_for_file(owner_file_idx)?;
        let symbol = binder.get_symbol(symbol_id)?;
        // `resolve_export_in_file` deliberately falls back to augmentation-only
        // exports after exhausting a module's native exports and re-exports.
        // This query is the pre-augmentation side of the split, so recognize
        // that fallback by declaration identity instead of treating the
        // augmentation symbol as a native value.
        let resolved_from_augmentation_fallback = self
            .get_module_augmentation_declarations(module_spec, export_name)
            .iter()
            .any(|augmentation| {
                let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
                self.ctx.get_file_idx_for_arena(arena) == Some(owner_file_idx)
                    && (symbol.value_declaration == augmentation.node
                        || symbol.declarations.contains(&augmentation.node))
            });
        if resolved_from_augmentation_fallback {
            return Some((false, false));
        }
        let flags = symbol.flags;
        let raw_has_value = flags & symbol_flags::VALUE != 0;
        let is_namespace_only =
            flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0
                && flags & (symbol_flags::VALUE & !symbol_flags::VALUE_MODULE) == 0;
        let has_value = raw_has_value
            && (!is_namespace_only || self.symbol_has_runtime_value_in_binder(binder, symbol_id));
        Some((flags & symbol_flags::TYPE != 0, has_value))
    }

    pub(crate) fn collect_module_augmentation_names(&self, module_spec: &str) -> Vec<String> {
        let mut names = Vec::new();
        for candidate in self.module_augmentation_key_candidates(module_spec) {
            if let Some(augmentations) = self.ctx.binder.module_augmentations.get(&candidate) {
                for augmentation in augmentations {
                    if !names.iter().any(|name| name == &augmentation.name) {
                        names.push(augmentation.name.clone());
                    }
                }
            }
            if let Some(index) = self.ctx.global_module_augmentations_index.as_ref()
                && let Some(entries) = index.get(&candidate)
            {
                for (_, augmentation) in entries {
                    if !names.iter().any(|name| name == &augmentation.name) {
                        names.push(augmentation.name.clone());
                    }
                }
            }
        }
        names
    }

    pub(super) fn module_augmentation_source_files(&self, module_spec: &str) -> Vec<u32> {
        let candidates = self.module_augmentation_key_candidates(module_spec);
        let mut files = FxHashSet::default();

        for candidate in &candidates {
            if self.ctx.binder.module_augmentations.contains_key(candidate) {
                files.insert(self.ctx.current_file_idx as u32);
            }
        }
        if let Some(augmentation_index) = self.ctx.global_module_augmentations_index.as_ref() {
            for candidate in &candidates {
                if let Some(entries) = augmentation_index.get(candidate) {
                    files.extend(entries.iter().map(|(file_idx, _)| *file_idx as u32));
                }
            }
        } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
            for (file_idx, binder) in all_binders.iter().enumerate() {
                if candidates
                    .iter()
                    .any(|candidate| binder.module_augmentations.contains_key(candidate))
                {
                    files.insert(file_idx as u32);
                }
            }
        }

        files.into_iter().collect()
    }

    /// Preserve `self: Foo` inside `declare module "./m" { interface Foo { ... } }`
    /// as a Lazy `DefId` so post-merge publication can redirect it to the merged type.
    pub(super) fn module_augmentation_self_reference_type(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        type_annotation: tsz_parser::parser::NodeIndex,
    ) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext::{QUALIFIED_NAME, TYPE_REFERENCE};

        let annotation_node = self.ctx.arena.get(type_annotation)?;
        if annotation_node.kind != TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.ctx.arena.get_type_ref(annotation_node)?;
        let name_node = self.ctx.arena.get(type_ref.type_name)?;
        if name_node.kind == QUALIFIED_NAME {
            return None;
        }
        let ident = self.ctx.arena.get_identifier(name_node)?;
        if ident.escaped_text != interface_name {
            return None;
        }

        let symbol_id =
            self.current_module_augmentation_target_symbol(module_spec, interface_name)?;
        let definition = self.ctx.def_id_for_declaration_in_file(
            symbol_id,
            self.ctx.current_file_idx,
            interface_name,
        )?;
        let base_type = self.ctx.types.factory().lazy(definition);
        let type_arguments = type_ref
            .type_arguments
            .as_ref()
            .map(|arguments| {
                arguments
                    .nodes
                    .iter()
                    .map(|&argument| self.get_type_from_type_node_in_type_literal(argument))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if type_arguments.is_empty() {
            Some(base_type)
        } else {
            Some(
                module_augmentation_boundary::self_reference_application_type(
                    self.ctx.types,
                    base_type,
                    type_arguments,
                ),
            )
        }
    }

    pub(crate) fn current_module_augmentation_target_symbol(
        &self,
        module_spec: &str,
        interface_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let candidates = self.module_augmentation_key_candidates(module_spec);
        if let Some(targets) = self.ctx.global_augmentation_targets_index.as_ref() {
            let binder = self.ctx.get_binder_for_file(self.ctx.current_file_idx)?;
            return candidates.iter().find_map(|candidate| {
                let is_current_file_target = targets
                    .get(candidate)?
                    .iter()
                    .any(|&(_, file_idx)| file_idx == self.ctx.current_file_idx);
                if !is_current_file_target {
                    return None;
                }

                // Skeleton entries are extracted before program binding can
                // remap binder-local symbol IDs. Use the skeleton index only
                // to route to the owning `(target, file)` pair, then select
                // the final symbol ID from that file binder's authoritative
                // augmentation registry.
                binder
                    .augmentation_target_modules
                    .iter()
                    .find_map(|(&symbol_id, target)| {
                        (target == candidate
                            && binder
                                .get_symbol(symbol_id)
                                .is_some_and(|symbol| symbol.escaped_name == interface_name))
                        .then_some(symbol_id)
                    })
            });
        }

        self.ctx
            .binder
            .augmentation_target_modules
            .iter()
            .find_map(|(&symbol_id, target)| {
                (candidates.iter().any(|candidate| candidate == target)
                    && self
                        .ctx
                        .binder
                        .get_symbol(symbol_id)
                        .is_some_and(|symbol| symbol.escaped_name == interface_name))
                .then_some(symbol_id)
            })
    }

    /// Publish the merged type for augmentation-local self references without
    /// allowing binder-local `SymbolId` collisions to cross file boundaries.
    pub(crate) fn update_augmentation_local_symbol_types(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        merged_type: TypeId,
        applied_declarations: &[ModuleAugmentation],
        include_candidate_symbols: bool,
    ) {
        let mut matching_symbols = Vec::new();
        for augmentation in applied_declarations {
            let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
            let Some(file_idx) = self.ctx.get_file_idx_for_arena(arena).or_else(|| {
                arena
                    .shares_node_storage_with(self.ctx.arena)
                    .then_some(self.ctx.current_file_idx)
            }) else {
                continue;
            };
            let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
                continue;
            };
            let mut candidates = Vec::new();
            if let Some(symbol_id) = binder.get_node_symbol(augmentation.node) {
                candidates.push(symbol_id);
            }
            for &symbol_id in binder.get_symbols().find_all_by_name(interface_name) {
                if !candidates.contains(&symbol_id) {
                    candidates.push(symbol_id);
                }
            }
            for symbol_id in candidates {
                if binder.get_symbol(symbol_id).is_some_and(|symbol| {
                    symbol.escaped_name == interface_name
                        && symbol.flags & symbol_flags::TYPE != 0
                        && (symbol.value_declaration == augmentation.node
                            || symbol.declarations.contains(&augmentation.node))
                }) {
                    matching_symbols.push((file_idx, symbol_id));
                }
            }
        }

        if include_candidate_symbols {
            let candidates = self.module_augmentation_key_candidates(module_spec);
            if let Some(targets) = self.ctx.global_augmentation_targets_index.as_ref() {
                for candidate in &candidates {
                    if let Some(entries) = targets.get(candidate) {
                        for &(symbol_id, file_idx) in entries {
                            if self
                                .ctx
                                .get_binder_for_file(file_idx)
                                .and_then(|binder| binder.get_symbol(symbol_id))
                                .is_some_and(|symbol| symbol.escaped_name == interface_name)
                            {
                                matching_symbols.push((file_idx, symbol_id));
                            }
                        }
                    }
                }
            } else if let Some(binders) = self.ctx.all_binders.as_ref() {
                for binder in binders.iter() {
                    for (&symbol_id, target) in binder.augmentation_target_modules.iter() {
                        if candidates.iter().any(|candidate| candidate == target)
                            && binder
                                .get_symbol(symbol_id)
                                .is_some_and(|symbol| symbol.escaped_name == interface_name)
                            && let Some(file_idx) =
                                self.augmentation_symbol_owner_file(binder, symbol_id)
                        {
                            matching_symbols.push((file_idx, symbol_id));
                        }
                    }
                }
            } else {
                for (&symbol_id, target) in self.ctx.binder.augmentation_target_modules.iter() {
                    if candidates.iter().any(|candidate| candidate == target)
                        && self
                            .ctx
                            .binder
                            .get_symbol(symbol_id)
                            .is_some_and(|symbol| symbol.escaped_name == interface_name)
                    {
                        matching_symbols.push((self.ctx.current_file_idx, symbol_id));
                    }
                }
            }
        }

        matching_symbols.sort_unstable();
        matching_symbols.dedup();
        let definitions: Vec<_> = matching_symbols
            .into_iter()
            .filter_map(|(file_idx, symbol_id)| {
                let definition =
                    self.ctx
                        .def_id_for_declaration_in_file(symbol_id, file_idx, interface_name)?;
                if file_idx == self.ctx.current_file_idx {
                    self.ctx.symbol_types.insert(symbol_id, merged_type);
                    self.ctx
                        .symbol_instance_types
                        .insert(symbol_id, merged_type);
                }
                Some(definition)
            })
            .collect();
        for definition in definitions {
            self.ctx
                .register_augmented_def_in_envs(definition, merged_type, false);
        }
    }

    fn augmentation_symbol_owner_file(
        &self,
        binder: &tsz_binder::BinderState,
        symbol_id: tsz_binder::SymbolId,
    ) -> Option<usize> {
        binder
            .symbol_arenas
            .get(&symbol_id)
            .and_then(|arena| self.ctx.get_file_idx_for_arena(arena))
            .or_else(|| {
                let symbol = binder.get_symbol(symbol_id)?;
                symbol.declarations.iter().find_map(|declaration| {
                    binder
                        .declaration_arenas
                        .get(&(symbol_id, *declaration))
                        .and_then(|arenas| {
                            arenas
                                .iter()
                                .find_map(|arena| self.ctx.get_file_idx_for_arena(arena))
                        })
                })
            })
    }
}
