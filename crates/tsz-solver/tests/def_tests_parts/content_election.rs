/// #14344 Stage 5 (PR2 election): the content-election pass converges
/// cross-arena variants of ONE declaration onto a single representative, which
/// is exactly what makes `canonical_def_id` agree across arenas. Models the
/// #13862 shape: two per-file binders each minted a `DefId` for the same
/// `interface Shared` declared in one module (different arena `file_ids` / `DefId`
/// values), one carrying the heritage-complete body, the other a body-less
/// cross-arena alias. After election, BOTH `canonical_def_id` to the body-
/// bearing representative.
///
/// Uses the flag-independent `elect_content_representatives_unchecked` so the
/// convergence is verified deterministically without depending on the
/// process-wide `TSZ_CANONICAL_DEFID` `OnceLock`. The public
/// `elect_content_representatives` is a one-line gate that delegates here when
/// the flag is on and returns 0 (no-op, byte-identical) when off.
#[test]
fn content_election_converges_cross_arena_variants() {
    let interner = create_test_interner();
    let store = DefinitionStore::new();

    // One declaring module, file_idx 7, recorded canonical path.
    let decl_path = interner.intern_string("/proj/dom.d.ts");
    store.set_file_canonical_path(7, decl_path);

    // Variant A: the declaring-side def WITH the heritage-complete body.
    let mut a_info = DefinitionInfo::interface(interner.intern_string("Shared"), vec![], vec![]);
    a_info.file_id = Some(7);
    a_info.span = Some((100, 100));
    let a = store.register(a_info);
    store.set_body(a, TypeId::STRING);

    // Variant B: a cross-arena alias-side def, SAME content key, body-less, a
    // different later-discovered raw `DefId`, but its declaring identity is the
    // same module+span (the election key resolves to file_idx 7).
    let mut b_info = DefinitionInfo::interface(interner.intern_string("Shared"), vec![], vec![]);
    b_info.file_id = Some(7);
    b_info.span = Some((100, 100));
    let b = store.register(b_info);

    // Before election: independently minted, neither forwards to the other.
    assert_eq!(store.canonical_def_id(a), a);
    assert_eq!(store.canonical_def_id(b), b);

    let links = store.elect_content_representatives_unchecked();
    assert!(links >= 1, "election must add at least one forward link");

    // Both variants now canonicalize to the SAME representative: the
    // body-bearing one (A), regardless of which raw `DefId` is numerically
    // smaller.
    let rep = store.canonical_def_id(a);
    assert_eq!(
        store.canonical_def_id(b),
        rep,
        "cross-arena variants must converge to one representative"
    );
    assert_eq!(
        rep, a,
        "the body-bearing variant must be elected representative"
    );
    assert_eq!(store.get_body(rep), Some(TypeId::STRING));

    // Idempotent: re-running adds no new links.
    assert_eq!(store.elect_content_representatives_unchecked(), 0);

    // A def in a DIFFERENT module (different canonical path) is NOT grouped with
    // the above, even with the same name/kind/span.
    let other_path = interner.intern_string("/proj/other.ts");
    store.set_file_canonical_path(9, other_path);
    let mut c_info = DefinitionInfo::interface(interner.intern_string("Shared"), vec![], vec![]);
    c_info.file_id = Some(9);
    c_info.span = Some((100, 100));
    let c = store.register(c_info);
    store.elect_content_representatives_unchecked();
    assert_eq!(
        store.canonical_def_id(c),
        c,
        "a same-named decl in a different declaring module must NOT converge"
    );
}
