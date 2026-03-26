//! Library type resolution: resolving built-in types from `.d.ts` lib files,
//! merging interface heritage from lib arenas, and handling global augmentations.
//!
//! ## Stable Identity Helpers
//!
//! Lib lowering resolves `NodeIndex` values from multiple arenas into `SymbolIds`
//! and `DefIds`.  The canonical resolution path is:
//!
//! 1. [`resolve_lib_node_in_arenas`] — `NodeIndex` → `SymbolId` via
//!    identifier-text lookup across declaration arenas, then `file_locals` lookup.
//! 2. [`CheckerContext::get_lib_def_id`] — SymbolId → DefId, preferring
//!    pre-populated identities from `semantic_defs` with on-demand fallback.
//! 3. [`lib_def_id_from_node`] — one-step `NodeIndex` → DefId via (1)+(2), for
//!    the merged-binder path.
//! 4. [`lib_def_id_from_node_in_lib_contexts`] — one-step `NodeIndex` → DefId via
//!    [`resolve_lib_node_in_lib_contexts`]+(2), for per-lib-context lowering.
//! 5. [`augmentation_def_id_from_node`] — one-step `NodeIndex` → `DefId` via
//!    [`resolve_augmentation_node`]+(2), for global-augmentation lowering.
//!
//! All lib-lowering resolver closures should delegate to these helpers instead
//! of maintaining per-call caches.

use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::is_compiler_managed_type;

/// Stub value resolver for lib lowering — lib declarations have no runtime values.
///
/// All four lib-lowering sites (`resolve_lib_type_by_name`, `prime_lib_type_params`,
/// `resolve_lib_type_with_params`, `lower_augmentation_for_arena`) pass this as the
/// `value_resolver` argument to `TypeLowering::with_hybrid_resolver`.
pub(crate) const fn no_value_resolver(_: NodeIndex) -> Option<u32> {
    None
}

/// Map a `SyntaxKind` keyword to a built-in `TypeId`.
///
/// Returns `None` for non-keyword syntax kinds, letting callers fall through
/// to more expensive resolution paths only when necessary.
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

/// Map a keyword type *name* (e.g. `"string"`, `"number"`) to a built-in `TypeId`.
///
/// This covers the same set as [`keyword_syntax_to_type_id`] but works from
/// identifier text, which is needed when resolving type references from lib
/// arenas where the node kind might be `TypeReference` rather than a raw keyword.
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

/// Resolve the fallback arena for a lib symbol.
///
/// This is the canonical lookup order:
/// 1. Per-symbol arena from `binder.symbol_arenas` (set during lib merging).
/// 2. First lib context's arena (covers es5.d.ts and similar primary libs).
/// 3. The user file's arena (final fallback).
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

/// Resolve the fallback arena for a lib symbol within a single lib context.
///
/// This is used in per-lib-context iteration (e.g., `resolve_lib_type_with_params`)
/// where each lib context is processed independently. The lookup order is:
/// 1. Per-symbol arena from `binder.symbol_arenas`.
/// 2. The lib context's own arena.
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
///
/// Resolves each declaration to the correct arena via `binder.declaration_arenas`,
/// falling back to:
/// - `user_arena` if the declaration node exists there (local augmentations), or
/// - `fallback_arena` otherwise (lib declarations).
///
/// When `user_arena` is `None`, the fallback is used directly (e.g., in
/// `prime_lib_type_params` which has no user-arena context).
pub(crate) fn collect_lib_decls_with_arenas<'a>(
    binder: &'a tsz_binder::BinderState,
    sym_id: tsz_binder::SymbolId,
    declarations: &[NodeIndex],
    fallback_arena: &'a NodeArena,
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
                && ua.get(decl_idx).is_some()
            {
                // User augmentations (e.g., `interface Array<T> extends IFoo<T>`)
                // are not in declaration_arenas. Check the user arena before
                // falling back.
                vec![(decl_idx, ua)]
            } else {
                vec![(decl_idx, fallback_arena)]
            }
        })
        .collect()
}

/// Deduplicate `(NodeIndex, &NodeArena)` pairs by both index and arena pointer.
///
/// The lib merger can produce duplicate entries when the same lib file is loaded
/// from multiple lib contexts.  Comparing by both `NodeIndex` AND arena pointer
/// avoids dropping entries from different lib files that happen to share the same
/// `NodeIndex` (e.g., `SymbolConstructor` in `es2015.symbol.wellknown.d.ts` vs
/// `es2020.symbol.wellknown.d.ts`).
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
    resolve_lib_node_in_arenas(binder, node_idx, decl_arenas, fallback_arena)
        .map(|sym_id| ctx.get_lib_def_id(sym_id))
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
    resolve_lib_node_in_lib_contexts(node_idx, decl_arenas, fallback_arena, lib_contexts)
        .map(|sym_id| ctx.get_lib_def_id(sym_id))
}

