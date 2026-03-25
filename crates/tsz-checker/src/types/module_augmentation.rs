//! Module augmentation merging (Rule #44).
//!
//! Handles `declare module 'x' { ... }` augmentations by:
//! - Discovering augmentation declarations across files
//! - Extracting augmentation members (properties, methods, variables)
//! - Merging augmented members into the target interface/namespace type
//! - Updating cached symbol types for self-referential augmentations

use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tsz_binder::ModuleAugmentation;
use tsz_solver::TypeId;
use tsz_solver::Visibility;

impl<'a> CheckerState<'a> {
    /// Get module augmentation declarations for a given module specifier and interface name.
    ///
    /// This function looks up interface/type declarations inside `declare module 'x'` blocks
    /// that should be merged with the target module's interface.
    ///
    /// # Arguments
    /// * `module_spec` - The module specifier (e.g., "express", "lodash")
    /// * `interface_name` - The name of the interface to find augmentations for
    ///
    /// # Returns
    /// A vector of `NodeIndex` pointing to augmentation declarations
    ///
    /// # Example
    /// ```typescript
    /// // In user code:
    /// declare module 'express' {
    ///     interface Request {
    ///         user: User;  // This augments the original Request interface
    ///     }
    /// }
    /// ```
    pub(crate) fn get_module_augmentation_declarations(
        &self,
        module_spec: &str,
        interface_name: &str,
    ) -> Vec<tsz_binder::ModuleAugmentation> {
        let mut result = Vec::new();
        let mut candidates = crate::module_resolution::module_specifier_candidates(module_spec);
        fn push_unique(candidates: &mut Vec<String>, candidate: String) {
            if !candidate.is_empty() && !candidates.iter().any(|existing| existing == &candidate) {
                candidates.push(candidate);
            }
        }

        let trimmed = module_spec.trim().trim_matches('"').trim_matches('\'');
        let mut resolved_source_idx = None;
        for specifier in [module_spec, trimmed] {
            if let Some(target_idx) = self.ctx.resolve_import_target(specifier) {
                push_unique(&mut candidates, format!("file_idx:{target_idx}"));
                if resolved_source_idx.is_none() {
                    resolved_source_idx = Some(target_idx);
                }
            }
        }

        // When module_spec is a resolved file path, augmentations may be keyed by a
        // bare specifier. Reverse-lookup: resolve file path to file index, then find
        // augmentation keys that resolve to the same target file.
        if resolved_source_idx.is_none()
            && let Some(arenas) = self.ctx.all_arenas.as_ref()
        {
            for (idx, arena) in arenas.iter().enumerate() {
                if let Some(sf) = arena.source_files.first()
                    && (sf.file_name == module_spec || sf.file_name == trimmed)
                {
                    resolved_source_idx = Some(idx);
                    break;
                }
            }
        }
        if let Some(source_idx) = resolved_source_idx {
            let all_aug_keys: Vec<(String, usize)> =
                if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
                    aug_index
                        .iter()
                        .flat_map(|(key, entries)| {
                            entries
                                .first()
                                .map(|(file_idx, _)| (key.clone(), *file_idx))
                        })
                        .collect()
                } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                    let mut keys = Vec::new();
                    for (file_idx, binder) in all_binders.iter().enumerate() {
                        for aug_key in binder.module_augmentations.keys() {
                            if !keys.iter().any(|(k, _): &(String, usize)| k == aug_key) {
                                keys.push((aug_key.clone(), file_idx));
                            }
                        }
                    }
                    keys
                } else {
                    Vec::new()
                };
            for (aug_key, aug_file_idx) in all_aug_keys {
                if candidates.contains(&aug_key) {
                    continue;
                }
                if self
                    .ctx
                    .resolve_import_target_from_file(aug_file_idx, &aug_key)
                    .is_some_and(|idx| idx == source_idx)
                {
                    candidates.push(aug_key);
                }
            }
        }

        let is_relative = |specifier: &str| {
            specifier.starts_with("./")
                || specifier.starts_with(".\\")
                || specifier.starts_with("../")
                || specifier.starts_with("..\\")
                || specifier == "."
                || specifier == ".."
        };
        if is_relative(trimmed)
            && let Some(parent) = Path::new(&self.ctx.file_name).parent()
        {
            let normalized =
                parent
                    .join(trimmed)
                    .components()
                    .fold(PathBuf::new(), |mut path, component| {
                        match component {
                            Component::Prefix(prefix) => path.push(prefix.as_os_str()),
                            Component::RootDir => path.push(component.as_os_str()),
                            Component::CurDir => {}
                            Component::ParentDir => {
                                path.pop();
                            }
                            Component::Normal(part) => path.push(part),
                        }
                        path
                    });
            push_unique(&mut candidates, normalized.to_string_lossy().to_string());
        }

        for candidate in &candidates {
            if let Some(augmentations) = self.ctx.binder.module_augmentations.get(candidate) {
                result.extend(
                    augmentations
                        .iter()
                        .filter(|aug| aug.name == interface_name)
                        .cloned(),
                );
            }
        }

        // Use global module augmentations index for O(1) lookup instead of O(N) binder scan.
        // Cross-file augmentations need their arena populated so the node index is
        // interpreted in the correct AST arena (not the current file's arena).
        if result.is_empty() {
            if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
                for candidate in &candidates {
                    if let Some(entries) = aug_index.get(candidate) {
                        for (file_idx, aug) in entries.iter() {
                            if aug.name != interface_name {
                                continue;
                            }
                            let mut cloned = aug.clone();
                            if cloned.arena.is_none()
                                && let Some(arenas) = self.ctx.all_arenas.as_ref()
                                && let Some(arena) = arenas.get(*file_idx)
                            {
                                cloned.arena = Some(Arc::clone(arena));
                            }
                            result.push(cloned);
                        }
                    }
                }
            } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                for (file_idx, binder) in all_binders.iter().enumerate() {
                    for candidate in &candidates {
                        if let Some(augmentations) = binder.module_augmentations.get(candidate) {
                            for aug in augmentations.iter() {
                                if aug.name != interface_name {
                                    continue;
                                }
                                let mut cloned = aug.clone();
                                if cloned.arena.is_none()
                                    && let Some(arenas) = self.ctx.all_arenas.as_ref()
                                    && let Some(arena) = arenas.get(file_idx)
                                {
                                    cloned.arena = Some(Arc::clone(arena));
                                }
                                result.push(cloned);
                            }
                        }
                    }
                }
            }
        }

        // If still no direct augmentations found, search inside namespace augmentation
        // bodies for nested interface declarations.  This handles:
        //   declare module "./reexport" { namespace Root { interface Foo { self: Foo } } }
        // When resolving `ns.Root.Foo`, the augmentation for "Foo" is nested inside
        // the namespace "Root" augmentation, not registered as a top-level entry.
        if result.is_empty() {
            self.find_nested_augmentation_declarations(&candidates, interface_name, &mut result);
        }

        // If still no augmentations found, check augmentations on modules that
        // re-export from our source module. For example, if `./index` re-exports
        // from `./eventList` via `export * from './eventList'`, augmentations
        // targeting `./index` should also apply to interfaces from `./eventList`.
        if result.is_empty()
            && let Some(source_idx) = resolved_source_idx
        {
            // Use global module augmentations index when available for O(1) key iteration,
            // falling back to O(N) binder scan otherwise.
            #[allow(clippy::type_complexity)]
            let aug_entries: Vec<(String, Vec<(usize, ModuleAugmentation)>)> =
                if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
                    aug_index
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                    let mut entries: FxHashMap<String, Vec<(usize, ModuleAugmentation)>> =
                        FxHashMap::default();
                    for (file_idx, binder) in all_binders.iter().enumerate() {
                        for (aug_key, augs) in binder.module_augmentations.iter() {
                            entries
                                .entry(aug_key.clone())
                                .or_default()
                                .extend(augs.iter().map(|aug| (file_idx, aug.clone())));
                        }
                    }
                    entries.into_iter().collect()
                } else {
                    Vec::new()
                };

            let all_binders = self.ctx.all_binders.as_ref();
            for (aug_key, indexed_augs) in &aug_entries {
                if candidates.iter().any(|c| c == aug_key) {
                    continue;
                }
                if !indexed_augs
                    .iter()
                    .any(|(_, aug)| aug.name == interface_name)
                {
                    continue;
                }
                // Use the first file_idx that has this augmentation for resolution
                let Some(&(binder_idx, _)) = indexed_augs.first() else {
                    continue;
                };
                // Resolve the augmentation target module from the augmenting file
                let Some(aug_target_idx) = self
                    .ctx
                    .resolve_import_target_from_file(binder_idx, aug_key)
                else {
                    continue;
                };
                let Some(aug_target_binder) =
                    all_binders.and_then(|binders| binders.get(aug_target_idx))
                else {
                    continue;
                };
                // Check if the augmentation target re-exports from source
                let reexports_from_source =
                    aug_target_binder
                        .wildcard_reexports
                        .values()
                        .any(|sources| {
                            sources.iter().any(|src| {
                                self.ctx
                                    .resolve_import_target_from_file(aug_target_idx, src)
                                    == Some(source_idx)
                            })
                        });
                if reexports_from_source {
                    for (file_idx, aug) in indexed_augs.iter() {
                        if aug.name != interface_name {
                            continue;
                        }
                        let mut cloned = aug.clone();
                        if cloned.arena.is_none()
                            && let Some(arenas) = self.ctx.all_arenas.as_ref()
                            && let Some(arena) = arenas.get(*file_idx)
                        {
                            cloned.arena = Some(Arc::clone(arena));
                        }
                        result.push(cloned);
                    }
                }
            }
        }

        result
    }

    /// Search inside namespace augmentation bodies for nested interface declarations.
    ///
    /// For an augmentation like:
    /// ```typescript
    /// declare module "./m" { namespace Root { interface Foo { self: Foo } } }
    /// ```
    /// A lookup for `interface_name = "Foo"` will not find it via the top-level
    /// augmentation name ("Root").  This helper walks one level into namespace
    /// augmentations and collects nested interface declarations that match.
    fn find_nested_augmentation_declarations(
        &self,
        candidates: &[String],
        interface_name: &str,
        result: &mut Vec<tsz_binder::ModuleAugmentation>,
    ) {
        use tsz_parser::parser::syntax_kind_ext::{
            INTERFACE_DECLARATION, MODULE_BLOCK, MODULE_DECLARATION,
        };
        // Helper: search a single augmentation list for nested interface declarations
        let search_augmentations =
            |augs: &[tsz_binder::ModuleAugmentation],
             arena: &tsz_parser::parser::NodeArena,
             external_arena: Option<&Arc<tsz_parser::parser::NodeArena>>,
             result: &mut Vec<tsz_binder::ModuleAugmentation>| {
                for aug in augs {
                    let Some(node) = arena.get(aug.node) else {
                        continue;
                    };
                    // Only look inside namespace (ModuleDeclaration) augmentations
                    if node.kind != MODULE_DECLARATION {
                        continue;
                    }
                    let Some(module_decl) = arena.get_module(node) else {
                        continue;
                    };
                    let Some(body_node) = arena.get(module_decl.body) else {
                        continue;
                    };
                    if body_node.kind != MODULE_BLOCK {
                        continue;
                    }
                    let Some(block) = arena.get_module_block(body_node) else {
                        continue;
                    };
                    let Some(statements) = block.statements.as_ref() else {
                        continue;
                    };
                    for &stmt_idx in &statements.nodes {
                        let Some(stmt_node) = arena.get(stmt_idx) else {
                            continue;
                        };
                        if stmt_node.kind == INTERFACE_DECLARATION {
                            if let Some(iface) = arena.get_interface(stmt_node)
                                && let Some(name_node) = arena.get(iface.name)
                                && let Some(id_data) = arena.get_identifier(name_node)
                                && id_data.escaped_text == interface_name
                            {
                                let mut nested = tsz_binder::ModuleAugmentation::new(
                                    interface_name.to_string(),
                                    stmt_idx,
                                );
                                nested.arena =
                                    external_arena.cloned().or_else(|| aug.arena.clone());
                                result.push(nested);
                            }
                        }
                    }
                }
            };
        // Search current binder's augmentations
        for candidate in candidates {
            if let Some(augmentations) = self.ctx.binder.module_augmentations.get(candidate) {
                search_augmentations(augmentations, self.ctx.arena, None, result);
            }
        }
        if !result.is_empty() {
            return;
        }
        // Search cross-file augmentations
        if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
            for candidate in candidates {
                if let Some(entries) = aug_index.get(candidate) {
                    for (file_idx, aug) in entries.iter() {
                        let aug_slice = std::slice::from_ref(aug);
                        if let Some(arenas) = self.ctx.all_arenas.as_ref()
                            && let Some(arena) = arenas.get(*file_idx)
                        {
                            search_augmentations(aug_slice, arena, Some(arena), result);
                        }
                    }
                }
            }
        } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
            for (file_idx, binder) in all_binders.iter().enumerate() {
                for candidate in candidates {
                    if let Some(augmentations) = binder.module_augmentations.get(candidate) {
                        let ext_arena = self
                            .ctx
                            .all_arenas
                            .as_ref()
                            .and_then(|arenas| arenas.get(file_idx));
                        let arena = ext_arena.map_or(self.ctx.arena, |a| a.as_ref());
                        search_augmentations(augmentations, arena, ext_arena, result);
                    }
                }
            }
        }
    }

    /// Get all module augmentation members for a given module specifier and interface name.
    ///
    /// This function retrieves the properties from augmentation declarations and returns them
    /// as `PropertyInfo` objects ready for merging with the original interface.
    ///
    /// # Arguments
    /// * `module_spec` - The module specifier (e.g., "express", "lodash")
    /// * `interface_name` - The name of the interface to find augmentation members for
    ///
    /// # Returns
    /// A vector of `PropertyInfo` representing the augmented members
    fn get_module_augmentation_members_inner(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        type_args: Option<&[TypeId]>,
    ) -> Vec<tsz_solver::PropertyInfo> {
        use tsz_parser::parser::syntax_kind_ext::{
            EXPORT_DECLARATION, FUNCTION_DECLARATION, INTERFACE_DECLARATION, METHOD_SIGNATURE,
            MODULE_BLOCK, MODULE_DECLARATION, PROPERTY_SIGNATURE, VARIABLE_STATEMENT,
        };
        use tsz_solver::PropertyInfo;
        use tsz_solver::TypeId;

        let augmentation_decls =
            self.get_module_augmentation_declarations(module_spec, interface_name);

        let mut members = Vec::new();
        let mut aug_member_order: u32 = 0;

        for augmentation in augmentation_decls {
            // Use the stored arena from the augmentation (cross-file resolution)
            let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);

            let Some(node) = arena.get(augmentation.node) else {
                continue;
            };

            if let Some(interface) = arena.get_interface(node) {
                let (interface_type_params, interface_type_param_updates) =
                    if std::ptr::eq(arena, self.ctx.arena) {
                        let (params, updates) =
                            self.push_type_parameters(&interface.type_parameters);
                        (params, Some(updates))
                    } else {
                        (Vec::new(), None)
                    };

                let interface_substitution = if let Some(type_args) = type_args
                    && !interface_type_params.is_empty()
                    && interface_type_params.len() == type_args.len()
                {
                    Some(
                        crate::query_boundaries::common::TypeSubstitution::from_args(
                            self.ctx.types,
                            &interface_type_params,
                            type_args,
                        ),
                    )
                } else {
                    None
                };

                // Extract members from interface augmentations.
                for &member_idx in &interface.members.nodes {
                    let Some(member_node) = arena.get(member_idx) else {
                        continue;
                    };

                    if (member_node.kind == PROPERTY_SIGNATURE
                        || member_node.kind == METHOD_SIGNATURE)
                        && let Some(sig) = arena.get_signature(member_node)
                        && let Some(name_node) = arena.get(sig.name)
                        && let Some(id_data) = arena.get_identifier(name_node)
                    {
                        let type_id = if std::ptr::eq(arena, self.ctx.arena) {
                            let mut type_id = self.get_type_of_interface_member_simple(member_idx);
                            if let Some(substitution) = interface_substitution.as_ref() {
                                type_id = crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    type_id,
                                    substitution,
                                );
                            }
                            type_id
                        } else {
                            self.delegate_cross_arena_interface_member_simple_type(
                                augmentation.node,
                                member_idx,
                                arena,
                                type_args,
                            )
                            .unwrap_or(TypeId::ANY)
                        };

                        aug_member_order += 1;
                        members.push(PropertyInfo {
                            name: self.ctx.types.intern_string(&id_data.escaped_text),
                            type_id,
                            write_type: type_id,
                            optional: sig.question_token,
                            readonly: self.has_readonly_modifier(&sig.modifiers),
                            is_method: member_node.kind == METHOD_SIGNATURE,
                            is_class_prototype: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                            declaration_order: aug_member_order,
                        });
                    }
                }
                if let Some(updates) = interface_type_param_updates {
                    self.pop_type_parameters(updates);
                }
                continue;
            }

            // Namespace/module augmentations contribute value members.
            if node.kind == MODULE_DECLARATION
                && let Some(module_decl) = arena.get_module(node)
                && module_decl.body.is_some()
                && let Some(body_node) = arena.get(module_decl.body)
                && body_node.kind == MODULE_BLOCK
                && let Some(block) = arena.get_module_block(body_node)
                && let Some(statements) = block.statements.as_ref()
            {
                for &stmt_idx in &statements.nodes {
                    let Some(stmt_node) = arena.get(stmt_idx) else {
                        continue;
                    };

                    match stmt_node.kind {
                        VARIABLE_STATEMENT => {
                            if let Some(var_stmt) = arena.get_variable(stmt_node) {
                                for &decl_idx in &var_stmt.declarations.nodes {
                                    if let Some(list_node) = arena.get(decl_idx)
                                        && let Some(decl_list) = arena.get_variable(list_node)
                                    {
                                        for &inner_decl_idx in &decl_list.declarations.nodes {
                                            let Some(decl_node) = arena.get(inner_decl_idx) else {
                                                continue;
                                            };
                                            let Some(decl) =
                                                arena.get_variable_declaration(decl_node)
                                            else {
                                                continue;
                                            };
                                            let Some(name_node) = arena.get(decl.name) else {
                                                continue;
                                            };
                                            let Some(id_data) = arena.get_identifier(name_node)
                                            else {
                                                continue;
                                            };

                                            let type_id = if decl.type_annotation.is_some()
                                                && std::ptr::eq(arena, self.ctx.arena)
                                            {
                                                self.get_type_of_node(decl.type_annotation)
                                            } else {
                                                TypeId::ANY
                                            };

                                            members.push(PropertyInfo {
                                                name: self
                                                    .ctx
                                                    .types
                                                    .intern_string(&id_data.escaped_text),
                                                type_id,
                                                write_type: type_id,
                                                optional: false,
                                                readonly: false,
                                                is_method: false,
                                                is_class_prototype: false,
                                                visibility: Visibility::Public,
                                                parent_id: None,
                                                declaration_order: 0,
                                            });
                                        }
                                    } else if let Some(decl_node) = arena.get(decl_idx)
                                        && let Some(decl) =
                                            arena.get_variable_declaration(decl_node)
                                        && let Some(name_node) = arena.get(decl.name)
                                        && let Some(id_data) = arena.get_identifier(name_node)
                                    {
                                        let type_id = if decl.type_annotation.is_some()
                                            && std::ptr::eq(arena, self.ctx.arena)
                                        {
                                            self.get_type_of_node(decl.type_annotation)
                                        } else {
                                            TypeId::ANY
                                        };

                                        members.push(PropertyInfo {
                                            name: self
                                                .ctx
                                                .types
                                                .intern_string(&id_data.escaped_text),
                                            type_id,
                                            write_type: type_id,
                                            optional: false,
                                            readonly: false,
                                            is_method: false,
                                            is_class_prototype: false,
                                            visibility: Visibility::Public,
                                            parent_id: None,
                                            declaration_order: 0,
                                        });
                                    }
                                }
                            }
                        }
                        FUNCTION_DECLARATION => {
                            if let Some(func) = arena.get_function(stmt_node)
                                && let Some(name_node) = arena.get(func.name)
                                && let Some(id_data) = arena.get_identifier(name_node)
                            {
                                members.push(PropertyInfo {
                                    name: self.ctx.types.intern_string(&id_data.escaped_text),
                                    type_id: TypeId::ANY,
                                    write_type: TypeId::ANY,
                                    optional: false,
                                    readonly: false,
                                    is_method: true,
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order: 0,
                                });
                            }
                        }
                        INTERFACE_DECLARATION => {
                            if let Some(iface) = arena.get_interface(stmt_node)
                                && let Some(name_node) = arena.get(iface.name)
                                && let Some(id_data) = arena.get_identifier(name_node)
                            {
                                members.push(PropertyInfo {
                                    name: self.ctx.types.intern_string(&id_data.escaped_text),
                                    type_id: TypeId::ANY,
                                    write_type: TypeId::ANY,
                                    optional: false,
                                    readonly: false,
                                    is_method: false,
                                    is_class_prototype: false,
                                    visibility: Visibility::Public,
                                    parent_id: None,
                                    declaration_order: 0,
                                });
                            }
                        }
                        EXPORT_DECLARATION => {
                            if let Some(export_decl) = arena.get_export_decl(stmt_node)
                                && export_decl.export_clause.is_some()
                                && let Some(clause_node) = arena.get(export_decl.export_clause)
                                && clause_node.kind == VARIABLE_STATEMENT
                                && let Some(var_stmt) = arena.get_variable(clause_node)
                            {
                                for &decl_idx in &var_stmt.declarations.nodes {
                                    if let Some(list_node) = arena.get(decl_idx)
                                        && let Some(decl_list) = arena.get_variable(list_node)
                                    {
                                        for &inner_decl_idx in &decl_list.declarations.nodes {
                                            let Some(decl_node) = arena.get(inner_decl_idx) else {
                                                continue;
                                            };
                                            let Some(decl) =
                                                arena.get_variable_declaration(decl_node)
                                            else {
                                                continue;
                                            };
                                            let Some(name_node) = arena.get(decl.name) else {
                                                continue;
                                            };
                                            let Some(id_data) = arena.get_identifier(name_node)
                                            else {
                                                continue;
                                            };

                                            let type_id = if decl.type_annotation.is_some()
                                                && std::ptr::eq(arena, self.ctx.arena)
                                            {
                                                self.get_type_of_node(decl.type_annotation)
                                            } else {
                                                TypeId::ANY
                                            };

                                            members.push(PropertyInfo {
                                                name: self
                                                    .ctx
                                                    .types
                                                    .intern_string(&id_data.escaped_text),
                                                type_id,
                                                write_type: type_id,
                                                optional: false,
                                                readonly: false,
                                                is_method: false,
                                                is_class_prototype: false,
                                                visibility: Visibility::Public,
                                                parent_id: None,
                                                declaration_order: 0,
                                            });
                                        }
                                    } else if let Some(decl_node) = arena.get(decl_idx)
                                        && let Some(decl) =
                                            arena.get_variable_declaration(decl_node)
                                        && let Some(name_node) = arena.get(decl.name)
                                        && let Some(id_data) = arena.get_identifier(name_node)
                                    {
                                        let type_id = if decl.type_annotation.is_some()
                                            && std::ptr::eq(arena, self.ctx.arena)
                                        {
                                            self.get_type_of_node(decl.type_annotation)
                                        } else {
                                            TypeId::ANY
                                        };

                                        members.push(PropertyInfo {
                                            name: self
                                                .ctx
                                                .types
                                                .intern_string(&id_data.escaped_text),
                                            type_id,
                                            write_type: type_id,
                                            optional: false,
                                            readonly: false,
                                            is_method: false,
                                            is_class_prototype: false,
                                            visibility: Visibility::Public,
                                            parent_id: None,
                                            declaration_order: 0,
                                        });
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        members
    }

    pub(crate) fn get_module_augmentation_members(
        &mut self,
        module_spec: &str,
        interface_name: &str,
    ) -> Vec<tsz_solver::PropertyInfo> {
        self.get_module_augmentation_members_inner(module_spec, interface_name, None)
    }

    pub(crate) fn get_module_augmentation_members_instantiated(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        type_args: &[TypeId],
    ) -> Vec<tsz_solver::PropertyInfo> {
        self.get_module_augmentation_members_inner(module_spec, interface_name, Some(type_args))
    }

    /// Apply module augmentations to an interface type.
    ///
    /// This function merges augmentation members into an existing interface type,
    /// implementing Rule #44: Module Augmentation Merging.
    ///
    /// # Arguments
    /// * `module_spec` - The module specifier being augmented
    /// * `interface_name` - The name of the interface being augmented
    /// * `base_type` - The original interface type
    ///
    /// # Returns
    /// The merged `TypeId` including augmented members
    ///
    /// # Example
    /// ```typescript
    /// // Original express types:
    /// declare module 'express' {
    ///     interface Request { body: any; }
    /// }
    ///
    /// // User augmentation:
    /// declare module 'express' {
    ///     interface Request { user: User; }
    /// }
    ///
    /// // Result: Request has both body and user properties
    /// ```
    pub(crate) fn apply_module_augmentations(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        base_type: tsz_solver::TypeId,
    ) -> tsz_solver::TypeId {
        use crate::query_boundaries::state::type_resolution as query;
        use tsz_solver::type_queries::{AugmentationTargetKind, classify_for_augmentation};
        use tsz_solver::{CallableShape, ObjectShape};

        let guard_key = (
            module_spec.to_string(),
            interface_name.to_string(),
            base_type,
        );
        {
            let mut active = self.ctx.module_augmentation_application_set.borrow_mut();
            if !active.insert(guard_key.clone()) {
                return base_type;
            }
        }
        let augmentation_members =
            self.get_module_augmentation_members(module_spec, interface_name);
        if augmentation_members.is_empty() {
            self.ctx
                .module_augmentation_application_set
                .borrow_mut()
                .remove(&guard_key);
            return base_type;
        }

        // Resolve Lazy(DefId) types to their structural representation before classifying.
        // Interface types from other files arrive as Lazy(DefId) — we need the concrete
        // Object/ObjectWithIndex/Callable shape to merge properties directly.
        let resolved_base = if let Some(def_id) = query::get_lazy_def_id(self.ctx.types, base_type)
        {
            // Look up DefId in the type environment
            if let Some(env_type) = self.ctx.type_env.borrow().get_def(def_id) {
                env_type
            } else {
                // Fall back to full evaluation
                let evaluated = self.evaluate_type_with_env(base_type);
                if evaluated != base_type && !evaluated.is_intrinsic() {
                    evaluated
                } else {
                    base_type
                }
            }
        } else {
            base_type
        };
        let kind = classify_for_augmentation(self.ctx.types, resolved_base);
        let factory = self.ctx.types.factory();

        let result = match kind {
            AugmentationTargetKind::Object(shape_id) => {
                let base_shape = self.ctx.types.object_shape(shape_id);
                let merged_properties =
                    self.merge_properties(&augmentation_members, &base_shape.properties);
                factory.object(merged_properties)
            }
            AugmentationTargetKind::ObjectWithIndex(shape_id) => {
                let base_shape = self.ctx.types.object_shape(shape_id);
                let merged_properties =
                    self.merge_properties(&augmentation_members, &base_shape.properties);
                factory.object_with_index(ObjectShape {
                    properties: merged_properties,
                    string_index: base_shape.string_index.clone(),
                    number_index: base_shape.number_index.clone(),
                    ..ObjectShape::default()
                })
            }
            AugmentationTargetKind::Callable(shape_id) => {
                let base_shape = self.ctx.types.callable_shape(shape_id);
                let prototype_name = self.ctx.types.intern_string("prototype");
                let mut merged_properties =
                    self.merge_properties(&augmentation_members, &base_shape.properties);
                if !base_shape.construct_signatures.is_empty()
                    && let Some(prototype_prop) = merged_properties
                        .iter_mut()
                        .find(|prop| prop.name == prototype_name)
                {
                    let augmented_prototype = self.apply_module_augmentations(
                        module_spec,
                        interface_name,
                        prototype_prop.type_id,
                    );
                    prototype_prop.type_id = augmented_prototype;
                    prototype_prop.write_type = augmented_prototype;
                }
                factory.callable(CallableShape {
                    call_signatures: base_shape.call_signatures.clone(),
                    construct_signatures: base_shape.construct_signatures.clone(),
                    properties: merged_properties,
                    string_index: base_shape.string_index.clone(),
                    number_index: base_shape.number_index.clone(),
                    symbol: None,
                    is_abstract: false,
                })
            }
            AugmentationTargetKind::Other => {
                // For types that still can't be decomposed after evaluation (e.g.
                // intrinsics, intersections), create an intersection of the base type
                // and a new object with the augmentation members.
                if !augmentation_members.is_empty() {
                    let aug_object = factory.object(augmentation_members);
                    factory.intersection2(base_type, aug_object)
                } else {
                    base_type
                }
            }
        };

        // Update cached types for augmentation-local symbols so that
        // self-referential type references (e.g., `self: Foo` inside
        // `declare module "./m" { interface Foo { self: Foo } }`) resolve to
        // the merged type instead of the augmentation-only type.
        // Both symbol_types and type_env must be updated because resolve_lazy
        // checks symbol_types first.
        if result != base_type {
            self.update_augmentation_local_symbol_types(module_spec, interface_name, result);
        }

        self.ctx
            .module_augmentation_application_set
            .borrow_mut()
            .remove(&guard_key);

        result
    }

    /// Update `symbol_types` and `type_env` for augmentation-local interface symbols
    /// so self-referential type references resolve to the merged type.
    /// Searches both the current binder and `all_binders` since the augmentation
    /// may be declared in a different file than the one being checked.
    fn update_augmentation_local_symbol_types(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        merged_type: tsz_solver::TypeId,
    ) {
        // Collect matching symbol IDs from all binders
        let mut matching_sym_ids = Vec::new();

        // Check current binder

        for (&aug_sym_id, aug_module) in &self.ctx.binder.augmentation_target_modules {
            if aug_module == module_spec
                && let Some(aug_sym) = self.ctx.binder.get_symbol(aug_sym_id)
                && aug_sym.escaped_name == interface_name
            {
                matching_sym_ids.push(aug_sym_id);
            }
        }

        // Check cross-file augmentations using global index for O(1) lookup
        if let Some(aug_targets) = self.ctx.global_augmentation_targets_index.as_ref() {
            if let Some(entries) = aug_targets.get(module_spec) {
                for &(aug_sym_id, _file_idx) in entries {
                    if let Some(aug_sym) = self.ctx.binder.get_symbol(aug_sym_id)
                        && aug_sym.escaped_name == interface_name
                        && !matching_sym_ids.contains(&aug_sym_id)
                    {
                        matching_sym_ids.push(aug_sym_id);
                    }
                }
            }
        } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
            // Fallback: O(N) scan when index is not available
            for binder in all_binders.iter() {
                for (&aug_sym_id, aug_module) in &binder.augmentation_target_modules {
                    if aug_module == module_spec
                        && let Some(aug_sym) = binder.get_symbol(aug_sym_id)
                        && aug_sym.escaped_name == interface_name
                        && !matching_sym_ids.contains(&aug_sym_id)
                    {
                        matching_sym_ids.push(aug_sym_id);
                    }
                }
            }
        }

        // Update symbol_types, symbol_instance_types, and type_env for each matching symbol.
        // symbol_instance_types must be updated because resolve_lazy() checks it
        // BEFORE symbol_types for INTERFACE symbols, so an un-augmented entry there
        // would shadow the updated symbol_types value.
        // Collect def IDs first (get_or_create_def_id borrows ctx mutably),
        // then batch-insert into type_env with a single borrow.
        let def_ids: Vec<_> = matching_sym_ids
            .iter()
            .map(|&aug_sym_id| {
                self.ctx.symbol_types.insert(aug_sym_id, merged_type);
                self.ctx
                    .symbol_instance_types
                    .insert(aug_sym_id, merged_type);
                self.ctx.get_or_create_def_id(aug_sym_id)
            })
            .collect();
        if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
            for aug_def_id in def_ids {
                env.insert_def(aug_def_id, merged_type);
            }
        }
    }
}
