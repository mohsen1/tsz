//! Library type resolution for built-in `.d.ts` declarations and global
//! augmentations. Keep resolver logic here as shared helpers to preserve stable
//! SymbolId/DefId identity across lib arenas.

use crate::query_boundaries::common::TypeResolver;
use crate::query_boundaries::type_predicates::is_compiler_managed_type;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

pub(crate) use super::lib_decls::{
    collect_lib_decls_with_arenas, collect_lib_decls_with_arenas_in_contexts, dedup_decl_arenas,
    resolve_lib_context_fallback_arena, resolve_lib_fallback_arena,
};
use super::lib_name_text::{entity_name_text_from_decl_arenas, entity_name_text_in_arena};
use super::lib_resolution_selected::{
    canonical_interface_symbol_id, register_selected_lib_def_resolved, selected_lib_symbol_for_name,
};

/// Index from identifier text to `(file_idx, SymbolId)` entries in `file_locals`.
pub(crate) type FileLocalsIndex = FxHashMap<String, Vec<(usize, tsz_binder::SymbolId)>>;

/// Stub value resolver for lib lowering; lib declarations have no runtime values.
pub(crate) const fn no_value_resolver(_: NodeIndex) -> Option<u32> {
    None
}

/// Per-name lib-resolution marker used to fix lib-interface heritage drops under
/// resolution cycles (#12299).
#[derive(Clone, Copy, PartialEq, Eq)]
enum LibResolutionMark {
    /// `resolve_lib_type_by_name` for this name is currently on the stack.
    InProgress,
    /// The most recent resolution dropped an in-progress heritage base (directly
    /// or transitively), so the produced type is incomplete and was not cached.
    Incomplete,
}

thread_local! {
    /// Lib-resolution markers, scoped to the active thread. This is transient
    /// resolution-stack state (like the `ASSIGNABILITY_EVAL_VISITING` guard), so
    /// it lives in a thread-local rather than growing `CheckerContext`. Entries
    /// self-clear: `InProgress` is push/pop balanced by `resolve_lib_type_by_name`,
    /// and an `Incomplete` mark is removed the next time the name resolves
    /// completely — which always happens (the cache slot was dropped) before a
    /// derived interface reads it as a heritage base.
    static LIB_RESOLUTION_MARKS: std::cell::RefCell<FxHashMap<String, LibResolutionMark>> =
        std::cell::RefCell::new(FxHashMap::default());

    /// Depth of the active `resolve_lib_type_by_name` call stack on this thread.
    /// `InProgress` markers are installed only by that function, so `depth == 0`
    /// means no lib resolution is on the stack and every cycle has fully
    /// unwound. The outermost call uses this boundary to drain names left
    /// `Incomplete` by a mutual heritage cycle (#12299).
    static LIB_RESOLUTION_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };

    /// Set while the outermost call is re-resolving cycle-incomplete names, so a
    /// nested base resolution does not recursively launch another drain.
    static LIB_RESOLUTION_DRAINING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Look up a lib type `name`'s resolution marker, trying the same normalizations
/// the resolver caches under: the raw name, the `globalThis.` strip, and the
/// qualified tail after the last `.`.
fn lib_resolution_mark(name: &str) -> Option<LibResolutionMark> {
    LIB_RESOLUTION_MARKS.with(|marks| {
        let marks = marks.borrow();
        if let Some(&mark) = marks.get(name) {
            return Some(mark);
        }
        let normalized = name.strip_prefix("globalThis.").unwrap_or(name);
        if normalized != name
            && let Some(&mark) = marks.get(normalized)
        {
            return Some(mark);
        }
        match normalized.rsplit('.').next() {
            Some(tail) if tail != normalized => marks.get(tail).copied(),
            _ => None,
        }
    })
}

/// Kill-switch: set `TSZ_DISABLE_LIB_GENERIC_PREWARM_DEFER` to a non-empty,
/// non-`0` value to restore the legacy behavior of fully resolving every
/// generic lib type reference reached during the `resolve_lib_type_by_name`
/// prewarm. Default (unset) defers to `prime_referenced_lib_type_params` (see
/// its doc for the defer semantics and issue #12101). The deferred form is
/// speed-only: with the switch on vs. off the produced types and diagnostics
/// are identical for every input.
fn lib_generic_prewarm_defer_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_LIB_GENERIC_PREWARM_DEFER")
            .is_ok_and(|v| !v.is_empty() && v != "0")
    })
}

/// Record `name`'s lib-resolution marker.
fn set_lib_resolution_mark(name: &str, mark: LibResolutionMark) {
    LIB_RESOLUTION_MARKS.with(|marks| {
        marks.borrow_mut().insert(name.to_string(), mark);
    });
}

/// Clear `name`'s lib-resolution marker (it resolved completely).
fn clear_lib_resolution_mark(name: &str) {
    LIB_RESOLUTION_MARKS.with(|marks| {
        marks.borrow_mut().remove(name);
    });
}

/// Enter a `resolve_lib_type_by_name` frame (increment the active depth).
fn enter_lib_resolution() {
    LIB_RESOLUTION_DEPTH.with(|depth| depth.set(depth.get() + 1));
}

/// Leave a `resolve_lib_type_by_name` frame and return the remaining depth.
/// `depth == 0` marks the outermost call, where cycle draining runs.
fn leave_lib_resolution() -> u32 {
    LIB_RESOLUTION_DEPTH.with(|depth| {
        let next = depth.get().saturating_sub(1);
        depth.set(next);
        next
    })
}

/// Whether the outermost call is currently draining cycle-incomplete names.
fn lib_resolution_is_draining() -> bool {
    LIB_RESOLUTION_DRAINING.with(std::cell::Cell::get)
}

fn set_lib_resolution_draining(value: bool) {
    LIB_RESOLUTION_DRAINING.with(|flag| flag.set(value));
}

/// Whether any name is currently marked `Incomplete`. A cheap, allocation-free
/// gate for the hot outermost-call path before the `collect` scan.
fn has_incomplete_lib_names() -> bool {
    LIB_RESOLUTION_MARKS.with(|marks| {
        marks
            .borrow()
            .values()
            .any(|mark| *mark == LibResolutionMark::Incomplete)
    })
}

/// Names currently marked `Incomplete` because a mutual heritage cycle dropped a
/// base that was itself mid-resolution (#12299).
fn collect_incomplete_lib_names() -> Vec<String> {
    LIB_RESOLUTION_MARKS.with(|marks| {
        marks
            .borrow()
            .iter()
            .filter(|(_, mark)| **mark == LibResolutionMark::Incomplete)
            .map(|(name, _)| name.clone())
            .collect()
    })
}

/// Reset the per-compilation lib-resolution thread-locals at a project-row
/// boundary. The depth counter and draining flag are balanced in the normal
/// path, but a mid-resolution bail-out (stack-overflow breaker, fuel
/// exhaustion, or a panic caught by the batch driver) can leave `depth > 0`,
/// which would suppress the cycle drain for every later row on this worker
/// thread. Wired into `clear_all_thread_local_state`.
pub fn reset_lib_resolution_state() {
    LIB_RESOLUTION_MARKS.with(|marks| marks.borrow_mut().clear());
    LIB_RESOLUTION_DEPTH.with(|depth| depth.set(0));
    LIB_RESOLUTION_DRAINING.with(|flag| flag.set(false));
}

/// Map a keyword `SyntaxKind` to its built-in `TypeId`.
pub(crate) const fn keyword_syntax_to_type_id(kind: u16) -> Option<TypeId> {
    match kind {
        k if k == SyntaxKind::StringKeyword as u16 => Some(TypeId::STRING),
        k if k == SyntaxKind::NumberKeyword as u16 => Some(TypeId::NUMBER),
        k if k == SyntaxKind::BooleanKeyword as u16 => Some(TypeId::BOOLEAN),
        k if k == SyntaxKind::VoidKeyword as u16 => Some(TypeId::VOID),
        k if k == SyntaxKind::UndefinedKeyword as u16 => Some(TypeId::UNDEFINED),
        k if k == SyntaxKind::NullKeyword as u16 => Some(TypeId::NULL),
        k if k == SyntaxKind::NeverKeyword as u16 => Some(TypeId::NEVER),
        k if k == SyntaxKind::UnknownKeyword as u16 => Some(TypeId::UNKNOWN),
        k if k == SyntaxKind::AnyKeyword as u16 => Some(TypeId::ANY),
        k if k == SyntaxKind::ObjectKeyword as u16 => Some(TypeId::OBJECT),
        k if k == SyntaxKind::SymbolKeyword as u16 => Some(TypeId::SYMBOL),
        k if k == SyntaxKind::BigIntKeyword as u16 => Some(TypeId::BIGINT),
        _ => None,
    }
}