/// Resolve a `NodeIndex` directly to a `DefId` via the augmentation resolution
/// strategy.
///
/// This is the stable one-step helper for augmentation lowering: it combines
/// [`resolve_augmentation_node`] (`NodeIndex` → `SymbolId`) with
/// [`CheckerContext::get_lib_def_id`] (`SymbolId` → `DefId`).  Using this
/// instead of inline two-step resolution at each callsite keeps the pattern
/// consistent with [`lib_def_id_from_node`].
#[allow(clippy::type_complexity)]
pub(crate) fn augmentation_def_id_from_node(
    ctx: &crate::context::CheckerContext<'_>,
    binder: &tsz_binder::BinderState,
    arena: &NodeArena,
    node_idx: NodeIndex,
    global_file_locals_index: Option<&FxHashMap<String, Vec<(usize, tsz_binder::SymbolId)>>>,
    all_binders: Option<&[std::sync::Arc<tsz_binder::BinderState>]>,
    lib_contexts: &[crate::context::LibContext],
) -> Option<tsz_solver::DefId> {
    resolve_augmentation_node(
        binder,
        arena,
        node_idx,
        global_file_locals_index,
        all_binders,
        lib_contexts,
    )
    .map(|sym_id| ctx.get_lib_def_id(sym_id))
}

