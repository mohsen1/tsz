//! Declaration-arena helpers for lib symbols and global augmentations.

use std::sync::Arc;
use tsz_parser::parser::node::NodeAccess;
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
                    // Blind fallback: `decl_idx` was never proven to belong to
                    // `fallback_arena`. `NodeIndex`es are arena-local, so for a
                    // cross-file program symbol the same index addresses an
                    // unrelated node in this arena; lowering that node
                    // manufactures a wrong type (empty interface bodies,
                    // mis-typed members) that then leaks into the shared
                    // `DefinitionStore` and poisons sibling checkers under
                    // parallel fresh checking (issue #13255). Keep the pair
                    // only when the node is a named declaration that actually
                    // declares this symbol's name.
                    if fallback_arena_node_declares_symbol(binder, sym_id, decl_idx, fallback_arena)
                    {
                        vec![(decl_idx, fallback_arena)]
                    } else {
                        Vec::new()
                    }
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

/// Whether the node at `decl_idx` in `arena` is a named declaration whose
/// declared name equals the symbol's escaped name.
///
/// Used to validate the blind arena fallback in
/// [`collect_lib_decls_with_arenas_in_contexts`]: a pair is only usable for
/// name-driven lib lowering when the node provably declares the looked-up
/// name. Unrecognized node kinds and name mismatches are rejected — they are
/// foreign-arena index collisions, not declarations of this symbol.
fn fallback_arena_node_declares_symbol(
    binder: &tsz_binder::BinderState,
    sym_id: tsz_binder::SymbolId,
    decl_idx: NodeIndex,
    arena: &NodeArena,
) -> bool {
    let Some(symbol) = binder.get_symbol(sym_id) else {
        return false;
    };
    let Some(node) = arena.get(decl_idx) else {
        return false;
    };
    let name_idx = if let Some(interface) = arena.get_interface(node) {
        interface.name
    } else if let Some(alias) = arena.get_type_alias(node) {
        alias.name
    } else if let Some(class) = arena.get_class(node) {
        class.name
    } else if let Some(function) = arena.get_function(node) {
        function.name
    } else if let Some(enum_decl) = arena.get_enum(node) {
        enum_decl.name
    } else if let Some(module) = arena.get_module(node) {
        module.name
    } else if let Some(variable) = arena.get_variable_declaration(node) {
        variable.name
    } else {
        return false;
    };
    arena
        .get_identifier_text(name_idx)
        .is_some_and(|name| name == symbol.escaped_name)
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