/// Map a keyword type name (for example `"string"`) to a built-in `TypeId`.
pub(crate) fn keyword_name_to_type_id(name: &str) -> Option<TypeId> {
    match name {
        "string" => Some(TypeId::STRING),
        "number" => Some(TypeId::NUMBER),
        "boolean" => Some(TypeId::BOOLEAN),
        "void" => Some(TypeId::VOID),
        "undefined" => Some(TypeId::UNDEFINED),
        "null" => Some(TypeId::NULL),
        "never" => Some(TypeId::NEVER),
        "unknown" => Some(TypeId::UNKNOWN),
        "any" => Some(TypeId::ANY),
        "object" => Some(TypeId::OBJECT),
        "symbol" => Some(TypeId::SYMBOL),
        "bigint" => Some(TypeId::BIGINT),
        _ => None,
    }
}

/// Resolve a `NodeIndex` directly to a `DefId` via the merged binder.
///
/// This is the stable one-step helper for lib lowering: it combines
/// [`resolve_lib_node_in_arenas`] (`NodeIndex` → `SymbolId`) with
/// [`CheckerContext::get_lib_def_id`] (`SymbolId` → `DefId`).  Using this
/// instead of the two-step closure pattern avoids duplicating the
/// resolution logic at every callsite.
pub(crate) fn lib_def_id_from_node(
    ctx: &crate::context::CheckerContext<'_>,
    binder: &tsz_binder::BinderState,
    node_idx: NodeIndex,
    decl_arenas: &[(NodeIndex, &NodeArena)],
    fallback_arena: &NodeArena,
) -> Option<tsz_solver::DefId> {
    let sym_id = resolve_lib_node_in_arenas(binder, node_idx, decl_arenas, fallback_arena)?;
    if let Some(symbol) = binder
        .get_symbol_with_libs(sym_id, &[])
        .filter(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_PARAMETER))
    {
        // The owning binder says this is a type parameter; resolve the def
        // name-verified against that binder's symbol name so a raw-id
        // collision with another lib binder cannot answer with an unrelated
        // def.
        return Some(ctx.get_or_create_def_id_for_symbol_name(sym_id, &symbol.escaped_name));
    }

    if let Some(name) = entity_name_text_from_decl_arenas(node_idx, decl_arenas, fallback_arena) {
        let expected_name = name
            .strip_prefix("globalThis.")
            .unwrap_or(&name)
            .rsplit('.')
            .next()
            .unwrap_or(&name);
        if let Some(def_id) = ctx.actual_lib_def_id_for_bare_name(expected_name) {
            return Some(def_id);
        }
        return Some(ctx.get_canonical_lib_def_id(expected_name, sym_id));
    }

    // No syntactic name available; verify against the owning binder's symbol
    // name instead of trusting the raw-id resolution.
    if let Some(symbol) = binder.get_symbol_with_libs(sym_id, &[]) {
        return Some(ctx.get_or_create_def_id_for_symbol_name(sym_id, &symbol.escaped_name));
    }
    Some(ctx.get_lib_def_id(sym_id))
}

/// Resolve a `NodeIndex` directly to a `DefId` via lib-context binders.
///
/// Same as [`lib_def_id_from_node`] but delegates to
/// [`resolve_lib_node_in_lib_contexts`] for per-lib-context lowering
/// (e.g., `resolve_lib_type_with_params`).
pub(crate) fn lib_def_id_from_node_in_lib_contexts(
    ctx: &crate::context::CheckerContext<'_>,
    node_idx: NodeIndex,
    decl_arenas: &[(NodeIndex, &NodeArena)],
    fallback_arena: &NodeArena,
    lib_contexts: &[crate::context::LibContext],
) -> Option<tsz_solver::DefId> {
    let sym_id =
        resolve_lib_node_in_lib_contexts(node_idx, decl_arenas, fallback_arena, lib_contexts)?;
    // NOTE: `resolve_lib_node_in_lib_contexts` only resolves through lib
    // `file_locals` (name-keyed), which never hold type parameters. The old
    // type-parameter probe here checked the raw SymbolId against *every* lib
    // binder, so it could only fire on a raw-id collision with an unrelated
    // binder's type-parameter symbol — and then resolved the def through the
    // same context-agnostic raw-id path (the lib-def identity-collision
    // family). Resolve by syntactic name instead.
    let name = entity_name_text_from_decl_arenas(node_idx, decl_arenas, fallback_arena)?;
    let expected_name = name
        .strip_prefix("globalThis.")
        .unwrap_or(&name)
        .rsplit('.')
        .next()
        .unwrap_or(&name);
    if let Some(def_id) = ctx.actual_lib_def_id_for_bare_name(expected_name) {
        return Some(def_id);
    }
    Some(ctx.get_canonical_lib_def_id(expected_name, sym_id))
}

/// Resolve a `NodeIndex` directly to a `DefId` via the augmentation resolution
/// strategy.
///
/// This is the stable one-step helper for augmentation lowering: it combines
/// [`resolve_augmentation_node`] (`NodeIndex` → `SymbolId`) with
/// [`CheckerContext::get_lib_def_id`] (`SymbolId` → `DefId`).  Using this
/// instead of inline two-step resolution at each callsite keeps the pattern
/// consistent with [`lib_def_id_from_node`].
pub(crate) fn augmentation_def_id_from_node(
    ctx: &crate::context::CheckerContext<'_>,
    binder: &tsz_binder::BinderState,
    arena: &NodeArena,
    node_idx: NodeIndex,
    global_file_locals_index: Option<&FileLocalsIndex>,
    all_binders: Option<&[std::sync::Arc<tsz_binder::BinderState>]>,
    lib_contexts: &[crate::context::LibContext],
) -> Option<tsz_solver::DefId> {
    let sym_id = resolve_augmentation_node(
        binder,
        arena,
        node_idx,
        global_file_locals_index,
        all_binders,
        lib_contexts,
    )?;
    if let Some(name) = entity_name_text_in_arena(arena, node_idx) {
        let expected_name = name
            .strip_prefix("globalThis.")
            .unwrap_or(&name)
            .rsplit('.')
            .next()
            .unwrap_or(&name);
        Some(ctx.get_or_create_def_id_for_symbol_name(sym_id, expected_name))
    } else if let Some(symbol) = binder.get_symbol_with_libs(sym_id, &[]) {
        // No syntactic name; verify against the owning binder's symbol name
        // instead of trusting the raw-id resolution (lib-def identity
        // collision family).
        Some(ctx.get_or_create_def_id_for_symbol_name(sym_id, &symbol.escaped_name))
    } else {
        Some(ctx.get_lib_def_id(sym_id))
    }
}

/// Collect every node index reachable from `decl_idx`'s `extends`/`implements`
/// heritage clauses (the clause nodes and their full subtrees). Used by the
/// `resolve_lib_type_by_name` prewarm to keep heritage-base generic references
/// eager while deferring member/parameter-position ones (#12101). Returns an
/// empty set for non-interface declarations (type aliases have no heritage).
fn collect_heritage_subtree_nodes(
    arena: &NodeArena,
    decl_idx: NodeIndex,
) -> rustc_hash::FxHashSet<NodeIndex> {
    let mut nodes = rustc_hash::FxHashSet::default();
    let Some(clauses) = arena
        .get(decl_idx)
        .and_then(|node| arena.get_interface(node))
        .and_then(|iface| iface.heritage_clauses.as_ref())
    else {
        return nodes;
    };
    let mut stack: Vec<NodeIndex> = clauses.nodes.to_vec();
    while let Some(node_idx) = stack.pop() {
        if nodes.insert(node_idx) {
            stack.extend(arena.get_children(node_idx));
        }
    }
    nodes
}

/// Resolve a lib node through node bindings, lexical scopes, and file-level symbols.
pub(crate) fn resolve_lib_node_in_arenas(
    binder: &tsz_binder::BinderState,
    node_idx: NodeIndex,
    decl_arenas: &[(NodeIndex, &NodeArena)],
    fallback_arena: &NodeArena,
) -> Option<tsz_binder::SymbolId> {
    if let Some(sym_id) =
        resolve_node_symbol_in_decl_arenas(binder, node_idx, decl_arenas, fallback_arena)
    {
        return Some(sym_id);
    }
    for (_, arena) in decl_arenas {
        if let Some(ident_name) = arena.get_identifier_text(node_idx) {
            if is_compiler_managed_type(ident_name) {
                continue;
            }
            if let Some(sym_id) = resolve_scope_chain(binder, arena, node_idx) {
                return Some(sym_id);
            }
            if let Some(found_sym) = binder.file_locals.get(ident_name) {
                return Some(found_sym);
            }
        }
    }
    if let Some(ident_name) = fallback_arena.get_identifier_text(node_idx) {
        if is_compiler_managed_type(ident_name) {
            return None;
        }
        if let Some(sym_id) = resolve_scope_chain(binder, fallback_arena, node_idx) {
            return Some(sym_id);
        }
        if let Some(found_sym) = binder.file_locals.get(ident_name) {
            return Some(found_sym);
        }
    }
    None
}

