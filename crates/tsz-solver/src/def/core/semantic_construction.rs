//! Construction of a pre-populated [`DefinitionStore`] from binder semantic
//! definitions.
//!
//! The canonical binder-owned-identity -> solver `DefId` factories. They run
//! without a checker context (merge-time or single-file construction) and only
//! call the store's own registration / heritage-resolution methods, so this
//! concern lives beside the storage core rather than inside it.
//!
//! [`DefinitionStore`]: super::DefinitionStore

use super::{Atom, DefId, DefKind, DefinitionInfo, DefinitionStore};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::atomic::Ordering;

#[cfg(test)]
thread_local! {
    static DETERMINISTIC_ELECTION_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Test guard that forces [`deterministic_store_election_enabled`] on/off for the
/// current thread, so the ordering invariants can be pinned without relying on a
/// process-global env var (which an `OnceLock` would latch on first read).
#[cfg(test)]
pub(crate) struct DeterministicElectionGuard {
    previous: Option<bool>,
}

#[cfg(test)]
impl DeterministicElectionGuard {
    pub(crate) fn new(enabled: bool) -> Self {
        let previous = DETERMINISTIC_ELECTION_TEST_OVERRIDE.with(|slot| {
            let previous = slot.get();
            slot.set(Some(enabled));
            previous
        });
        Self { previous }
    }
}

#[cfg(test)]
impl Drop for DeterministicElectionGuard {
    fn drop(&mut self) {
        DETERMINISTIC_ELECTION_TEST_OVERRIDE.with(|slot| slot.set(self.previous));
    }
}

/// Whether shared-store `DefId` allocation / canonical election runs in the
/// deterministic, `#14344`-sound home-decl order (see
/// [`DefinitionStore::from_semantic_def_entries`]).
///
/// The reorder is gated so the default (all-flags-OFF) pipeline stays
/// byte-identical to `main`: the historical construction allocates `DefId`s in
/// `FxHashMap` iteration order, and the composed cross-arena `#14344` / `#14345`
/// substrate channels are what make the elected canonical observable (and what
/// the run-to-run flap manifests under). Enabled iff any campaign substrate
/// channel is active (see
/// [`CAMPAIGN_STORE_CHANNELS`](super::campaign_channels::CAMPAIGN_STORE_CHANNELS)),
/// or explicitly via `TSZ_DETERMINISTIC_STORE_ELECTION=1`. When none is set, the
/// historical construction path is preserved unchanged.
///
/// Pre-#15317 this keyed off only 4 of the publication channels, leaving a
/// determinism hole: a gauge composing e.g. `TSZ_TYPEPARAM_DECL_IDENTITY +
/// TSZ_XARENA_BASE_DECL + TSZ_XARENA_HERITAGE_TYPEARG` (none of which was in the
/// old list) kept hash-order election and could reproduce the historical
/// run-to-run flap while measuring. Deriving from the full campaign channel set
/// closes that hole; because two of those channels are read in `tsz-checker`, the
/// derivation reads their env vars directly rather than calling accessors the
/// solver cannot reach.
pub(crate) fn deterministic_store_election_enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = DETERMINISTIC_ELECTION_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return enabled;
    }

    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("TSZ_DETERMINISTIC_STORE_ELECTION").is_ok_and(|v| v == "1")
            || super::campaign_channels::any_campaign_store_channel_enabled()
    })
}

impl DefinitionStore {
    /// Create a pre-populated `DefinitionStore` from binder `SemanticDefEntry` data.
    ///
    /// This is the canonical factory for converting binder-owned stable identity
    /// into solver `DefId`s. It runs as a standalone function (no checker context
    /// needed), enabling identity creation at merge time or single-file
    /// construction time rather than as checker-side repair.
    ///
    /// The function performs three passes:
    /// 1. Create `DefId`s and `DefinitionInfo` for each `SemanticDefEntry`.
    /// 2. Wire namespace exports from `parent_namespace` relationships.
    /// 3. Resolve heritage names (extends/implements) to `DefId`s.
    ///
    /// The `intern_string` callback abstracts over `TypeInterner::intern_string`
    /// vs `QueryDatabase::intern_string`, so both the merge pipeline and checker
    /// constructors can use this without coupling to a specific interner type.
    pub fn from_semantic_defs(
        semantic_defs: &rustc_hash::FxHashMap<tsz_binder::SymbolId, tsz_binder::SemanticDefEntry>,
        intern_string: impl Fn(&str) -> Atom,
    ) -> Self {
        let entries: Vec<_> = semantic_defs
            .iter()
            .map(|(&sym_id, entry)| (sym_id, entry))
            .collect();
        Self::from_semantic_def_entries(&entries, intern_string)
    }

