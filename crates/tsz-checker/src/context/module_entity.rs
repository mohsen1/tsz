//! Module entity resolution helpers for `CheckerContext`.
//!
//! Contains `module_resolves_to_non_module_entity` and its supporting functions.
//! Extracted from `context/mod.rs` for maintainability.

use tsz_binder::BinderState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;

use super::CheckerContext;
use crate::module_resolution::module_specifier_candidates;

/// Check whether a binder symbol has namespace shape (exports, members, or
/// a namespace declaration with a non-empty body).
///
/// This is used in multiple places during module-entity resolution to decide
/// whether an `export =` target has module/namespace characteristics.
fn has_namespace_shape(binder: &BinderState, sym: &tsz_binder::Symbol) -> bool {
    let has_namespace_decl = sym.declarations.iter().any(|decl_idx| {
        if decl_idx.is_none() {
            return false;
        }
        binder
            .declaration_arenas
            .get(&(sym.id, *decl_idx))
            .and_then(|v| v.first())
            .is_some_and(|arena| {
                let Some(node) = arena.get(*decl_idx) else {
                    return false;
                };
                if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                    return false;
                }
                let Some(module_decl) = arena.get_module(node) else {
                    return false;
                };
                if module_decl.body.is_none() {
                    return false;
                }
                let Some(body_node) = arena.get(module_decl.body) else {
                    return false;
                };
                if body_node.kind == syntax_kind_ext::MODULE_BLOCK
                    && let Some(block) = arena.get_module_block(body_node)
                    && let Some(statements) = block.statements.as_ref()
                {
                    return !statements.nodes.is_empty();
                }
                true
            })
    });

    sym.exports.as_ref().is_some_and(|tbl| !tbl.is_empty())
        || sym.members.as_ref().is_some_and(|tbl| !tbl.is_empty())
        || has_namespace_decl
}

/// Recursively check whether a node tree contains a namespace declaration
/// with the given name and a non-empty body.
fn contains_namespace_decl_named(
    arena: &NodeArena,
    idx: NodeIndex,
    target_name: &str,
    depth: usize,
) -> bool {
    if depth > 128 {
        return false;
    }
    let Some(node) = arena.get(idx) else {
        return false;
    };

    if node.kind == syntax_kind_ext::MODULE_DECLARATION {
        let Some(module_decl) = arena.get_module(node) else {
            return false;
        };
        if let Some(name_node) = arena.get(module_decl.name)
            && let Some(id) = arena.get_identifier(name_node)
            && id.escaped_text == target_name
        {
            if module_decl.body.is_none() {
                return false;
            }
            if let Some(body_node) = arena.get(module_decl.body)
                && body_node.kind == syntax_kind_ext::MODULE_BLOCK
                && let Some(block) = arena.get_module_block(body_node)
                && let Some(stmts) = block.statements.as_ref()
            {
                return !stmts.nodes.is_empty();
            }
            return true;
        }
        if module_decl.body.is_some() {
            return contains_namespace_decl_named(arena, module_decl.body, target_name, depth + 1);
        }
        return false;
    }

    if node.kind == syntax_kind_ext::MODULE_BLOCK
        && let Some(block) = arena.get_module_block(node)
        && let Some(statements) = block.statements.as_ref()
    {
        for &stmt in &statements.nodes {
            if contains_namespace_decl_named(arena, stmt, target_name, depth + 1) {
                return true;
            }
        }
    }

    false
}