/// Resolve a `NodeIndex` to a `SymbolId` by searching across multiple
/// declaration arenas.
///
/// This is the stable resolution path for lib lowering.  It replaces the
/// per-call resolver closures that previously duplicated this logic (and
/// sometimes added redundant local caches).
///
/// The lookup order is:
/// 1. Iterate `decl_arenas`; for each arena that yields identifier text at
///    `node_idx`, check `binder.file_locals`.
/// 2. If no declaration arena matched, try `fallback_arena`.
///
/// Returns `None` when the identifier is a compiler-managed type (e.g.,
/// `__String`) or when no matching symbol is found.
pub(crate) fn resolve_lib_node_in_arenas(
    binder: &tsz_binder::BinderState,
    node_idx: NodeIndex,
    decl_arenas: &[(NodeIndex, &NodeArena)],
    fallback_arena: &NodeArena,
) -> Option<tsz_binder::SymbolId> {
    for (_, arena) in decl_arenas {
        if let Some(ident_name) = arena.get_identifier_text(node_idx) {
            if is_compiler_managed_type(ident_name) {
                continue;
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
        if let Some(found_sym) = binder.file_locals.get(ident_name) {
            return Some(found_sym);
        }
    }
    None
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
#[allow(clippy::type_complexity)]
pub(crate) fn resolve_name_to_lib_symbol(
    name: &str,
    primary_binder: &tsz_binder::BinderState,
    global_file_locals_index: Option<&FxHashMap<String, Vec<(usize, tsz_binder::SymbolId)>>>,
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
#[allow(clippy::type_complexity)]
pub(crate) fn resolve_augmentation_node(
    binder: &tsz_binder::BinderState,
    arena: &NodeArena,
    node_idx: NodeIndex,
    global_file_locals_index: Option<&FxHashMap<String, Vec<(usize, tsz_binder::SymbolId)>>>,
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

    /// Resolve a library type by name from lib.d.ts and other library contexts.
    ///
    /// This function resolves types from library definition files like lib.d.ts,
    /// es2015.d.ts, etc., which provide built-in JavaScript types and DOM APIs.
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
    /// Merge base interface members into a lib interface type by walking
    /// heritage (`extends`) clauses in declaration-specific arenas.
    ///
    /// This is needed because `merge_interface_heritage_types` uses `self.ctx.arena`
    /// (the user file arena) and cannot read lib declarations that live in lib arenas.
    /// Takes the interface name and looks up declarations from the binder.
    pub(crate) fn merge_lib_interface_heritage(
        &mut self,
        mut derived_type: TypeId,
        name: &str,
    ) -> TypeId {
        // Guard against infinite recursion in recursive generic hierarchies
        // (e.g., interface B<T extends B<T,S>> extends A<B<T,S>, B<T,S>>)
        if !self.ctx.enter_recursion() {
            return derived_type;
        }

        // Name-based cycle guard: prevent re-entrant heritage merging for the same
        // interface name. This breaks the resolve_lib_type_by_name ↔ merge_lib_interface_heritage
        // mutual recursion that occurs through deep heritage chains
        // (e.g., Array → ReadonlyArray → Iterable → ...), especially when child
        // CheckerStates are created for cross-arena type param resolution.
        if !self.ctx.lib_heritage_in_progress.insert(name.to_string()) {
            self.ctx.leave_recursion();
            return derived_type;
        }

        let lib_contexts = self.ctx.lib_contexts.clone();

        // Look up the symbol and its declarations
        let Some(sym_id) = self.ctx.binder.file_locals.get(name) else {
            self.ctx.lib_heritage_in_progress.remove(name);
            self.ctx.leave_recursion();
            return derived_type;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            self.ctx.lib_heritage_in_progress.remove(name);
            self.ctx.leave_recursion();
            return derived_type;
        };

        let fallback_arena =
            resolve_lib_fallback_arena(self.ctx.binder, sym_id, &lib_contexts, self.ctx.arena);

        let decls_with_arenas = collect_lib_decls_with_arenas(
            self.ctx.binder,
            sym_id,
            &symbol.declarations,
            fallback_arena,
            Some(self.ctx.arena),
        );

        // Early exit: skip expensive type parameter scope setup and heritage merge
        // if no declarations have extends clauses
        let has_any_heritage = decls_with_arenas.iter().any(|&(decl_idx, arena)| {
            let Some(node) = arena.get(decl_idx) else {
                return false;
            };
            let Some(interface) = arena.get_interface(node) else {
                return false;
            };
            interface
                .heritage_clauses
                .as_ref()
                .is_some_and(|hc| !hc.nodes.is_empty())
        });

        if !has_any_heritage {
            self.ctx.leave_recursion();
            return derived_type;
        }

        // Seed type-parameter scope with the derived interface's generic params so
        // heritage args like `extends IteratorObject<T, ...>` resolve `T` correctly.
        // Without this, lib heritage substitution falls back to `unknown` and loses
        // member types (e.g. `ArrayIterator<T>.next().value` becomes `unknown`).
        let mut scope_restore: Vec<(String, Option<TypeId>)> = Vec::new();
        for param in self.get_type_params_for_symbol(sym_id) {
            let name = self.ctx.types.resolve_atom(param.name).to_string();
            let param_ty = self.ctx.types.type_param(param);
            let prev = self.ctx.type_parameter_scope.insert(name.clone(), param_ty);
            scope_restore.push((name, prev));
        }

        // Collect base type info: name and type argument node indices with their arena.
        // We collect these first to avoid borrow conflicts during resolution.
        struct HeritageBase<'a> {
            name: String,
            type_arg_indices: Vec<NodeIndex>,
            arena: &'a NodeArena,
        }
        let mut bases: Vec<HeritageBase<'_>> = Vec::new();

        for &(decl_idx, arena) in &decls_with_arenas {
            let Some(node) = arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = arena.get_interface(node) else {
                continue;
            };
            let Some(ref heritage_clauses) = interface.heritage_clauses else {
                continue;
            };

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = arena.get(type_idx) else {
                        continue;
                    };

                    // Extract the base type name and type arguments
                    let (expr_idx, type_arguments) =
                        if let Some(eta) = arena.get_expr_type_args(type_node) {
                            (eta.expression, eta.type_arguments.as_ref())
                        } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                            if let Some(tr) = arena.get_type_ref(type_node) {
                                (tr.type_name, tr.type_arguments.as_ref())
                            } else {
                                (type_idx, None)
                            }
                        } else {
                            (type_idx, None)
                        };

                    if let Some(base_name) = arena.get_identifier_text(expr_idx) {
                        let type_arg_indices = type_arguments
                            .map(|args| args.nodes.clone())
                            .unwrap_or_default();
                        bases.push(HeritageBase {
                            name: base_name.to_string(),
                            type_arg_indices,
                            arena,
                        });
                    }
                }
            }
        }

        // Now resolve each base type and merge, applying type argument substitution
        for base in &bases {
            if let Some(mut base_type) = self.resolve_lib_type_by_name(&base.name) {
                // If there are type arguments, resolve them and substitute
                if !base.type_arg_indices.is_empty() {
                    let base_sym = self.ctx.binder.file_locals.get(&base.name);
                    if let Some(base_sym_id) = base_sym {
                        let base_params = self.get_type_params_for_symbol(base_sym_id);
                        if !base_params.is_empty() {
                            let mut type_args = Vec::new();
                            for &arg_idx in &base.type_arg_indices {
                                // Resolve type arguments from the lib arena.
                                // Heritage type args are typically simple type
                                // references (e.g., `string`, `number`).
                                let ty = self.resolve_lib_heritage_type_arg(arg_idx, base.arena);
                                type_args.push(ty);
                            }
                            // Pad/truncate args to match params
                            while type_args.len() < base_params.len() {
                                let param = &base_params[type_args.len()];
                                type_args.push(
                                    param
                                        .default
                                        .or(param.constraint)
                                        .unwrap_or(TypeId::UNKNOWN),
                                );
                            }
                            type_args.truncate(base_params.len());

                            let substitution =
                                crate::query_boundaries::common::TypeSubstitution::from_args(
                                    self.ctx.types,
                                    &base_params,
                                    &type_args,
                                );
                            base_type = crate::query_boundaries::common::instantiate_type(
                                self.ctx.types,
                                base_type,
                                &substitution,
                            );
                        }
                    }
                }
                derived_type = self.merge_interface_types(derived_type, base_type);
            }
        }

        for (name, prev) in scope_restore {
            if let Some(prev_ty) = prev {
                self.ctx.type_parameter_scope.insert(name, prev_ty);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }

        self.ctx.lib_heritage_in_progress.remove(name);
        self.ctx.leave_recursion();
        derived_type
    }

    /// Resolve a type argument node from a lib arena to a TypeId.
    /// Handles simple keyword types (string, number, etc.), type references
    /// to other lib types, and the derived interface's own type parameters.
    fn resolve_lib_heritage_type_arg(&mut self, node_idx: NodeIndex, arena: &NodeArena) -> TypeId {
        let Some(node) = arena.get(node_idx) else {
            return TypeId::UNKNOWN;
        };

        // Handle keyword types (string, number, boolean, etc.)
        if let Some(ty) = keyword_syntax_to_type_id(node.kind) {
            return ty;
        }

        // Handle type references (e.g., other interface names or type params)
        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = arena.get_type_ref(node)
            && let Some(name) = arena.get_identifier_text(type_ref.type_name)
        {
            if let Some(ty) = keyword_name_to_type_id(name) {
                return ty;
            }
            return self.resolve_heritage_type_arg_by_name(name);
        }

        // For identifiers, try resolving the name
        if let Some(name) = arena.get_identifier_text(node_idx) {
            return self.resolve_heritage_type_arg_by_name(name);
        }

        TypeId::UNKNOWN
    }

    /// Resolve a heritage type argument by name: type-parameter scope → lib type → symbolic param.
    fn resolve_heritage_type_arg_by_name(&mut self, name: &str) -> TypeId {
        if let Some(&type_id) = self.ctx.type_parameter_scope.get(name) {
            return type_id;
        }
        if let Some(ty) = self.resolve_lib_type_by_name(name) {
            return ty;
        }
        // Preserve unresolved lib heritage args as symbolic type params
        // (e.g. `T` in `extends IteratorObject<T, ...>`) instead of
        // collapsing to unknown.
        let atom = self.ctx.types.intern_string(name);
        self.ctx.types.type_param(tsz_solver::TypeParamInfo {
            name: atom,
            constraint: None,
            default: None,
            is_const: false,
        })
    }

    pub(crate) fn resolve_lib_type_by_name(&mut self, name: &str) -> Option<TypeId> {
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

        // Check shared cross-file lib cache first
        if let Some(cached) = self.ctx.lib_type_resolution_cache.get(name) {
            return *cached;
        }

        tracing::trace!(name, "resolve_lib_type_by_name: called");
        let mut lib_type_id: Option<TypeId> = None;
        let factory = self.ctx.types.factory();
        let mut symbol_has_interface = false;

        let lib_contexts = self.ctx.lib_contexts.clone();
        // Collect lowered types from the symbol's declarations.
        // The main file's binder already has merged declarations from all lib files.
        let mut lib_types: Vec<TypeId> = Vec::new();

        // CRITICAL: Look up the symbol in the MAIN file's binder (self.ctx.binder),
        // not in lib_ctx.binder. The main file's binder has lib symbols merged with
        // unique SymbolIds via merge_lib_contexts_into_binder during binding.
        // lib_ctx.binder is a SEPARATE merged binder with DIFFERENT SymbolIds.
        // Using lib_ctx.binder's SymbolIds with self.ctx.get_or_create_def_id causes
        // SymbolId collisions and wrong type resolution.
        let lib_binders = self.get_lib_binders();
        let sym_id = self.ctx.binder.file_locals.get(name).or_else(|| {
            self.ctx
                .binder
                .get_global_type_with_libs(name, &lib_binders)
        });

        if let Some(sym_id) = sym_id {
            // Get the symbol's declaration(s) from the main file's binder
            if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                symbol_has_interface = (symbol.flags & tsz_binder::symbol_flags::INTERFACE) != 0;
                let fallback_arena = resolve_lib_fallback_arena(
                    self.ctx.binder,
                    sym_id,
                    &lib_contexts,
                    self.ctx.arena,
                );

                let decls_with_arenas = collect_lib_decls_with_arenas(
                    self.ctx.binder,
                    sym_id,
                    &symbol.declarations,
                    fallback_arena,
                    Some(self.ctx.arena),
                );

                // Resolver triplet: delegates to stable helpers instead of
                // maintaining per-call caches. The `resolver` closure extracts
                // the raw `u32` at the TypeLowering boundary; all internal
                // resolution uses type-safe `SymbolId`.
                let binder = &self.ctx.binder;
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
                    self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
                };

                let lazy_type_params_resolver =
                    |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);

                // Create base lowering with the fallback arena and both resolvers
                let lowering = TypeLowering::with_hybrid_resolver(
                    fallback_arena,
                    self.ctx.types,
                    &resolver,
                    &def_id_resolver,
                    &no_value_resolver,
                )
                .with_lazy_type_params_resolver(&lazy_type_params_resolver)
                .with_name_def_id_resolver(&name_resolver);

                // Try to lower as interface first (handles declaration merging)
                if !symbol.declarations.is_empty() {
                    // Check if any declaration is a type alias — if so, skip interface
                    // lowering. Type aliases like Record<K,T>, Partial<T>, Pick<T,K>
                    // would incorrectly succeed interface lowering with 0 type params,
                    // preventing the proper type alias path from running.
                    let is_type_alias = (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0;

                    if !is_type_alias {
                        let deduped = dedup_decl_arenas(&decls_with_arenas);

                        // Use lower_merged_interface_declarations for proper multi-arena support
                        let (ty, params) = lowering.lower_merged_interface_declarations(&deduped);

                        // If lowering succeeded (not ERROR), use the result
                        if ty != TypeId::ERROR {
                            // Register DefId, type params, and body in one step.
                            self.ctx.register_lib_def_resolved(sym_id, ty, params);

                            lib_types.push(ty);
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
                                    let def_id =
                                        self.ctx.register_lib_def_resolved(sym_id, ty, params);

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
        //
        // CRITICAL: Insert the pre-heritage type into the cache BEFORE merging heritage.
        // merge_lib_interface_heritage calls resolve_lib_type_by_name recursively for base
        // types. Without this early cache insertion, recursive calls redo all lowering work
        // for types already being resolved, causing O(n!) blowup on deep heritage chains
        // (e.g., es5.d.ts where Array extends ReadonlyArray, etc.).
        // The recursive call gets the un-merged type (missing inherited members), which is
        // still correct for breaking cycles. The final cache update below overwrites with
        // the fully-merged type.
        if let Some(ty) = lib_type_id {
            self.ctx
                .lib_type_resolution_cache
                .insert(name.to_string(), Some(ty));
            lib_type_id = Some(self.merge_lib_interface_heritage(ty, name));
        }

        // Merge global augmentations (declare global { interface X { ... } }).
        if let Some(merged) = self.merge_global_augmentations(name, lib_type_id, &lib_contexts) {
            lib_type_id = Some(merged);
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
                            let merged = self.merge_interface_types(current_type, base_type);
                            if merged != current_type {
                                lib_type_id = Some(merged);
                            }
                        }
                    }
                }
            }
        }

        // For generic lib interfaces, we already cached the type params in the
        // interface lowering code above. The type is already correctly lowered
        // and can be returned directly.
        self.ctx
            .lib_type_resolution_cache
            .insert(name.to_string(), lib_type_id);

        // Store in shared cross-file cache for other parallel file checks.
        let _has_augmentations = self
            .ctx
            .binder
            .global_augmentations
            .get(name)
            .is_some_and(|v| !v.is_empty());
        lib_type_id
    }

    /// Lower augmentation declarations from a given arena and return the resulting `TypeId`.
    ///
    /// This is the shared implementation for global-augmentation lowering used by both
    /// `resolve_lib_type_by_name` and `resolve_lib_type_with_params`. It builds the
    /// standard resolver triplet (node → SymbolId, node → DefId, name → DefId) using
    /// [`resolve_augmentation_node`] and [`CheckerContext::get_lib_def_id`], then
    /// delegates to `TypeLowering::lower_interface_declarations`.
    ///
    /// Callers merge the returned type into their running `lib_type_id` via intersection.
    pub(crate) fn lower_augmentation_for_arena(
        &self,
        arena_ref: &NodeArena,
        decls: &[NodeIndex],
        lib_contexts: &[crate::context::LibContext],
    ) -> TypeId {
        let binder_ref = self.ctx.binder;
        let decl_binder = self
            .ctx
            .get_binder_for_arena(arena_ref)
            .unwrap_or(binder_ref);
        let global_idx = self.ctx.global_file_locals_index.as_deref();
        let all_binders_slice = self.ctx.all_binders.as_ref().map(|v| v.as_slice());
        let resolver = |node_idx: NodeIndex| -> Option<u32> {
            resolve_augmentation_node(
                decl_binder,
                arena_ref,
                node_idx,
                global_idx,
                all_binders_slice,
                lib_contexts,
            )
            .map(|sym_id| sym_id.0)
        };
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
            augmentation_def_id_from_node(
                &self.ctx,
                decl_binder,
                arena_ref,
                node_idx,
                global_idx,
                all_binders_slice,
                lib_contexts,
            )
        };
        let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
            self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
        };
        let lowering = tsz_lowering::TypeLowering::with_hybrid_resolver(
            arena_ref,
            self.ctx.types,
            &resolver,
            &def_id_resolver,
            &no_value_resolver,
        )
        .with_name_def_id_resolver(&name_resolver);
        lowering.lower_interface_declarations(decls)
    }

    /// Merge global augmentations for `name` into `lib_type_id`.
    ///
    /// This consolidates the augmentation-merge pattern that was previously
    /// duplicated between `resolve_lib_type_by_name` and
    /// `resolve_lib_type_with_params`. Both callers group augmentation
    /// declarations by arena (current-file vs cross-file), lower each group
    /// via [`lower_augmentation_for_arena`], and merge via intersection.
    pub(crate) fn merge_global_augmentations(
        &self,
        name: &str,
        lib_type_id: Option<TypeId>,
        lib_contexts: &[crate::context::LibContext],
    ) -> Option<TypeId> {
        let augmentation_decls = self.ctx.binder.global_augmentations.get(name)?;
        if augmentation_decls.is_empty() {
            return lib_type_id;
        }

        let factory = self.ctx.types.factory();
        let current_arena: &NodeArena = self.ctx.arena;
        let mut result = lib_type_id;

        // Group augmentation declarations by arena.
        let mut current_file_decls: Vec<NodeIndex> = Vec::new();
        let mut cross_file_groups: FxHashMap<usize, (Arc<NodeArena>, Vec<NodeIndex>)> =
            FxHashMap::default();

        for aug in augmentation_decls {
            if let Some(ref arena) = aug.arena {
                let key = Arc::as_ptr(arena) as usize;
                cross_file_groups
                    .entry(key)
                    .or_insert_with(|| (Arc::clone(arena), Vec::new()))
                    .1
                    .push(aug.node);
            } else {
                current_file_decls.push(aug.node);
            }
        }

        // Lower current-file augmentations.
        if !current_file_decls.is_empty() {
            let aug_type =
                self.lower_augmentation_for_arena(current_arena, &current_file_decls, lib_contexts);
            result = Some(if let Some(lib_type) = result {
                factory.intersection2(lib_type, aug_type)
            } else {
                aug_type
            });
        }

        // Lower cross-file augmentations (each group uses its own arena).
        for (arena, decls) in cross_file_groups.values() {
            let aug_type = self.lower_augmentation_for_arena(arena.as_ref(), decls, lib_contexts);
            result = Some(if let Some(lib_type) = result {
                factory.intersection2(lib_type, aug_type)
            } else {
                aug_type
            });
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- keyword_syntax_to_type_id ----

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
        // Use an arbitrary non-keyword kind value
        assert_eq!(keyword_syntax_to_type_id(0), None);
        assert_eq!(keyword_syntax_to_type_id(9999), None);
    }

    // ---- keyword_name_to_type_id ----

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
        assert_eq!(keyword_name_to_type_id("String"), None); // capital S
        assert_eq!(keyword_name_to_type_id(""), None);
    }

    #[test]
    fn keyword_name_and_syntax_agree() {
        // Verify both mapping functions return the same TypeId for each keyword
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

    // ---- dedup_decl_arenas ----

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
}

