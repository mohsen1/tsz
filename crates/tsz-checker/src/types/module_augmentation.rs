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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModuleAugmentationSpace {
    Type,
    Value,
}

impl<'a> CheckerState<'a> {
    pub(crate) fn module_augmentation_key_candidates(&self, module_spec: &str) -> Vec<String> {
        let mut candidates = crate::module_resolution::module_specifier_candidates(module_spec);

        fn push_unique(candidates: &mut Vec<String>, candidate: String) {
            if !candidate.is_empty() && !candidates.iter().any(|existing| existing == &candidate) {
                candidates.push(candidate);
            }
        }

        let trimmed = module_spec.trim().trim_matches('"').trim_matches('\'');
        let mut resolved_source_idx = trimmed
            .strip_prefix("file_idx:")
            .and_then(|file_idx| file_idx.parse::<usize>().ok());
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
                            entries.iter().map(|(file_idx, _)| (key.clone(), *file_idx))
                        })
                        .collect()
                } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                    let mut keys = Vec::new();
                    for (file_idx, binder) in all_binders.iter().enumerate() {
                        for aug_key in binder.module_augmentations.keys() {
                            keys.push((aug_key.clone(), file_idx));
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
                trimmed
                    .strip_prefix("file_idx:")
                    .and_then(|file_idx| file_idx.parse::<usize>().ok())
            })
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

        // Program order is authoritative; current-first made overload order
        // depend on which file asked the question.
        if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
            let mut indexed = Vec::new();
            for candidate in &candidates {
                if let Some(entries) = aug_index.get(candidate) {
                    for (file_idx, aug) in entries.iter() {
                        indexed.push((*file_idx, aug.clone()));
                    }
                }
            }
            indexed.sort_by_key(|(file_idx, aug)| {
                let arena = aug.arena.as_deref().or_else(|| {
                    self.ctx
                        .all_arenas
                        .as_ref()
                        .and_then(|arenas| arenas.get(*file_idx))
                        .map(AsRef::as_ref)
                });
                let position = arena
                    .and_then(|arena| arena.get(aug.node))
                    .map_or(u32::MAX, |node| node.pos);
                (*file_idx, position)
            });
            for (file_idx, aug) in indexed {
                push_aug(file_idx, aug);
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

        // Add unindexed current-file entries without moving indexed entries.
        for candidate in &candidates {
            if let Some(augmentations) = self.ctx.binder.module_augmentations.get(candidate) {
                for aug in augmentations.iter().cloned() {
                    push_aug(self.ctx.current_file_idx, aug);
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
                if candidates.iter().any(|candidate| candidate == aug_key) {
                    continue;
                }
                if !indexed_augs
                    .iter()
                    .any(|(_, aug)| aug.name == interface_name)
                {
                    continue;
                }
                let Some(&(binder_idx, _)) = indexed_augs.first() else {
                    continue;
                };
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
                            self.ctx
                                .resolve_dynamic_symbol_file_index(sym_id)
                                .or_else(|| self.ctx.resolve_symbol_file_index(sym_id))
                                == Some(source_idx)
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

        result.sort_by_key(|augmentation| {
            let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
            let file_idx = self
                .ctx
                .get_file_idx_for_arena(arena)
                .unwrap_or(self.ctx.current_file_idx);
            let position = arena
                .get(augmentation.node)
                .map_or(u32::MAX, |node| node.pos);
            (file_idx, position)
        });
        let mut ordered_seen = FxHashSet::default();
        result.retain(|augmentation| {
            let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
            let file_idx = self
                .ctx
                .get_file_idx_for_arena(arena)
                .unwrap_or(self.ctx.current_file_idx);
            ordered_seen.insert((file_idx, augmentation.node))
        });
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
        if let Some(aug_index) = self.ctx.global_module_augmentations_index.as_ref() {
            let mut indexed = Vec::new();
            for candidate in candidates {
                if let Some(entries) = aug_index.get(candidate) {
                    for (file_idx, aug) in entries.iter() {
                        indexed.push((*file_idx, aug));
                    }
                }
            }
            indexed.sort_by_key(|(file_idx, aug)| {
                let position = self
                    .ctx
                    .all_arenas
                    .as_ref()
                    .and_then(|arenas| arenas.get(*file_idx))
                    .and_then(|arena| arena.get(aug.node))
                    .map_or(u32::MAX, |node| node.pos);
                (*file_idx, position)
            });
            for (file_idx, aug) in indexed {
                if let Some(arena) = self
                    .ctx
                    .all_arenas
                    .as_ref()
                    .and_then(|arenas| arenas.get(file_idx))
                {
                    search_augmentations(std::slice::from_ref(aug), arena, Some(arena), result);
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
        } else {
            for candidate in candidates {
                if let Some(augmentations) = self.ctx.binder.module_augmentations.get(candidate) {
                    search_augmentations(augmentations, self.ctx.arena, None, result);
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
    pub(crate) fn get_module_augmentation_members_inner(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        type_args: Option<&[TypeId]>,
        runtime_declarations: Option<&[ModuleAugmentation]>,
    ) -> crate::types_domain::module_augmentation_constructs::ModuleAugmentationInterfaceSurface
    {
        use tsz_parser::parser::syntax_kind_ext::{
            ENUM_DECLARATION, METHOD_SIGNATURE, MODULE_DECLARATION, PROPERTY_SIGNATURE,
        };

        let collected_declarations;
        let augmentation_decls = if let Some(runtime_declarations) = runtime_declarations {
            runtime_declarations
        } else {
            collected_declarations =
                self.get_module_augmentation_declarations(module_spec, interface_name);
            &collected_declarations
        };

        let mut members = Vec::new();
        let mut value_members = Vec::new();
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
                        crate::query_boundaries::type_rewrite::TypeSubstitution::from_args(
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
                                type_id = crate::query_boundaries::type_rewrite::instantiate_type(
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

            if node.kind == ENUM_DECLARATION || node.kind == MODULE_DECLARATION {
                value_members.extend(self.module_augmentation_runtime_value_members(
                    augmentation.node,
                    arena,
                    &mut aug_member_order,
                ));
                continue;
            }
        }

        let direct_names: FxHashSet<_> = members.iter().map(|member| member.name).collect();
        let mut interface_surface = if runtime_declarations.is_some() {
            self.get_module_augmentation_interface_surface_from_declarations(
                augmentation_decls,
                type_args,
            )
        } else {
            self.get_module_augmentation_interface_surface(module_spec, interface_name, type_args)
        };
        for mut property in interface_surface.properties.drain(..) {
            if direct_names.contains(&property.name) {
                continue;
            }
            aug_member_order += 1;
            property.declaration_order = aug_member_order;
            members.push(property);
        }

        interface_surface.properties = members;
        interface_surface.value_properties =
            self.normalize_module_augmentation_runtime_value_members(value_members);
        interface_surface
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
    pub(crate) fn apply_module_type_augmentations(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        base_type: TypeId,
    ) -> TypeId {
        if crate::query_boundaries::enum_analysis::is_enum_type(&self.ctx, base_type) {
            return base_type;
        }
        self.apply_module_augmentations_in_space(
            module_spec,
            interface_name,
            base_type,
            ModuleAugmentationSpace::Type,
            false,
            None,
        )
    }

    pub(crate) fn apply_module_value_augmentations(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        base_type: TypeId,
    ) -> TypeId {
        if !self.ctx.program_has_module_augmentations() {
            return base_type;
        }
        let runtime_declarations =
            self.module_augmentation_runtime_declarations(module_spec, interface_name);
        self.apply_module_augmentations_in_space(
            module_spec,
            interface_name,
            base_type,
            ModuleAugmentationSpace::Value,
            false,
            Some(&runtime_declarations),
        )
    }

    pub(crate) fn apply_module_value_augmentations_to_direct_value(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        base_type: TypeId,
    ) -> TypeId {
        if !self.ctx.program_has_module_augmentations() {
            return base_type;
        }
        let runtime_declarations =
            self.module_augmentation_runtime_declarations(module_spec, interface_name);
        self.apply_module_augmentations_in_space(
            module_spec,
            interface_name,
            base_type,
            ModuleAugmentationSpace::Value,
            true,
            Some(&runtime_declarations),
        )
    }

    pub(crate) fn apply_module_runtime_value_augmentations(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        base_type: TypeId,
        base_has_selected_declaration_space: bool,
        runtime_declarations: &[ModuleAugmentation],
    ) -> TypeId {
        self.apply_module_augmentations_in_space(
            module_spec,
            interface_name,
            base_type,
            ModuleAugmentationSpace::Value,
            base_has_selected_declaration_space,
            Some(runtime_declarations),
        )
    }

    pub(crate) fn apply_module_augmentations_in_space(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        base_type: TypeId,
        space: ModuleAugmentationSpace,
        base_has_selected_declaration_space: bool,
        runtime_declarations: Option<&[ModuleAugmentation]>,
    ) -> TypeId {
        use crate::query_boundaries::module_augmentation::{
            AugmentationTargetKind, classify_for_augmentation,
        };
        use crate::query_boundaries::state::type_resolution as query;

        // Fast-path: avoids string allocations and hashset bookkeeping when
        // no augmentations are registered anywhere in the program.
        if !self.ctx.program_has_module_augmentations() {
            return base_type;
        }
        let applied_declarations = runtime_declarations.map_or_else(
            || self.get_module_augmentation_declarations(module_spec, interface_name),
            |declarations| declarations.to_vec(),
        );
        if applied_declarations.is_empty() {
            return base_type;
        }
        let augmentation_declares_type_space = applied_declarations.iter().any(|augmentation| {
            let arena = augmentation.arena.as_deref().unwrap_or(self.ctx.arena);
            arena.get(augmentation.node).is_some_and(|node| {
                arena.get_interface(node).is_some()
                    || arena.get_class(node).is_some()
                    || arena.get_enum(node).is_some()
                    || arena.get_type_alias(node).is_some()
            })
        });
        let original_base_type = base_type;
        let guard_key = (
            module_spec.to_string(),
            interface_name.to_string(),
            base_type,
            space == ModuleAugmentationSpace::Value,
        );
        {
            let mut active = self.ctx.module_augmentation_application_set.borrow_mut();
            if !active.insert(guard_key.clone()) {
                return base_type;
            }
        }
        let publication_transaction = self.begin_module_augmentation_publication();
        let bailout_epoch_before = Self::cross_arena_bailout_epoch();
        let augmentation_surface = self.get_module_augmentation_members_inner(
            module_spec,
            interface_name,
            None,
            Some(&applied_declarations),
        );
        let crate::types_domain::module_augmentation_constructs::
            ModuleAugmentationInterfaceSurface {
                properties: augmentation_members,
                value_properties: augmentation_value_members,
                call_signatures: augmentation_call_signatures,
                string_index: augmentation_string_index,
                number_index: augmentation_number_index,
                symbol_index: augmentation_symbol_index,
            } = augmentation_surface;
        let target_native_spaces =
            self.module_augmentation_target_native_spaces(module_spec, interface_name);
        let seed_missing_declaration_space = match space {
            ModuleAugmentationSpace::Type => {
                target_native_spaces.is_some_and(|(has_type, _)| !has_type)
                    && augmentation_declares_type_space
                    && !base_has_selected_declaration_space
            }
            ModuleAugmentationSpace::Value => {
                target_native_spaces.is_some_and(|(_, has_value)| !has_value)
                    && !augmentation_value_members.is_empty()
                    && !base_has_selected_declaration_space
            }
        };
        let base_type = if seed_missing_declaration_space {
            module_augmentation_boundary::empty_declaration_space_type(self.ctx.types)
        } else {
            base_type
        };

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
        let base_shape_symbol = match kind {
            AugmentationTargetKind::Object(shape_id)
            | AugmentationTargetKind::ObjectWithIndex(shape_id) => {
                self.ctx.types.object_shape(shape_id).symbol
            }
            AugmentationTargetKind::Callable(shape_id) => {
                self.ctx.types.callable_shape(shape_id).symbol
            }
            AugmentationTargetKind::Other => None,
        };
        let owner_def_id = base_def_id
            .or_else(|| base_shape_symbol.map(|symbol| self.ctx.get_or_create_def_id(symbol)));
        let target_is_value_side = space == ModuleAugmentationSpace::Value;
        let runtime_declares_class_value = runtime_declarations.is_some_and(|declarations| {
            declarations.iter().any(|declaration| {
                let arena = declaration.arena.as_deref().unwrap_or(self.ctx.arena);
                arena.get(declaration.node).is_some_and(|node| {
                    node.kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION
                })
            })
        });
        let class_prototype_index = if target_is_value_side {
            match kind {
                AugmentationTargetKind::Callable(shape_id) => {
                    let base_shape = self.ctx.types.callable_shape(shape_id);
                    let class_owner = base_shape.symbol.filter(|&owner| {
                        runtime_declares_class_value || self.symbol_has_class_declaration(owner)
                    });
                    class_owner.and_then(|owner| {
                        crate::query_boundaries::class_type::
                            symbol_owned_class_prototype_property_index(
                                self.ctx.types,
                                &base_shape,
                                owner,
                            )
                    })
                }
                _ => None,
            }
        } else {
            None
        };
        let augmentation_construct_signatures =
            if !target_is_value_side || class_prototype_index.is_some() {
                if let Some(runtime_declarations) = runtime_declarations {
                    self.get_module_augmentation_construct_signatures_from_declarations(
                        runtime_declarations,
                        interface_name,
                        owner_def_id,
                    )
                } else {
                    self.get_module_augmentation_construct_signatures(
                        module_spec,
                        interface_name,
                        owner_def_id,
                    )
                }
            } else {
                Vec::new()
            };
        let augmentation_value_call_signatures =
            if target_is_value_side && !base_has_selected_declaration_space {
                self.module_augmentation_function_value_type(&applied_declarations)
                    .and_then(|function_type| {
                        crate::query_boundaries::module_augmentation::call_signatures(
                            self.ctx.types,
                            function_type,
                        )
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
        if Self::cross_arena_bailout_epoch() != bailout_epoch_before {
            // A nested owner-arena query hit the cross-arena depth cap. Its
            // empty member/signature vector is provisional, not evidence that
            // the augmentation contributes nothing. Return the standard
            // provisional `any` sentinel so enclosing symbol resolution sees
            // the advanced bailout epoch and refuses to cache an incomplete
            // concrete body.
            self.rollback_module_augmentation_publication(publication_transaction);
            self.ctx
                .module_augmentation_application_set
                .borrow_mut()
                .remove(&guard_key);
            return TypeId::ANY;
        }
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

        let mut augmentation_surface_applied_to_prototype = false;
        let mut class_value_target = false;
        let mut augmented_prototype_for_publication = None;
        let target_members = if target_is_value_side {
            &augmentation_value_members
        } else {
            &augmentation_members
        };
        let target_string_index = (!target_is_value_side)
            .then_some(augmentation_string_index)
            .flatten();
        let target_number_index = (!target_is_value_side)
            .then_some(augmentation_number_index)
            .flatten();
        let target_symbol_index = (!target_is_value_side)
            .then_some(augmentation_symbol_index)
            .flatten();
        let has_instance_augmentation_surface = !augmentation_members.is_empty()
            || !augmentation_call_signatures.is_empty()
            || !augmentation_construct_signatures.is_empty()
            || augmentation_string_index.is_some()
            || augmentation_number_index.is_some()
            || augmentation_symbol_index.is_some();
        let (direct_call_signatures, direct_construct_signatures) = if target_is_value_side {
            (augmentation_value_call_signatures, Vec::new())
        } else {
            (
                augmentation_call_signatures,
                augmentation_construct_signatures,
            )
        };
        let direct_signature_surface_applies = if target_is_value_side {
            class_prototype_index.is_none()
        } else {
            true
        };
        let has_relevant_augmentation_surface = match space {
            ModuleAugmentationSpace::Type => {
                has_instance_augmentation_surface || seed_missing_declaration_space
            }
            ModuleAugmentationSpace::Value => {
                !augmentation_value_members.is_empty()
                    || !direct_call_signatures.is_empty()
                    || (class_prototype_index.is_some() && has_instance_augmentation_surface)
            }
        };
        if !has_relevant_augmentation_surface {
            self.commit_module_augmentation_publication(publication_transaction);
            self.ctx
                .module_augmentation_application_set
                .borrow_mut()
                .remove(&guard_key);
            return base_type;
        }
        let mut deferred_string_index = None;
        let mut deferred_number_index = None;
        let mut deferred_symbol_index = None;
        let mut result = match kind {
            AugmentationTargetKind::Object(shape_id) => {
                let base_shape = self.ctx.types.object_shape(shape_id);
                let merged_properties = self.merge_properties(
                    target_members,
                    &base_shape.properties,
                    crate::interface_type::InterfaceMergeMode::Declaration,
                );
                let merged_string_index =
                    crate::query_boundaries::intersection_display::merge_index_signature_infos(
                        self.ctx.types,
                        base_shape.string_index_signature().copied(),
                        target_string_index,
                    );
                let merged_number_index =
                    crate::query_boundaries::intersection_display::merge_index_signature_infos(
                        self.ctx.types,
                        base_shape.number_index,
                        target_number_index,
                    );
                let merged_symbol_index =
                    crate::query_boundaries::intersection_display::merge_index_signature_infos(
                        self.ctx.types,
                        base_shape.symbol_index_signature().copied(),
                        target_symbol_index,
                    );
                if !direct_call_signatures.is_empty() || !direct_construct_signatures.is_empty() {
                    // `CallableShape` has no dedicated symbol-index slot. The
                    // callable merge below carries string/number indices, and
                    // this deferred raw intersection restores the independent
                    // symbol key space afterward.
                    deferred_symbol_index = merged_symbol_index;
                }
                // Preserve the base interface's nominal identity (symbol) and
                // object-level flags so the augmented type keeps its canonical
                // declaration name (e.g. `Tool` rather than an expanded
                // `{ ... }` literal) and stays a single interned identity.
                self.ctx
                    .types
                    .factory()
                    .object_with_shape_metadata_and_index_signatures(
                        merged_properties,
                        &base_shape,
                        merged_string_index,
                        merged_number_index,
                        merged_symbol_index,
                    )
            }
            AugmentationTargetKind::ObjectWithIndex(shape_id) => {
                let base_shape = self.ctx.types.object_shape(shape_id);
                let merged_properties = self.merge_properties(
                    target_members,
                    &base_shape.properties,
                    crate::interface_type::InterfaceMergeMode::Declaration,
                );
                let merged_string_index =
                    crate::query_boundaries::intersection_display::merge_index_signature_infos(
                        self.ctx.types,
                        base_shape.string_index_signature().copied(),
                        target_string_index,
                    );
                let merged_number_index =
                    crate::query_boundaries::intersection_display::merge_index_signature_infos(
                        self.ctx.types,
                        base_shape.number_index,
                        target_number_index,
                    );
                let merged_symbol_index =
                    crate::query_boundaries::intersection_display::merge_index_signature_infos(
                        self.ctx.types,
                        base_shape.symbol_index_signature().copied(),
                        target_symbol_index,
                    );
                if !direct_call_signatures.is_empty() || !direct_construct_signatures.is_empty() {
                    deferred_symbol_index = merged_symbol_index;
                }
                self.ctx
                    .types
                    .factory()
                    .object_with_shape_metadata_and_index_signatures(
                        merged_properties,
                        &base_shape,
                        merged_string_index,
                        merged_number_index,
                        merged_symbol_index,
                    )
            }
            AugmentationTargetKind::Callable(shape_id) => {
                let base_shape = self.ctx.types.callable_shape(shape_id);
                let mut construct_signatures = base_shape.construct_signatures.clone();
                let properties = if let Some(prototype_index) = class_prototype_index {
                    class_value_target = true;
                    // Class/interface declaration merging extends the instance
                    // side, while a same-name namespace/enum declaration
                    // extends the static value. Keep both surfaces disjoint.
                    let mut properties = base_shape.properties.clone();
                    if has_instance_augmentation_surface {
                        let prototype_prop = &mut properties[prototype_index];
                        let unaugmented_prototype = prototype_prop.type_id;
                        let augmented_prototype = if runtime_declarations.is_some() {
                            self.apply_module_augmentations_in_space(
                                module_spec,
                                interface_name,
                                prototype_prop.type_id,
                                ModuleAugmentationSpace::Type,
                                base_has_selected_declaration_space,
                                Some(&applied_declarations),
                            )
                        } else {
                            self.apply_module_type_augmentations(
                                module_spec,
                                interface_name,
                                prototype_prop.type_id,
                            )
                        };
                        prototype_prop.type_id = augmented_prototype;
                        prototype_prop.write_type = augmented_prototype;
                        for signature in &mut construct_signatures {
                            signature.return_type =
                                if signature.return_type == unaugmented_prototype {
                                    augmented_prototype
                                } else if runtime_declarations.is_some() {
                                    self.apply_module_augmentations_in_space(
                                        module_spec,
                                        interface_name,
                                        signature.return_type,
                                        ModuleAugmentationSpace::Type,
                                        base_has_selected_declaration_space,
                                        Some(&applied_declarations),
                                    )
                                } else {
                                    self.apply_module_type_augmentations(
                                        module_spec,
                                        interface_name,
                                        signature.return_type,
                                    )
                                };
                        }
                        augmented_prototype_for_publication = Some(augmented_prototype);
                        augmentation_surface_applied_to_prototype = true;
                    }
                    self.merge_properties(
                        &augmentation_value_members,
                        &properties,
                        crate::interface_type::InterfaceMergeMode::Declaration,
                    )
                } else {
                    // Non-constructor callable (namespace, function): merge
                    // augmentation members as direct properties. Constructable
                    // interfaces also stay on this path: unlike class values,
                    // they have no implicit symbol-owned `prototype` property.
                    self.merge_properties(
                        target_members,
                        &base_shape.properties,
                        crate::interface_type::InterfaceMergeMode::Declaration,
                    )
                };
                if class_prototype_index.is_none() && !target_is_value_side {
                    // A callable has only one string-or-symbol slot. Keep its
                    // existing surface intact and carry every augmentation
                    // index in a separate raw intersection after signature
                    // merging, so string + symbol never overwrite each other.
                    deferred_string_index = augmentation_string_index;
                    deferred_number_index = augmentation_number_index;
                    deferred_symbol_index = augmentation_symbol_index;
                }
                module_augmentation_boundary::augmented_callable_type(
                    self.ctx.types,
                    base_shape.call_signatures.clone(),
                    construct_signatures,
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
                if !direct_call_signatures.is_empty() || !direct_construct_signatures.is_empty() {
                    deferred_string_index = target_string_index;
                    deferred_number_index = target_number_index;
                    deferred_symbol_index = target_symbol_index;
                }
                module_augmentation_boundary::other_target_with_augmentation_surface(
                    self.ctx.types,
                    base_type,
                    target_members.to_vec(),
                    target_string_index,
                    target_number_index,
                    target_symbol_index,
                )
            }
        };
        if direct_signature_surface_applies
            && !augmentation_surface_applied_to_prototype
            && (!direct_call_signatures.is_empty() || !direct_construct_signatures.is_empty())
        {
            let augmentation_type = module_augmentation_boundary::augmented_callable_type(
                self.ctx.types,
                direct_call_signatures,
                direct_construct_signatures,
                Vec::new(),
                None,
                None,
                base_shape_symbol,
                false,
            );
            result = self.merge_interface_types_augmentation(augmentation_type, result);
        }
        if direct_signature_surface_applies
            && !augmentation_surface_applied_to_prototype
            && (deferred_string_index.is_some()
                || deferred_number_index.is_some()
                || deferred_symbol_index.is_some())
        {
            result = module_augmentation_boundary::with_augmentation_index_surface_raw(
                self.ctx.types,
                result,
                deferred_string_index,
                deferred_number_index,
                deferred_symbol_index,
            );
        }
        if Self::cross_arena_bailout_epoch() != bailout_epoch_before {
            self.rollback_module_augmentation_publication(publication_transaction);
            self.ctx
                .module_augmentation_application_set
                .borrow_mut()
                .remove(&guard_key);
            return TypeId::ANY;
        }

        // Publications before the final bailout check remain private in the
        // transaction overlay. A delegated recursion refusal rolls the whole
        // batch back, so only this post-check path can reach shared state.
        match kind {
            AugmentationTargetKind::Object(shape_id) => {
                let base_shape = self.ctx.types.object_shape(shape_id);
                if let (Some(symbol), Some(def_id)) = (base_shape.symbol, base_def_id) {
                    self.ctx
                        .definition_store
                        .register_module_augmentation_symbol_def_if_enabled(symbol.0, def_id);
                    if self
                        .ctx
                        .definition_store
                        .register_module_augmented_body_if_enabled(
                            def_id,
                            resolved_base,
                            result,
                            self.ctx.types,
                            &self.module_augmentation_source_files(module_spec),
                        )
                    {
                        self.ctx.clear_type_evaluation_caches_for_def(def_id);
                    }
                }
            }
            AugmentationTargetKind::ObjectWithIndex(shape_id) => {
                let base_shape = self.ctx.types.object_shape(shape_id);
                if let (Some(symbol), Some(def_id)) = (base_shape.symbol, base_def_id) {
                    self.ctx
                        .definition_store
                        .register_module_augmentation_symbol_def_if_enabled(symbol.0, def_id);
                }
            }
            AugmentationTargetKind::Callable(_) | AugmentationTargetKind::Other => {}
        }

        // Update cached types for augmentation-local symbols so that
        // self-referential type references (e.g., `self: Foo` inside
        // `declare module "./m" { interface Foo { self: Foo } }`) resolve to
        // the merged type instead of the augmentation-only type.
        // Both symbol_types and type_env must be updated because resolve_lazy
        // checks symbol_types first.
        if result != original_base_type {
            let local_publication = augmented_prototype_for_publication
                .or_else(|| (!target_is_value_side && !class_value_target).then_some(result));
            if let Some(local_publication) = local_publication {
                self.update_augmentation_local_symbol_types(
                    module_spec,
                    interface_name,
                    local_publication,
                    &applied_declarations,
                    runtime_declarations.is_none(),
                );
            }

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

        self.commit_module_augmentation_publication(publication_transaction);
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
    pub(crate) fn augmentation_member_key_name(
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
        self.apply_module_type_augmentations(&home_file_name, &interface_name, base_type)
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

        // Gather declarations in program/file order. Each one is lowered
        // against its owning arena, and storage identity deduplicates the
        // aggregate-binder and per-file-binder views of the same declaration.
        let mut ordered_declarations: Vec<(Option<Arc<NodeArena>>, NodeIndex)> = Vec::new();
        let mut is_self_global_aug = false;
        let is_own_declaration = |arena: &NodeArena, node: NodeIndex| {
            own_declarations.contains(&node)
                && (self
                    .ctx
                    .binder
                    .declaration_arenas
                    .get(&(sym_id, node))
                    .is_some_and(|arenas| {
                        arenas
                            .iter()
                            .any(|candidate| candidate.as_ref().shares_node_storage_with(arena))
                    })
                    || (self.ctx.declaration_is_local_to_current_arena(sym_id, node)
                        && self.ctx.arena.shares_node_storage_with(arena)))
        };

        let mut push_declaration = |arena: Option<Arc<NodeArena>>, node: NodeIndex| {
            let arena_ref = arena.as_deref().unwrap_or(self.ctx.arena);
            if ordered_declarations
                .iter()
                .any(|(existing, existing_node)| {
                    *existing_node == node
                        && existing
                            .as_deref()
                            .unwrap_or(self.ctx.arena)
                            .shares_node_storage_with(arena_ref)
                })
            {
                return;
            }
            ordered_declarations.push((arena, node));
        };

        if let (Some(all_binders), Some(all_arenas)) =
            (self.ctx.all_binders.clone(), self.ctx.all_arenas.clone())
        {
            for (binder, arena) in all_binders.iter().zip(all_arenas.iter()) {
                let Some(aug_decls) = binder.global_augmentations.get(&name) else {
                    continue;
                };
                for aug in aug_decls {
                    // Per-file binders own their arena-relative entries. Explicit
                    // foreign arenas are supplied by the aggregate view below.
                    if aug.arena.is_some() {
                        continue;
                    }
                    if !is_direct_declare_global_member(arena.as_ref(), aug.node) {
                        continue;
                    }
                    if is_own_declaration(arena.as_ref(), aug.node) {
                        is_self_global_aug = true;
                    }
                    push_declaration(Some(Arc::clone(arena)), aug.node);
                }
            }
        }

        if let Some(aug_decls) = self.ctx.binder.global_augmentations.get(&name) {
            for aug in aug_decls {
                let arena = aug.arena.clone();
                let arena_ref = arena.as_deref().unwrap_or(self.ctx.arena);
                if !is_direct_declare_global_member(arena_ref, aug.node) {
                    continue;
                }
                if is_own_declaration(arena_ref, aug.node) {
                    is_self_global_aug = true;
                }
                push_declaration(arena, aug.node);
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
        let owner_def_id = self.ctx.get_or_create_def_id(sym_id);
        let mut result = base_type;
        for (arena, declaration) in ordered_declarations {
            let arena_ref = arena.as_deref().unwrap_or(self.ctx.arena);
            let aug_type = self.lower_augmentation_for_arena(
                arena_ref,
                std::slice::from_ref(&declaration),
                &lib_contexts,
                &name,
                Some(owner_def_id),
            );
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
