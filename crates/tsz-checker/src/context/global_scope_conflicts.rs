//! Pre-built index for global-scope duplicate detection.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::{BinderState, symbol_flags};
use tsz_parser::parser::{NodeIndex, node::NodeArena};

/// Type alias for the global-scope-conflict index value type.
///
/// Each entry is `(file_idx, decl_node, symbol_flags, is_ambient)`:
/// - `file_idx`: which project file owns the declaration
/// - `decl_node`: the declaration's [`NodeIndex`]
/// - `symbol_flags`: binder-assigned flags for the symbol
/// - `is_ambient`: `true` for UMD exports, `false` for `declare global` vars
pub type GlobalScopeConflictEntry = (usize, NodeIndex, u32, bool);

/// Program-wide index mapping a symbol name to all cross-file
/// global-scope conflict candidates for that name.
///
/// Shared across all per-file checkers via `Arc`. Built once by
/// `ProgramContext::build_global_indices`.
pub type GlobalScopeConflictIndex = FxHashMap<String, Vec<GlobalScopeConflictEntry>>;

/// Build the global-scope-conflict index for `ProgramContext`.
///
/// This intentionally uses the same per-binder augmentation walk as the legacy
/// path. Some `GlobalAugmentation` records do not carry an arena pointer, so the
/// builder must fall back to the binder's file index instead of dropping them.
pub fn build_global_scope_conflict_index_for_program(
    binders: &[Arc<BinderState>],
    arena_to_file_idx: &FxHashMap<usize, usize>,
) -> GlobalScopeConflictIndex {
    let mut index = FxHashMap::default();
    index_umd_exports(&mut index, binders);

    let mut seen_nodes = FxHashSet::default();
    for (file_idx, binder) in binders.iter().enumerate() {
        index_global_augmentations(
            &mut index,
            &mut seen_nodes,
            binder,
            file_idx,
            Some(arena_to_file_idx),
            true,
        );
    }

    index
}

/// Build the global-scope-conflict index for legacy/test fallback checkers.
///
/// Legacy binders are direct per-file binders rather than merged cross-file
/// lookup binders. Their `global_augmentations` entries often have no arena
/// pointer, so those entries belong to the binder's own file index.
pub fn build_global_scope_conflict_index_for_legacy(
    binders: &[Arc<BinderState>],
    arena_to_file_idx: Option<&FxHashMap<usize, usize>>,
) -> GlobalScopeConflictIndex {
    let mut index = FxHashMap::default();
    index_umd_exports(&mut index, binders);

    let mut seen_nodes = FxHashSet::default();
    for (file_idx, binder) in binders.iter().enumerate() {
        index_global_augmentations(
            &mut index,
            &mut seen_nodes,
            binder,
            file_idx,
            arena_to_file_idx,
            true,
        );
    }

    index
}

fn index_umd_exports(index: &mut GlobalScopeConflictIndex, binders: &[Arc<BinderState>]) {
    for (file_idx, binder) in binders.iter().enumerate() {
        for (name, &sym_id) in binder.file_locals.iter() {
            if let Some(sym) = binder.symbols.get(sym_id)
                && sym.is_umd_export
                && let Some(decl_idx) = sym.primary_declaration()
            {
                index.entry(name.to_string()).or_default().push((
                    file_idx,
                    decl_idx,
                    symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::ALIAS,
                    true,
                ));
            }
        }
    }
}

fn index_global_augmentations(
    index: &mut GlobalScopeConflictIndex,
    seen_nodes: &mut FxHashSet<(usize, NodeIndex)>,
    binder: &BinderState,
    fallback_file_idx: usize,
    arena_to_file_idx: Option<&FxHashMap<usize, usize>>,
    use_fallback_file_idx: bool,
) {
    for (name, augs) in binder.global_augmentations.iter() {
        for aug in augs.iter() {
            if aug.flags & symbol_flags::VARIABLE == 0 {
                continue;
            }

            let file_idx = owner_file_idx(
                aug.arena.as_ref(),
                fallback_file_idx,
                arena_to_file_idx,
                use_fallback_file_idx,
            );
            let Some(file_idx) = file_idx else {
                continue;
            };

            if !seen_nodes.insert((file_idx, aug.node)) {
                continue;
            }

            index
                .entry(name.to_string())
                .or_default()
                .push((file_idx, aug.node, aug.flags, false));
        }
    }
}

fn owner_file_idx(
    arena: Option<&Arc<NodeArena>>,
    fallback_file_idx: usize,
    arena_to_file_idx: Option<&FxHashMap<usize, usize>>,
    use_fallback_file_idx: bool,
) -> Option<usize> {
    if let Some(file_idx) = arena.and_then(|arena| {
        arena_to_file_idx.and_then(|map| map.get(&(Arc::as_ptr(arena) as usize)).copied())
    }) {
        return Some(file_idx);
    }

    use_fallback_file_idx.then_some(fallback_file_idx)
}
