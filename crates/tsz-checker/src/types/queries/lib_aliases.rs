//! Alias-resolution query helpers for `CheckerState`.

use crate::context::ResolvedAliasProvenance;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::symbol_flags;
use tsz_parser::parser::syntax_kind_ext;

/// Kill-switch for the provenance-aware export resolution table (Goal 4).
///
/// The table is **on by default**: unlike the prior endpoint-only experiment,
/// the memoized value carries the *full chain provenance* (visited chain +
/// symbol→file registrations), so a cache hit is observationally identical to
/// re-walking — order-independent and diagnostic-preserving for `import type` /
/// `export type *` chains. `TSZ_DISABLE_EXPORT_TABLE` force-disables it for A/B
/// kill-switch verification (the byte-identical correctness gate).
fn export_table_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        !std::env::var("TSZ_DISABLE_EXPORT_TABLE")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

impl<'a> CheckerState<'a> {
    /// Resolve an alias symbol to its target symbol.
    ///
    /// This function follows alias chains to find the ultimate target symbol.
    /// Aliases are created by:
    /// - ES6 imports: `import { foo } from 'bar'`
    /// - Import equals: `import foo = require('bar')`
    /// - Re-exports: `export { foo } from 'bar'`
    ///
    /// ## Alias Resolution:
    /// - Follows re-export chains recursively
    /// - Uses binder's `resolve_import_symbol` for ES6 imports
    /// - Falls back to `module_exports` lookup
    /// - Handles circular references with `visited_aliases` tracking
    ///
    /// ## Re-export Chains:
    /// ```typescript
    /// // a.ts exports { x } from 'b.ts'
    /// // b.ts exports { x } from 'c.ts'
    /// // c.ts exports { x }
    /// // resolve_alias_symbol('x' in a.ts) → 'x' in c.ts
    /// ```
    ///
    /// ## Returns:
    /// - `Some(SymbolId)` - The resolved target symbol
    /// - `None` - If circular reference detected or resolution failed
    pub(crate) fn resolve_alias_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
        visited_aliases: &mut AliasCycleTracker,
    ) -> Option<tsz_binder::SymbolId> {
        if crate::checkers_domain::stack_overflow_tripped() {
            return None;
        }
        if crate::checkers_domain::should_probe_stack()
            && stacker::remaining_stack().unwrap_or(usize::MAX) < 512 * 1024
        {
            crate::checkers_domain::trip_stack_overflow();
            return None;
        }

        // Goal 4: provenance-aware export resolution table. Resolving an alias /
        // re-export / `export =` chain is otherwise a deep per-reference walk
        // (≈18% of large-ts-repo infra check time as a single `resolve_alias_
        // symbol` subtree). Memoizing the *full provenance* of a chain turns
        // repeat references into one lookup whose replayed state is byte-
        // identical to re-walking.
        //
        // A memoized entry is only consulted/inserted at the *top of the chain*
        // (`visited_aliases` empty): a mid-chain endpoint can depend on which
        // aliases are already on the walk, so it is not a position-independent
        // fact. The key is `(current_file_idx, sym_id)`, mirroring
        // `export_equals_named_cache` (both components required — see
        // `AliasResolutionTable`).
        let table_enabled = export_table_enabled() && visited_aliases.len() == 0;
        let table_key = (self.ctx.current_file_idx, sym_id);

        if table_enabled && let Some(provenance) = self.alias_table_lookup(table_key) {
            // Replay the full provenance so downstream type-only / value-meaning
            // classification observes exactly the state a full walk produced.
            for &chain_sym in &provenance.chain {
                visited_aliases.reseed(chain_sym);
            }
            for &(reg_sym, reg_file) in &provenance.registrations {
                self.ctx.register_symbol_file_target(reg_sym, reg_file);
            }
            return Some(provenance.endpoint);
        }

        let epoch_before = visited_aliases.cycle_abort_epoch();
        let delta_before = self.ctx.symbol_file_target_delta_len();
        let chain_before: Vec<tsz_binder::SymbolId> = if table_enabled {
            visited_aliases.iter().collect()
        } else {
            Vec::new()
        };
        let regs_before: Vec<(tsz_binder::SymbolId, usize)> = if table_enabled {
            self.ctx.symbol_file_target_delta_entries()
        } else {
            Vec::new()
        };

        let result = stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
            self.resolve_alias_symbol_inner(sym_id, visited_aliases)
        });

        // Insert only cycle-clean top-level resolutions. The captured chain /
        // registration *delta* (what this walk added) is the provenance a later
        // hit replays. A cycle/depth-cap truncation taints the epoch and is
        // excluded, because a truncated answer is position-dependent.
        if table_enabled
            && let Some(endpoint) = result
            && visited_aliases.cycle_abort_epoch() == epoch_before
        {
            let chain_delta: Vec<tsz_binder::SymbolId> = visited_aliases
                .iter()
                .filter(|s| !chain_before.contains(s))
                .collect();
            let registrations = self.alias_table_registration_delta(delta_before, &regs_before);
            self.alias_table_insert(
                table_key,
                ResolvedAliasProvenance {
                    endpoint,
                    chain: chain_delta,
                    registrations,
                },
            );
        }

        result
    }

    /// Clone of a memoized provenance entry, or `None` on a miss. The clone
    /// keeps the borrow of the table scoped so the replay can re-enter the
    /// resolver / registrations without a `RefCell` double-borrow.
    fn alias_table_lookup(
        &self,
        key: (usize, tsz_binder::SymbolId),
    ) -> Option<ResolvedAliasProvenance> {
        self.ctx.alias_resolution_table.borrow().get(&key).cloned()
    }

    fn alias_table_insert(
        &self,
        key: (usize, tsz_binder::SymbolId),
        provenance: ResolvedAliasProvenance,
    ) {
        self.ctx
            .alias_resolution_table
            .borrow_mut()
            .insert(key, provenance);
    }

    /// Compute the symbol→file registrations a walk added, by diffing the
    /// overlay delta length and entry set captured before the walk against the
    /// current state. Only entries new to this walk are returned.
    fn alias_table_registration_delta(
        &self,
        delta_len_before: usize,
        regs_before: &[(tsz_binder::SymbolId, usize)],
    ) -> Vec<(tsz_binder::SymbolId, usize)> {
        if self.ctx.symbol_file_target_delta_len() == delta_len_before {
            return Vec::new();
        }
        let before: rustc_hash::FxHashSet<tsz_binder::SymbolId> =
            regs_before.iter().map(|&(s, _)| s).collect();
        self.ctx
            .symbol_file_target_delta_entries()
            .into_iter()
            .filter(|(s, _)| !before.contains(s))
            .collect()
    }

    /// Look up the `export =` target for a require-style consumer of a module,
    /// preferring an explicit `"export="` binding and falling back to a
    /// `"module.exports"` binding when the current file's `require`-style
    /// import of an ESM module under Node20/NodeNext should treat
    /// `export { X as "module.exports" }` as the CommonJS `module.exports = X`
    /// value.
    fn export_equals_target_for_require_consumer(
        &self,
        exports: &tsz_binder::SymbolTable,
        module_specifier: &str,
    ) -> Option<tsz_binder::SymbolId> {
        if let Some(target_sym_id) = exports.get("export=") {
            return Some(target_sym_id);
        }
        if self.current_file_uses_module_exports_require_interop(module_specifier) {
            return exports.get("module.exports");
        }
        None
    }

    fn resolve_alias_symbol_inner(
        &self,
        sym_id: tsz_binder::SymbolId,
        visited_aliases: &mut AliasCycleTracker,
    ) -> Option<tsz_binder::SymbolId> {
        // Prevent stack overflow from long alias chains
        const MAX_ALIAS_RESOLUTION_DEPTH: usize = 128;
        if visited_aliases.len() >= MAX_ALIAS_RESOLUTION_DEPTH {
            // Depth-cap truncation: the answer is position-dependent, so taint
            // the cycle epoch to keep this (sub)resolution out of the memo table.
            visited_aliases.note_cycle_abort();
            return None;
        }

        // Use get_symbol_with_libs to properly handle symbols from lib files
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        // Defensive: Verify symbol is valid before accessing fields
        // This prevents crashes when symbol IDs reference non-existent symbols
        if !symbol.has_any_flags(symbol_flags::ALIAS) {
            return Some(sym_id);
        }
        if visited_aliases.contains(&sym_id) {
            // Re-export cycle hit: position-dependent truncation.
            visited_aliases.note_cycle_abort();
            return None;
        }
        visited_aliases.push(sym_id);
        // First, try using the binder's resolve_import_symbol which follows re-export chains
        // This handles both named re-exports (`export { foo } from 'bar'`) and wildcard
        // re-exports (`export * from 'bar'`), properly following chains like:
        // a.ts exports { x } from 'b.ts'
        // b.ts exports { x } from 'c.ts'
        // c.ts exports { x }
        if let Some(resolved_sym_id) = self.ctx.binder.resolve_import_symbol(sym_id) {
            // Prevent infinite loops in re-export chains
            if visited_aliases.contains(&resolved_sym_id) {
                // Cycle: skip recursion and fall through to the fallback paths.
                // Mark the epoch so this position-dependent answer is not cached.
                visited_aliases.note_cycle_abort();
            } else {
                let mut preferred_target = resolved_sym_id;
                if let Some(module_name) = symbol.import_module.as_ref() {
                    let export_name = symbol
                        .import_name
                        .as_deref()
                        .unwrap_or(symbol.escaped_name.as_str());
                    if export_name != "*"
                        && self.symbol_is_namespace_only_tracked(resolved_sym_id, visited_aliases)
                        && let Some(member_sym_id) = self
                            .resolve_named_export_via_export_equals_tracked(
                                module_name,
                                export_name,
                                visited_aliases,
                            )
                    {
                        preferred_target = member_sym_id;
                    }
                }
                return self.resolve_alias_symbol(preferred_target, visited_aliases);
            }
        }

        // Fallback to direct module_exports lookup for backward compatibility
        // Handle ES6 imports: import { X } from 'module' or import X from 'module'
        // The binder sets import_module and import_name for these
        if let Some(ref module_name) = symbol.import_module {
            let export_name = symbol
                .import_name
                .as_deref()
                .unwrap_or(&symbol.escaped_name);
            if export_name == "default"
                && self.ctx.compiler_options.module.is_node_module()
                && self.ctx.file_is_esm == Some(true)
                && !self.module_is_esm(module_name)
            {
                return Some(sym_id);
            }
            // Resolve the declaring file the alias's module specifier is relative
            // to from the stable global index, never the order-dependent dynamic
            // overlay, so the specifier base is identical regardless of when this
            // alias is first resolved (refs #7574, #12148).
            let source_file_idx = self
                .ctx
                .resolve_symbol_file_index_stable(sym_id)
                .unwrap_or(self.ctx.current_file_idx);
            // Look up the exported symbol in module_exports.
            // Only do this for named/default imports (import_name is Some).
            // For namespace/require imports (`import X = require("m")`),
            // import_name is None and escaped_name could accidentally match
            // a specific export, resolving the alias to that export instead
            // of the module namespace.
            if symbol.import_name.is_some() {
                let export_equals_member = self.resolve_named_export_via_export_equals_tracked(
                    module_name,
                    export_name,
                    visited_aliases,
                );
                if let Some(target_sym_id) = self.resolve_cross_file_export_from_file(
                    module_name,
                    export_name,
                    Some(source_file_idx),
                ) {
                    let resolved_target =
                        if self.symbol_is_namespace_only_tracked(target_sym_id, visited_aliases) {
                            export_equals_member.unwrap_or(target_sym_id)
                        } else {
                            target_sym_id
                        };
                    if let Some(target_file_idx) =
                        self.ctx.resolve_symbol_file_index_stable(target_sym_id)
                    {
                        // Keep the alias itself pinned to the owning file so later
                        // type computation doesn't re-read a colliding local symbol
                        // with the same raw SymbolId. Pin to the target's *stable*
                        // declaring file (not the order-dependent dynamic overlay)
                        // so this alias→file mapping is identical regardless of
                        // when the alias is first resolved (refs #7574, #12148).
                        self.ctx
                            .register_symbol_file_target(sym_id, target_file_idx);
                    }
                    return Some(resolved_target);
                }
                if let Some(exports) = self.ctx.binder.module_exports.get(module_name)
                    && let Some(target_sym_id) = exports.get(export_name)
                {
                    let resolved_target =
                        if self.symbol_is_namespace_only_tracked(target_sym_id, visited_aliases) {
                            export_equals_member.unwrap_or(target_sym_id)
                        } else {
                            target_sym_id
                        };
                    // Recursively resolve if the target is also an alias
                    return self.resolve_alias_symbol(resolved_target, visited_aliases);
                }
                if let Some(all_binders) = &self.ctx.all_binders {
                    if let Some(file_indices) = self.ctx.files_for_module_specifier(module_name) {
                        for &file_idx in file_indices {
                            if let Some(binder) = all_binders.get(file_idx)
                                && let Some(exports) = binder.module_exports.get(module_name)
                                && let Some(target_sym_id) = exports.get(export_name)
                            {
                                let resolved_target = if self.symbol_is_namespace_only_tracked(
                                    target_sym_id,
                                    visited_aliases,
                                ) {
                                    export_equals_member.unwrap_or(target_sym_id)
                                } else {
                                    target_sym_id
                                };
                                return self.resolve_alias_symbol(resolved_target, visited_aliases);
                            }
                        }
                    } else {
                        for binder in all_binders.iter() {
                            if let Some(exports) = binder.module_exports.get(module_name)
                                && let Some(target_sym_id) = exports.get(export_name)
                            {
                                let resolved_target = if self.symbol_is_namespace_only_tracked(
                                    target_sym_id,
                                    visited_aliases,
                                ) {
                                    export_equals_member.unwrap_or(target_sym_id)
                                } else {
                                    target_sym_id
                                };
                                return self.resolve_alias_symbol(resolved_target, visited_aliases);
                            }
                        }
                    }
                }
            }

            if symbol.import_name.is_some()
                && let Some(target_sym_id) = self.resolve_named_export_via_export_equals_tracked(
                    module_name,
                    export_name,
                    visited_aliases,
                )
            {
                return self.resolve_alias_symbol(target_sym_id, visited_aliases);
            }

            // For namespace/require imports (`import X = require("m")` and
            // `import * as X from "m"`), import_name is `None` or `"*"` and the
            // symbol's escaped_name won't match any module export. Try the
            // module's `export =` value (key `"export="`), falling back to
            // `"module.exports"` for Node20/NodeNext CJS-of-ESM consumers —
            // see `export_equals_target_for_require_consumer`.
            if symbol.import_name.is_none() {
                let lookup = |binder: &tsz_binder::BinderState| {
                    binder.module_exports.get(module_name).and_then(|exports| {
                        self.export_equals_target_for_require_consumer(exports, module_name)
                    })
                };
                if let Some(target_sym_id) = lookup(self.ctx.binder) {
                    return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                }
                if let Some(all_binders) = &self.ctx.all_binders {
                    if let Some(file_indices) = self.ctx.files_for_module_specifier(module_name) {
                        for &file_idx in file_indices {
                            if let Some(binder) = all_binders.get(file_idx)
                                && let Some(target_sym_id) = lookup(binder)
                            {
                                return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                            }
                        }
                    } else {
                        for binder in all_binders.iter() {
                            if let Some(target_sym_id) = lookup(binder) {
                                return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                            }
                        }
                    }
                }
            }
            // Cross-file fallback: the module specifier is relative to the
            // declaring file, not the current file.  Use the stable declaring-file
            // index (never the order-dependent dynamic overlay) to find the source
            // file and resolve_import_target_from_file to convert the relative
            // specifier into a target file index (refs #7574, #12148).
            let source_file_idx = self
                .ctx
                .resolve_symbol_file_index_stable(sym_id)
                .unwrap_or(self.ctx.current_file_idx);
            if let Some(target_idx) = self
                .ctx
                .resolve_import_target_from_file(source_file_idx, module_name)
                && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
            {
                let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
                if let Some(file_name) = target_arena.source_files.first().map(|f| &f.file_name) {
                    // Namespace imports (`import * as X` / `export * as X`)
                    // resolve to the module symbol rather than a specific export.
                    // The symbol itself acts as the namespace container; record
                    // all target exports for cross-file member resolution.
                    let is_namespace_import = export_name == "*"
                        || (symbol.import_name.is_none() && symbol.escaped_name != "default");
                    if is_namespace_import {
                        if let Some(exports) =
                            self.ctx.module_exports_for_module(target_binder, file_name)
                        {
                            if let Some(export_equals_sym_id) =
                                self.export_equals_target_for_require_consumer(exports, module_name)
                            {
                                self.ctx
                                    .register_symbol_file_target(export_equals_sym_id, target_idx);
                                return Some(export_equals_sym_id);
                            }
                            for (_, &sid) in exports.iter() {
                                self.ctx.register_symbol_file_target(sid, target_idx);
                            }
                        }
                        // Keep the namespace import alias owned by the current file.
                        // Only the exported target symbols belong to the imported module.
                        // Rebinding the alias itself to the target file causes raw
                        // SymbolId collisions to overwrite local import aliases with
                        // unrelated module-local symbols from the target binder.
                        // Return the alias symbol itself — the caller
                        // resolves members through resolve_symbol_export
                        // which follows import_module re-exports.
                        return Some(sym_id);
                    }
                    if let Some(exports) =
                        self.ctx.module_exports_for_module(target_binder, file_name)
                    {
                        if let Some(target_sym_id) = exports.get(export_name) {
                            self.ctx
                                .register_symbol_file_target(target_sym_id, target_idx);
                            return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                        }
                        // For require imports, also try "export=" — and the
                        // Node20/NodeNext CJS-of-ESM `"module.exports"` fallback
                        // when applicable (see
                        // `export_equals_target_for_require_consumer`).
                        if symbol.import_name.is_none()
                            && let Some(target_sym_id) =
                                self.export_equals_target_for_require_consumer(exports, module_name)
                        {
                            self.ctx
                                .register_symbol_file_target(target_sym_id, target_idx);
                            return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                        }
                    }

                    // Follow named re-exports: `export { A } from './other'`
                    if let Some(file_reexports) =
                        self.ctx.reexports_for_file(target_binder, file_name)
                        && let Some((source_module, original_name)) =
                            file_reexports.get(export_name)
                    {
                        let name_to_lookup = original_name.as_deref().unwrap_or(export_name);
                        if let Some(reexport_target_idx) = self
                            .ctx
                            .resolve_import_target_from_file(target_idx, source_module)
                            && let Some(reexport_binder) =
                                self.ctx.get_binder_for_file(reexport_target_idx)
                        {
                            let reexport_arena =
                                self.ctx.get_arena_for_file(reexport_target_idx as u32);
                            if let Some(reexport_file) =
                                reexport_arena.source_files.first().map(|f| &f.file_name)
                                && let Some(re_exports) =
                                    reexport_binder.module_exports.get(reexport_file)
                                && let Some(target_sym_id) = re_exports.get(name_to_lookup)
                            {
                                self.ctx.register_symbol_file_target(
                                    target_sym_id,
                                    reexport_target_idx,
                                );
                                return self.resolve_alias_symbol(target_sym_id, visited_aliases);
                            }
                        }
                    }

                    // Follow wildcard re-exports: `export * from './other'`
                    // This resolves imports through chains like:
                    //   g.ts: import { A } from './f'
                    //   f.ts: export * from './e'  (where e.ts exports A)
                    if let Some(source_modules) = self
                        .ctx
                        .wildcard_reexports_for_file(target_binder, file_name)
                    {
                        let source_modules = source_modules.clone();
                        for source_module in &source_modules {
                            if let Some(wc_target_idx) = self
                                .ctx
                                .resolve_import_target_from_file(target_idx, source_module)
                                && let Some(wc_binder) = self.ctx.get_binder_for_file(wc_target_idx)
                            {
                                let wc_arena = self.ctx.get_arena_for_file(wc_target_idx as u32);
                                if let Some(wc_file) =
                                    wc_arena.source_files.first().map(|f| &f.file_name)
                                    && let Some(wc_exports) = wc_binder.module_exports.get(wc_file)
                                    && let Some(target_sym_id) = wc_exports.get(export_name)
                                {
                                    self.ctx
                                        .register_symbol_file_target(target_sym_id, wc_target_idx);
                                    return self
                                        .resolve_alias_symbol(target_sym_id, visited_aliases);
                                }
                            }
                        }
                    }
                }
            }
            // For ES6 imports, if we can't find the export, return the alias symbol itself
            // This allows the type checker to use the symbol reference
            return Some(sym_id);
        }

        let decl_idx = symbol.primary_declaration()?;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            let import = self.ctx.arena.get_import_decl(decl_node)?;
            // Track resolution depth to prevent stack overflow
            let depth = visited_aliases.len();
            if depth >= 128 {
                // Depth-cap truncation: keep this subtree out of the memo table.
                visited_aliases.note_cycle_abort();
                return None; // Prevent stack overflow
            }
            if let Some(target) =
                self.resolve_qualified_symbol_inner(import.module_specifier, visited_aliases, depth)
            {
                return Some(target);
            }
            return self
                .resolve_require_call_symbol(import.module_specifier, Some(visited_aliases));
        }
        if symbol.import_module.is_none() {
            return Some(sym_id);
        }
        // For other alias symbols (not ES6 imports or import equals), return None
        // to indicate we couldn't resolve the alias
        None
    }
}
