//! Module augmentation merging (Rule #44).
//!
//! Handles `declare module 'x' { ... }` augmentations by:
//! - Discovering augmentation declarations across files
//! - Extracting augmentation members (properties, methods, variables)
//! - Merging augmented members into the target interface/namespace type
//! - Updating cached symbol types for self-referential augmentations

use crate::query_boundaries::module_augmentation as module_augmentation_boundary;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tsz_binder::{ModuleAugmentation, symbol_flags};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn module_augmentation_key_candidates(&self, module_spec: &str) -> Vec<String> {
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

        // When `trimmed` is a resolved file path that points inside a
        // node_modules directory, also push the bare package name. This lets
        // `declare module 'math2d' { ... }` apply to imports resolved to
        // `node_modules/math2d/index.d.ts`.
        if let Some(nm_pos) = trimmed.rfind("node_modules/") {
            let after_nm = &trimmed[nm_pos + "node_modules/".len()..];
            let package_name = if after_nm.starts_with('@') {
                let parts: Vec<&str> = after_nm.splitn(3, '/').collect();
                if parts.len() >= 2 {
                    format!("{}/{}", parts[0], parts[1])
                } else {
                    String::new()
                }
            } else {
                after_nm.split('/').next().unwrap_or("").to_string()
            };
            if !package_name.is_empty() {
                push_unique(&mut candidates, package_name.clone());
                push_unique(&mut candidates, format!("'{package_name}'"));
                push_unique(&mut candidates, format!("\"{package_name}\""));
            }
        }

        candidates
    }

    pub(crate) fn collect_module_augmentation_names(&self, module_spec: &str) -> Vec<String> {
        let mut names = Vec::new();
        for candidate in self.module_augmentation_key_candidates(module_spec) {
            if let Some(augmentations) = self.ctx.binder.module_augmentations.get(&candidate) {
                for aug in augmentations {
                    if !names.iter().any(|existing| existing == &aug.name) {
                        names.push(aug.name.clone());
                    }
                }
            }
            if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref()
                && let Some(entries) = aug_index.get(&candidate)
            {
                for (_, aug) in entries {
                    if !names.iter().any(|existing| existing == &aug.name) {
                        names.push(aug.name.clone());
                    }
                }
            }
        }
        names
    }

    /// Check whether any module augmentation for `module_spec` provides type
    /// parameters for `interface_name`. This handles the case where a non-generic
    /// symbol is imported from a module, and a `declare module '...'` augmentation
    /// adds a generic type alias or interface with type parameters.
    ///
    /// This is consulted by `validate_type_reference_type_arguments` to suppress
    /// false-positive TS2315 ("Type 'X' is not generic") when the base symbol has
    /// no type parameters but a module augmentation adds them.
    pub(crate) fn module_augmentation_has_type_params(
        &self,
        module_spec: &str,
        interface_name: &str,
    ) -> bool {
        let augs = self.get_module_augmentation_declarations(module_spec, interface_name);
        for aug in &augs {
            // Same-file augmentations have `aug.arena = None` (binder doesn't
            // attach a per-augmentation arena when the augmentation lives in
            // the file currently being checked). Fall back to `self.ctx.arena`
            // so this same-file path is inspected — otherwise the suppression
            // silently misses any `declare module '...' { type X<T> = … }`
            // in the same file as the usage.
            let arena: &tsz_parser::parser::NodeArena =
                aug.arena.as_deref().unwrap_or(self.ctx.arena);
            if let Some(node) = arena.get(aug.node) {
                if let Some(ta) = arena.get_type_alias(node)
                    && ta
                        .type_parameters
                        .as_ref()
                        .is_some_and(|tp| !tp.nodes.is_empty())
                {
                    return true;
                }
                if let Some(iface) = arena.get_interface(node)
                    && iface
                        .type_parameters
                        .as_ref()
                        .is_some_and(|tp| !tp.nodes.is_empty())
                {
                    return true;
                }
                if let Some(class) = arena.get_class(node)
                    && class
                        .type_parameters
                        .as_ref()
                        .is_some_and(|tp| !tp.nodes.is_empty())
                {
                    return true;
                }
            }
        }
        false
    }

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
        let candidates = self.module_augmentation_key_candidates(module_spec);
        let trimmed = module_spec.trim().trim_matches('"').trim_matches('\'');
        let resolved_source_idx = self
            .ctx
            .resolve_import_target(module_spec)
            .or_else(|| self.ctx.resolve_import_target(trimmed))
            .or_else(|| {
                self.ctx.all_arenas.as_ref().and_then(|arenas| {
                    arenas.iter().enumerate().find_map(|(idx, arena)| {
                        arena.source_files.first().and_then(|sf| {
                            (sf.file_name == module_spec || sf.file_name == trimmed).then_some(idx)
                        })
                    })
                })
            });

        let mut seen = FxHashSet::default();
        let mut push_aug = |file_idx: usize, mut aug: ModuleAugmentation| {
            if aug.name != interface_name || !seen.insert((file_idx, aug.node)) {
                return;
            }
            if aug.arena.is_none()
                && let Some(arenas) = self.ctx.all_arenas.as_ref()
                && let Some(arena) = arenas.get(file_idx)
            {
                aug.arena = Some(Arc::clone(arena));
            }
            result.push(aug);
        };

        for candidate in &candidates {
            if let Some(augmentations) = self.ctx.binder.module_augmentations.get(candidate) {
                for aug in augmentations.iter().cloned() {
                    push_aug(self.ctx.current_file_idx, aug);
                }
            }
        }

        // Use the global module augmentations index in addition to local binder
        // hits: the current file can augment the same interface as siblings.
        if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
            for candidate in &candidates {
                if let Some(entries) = aug_index.get(candidate) {
                    for (file_idx, aug) in entries.iter() {
                        push_aug(*file_idx, aug.clone());
                    }
                }
            }
        } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
            for (file_idx, binder) in all_binders.iter().enumerate() {
                for candidate in &candidates {
                    if let Some(augmentations) = binder.module_augmentations.get(candidate) {
                        for aug in augmentations.iter().cloned() {
                            push_aug(file_idx, aug);
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
            type IndexedAug = (usize, ModuleAugmentation);
            let aug_entries: Vec<(String, Vec<IndexedAug>)> =
                if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
                    aug_index
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                    let mut entries: FxHashMap<String, Vec<IndexedAug>> = FxHashMap::default();
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
            // Forward `export *` barrel direction (computed once, loop-invariant):
            // the module we are resolving the interface from (`source_idx`)
            // re-exports `interface_name`, and the chain lands on its declaring
            // file. When that declaring file is an augmentation target, the
            // augmentation must merge even though it was reached through the
            // barrel (e.g. `./c` does `export * from './a'` and the augmentation
            // targets `./a`). `resolve_export_in_file` follows wildcard and named
            // re-export chains transitively, so multi-hop barrels are covered.
            let source_export_decl_file = {
                let mut visited = rustc_hash::FxHashSet::default();
                self.resolve_export_in_file(source_idx, interface_name, &mut visited)
                    .map(|(_sym_id, decl_file_idx)| decl_file_idx)
            };
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
                let Some(aug_target_file_name) = self
                    .ctx
                    .get_arena_for_file(aug_target_idx as u32)
                    .source_files
                    .first()
                    .map(|source_file| source_file.file_name.as_str())
                else {
                    continue;
                };
                // Check if the augmentation target re-exports from source. Use
                // context accessors because the real driver stores program-wide
                // re-export maps on ProgramContext instead of cloning them into each
                // per-file binder.
                let wildcard_reexports_from_source = self
                    .ctx
                    .wildcard_reexports_for_file(aug_target_binder, aug_target_file_name)
                    .is_some_and(|sources| {
                        sources.iter().any(|(src, _is_type_only)| {
                            self.ctx
                                .resolve_import_target_from_file(aug_target_idx, src)
                                == Some(source_idx)
                        })
                    });
                let named_reexports_from_source = self
                    .ctx
                    .reexports_for_file(aug_target_binder, aug_target_file_name)
                    .is_some_and(|reexports| {
                        reexports
                            .iter()
                            .any(|(exported_name, (source_module, original_name))| {
                                let reexported_name =
                                    original_name.as_deref().unwrap_or(exported_name);
                                reexported_name == interface_name
                                    && self.ctx.resolve_import_target_from_file(
                                        aug_target_idx,
                                        source_module,
                                    ) == Some(source_idx)
                            })
                    });
                // Forward `export *` barrel direction (see `source_export_decl_file`
                // above): the interface reached through `source_idx` declares in the
                // augmentation target. Excludes the self case (`source_idx ==
                // aug_target_idx`), which the direct candidate path already handles.
                let source_reexports_aug_target =
                    source_idx != aug_target_idx && source_export_decl_file == Some(aug_target_idx);
                let reexports_from_source = wildcard_reexports_from_source
                    || named_reexports_from_source
                    || source_reexports_aug_target
                    || self
                        .resolve_cross_file_export_from_file(
                            aug_key,
                            interface_name,
                            Some(binder_idx),
                        )
                        .is_some_and(|sym_id| {
                            self.ctx.resolve_symbol_file_index(sym_id) == Some(source_idx)
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

    fn module_augmentation_source_files(&self, module_spec: &str) -> Vec<u32> {
        let candidates = self.module_augmentation_key_candidates(module_spec);
        let mut files = FxHashSet::default();

        for candidate in &candidates {
            if self.ctx.binder.module_augmentations.contains_key(candidate) {
                files.insert(self.ctx.current_file_idx as u32);
            }
        }
        if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
            for candidate in &candidates {
                if let Some(entries) = aug_index.get(candidate) {
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

    pub(crate) fn module_augmentation_value_type(
        &mut self,
        module_spec: &str,
        name: &str,
    ) -> Option<TypeId> {
        use tsz_parser::parser::syntax_kind_ext;

        for augmentation in self.get_module_augmentation_declarations(module_spec, name) {
            let arena_ref = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
            let Some(node) = arena_ref.get(augmentation.node) else {
                continue;
            };

            match node.kind {
                syntax_kind_ext::VARIABLE_DECLARATION
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION => {
                    // Resolve the declared type against the augmentation's own
                    // arena/binder (#14853). The declaration may live in a
                    // foreign arena (the augmenting file) while we are checking
                    // the consumer, so a plain `get_type_of_node` against the
                    // current arena would misread the node — delegate instead.
                    return Some(
                        self.augmentation_export_declaration_type(augmentation.node, arena_ref)
                            .unwrap_or(TypeId::ANY),
                    );
                }
                _ => {}
            }
        }

        None
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
                        if stmt_node.kind == INTERFACE_DECLARATION
                            && let Some(iface) = arena.get_interface(stmt_node)
                            && let Some(name_node) = arena.get(iface.name)
                            && let Some(id_data) = arena.get_identifier(name_node)
                            && id_data.escaped_text == interface_name
                        {
                            let mut nested = tsz_binder::ModuleAugmentation::new(
                                interface_name.to_string(),
                                stmt_idx,
                            );
                            nested.arena = external_arena.cloned().or_else(|| aug.arena.clone());
                            result.push(nested);
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
            ENUM_DECLARATION, EXPORT_DECLARATION, FUNCTION_DECLARATION, INTERFACE_DECLARATION,
            METHOD_SIGNATURE, MODULE_BLOCK, MODULE_DECLARATION, PROPERTY_SIGNATURE,
            VARIABLE_STATEMENT,
        };

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
                let delegatable_member_indices: Vec<_> = if std::ptr::eq(arena, self.ctx.arena) {
                    Vec::new()
                } else {
                    interface
                        .members
                        .nodes
                        .iter()
                        .copied()
                        .filter(|&member_idx| {
                            arena.get(member_idx).is_some_and(|member_node| {
                                member_node.kind == PROPERTY_SIGNATURE
                                    || member_node.kind == METHOD_SIGNATURE
                            })
                        })
                        .collect()
                };
                let delegated_member_types = if delegatable_member_indices.is_empty() {
                    None
                } else {
                    self.delegate_cross_arena_interface_member_simple_types(
                        augmentation.node,
                        &delegatable_member_indices,
                        arena,
                        type_args,
                        true,
                    )
                };

                // Extract members from interface augmentations.
                for &member_idx in &interface.members.nodes {
                    let Some(member_node) = arena.get(member_idx) else {
                        continue;
                    };

                    if (member_node.kind == PROPERTY_SIGNATURE
                        || member_node.kind == METHOD_SIGNATURE)
                        && let Some(sig) = arena.get_signature(member_node)
                        && let Some(member_name) =
                            self.augmentation_member_key_name(arena, sig.name)
                    {
                        let type_id = if std::ptr::eq(arena, self.ctx.arena) {
                            let mut type_id = if member_node.kind == PROPERTY_SIGNATURE
                                && sig.type_annotation.is_some()
                                && let Some(self_ref_type) = self
                                    .module_augmentation_self_reference_type(
                                        module_spec,
                                        interface_name,
                                        sig.type_annotation,
                                    ) {
                                self_ref_type
                            } else {
                                self.get_type_of_interface_member_simple(member_idx)
                            };
                            if let Some(substitution) = interface_substitution.as_ref() {
                                type_id = crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    type_id,
                                    substitution,
                                );
                            }
                            type_id
                        } else {
                            delegated_member_types
                                .as_ref()
                                .and_then(|types| types.get(&member_idx).copied())
                                .unwrap_or(TypeId::ANY)
                        };

                        aug_member_order += 1;
                        members.push(module_augmentation_boundary::augmentation_member_property(
                            self.ctx.types.intern_string(&member_name),
                            type_id,
                            sig.question_token,
                            self.has_readonly_modifier(&sig.modifiers),
                            member_node.kind == METHOD_SIGNATURE,
                            aug_member_order,
                        ));
                    }
                }
                if let Some(updates) = interface_type_param_updates {
                    self.pop_type_parameters(updates);
                }
                continue;
            }

            if node.kind == ENUM_DECLARATION
                && let Some(enum_decl) = arena.get_enum(node)
            {
                for &member_idx in &enum_decl.members.nodes {
                    let Some(member_node) = arena.get(member_idx) else {
                        continue;
                    };
                    let Some(member) = arena.get_enum_member(member_node) else {
                        continue;
                    };
                    let Some(name_node) = arena.get(member.name) else {
                        continue;
                    };
                    let member_name = arena
                        .get_identifier(name_node)
                        .map(|ident| ident.escaped_text.to_string())
                        .or_else(|| arena.get_literal(name_node).map(|lit| lit.text.clone()));
                    let Some(member_name) = member_name else {
                        continue;
                    };

                    aug_member_order += 1;
                    members.push(
                        module_augmentation_boundary::augmentation_any_member_property(
                            self.ctx.types.intern_string(&member_name),
                            true,
                            false,
                            aug_member_order,
                        ),
                    );
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

                                            members.push(
                                                module_augmentation_boundary::augmentation_value_member_property(
                                                    self.ctx.types.intern_string(
                                                        &id_data.escaped_text,
                                                    ),
                                                    type_id,
                                                ),
                                            );
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

                                        members.push(
                                            module_augmentation_boundary::augmentation_value_member_property(
                                                self.ctx
                                                    .types
                                                    .intern_string(&id_data.escaped_text),
                                                type_id,
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                        FUNCTION_DECLARATION => {
                            if let Some(func) = arena.get_function(stmt_node)
                                && let Some(name_node) = arena.get(func.name)
                                && let Some(id_data) = arena.get_identifier(name_node)
                            {
                                members.push(
                                    module_augmentation_boundary::augmentation_any_method_member_property(
                                        self.ctx.types.intern_string(&id_data.escaped_text),
                                    ),
                                );
                            }
                        }
                        INTERFACE_DECLARATION => {
                            if let Some(iface) = arena.get_interface(stmt_node)
                                && let Some(name_node) = arena.get(iface.name)
                                && let Some(id_data) = arena.get_identifier(name_node)
                            {
                                members.push(
                                    module_augmentation_boundary::augmentation_any_value_member_property(
                                        self.ctx.types.intern_string(&id_data.escaped_text),
                                    ),
                                );
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

                                            members.push(
                                                module_augmentation_boundary::augmentation_value_member_property(
                                                    self.ctx.types.intern_string(
                                                        &id_data.escaped_text,
                                                    ),
                                                    type_id,
                                                ),
                                            );
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

                                        members.push(
                                            module_augmentation_boundary::augmentation_value_member_property(
                                                self.ctx
                                                    .types
                                                    .intern_string(&id_data.escaped_text),
                                                type_id,
                                            ),
                                        );
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

    /// Preserve `self: Foo` inside `declare module "./m" { interface Foo { ... } }`
    /// as a Lazy DefId so the post-merge cache update can redirect it to the merged type.
    fn module_augmentation_self_reference_type(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        type_annotation: tsz_parser::parser::NodeIndex,
    ) -> Option<TypeId> {
        use crate::symbol_resolver::TypeSymbolResolution;
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

        let sym_id = match self.resolve_identifier_symbol_in_type_position(type_ref.type_name) {
            TypeSymbolResolution::Type(sym_id) => sym_id,
            TypeSymbolResolution::ValueOnly(_) | TypeSymbolResolution::NotFound => return None,
        };
        let aug_module = self.ctx.binder.augmentation_target_modules.get(&sym_id)?;
        if aug_module != module_spec
            && !self
                .module_augmentation_key_candidates(module_spec)
                .iter()
                .any(|candidate| candidate == aug_module)
        {
            return None;
        }

        let base_type = self.ctx.create_lazy_type_ref(sym_id);
        let type_args = type_ref
            .type_arguments
            .as_ref()
            .map(|args| {
                args.nodes
                    .iter()
                    .map(|&arg_idx| self.get_type_from_type_node_in_type_literal(arg_idx))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if type_args.is_empty() {
            Some(base_type)
        } else {
            Some(
                module_augmentation_boundary::self_reference_application_type(
                    self.ctx.types,
                    base_type,
                    type_args,
                ),
            )
        }
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
        use crate::query_boundaries::common::{AugmentationTargetKind, classify_for_augmentation};
        use crate::query_boundaries::state::type_resolution as query;

        // Fast-path: avoids string allocations and hashset bookkeeping when
        // no augmentations are registered anywhere in the program.
        if !self.ctx.program_has_module_augmentations() {
            return base_type;
        }

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
        let base_def_id = query::get_lazy_def_id(self.ctx.types, base_type);
        let resolved_base = if let Some(def_id) = base_def_id {
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
        // #14344 / #14345 producer: the HOME interface symbol carried by the
        // base shape, captured when the base is an EMPTY pre-merge object
        // snapshot (the fp-ts `URItoKindN` registry — zero own members). The
        // merged body is published below and the home-symbol → home-`DefId`
        // redirect edge is recorded so the solver's index-reduction consumer
        // can re-index it. Structural: keyed on the base shape's symbol and
        // emptiness, never a name/file string.
        let empty_base_home_symbol: Option<tsz_binder::SymbolId> = match kind {
            AugmentationTargetKind::Object(shape_id) => {
                let base_shape = self.ctx.types.object_shape(shape_id);
                (base_shape.properties.is_empty())
                    .then_some(base_shape.symbol)
                    .flatten()
            }
            AugmentationTargetKind::ObjectWithIndex(shape_id) => {
                let base_shape = self.ctx.types.object_shape(shape_id);
                (base_shape.properties.is_empty()
                    && base_shape.string_index.is_none()
                    && base_shape.number_index.is_none()
                    && base_shape.symbol_index.is_none())
                .then_some(base_shape.symbol)
                .flatten()
            }
            _ => None,
        };

        let result = match kind {
            AugmentationTargetKind::Object(shape_id) => {
                let base_shape = self.ctx.types.object_shape(shape_id);
                let merged_properties = self.merge_properties(
                    &augmentation_members,
                    &base_shape.properties,
                    crate::interface_type::InterfaceMergeMode::Declaration,
                );
                if let (Some(symbol), Some(def_id)) = (base_shape.symbol, base_def_id) {
                    self.ctx
                        .definition_store
                        .register_module_augmentation_symbol_def_if_enabled(symbol.0, def_id);
                }
                // Preserve the base interface's nominal identity (symbol) and
                // object-level flags so the augmented type keeps its canonical
                // declaration name (e.g. `Tool` rather than an expanded
                // `{ ... }` literal) and stays a single interned identity.
                let augmented = self
                    .ctx
                    .types
                    .factory()
                    .object_with_shape_metadata(merged_properties, &base_shape);
                if let Some(def_id) = base_def_id
                    && base_shape.symbol.is_some()
                    && self
                        .ctx
                        .definition_store
                        .register_module_augmented_body_if_enabled(
                            def_id,
                            resolved_base,
                            augmented,
                            self.ctx.types,
                            &self.module_augmentation_source_files(module_spec),
                        )
                {
                    self.ctx.clear_type_evaluation_caches_for_def(def_id);
                }
                augmented
            }
            AugmentationTargetKind::ObjectWithIndex(shape_id) => {
                let base_shape = self.ctx.types.object_shape(shape_id);
                let merged_properties = self.merge_properties(
                    &augmentation_members,
                    &base_shape.properties,
                    crate::interface_type::InterfaceMergeMode::Declaration,
                );
                if let (Some(symbol), Some(def_id)) = (base_shape.symbol, base_def_id) {
                    self.ctx
                        .definition_store
                        .register_module_augmentation_symbol_def_if_enabled(symbol.0, def_id);
                }
                self.ctx
                    .types
                    .factory()
                    .object_with_shape_metadata(merged_properties, &base_shape)
            }
            AugmentationTargetKind::Callable(shape_id) => {
                let base_shape = self.ctx.types.callable_shape(shape_id);
                let properties = if base_shape.construct_signatures.is_empty() {
                    // Non-constructor callable (namespace, function): merge
                    // augmentation members as direct properties.
                    self.merge_properties(
                        &augmentation_members,
                        &base_shape.properties,
                        crate::interface_type::InterfaceMergeMode::Declaration,
                    )
                } else {
                    // Class constructor: augmentation members belong on the
                    // prototype (instance type), not as static properties of
                    // the constructor itself.
                    let prototype_name = self.ctx.types.intern_string("prototype");
                    let mut properties = base_shape.properties.clone();
                    if let Some(prototype_prop) = properties
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
                    properties
                };
                module_augmentation_boundary::augmented_callable_type(
                    self.ctx.types,
                    base_shape.call_signatures.clone(),
                    base_shape.construct_signatures.clone(),
                    properties,
                    // Preserve the callable's nominal identity and abstractness
                    // so the augmented class/namespace keeps its declaration name.
                    base_shape.string_index,
                    base_shape.number_index,
                    base_shape.symbol,
                    base_shape.is_abstract,
                )
            }
            AugmentationTargetKind::Other => {
                // For types that still can't be decomposed after evaluation (e.g.
                // intrinsics, intersections), create an intersection of the base type
                // and a new object with the augmentation members.
                module_augmentation_boundary::other_target_with_augmentation_members(
                    self.ctx.types,
                    base_type,
                    augmentation_members,
                )
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

            // #14344 / #14345 producer: when the augmented base was an EMPTY
            // pre-merge snapshot (the HKT `URItoKindN` registry), publish the
            // merged body under the home interface's own `DefId` and record the
            // home-symbol → home-`DefId` redirect edge so the solver's
            // index-reduction consumer can map a frozen empty `shape.symbol`
            // back to the populated home def. Flag-gated (default-OFF); the
            // store's `_if_enabled` write-through is a no-op when the flag is
            // OFF, so flag-OFF is byte-parity.
            if let Some(home_symbol) = empty_base_home_symbol {
                self.publish_augmented_base_body_redirect(home_symbol, result);
            }
        }

        self.ctx
            .module_augmentation_application_set
            .borrow_mut()
            .remove(&guard_key);

        result
    }

    /// Resolve an augmentation member's property-key name from the augmenting
    /// file's own arena/binder, purely syntactically.
    ///
    /// Plain identifier, string-literal, and numeric-literal keys are read
    /// directly. A computed key `[expr]` is resolved when `expr` is a string
    /// literal or an identifier bound to a string-initialized `const` in the
    /// augmenting file — the fp-ts `const URI = "X"; interface I { [URI]: ... }`
    /// pattern. The resolution is deliberately syntactic (no expression type
    /// evaluation): augmentation member collection runs *inside* interface body
    /// resolution, so calling the evaluating `get_property_name_resolved` here
    /// would re-enter interface resolution and recurse. It also works uniformly
    /// for cross-arena augmentations whose key `const` lives in another file's
    /// binder (#13653).
    fn augmentation_member_key_name(
        &mut self,
        arena: &tsz_parser::parser::NodeArena,
        name_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<String> {
        Self::augmentation_member_key_name_in_arena(arena, name_idx, |ident_name| {
            let binder = self.ctx.get_binder_for_arena(arena)?;
            let sym_id = binder.file_locals.get(ident_name)?;
            binder
                .get_symbol(sym_id)
                .and_then(|symbol| arena.get(symbol.value_declaration))
                .and_then(|decl| arena.get_variable_declaration(decl))
                .and_then(|var_decl| arena.get(var_decl.initializer))
                .and_then(|init| arena.get_literal(init))
                .map(|lit| lit.text.clone())
        })
    }

    /// Pure-arena key resolution shared by all augmentation paths.
    /// `resolve_const` maps a computed-key identifier to its string-`const`
    /// value (so the same matching can be unit-tested without a live binder).
    fn augmentation_member_key_name_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        name_idx: tsz_parser::parser::NodeIndex,
        resolve_const: impl FnOnce(&str) -> Option<String>,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME;
        let name_node = arena.get(name_idx)?;
        if let Some(ident) = arena.get_identifier(name_node) {
            return Some(ident.escaped_text.to_string());
        }
        if let Some(lit) = arena.get_literal(name_node) {
            return Some(lit.text.clone());
        }
        if name_node.kind == COMPUTED_PROPERTY_NAME {
            let computed = arena.get_computed_property(name_node)?;
            let expr_node = arena.get(computed.expression)?;
            if let Some(lit) = arena.get_literal(expr_node) {
                return Some(lit.text.clone());
            }
            if let Some(ident) = arena.get_identifier(expr_node) {
                return resolve_const(ident.escaped_text.as_str());
            }
        }
        None
    }

    /// Merge `declare module "./home" { interface I { ... } }` augmentations into
    /// the type of interface `I` declared (and exported) in its own home module,
    /// even when `I` is reached by reference within its declaring file rather than
    /// through an import alias.
    ///
    /// This is the fp-ts higher-kinded-types pattern: a central `URItoKind`
    /// interface in `./HKT` is augmented from many sibling files via
    /// `declare module "./HKT" { interface URItoKind<A> { readonly Foo: ... } }`.
    /// Computing `keyof URItoKind` (for the `Kind`/`URIS` constraint) must see the
    /// cross-file registered members. The import-driven augmentation path only
    /// fires for symbols carrying an `import_module()` specifier, so a self-module
    /// interface reference never picked them up.
    ///
    /// Gated on `is_exported`: a file-local (non-exported) interface is not part
    /// of the module's augmentable export surface, so a same-file
    /// `declare module "./self"` augmentation stays an independent symbol and must
    /// not merge into it (#6164). Symbols already reached through an import alias
    /// are left to the import path to avoid double application.
    pub(crate) fn apply_self_module_augmentations(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        base_type: TypeId,
    ) -> TypeId {
        if base_type == TypeId::ERROR
            || base_type == TypeId::UNKNOWN
            || !self.ctx.program_has_module_augmentations()
        {
            return base_type;
        }
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return base_type;
        };
        // The import-driven path (`type_reference_symbol_type`) already applies
        // augmentations for symbols reached through an import alias.
        if symbol.import_module().is_some() {
            return base_type;
        }
        // Only exported interfaces participate in their module's augmentable
        // surface; a non-exported file-local interface keeps its self-module
        // augmentation as a separate symbol (#6164).
        if !symbol.is_exported {
            return base_type;
        }
        let interface_name = symbol.escaped_name.clone();
        let home_idx = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .unwrap_or(self.ctx.current_file_idx);
        let Some(home_file_name) = self
            .ctx
            .get_arena_for_file(home_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return base_type;
        };
        self.apply_module_augmentations(&home_file_name, &interface_name, base_type)
    }

    /// Fold `declare global { interface X { ... } }` declarations into the
    /// materialized body of a user-declared global interface `X`, so every
    /// type-level consumer (`keyof X`, `X[K]`, assignability, type display)
    /// observes the SAME merged shape that value-position member access already
    /// reaches through the global-augmentation channel — independent of
    /// declaration or file order.
    ///
    /// Each `declare global { ... }` block binds its interface to a SEPARATE
    /// symbol: the binder restores the boundary scope between augmentation
    /// blocks so they cannot shadow lib globals, which also prevents repeated
    /// `declare global { interface X }` blocks (and cross-file `declare global`s)
    /// from declaration-merging into one symbol. Member lookup reunites those
    /// partial symbols through `global_augmentations`, but a bare type reference
    /// resolves just one of them, leaving `keyof X` / `X["k"]` blind to the
    /// other blocks' members (false `TS2339`/`TS2536`/`TS2344`, and a `keyof`
    /// that drops keys). This mirrors `apply_self_module_augmentations` for the
    /// `declare module` case, reusing the global-augmentation lowering helpers
    /// (`lower_augmentation_for_arena` / `combine_augmentation_with_lib`) over
    /// declarations gathered from every binder in the program.
    pub(crate) fn apply_self_global_augmentations(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        base_type: TypeId,
    ) -> TypeId {
        use tsz_parser::parser::{NodeArena, NodeIndex, syntax_kind_ext};
        use tsz_scanner::SyntaxKind;

        fn is_direct_declare_global_member(arena: &NodeArena, node: NodeIndex) -> bool {
            let Some(block_idx) = arena.parent_of(node) else {
                return false;
            };
            let Some(block) = arena.get(block_idx) else {
                return false;
            };
            if block.kind != syntax_kind_ext::MODULE_BLOCK {
                return false;
            }
            let Some(module_idx) = arena.parent_of(block_idx) else {
                return false;
            };
            let Some(module_node) = arena.get(module_idx) else {
                return false;
            };
            if module_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                return false;
            }
            let Some(module) = arena.get_module(module_node) else {
                return false;
            };
            // The `global` in `declare global { ... }` is parsed either as a
            // dedicated `GlobalKeyword` token or as an `Identifier` whose text
            // is "global" (soft keyword). Accept both, mirroring the binder's
            // `declare global` detection (`tsz_binder::state::core`); checking
            // only `GlobalKeyword` dropped the Identifier form, so the merged
            // interface body was never folded/published to the symbol's `DefId`
            // and indexed access `X[K]` read a single block's partial members.
            arena.get(module.name).is_some_and(|name| {
                name.kind == SyntaxKind::GlobalKeyword as u16
                    || arena
                        .get_identifier(name)
                        .is_some_and(|ident| ident.escaped_text == "global")
            })
        }

        if base_type == TypeId::ERROR
            || base_type == TypeId::UNKNOWN
            || !self.ctx.program_has_global_augmentations()
        {
            return base_type;
        }
        // Lib/global builtin interfaces (e.g. `Array`, `Symbol`) already fold
        // their global augmentations through the lib-resolution path; folding
        // again here would double-apply.
        if self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
            || self.ctx.binder.lib_symbol_ids.contains(&sym_id)
        {
            return base_type;
        }
        let (name, own_declarations) = {
            let Some(symbol) = self
                .ctx
                .binder
                .get_symbol(sym_id)
                .or_else(|| self.get_cross_file_symbol(sym_id))
            else {
                return base_type;
            };
            // Imported aliases get their augmentations applied on the import
            // path; leave them to it to avoid double application.
            if symbol.import_module().is_some() {
                return base_type;
            }
            if !symbol.has_any_flags(symbol_flags::INTERFACE) {
                return base_type;
            }
            (symbol.escaped_name.clone(), symbol.all_declarations())
        };

        // Gather every `declare global { interface <name> }` declaration in the
        // program, grouped by the arena it lives in so each group is lowered
        // against its own arena/binder. Two sources contribute:
        //   * the current binder (current-file entries lower against
        //     `self.ctx.arena`; cross-file entries carry their own arena), and
        //   * every cross-file binder in `all_binders`, paired with its arena.
        // Pairing across `all_binders`/`all_arenas` makes the fold correct
        // whether or not the program was loaded through a single pre-aggregated
        // primary binder.
        let current_arena_ptr = std::ptr::from_ref::<NodeArena>(self.ctx.arena) as usize;
        let mut current_decls: Vec<NodeIndex> = Vec::new();
        let mut cross_groups: FxHashMap<usize, (Arc<NodeArena>, Vec<NodeIndex>)> =
            FxHashMap::default();
        let mut is_self_global_aug = false;

        if let Some(aug_decls) = self.ctx.binder.global_augmentations.get(&name) {
            for aug in aug_decls {
                if !is_direct_declare_global_member(
                    aug.arena.as_deref().unwrap_or(self.ctx.arena),
                    aug.node,
                ) {
                    continue;
                }
                if own_declarations.contains(&aug.node) {
                    is_self_global_aug = true;
                }
                match aug.arena {
                    Some(ref arena) => {
                        cross_groups
                            .entry(Arc::as_ptr(arena) as usize)
                            .or_insert_with(|| (Arc::clone(arena), Vec::new()))
                            .1
                            .push(aug.node);
                    }
                    None => current_decls.push(aug.node),
                }
            }
        }

        if let (Some(all_binders), Some(all_arenas)) =
            (self.ctx.all_binders.clone(), self.ctx.all_arenas.clone())
        {
            for (binder, arena) in all_binders.iter().zip(all_arenas.iter()) {
                // The current binder's entries were already gathered above; when
                // it is also one of `all_binders` (single-checker programs)
                // re-reading it would duplicate every current-file declaration.
                if std::ptr::eq(binder.as_ref(), self.ctx.binder) {
                    continue;
                }
                let Some(aug_decls) = binder.global_augmentations.get(&name) else {
                    continue;
                };
                let arena_ptr = Arc::as_ptr(arena) as usize;
                for aug in aug_decls {
                    // Entries in a cross-file binder are current-file relative to
                    // THAT binder, so their declaration nodes belong to `arena`.
                    if aug.arena.is_some() {
                        continue;
                    }
                    if !is_direct_declare_global_member(arena.as_ref(), aug.node) {
                        continue;
                    }
                    if own_declarations.contains(&aug.node) {
                        is_self_global_aug = true;
                    }
                    if arena_ptr == current_arena_ptr {
                        current_decls.push(aug.node);
                    } else {
                        cross_groups
                            .entry(arena_ptr)
                            .or_insert_with(|| (Arc::clone(arena), Vec::new()))
                            .1
                            .push(aug.node);
                    }
                }
            }
        }

        // Only fold when THIS symbol is itself one of the `declare global`
        // interface declarations of `name`. A module-scoped `interface X` that
        // merely shares its name with a `declare global { interface X }` must
        // NOT absorb the global members — they live in different declaration
        // scopes in `tsc`.
        if !is_self_global_aug {
            return base_type;
        }

        let lib_contexts = self.ctx.lib_contexts.clone();
        let mut result = base_type;
        if !current_decls.is_empty() {
            let aug_type =
                self.lower_augmentation_for_arena(self.ctx.arena, &current_decls, &lib_contexts);
            result = self.combine_augmentation_with_lib(Some(result), aug_type);
        }
        // Fold the cross-arena `declare global` groups in a deterministic
        // program order. `cross_groups` is keyed on each arena's raw pointer
        // address, so iterating it directly (`into_values`) folds in address
        // order — a memory-layout-dependent sequence that varies with ASLR
        // across otherwise-identical runs. `combine_augmentation_with_lib`
        // (via `merge_interface_types`) is order-sensitive: the merged
        // interface's member and overload-signature order — and therefore its
        // interned `TypeId` identity — depends on the fold order. An
        // address-ordered fold thus splits one merged lib interface (e.g. the
        // globally augmented `Document`/`HTMLElement`) into distinct identities
        // across runs, which later meet in a relation and mis-fire TS2345/
        // TS2430. Sort by the owning file index (with the source file name as a
        // stable tiebreaker) so the fold follows tsc's program-declaration
        // order regardless of memory layout.
        let mut ordered_groups: Vec<(Arc<NodeArena>, Vec<NodeIndex>)> =
            cross_groups.into_values().collect();
        ordered_groups.sort_by_cached_key(|(arena, _)| {
            (
                self.ctx
                    .get_file_idx_for_arena(arena.as_ref())
                    .unwrap_or(usize::MAX),
                arena
                    .source_files
                    .first()
                    .map(|sf| sf.file_name.clone())
                    .unwrap_or_default(),
            )
        });
        for (arena, decls) in ordered_groups {
            let aug_type = self.lower_augmentation_for_arena(arena.as_ref(), &decls, &lib_contexts);
            result = self.combine_augmentation_with_lib(Some(result), aug_type);
        }

        // Publish the merged body to the symbol's `DefId` so the solver's
        // `Lazy(DefId)`-driven consumers — `keyof`, indexed access, constraint
        // satisfaction — resolve the merged shape, not the partial body that
        // `compute_type_of_symbol` registered from a single block. Mirrors
        // `update_augmentation_local_symbol_types` for the `declare module` case.
        if result != base_type {
            use crate::query_boundaries::state::type_environment;
            let def_id = self.ctx.get_or_create_def_id(sym_id);
            self.ctx.symbol_types.insert(sym_id, result);
            self.ctx.symbol_instance_types.insert(sym_id, result);
            if let Some(shape) = type_environment::object_shape(self.ctx.types, result) {
                self.ctx.definition_store.set_instance_shape(def_id, shape);
            }
            self.ctx
                .register_augmented_def_in_envs(def_id, result, false);
        }
        result
    }

    /// Update `symbol_types` and both environments for augmentation-local interface symbols
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

        for (&aug_sym_id, aug_module) in self.ctx.binder.augmentation_target_modules.iter() {
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
                for &(aug_sym_id, file_idx) in entries {
                    if let Some(aug_sym) = self
                        .ctx
                        .get_binder_for_file(file_idx)
                        .and_then(|binder| binder.get_symbol(aug_sym_id))
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
                for (&aug_sym_id, aug_module) in binder.augmentation_target_modules.iter() {
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

        // Update symbol_types, symbol_instance_types, and env mappings for each matching symbol.
        // symbol_instance_types must be updated because resolve_lazy() checks it
        // BEFORE symbol_types for INTERFACE symbols, so an un-augmented entry there
        // would shadow the updated symbol_types value.
        // Collect def IDs first (get_or_create_def_id borrows ctx mutably),
        // then publish them through the context-owned dual-env authority.
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
        for aug_def_id in def_ids {
            self.ctx
                .register_augmented_def_in_envs(aug_def_id, merged_type, false);
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::state::CheckerState;
    use std::sync::Arc;
    use tsz_binder::{BinderState, ModuleAugmentation};
    use tsz_parser::parser::{NodeArena, ParserState};

    /// `augmentation_member_key_name_in_arena` resolves identifier, string-literal,
    /// and computed string-`const` property keys, and leaves an unresolvable
    /// computed key as `None`. The const resolver is renamed (`TAG`, not a
    /// hard-coded fp-ts name) to lock the structural, name-independent contract.
    #[test]
    fn augmentation_member_key_name_resolves_identifier_string_and_computed_const() {
        let source = r#"
interface I {
    foo: number;
    "bar": number;
    [TAG]: number;
    [OTHER]: number;
}
"#;
        let mut parser = ParserState::new("t.ts".to_string(), source.to_string());
        parser.parse_source_file();
        let arena = Arc::new(parser.into_arena());
        let sf = arena.source_files.first().expect("source file");
        let iface_idx = sf
            .statements
            .nodes
            .iter()
            .copied()
            .find(|&idx| {
                arena
                    .get(idx)
                    .and_then(|n| arena.get_interface(n))
                    .is_some()
            })
            .expect("interface node");
        let iface = arena
            .get_interface(arena.get(iface_idx).expect("iface node"))
            .expect("interface data");

        let resolved: Vec<Option<String>> = iface
            .members
            .nodes
            .iter()
            .copied()
            .filter_map(|member_idx| {
                let member = arena.get(member_idx)?;
                let sig = arena.get_signature(member)?;
                Some(CheckerState::augmentation_member_key_name_in_arena(
                    &arena,
                    sig.name,
                    // Only `TAG` is a known string const; `OTHER` is unknown.
                    |name| (name == "TAG").then(|| "computed_tag".to_string()),
                ))
            })
            .collect();

        assert_eq!(
            resolved,
            vec![
                Some("foo".to_string()),
                Some("bar".to_string()),
                Some("computed_tag".to_string()),
                None,
            ],
            "key resolver must handle identifier, string-literal, and computed \
             string-const keys, and drop unresolvable computed keys"
        );
    }

    #[test]
    fn module_augmentation_has_type_params_detects_type_alias_with_params() {
        // Set up a binder with a module augmentation that has a generic type alias.
        let mut binder = BinderState::new();
        let aug_name = "Row2".to_string();

        // Parse a type alias `type Row2<T> = {}` to get a node with type params.
        let source = "type Row2<T> = {}";
        let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
        parser.parse_source_file();
        let arena = Arc::new(parser.into_arena());

        // Find the type alias declaration node
        let sf = arena.source_files.first().expect("source file");
        let type_alias_node = sf
            .statements
            .nodes
            .iter()
            .copied()
            .find(|&idx| {
                arena
                    .get(idx)
                    .and_then(|n| arena.get_type_alias(n))
                    .is_some()
            })
            .expect("type alias node");

        // Register a module augmentation with the arena
        let aug = ModuleAugmentation::with_arena(aug_name, type_alias_node, Arc::clone(&arena));
        Arc::get_mut(&mut binder.module_augmentations)
            .expect("fresh Arc")
            .insert(".".to_string(), vec![aug]);

        // Set up CheckerState with the binder
        let types = tsz_solver::construction::TypeInterner::new();
        let main_arena = Arc::new(NodeArena::new());
        let checker = CheckerState::new(
            &main_arena,
            &binder,
            &types,
            "test.ts".to_string(),
            Default::default(),
        );

        assert!(
            checker.module_augmentation_has_type_params(".", "Row2"),
            "Should detect type params in module augmentation for '.' Row2"
        );
        assert!(
            !checker.module_augmentation_has_type_params(".", "Nonexistent"),
            "Should not detect type params for non-existent name"
        );
        assert!(
            !checker.module_augmentation_has_type_params("./other", "Row2"),
            "Should not detect type params for non-matching module specifier"
        );
    }

    #[test]
    fn module_augmentation_has_type_params_rejects_non_generic_interface() {
        let mut binder = BinderState::new();
        let aug_name = "Foo".to_string();

        let source = "interface Foo {}";
        let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
        parser.parse_source_file();
        let arena = Arc::new(parser.into_arena());

        let sf = arena.source_files.first().expect("source file");
        let iface_node = sf
            .statements
            .nodes
            .iter()
            .copied()
            .find(|&idx| {
                arena
                    .get(idx)
                    .and_then(|n| arena.get_interface(n))
                    .is_some()
            })
            .expect("interface node");

        let aug = ModuleAugmentation::with_arena(aug_name, iface_node, Arc::clone(&arena));
        Arc::get_mut(&mut binder.module_augmentations)
            .expect("fresh Arc")
            .insert(".".to_string(), vec![aug]);

        let types = tsz_solver::construction::TypeInterner::new();
        let main_arena = Arc::new(NodeArena::new());
        let checker = CheckerState::new(
            &main_arena,
            &binder,
            &types,
            "test.ts".to_string(),
            Default::default(),
        );

        assert!(
            !checker.module_augmentation_has_type_params(".", "Foo"),
            "Should NOT detect type params for non-generic interface"
        );
    }
}