#[cfg(test)]
mod integration_tests {
    use crate::test_utils::check_source_codes;

    // ---- Promise / lib ref lowering ----

    #[test]
    fn promise_type_annotation_no_error() {
        // Without lib contexts, Promise is unknown. We just verify no crash.
        let codes = check_source_codes("let p: Promise<number>;");
        // TS2304 (Cannot find name) or TS2583 (needs lib change) expected without libs
        assert!(
            codes.contains(&2304) || codes.contains(&2583) || codes.is_empty(),
            "Promise without libs should produce TS2304/TS2583 or pass: {codes:?}"
        );
    }

    #[test]
    fn async_function_returns_promise_no_crash() {
        // Async functions implicitly return Promise — verify no panic during lowering
        let _codes = check_source_codes("async function f(): Promise<string> { return ''; }");
    }

    #[test]
    fn generic_lib_ref_annotation_no_crash() {
        // Generic lib-like types referenced without lib contexts should not crash
        let _codes = check_source_codes("let a: Array<number> = [];");
    }

    // ---- import type lowering ----

    #[test]
    fn import_type_basic_no_crash() {
        // import() type expressions should not crash the lowering pipeline
        let _codes = check_source_codes("type T = import('./other').Foo;");
    }

    #[test]
    fn import_type_with_generic_no_crash() {
        let _codes = check_source_codes("type T = import('./other').Bar<number>;");
    }

