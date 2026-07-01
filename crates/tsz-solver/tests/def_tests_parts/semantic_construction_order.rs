// =============================================================================
// DefinitionStore semantic-construction ordering / election determinism (#14344)
// =============================================================================
//
// `from_semantic_defs` and `from_semantic_defs_with_overlays` build their entry
// slice by iterating `FxHashMap`s, whose iteration order is not stable across
// the differing insertion histories produced by parallel/overlay merges. The
// construction therefore sorts by the arena-invariant home-decl provenance —
// declaring `(file_id, span_start)` then the raw `SymbolId` — before allocating
// `DefId`s in Pass 1, so both the assigned `DefId` values and every first-wins
// election (`symbol_only_index`, `name_to_defs` heritage candidate) are the same
// on every run. These tests pin that ordering and the run-to-run stability.

fn ordering_entry(
    name: &str,
    file_id: u32,
    span_start: u32,
) -> tsz_binder::SemanticDefEntry {
    let mut entry = semantic_def_entry(name, file_id, tsz_binder::SemanticDefKind::Interface);
    entry.span_start = span_start;
    entry
}

/// `DefId`s are allocated by `(file_id, span_start, symbol_id)`, not by the
/// order symbols happen to be inserted into the source `FxHashMap`.
#[test]
fn from_semantic_defs_allocates_by_declaration_provenance() {
    let _guard = super::semantic_construction::DeterministicElectionGuard::new(true);
    let interner = create_test_interner();
    let mut defs = rustc_hash::FxHashMap::default();

    // Insert out of provenance order; the store must still elect DefIds by the
    // arena-invariant (file, span, symbol) key.
    defs.insert(tsz_binder::SymbolId(300), ordering_entry("Late", 7, 40));
    defs.insert(tsz_binder::SymbolId(100), ordering_entry("Early", 3, 10));
    defs.insert(tsz_binder::SymbolId(200), ordering_entry("Middle", 3, 20));

    let store = DefinitionStore::from_semantic_defs(&defs, |s| interner.intern_string(s));

    let early = store.find_def_by_symbol(100).expect("Early def");
    let middle = store.find_def_by_symbol(200).expect("Middle def");
    let late = store.find_def_by_symbol(300).expect("Late def");

    assert_eq!(
        [early, middle, late],
        [DefId(1), DefId(2), DefId(3)],
        "shared-store DefId election must follow stable file/span/symbol provenance"
    );
}

/// Same-`(file, span)` entries fall back to the raw `SymbolId` tiebreaker, so a
/// deterministic total order still exists.
#[test]
fn from_semantic_defs_breaks_span_ties_by_symbol_id() {
    let _guard = super::semantic_construction::DeterministicElectionGuard::new(true);
    let interner = create_test_interner();
    let mut defs = rustc_hash::FxHashMap::default();

    defs.insert(tsz_binder::SymbolId(50), ordering_entry("B", 1, 5));
    defs.insert(tsz_binder::SymbolId(40), ordering_entry("A", 1, 5));

    let store = DefinitionStore::from_semantic_defs(&defs, |s| interner.intern_string(s));

    assert_eq!(store.find_def_by_symbol(40), Some(DefId(1)));
    assert_eq!(store.find_def_by_symbol(50), Some(DefId(2)));
}

/// The elected `DefId` for each symbol is identical no matter what order the
/// entries were inserted into the base and overlay maps — the run-to-run
/// determinism property the shared store relies on.
#[test]
fn from_semantic_defs_with_overlays_is_insertion_order_independent() {
    let _guard = super::semantic_construction::DeterministicElectionGuard::new(true);
    let interner = create_test_interner();

    let build = |seed: &[(u32, &str, u32, u32)], overlay_seed: &[(u32, &str, u32, u32)]| {
        let mut base = rustc_hash::FxHashMap::default();
        for &(sym, name, file, span) in seed {
            base.insert(tsz_binder::SymbolId(sym), ordering_entry(name, file, span));
        }
        let mut overlay = rustc_hash::FxHashMap::default();
        for &(sym, name, file, span) in overlay_seed {
            overlay.insert(tsz_binder::SymbolId(sym), ordering_entry(name, file, span));
        }
        let store = DefinitionStore::from_semantic_defs_with_overlays(&base, [&overlay], |s| {
            interner.intern_string(s)
        });
        // Snapshot: (symbol -> elected DefId) for every input symbol.
        let mut mapping: Vec<(u32, u32)> = seed
            .iter()
            .chain(overlay_seed.iter())
            .map(|&(sym, _, _, _)| sym)
            .map(|sym| (sym, store.find_def_by_symbol(sym).expect("def").0))
            .collect();
        mapping.sort_unstable();
        mapping.dedup();
        mapping
    };

    let base_a = [(5u32, "E", 4u32, 40u32), (1, "A", 1, 10), (3, "C", 2, 30)];
    let overlay_a = [(2u32, "B", 1u32, 20u32), (4, "D", 3, 15)];

    // A permuted set of the same logical entries (reverse listing order).
    let base_b = [(3u32, "C", 2u32, 30u32), (1, "A", 1, 10), (5, "E", 4, 40)];
    let overlay_b = [(4u32, "D", 3u32, 15u32), (2, "B", 1, 20)];

    assert_eq!(
        build(&base_a, &overlay_a),
        build(&base_b, &overlay_b),
        "elected DefIds must be independent of FxHashMap insertion/iteration order"
    );
}