fn resolve_node_symbol_in_decl_arenas(
    binder: &tsz_binder::BinderState,
    node_idx: NodeIndex,
    decl_arenas: &[(NodeIndex, &NodeArena)],
    fallback_arena: &NodeArena,
) -> Option<tsz_binder::SymbolId> {
    for (_, arena) in decl_arenas {
        if let Some(sym_id) = resolve_node_symbol_in_arena(binder, arena, node_idx) {
            return Some(sym_id);
        }
    }
    resolve_node_symbol_in_arena(binder, fallback_arena, node_idx)
}

fn resolve_node_symbol_in_arena(
    binder: &tsz_binder::BinderState,
    arena: &NodeArena,
    node_idx: NodeIndex,
) -> Option<tsz_binder::SymbolId> {
    let arena_ptr = arena as *const NodeArena as usize;
    binder
        .cross_file_node_symbols
        .get(&arena_ptr)
        .and_then(|node_symbols| node_symbols.get(&node_idx.0).copied())
}

/// Walk a binder's scope chain from the enclosing scope of `node_idx` up to the
/// root, returning the first `SymbolId` that matches the identifier text at
/// `node_idx`.
///
/// This replaces the duplicated `resolve_in_scope` closures that previously
/// appeared in lib resolution, lib.rs, and property-access augmentation.
pub(crate) fn resolve_scope_chain(
    binder: &tsz_binder::BinderState,
    arena: &NodeArena,
    node_idx: NodeIndex,
) -> Option<tsz_binder::SymbolId> {
    let ident_name = arena.get_identifier_text(node_idx)?;
    let mut scope_id = binder.find_enclosing_scope(arena, node_idx)?;
    while scope_id != tsz_binder::ScopeId::NONE {
        let scope = binder.scopes.get(scope_id.0 as usize)?;
        if let Some(sym_id) = scope.table.get(ident_name) {
            return Some(sym_id);
        }
        scope_id = scope.parent;
    }
    None
}

/// Resolve a symbol name across the main binder, global index, all binders,
/// and lib contexts.
///
/// This consolidates the multi-tier fallback pattern that was previously
/// inlined in augmentation resolver closures (with a per-call
/// `RefCell<FxHashMap>` cache that added complexity for negligible benefit
/// given the O(1) nature of each tier).
pub(crate) fn resolve_name_to_lib_symbol(
    name: &str,
    primary_binder: &tsz_binder::BinderState,
    global_file_locals_index: Option<&FileLocalsIndex>,
    all_binders: Option<&[std::sync::Arc<tsz_binder::BinderState>]>,
    lib_contexts: &[crate::context::LibContext],
) -> Option<tsz_binder::SymbolId> {
    // Tier 1: primary binder file_locals (O(1))
    if let Some(sym) = primary_binder.file_locals.get(name) {
        return Some(sym);
    }
    // Tier 2: global file_locals index (O(1))
    if let Some(idx) = global_file_locals_index {
        if let Some(entries) = idx.get(name)
            && let Some(&(_file_idx, sym_id)) = entries.first()
        {
            return Some(sym_id);
        }
    } else if let Some(binders) = all_binders {
        // Tier 2b: O(N) binder scan only when no global index
        for binder in binders {
            if let Some(found_sym) = binder.file_locals.get(name) {
                return Some(found_sym);
            }
        }
    }
    // Tier 3: lib contexts
    lib_contexts
        .iter()
        .find_map(|ctx| ctx.binder.file_locals.get(name))
}

/// Resolve a `NodeIndex` to a `SymbolId` by searching across declaration
/// arenas and then all lib context binders.
///
/// This is the stable resolution path for per-lib-context lowering (e.g.,
/// `resolve_lib_type_with_params`) where the main file's merged binder is
/// not yet available or the symbol lookup must span individual lib binders.
///
/// The lookup order is:
/// 1. Iterate `decl_arenas`; for each arena that yields identifier text at
///    `node_idx`, search all `lib_contexts` binders for a matching symbol.
/// 2. If no declaration arena matched, try `fallback_arena` with the same
///    lib-contexts search.
///
/// Returns `None` when the identifier is a compiler-managed type (e.g.,
/// `__String`) or when no matching symbol is found.
pub(crate) fn resolve_lib_node_in_lib_contexts(
    node_idx: NodeIndex,
    decl_arenas: &[(NodeIndex, &NodeArena)],
    fallback_arena: &NodeArena,
    lib_contexts: &[crate::context::LibContext],
) -> Option<tsz_binder::SymbolId> {
    for (_, arena) in decl_arenas {
        if let Some(ident_name) = arena.get_identifier_text(node_idx) {
            if is_compiler_managed_type(ident_name) {
                return None;
            }
            for ctx in lib_contexts {
                if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
                    return Some(found_sym);
                }
            }
            break;
        }
    }
    let ident_name = fallback_arena.get_identifier_text(node_idx)?;
    if is_compiler_managed_type(ident_name) {
        return None;
    }
    for ctx in lib_contexts {
        if let Some(found_sym) = ctx.binder.file_locals.get(ident_name) {
            return Some(found_sym);
        }
    }
    None
}

/// Resolve a `NodeIndex` to a `SymbolId` using the augmentation resolution
/// strategy: node-symbol lookup → scope-chain walk → name-based multi-tier
/// fallback.
///
/// This consolidates the resolver closure that was duplicated in every
/// `lower_with_arena` augmentation helper across `lib_resolution.rs` and
/// `lib.rs`.  The three tiers are:
/// 1. `binder.get_node_symbol(node_idx)` — direct AST node → symbol binding.
/// 2. `resolve_scope_chain(...)` — lexical scope walk from the node's enclosing
///    scope up to root.
/// 3. `resolve_name_to_lib_symbol(...)` — `file_locals` / global index / all-binders
///    / lib-contexts multi-tier fallback (same as standalone function above).
///
/// Returns `None` for compiler-managed types (e.g., `__String`).
pub(crate) fn resolve_augmentation_node(
    binder: &tsz_binder::BinderState,
    arena: &NodeArena,
    node_idx: NodeIndex,
    global_file_locals_index: Option<&FileLocalsIndex>,
    all_binders: Option<&[std::sync::Arc<tsz_binder::BinderState>]>,
    lib_contexts: &[crate::context::LibContext],
) -> Option<tsz_binder::SymbolId> {
    if let Some(sym_id) = binder.get_node_symbol(node_idx) {
        return Some(sym_id);
    }
    if let Some(sym_id) = resolve_scope_chain(binder, arena, node_idx) {
        return Some(sym_id);
    }
    let ident_name = arena.get_identifier_text(node_idx)?;
    if is_compiler_managed_type(ident_name) {
        return None;
    }
    resolve_name_to_lib_symbol(
        ident_name,
        binder,
        global_file_locals_index,
        all_binders,
        lib_contexts,
    )
}

impl<'a> CheckerState<'a> {
    // Section 45: Symbol Resolution Utilities
    // ----------------------------------------

    pub(crate) fn resolve_lib_symbol_by_name(&self, name: &str) -> Option<tsz_binder::SymbolId> {
        let lib_binders = self.get_lib_binders();
        self.ctx.binder.file_locals.get(name).or_else(|| {
            self.ctx
                .binder
                .get_global_type_with_libs(name, &lib_binders)
        })
    }

    pub(crate) fn resolve_lib_symbol_by_entity_name(
        &self,
        name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let normalized = name.strip_prefix("globalThis.").unwrap_or(name);
        self.resolve_lib_symbol_by_name(normalized).or_else(|| {
            normalized
                .rsplit('.')
                .next()
                .filter(|tail| *tail != normalized)
                .and_then(|tail| self.resolve_lib_symbol_by_name(tail))
        })
    }

    pub(crate) fn resolve_lib_type_by_entity_name(&mut self, name: &str) -> Option<TypeId> {
        let normalized = name.strip_prefix("globalThis.").unwrap_or(name);
        self.resolve_lib_type_by_name(normalized).or_else(|| {
            normalized
                .rsplit('.')
                .next()
                .filter(|tail| *tail != normalized)
                .and_then(|tail| self.resolve_lib_type_by_name(tail))
        })
    }

    /// Whether `name`'s `resolve_lib_type_by_name` call is currently on the stack
    /// (an in-progress base) rather than already resolved. Matches the same
    /// `globalThis.`/qualified-tail normalization the resolver caches under (#12299).
    pub(crate) fn lib_name_resolution_in_progress(&self, name: &str) -> bool {
        lib_resolution_mark(name) == Some(LibResolutionMark::InProgress)
    }

