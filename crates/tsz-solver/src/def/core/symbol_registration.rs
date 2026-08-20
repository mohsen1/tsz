//! Atomic `(SymbolId, file_idx)` -> `DefId` registration for the shared
//! [`DefinitionStore`].
//!
//! Per-file checkers stabilize binder symbols into `DefId`s lazily. The
//! historical `lookup_by_symbol` -> `register` -> `register_symbol_mapping`
//! sequence had a race window under parallel fresh checking: two checkers
//! could both miss the lookup and each mint a distinct `DefId` for the same
//! symbol identity. The composite index kept the last writer, but each
//! checker cached its own `DefId` locally, splitting type identity
//! program-wide ("Two different types with this name exist", divergent
//! relation results — issue #13255). The entry-guarded form here makes the
//! mint atomic: the first registrar wins and every concurrent caller
//! converges on the winning `DefId`.

use super::{DefId, DefinitionInfo, DefinitionStore};

impl DefinitionStore {
    /// Atomically resolve-or-register the `DefId` for a `(SymbolId,
    /// file_idx)` identity. Returns the `DefId` and whether this call minted
    /// it (`true`) or converged on an existing registration (`false`).
    ///
    /// `info` is only consumed when this call wins the registration; every
    /// concurrent caller derives an identical `DefinitionInfo` skeleton from
    /// the same binder symbol, so dropping the loser's copy is lossless.
    pub fn register_for_symbol(
        &self,
        symbol_id: u32,
        file_idx: u32,
        info: DefinitionInfo,
    ) -> (DefId, bool) {
        use dashmap::mapref::entry::Entry;
        match self.symbol_def_index.entry((symbol_id, file_idx)) {
            Entry::Occupied(existing) => {
                let existing_def_id = *existing.get();
                if Self::decl_site_key_for_info(&info).is_none() {
                    // Raw `(symbol, file)` keys collide across binder id
                    // spaces (each test/per-file binder numbers privately).
                    // Without a decl site to disambiguate, only converge on
                    // the occupant when it records the SAME name; a
                    // different-named occupant belongs to another
                    // declaration, and adopting it hands this symbol that
                    // definition's identity (generic params/defaults leak
                    // through the symbol-keyed fallbacks).
                    let same_name = self
                        .get(existing_def_id)
                        .is_some_and(|existing_info| existing_info.name == info.name);
                    if same_name {
                        return (existing_def_id, false);
                    }
                    drop(existing);
                    return (self.register(info), true);
                }

                let existing_matches_decl_site =
                    self.get(existing_def_id).is_some_and(|existing_info| {
                        Self::infos_have_same_decl_site(&existing_info, &info)
                    });
                if existing_matches_decl_site {
                    return (existing_def_id, false);
                }

                if let Some(def_id) = self.find_decl_site_def_for_info(&info) {
                    return (def_id, false);
                }

                drop(existing);
                (self.register(info), true)
            }
            Entry::Vacant(vacant) => {
                // Cross-arena convergence: a sibling checker may have already
                // minted a `DefId` for this *declaration site* under a
                // different arena-local `(symbol, file)` identity — per-file
                // binders number symbols privately, so the same source
                // declaration reached through two arenas arrives with two
                // distinct `symbol_id`s and misses each other in this index.
                // Reuse the existing decl-site `DefId` instead of minting a
                // second one for a single declaration: a duplicate mint splits
                // the declaration's body across two `DefId`s whose independent
                // materialization order is thread-schedule dependent, so a
                // reader that resolves through the non-canonical twin observes
                // a half-built body — the diagnostic that flickers run-to-run
                // in issue #16309 (evidence #1/#2). This mirrors the
                // decl-site fallback the `Occupied` arm already applies.
                if let Some(existing_def_id) = self.find_decl_site_def_for_info(&info) {
                    vacant.insert(existing_def_id);
                    self.insert_symbol_only_mapping(symbol_id, existing_def_id);
                    self.bump_generation();
                    return (existing_def_id, false);
                }

                // `register` touches only other maps (`definitions`,
                // `name_to_defs`, `symbol_only_index`, `file_to_defs`),
                // never `symbol_def_index`, so allocating under this entry
                // guard cannot deadlock.
                let def_id = self.register(info);
                vacant.insert(def_id);
                self.insert_symbol_only_mapping(symbol_id, def_id);
                self.bump_generation();
                (def_id, true)
            }
        }
    }
}