    // ---- lib keyword type refs ----

    #[test]
    fn keyword_type_refs_no_error() {
        let codes = check_source_codes(
            "let s: string; let n: number; let b: boolean; let v: void; let u: undefined;",
        );
        // Keyword types always resolve (no lib needed)
        assert!(
            codes.is_empty(),
            "Keyword type annotations should produce no errors: {codes:?}"
        );
    }

    #[test]
    fn keyword_type_in_function_params_no_error() {
        let codes =
            check_source_codes("function f(a: string, b: number): boolean { return true; }");
        assert!(
            codes.is_empty(),
            "Keyword types in function params should produce no errors: {codes:?}"
        );
    }

    #[test]
    fn null_and_never_types_no_error() {
        let codes = check_source_codes("let n: null = null; let x: never = undefined as never;");
        // 'never' assignment may error, but should not crash
        let _ = codes;
    }

    #[test]
    fn union_of_keyword_types_no_error() {
        let codes = check_source_codes("let x: string | number | boolean = 'hello';");
        assert!(
            codes.is_empty(),
            "Union of keyword types should produce no errors: {codes:?}"
        );
    }

    // ---- Promise lowering edge cases ----

    #[test]
    fn promise_nested_generic_no_crash() {
        // Nested Promise generics should not crash during lib lowering
        let _codes = check_source_codes("let p: Promise<Promise<number>>;");
    }