    /// Whether `name`'s most recent resolution was incomplete because a heritage
    /// base was dropped mid-cycle (used to propagate the taint to derived types).
    pub(crate) fn lib_name_heritage_incomplete(&self, name: &str) -> bool {
        lib_resolution_mark(name) == Some(LibResolutionMark::Incomplete)
    }

    /// Prime a generic lib type referenced from a declaration being lowered:
    /// register its `DefId` and type parameters without materializing its
    /// member body. The lowered `Application` then carries the correct arity
    /// while the referenced interface's body resolves on demand when the
    /// application is structurally consumed (issue #12101). Shared by both the
    /// generic (type-argument) and non-generic reference arms of the
    /// `resolve_lib_type_by_name` prewarm walk.
    fn prime_referenced_lib_type_params(
        &mut self,
        name: &str,
        prewarmed: &mut FxHashMap<tsz_solver::DefId, Vec<tsz_solver::TypeParamInfo>>,
    ) {
        self.prime_lib_type_params(name);
        if let Some(ref_sym_id) = self.ctx.binder.file_locals.get(name) {
            // A user interface merging into a lib interface hoists lib globals
            // into the primary binder's file_locals, so this SymbolId is a
            // merged identity: the raw lookup can answer with a colliding def
            // from another lib binder (FlatArray -> eval; the lib-def identity
            // collision family). Verify against the requested name.
            let def_id = self.ctx.lib_def_id_verified(name, ref_sym_id);
            if let Some(params) = self.ctx.get_def_type_params(def_id)
                && !params.is_empty()
            {
                prewarmed.insert(def_id, params);
            }
        }
    }

    /// Resolve a library type by name, draining cycle-incomplete names at the
    /// outermost call boundary.
    ///
    /// `resolve_lib_type_by_name_inner` resolves one name, but a mutual lib
    /// heritage cycle (the DOM `Element` ↔ `Node` ↔ `HTMLElement` diamond,
    /// #12299) has no resolution order in which every base is already complete:
    /// whichever interface resolves first sees the other still in-progress and
    /// drops it, producing a base-less (`Incomplete`) body. The dropped base is
    /// often a *nested* interface (`Element` reached through `Node`), not the
    /// requested name, so the trigger is "any name was left incomplete", not
    /// "this name". Once the whole call stack unwinds (`depth == 0`) nothing is
    /// in-progress, so re-resolving each `Incomplete` name finds its bases
    /// resolvable and merges them into a flattened body. The flattened (not
    /// intersection) shape keeps generic inference over DOM types intact.
    pub(crate) fn resolve_lib_type_by_name(&mut self, name: &str) -> Option<TypeId> {
        enter_lib_resolution();
        let result = self.resolve_lib_type_by_name_inner(name);
        let depth_after = leave_lib_resolution();

        // Only the outermost call drains, never while already draining, and only
        // when the cascade left some cycle-incomplete name behind. A nested
        // interface (the DOM `Element` reached through `Node`) is rarely the
        // outermost request, so the trigger is "any incomplete", not "this name".
        if depth_after != 0 || lib_resolution_is_draining() || !has_incomplete_lib_names() {
            return result;
        }

        set_lib_resolution_draining(true);
        self.drain_incomplete_lib_heritage(collect_incomplete_lib_names());
        set_lib_resolution_draining(false);

        // The drain rewired the def body / caches for `name` if it was part of a
        // cycle; return the now-complete type so the caller does not keep the
        // base-less value computed during the cycle.
        self.ctx
            .lib_type_resolution_caches
            .types
            .get(name)
            .and_then(|cached| *cached)
            .or(result)
    }

    /// Re-resolve cycle-incomplete lib names until none remain or no further
    /// progress is made. Each re-resolution runs through the public entry so
    /// nested base lookups still increment depth, but `LIB_RESOLUTION_DRAINING`
    /// suppresses a recursive drain. Bounded by the number of names left
    /// incomplete to guarantee termination even if a genuine gap never resolves.
    fn drain_incomplete_lib_heritage(&mut self, mut incomplete: Vec<String>) {
        let max_passes = incomplete.len().saturating_add(1);
        for _ in 0..max_passes {
            if incomplete.is_empty() {
                break;
            }
            for lib_name in &incomplete {
                // The cycle pass removed the name from the resolution cache, so
                // this recomputes from declarations against the now-cached bases.
                let _ = self.resolve_lib_type_by_name(lib_name);
            }
            let remaining = collect_incomplete_lib_names();
            if remaining.len() >= incomplete.len() {
                // No name cleared this pass — re-resolving cannot make further
                // progress (a base is genuinely unresolvable). Clear the stale
                // markers so later top-level resolutions do not re-drain them.
                for lib_name in &remaining {
                    clear_lib_resolution_mark(lib_name);
                }
                break;
            }
            incomplete = remaining;
        }
    }

