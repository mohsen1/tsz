//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-parser/src/parser/node_arena/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 708612bfdc6f1007ac7eca51dd388c26b3f600dde52fa0dbe20da15daabff6d8 286 estimated_size_bytes_is_nonzero_for_empty_arena
    #[test]
    fn estimated_size_bytes_is_nonzero_for_empty_arena() {
        let arena = NodeArena::new();
        let size = arena.estimated_size_bytes();
        // Even an empty arena has struct overhead + vec capacities
        assert!(
            size > 0,
            "estimated_size_bytes should be nonzero for a fresh arena"
        );
    }
// TSZ_INLINE_TEST_END 708612bfdc6f1007ac7eca51dd388c26b3f600dde52fa0dbe20da15daabff6d8

// TSZ_INLINE_TEST_BEGIN 07143af2a9f6d2ba5817e450c8a1657fd440c8595f9800d9a7ebb72ab0eb4655 297 estimated_size_bytes_grows_with_nodes
    #[test]
    fn estimated_size_bytes_grows_with_nodes() {
        let mut arena = NodeArena::new();
        let empty_size = arena.estimated_size_bytes();

        // Add some nodes
        for i in 0..100 {
            arena.add_token(1, i * 10, i * 10 + 5);
        }
        let populated_size = arena.estimated_size_bytes();

        assert!(
            populated_size > empty_size,
            "estimated_size_bytes should grow after adding nodes: {empty_size} -> {populated_size}"
        );
    }
// TSZ_INLINE_TEST_END 07143af2a9f6d2ba5817e450c8a1657fd440c8595f9800d9a7ebb72ab0eb4655

// TSZ_INLINE_TEST_BEGIN d279c536fe09a6a8c7cdba47689b42933e31253147367c5a3411a143192fe52f 314 estimated_size_bytes_accounts_for_interner
    #[test]
    fn estimated_size_bytes_accounts_for_interner() {
        let mut arena = NodeArena::new();
        let before = arena.estimated_size_bytes();

        // Intern many strings
        for i in 0..200 {
            let _ = arena.interner.intern(&format!("identifier_{i}"));
        }
        let after = arena.estimated_size_bytes();

        assert!(
            after > before,
            "estimated_size_bytes should grow with interned strings: {before} -> {after}"
        );
    }
// TSZ_INLINE_TEST_END d279c536fe09a6a8c7cdba47689b42933e31253147367c5a3411a143192fe52f

// TSZ_INLINE_TEST_BEGIN fd4b1f3f45fbb9cdd93d6b9a894d098f95c3bd1ae5a3a3e7a3c99e521e762a0d 331 len_u32_overflow_panics_with_expected_message
    #[test]
    #[should_panic(
        expected = "node arena length exceeds u32::MAX; large AST support requires a larger span type"
    )]
    fn len_u32_overflow_panics_with_expected_message() {
        let arena = NodeArena::new();
        let _ = arena.len_u32(usize::MAX);
    }
// TSZ_INLINE_TEST_END fd4b1f3f45fbb9cdd93d6b9a894d098f95c3bd1ae5a3a3e7a3c99e521e762a0d

// TSZ_INLINE_TEST_BEGIN 7d29e3d1830cac5dfea4ea02ea3b02d08be88e553d890f2a228fd3c48f8f5c0a 347 resolve_identifier_text_falls_back_to_escaped_when_interner_stale
    /// Workstream-7 deliverable 3 ("Add a defensive identifier text
    /// resolution path only if it is consistent with the parser identity
    /// model"): when an `IdentifierData.atom` is set but the arena's
    /// interner returns `""` for it (the stale-interner regression PR #1205
    /// fixed for incremental parse), `resolve_identifier_text` must fall
    /// back to `escaped_text` rather than silently surface the empty
    /// string.
    #[test]
    fn resolve_identifier_text_falls_back_to_escaped_when_interner_stale() {
        let mut arena = NodeArena::new();
        // Use `Interner::new()` so Atom(0) is reserved for the empty
        // string (the production scanner setup); without this, the
        // default-constructed interner gives Atom(0) to the first
        // interned string, which the resolver classifies as Atom::NONE.
        arena.set_interner(Interner::new());
        // Construct an atom that the arena's freshly-created interner does
        // not have — Atom(99_999) is well past any populated index.
        let stale_atom = AstAtom(99_999);
        assert!(arena.interner().resolve(stale_atom).is_empty());

        let data = IdentifierData {
            atom: stale_atom,
            escaped_text: IdentText::from("uniquely_named_identifier"),
            original_text: None,
        };

        assert_eq!(
            arena.resolve_identifier_text(&data),
            "uniquely_named_identifier",
            "stale interner must not produce an empty identifier — fall back to escaped_text"
        );
    }
// TSZ_INLINE_TEST_END 7d29e3d1830cac5dfea4ea02ea3b02d08be88e553d890f2a228fd3c48f8f5c0a

// TSZ_INLINE_TEST_BEGIN 7bee633da4ae56213cf2cb4368cd55b45eaf9344f9108327804e6651575317f5 380 resolve_identifier_text_prefers_escaped_text_when_atom_resolves
    /// Sanity check: parsed identifier text is authoritative for display even
    /// when an atom resolves to a different string.
    ///
    /// Use `Interner::new()` (which reserves Atom(0) for the empty string,
    /// matching the production scanner setup) rather than `Default::default`
    /// so the first interned string gets `Atom(1)`, not `Atom(0)` (which
    /// the resolver classifies as `Atom::NONE`).
    #[test]
    fn resolve_identifier_text_prefers_escaped_text_when_atom_resolves() {
        let mut arena = NodeArena::new();
        arena.set_interner(Interner::new());
        let atom = arena.interner.intern("canonical_text");
        assert_ne!(
            atom,
            AstAtom::NONE,
            "intern result must not be AstAtom::NONE"
        );

        let data = IdentifierData {
            atom,
            // escaped_text intentionally differs from the canonical so we
            // can confirm which branch was taken.
            escaped_text: IdentText::from("stale_escaped_form"),
            original_text: None,
        };

        assert_eq!(arena.resolve_identifier_text(&data), "stale_escaped_form");
    }
// TSZ_INLINE_TEST_END 7bee633da4ae56213cf2cb4368cd55b45eaf9344f9108327804e6651575317f5