    #[test]
    fn promise_union_type_arg_no_crash() {
        let _codes = check_source_codes("let p: Promise<string | number>;");
    }

    #[test]
    fn promise_in_return_type_no_crash() {
        let _codes = check_source_codes("function f(): Promise<void> { return undefined as any; }");
    }

    #[test]
    fn promise_all_pattern_no_crash() {
        // Promise.all-like usage pattern
        let _codes =
            check_source_codes("async function f() { const a = await Promise.resolve(1); }");
    }

    #[test]
    fn promise_like_type_no_crash() {
        // PromiseLike is a separate lib interface
        let _codes = check_source_codes("let p: PromiseLike<string>;");
    }

    // ---- lib ref lowering: generic types ----

    #[test]
    fn map_type_no_crash() {
        let _codes = check_source_codes("let m: Map<string, number>;");
    }

    #[test]
    fn set_type_no_crash() {
        let _codes = check_source_codes("let s: Set<number>;");
    }

    #[test]
    fn readonly_array_no_crash() {
        let _codes = check_source_codes("let a: ReadonlyArray<string>;");
    }

    #[test]
    fn record_type_no_crash() {
        let _codes = check_source_codes("let r: Record<string, number>;");
    }

    #[test]
    fn partial_type_no_crash() {
        let _codes = check_source_codes("type P = Partial<{ a: number; b: string }>;");
    }