    /// Resolve a single library type by name from lib.d.ts and other library
    /// contexts. The public `resolve_lib_type_by_name` wraps this with the
    /// heritage-cycle drain.
    ///
    /// ## Library Contexts:
    /// - Searches through loaded library contexts (lib.d.ts, es2015.d.ts, etc.)
    /// - Each lib context has its own binder and arena
    /// - Types are "lowered" from lib arena to main arena
    ///
    /// ## Declaration Merging:
    /// - Interfaces can have multiple declarations that are merged
    /// - All declarations are lowered together to create merged type
    /// - Essential for types like `Array` which have multiple lib declarations
    ///
    /// ## Global Augmentations:
    /// - User's `declare global` blocks are merged with lib types
    /// - Allows extending built-in types like `Window`, `String`, etc.
    ///
    /// ## Examples:
    /// ```typescript
    /// // Built-in types from lib.d.ts
    /// let arr: Array<number>;  // resolve_lib_type_by_name("Array")
    /// let obj: Object;         // resolve_lib_type_by_name("Object")
    /// let prom: Promise<string>; // resolve_lib_type_by_name("Promise")
    ///
    /// // Global augmentation
    /// declare global {
    ///   interface Window {
    ///     myCustomProperty: string;
    ///   }
    /// }
    /// // lib Window type is merged with augmentation
    /// ```
    fn resolve_lib_type_by_name_inner(&mut self, name: &str) -> Option<TypeId> {
        use tsz_lowering::TypeLowering;

        // When TS5107/TS5101 deprecation diagnostics are present, skip all lib type
        // resolution. tsc stops compilation at TS5107 and never resolves lib types.
        // We still walk the AST for grammar errors (17xxx), but short-circuit type
        // resolution to avoid the O(n²) memory explosion from multiple files
        // independently resolving deep es5 heritage chains.
        if self.ctx.skip_lib_type_resolution {
            return None;
        }

        // TS 6.0 lib intrinsic: resolves to `undefined` when
        // `strictBuiltinIteratorReturn` is enabled (implied by `--strict`),
        // or `any` when disabled.
        if name == "BuiltinIteratorReturn" {
            return if self.ctx.compiler_options.strict_builtin_iterator_return {
                Some(TypeId::UNDEFINED)
            } else {
                Some(TypeId::ANY)
            };
        }

        if name == "Array"
            && self.ctx.share_owner_symbol_type_results
            && !self.ctx.emit_declarations()
            && !self.lib_name_has_local_augmentation(name)
            && let Some(ty) = self
                .ctx
                .types
                .get_array_display_base_type()
                .or_else(|| TypeResolver::get_array_base_type(&self.ctx.types))
        {
            self.ctx
                .lib_type_resolution_caches
                .types
                .insert(name.to_string(), Some(ty));
            return Some(ty);
        }

        if let Some(cached) = self.ctx.lib_type_resolution_caches.types.get(name)
            && self.cached_lib_type_is_usable(name, *cached)
        {
            return *cached;
        }
        // Skip shared cache when this checker locally augments `name`, or when
        // shared-owner parallel checking could observe a heritage-thin shape.
        if !self.lib_name_requires_parallel_local_resolution(name)
            && let Some(ref shared) = self.ctx.shared_lib_type_cache
            && let Some(entry) = shared.get(name)
        {
            let cached = *entry;
            if self.cached_lib_type_is_usable(name, cached) {
                self.ctx
                    .lib_type_resolution_caches
                    .types
                    .insert(name.to_string(), cached);
                return cached;
            }
        }

        tracing::trace!(name, "resolve_lib_type_by_name: called");
        // Mark this name as in-progress. Recursive lib graphs such as
        // `Promise`/`PromiseLike` can re-enter through generic-reference
        // prewarming before the final cache write below; returning `None` for
        // the in-progress edge breaks that cycle and the completed result
        // overwrites this sentinel.
        self.ctx
            .lib_type_resolution_caches
            .types
            .insert(name.to_string(), None);
        // Record that `name`'s resolution is on the stack so a heritage merge of
        // a derived interface reached transitively from here can tell this
        // in-progress base apart from a genuinely-missing one (#12299). A nested
        // resolve for the same name is short-circuited by the sentinel cache hit
        // above, so this marker is only installed by the outermost call.
        set_lib_resolution_mark(name, LibResolutionMark::InProgress);
        let mut lib_type_id: Option<TypeId> = None;
        let mut heritage_incomplete = false;
        let factory = self.ctx.types.factory();
        let mut symbol_has_interface = false;
        let mut selected_lib_def_id = None;

        let lib_contexts = self.ctx.lib_contexts.clone();
        // Collect lowered types from the symbol's declarations.
        // The main file's binder already has merged declarations from all lib files.
        let mut lib_types: Vec<TypeId> = Vec::new();

        let lib_binders = self.get_lib_binders();
        let sym_id = if self.ctx.file_local_type_shadow_for_lib_name(name) {
            None
        } else {
            self.ctx.binder.file_locals.get(name)
        }
        .or_else(|| {
            self.ctx
                .binder
                .get_global_type_with_libs(name, &lib_binders)
        })
        // Cross-file lookup binders (cross-arena delegation) keep
        // `file_locals` per-file and carry the hoisted lib-origin globals in
        // `program_globals`. Resolve through it BEFORE the per-lib-context
        // fallback below: the program-space symbol carries the declarations
        // merged across ALL lib files, while a single lib context's symbol
        // only carries that one file's declarations — lowering the latter
        // produces a partial interface body (e.g. `SymbolConstructor`
        // without `asyncDispose`), making derived types and diagnostics
        // depend on which file's checker resolved the name first.
        // `program_globals` is empty on primary (lib-merged) binders, so
        // this changes nothing there.
        .or_else(|| self.ctx.binder.program_global_type(name))
        .or_else(|| {
            resolve_name_to_lib_symbol(
                name,
                self.ctx.binder,
                self.ctx.global_file_locals_index.as_deref(),
                self.ctx
                    .all_binders
                    .as_ref()
                    .map(|binders| binders.as_ref().as_slice()),
                &self.ctx.lib_contexts,
            )
        });

        let selected_symbol = selected_lib_symbol_for_name(&self.ctx, name, sym_id, &lib_binders);

        if let Some((sym_id, selected_binder_arc)) = selected_symbol {
            let selected_from_lib_context = selected_binder_arc.is_some();
            let selected_binder = selected_binder_arc.as_deref().unwrap_or(self.ctx.binder);
            // Get the symbol's declaration(s) from the main file's binder
            if let Some(symbol) = selected_binder.get_symbol_with_libs(sym_id, &lib_binders) {
                symbol_has_interface = symbol.has_any_flags(tsz_binder::symbol_flags::INTERFACE);
                let fallback_arena = resolve_lib_fallback_arena(
                    selected_binder,
                    sym_id,
                    &lib_contexts,
                    self.ctx.arena,
                );

                let mut decls_with_arenas = collect_lib_decls_with_arenas_in_contexts(
                    selected_binder,
                    sym_id,
                    &symbol.declarations,
                    fallback_arena,
                    &lib_contexts,
                    Some(self.ctx.arena),
                );
                // A single-lib current binder (lib-baseline diagnostics pass)
                // owns only its own file's declarations for a merged global;
                // union in the sibling lib contexts' same-named declarations
                // so the lowered body is the full cross-lib merge, not a
                // partial view that then gets published as canonical.
                if !selected_from_lib_context
                    && crate::state_type_analysis::cross_file_direct::is_builtin_lib_declaration_arena(
                        self.ctx.arena,
                    )
                {
                    super::lib_decls::extend_decls_with_lib_context_globals(
                        name,
                        &lib_contexts,
                        &mut decls_with_arenas,
                    );
                }
                let interface_canonical_sym_id = canonical_interface_symbol_id(
                    &self.ctx,
                    name,
                    sym_id,
                    selected_from_lib_context,
                );
                // Structural guard: a NON-lib (user/program-file) interface that
                // declares `extends` heritage must not have its heritage-LESS
                // lowering published here as the canonical body. `resolve_lib_type_
                // by_name` lowers only the interface's own members; the
                // class/interface-inherited members are merged later by the
                // interface-type path. Publishing the bare body shadows that merged
                // form for a merged interface+value symbol that is `export default`-
                // ed (the value-side path re-enters this resolution mid-merge),
                // dropping class-inherited members (false TS2339). Real lib
                // interfaces (`Array extends ReadonlyArray`) keep the existing path:
                // their heritage is reconciled through `merge_lib_interface_heritage`
                // on both sides. For the guarded case, hand back the symbol's lazy
                // reference so consumers resolve to the heritage-complete body once
                // the interface-type path computes it. The lazy ref is captured here
                // under a mutable borrow that ends before the lowering closures
                // below (which hold an immutable `self.ctx` borrow).
                let user_interface_has_heritage = !selected_from_lib_context
                    && !self.ctx.symbol_is_from_lib(sym_id)
                    && !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                    && decls_with_arenas.iter().any(|&(decl_idx, decl_arena)| {
                        decl_arena
                            .get(decl_idx)
                            .and_then(|node| decl_arena.get_interface(node))
                            .and_then(|iface| iface.heritage_clauses.as_ref())
                            .is_some_and(|clauses| !clauses.nodes.is_empty())
                    });
                let user_interface_heritage_lazy_ref = if user_interface_has_heritage {
                    Some(self.ctx.create_lazy_type_ref(interface_canonical_sym_id))
                } else {
                    None
                };
                let mut prewarmed_lazy_type_params = rustc_hash::FxHashMap::default();
                // Defer generic-reference bodies by default (#12101): prime only
                // the referenced type's arity and resolve its body on demand,
                // instead of eagerly materializing it during the prewarm walk.
                let defer_generic_prewarm = !lib_generic_prewarm_defer_disabled();
                for (decl_idx, decl_arena) in &decls_with_arenas {
                    // Generic references in a `extends`/`implements` heritage
                    // clause are NOT deferred: the base's members are merged into
                    // this interface and feed structural checks (assignability,
                    // rest/spread arity), so the base must materialize during
                    // lowering rather than as a deferred `Application`. Only
                    // member/parameter-position generic references (e.g. the
                    // `MessageEvent<T>` in an `on*` handler property) defer.
                    let heritage_nodes = collect_heritage_subtree_nodes(decl_arena, *decl_idx);
                    let mut stack = vec![*decl_idx];
                    while let Some(node_idx) = stack.pop() {
                        let Some(node) = decl_arena.get(node_idx) else {
                            continue;
                        };
                        if node.kind == syntax_kind_ext::TYPE_REFERENCE
                            && let Some(type_ref) = decl_arena.get_type_ref(node)
                        {
                            let has_type_args = type_ref
                                .type_arguments
                                .as_ref()
                                .is_some_and(|args| !args.nodes.is_empty());
                            if has_type_args
                                && let Some(ref_sym_id) = resolve_lib_node_in_arenas(
                                    selected_binder,
                                    type_ref.type_name,
                                    &decls_with_arenas,
                                    fallback_arena,
                                )
                                && ref_sym_id != sym_id
                            {
                                let ref_name = self
                                    .ctx
                                    .binder
                                    .get_symbol_with_libs(ref_sym_id, &lib_binders)
                                    .or_else(|| {
                                        selected_binder
                                            .get_symbol_with_libs(ref_sym_id, &lib_binders)
                                    })
                                    .map(|symbol| symbol.escaped_name.clone());
                                if let Some(ref_name) = ref_name {
                                    if defer_generic_prewarm && !heritage_nodes.contains(&node_idx)
                                    {
                                        self.prime_referenced_lib_type_params(
                                            &ref_name,
                                            &mut prewarmed_lazy_type_params,
                                        );
                                    } else {
                                        let _ = self.resolve_lib_type_by_name(&ref_name);
                                    }
                                }
                            }
                            if !has_type_args
                                && let Some(name_node) = decl_arena.get(type_ref.type_name)
                                && name_node.kind == SyntaxKind::Identifier as u16
                                && let Some(name) =
                                    decl_arena.get_identifier_text(type_ref.type_name)
                            {
                                self.prime_referenced_lib_type_params(
                                    name,
                                    &mut prewarmed_lazy_type_params,
                                );
                            }
                        }
                        stack.extend(decl_arena.get_children(node_idx));
                    }
                }

                let binder = selected_binder;
                let resolver = |node_idx: NodeIndex| -> Option<u32> {
                    resolve_lib_node_in_arenas(binder, node_idx, &decls_with_arenas, fallback_arena)
                        .map(|sym_id| sym_id.0)
                };
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
                    lib_def_id_from_node(
                        &self.ctx,
                        binder,
                        node_idx,
                        &decls_with_arenas,
                        fallback_arena,
                    )
                };
                let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
                    self.resolve_actual_lib_name_to_def_id_for_lowering(type_name)
                        .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
                };

                let lazy_type_params_resolver = |def_id: tsz_solver::def::DefId| {
                    prewarmed_lazy_type_params
                        .get(&def_id)
                        .cloned()
                        .or_else(|| self.ctx.get_def_type_params(def_id))
                };

                // Create base lowering with the fallback arena and both resolvers
                let lowering = TypeLowering::with_hybrid_resolver(
                    fallback_arena,
                    self.ctx.types,
                    &resolver,
                    &def_id_resolver,
                    &resolver,
                )
                .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
                .with_lazy_type_params_resolver(&lazy_type_params_resolver)
                .with_name_def_id_resolver(&name_resolver);
                // Name-first unconditionally: lib declarations reference global
                // lib type names cross-arena, and the priming phase runs without
                // the parallel-only indices (see `resolve_lib_type_with_params`).
                // This matches the parallel path, where `all_binders` made the
                // old gate always true.
                let lowering = lowering.prefer_name_def_id_resolution();

                // Try to lower as interface first (handles declaration merging)
                if !symbol.declarations.is_empty() {
                    // Check if any declaration is a type alias — if so, skip interface
                    // lowering. Type aliases like Record<K,T>, Partial<T>, Pick<T,K>
                    // would incorrectly succeed interface lowering with 0 type params,
                    // preventing the proper type alias path from running.
                    let is_type_alias = symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS);

                    if !is_type_alias {
                        let deduped = dedup_decl_arenas(&decls_with_arenas);

                        // Skip publishing the heritage-LESS body for a user interface
                        // with `extends` heritage; push its lazy reference (captured
                        // above, see `user_interface_heritage_lazy_ref`) so consumers
                        // resolve the heritage-complete body computed by the
                        // interface-type path instead.
                        if let Some(lazy_ref) = user_interface_heritage_lazy_ref {
                            lib_types.push(lazy_ref);
                        } else {
                            // Use lower_merged_interface_declarations for proper multi-arena support.
                            // Pass sym_id so the resulting Object type gets stamped with the
                            // interface's symbol — this allows the formatter to display the
                            // named form (e.g., "Num") instead of the structural expansion.
                            let (ty, params) = lowering
                                .lower_merged_interface_declarations_with_symbol(
                                    &deduped,
                                    Some(interface_canonical_sym_id),
                                );

                            // If lowering succeeded (not ERROR), use the result
                            if ty != TypeId::ERROR {
                                // Register DefId, type params, and body in one step.
                                let def_id = register_selected_lib_def_resolved(
                                    &self.ctx,
                                    name,
                                    sym_id,
                                    selected_from_lib_context,
                                    ty,
                                    params,
                                );
                                selected_lib_def_id.get_or_insert(def_id);

                                lib_types.push(ty);
                            }
                        }
                    }

                    // Interface lowering skipped or returned ERROR - try as type alias
                    // Type aliases like Partial<T>, Pick<T,K>, Record<K,T> have their
                    // declaration in symbol.declarations but are not interface nodes
                    if lib_types.is_empty() {
                        for (decl_idx, decl_arena) in &decls_with_arenas {
                            if let Some(node) = decl_arena.get(*decl_idx)
                                && let Some(alias) = decl_arena.get_type_alias(node)
                            {
                                let alias_lowering = lowering.with_arena(decl_arena);
                                let (ty, params) =
                                    alias_lowering.lower_type_alias_declaration(alias);
                                if ty != TypeId::ERROR {
                                    // Register DefId, type params, and body in one step.
                                    let def_id = register_selected_lib_def_resolved(
                                        &self.ctx,
                                        name,
                                        sym_id,
                                        selected_from_lib_context,
                                        ty,
                                        params,
                                    );
                                    selected_lib_def_id.get_or_insert(def_id);

                                    // CRITICAL: Return Lazy(DefId) instead of the structural body.
                                    // Application types only expand when the base is Lazy, not when
                                    // it's the actual MappedType/Object/etc. This allows evaluate_application
                                    // to trigger and substitute type parameters correctly.
                                    let lazy_type = self.ctx.types.factory().lazy(def_id);
                                    lib_types.push(lazy_type);

                                    // Type aliases don't merge across files, take the first one
                                    break;
                                }
                            }
                        }
                    }
                }