    /// Create a pre-populated `DefinitionStore` from a base semantic-def map plus
    /// per-file overlay maps without cloning the base map or its entries.
    ///
    /// Overlay entries take precedence over base entries with the same `SymbolId`,
    /// matching the previous clone-then-insert construction used by the CLI
    /// shared-store setup.
    pub fn from_semantic_defs_with_overlays<'a, I>(
        base: &'a rustc_hash::FxHashMap<tsz_binder::SymbolId, tsz_binder::SemanticDefEntry>,
        overlays: I,
        intern_string: impl Fn(&str) -> Atom,
    ) -> Self
    where
        I: IntoIterator<
            Item = &'a rustc_hash::FxHashMap<tsz_binder::SymbolId, tsz_binder::SemanticDefEntry>,
        >,
    {
        let mut overlay_entries: rustc_hash::FxHashMap<
            tsz_binder::SymbolId,
            &tsz_binder::SemanticDefEntry,
        > = rustc_hash::FxHashMap::default();
        for overlay in overlays {
            for (&sym_id, entry) in overlay {
                overlay_entries.insert(sym_id, entry);
            }
        }

        let mut entries = Vec::with_capacity(base.len().saturating_add(overlay_entries.len()));
        entries.extend(
            base.iter()
                .filter(|(sym_id, _)| !overlay_entries.contains_key(sym_id))
                .map(|(&sym_id, entry)| (sym_id, entry)),
        );
        entries.extend(overlay_entries);

        Self::from_semantic_def_entries(&entries, intern_string)
    }

    fn from_semantic_def_entries(
        semantic_defs: &[(tsz_binder::SymbolId, &tsz_binder::SemanticDefEntry)],
        intern_string: impl Fn(&str) -> Atom,
    ) -> Self {
        // `DefId`s are allocated sequentially in Pass 1 in the order of this
        // slice, and the shared-store elections downstream — first-wins
        // `symbol_only_index`/`symbol_to_def` (`or_insert`) and the first
        // `name_to_defs` heritage candidate — resolve by that same order. The
        // two public constructors build the slice by iterating `FxHashMap`s,
        // whose iteration order is not stable across the differing insertion
        // histories produced by parallel/overlay merges, so both the assigned
        // `DefId` values and the elected canonical copy would otherwise vary
        // run-to-run (issue #14344). Sort by the arena-invariant home-decl
        // provenance — declaring `(file_id, span_start)` (both preserved across
        // arena copies), then the raw `SymbolId` as a stable final tiebreaker —
        // so every run allocates the same `DefId`s and elects the same canonical
        // def. This is the sound #14344 home-decl identity, not an arbitrary
        // stabilization: cross-arena copies of one source decl share
        // `(file_id, span_start)` and the smaller `SymbolId` wins deterministically.
        //
        // Gated: the reorder changes which equivalent cross-arena copy is
        // elected canonical, which is observable only once the composed
        // `#14344` / `#14345` shared-store channels expose that canonical to
        // inference. Behind `deterministic_store_election_enabled` the default
        // (all-flags-OFF) construction keeps the historical `FxHashMap` order so
        // it stays byte-identical to `main`.
        let owned_sorted;
        let semantic_defs: &[(tsz_binder::SymbolId, &tsz_binder::SemanticDefEntry)] =
            if deterministic_store_election_enabled() {
                let mut ordered = semantic_defs.to_vec();
                ordered.sort_by_key(|(sym_id, entry)| (entry.file_id, entry.span_start, sym_id.0));
                owned_sorted = ordered;
                &owned_sorted
            } else {
                semantic_defs
            };

        let class_count = semantic_defs
            .iter()
            .map(|(_, entry)| *entry)
            .filter(|entry| entry.kind == tsz_binder::SemanticDefKind::Class)
            .count();
        let mut file_ids = FxHashSet::default();
        for (_, entry) in semantic_defs {
            file_ids.insert(entry.file_id);
        }
        let total_definitions = semantic_defs.len() + class_count;
        let store = Self::with_capacities(total_definitions, file_ids.len());

        if semantic_defs.is_empty() {
            return store;
        }

        let mut def_infos = Vec::with_capacity(total_definitions);
        let mut symbol_to_def: FxHashMap<u32, DefId> = FxHashMap::default();
        let mut symbol_only_index: FxHashMap<u32, DefId> = FxHashMap::default();
        let mut symbol_def_index_entries = Vec::with_capacity(semantic_defs.len());
        let mut file_to_defs: FxHashMap<u32, Vec<DefId>> = FxHashMap::default();
        let mut name_to_defs: FxHashMap<Atom, Vec<DefId>> = FxHashMap::default();
        let mut class_to_constructor_entries = Vec::with_capacity(class_count);

        symbol_to_def.reserve(semantic_defs.len());
        symbol_only_index.reserve(semantic_defs.len());
        file_to_defs.reserve(file_ids.len());
        name_to_defs.reserve(semantic_defs.len());

        let mut next_id = DefId::FIRST_VALID;

        const fn info_index(def_id: DefId) -> usize {
            def_id.0.saturating_sub(DefId::FIRST_VALID) as usize
        }

        fn preloaded_info(
            definitions: &[(DefId, DefinitionInfo)],
            def_id: DefId,
        ) -> Option<&DefinitionInfo> {
            definitions
                .get(info_index(def_id))
                .and_then(|(stored_id, info)| (*stored_id == def_id).then_some(info))
        }

        fn preloaded_info_mut(
            definitions: &mut [(DefId, DefinitionInfo)],
            def_id: DefId,
        ) -> Option<&mut DefinitionInfo> {
            definitions
                .get_mut(info_index(def_id))
                .and_then(|(stored_id, info)| (*stored_id == def_id).then_some(info))
        }

        fn record_preloaded_definition(
            def_infos: &mut Vec<(DefId, DefinitionInfo)>,
            file_to_defs: &mut FxHashMap<u32, Vec<DefId>>,
            name_to_defs: &mut FxHashMap<Atom, Vec<DefId>>,
            def_id: DefId,
            info: DefinitionInfo,
        ) {
            if let Some(file_id) = info.file_id {
                file_to_defs.entry(file_id).or_default().push(def_id);
            }
            name_to_defs.entry(info.name).or_default().push(def_id);
            def_infos.push((def_id, info));
        }

        // Pass 1: Create DefIds and DefinitionInfo for each entry.
        for (sym_id, entry) in semantic_defs {
            let info = DefinitionInfo::from_semantic_def(entry, sym_id.0, &intern_string);
            let kind = info.kind;

            let def_id = DefId(next_id);
            next_id = next_id.saturating_add(1);
            symbol_to_def.entry(sym_id.0).or_insert(def_id);
            symbol_only_index.entry(sym_id.0).or_insert(def_id);
            symbol_def_index_entries.push(((sym_id.0, entry.file_id), def_id));
            record_preloaded_definition(
                &mut def_infos,
                &mut file_to_defs,
                &mut name_to_defs,
                def_id,
                info,
            );

            if kind == DefKind::Class {
                let ctor_def_id = DefId(next_id);
                next_id = next_id.saturating_add(1);
                let ctor_info = DefinitionInfo::class_constructor_from_semantic_def(
                    entry,
                    sym_id.0,
                    &intern_string,
                );
                record_preloaded_definition(
                    &mut def_infos,
                    &mut file_to_defs,
                    &mut name_to_defs,
                    ctor_def_id,
                    ctor_info,
                );
                class_to_constructor_entries.push((def_id, ctor_def_id));
            }
        }

        // Pass 2: Wire namespace exports from parent_namespace relationships.
        for (sym_id, entry) in semantic_defs {
            if let Some(parent_sym) = entry.parent_namespace {
                let child_def = symbol_to_def.get(&sym_id.0).copied();
                let parent_def = symbol_to_def.get(&parent_sym.0).copied();
                if let (Some(child_def_id), Some(parent_def_id)) = (child_def, parent_def) {
                    let Some(name) = preloaded_info(&def_infos, child_def_id).map(|info| info.name)
                    else {
                        continue;
                    };
                    if let Some(parent_info) = preloaded_info_mut(&mut def_infos, parent_def_id) {
                        parent_info.add_export(name, child_def_id);
                    }
                }
            }
        }

        // Pass 3: Resolve heritage names to DefIds.
        for (sym_id, entry) in semantic_defs {
            let def_id = match symbol_to_def.get(&sym_id.0).copied() {
                Some(def_id) => def_id,
                None => continue,
            };

            // Resolve extends_names → DefinitionInfo.extends
            let mut resolved_extends = None;
            if !entry.extends_names.is_empty() {
                for name_str in &entry.extends_names {
                    if name_str.contains('.') {
                        continue; // property-access names resolved by checker
                    }
                    let name_atom = intern_string(name_str);
                    if let Some(candidates) = name_to_defs.get(&name_atom) {
                        for &candidate_id in candidates {
                            if candidate_id == def_id {
                                continue;
                            }
                            if let Some(candidate_info) = preloaded_info(&def_infos, candidate_id)
                                && matches!(
                                    candidate_info.kind,
                                    DefKind::Class | DefKind::Interface
                                )
                            {
                                resolved_extends = Some(candidate_id);
                                break;
                            }
                        }
                    }
                    break; // only first extends name for the extends field
                }
            }
            if let Some(extends) = resolved_extends
                && let Some(info) = preloaded_info_mut(&mut def_infos, def_id)
            {
                info.extends = Some(extends);
            }

            // Resolve implements_names → DefinitionInfo.implements
            if !entry.implements_names.is_empty() {
                let mut resolved_implements = Vec::with_capacity(entry.implements_names.len());
                for name_str in &entry.implements_names {
                    if name_str.contains('.') {
                        continue;
                    }
                    let name_atom = intern_string(name_str);
                    if let Some(candidates) = name_to_defs.get(&name_atom) {
                        for &candidate_id in candidates {
                            if candidate_id == def_id {
                                continue;
                            }
                            if let Some(candidate_info) = preloaded_info(&def_infos, candidate_id)
                                && matches!(
                                    candidate_info.kind,
                                    DefKind::Interface | DefKind::Class
                                )
                            {
                                resolved_implements.push(candidate_id);
                                break;
                            }
                        }
                    }
                }
                if !resolved_implements.is_empty()
                    && let Some(info) = preloaded_info_mut(&mut def_infos, def_id)
                {
                    info.implements = resolved_implements;
                }
            }
        }

        for (def_id, info) in def_infos {
            store.register_decl_site_identity(def_id, &info);
            store.definitions.insert(def_id, info);
        }
        for (symbol_id, def_id) in symbol_only_index {
            store.insert_symbol_only_mapping(symbol_id, def_id);
        }
        for ((symbol_id, file_id), def_id) in symbol_def_index_entries {
            store.symbol_def_index.insert((symbol_id, file_id), def_id);
        }
        for (file_id, def_ids) in file_to_defs {
            store.file_to_defs.insert(file_id, def_ids);
        }
        for (name, def_ids) in name_to_defs {
            store.name_to_defs.insert(name, def_ids);
        }
        for (class_def, ctor_def) in class_to_constructor_entries {
            store.class_to_constructor.insert(class_def, ctor_def);
        }
        store.next_id.store(next_id, Ordering::SeqCst);

        // Mark as fully populated so parallel checkers skip redundant population.
        store.mark_fully_populated();

        store
    }
}
