//! Cross-file module export / re-export / alias resolution lookups for
//! `CheckerContext`, split out of `core.rs`.
//!
//! Owns the file-name-key lookup helper plus the program-aware
//! `module_exports` / `reexports` / `wildcard_reexports` / `alias_partner`
//! resolution surface, including namespace-re-export anchor backing-file
//! resolution.

use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;

use super::CheckerContext;

impl CheckerContext<'_> {
    /// Try every file-name key variant (`./foo.ts`, `foo.ts`,
    /// backslash-normalized) against `map` and return the first match.
    ///
    /// Avoids allocating a candidate `Vec<String>` up front: direct
    /// matches and `./`-strip return immediately without building any
    /// owned strings, and the backslash-normalize / `./`-prefix branches
    /// only run when the common case misses.
    #[inline]
    fn lookup_any_file_key<'m, T>(
        file_name: &str,
        map: &'m rustc_hash::FxHashMap<String, T>,
    ) -> Option<&'m T> {
        // Direct match — common case, zero allocations.
        if let Some(v) = map.get(file_name) {
            return Some(v);
        }
        // Strip a leading `./` without allocating.
        if let Some(stripped) = file_name.strip_prefix("./")
            && let Some(v) = map.get(stripped)
        {
            return Some(v);
        }
        // Backslash-normalized variant (only allocates when input has backslashes).
        let normalized: Option<String> = if file_name.as_bytes().contains(&b'\\') {
            let n = file_name.replace('\\', "/");
            if let Some(v) = map.get(&n) {
                return Some(v);
            }
            Some(n)
        } else {
            None
        };
        let bare_prefix_needed = |c: &str| {
            !c.starts_with("./")
                && !c.starts_with("../")
                && !c.starts_with('/')
                && !c.starts_with(".\\")
                && !c.starts_with("..\\")
        };
        if bare_prefix_needed(file_name) {
            let prefixed = format!("./{file_name}");
            if let Some(v) = map.get(&prefixed) {
                return Some(v);
            }
        }
        if let Some(ref n) = normalized {
            if let Some(stripped) = n.strip_prefix("./")
                && let Some(v) = map.get(stripped)
            {
                return Some(v);
            }
            if bare_prefix_needed(n) {
                let prefixed = format!("./{n}");
                if let Some(v) = map.get(&prefixed) {
                    return Some(v);
                }
            }
        }
        None
    }

    /// Look up the re-export entries for `file_name` in the cross-file
    /// program-wide re-export map.
    ///
    /// Prefers `ProgramContext`-level `program_reexports` (a single `Arc`-shared
    /// allocation across all N cross-file lookup binders). Falls back to
    /// `binder.reexports` for standalone callers without a `ProgramContext`.
    /// Tries file-name key variants (`./foo.ts` / `foo.ts` / backslash-
    /// normalized).
    pub fn reexports_for_file<'b>(
        &'b self,
        binder: &'b tsz_binder::BinderState,
        file_name: &str,
    ) -> Option<&'b tsz_binder::FileReexports> {
        if let Some(ref idx) = self.program_reexports {
            return Self::lookup_any_file_key(file_name, idx);
        }
        Self::lookup_any_file_key(file_name, &binder.reexports)
    }

    /// See [`reexports_for_file`]: wildcard `export * from`.
    ///
    /// Each entry is `(source_module, is_type_only)`. `is_type_only` is `true`
    /// for `export type * from "X"` chains.
    pub fn wildcard_reexports_for_file<'b>(
        &'b self,
        binder: &'b tsz_binder::BinderState,
        file_name: &str,
    ) -> Option<&'b Vec<(String, bool)>> {
        if let Some(ref idx) = self.program_wildcard_reexports {
            return Self::lookup_any_file_key(file_name, idx.as_ref());
        }
        Self::lookup_any_file_key(file_name, &binder.wildcard_reexports)
    }

    /// Look up the module-exports table for a given module/file key.
    ///
    /// Prefers the project-wide `program_module_exports` (an `Arc`-shared
    /// allocation across all N cross-file lookup binders). Falls back to
    /// `binder.module_exports` for standalone callers without a
    /// `ProgramContext`. Tries file-name key variants
    /// (`./foo.ts` / `foo.ts` / backslash-normalized).
    pub fn module_exports_for_module<'b>(
        &'b self,
        binder: &'b tsz_binder::BinderState,
        module_key: &str,
    ) -> Option<&'b tsz_binder::SymbolTable> {
        let map: &'b rustc_hash::FxHashMap<String, tsz_binder::SymbolTable> =
            if let Some(ref idx) = self.program_module_exports {
                idx.as_ref()
            } else {
                binder.module_exports.as_ref()
            };
        // tsc consults ambient `declare module "x"` names only for
        // NON-relative specifiers: a relative import ('./x') resolves to a
        // file and never merges an ambient module's exports, even when the
        // ambient name equals the './'-stripped specifier. The map keys both
        // file names and ambient names, so the relative path must guard the
        // stripped candidate against declared ambient modules (wildcard
        // patterns below are unaffected — './logo.svg' still matches
        // 'declare module "*.svg"').
        let is_relative_specifier = module_key.starts_with("./")
            || module_key.starts_with("../")
            || module_key.starts_with(".\\")
            || module_key.starts_with("..\\");
        if is_relative_specifier {
            if let Some(table) = map.get(module_key) {
                return Some(table);
            }
            if let Some(stripped) = module_key
                .strip_prefix("./")
                .or_else(|| module_key.strip_prefix(".\\"))
                && !self.declared_modules_contains(binder, stripped)
                && let Some(table) = map.get(stripped)
            {
                return Some(table);
            }
            return self.lookup_wildcard_module_exports(module_key, map);
        }
        if let Some(table) = Self::lookup_any_file_key(module_key, map) {
            return Some(table);
        }
        // Wildcard ambient-module fallback: a concrete specifier (e.g.
        // `./logo.svg`) satisfied by a *pattern* module (`declare module
        // "*.svg"`) stores its exports under the pattern key. Resolve the
        // specifier onto its matching pattern as tsc does, else bindings = `any`.
        self.lookup_wildcard_module_exports(module_key, map)
    }

    /// Resolve a concrete module specifier onto a declared *wildcard* ambient
    /// module's export table, when no exact `module_exports` key matched.
    ///
    /// Returns `None` for keys that are themselves wildcard patterns (a pattern
    /// is never resolved against another pattern) and when no declared pattern
    /// matches. The chosen pattern follows tsc's longest-prefix preference.
    fn lookup_wildcard_module_exports<'b>(
        &self,
        module_key: &str,
        map: &'b rustc_hash::FxHashMap<String, tsz_binder::SymbolTable>,
    ) -> Option<&'b tsz_binder::SymbolTable> {
        let normalized = module_key.trim().trim_matches('"').trim_matches('\'');
        if normalized.contains('*') {
            return None;
        }
        // Fast path: the project-wide skeleton index already separates the
        // wildcard patterns, so most projects (which declare none) skip the scan
        // entirely, and those that do match against a small pre-built list.
        if let Some(dm) = &self.global_declared_modules {
            if dm.patterns.is_empty() {
                return None;
            }
            return dm
                .best_matching_pattern(normalized)
                .and_then(|pattern| map.get(pattern));
        }
        // Standalone/test fallback (no skeleton index): scan the export map's own
        // keys for wildcard patterns, ranked by the same longest-prefix rule.
        crate::context::global_declared_modules::best_wildcard_match(
            map.keys().map(String::as_str),
            normalized,
        )
        .and_then(|key| map.get(key))
    }

    /// Like `module_exports_for_module` but tests existence only.
    pub fn module_exports_contains_module(
        &self,
        binder: &tsz_binder::BinderState,
        module_key: &str,
    ) -> bool {
        self.module_exports_for_module(binder, module_key).is_some()
    }

    /// Resolve a node → symbol lookup by arena pointer against the
    /// cross-file node-symbol map. Prefers the shared project-wide map
    /// installed by `ProgramContext::apply_to`; falls back to the per-binder
    /// copy for tests and standalone callers.
    pub fn cross_file_node_symbols_for_arena<'b>(
        &'b self,
        binder: &'b tsz_binder::BinderState,
        arena_ptr: usize,
    ) -> Option<&'b Arc<FxHashMap<u32, SymbolId>>> {
        if let Some(ref idx) = self.program_cross_file_node_symbols {
            return idx.get(&arena_ptr);
        }
        binder.cross_file_node_symbols.get(&arena_ptr)
    }

    /// Test whether `module_name` is declared as an ambient module anywhere
    /// in the project. Prefers the project-wide `global_declared_modules`
    /// index built from the skeleton; falls back to the per-binder
    /// `declared_modules` set for tests / standalone callers.
    pub fn declared_modules_contains(
        &self,
        binder: &tsz_binder::BinderState,
        module_name: &str,
    ) -> bool {
        if let Some(ref dm) = self.global_declared_modules {
            return dm.exact.contains(module_name);
        }
        binder.declared_modules.contains(module_name)
    }

    /// Resolve `sym_id` to its alias partner. Prefers the project-wide
    /// `program_alias_partners` map installed by `ProgramContext::apply_to`;
    /// falls back to per-binder `alias_partners` for tests/standalone callers.
    pub fn alias_partner_for(
        &self,
        binder: &tsz_binder::BinderState,
        sym_id: SymbolId,
    ) -> Option<SymbolId> {
        if let Some(ref ap) = self.program_alias_partners {
            return ap.get(&sym_id).copied();
        }
        binder.alias_partners.get(&sym_id).copied()
    }

    /// Test whether `sym_id` has an alias partner. Prefers the project-wide
    /// map; falls back to per-binder.
    pub fn alias_partners_contains(
        &self,
        binder: &tsz_binder::BinderState,
        sym_id: SymbolId,
    ) -> bool {
        if let Some(ref ap) = self.program_alias_partners {
            return ap.contains_key(&sym_id);
        }
        binder.alias_partners.contains_key(&sym_id)
    }

    /// Reverse lookup: find the `TYPE_ALIAS` partner that points at
    /// `alias_sym_id`. Used by the type-position symbol resolver to redirect
    /// an ALIAS symbol back to its merged `TYPE_ALIAS` counterpart. Prefers
    /// the project-wide map; falls back to the per-binder map for
    /// standalone callers.
    pub fn alias_partner_reverse(
        &self,
        binder: &tsz_binder::BinderState,
        alias_sym_id: SymbolId,
    ) -> Option<SymbolId> {
        if let Some(ref ap) = self.program_alias_partners {
            return ap.iter().find_map(|(&type_alias_id, &alias_id)| {
                (alias_id == alias_sym_id).then_some(type_alias_id)
            });
        }
        binder
            .alias_partners
            .iter()
            .find_map(|(&type_alias_id, &alias_id)| {
                (alias_id == alias_sym_id).then_some(type_alias_id)
            })
    }

    /// Resolve a member exported by the target module of an ALIAS symbol.
    ///
    /// When an ALIAS symbol's `import_module` holds a relative specifier
    /// (e.g., `"./Something"`), it must be resolved from the ALIAS's source
    /// file, not the current file.  This helper uses `cross_file_symbol_targets`
    /// to find the ALIAS's origin file, resolves the specifier from that file's
    /// perspective, then looks up the member in the target module's exports.
    pub fn resolve_alias_import_member(
        &self,
        alias_id: tsz_binder::SymbolId,
        module_specifier: &str,
        member_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let source_file_idx = self.resolve_symbol_file_index(alias_id)?;
        let target_idx = self.resolve_import_target_from_file(source_file_idx, module_specifier)?;
        let mut visited = FxHashSet::default();
        self.resolve_export_in_target_file(target_idx, member_name, &mut visited)
    }

    /// Resolve `export_name` from `target_idx`'s public surface, following named
    /// and wildcard re-export edges across binder boundaries.
    ///
    /// Unlike [`tsz_binder::BinderState::resolve_import_with_reexports_type_only`],
    /// this consults the program-wide export/re-export indexes through
    /// [`Self::module_exports_for_module`], [`Self::reexports_for_file`], and
    /// [`Self::wildcard_reexports_for_file`]. Those indexes are the authoritative
    /// source of truth in multi-file mode, where each file's own binder keeps its
    /// `module_exports`/`reexports`/`wildcard_reexports` tables empty and the
    /// data is hoisted into the program skeleton instead. Resolving a member
    /// through a re-exported namespace/alias that lives in another binder must go
    /// through this program-aware path or the lookup silently misses every
    /// export (the cause of the cross-binder TS2503/TS2339 family).
    pub fn resolve_export_in_target_file(
        &self,
        target_idx: usize,
        export_name: &str,
        visited: &mut FxHashSet<usize>,
    ) -> Option<tsz_binder::SymbolId> {
        if !visited.insert(target_idx) {
            return None;
        }
        let target_binder = self.get_binder_for_file(target_idx)?;
        let target_arena = self.get_arena_for_file(target_idx as u32);
        let file_name = target_arena.source_files.first()?.file_name.clone();

        // Direct exports (program-aware).
        if let Some(exports) = self.module_exports_for_module(target_binder, &file_name)
            && let Some(sym_id) = exports.get(export_name)
            && target_binder.get_symbol(sym_id).is_some()
        {
            self.register_symbol_file_target(sym_id, target_idx);
            return Some(sym_id);
        }

        // Named re-exports: `export { foo } from './other'` (and `as` renames).
        if let Some(reexports) = self.reexports_for_file(target_binder, &file_name)
            && let Some((source_module, original_name)) = reexports.get(export_name)
        {
            let name = original_name.as_deref().unwrap_or(export_name);
            if let Some(source_idx) =
                self.resolve_import_target_from_file(target_idx, source_module)
                && let Some(resolved) =
                    self.resolve_export_in_target_file(source_idx, name, visited)
            {
                return Some(resolved);
            }
        }

        // Wildcard re-exports: `export * from './other'`.
        if let Some(source_modules) = self.wildcard_reexports_for_file(target_binder, &file_name) {
            let source_modules = source_modules.clone();
            for (source_module, _is_type_only) in &source_modules {
                if let Some(source_idx) =
                    self.resolve_import_target_from_file(target_idx, source_module)
                    && let Some(resolved) =
                        self.resolve_export_in_target_file(source_idx, export_name, visited)
                {
                    return Some(resolved);
                }
            }
        }

        // Fallback: the target binder's own re-export resolution for
        // single-file / ambient-module binders whose local tables are
        // populated (e.g. `declare module "x" { ... }`).
        target_binder
            .resolve_import_with_reexports_type_only(&file_name, export_name)
            .map(|(sym_id, _)| {
                self.register_symbol_file_target(sym_id, target_idx);
                sym_id
            })
    }

    /// When `alias_id` is a *named* import bound to an `export * as NS from '<m>'`
    /// namespace re-export, return the file index of the re-exported module `<m>`
    /// — the backing module whose exports are the anchor's members. Returns
    /// `None` for any other alias shape.
    ///
    /// `tsc` treats such a named import as a type-position namespace anchor whose
    /// members are the exports of `<m>`. Because the member is not part of the
    /// *importing* module's own export surface, the ordinary re-export member
    /// lookup misses it; [`Self::resolve_member_via_namespace_reexport`] resolves
    /// the member through this backing file instead. The hop is keyed by file
    /// index + module specifier (never raw `SymbolId`), so cross-binder id
    /// collisions cannot interfere. This is also the structural predicate behind
    /// the "missing member is TS2694, not TS2503" diagnostic.
    pub(crate) fn namespace_reexport_anchor_backing_file(
        &self,
        alias_id: tsz_binder::SymbolId,
    ) -> Option<usize> {
        let alias = self.binder.get_symbol(alias_id)?;
        if !alias.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
            return None;
        }
        // A whole-namespace import (`import * as NS`) is handled by the ordinary
        // re-export path; this targets a *named* binding (`import { NS }` /
        // `import { NS as X }`) of an `export * as NS` re-export. Check before
        // allocating below so the common star-import case stays allocation-free.
        let import_name = alias.import_name()?;
        if import_name == "*" {
            return None;
        }
        let import_module = alias.import_module()?.to_string();
        let import_name = import_name.to_string();
        // The alias's declaring file is the base for resolving the relative
        // `import_module` specifier. `resolve_symbol_file_index` can be polluted
        // by an earlier cross-file `register_symbol_file_target` (it pins the
        // importing alias to the *target* file), and a named import is local to
        // the current file anyway, so try the current file first and fall back to
        // the recorded index when it differs.
        let recorded = self
            .resolve_symbol_file_index(alias_id)
            .filter(|&recorded| recorded != self.current_file_idx);
        std::iter::once(self.current_file_idx)
            .chain(recorded)
            .find_map(|source_file_idx| {
                self.namespace_reexport_anchor_backing_file_from(
                    source_file_idx,
                    &import_module,
                    &import_name,
                )
            })
    }

    /// Resolve `NS.member` to its target `SymbolId` when `NS` (`alias_id`) is a
    /// named import bound to an `export * as NS` namespace re-export. Returns
    /// `None` when `alias_id` is not such an anchor or the module has no such
    /// member. Shared by the qualified-name type resolvers so the
    /// backing-file + export lookup lives in one place.
    pub(crate) fn resolve_member_via_namespace_reexport(
        &self,
        alias_id: tsz_binder::SymbolId,
        member_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let backing_idx = self.namespace_reexport_anchor_backing_file(alias_id)?;
        let mut visited = rustc_hash::FxHashSet::default();
        self.resolve_export_in_target_file(backing_idx, member_name, &mut visited)
    }

    /// One attempt of [`Self::namespace_reexport_anchor_backing_file`] using
    /// `source_file_idx` as the base for the relative `import_module` specifier.
    fn namespace_reexport_anchor_backing_file_from(
        &self,
        source_file_idx: usize,
        import_module: &str,
        import_name: &str,
    ) -> Option<usize> {
        let target_idx = self.resolve_import_target_from_file(source_file_idx, import_module)?;
        let target_binder = self.get_binder_for_file(target_idx)?;
        let target_arena = self.get_arena_for_file(target_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();
        let ns_sym_id = self
            .module_exports_for_module(target_binder, &target_file_name)
            .and_then(|exports| exports.get(import_name))?;
        let ns_sym = target_binder.get_symbol(ns_sym_id)?;
        // Only an `export * as NS` namespace re-export qualifies: it carries an
        // import module and the wildcard `*` import name.
        if !ns_sym.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            || ns_sym.import_name() != Some("*")
        {
            return None;
        }
        let ns_module = ns_sym.import_module()?;
        self.resolve_import_target_from_file(target_idx, ns_module)
    }
}
