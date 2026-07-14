//! `DefId` pre-population and local-cache warming for `CheckerContext`,
//! split out of `def_mapping.rs`.
//!
//! Owns `pre_populate_def_ids_from_*`, the shared `populate_def_ids_from_semantic_defs`
//! helper, `warm_local_caches_from_shared_store`, and cross-batch heritage
//! resolution.

use tracing::trace;

use super::eager_warm_local_caches;
use crate::context::CheckerContext;

impl CheckerContext<'_> {
    /// Pre-populate `symbol_to_def` and `def_to_symbol` from the binder's
    /// `semantic_defs` index (Phase 1 DefId-first stable identity).
    ///
    /// Called once during checker construction so that `get_or_create_def_id`
    /// finds stable `DefIds` already present for top-level declarations. This
    /// moves identity creation to bind time (deterministic, early) rather than
    /// being recovered on-demand in hot checker paths (late, order-dependent).
    ///
    /// Returns the number of `DefIds` pre-populated.
    pub fn pre_populate_def_ids_from_binder(&self) -> usize {
        self.populate_def_ids_from_semantic_defs(&self.binder.semantic_defs)
    }

    /// Pre-populate `symbol_to_def` and `def_to_symbol` from all lib binders'
    /// `semantic_defs` indices.
    ///
    /// Lib binders contain definitions for standard library types (Array, Promise,
    /// Error, Map, etc.). Without this, every `get_or_create_def_id` call for a
    /// lib symbol falls through to the Step 3 O(N) `lib_contexts.iter()` scan to
    /// find the symbol and create its DefId on demand. By pre-populating here, these
    /// symbols hit the O(1) `find_def_by_symbol` path in Step 2 instead.
    ///
    /// Returns the total number of `DefIds` pre-populated across all lib binders.
    pub fn pre_populate_def_ids_from_lib_binders(&self) -> usize {
        let mut total = 0;
        for lib_ctx in self.lib_contexts.iter() {
            total += self.populate_def_ids_from_semantic_defs(&lib_ctx.binder.semantic_defs);
        }
        total
    }

    /// Pre-populate `symbol_to_def` and `def_to_symbol` from all cross-file
    /// binders' `semantic_defs` indices (multi-file stable identity).
    ///
    /// In multi-file compilation, each file has its own binder with its own
    /// `semantic_defs`. Without this, cross-file type references (e.g.,
    /// importing a class from another file) hit the O(N) `all_binders` scan
    /// in `get_or_create_def_id` Step 3 and create `DefIds` on demand.
    ///
    /// By pre-populating here, those `SymbolIds` are already registered in the
    /// `DefinitionStore`'s `symbol_only_index`, so `get_or_create_def_id`
    /// Step 2 finds them in O(1) without the repair path.
    ///
    /// Called from `ProgramContext::apply_to` after `set_all_binders`.
    /// Safe to overlap with `pre_populate_def_ids_from_binder` (the current
    /// file's binder may also appear in `all_binders`); the dedup check in
    /// `populate_def_ids_from_semantic_defs` skips already-registered entries.
    ///
    /// Returns the total number of new `DefIds` pre-populated.
    pub fn pre_populate_def_ids_from_all_binders(&self) -> usize {
        let Some(ref binders) = self.all_binders else {
            return 0;
        };
        let mut total = 0;
        for binder in binders.iter() {
            total += self.populate_def_ids_from_semantic_defs(&binder.semantic_defs);
        }
        total
    }

    /// Core helper: populate DefId mappings from a `semantic_defs` map.
    ///
    /// Used by both `pre_populate_def_ids_from_binder` (primary binder) and
    /// `pre_populate_def_ids_from_lib_binders` (lib binders). The logic is
    /// identical: convert `SemanticDefEntry` to `DefinitionInfo`, register in
    /// the `DefinitionStore`, and populate local caches.
    fn populate_def_ids_from_semantic_defs(
        &self,
        semantic_defs: &rustc_hash::FxHashMap<tsz_binder::SymbolId, tsz_binder::SemanticDefEntry>,
    ) -> usize {
        use tsz_solver::def::{DefKind, DefinitionInfo};

        if semantic_defs.is_empty() {
            return 0;
        }

        // Sort by span_start so that DefIds are allocated in source declaration
        // order. FxHashMap iteration order is non-deterministic, which previously
        // caused type alias DefIds to be allocated out of source order. This broke
        // union member sorting (which compares Lazy(DefId) bases by DefId value)
        // and produced wrong display order in diagnostics (e.g., `Z | Y | X`
        // instead of `X | Y | Z`).
        let mut sorted_entries: Vec<_> = semantic_defs.iter().collect();
        sorted_entries.sort_by_key(|(_, entry)| entry.span_start);

        let mut count = 0;
        // Hold s2d/d2s borrows across the whole loop. Both register_def_kind_in_envs
        // and definition_store.register only touch type_env/type_environment and the
        // shared DashMap, never s2d/d2s. Saves 2 RefCell ops per inserted entry.
        let mut s2d = self.symbol_to_def.borrow_mut();
        let mut d2s = self.def_to_symbol.borrow_mut();
        for (&sym_id, entry) in sorted_entries {
            // Skip if already mapped (e.g., from a previous lib merge pass
            // or the primary binder's pre-population).
            if s2d.contains_key(&sym_id) {
                continue;
            }

            // Also skip if the DefinitionStore already has a mapping for this
            // symbol (e.g., from another lib binder that declared the same
            // global interface via declaration merging).
            if self.definition_store.find_def_by_symbol(sym_id.0).is_some() {
                continue;
            }

            let intern = |s: &str| self.types.intern_string(s);
            let info = DefinitionInfo::from_semantic_def(entry, sym_id.0, &intern);
            let kind = info.kind;

            let def_id = self.definition_store.register(info);
            trace!(
                symbol_name = %entry.name,
                symbol_id = %sym_id.0,
                def_id = %def_id.0,
                kind = ?kind,
                "Pre-populated DefId from semantic_defs"
            );

            // Register in the authoritative index so other checker contexts
            // can find this DefId via lookup_by_symbol() without creating
            // duplicates. This closes the gap where pre-populated DefIds
            // were only in the local cache but invisible to the shared store.
            self.definition_store
                .register_symbol_mapping(sym_id.0, entry.file_id, def_id);

            s2d.insert(sym_id, def_id);
            d2s.insert(def_id, sym_id);

            // Propagate DefKind to both TypeEnvironments (evaluator + flow-analyzer)
            self.register_def_kind_in_envs(def_id, kind);

            // For classes, create a ClassConstructor companion DefId so the
            // checker can reuse stable identity instead of creating one on demand.
            // The body is left empty (filled lazily during type checking).
            if kind == DefKind::Class {
                let ctor_info =
                    DefinitionInfo::class_constructor_from_semantic_def(entry, sym_id.0, &intern);
                let ctor_def_id = self.definition_store.register(ctor_info);
                self.definition_store
                    .register_constructor_companion(def_id, ctor_def_id);
                self.register_def_kind_in_envs(ctor_def_id, DefKind::ClassConstructor);
            }

            count += 1;
        }

        // Pass 2: Wire namespace exports from parent_namespace relationships.
        // After all DefIds are created/warmed, walk entries with parent_namespace
        // and register them as exports of their parent's DefinitionInfo.
        for (&sym_id, entry) in semantic_defs {
            if let Some(parent_sym) = entry.parent_namespace {
                let child_def = self.definition_store.find_def_by_symbol(sym_id.0);
                let parent_def = self.definition_store.find_def_by_symbol(parent_sym.0);
                if let (Some(child_def_id), Some(parent_def_id)) = (child_def, parent_def) {
                    let name = self.types.intern_string(&entry.name);
                    self.definition_store
                        .add_export(parent_def_id, name, child_def_id);
                }
            }
        }

        count
    }

    /// Warm local `symbol_to_def` / `def_to_symbol` caches from the shared
    /// `DefinitionStore` in a single pass.
    ///
    /// When the checker receives a pre-populated `DefinitionStore` from the
    /// merge pipeline (via `with_options_and_shared_def_store`), this method
    /// is more efficient than `pre_populate_def_ids_from_binder()` +
    /// `pre_populate_def_ids_from_lib_binders()` because it reads directly
    /// from the store's authoritative symbol→DefId index instead of
    /// re-iterating each binder's `semantic_defs` and re-converting
    /// `SemanticDefEntry` → `DefinitionInfo`.
    ///
    /// Also discovers `ClassConstructor` companion `DefId`s for any class `DefId`s
    /// encountered, ensuring constructor identity is warm from the start
    /// rather than being created on-demand in checker hot paths.
    ///
    /// Returns the number of mappings warmed.
    pub fn warm_local_caches_from_shared_store(&self) -> usize {
        // Fast path: skip if already warmed (avoids iterating all mappings a
        // second time when the constructor already called this method).
        if self.local_caches_warmed.get() {
            return 0;
        }

        if self.definition_store.is_empty() {
            return 0;
        }

        // Lazy-warm (default): the eager bulk copy below installs every
        // *cross-file* symbol of the whole program into this single file's
        // local `symbol_to_def` / `def_to_symbol` maps. The snapshot it reads
        // (`all_symbol_mappings_snapshot`) holds one entry per program symbol,
        // so the loop is O(program-symbols) per file and copies
        // O(program-symbols) entries into every per-file checker — an
        // O(files × program-symbols) time-and-memory term that rises with
        // total program size (the scale cliff).
        //
        // Every consumer of these local maps already has an authoritative
        // O(1) fallback into the shared store on a cache miss: cross-file
        // `SymbolId → DefId` resolves through `get_or_create_def_id` /
        // `symbol_to_def_id` (`DefinitionStore::lookup_by_symbol`), and the
        // reverse `DefId → SymbolId` resolves through `def_to_symbol_id`
        // (`DefinitionStore::get_symbol_id`). Skipping the eager copy makes
        // each file populate only the cross-file symbols it actually
        // references, on demand, restoring per-file cost that is independent
        // of total program size. Set `TSZ_EAGER_WARM_LOCAL_CACHES=1` to
        // restore the legacy eager bulk copy (rollback escape hatch).
        if !eager_warm_local_caches() {
            self.local_caches_warmed.set(true);
            return 0;
        }

        let mappings = self.definition_store.all_symbol_mappings_snapshot();
        let mut count = 0;

        // Hold both RefCell borrows for the entire loop. The previous
        // implementation re-borrowed `symbol_to_def` and `def_to_symbol`
        // on every iteration (one borrow + two borrow_mut per mapping),
        // which adds ~3 RefCell ops per entry. With 10k+ mappings on
        // large repos this is measurable. The body never recurses into
        // these RefCells, so a single mut borrow is safe.
        let symbols = self.binder.get_symbols();
        let mut s2d = self.symbol_to_def.borrow_mut();
        let mut d2s = self.def_to_symbol.borrow_mut();

        // Pre-size to avoid rehashing during bulk insertion.
        if mappings.len() > s2d.len() {
            let additional = mappings.len() - s2d.len();
            s2d.reserve(additional);
            d2s.reserve(additional);
        }

        for &(raw_sym_id, def_id) in mappings.iter() {
            let sym_id = tsz_binder::SymbolId(raw_sym_id);

            // Skip if already in local cache (e.g., from a prior warm pass).
            if s2d.contains_key(&sym_id) {
                continue;
            }

            // Skip if the current file's binder owns a symbol at this SymbolId.
            // The shared store's `symbol_only_index` is file-agnostic: the same
            // raw `SymbolId(u32)` from different files maps to whichever DefId
            // was registered first.  Installing such a mapping would pollute this
            // file's local caches with a DefId that belongs to a *different*
            // file's symbol — for example, File A's `Set` (SymbolId 5) would
            // shadow File B's `AtTop` (also SymbolId 5) during instanceof
            // narrowing.  Skipping here is safe: when the local symbol is
            // actually referenced, `get_or_create_def_id` will resolve it
            // correctly through the file-aware `symbol_def_index`.
            if symbols.get(sym_id).is_some() {
                continue;
            }

            s2d.insert(sym_id, def_id);
            d2s.insert(def_id, sym_id);

            // NOTE: DefKind registration is intentionally skipped here.
            // The TypeEnvironment is rebuilt from scratch in build_type_environment()
            // (called later in check_source_file), and ensure_both_envs_have_definition_store()
            // installs the DefinitionStore reference into both TypeEnvironments for lazy DefKind fallback.
            // Eagerly registering DefKinds here would be overwritten and wastes
            // N DashMap lookups per symbol (for .get() and .get_constructor_def()).

            count += 1;
        }
        drop(s2d);
        drop(d2s);

        trace!(
            count,
            total_mappings = mappings.len(),
            "Warmed local caches from shared DefinitionStore"
        );

        self.local_caches_warmed.set(true);
        count
    }

    /// Returns `true` if the shared `DefinitionStore` has been pre-populated
    /// (i.e., it contains definitions registered at merge time, not just an
    /// empty store created by the default constructor).
    ///
    /// When true, `warm_local_caches_from_shared_store()` can replace the
    /// more expensive `pre_populate_def_ids_from_binder()` +
    /// `pre_populate_def_ids_from_lib_binders()` calls.
    pub fn has_shared_store(&self) -> bool {
        !self.definition_store.is_empty()
    }

    /// Resolve heritage for definitions whose extends/implements targets were
    /// not found during their batch's pass 2 (cross-batch heritage).
    ///
    /// This handles the common case where a user class extends a lib type
    /// (e.g., `class MyError extends Error`): when `pre_populate_def_ids_from_binder`
    /// processes the user file, the lib type's `DefId` hasn't been registered yet
    /// (lib binders are pre-populated separately). After ALL pre-population batches
    /// complete, this method resolves the remaining heritage using the
    /// `DefinitionStore`'s name index, which now contains entries from all batches.
    ///
    /// Called once during checker construction after all `pre_populate_*` methods.
    /// Returns the number of heritage links resolved.
    pub fn resolve_cross_batch_heritage(&self) -> usize {
        use tsz_solver::def::DefKind;

        let mut resolved_count = 0;

        // Collect all semantic_defs from all sources (primary binder + all_binders).
        // The shared DefinitionStore's name_to_defs index is already populated from
        // all pre-population batches, so name-based lookups will find targets from
        // any batch (user files, lib files, cross-file binders).
        let sources: Vec<
            &rustc_hash::FxHashMap<tsz_binder::SymbolId, tsz_binder::SemanticDefEntry>,
        > = {
            // `&*x.semantic_defs` dereferences the `Arc<FxHashMap<...>>` so the
            // resulting reference targets the underlying map (the type the Vec
            // expects), not the Arc wrapper.
            let mut v = vec![&*self.binder.semantic_defs];
            for lib_ctx in self.lib_contexts.iter() {
                v.push(&*lib_ctx.binder.semantic_defs);
            }
            if let Some(ref binders) = self.all_binders {
                for binder in binders.iter() {
                    v.push(&*binder.semantic_defs);
                }
            }
            v
        };

        for source in &sources {
            for (&sym_id, entry) in *source {
                let def_id = match self.definition_store.find_def_by_symbol(sym_id.0) {
                    Some(id) => id,
                    None => continue,
                };

                // Skip if extends is already wired (from pre-populate Pass 3)
                if let Some(info) = self.definition_store.get(def_id)
                    && info.extends.is_some()
                {
                    continue;
                }

                // Resolve extends_names → extends
                for name_str in &entry.extends_names {
                    if name_str.contains('.') {
                        continue;
                    }
                    let name_atom = self.types.intern_string(name_str);
                    if let Some(candidates) = self.definition_store.find_defs_by_name(name_atom) {
                        for &candidate_id in &candidates {
                            if candidate_id == def_id {
                                continue;
                            }
                            if let Some(info) = self.definition_store.get(candidate_id)
                                && matches!(info.kind, DefKind::Class | DefKind::Interface)
                            {
                                self.definition_store.set_extends(def_id, candidate_id);
                                resolved_count += 1;
                                break;
                            }
                        }
                    }
                    break; // only first extends name
                }

                // Resolve implements_names → implements
                if !entry.implements_names.is_empty() {
                    let mut resolved = Vec::new();
                    for name_str in &entry.implements_names {
                        if name_str.contains('.') {
                            continue;
                        }
                        let name_atom = self.types.intern_string(name_str);
                        if let Some(candidates) = self.definition_store.find_defs_by_name(name_atom)
                        {
                            for &candidate_id in &candidates {
                                if candidate_id == def_id {
                                    continue;
                                }
                                if let Some(info) = self.definition_store.get(candidate_id)
                                    && matches!(info.kind, DefKind::Interface | DefKind::Class)
                                {
                                    resolved.push(candidate_id);
                                    break;
                                }
                            }
                        }
                    }
                    if !resolved.is_empty() {
                        self.definition_store
                            .set_implements(def_id, resolved.clone());
                        resolved_count += resolved.len();
                    }
                }
            }
        }

        resolved_count
    }
}
