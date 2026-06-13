//! Resolver-boundary lookup of value-space import targets.
//!
//! Property-access recovery paths (for example imported `arrayToEnum` member
//! recovery) need the concrete *value* symbol a named import ultimately binds
//! to, together with the file that declares it and whether the import edge is
//! type-only. Re-deriving that locally from checker AST scans and binder
//! name scans drifts on renamed imports, named/wildcard re-exports, and
//! `export type` edges. This helper routes the lookup through the shared
//! binder-backed alias resolver so the recovered fact stays consistent with the
//! rest of the program's module graph.

use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{SymbolId, symbol_flags};

/// A value-space import target resolved through the binder's module graph.
///
/// Returned by [`CheckerState::resolve_value_import_target`] so checker-side
/// recovery paths consume a structured module-graph fact instead of scanning
/// import declarations and target binders with checker-local AST/name
/// heuristics.
pub(crate) struct ResolvedValueImport {
    /// The concrete symbol the import ultimately binds to, after following
    /// renamed imports and named/wildcard re-export chains across files.
    pub symbol: SymbolId,
    /// Index of the file whose binder owns [`Self::symbol`].
    pub file_idx: usize,
    /// Whether the resolved edge passed through a type-only import/export
    /// (`import type`, `import { type X }`, `export type { }`, or
    /// `export type *`). Such targets are erased at runtime and must not
    /// contribute a value.
    pub type_only: bool,
}

impl<'a> CheckerState<'a> {
    /// Resolve a value-space import alias to its concrete declaration through the
    /// shared binder-backed alias resolver.
    ///
    /// `alias_sym_id` is the local alias symbol bound by an `import { name }` or
    /// `import { orig as name }` clause. The returned [`ResolvedValueImport`]
    /// carries the resolved symbol, its owning file index, and whether the edge
    /// is type-only. Returns `None` when the symbol is not an import alias, is a
    /// namespace import (`import * as ns`), or cannot be resolved to a concrete
    /// declaration.
    ///
    /// Resolution is delegated to [`CheckerState::resolve_alias_symbol`] so
    /// renamed imports, named re-exports (`export { name } from`), and wildcard
    /// re-exports (`export * from`) are followed file-by-file consistently with
    /// the rest of the program rather than re-derived from local AST scans.
    pub(crate) fn resolve_value_import_target(
        &self,
        alias_sym_id: SymbolId,
    ) -> Option<ResolvedValueImport> {
        // Borrow the alias module-graph facts in place: every helper below takes
        // `&self`, so the borrow can be held across them without cloning the
        // specifier/name strings.
        let alias = self.get_cross_file_symbol(alias_sym_id)?;
        if alias.flags & symbol_flags::ALIAS == 0 {
            return None;
        }
        let module_specifier = alias.import_module()?;
        // Namespace / `export=` imports (`import * as ns`) bind the module
        // object, not a single named value export, so there is no value-space
        // target to recover here.
        let import_name = alias.import_name()?;
        if import_name == "*" {
            return None;
        }

        // Resolve first, before the cross-binder type-only walk: the recursive
        // alias resolution gates the (more expensive) `export type` chain check,
        // so property accesses on imports that don't resolve to a distinct value
        // never pay for it.
        let mut visited = AliasCycleTracker::new();
        let resolved = self.resolve_alias_symbol(alias_sym_id, &mut visited)?;
        // `resolve_alias_symbol` echoes the input back when it is not an
        // import-backed alias or the chain could not be followed; treat that as
        // "no value-space target". This also avoids re-pinning the alias's own
        // (per-binder) `SymbolId` to a foreign file below.
        if resolved == alias_sym_id {
            return None;
        }

        // `resolve_alias_symbol` already pins the resolved target to its owning
        // file; read that stable mapping back rather than re-registering it here
        // so this query stays a side-effect-free read of the module graph.
        let file_idx = self.ctx.resolve_symbol_file_index_stable(resolved)?;

        // A type-only edge erases the value at runtime: either this import edge
        // is itself type-only (`import type`/`import { type X }`), or the export
        // it resolves to is type-only somewhere along the re-export chain.
        let type_only = alias.is_type_only
            || self.is_export_type_only_across_binders(module_specifier, import_name);

        Some(ResolvedValueImport {
            symbol: resolved,
            file_idx,
            type_only,
        })
    }
}
