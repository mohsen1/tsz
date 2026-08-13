//! Declaration-arena helpers for lib symbols and global augmentations.

use std::sync::Arc;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeArena, NodeIndex};

/// Canonical TypeScript lib load order (the `<reference lib=...>` dependency
/// linearization tsc uses). `es5` is the base and loads first; each later
/// standard-library file follows in reference order. Kept in the same order as
/// `tsz_core::config`'s `VALID_LIB_VALUES` — the checker cannot depend on
/// `tsz-core` (it is a lower crate), so the order is mirrored here; keep the two
/// in sync.
///
/// A multi-declaration (merged) lib interface such as `Map` is declared across
/// several files (`es2015.collection` + `es2015.iterable` +
/// `es2015.symbol.wellknown`). tsc merges those declarations in lib load order,
/// so the flat missing-property list lists `es2015.collection`'s members
/// (`clear`, `delete`, …) before `es2015.iterable`'s. `lib_contexts` are NOT
/// stored in load order (their order is resolution-incidental), so the merge
/// path imposes this order explicitly via [`lib_file_load_rank`] rather than
/// relying on context iteration order (issue #17344 follow-up).
const LIB_LOAD_ORDER: &[&str] = &[
    "es5",
    "es2015.core",
    "es2015.collection",
    "es2015.generator",
    "es2015.iterable",
    "es2015.promise",
    "es2015.proxy",
    "es2015.reflect",
    "es2015.symbol",
    "es2015.symbol.wellknown",
    "es2016.array.include",
    "es2016.intl",
    "es2017.arraybuffer",
    "es2017.date",
    "es2017.object",
    "es2017.sharedmemory",
    "es2017.string",
    "es2017.intl",
    "es2017.typedarrays",
    "es2018.asyncgenerator",
    "es2018.asynciterable",
    "es2018.intl",
    "es2018.promise",
    "es2018.regexp",
    "es2019.array",
    "es2019.object",
    "es2019.string",
    "es2019.symbol",
    "es2019.intl",
    "es2020.bigint",
    "es2020.date",
    "es2020.promise",
    "es2020.sharedmemory",
    "es2020.string",
    "es2020.symbol.wellknown",
    "es2020.intl",
    "es2020.number",
    "es2021.promise",
    "es2021.string",
    "es2021.weakref",
    "es2021.intl",
    "es2022.array",
    "es2022.error",
    "es2022.intl",
    "es2022.object",
    "es2022.string",
    "es2022.regexp",
    "es2023.array",
    "es2023.collection",
    "es2023.intl",
    "es2024.arraybuffer",
    "es2024.collection",
    "es2024.object",
    "es2024.promise",
    "es2024.regexp",
    "es2024.sharedmemory",
    "es2024.string",
    "es2025.collection",
    "es2025.float16",
    "es2025.intl",
    "es2025.iterator",
    "es2025.promise",
    "es2025.regexp",
    "esnext.array",
    "esnext.collection",
    "esnext.symbol",
    "esnext.asynciterable",
    "esnext.intl",
    "esnext.disposable",
    "esnext.bigint",
    "esnext.string",
    "esnext.promise",
    "esnext.weakref",
    "esnext.decorators",
    "esnext.object",
    "esnext.regexp",
    "esnext.iterator",
    "esnext.float16",
    "esnext.error",
    "esnext.sharedmemory",
    "esnext.date",
    "esnext.temporal",
    "esnext.typedarrays",
    "dom",
    "dom.iterable",
    "dom.asynciterable",
    "webworker",
    "webworker.importscripts",
    "webworker.iterable",
    "webworker.asynciterable",
    "scripthost",
    "decorators",
    "decorators.legacy",
];