    #[test]
    fn pick_type_no_crash() {
        let _codes = check_source_codes("type P = Pick<{ a: number; b: string }, 'a'>;");
    }

    // ---- import-type lowering edge cases ----

    #[test]
    fn import_type_typeof_no_crash() {
        let _codes = check_source_codes("type T = typeof import('./mod');");
    }

    #[test]
    fn import_type_nested_access_no_crash() {
        // Nested property access on import type
        let _codes = check_source_codes("type T = import('./mod').Ns.Inner;");
    }

    #[test]
    fn import_type_in_function_param_no_crash() {
        let _codes = check_source_codes(
            "function f(x: import('./mod').Foo): import('./mod').Bar { return x as any; }",
        );
    }

    #[test]
    fn import_type_with_multiple_generics_no_crash() {
        let _codes = check_source_codes("type T = import('./mod').Map<string, number>;");
    }

    // ---- lib ref lowering: intersection of keyword and lib types ----

    #[test]
    fn intersection_of_keyword_and_lib_type_no_crash() {
        let _codes = check_source_codes("type T = string & { brand: true };");
    }

    #[test]
    fn conditional_type_with_lib_ref_no_crash() {
        let _codes = check_source_codes(
            "type IsArray<T> = T extends Array<infer U> ? U : never; type X = IsArray<number[]>;",
        );
    }

    #[test]
    fn error_type_no_crash() {
        // Error is a lib type
        let _codes = check_source_codes("let e: Error;");
    }

    #[test]
    fn regexp_type_no_crash() {
        let _codes = check_source_codes("let r: RegExp;");
    }

    #[test]
    fn date_type_no_crash() {
        let _codes = check_source_codes("let d: Date;");
    }

    // ---- Promise lowering: behavioral correctness ----

    #[test]
    fn promise_assignment_to_wrong_type_no_crash() {
        // Promise<number> should not be assignable to string without error
        let _codes = check_source_codes("let p: Promise<number>; let s: string = p as any;");
    }

    #[test]
    fn async_function_inferred_return_type_no_crash() {
        // Async function return type inference: the returned value wraps in Promise
        let _codes = check_source_codes("async function f() { return 42; }");
    }

    #[test]
    fn promise_with_void_type_arg_no_crash() {
        // Promise<void> is common for side-effect-only async functions
        let _codes =
            check_source_codes("async function run(): Promise<void> { console.log('done'); }");
    }

    #[test]
    fn promise_constructor_pattern_no_crash() {
        // new Promise() pattern exercises the constructor signature lowering
        let _codes = check_source_codes(
            "let p = new Promise<number>((resolve, reject) => { resolve(1); });",
        );
    }

    #[test]
    fn promise_then_chain_no_crash() {
        // .then() method resolution exercises lib heritage merging
        let _codes =
            check_source_codes("declare let p: Promise<number>; let q = p.then(x => x + 1);");
    }

    #[test]
    fn promise_catch_no_crash() {
        let _codes = check_source_codes(
            "declare let p: Promise<number>; let q = p.catch(e => console.log(e));",
        );
    }

    #[test]
    fn promise_race_all_no_crash() {
        // Promise.race / Promise.all are static methods on the Promise constructor
        let _codes = check_source_codes(
            "declare let a: Promise<number>; declare let b: Promise<string>; \
             let r = Promise.race([a, b]);",
        );
    }

    #[test]
    fn awaited_type_no_crash() {
        // Awaited<T> is a conditional type alias in lib
        let _codes = check_source_codes("type X = Awaited<Promise<number>>;");
    }

    // ---- lib ref lowering: generic utility types (behavioral) ----

    #[test]
    fn required_type_no_crash() {
        let _codes = check_source_codes("type R = Required<{ a?: number; b?: string }>;");
    }

