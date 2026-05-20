//! Declaration-arena helpers for lib symbols and global augmentations.

use std::sync::Arc;
use tsz_parser::parser::{NodeArena, NodeIndex};

/// Resolve fallback arena for a lib symbol from merged binders/lib contexts.
pub(crate) fn resolve_lib_fallback_arena<'a>(
    binder: &'a tsz_binder::BinderState,
    sym_id: tsz_binder::SymbolId,
    lib_contexts: &'a [crate::context::LibContext],
    user_arena: &'a NodeArena,
) -> &'a NodeArena {
    binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref)
        .or_else(|| lib_contexts.first().map(|ctx| ctx.arena.as_ref()))
        .unwrap_or(user_arena)
}

/// Resolve fallback arena for a lib symbol within a single lib context.
pub(crate) fn resolve_lib_context_fallback_arena<'a>(
    binder: &'a tsz_binder::BinderState,
    sym_id: tsz_binder::SymbolId,
    lib_arena: &'a NodeArena,
) -> &'a NodeArena {
    binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref)
        .unwrap_or(lib_arena)
}

/// Build `(NodeIndex, &NodeArena)` pairs for a symbol's declarations.
/// Uses `declaration_arenas`, then falls back to an owned user declaration or
/// the lib arena.
pub(crate) fn collect_lib_decls_with_arenas<'a>(
    binder: &'a tsz_binder::BinderState,
    sym_id: tsz_binder::SymbolId,
    declarations: &[NodeIndex],
    fallback_arena: &'a NodeArena,
    user_arena: Option<&'a NodeArena>,
) -> Vec<(NodeIndex, &'a NodeArena)> {
    collect_lib_decls_with_arenas_in_contexts(
        binder,
        sym_id,
        declarations,
        fallback_arena,
        &[],
        user_arena,
    )
}

pub(crate) fn collect_lib_decls_with_arenas_in_contexts<'a>(
    binder: &'a tsz_binder::BinderState,
    sym_id: tsz_binder::SymbolId,
    declarations: &[NodeIndex],
    fallback_arena: &'a NodeArena,
    lib_contexts: &'a [crate::context::LibContext],
    user_arena: Option<&'a NodeArena>,
) -> Vec<(NodeIndex, &'a NodeArena)> {
    declarations
        .iter()
        .flat_map(|&decl_idx| {
            if let Some(arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                arenas
                    .iter()
                    .map(|arc| (decl_idx, arc.as_ref()))
                    .collect::<Vec<_>>()
            } else if let Some(ua) = user_arena
                && is_current_file_global_augmentation_decl(binder, sym_id, decl_idx, ua)
                && ua.get(decl_idx).is_some()
            {
                vec![(decl_idx, ua)]
            } else {
                let lib_decl_arenas =
                    collect_decl_arenas_from_lib_contexts(binder, sym_id, decl_idx, lib_contexts);
                if lib_decl_arenas.is_empty() {
                    vec![(decl_idx, fallback_arena)]
                } else {
                    lib_decl_arenas
                        .into_iter()
                        .map(|arena| (decl_idx, arena))
                        .collect()
                }
            }
        })
        .collect()
}

fn collect_decl_arenas_from_lib_contexts<'a>(
    binder: &tsz_binder::BinderState,
    sym_id: tsz_binder::SymbolId,
    decl_idx: NodeIndex,
    lib_contexts: &'a [crate::context::LibContext],
) -> Vec<&'a NodeArena> {
    let Some(symbol) = binder.get_symbol(sym_id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for lib_ctx in lib_contexts {
        let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(&symbol.escaped_name) else {
            continue;
        };
        let Some(lib_symbol) = lib_ctx.binder.get_symbol(lib_sym_id) else {
            continue;
        };
        if lib_symbol.declarations.contains(&decl_idx) && lib_ctx.arena.get(decl_idx).is_some() {
            out.push(lib_ctx.arena.as_ref());
        }
    }
    out
}

fn is_current_file_global_augmentation_decl(
    binder: &tsz_binder::BinderState,
    sym_id: tsz_binder::SymbolId,
    decl_idx: NodeIndex,
    user_arena: &NodeArena,
) -> bool {
    let Some(symbol) = binder.get_symbol(sym_id) else {
        return false;
    };
    let Some(augmentations) = binder.global_augmentations.get(&symbol.escaped_name) else {
        return false;
    };
    let user_arena_ptr = user_arena as *const NodeArena;
    augmentations.iter().any(|aug| {
        aug.node == decl_idx
            && aug
                .arena
                .as_ref()
                .is_none_or(|arena| std::ptr::eq(Arc::as_ptr(arena), user_arena_ptr))
    })
}

/// Deduplicate declaration-arena pairs by `(NodeIndex, arena pointer)`.
pub(crate) fn dedup_decl_arenas<'a>(
    decls: &[(NodeIndex, &'a NodeArena)],
) -> Vec<(NodeIndex, &'a NodeArena)> {
    let mut seen = Vec::with_capacity(decls.len());
    let mut out = Vec::with_capacity(decls.len());
    for &(idx, arena) in decls {
        let key = (idx, arena as *const NodeArena);
        if !seen.contains(&key) {
            seen.push(key);
            out.push((idx, arena));
        }
    }
    out
}