/// Load-order rank of a lib `.d.ts` file (lower = loaded earlier). A
/// `lib.es2015.collection.d.ts` basename is normalized to `es2015.collection`
/// and looked up in [`LIB_LOAD_ORDER`]. Non-lib files and unknown lib names
/// rank last (`usize::MAX`), so a stable sort leaves them after all recognized
/// lib files without reordering them among themselves.
pub(crate) fn lib_file_load_rank(arena: &NodeArena) -> usize {
    let Some(source) = arena.source_files.first() else {
        return usize::MAX;
    };
    let base = std::path::Path::new(&source.file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(source.file_name.as_str());
    let lib_name = base
        .strip_prefix("lib.")
        .unwrap_or(base)
        .strip_suffix(".d.ts")
        .unwrap_or(base);
    LIB_LOAD_ORDER
        .iter()
        .position(|&known| known == lib_name)
        .unwrap_or(usize::MAX)
}

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
                    // only when ownership is provable: either the binder
                    // registered `fallback_arena` as this symbol's home arena,
                    // or the node is a named declaration that declares this
                    // symbol's name.
                    if fallback_arena_pair_is_trusted(binder, sym_id, decl_idx, fallback_arena) {
                        vec![(decl_idx, fallback_arena)]
                    } else {
                        tracing::debug!(
                            sym_id = ?sym_id,
                            symbol = %binder
                                .get_symbol(sym_id)
                                .map(|s| s.escaped_name.as_str())
                                .unwrap_or("<unknown>"),
                            decl_idx = ?decl_idx,
                            "rejecting foreign-arena lib-decl fallback pair"
                        );
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

/// Whether a `(decl_idx, fallback_arena)` pair produced by the last-resort
/// fallback in [`collect_lib_decls_with_arenas_in_contexts`] provably belongs
/// together.
///
/// Two independent proofs are accepted:
/// - the binder registered `fallback_arena` as the symbol's home arena in
///   `symbol_arenas` (covers declarations without a plain identifier name:
///   binding patterns, default exports, `export =`); or
/// - the node at `decl_idx` is a named declaration whose declared name equals
///   the symbol's escaped name.
///
/// Pairs that satisfy neither are foreign-arena `NodeIndex` collisions, not
/// declarations of this symbol, and lowering them manufactures wrong types
/// (issue #13255).
fn fallback_arena_pair_is_trusted(
    binder: &tsz_binder::BinderState,
    sym_id: tsz_binder::SymbolId,
    decl_idx: NodeIndex,
    arena: &NodeArena,
) -> bool {
    if arena.get(decl_idx).is_none() {
        return false;
    }
    if binder
        .symbol_arenas
        .get(&sym_id)
        .is_some_and(|home| std::ptr::eq(home.as_ref(), arena))
    {
        return true;
    }
    fallback_arena_node_declares_symbol(binder, sym_id, decl_idx, arena)
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
    // Plain identifier names first; string-literal names (e.g. ambient
    // `declare module "name"`) resolve through literal text.
    arena
        .get_identifier_text(name_idx)
        .or_else(|| arena.get_literal_text(name_idx))
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

/// Union same-named global declarations from every lib context into `decls`.
///
/// A checker whose current binder is a SINGLE-lib binder (the lib-baseline
/// diagnostics passes) resolves a global like `Array` to its local symbol,
/// whose declaration list covers only that one file. Lowering that partial
/// set and publishing it as the canonical lib body hands every consumer a
/// partial interface (issue #13255 family; a user `interface Error`
/// augmentation made it user-visible: `RegExpMatchArray`/`RegExpExecArray`
/// lost their `Array` heritage members). Extend with each lib context's
/// same-named declarations.
///
/// A lib file can be resident as SEVERAL independently parsed arenas (the
/// baseline pass re-parses its own copy), so pointer/storage identity
/// under-detects duplicates. Dedup by the declaring lib FILE identity: a
/// context whose source file name is already represented among the collected
/// pairs contributes nothing — its declarations are the same source text, and
/// re-adding them doubles every member of the merged interface (witness:
/// duplicated `Document`/`AudioBuffer` overload sets producing a false
/// TS2430 `Document`-vs-`NonElementParentNode` under a `createElement`
/// augmentation).
pub(crate) fn extend_decls_with_lib_context_globals<'a>(
    name: &str,
    lib_contexts: &'a [crate::context::LibContext],
    decls: &mut Vec<(NodeIndex, &'a NodeArena)>,
) {
    let arena_file_name = |arena: &NodeArena| -> Option<String> {
        arena
            .source_files
            .first()
            .map(|source| source.file_name.clone())
    };
    // Only COMPLETE a partial view; never fabricate one. An empty collected
    // set means the pairs were rejected (foreign-arena collisions) or owned
    // by another resolution mechanism — extending from name-matched contexts
    // here would hand this path a body it never owned (witness: `Document`'s
    // heritage-sensitive TS2430 firing when the empty-set union displaced
    // the heritage-folded resolution under a `createElement` augmentation).
    if decls.is_empty() {
        return;
    }
    let mut covered_files: Vec<String> = decls
        .iter()
        .filter_map(|(_, arena)| arena_file_name(arena))
        .collect();
    for lib_ctx in lib_contexts {
        let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) else {
            continue;
        };
        let Some(lib_symbol) = lib_ctx.binder.get_symbol(lib_sym_id) else {
            continue;
        };
        if lib_symbol.escaped_name != name {
            continue;
        }
        let Some(ctx_file) = arena_file_name(lib_ctx.arena.as_ref()) else {
            continue;
        };
        if covered_files.contains(&ctx_file) {
            continue;
        }
        let mut added = false;
        for &decl_idx in &lib_symbol.declarations {
            if lib_ctx.arena.get(decl_idx).is_none() {
                continue;
            }
            let already = decls.iter().any(|(idx, arena)| {
                *idx == decl_idx && arena.shares_node_storage_with(lib_ctx.arena.as_ref())
            });
            if !already {
                decls.push((decl_idx, lib_ctx.arena.as_ref()));
                added = true;
            }
        }
        if added {
            covered_files.push(ctx_file);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    fn parse_and_bind(file_name: &str, source: &str) -> (Arc<NodeArena>, BinderState) {
        let mut parser = ParserState::new(file_name.to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        (Arc::new(parser.get_arena().clone()), binder)
    }

    fn interface_decl(
        binder: &BinderState,
        name: &str,
    ) -> (tsz_binder::SymbolId, NodeIndex, usize) {
        let sym_id = binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("binder should expose {name}"));
        let symbol = binder.get_symbol(sym_id).expect("symbol should resolve");
        let decl_idx = *symbol
            .declarations
            .first()
            .expect("symbol should have a declaration");
        (sym_id, decl_idx, symbol.declarations.len())
    }

    /// A `(decl_idx, fallback_arena)` pair where the index addresses an
    /// unrelated node in a foreign arena must be dropped, not lowered.
    /// This was the issue #13255 poison: cross-file program symbols fell
    /// back to an arena that never declared them, and the colliding node
    /// produced a wrong type in the shared definition store.
    #[test]
    fn foreign_arena_collision_pair_is_rejected() {
        let (_, owner_binder) = parse_and_bind(
            "telemetry.ts",
            "export interface TelemetryFrame { gimbalAxis: string; }\n",
        );
        let (sym_id, decl_idx, decl_count) = interface_decl(&owner_binder, "TelemetryFrame");
        assert_eq!(decl_count, 1);

        // A foreign arena large enough that `decl_idx` addresses *some*
        // node — just never a declaration of `TelemetryFrame`.
        let (foreign_arena, _) = parse_and_bind(
            "unrelated.ts",
            "const pad0 = 0;\nconst pad1 = 1;\nconst pad2 = 2;\nconst pad3 = 3;\n\
             const pad4 = 4;\nconst pad5 = 5;\nconst pad6 = 6;\nconst pad7 = 7;\n",
        );
        assert!(
            foreign_arena.get(decl_idx).is_some(),
            "collision setup requires the index to resolve in the foreign arena"
        );

        let pairs = collect_lib_decls_with_arenas(
            &owner_binder,
            sym_id,
            &[decl_idx],
            foreign_arena.as_ref(),
            None,
        );
        assert!(
            pairs.is_empty(),
            "foreign-arena collision pair must be rejected, got {} pair(s)",
            pairs.len()
        );
    }

    /// The fallback stays usable when the arena really declares the symbol:
    /// the node at `decl_idx` is a named declaration with the symbol's name.
    #[test]
    fn owning_arena_fallback_pair_is_kept() {
        let (owner_arena, owner_binder) = parse_and_bind(
            "telemetry.ts",
            "export interface ApogeeWindow { ascentRate: number; }\n",
        );
        let (sym_id, decl_idx, _) = interface_decl(&owner_binder, "ApogeeWindow");

        let pairs = collect_lib_decls_with_arenas(
            &owner_binder,
            sym_id,
            &[decl_idx],
            owner_arena.as_ref(),
            None,
        );
        assert_eq!(
            pairs.len(),
            1,
            "fallback pair in the declaring arena must be kept"
        );
        assert_eq!(pairs[0].0, decl_idx);
        assert!(std::ptr::eq(pairs[0].1, owner_arena.as_ref()));
    }

    /// Declarations without a plain identifier name (destructuring binding
    /// patterns here) cannot pass the name check, but the binder-registered
    /// home arena proves ownership, so the pair must survive.
    #[test]
    fn registered_home_arena_pair_is_trusted_without_name_match() {
        let (owner_arena, mut owner_binder) = parse_and_bind(
            "boom.ts",
            "export const { boomArmSpan } = { boomArmSpan: 4 };\n",
        );
        let sym_id = owner_binder
            .file_locals
            .get("boomArmSpan")
            .expect("binder should expose boomArmSpan");
        let symbol = owner_binder
            .get_symbol(sym_id)
            .expect("symbol should resolve");
        let decl_idx = *symbol
            .declarations
            .first()
            .expect("symbol should have a declaration");
        assert!(
            !fallback_arena_node_declares_symbol(
                &owner_binder,
                sym_id,
                decl_idx,
                owner_arena.as_ref()
            ),
            "test setup requires a declaration the name check cannot prove"
        );

        let symbol_arenas = Arc::make_mut(&mut owner_binder.symbol_arenas);
        symbol_arenas.insert(sym_id, Arc::clone(&owner_arena));

        let pairs = collect_lib_decls_with_arenas(
            &owner_binder,
            sym_id,
            &[decl_idx],
            owner_arena.as_ref(),
            None,
        );
        assert_eq!(
            pairs.len(),
            1,
            "registered home-arena pair must be trusted even without a provable name"
        );
    }
}