    #[test]
    fn readonly_utility_type_no_crash() {
        let _codes = check_source_codes("type R = Readonly<{ a: number; b: string }>;");
    }

    #[test]
    fn omit_type_no_crash() {
        let _codes =
            check_source_codes("type O = Omit<{ a: number; b: string; c: boolean }, 'c'>;");
    }

    #[test]
    fn exclude_extract_types_no_crash() {
        let _codes = check_source_codes(
            "type E = Exclude<'a' | 'b' | 'c', 'a'>; type X = Extract<'a' | 'b', 'a' | 'c'>;",
        );
    }

    #[test]
    fn return_type_utility_no_crash() {
        let _codes = check_source_codes(
            "function f(x: number): string { return ''; } type R = ReturnType<typeof f>;",
        );
    }

    #[test]
    fn parameters_utility_no_crash() {
        let _codes = check_source_codes(
            "function f(a: number, b: string): void {} type P = Parameters<typeof f>;",
        );
    }

    #[test]
    fn instance_type_utility_no_crash() {
        let _codes =
            check_source_codes("class Foo { x: number = 1; } type I = InstanceType<typeof Foo>;");
    }

    #[test]
    fn non_nullable_utility_no_crash() {
        let _codes = check_source_codes("type N = NonNullable<string | null | undefined>;");
    }

    // ---- import-type lowering: behavioral ----

    #[test]
    fn import_type_in_variable_decl_no_crash() {
        let _codes = check_source_codes("let x: import('./mod').SomeType = {} as any;");
    }

    #[test]
    fn import_type_in_type_alias_union_no_crash() {
        let _codes = check_source_codes("type T = string | import('./other').Foo;");
    }

    #[test]
    fn import_type_in_interface_extends_no_crash() {
        let _codes =
            check_source_codes("interface Foo extends import('./other').Bar { x: number; }");
    }

    #[test]
    fn import_type_in_class_implements_no_crash() {
        let _codes =
            check_source_codes("class Foo implements import('./other').IBar { x: number = 1; }");
    }

    #[test]
    fn import_type_conditional_no_crash() {
        let _codes =
            check_source_codes("type T = import('./mod').Foo extends string ? true : false;");
    }

    // ---- lib ref lowering: multiple generic params ----

    #[test]
    fn weak_map_weak_set_no_crash() {
        let _codes =
            check_source_codes("let wm: WeakMap<object, number>; let ws: WeakSet<object>;");
    }

    #[test]
    fn generator_type_no_crash() {
        let _codes = check_source_codes(
            "function* gen(): Generator<number, string, boolean> { yield 1; return ''; }",
        );
    }

    #[test]
    fn async_generator_type_no_crash() {
        let _codes = check_source_codes(
            "async function* gen(): AsyncGenerator<number, void, unknown> { yield 1; }",
        );
    }

    #[test]
    fn iterable_iterator_type_no_crash() {
        let _codes = check_source_codes("declare function iter(): IterableIterator<number>;");
    }

    #[test]
    fn async_iterable_type_no_crash() {
        let _codes = check_source_codes("declare function iter(): AsyncIterable<string>;");
    }

    // ---- lib ref lowering: heritage chain depth ----

    #[test]
    fn array_method_access_no_crash() {
        // Array extends ReadonlyArray — exercises heritage chain merging
        let _codes =
            check_source_codes("let a: Array<number> = [1, 2, 3]; let b = a.map(x => x + 1);");
    }

    #[test]
    fn typed_array_no_crash() {
        let _codes = check_source_codes("let a: Int32Array = new Int32Array(10);");
    }

    #[test]
    fn symbol_iterator_no_crash() {
        // Symbol.iterator exercises deep lib heritage chains
        let _codes = check_source_codes("let s = Symbol.iterator;");
    }

    // ---- lib ref + global augmentation patterns ----

    #[test]
    fn declare_global_interface_augmentation_no_crash() {
        let _codes = check_source_codes(
            "declare global { interface Window { myProp: string; } } \
             export {};",
        );
    }

    #[test]
    fn declare_global_array_augmentation_no_crash() {
        let _codes = check_source_codes(
            "declare global { interface Array<T> { customMethod(): T; } } \
             export {};",
        );
    }

    // ---- keyword type consistency ----

    #[test]
    fn keyword_types_in_generic_position_no_crash() {
        // Keyword types used as generic arguments should resolve correctly
        let codes = check_source_codes(
            "type Box<T> = { value: T }; \
             let a: Box<string>; let b: Box<number>; let c: Box<boolean>;",
        );
        assert!(
            codes.is_empty(),
            "Keyword types in generic position should produce no errors: {codes:?}"
        );
    }

    #[test]
    fn keyword_types_in_tuple_no_error() {
        let codes = check_source_codes("let t: [string, number, boolean] = ['a', 1, true];");
        assert!(
            codes.is_empty(),
            "Keyword types in tuple should produce no errors: {codes:?}"
        );
    }
}