                // For value declarations (vars, consts, functions)
                let decl_idx = symbol.value_declaration;
                if decl_idx.0 != u32::MAX {
                    // Get the correct arena for the value declaration from main binder
                    let value_arena = self
                        .ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        .map_or(fallback_arena, |arc| arc.as_ref());
                    let value_lowering = if value_arena
                        .get(decl_idx)
                        .and_then(|node| value_arena.get_source_file(node))
                        .is_some_and(|source| {
                            source.is_declaration_file
                                && source.file_name.starts_with("lib.")
                                && source.file_name.ends_with(".d.ts")
                        }) {
                        lowering
                            .with_arena(value_arena)
                            .prefer_name_def_id_resolution()
                    } else {
                        lowering.with_arena(value_arena)
                    };
                    let val_type = value_lowering.lower_type(decl_idx);
                    // Only include non-ERROR types. Value declaration lowering can fail
                    // when type references (e.g., `PromiseConstructor`) can't be resolved
                    // during TypeLowering. Including ERROR in the lib_types vector would
                    // cause intersection2 to collapse a valid interface type to ERROR.
                    if val_type != TypeId::ERROR {
                        lib_types.push(val_type);
                    }
                }
            }
        }

        for ty in lib_types.iter().copied() {
            if crate::query_boundaries::common::lazy_def_id(self.ctx.types, ty).is_some() {
                continue;
            }
            self.ensure_relation_input_ready(ty);
        }

        // Merge repeated lib interface declarations using interface-merge
        // semantics instead of a raw intersection. Constructor interfaces like
        // `RangeErrorConstructor` are split across multiple lib files
        // (`lib.es5.d.ts`, `lib.es2022.error.d.ts`), and intersecting their
        // callable shapes can drop constructor signatures from the merged type.
        // Non-interface lib entities still use intersection semantics.
        if lib_types.len() == 1 {
            lib_type_id = Some(lib_types[0]);
        } else if lib_types.len() > 1 {
            let mut merged = lib_types[0];
            for &ty in &lib_types[1..] {
                merged = if symbol_has_interface {
                    self.merge_interface_types(merged, ty)
                } else {
                    factory.intersection2(merged, ty)
                };
            }
            lib_type_id = Some(merged);
        }

        // Merge heritage (extends) from lib interface declarations.
        // This propagates base interface members (e.g., Iterator.next() into ArrayIterator).
        if let Some(ty) = lib_type_id {
            let (merged, incomplete) = self.merge_lib_interface_heritage(ty, name);
            lib_type_id = Some(merged);
            heritage_incomplete = incomplete;
        }

        // Merge global augmentations (declare global { interface X { ... } }).
        if let Some(merged) = self.merge_global_augmentations(name, lib_type_id, &lib_contexts) {
            lib_type_id = Some(merged);
        }

        // Local-only cache write so this checker's same-thread calls see the
        // augmented type. The SHARED cache is written exactly once at function
        // exit — augmentation-heritage below can still mutate `lib_type_id`,
        // and a concurrent reader snapshotting an intermediate value would
        // freeze it in their local cache and never re-resolve.
        if let Some(ty) = lib_type_id {
            self.ctx
                .lib_type_resolution_caches
                .types
                .insert(name.to_string(), Some(ty));

            // Register the final merged type in type_to_def so the formatter can
            // display "Date" instead of expanding all members. The initial
            // registration uses the pre-merge TypeId which changes after heritage
            // merging and global augmentations add more members.
            let selected_is_identity = selected_lib_def_id.is_some_and(|def_id| {
                crate::query_boundaries::lib_augmentations::is_lazy_def_identity(
                    self.ctx.types,
                    ty,
                    def_id,
                )
            });
            let name_atom = self.ctx.types.intern_string(name);
            let canonical_def_id = self
                .ctx
                .definition_store
                .find_defs_by_name(name_atom)
                .and_then(|defs| defs.first().copied());
            if let Some(def_id) = canonical_def_id
                && !selected_is_identity
                && !crate::query_boundaries::lib_augmentations::is_lazy_def_identity(
                    self.ctx.types,
                    ty,
                    def_id,
                )
            {
                self.ctx.definition_store.register_type_to_def(ty, def_id);
            }
        }

        // Process heritage clauses from global augmentations.
        // This is in a separate block because lower_with_arena borrows `self`
        // and we need `&mut self` for resolve_heritage_symbol/get_type_of_symbol.
        if let Some(augmentation_decls) = self.ctx.binder.global_augmentations.get(name)
            && !augmentation_decls.is_empty()
        {
            let current_arena: &NodeArena = self.ctx.arena;
            // Process heritage clauses from augmentation declarations that are in
            // the current file's arena. lower_interface_declarations only merges body
            // members, not extends clauses. User augmentations like
            // `interface Number extends ICloneable {}` need their heritage merged.
            //
            // Note: in parallel compilation, ALL augmentations get tagged with an
            // arena (even same-file ones), so we identify current-file augmentations
            // by checking if the arena pointer matches the current arena.
            //
            // We use a lightweight approach here (manual heritage walk + resolve_heritage_symbol)
            // instead of merge_interface_heritage_types, because that function triggers deep type
            // evaluation via resolve_type_for_interface_merge which can cause infinite loops
            // during lib type resolution.
            let current_arena_ptr = current_arena as *const NodeArena;
            let same_file_aug_nodes: Vec<NodeIndex> = augmentation_decls
                .iter()
                .filter(|aug| {
                    aug.arena
                        .as_ref()
                        .is_none_or(|a| Arc::as_ptr(a) == current_arena_ptr)
                })
                .map(|aug| aug.node)
                .collect();

            for &decl_idx in &same_file_aug_nodes {
                let Some(node) = current_arena.get(decl_idx) else {
                    continue;
                };
                let Some(interface) = current_arena.get_interface(node) else {
                    continue;
                };
                let Some(ref heritage_clauses) = interface.heritage_clauses else {
                    continue;
                };

                for &clause_idx in &heritage_clauses.nodes {
                    let Some(clause_node) = current_arena.get(clause_idx) else {
                        continue;
                    };
                    let Some(heritage) = current_arena.get_heritage_clause(clause_node) else {
                        continue;
                    };
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }

                    for &type_idx in &heritage.types.nodes {
                        let Some(type_node) = current_arena.get(type_idx) else {
                            continue;
                        };
                        let expr_idx =
                            if let Some(eta) = current_arena.get_expr_type_args(type_node) {
                                eta.expression
                            } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                                if let Some(tr) = current_arena.get_type_ref(type_node) {
                                    tr.type_name
                                } else {
                                    type_idx
                                }
                            } else {
                                type_idx
                            };

                        // resolve_heritage_symbol handles simple identifiers, qualified
                        // names, and property access expressions (e.g., EndGate.ICloneable).
                        let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                            continue;
                        };
                        let base_type = self.get_type_of_symbol(base_sym_id);
                        if base_type == TypeId::ERROR || base_type == TypeId::UNKNOWN {
                            continue;
                        }
                        if let Some(current_type) = lib_type_id {
                            let merged =
                                self.merge_interface_types_heritage(current_type, base_type);
                            if merged != current_type {
                                lib_type_id = Some(merged);
                            }
                        }
                    }
                }
            }
        }

        // Finalize after heritage merge — `merge_lib_interface_heritage`
        // above may have produced a new TypeId; helper rewires type→def
        // and the DefId body so literal and annotation paths agree.
        //
        // Gate on `!heritage_incomplete`: publishing a heritage-incomplete
        // (base-dropped) body through `register_finalized_lib_body` writes it
        // into the program-shared `DefinitionStore`, which sibling fresh
        // per-file checkers read via `Lazy(DefId)` resolution. The store is
        // last-writer-wins without the opt-in freeze, so a heritage-thin form
        // (own members only, inherited members missing) published during the
        // DOM `Node`/`Element`/`HTMLElement` cycle (#12299) can be observed by
        // another checker's relation — producing false TS2345/TS2740/TS2322
        // where a derived element interface is not recognized as its
        // transitive base (e.g. `HTMLDivElement` not assignable to `Node`).
        // The depth-0 drain below re-resolves the name once the dropped base
        // has completed and publishes the full body, so skipping the publish
        // here removes only the poisoned intermediate, never the final form.
        if let Some(ty) = lib_type_id
            && !heritage_incomplete
        {
            self.register_finalized_lib_body_for_def(name, ty, selected_lib_def_id);
            // Update the symbol_types cache for the INTERFACE type position.
            // compute_type_of_symbol may have cached a DIFFERENT TypeId
            // when has_local_interface_decl was a false positive (NodeIndex
            // collision), causing it to bypass resolve_lib_type_by_name and
            // use incomplete manual lowering.  We only update when:
            //   1. The symbol exists in file_locals (it's a global type)
            //   2. The cached type differs from the lib-resolved type
            //   3. The cached type was NOT produced by resolve_lib_type_by_name
            //      (first call to this function for this name)
            // This preserves user-file augmentations while fixing the
            // mismatch between annotation and literal type resolution paths.
            // Never overwrite an IMPORT ALIAS's cached type: `file_locals` is
            // name-keyed, so a user import named like a lib type would have
            // its VALUE-side type replaced with this TYPE-position lib type,
            // flipping class constructor/instance identity (#13185).
            if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
                && self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS))
                && let Some(old) = self.ctx.symbol_types.get(&sym_id)
                && old != ty
                && old != TypeId::ERROR
                && old != TypeId::ANY
            {
                // Membership-monotone: a lib interface can be re-resolved to a
                // heritage-thin body (an inherited base dropped while it was
                // itself mid-resolution) without `heritage_incomplete` being set
                // — e.g. `HTMLElement` momentarily resolved without `Node`'s
                // members during the DOM `Node`/`Element`/`HTMLElement` cycle
                // (#12299/#17595). Overwriting the cached body with that thin
                // form makes a later `HTMLElement <: Element` check see a
                // `Node`-less `HTMLElement` and mis-fire a default-lib `TS2430`.
                // Mirror `register_finalized_lib_body_for_def`'s guard, which
                // this direct cache write bypasses: never let a re-derivation
                // that strictly loses members clobber a more-complete cached
                // body (the membership-maximal body wins regardless of the order
                // resolutions finalize).
                if crate::query_boundaries::lib_augmentations::lib_body_strictly_loses_members(
                    self.ctx.types,
                    old,
                    ty,
                ) {
                    // Keep AND re-adopt the maximal body, exactly like the
                    // finalize path does ("keep and re-mirror the existing,
                    // more-complete body"). Skipping only the write while still
                    // returning/caching the thin `ty` splits the interface's
                    // identity: `symbol_types` and the def store keep the
                    // complete body while the name caches and every caller of
                    // this resolution get the thin one. Two distinct `TypeId`s
                    // for one merged interface then meet in a relation and
                    // mis-fire (e.g. `genericMethodOverspecialization`'s
                    // `(e: HTMLElement | null) => number` rejected against
                    // itself, #17641).
                    tracing::trace!(
                        name,
                        ?old,
                        ?ty,
                        "resolve_lib_type_by_name: thin re-derivation rejected; adopting cached maximal body"
                    );
                    lib_type_id = Some(old);
                } else {
                    self.ctx.symbol_types.insert(sym_id, ty);
                }
            }
        }

        if heritage_incomplete {
            // A heritage base was dropped while it was itself mid-resolution
            // (directly, or transitively through another incomplete base), so
            // `lib_type_id` is missing inherited members. Mark `name` incomplete
            // and do NOT persist the type: remove the local slot and skip the
            // shared cache so the next request recomputes once the base has
            // completed. The finalized def-body / `symbol_types` publication is
            // skipped above while incomplete, so the shared store never carries
            // the heritage-thin form; the depth-0 drain's recompute publishes
            // the full body once the dropped base resolves. See #12299.
            set_lib_resolution_mark(name, LibResolutionMark::Incomplete);
            self.ctx.lib_type_resolution_caches.types.remove(name);
            return lib_type_id;
        }

        // `name` resolved completely; clear any in-progress / stale incomplete marker.
        clear_lib_resolution_mark(name);

        // Mutation-isolation campaign: the body finalized above is the
        // program-wide form for this lib def — freeze it so later checkers'
        // re-finalizations (checker-relative TypeIds of the byte-identical
        // semantic form) cannot republish a different body into the shared
        // store. Only on clean completion (the heritage-incomplete recovery
        // above must stay able to overwrite), and only for names without
        // user-side augmentations (an augmented body is relative to the
        // augmenting file set, so the program-wide form is owned by the
        // augmentation-merging path, not this one). Unlike the shared-cache
        // gate below, builtin-merged names (Array, `Intl.*`,
        // iterator-return-dependent interfaces) are still frozen: within one
        // program their finalized bodies are option-stable, and per-file
        // checkers keep using their own `TypeEnvironment` bodies for local
        // resolution.
        // OPT-IN (TSZ_ENABLE_LIB_DEF_FREEZE=1): freezing still regresses the
        // declare-global augmentation class even with the program-wide name
        // gate (witness: importMeta.ts gains a spurious TS2345 — a def
        // RELATED to the augmented name gets pinned through a channel the
        // name gate does not cover). The mutation-isolation campaign needs
        // augmentation-aware freeze invalidation before this can default on.
        if super::lib::lib_def_freeze_enabled()
            && lib_type_id.is_some()
            && !self.any_program_file_augments_lib_name(name)
        {
            self.freeze_finalized_lib_def(name);
        }

        // Generic lib interfaces had their type params cached above.
        self.ctx
            .lib_type_resolution_caches
            .types
            .insert(name.to_string(), lib_type_id);
        if !self.lib_name_requires_parallel_local_resolution(name)
            && let Some(ref shared) = self.ctx.shared_lib_type_cache
        {
            shared.insert(name.to_string(), lib_type_id);
        }

        lib_type_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CheckerOptions;
    use crate::query_boundaries::type_construction::TypeInterner;
    use crate::state::CheckerState;
    use tsz_binder::BinderState;
    use tsz_solver::TypeParamInfo;
    use tsz_solver::construction::QueryDatabase;

    #[test]
    fn keyword_syntax_maps_string() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::StringKeyword as u16),
            Some(TypeId::STRING)
        );
    }

    #[test]
    fn keyword_syntax_maps_number() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::NumberKeyword as u16),
            Some(TypeId::NUMBER)
        );
    }

    #[test]
    fn keyword_syntax_maps_boolean() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::BooleanKeyword as u16),
            Some(TypeId::BOOLEAN)
        );
    }

    #[test]
    fn keyword_syntax_maps_void() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::VoidKeyword as u16),
            Some(TypeId::VOID)
        );
    }

    #[test]
    fn keyword_syntax_maps_never() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::NeverKeyword as u16),
            Some(TypeId::NEVER)
        );
    }

    #[test]
    fn keyword_syntax_maps_any() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::AnyKeyword as u16),
            Some(TypeId::ANY)
        );
    }

    #[test]
    fn keyword_syntax_maps_unknown() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::UnknownKeyword as u16),
            Some(TypeId::UNKNOWN)
        );
    }

    #[test]
    fn keyword_syntax_maps_null() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::NullKeyword as u16),
            Some(TypeId::NULL)
        );
    }

    #[test]
    fn keyword_syntax_maps_undefined() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::UndefinedKeyword as u16),
            Some(TypeId::UNDEFINED)
        );
    }

    #[test]
    fn keyword_syntax_maps_object() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::ObjectKeyword as u16),
            Some(TypeId::OBJECT)
        );
    }

    #[test]
    fn keyword_syntax_maps_symbol() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::SymbolKeyword as u16),
            Some(TypeId::SYMBOL)
        );
    }

    #[test]
    fn keyword_syntax_maps_bigint() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::BigIntKeyword as u16),
            Some(TypeId::BIGINT)
        );
    }

    #[test]
    fn keyword_syntax_returns_none_for_non_keyword() {
        assert_eq!(keyword_syntax_to_type_id(0), None);
        assert_eq!(keyword_syntax_to_type_id(9999), None);
    }

    #[test]
    fn keyword_name_maps_all_primitives() {
        assert_eq!(keyword_name_to_type_id("string"), Some(TypeId::STRING));
        assert_eq!(keyword_name_to_type_id("number"), Some(TypeId::NUMBER));
        assert_eq!(keyword_name_to_type_id("boolean"), Some(TypeId::BOOLEAN));
        assert_eq!(keyword_name_to_type_id("void"), Some(TypeId::VOID));
        assert_eq!(
            keyword_name_to_type_id("undefined"),
            Some(TypeId::UNDEFINED)
        );
        assert_eq!(keyword_name_to_type_id("null"), Some(TypeId::NULL));
        assert_eq!(keyword_name_to_type_id("never"), Some(TypeId::NEVER));
        assert_eq!(keyword_name_to_type_id("unknown"), Some(TypeId::UNKNOWN));
        assert_eq!(keyword_name_to_type_id("any"), Some(TypeId::ANY));
        assert_eq!(keyword_name_to_type_id("object"), Some(TypeId::OBJECT));
        assert_eq!(keyword_name_to_type_id("symbol"), Some(TypeId::SYMBOL));
        assert_eq!(keyword_name_to_type_id("bigint"), Some(TypeId::BIGINT));
    }

    #[test]
    fn keyword_name_returns_none_for_non_keyword() {
        assert_eq!(keyword_name_to_type_id("Promise"), None);
        assert_eq!(keyword_name_to_type_id("Array"), None);
        assert_eq!(keyword_name_to_type_id("String"), None);
        assert_eq!(keyword_name_to_type_id(""), None);
    }

    #[test]
    fn keyword_name_and_syntax_agree() {
        let pairs = [
            ("string", SyntaxKind::StringKeyword),
            ("number", SyntaxKind::NumberKeyword),
            ("boolean", SyntaxKind::BooleanKeyword),
            ("void", SyntaxKind::VoidKeyword),
            ("undefined", SyntaxKind::UndefinedKeyword),
            ("null", SyntaxKind::NullKeyword),
            ("never", SyntaxKind::NeverKeyword),
            ("unknown", SyntaxKind::UnknownKeyword),
            ("any", SyntaxKind::AnyKeyword),
            ("object", SyntaxKind::ObjectKeyword),
            ("symbol", SyntaxKind::SymbolKeyword),
            ("bigint", SyntaxKind::BigIntKeyword),
        ];
        for (name, kind) in pairs {
            assert_eq!(
                keyword_name_to_type_id(name),
                keyword_syntax_to_type_id(kind as u16),
                "Mismatch for keyword '{name}'"
            );
        }
    }

    #[test]
    fn dedup_empty() {
        let result = dedup_decl_arenas(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn dedup_single() {
        let arena = NodeArena::default();
        let idx = NodeIndex(0);
        let input = [(idx, &arena)];
        let result = dedup_decl_arenas(&input);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn dedup_same_arena_same_index() {
        let arena = NodeArena::default();
        let idx = NodeIndex(0);
        let input = [(idx, &arena), (idx, &arena)];
        let result = dedup_decl_arenas(&input);
        assert_eq!(
            result.len(),
            1,
            "Duplicate (same arena, same index) should be removed"
        );
    }

    #[test]
    fn dedup_different_arenas_same_index() {
        let arena1 = NodeArena::default();
        let arena2 = NodeArena::default();
        let idx = NodeIndex(0);
        let input = [(idx, &arena1), (idx, &arena2)];
        let result = dedup_decl_arenas(&input);
        assert_eq!(
            result.len(),
            2,
            "Same index from different arenas should be kept"
        );
    }

    #[test]
    fn dedup_same_arena_different_indices() {
        let arena = NodeArena::default();
        let idx0 = NodeIndex(0);
        let idx1 = NodeIndex(1);
        let input = [(idx0, &arena), (idx1, &arena)];
        let result = dedup_decl_arenas(&input);
        assert_eq!(
            result.len(),
            2,
            "Different indices from same arena should be kept"
        );
    }

    // ---- no_value_resolver ----

    #[test]
    fn no_value_resolver_always_returns_none() {
        assert_eq!(super::no_value_resolver(NodeIndex(0)), None);
        assert_eq!(super::no_value_resolver(NodeIndex(42)), None);
        assert_eq!(super::no_value_resolver(NodeIndex(u32::MAX)), None);
    }

    #[test]
    fn shared_array_name_resolution_reuses_registered_base_type() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let array_base = types.factory().object(Vec::new());
        types.set_array_base_type(
            array_base,
            vec![TypeParamInfo {
                name: types.intern_string("T"),
                constraint: None,
                default: None,
                is_const: false,
                origin: tsz_solver::TypeParamOrigin::User,
            }],
        );

        let mut checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        checker.ctx.share_owner_symbol_type_results = true;

        assert_eq!(checker.resolve_lib_type_by_name("Array"), Some(array_base));
    }

    #[test]
    fn known_global_constructor_cache_rejects_non_constructable_type() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        let non_constructable = types.factory().object(Vec::new());

        assert!(
            !checker.cached_lib_type_is_usable("ErrorConstructor", Some(non_constructable)),
            "known global constructor cache entries must actually be constructable"
        );
        assert!(
            checker.cached_lib_type_is_usable("Error", Some(non_constructable)),
            "non-constructor lib cache entries are not filtered by constructability"
        );
        assert!(
            !checker.cached_lib_type_is_usable("Error", Some(TypeId(10_000))),
            "cached non-intrinsic TypeIds must belong to the current interner"
        );
    }
}
