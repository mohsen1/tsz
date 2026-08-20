// =============================================================================
// register_for_symbol cross-arena convergence tests (issue #16309)
//
// Rule: when two per-file checker arenas stabilize the same source
// declaration, their private binders number its symbol differently, so the
// shared `(symbol_id, file_idx)` index sees two distinct keys for one
// declaration. `register_for_symbol` must converge both on a single `DefId`
// (via the decl-site index) instead of minting a second one — a duplicate
// mint splits the declaration's body across two `DefId`s whose independent
// materialization order is thread-schedule dependent, which is the source of
// the run-to-run diagnostic flicker tracked in #16309.
//
// These are threads-free: they drive the two arenas' registrations directly
// and in both orders, proving the winning identity does not depend on which
// arena the scheduler runs first.
// =============================================================================

fn decl_site_info(name: Atom, symbol_id: u32) -> DefinitionInfo {
    let mut info = DefinitionInfo::interface(name, Vec::new(), Vec::new());
    info.file_id = Some(7);
    info.span = Some((42, 42));
    info.symbol_id = Some(symbol_id);
    info
}

#[test]
fn register_for_symbol_converges_arena_local_twins_on_one_def() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();
    let name = interner.intern_string("Box");

    // Two arenas stabilize the same source declaration. Per-file binders
    // number symbols privately, so the shared `(symbol, file)` index sees two
    // distinct keys (100 / 200) that never collide — the pre-fix gap that let
    // each arena mint its own `DefId` for one declaration.
    let (first, first_minted) = store.register_for_symbol(100, 7, decl_site_info(name, 100));
    let (second, second_minted) = store.register_for_symbol(200, 7, decl_site_info(name, 200));

    assert!(first_minted, "the first registrar mints the canonical def");
    assert!(
        !second_minted,
        "the second arena must converge on the decl-site def, not mint a twin"
    );
    assert_eq!(
        first, second,
        "both arena-local twins must resolve to one DefId so the declaration \
         has a single body"
    );
    // The composite index for the second arena's own key now points at the
    // shared def, so later lookups agree regardless of which arena asks.
    assert_eq!(store.lookup_by_symbol(200, 7), Some(first));
}

#[test]
fn register_for_symbol_convergence_is_registration_order_independent() {
    let interner = create_test_interner();
    let name = interner.intern_string("Box");

    // Registering the twins in the opposite order must reach the same shape:
    // one shared DefId, minted by whoever arrives first. This is the
    // threads-free proof that the winning identity does not depend on which
    // arena the scheduler runs first — the property #16309 needs.
    let forward = DefinitionStore::new();
    let (f1, _) = forward.register_for_symbol(100, 7, decl_site_info(name, 100));
    let (f2, _) = forward.register_for_symbol(200, 7, decl_site_info(name, 200));

    let reverse = DefinitionStore::new();
    let (r2, _) = reverse.register_for_symbol(200, 7, decl_site_info(name, 200));
    let (r1, _) = reverse.register_for_symbol(100, 7, decl_site_info(name, 100));

    assert_eq!(f1, f2, "forward order converges");
    assert_eq!(r1, r2, "reverse order converges");
    assert!(
        forward.defs_have_same_decl_site(f1, f2) && reverse.defs_have_same_decl_site(r1, r2),
        "both orders recognize the shared declaration site"
    );
}

#[test]
fn register_for_symbol_keeps_distinct_decl_sites_apart() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();
    let name = interner.intern_string("Box");

    let (first, _) = store.register_for_symbol(100, 7, decl_site_info(name, 100));

    // A genuinely different declaration (different span) must still mint its
    // own DefId — convergence keys on the decl site, not merely the name.
    let mut other = DefinitionInfo::interface(name, Vec::new(), Vec::new());
    other.file_id = Some(7);
    other.span = Some((99, 99));
    other.symbol_id = Some(300);
    let (second, second_minted) = store.register_for_symbol(300, 7, other);

    assert!(second_minted, "a distinct decl site mints its own def");
    assert_ne!(first, second);
    assert!(!store.defs_have_same_decl_site(first, second));
}