/// Collect all `export =` target names from a node tree.
fn collect_export_equals_targets(
    arena: &NodeArena,
    idx: NodeIndex,
    out: &mut Vec<String>,
    depth: usize,
) {
    if depth > 128 {
        return;
    }
    let Some(node) = arena.get(idx) else {
        return;
    };

    if node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
        if let Some(assign) = arena.get_export_assignment(node)
            && assign.is_export_equals
            && let Some(expr_node) = arena.get(assign.expression)
            && let Some(id) = arena.get_identifier(expr_node)
        {
            out.push(id.escaped_text.clone());
        }
        return;
    }

    if node.kind == syntax_kind_ext::MODULE_DECLARATION {
        if let Some(module_decl) = arena.get_module(node)
            && module_decl.body.is_some()
        {
            collect_export_equals_targets(arena, module_decl.body, out, depth + 1);
        }
        return;
    }

    if node.kind == syntax_kind_ext::MODULE_BLOCK
        && let Some(block) = arena.get_module_block(node)
        && let Some(statements) = block.statements.as_ref()
    {
        for &stmt in &statements.nodes {
            collect_export_equals_targets(arena, stmt, out, depth + 1);
        }
    }
}

impl<'a> CheckerContext<'a> {
    /// Returns true if an augmentation target resolves to an `export =` value without
    /// namespace/module shape (TS2671/TS2649 cases).
    pub fn module_resolves_to_non_module_entity(&self, module_specifier: &str) -> bool {
        let candidates = module_specifier_candidates(module_specifier);

        let lookup_cached = |binder: &BinderState, key: &str| {
            binder.module_export_equals_non_module.get(key).copied()
        };

        if let Some(target_idx) = self.resolve_import_target(module_specifier)
            && let Some(target_binder) = self.get_binder_for_file(target_idx)
        {
            for candidate in &candidates {
                if let Some(non_module) = lookup_cached(target_binder, candidate) {
                    return non_module;
                }
            }
        }

        for candidate in &candidates {
            if let Some(non_module) = lookup_cached(self.binder, candidate) {
                return non_module;
            }
        }

        if let Some(all_binders) = self.all_binders.as_ref() {
            // Use global_module_binder_index for O(1) lookup instead of O(N) binder scan
            let mut checked = rustc_hash::FxHashSet::default();
            for candidate in &candidates {
                if let Some(file_indices) = self.files_for_module_specifier(candidate) {
                    for &idx in file_indices {
                        if checked.insert(idx)
                            && let Some(binder) = all_binders.get(idx)
                            && let Some(non_module) = lookup_cached(binder, candidate)
                        {
                            return non_module;
                        }
                    }
                } else {
                    // Fallback: O(N) scan when global index not available
                    for binder in all_binders.iter() {
                        if let Some(non_module) = lookup_cached(binder, candidate) {
                            return non_module;
                        }
                    }
                    break; // If index not available for one candidate, scan covered all
                }
            }
        }

        let export_equals_is_non_module = |binder: &BinderState,
                                           exports: &tsz_binder::SymbolTable|
         -> Option<bool> {
            let export_equals_sym_id = exports.get("export=")?;
            let has_named_exports = exports.iter().any(|(name, _)| name != "export=");
            tracing::trace!(
                module_specifier = module_specifier,
                export_equals_sym_id = export_equals_sym_id.0,
                has_named_exports,
                "module_resolves_to_non_module_entity: checking exports table"
            );

            let mut candidate_symbols = Vec::with_capacity(2);
            if let Some(sym) = binder.get_symbol(export_equals_sym_id) {
                candidate_symbols.push((binder, sym));
            } else if let Some(sym) = self.binder.get_symbol(export_equals_sym_id) {
                candidate_symbols.push((self.binder, sym));
            } else {
                // O(1) fast-path via resolve_symbol_file_index, then O(N) fallback
                let mut found = false;
                let file_idx = self.resolve_symbol_file_index(export_equals_sym_id);
                if let Some(file_idx) = file_idx
                    && let Some(target_binder) = self.get_binder_for_file(file_idx)
                    && let Some(sym) = target_binder.get_symbol(export_equals_sym_id)
                {
                    candidate_symbols.push((target_binder, sym));
                    found = true;
                }
                if !found && let Some(all_binders) = self.all_binders.as_ref() {
                    for other in all_binders.iter() {
                        if let Some(sym) = other.get_symbol(export_equals_sym_id) {
                            candidate_symbols.push((other.as_ref(), sym));
                            break;
                        }
                    }
                }
            }

            let export_assignment_target_name =
                |sym_binder: &BinderState, sym: &tsz_binder::Symbol| -> Option<String> {
                    let mut decls = sym.declarations.clone();
                    if sym.value_declaration.is_some() {
                        decls.push(sym.value_declaration);
                    }

                    for decl_idx in decls {
                        if decl_idx.is_none() {
                            continue;
                        }
                        let Some(arena) = sym_binder
                            .declaration_arenas
                            .get(&(sym.id, decl_idx))
                            .and_then(|v| v.first())
                        else {
                            continue;
                        };
                        let Some(node) = arena.get(decl_idx) else {
                            continue;
                        };
                        if node.kind != syntax_kind_ext::EXPORT_ASSIGNMENT {
                            continue;
                        }
                        let Some(assign) = arena.get_export_assignment(node) else {
                            continue;
                        };
                        if !assign.is_export_equals {
                            continue;
                        }
                        let Some(expr_node) = arena.get(assign.expression) else {
                            continue;
                        };
                        if let Some(id) = arena.get_identifier(expr_node) {
                            return Some(id.escaped_text.clone());
                        }
                    }

                    None
                };

            let symbol_has_namespace_shape =
                candidate_symbols.into_iter().any(|(sym_binder, sym)| {
                    tracing::trace!(
                        module_specifier = module_specifier,
                        symbol_name = sym.escaped_name.as_str(),
                        symbol_flags = sym.flags,
                        "module_resolves_to_non_module_entity: candidate symbol"
                    );
                    if has_namespace_shape(sym_binder, sym) {
                        return true;
                    }

                    if sym_binder
                        .get_symbols()
                        .find_all_by_name(&sym.escaped_name)
                        .iter()
                        .filter_map(|&candidate_id| sym_binder.get_symbol(candidate_id))
                        .any(|candidate| has_namespace_shape(sym_binder, candidate))
                    {
                        return true;
                    }

                    let Some(target_name) = export_assignment_target_name(sym_binder, sym) else {
                        return false;
                    };
                    tracing::trace!(
                        module_specifier = module_specifier,
                        target_name = target_name.as_str(),
                        "module_resolves_to_non_module_entity: export assignment target"
                    );

                    sym_binder
                        .get_symbols()
                        .find_all_by_name(&target_name)
                        .iter()
                        .filter_map(|&target_sym_id| sym_binder.get_symbol(target_sym_id))
                        .any(|target_sym| has_namespace_shape(sym_binder, target_sym))
                });

            tracing::trace!(
                module_specifier = module_specifier,
                symbol_has_namespace_shape,
                "module_resolves_to_non_module_entity: namespace shape computed"
            );
            Some(!has_named_exports && !symbol_has_namespace_shape)
        };

        let export_assignment_targets_namespace_via_source =
            |binder: &BinderState, arena: &NodeArena| {
                for source_file in &arena.source_files {
                    let mut export_targets = Vec::new();
                    for &stmt_idx in &source_file.statements.nodes {
                        collect_export_equals_targets(arena, stmt_idx, &mut export_targets, 0);
                    }
                    for target_name in export_targets {
                        let has_matching_namespace_decl = source_file
                            .statements
                            .nodes
                            .iter()
                            .copied()
                            .any(|top_stmt| {
                                contains_namespace_decl_named(arena, top_stmt, &target_name, 0)
                            });
                        if has_matching_namespace_decl {
                            return true;
                        }
                        if binder
                            .get_symbols()
                            .find_all_by_name(&target_name)
                            .iter()
                            .filter_map(|&target_id| binder.get_symbol(target_id))
                            .any(|target_sym| has_namespace_shape(binder, target_sym))
                        {
                            return true;
                        }
                    }
                }
                false
            };

        if let Some(target_idx) = self.resolve_import_target(module_specifier)
            && let Some(target_binder) = self.get_binder_for_file(target_idx)
        {
            let target_arena = self.get_arena_for_file(target_idx as u32);
            for candidate in &candidates {
                if let Some(exports) = self.module_exports_for_module(target_binder, candidate)
                    && let Some(non_module) = export_equals_is_non_module(target_binder, exports)
                {
                    tracing::trace!(
                        module_specifier = module_specifier,
                        candidate = candidate.as_str(),
                        branch = "target_specifier_key",
                        non_module,
                        "module_resolves_to_non_module_entity: branch result"
                    );
                    if non_module
                        && export_assignment_targets_namespace_via_source(
                            target_binder,
                            target_arena,
                        )
                    {
                        tracing::trace!(
                            module_specifier = module_specifier,
                            candidate = candidate.as_str(),
                            branch = "target_specifier_key",
                            "module_resolves_to_non_module_entity: source fallback override"
                        );
                        return false;
                    }
                    return non_module;
                }
            }

            if let Some(target_file_name) = self
                .get_arena_for_file(target_idx as u32)
                .source_files
                .first()
                .map(|sf| sf.file_name.as_str())
                && let Some(exports) =
                    self.module_exports_for_module(target_binder, target_file_name)
                && let Some(non_module) = export_equals_is_non_module(target_binder, exports)
            {
                tracing::trace!(
                    module_specifier = module_specifier,
                    branch = "target_file_key",
                    non_module,
                    "module_resolves_to_non_module_entity: branch result"
                );
                if non_module
                    && export_assignment_targets_namespace_via_source(target_binder, target_arena)
                {
                    tracing::trace!(
                        module_specifier = module_specifier,
                        branch = "target_file_key",
                        "module_resolves_to_non_module_entity: source fallback override"
                    );
                    return false;
                }
                return non_module;
            }
        }

        let mut saw_non_module = false;
        if let Some(exports) = self.binder.module_exports.get(module_specifier)
            && let Some(non_module) = export_equals_is_non_module(self.binder, exports)
        {
            tracing::trace!(
                module_specifier = module_specifier,
                branch = "self_binder",
                non_module,
                "module_resolves_to_non_module_entity: branch result"
            );
            if non_module && export_assignment_targets_namespace_via_source(self.binder, self.arena)
            {
                tracing::trace!(
                    module_specifier = module_specifier,
                    branch = "self_binder",
                    "module_resolves_to_non_module_entity: source fallback override"
                );
                return false;
            }
            if !non_module {
                return false;
            }
            saw_non_module = true;
        }

        if let Some(all_binders) = self.all_binders.as_ref() {
            let check_binder_at =
                |idx: usize, binder: &BinderState, saw: &mut bool| -> Option<bool> {
                    let exports = binder.module_exports.get(module_specifier)?;
                    let non_module = export_equals_is_non_module(binder, exports)?;
                    tracing::trace!(
                        module_specifier = module_specifier,
                        branch = "all_binders",
                        binder_idx = idx,
                        non_module,
                        "module_resolves_to_non_module_entity: branch result"
                    );
                    if non_module
                        && let Some(all_arenas) = self.all_arenas.as_ref()
                        && let Some(arena) = all_arenas.get(idx)
                        && export_assignment_targets_namespace_via_source(binder, arena.as_ref())
                    {
                        tracing::trace!(
                            module_specifier = module_specifier,
                            branch = "all_binders",
                            binder_idx = idx,
                            "module_resolves_to_non_module_entity: source fallback override"
                        );
                        return Some(false);
                    }
                    if !non_module {
                        return Some(false);
                    }
                    *saw = true;
                    None
                };

            // Use O(1) module binder index when available.
            if let Some(file_indices) = self.files_for_module_specifier(module_specifier) {
                for &idx in file_indices {
                    if let Some(binder) = all_binders.get(idx)
                        && let Some(result) = check_binder_at(idx, binder, &mut saw_non_module)
                    {
                        return result;
                    }
                }
            } else {
                for (idx, binder) in all_binders.iter().enumerate() {
                    if let Some(result) = check_binder_at(idx, binder, &mut saw_non_module) {
                        return result;
                    }
                }
            }
        }

        saw_non_module
    }
}
