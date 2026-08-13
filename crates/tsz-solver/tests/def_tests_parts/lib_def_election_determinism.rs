// =============================================================================
// Determinism of the lib-def election / ordering (#16309 evidence #1/#2).
//
// Under parallel fresh checking, per-file checkers can mint an intermediate,
// pre-heritage-merge def for the same lib symbol as the pre-populated
// `u32::MAX`-sentinel def (the materialized heritage-merged identity). Those
// duplicates tie on every semantic election key, so a first-match / push-order
// election otherwise depends on whichever checker thread registered last — a
// thread-schedule-dependent (nondeterministic) outcome. Ordering by the FIXED
// sentinel witness (`def_is_non_program`, i.e. `file_id == u32::MAX`) makes the
// same declaration win either way. Body-materialization state is deliberately
// NOT used: it is itself timing-dependent and cannot key a deterministic order.
// =============================================================================

#[test]
fn sentinel_witness_is_a_fixed_structural_property() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();
    let name = interner.intern_string("DecoratorContext");

    // The heritage-merged authoritative identity the parallel driver composes:
    // `u32::MAX` sentinel file id.
    let mut sentinel = DefinitionInfo::interface(name, Vec::new(), Vec::new());
    sentinel.file_id = Some(u32::MAX);
    sentinel.symbol_id = Some(42);
    let sentinel = store.register(sentinel);

    // A per-file intermediate mint for the SAME lib symbol: file-attributed.
    let mut mint = DefinitionInfo::interface(name, Vec::new(), Vec::new());
    mint.file_id = Some(5);
    mint.symbol_id = Some(42);
    let mint = store.register(mint);

    assert!(store.def_is_non_program(sentinel));
    assert!(!store.def_is_non_program(mint));
}

#[test]
fn lib_def_election_tiebreak_is_registration_order_independent() {
    // Elect the "winner" the way a same-symbol tie is broken: prefer the
    // sentinel. Registration order (which racing threads decide) must not
    // change the elected identity.
    fn winner_is_sentinel(order_sentinel_first: bool) -> bool {
        let interner = create_test_interner();
        let store = DefinitionStore::new();
        let name = interner.intern_string("DecoratorContext");

        let make_sentinel = || {
            let mut d = DefinitionInfo::interface(name, Vec::new(), Vec::new());
            d.file_id = Some(u32::MAX);
            d.symbol_id = Some(42);
            d
        };
        let make_mint = || {
            let mut d = DefinitionInfo::interface(name, Vec::new(), Vec::new());
            d.file_id = Some(5);
            d.symbol_id = Some(42);
            d
        };

        if order_sentinel_first {
            store.register(make_sentinel());
            store.register(make_mint());
        } else {
            store.register(make_mint());
            store.register(make_sentinel());
        }

        let winner = store
            .find_defs_by_name(name)
            .expect("defs registered")
            .into_iter()
            .max_by_key(|d| store.def_is_non_program(*d))
            .expect("a winner");
        store.def_is_non_program(winner)
    }

    // Both push orders elect the authoritative (sentinel) def, so the outcome
    // no longer depends on registration order (Standing Rule 5).
    assert!(winner_is_sentinel(true));
    assert!(winner_is_sentinel(false));
}
