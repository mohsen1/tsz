//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/visitors/visitor_extract.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 869b166b261338c0124799b954b8a15309f11804981376b361507bc90214df54 971 application_base_error_walk_allows_below_cap
    #[test]
    fn application_base_error_walk_allows_below_cap() {
        assert_eq!(
            application_base_error_walk_state(MAX_ERROR_APPLICATION_BASE_DEPTH - 1),
            ApplicationBaseErrorWalkState::Continue
        );
    }
// TSZ_INLINE_TEST_END 869b166b261338c0124799b954b8a15309f11804981376b361507bc90214df54

// TSZ_INLINE_TEST_BEGIN f13964f4e28d3dd9ad7899a19d1b9d0d527dcdb3ce3f6de13cf25b6433d6b5b4 979 application_base_error_walk_limits_at_cap
    #[test]
    fn application_base_error_walk_limits_at_cap() {
        assert_eq!(
            application_base_error_walk_state(MAX_ERROR_APPLICATION_BASE_DEPTH),
            ApplicationBaseErrorWalkState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END f13964f4e28d3dd9ad7899a19d1b9d0d527dcdb3ce3f6de13cf25b6433d6b5b4

// TSZ_INLINE_TEST_BEGIN 1c3ccf0a670c5bb25f491c99cf3d30223ab42d9e17c9663d1ad44fab66ba28a5 987 error_base_before_cap_is_detected
    #[test]
    fn error_base_before_cap_is_detected() {
        let interner = TypeInterner::new();
        let ty = application_base_chain(
            &interner,
            TypeId::ERROR,
            usize::from(MAX_ERROR_APPLICATION_BASE_DEPTH - 1),
        );

        assert!(is_error_type(&interner, ty));
    }
// TSZ_INLINE_TEST_END 1c3ccf0a670c5bb25f491c99cf3d30223ab42d9e17c9663d1ad44fab66ba28a5

// TSZ_INLINE_TEST_BEGIN 1c3e0f9c8bb2a0e5489a75e770be83e09e699dd5668d04f5350c14a283b75520 999 error_base_at_cap_preserves_false_fallback
    #[test]
    fn error_base_at_cap_preserves_false_fallback() {
        let interner = TypeInterner::new();
        let ty = application_base_chain(
            &interner,
            TypeId::ERROR,
            usize::from(MAX_ERROR_APPLICATION_BASE_DEPTH),
        );

        assert!(!is_error_type(&interner, ty));
    }
// TSZ_INLINE_TEST_END 1c3e0f9c8bb2a0e5489a75e770be83e09e699dd5668d04f5350c14a283b75520

// TSZ_INLINE_TEST_BEGIN ebf913dca624b0d1975426f42a7f62b6bae3d25932b73bbebeb79691b81261ef 1266 standalone_unique_symbol_widens_to_symbol
    #[test]
    fn standalone_unique_symbol_widens_to_symbol() {
        // A standalone `unique symbol` value surface prints as `symbol` in .d.ts.
        let interner = TypeInterner::new();
        let sym = interner.unique_symbol(SymbolRef(42));
        let widened = widen_unique_symbol_value_type_for_dts(&interner, sym);
        assert_eq!(widened, TypeId::SYMBOL);
    }
// TSZ_INLINE_TEST_END ebf913dca624b0d1975426f42a7f62b6bae3d25932b73bbebeb79691b81261ef

// TSZ_INLINE_TEST_BEGIN dbf51b48f6ab02392d25232f5a0d64d480423744adb3853c5987e25b71f255d0 1275 two_distinct_unique_symbols_in_union_are_preserved
    #[test]
    fn two_distinct_unique_symbols_in_union_are_preserved() {
        // `typeof x | typeof y` over two distinct unique symbols must stay a
        // 2-member union; widening each member to `symbol` would dedupe them
        // into a single `symbol`, dropping the declared identities. This is the
        // `indirectUniqueSymbolDeclarationEmit` witness shape.
        let interner = TypeInterner::new();
        let sym_a = interner.unique_symbol(SymbolRef(1));
        let sym_b = interner.unique_symbol(SymbolRef(2));
        let union = interner.union(vec![sym_a, sym_b]);
        let widened = widen_unique_symbol_value_type_for_dts(&interner, union);
        assert_eq!(widened, union, "2-member unique-symbol union preserved");
        match interner.lookup(widened) {
            Some(TypeData::Union(list_id)) => {
                let members = interner.type_list(list_id);
                assert_eq!(members.len(), 2);
                assert!(members.contains(&sym_a));
                assert!(members.contains(&sym_b));
            }
            other => panic!("expected a 2-member union, got {other:?}"),
        }
    }
// TSZ_INLINE_TEST_END dbf51b48f6ab02392d25232f5a0d64d480423744adb3853c5987e25b71f255d0

// TSZ_INLINE_TEST_BEGIN 4744152c5c4e3e66731cd9f3ebf555d5e2d6b7e312c783b0ebb1b79348ae3ff5 1298 three_distinct_unique_symbols_in_union_are_preserved
    #[test]
    fn three_distinct_unique_symbols_in_union_are_preserved() {
        let interner = TypeInterner::new();
        let sym_a = interner.unique_symbol(SymbolRef(10));
        let sym_b = interner.unique_symbol(SymbolRef(20));
        let sym_c = interner.unique_symbol(SymbolRef(30));
        let union = interner.union(vec![sym_a, sym_b, sym_c]);
        let widened = widen_unique_symbol_value_type_for_dts(&interner, union);
        assert_eq!(widened, union);
        match interner.lookup(widened) {
            Some(TypeData::Union(list_id)) => {
                assert_eq!(interner.type_list(list_id).len(), 3);
            }
            other => panic!("expected a 3-member union, got {other:?}"),
        }
    }
// TSZ_INLINE_TEST_END 4744152c5c4e3e66731cd9f3ebf555d5e2d6b7e312c783b0ebb1b79348ae3ff5

// TSZ_INLINE_TEST_BEGIN 64fe3b7d6f3e0c7359a35e969d2df0822ed5e6fe3a9c09a273bc90b3fa186e7c 1315 single_distinct_unique_symbol_union_widens_to_symbol
    #[test]
    fn single_distinct_unique_symbol_union_widens_to_symbol() {
        // A union that contains only one distinct unique symbol (paired with a
        // passthrough non-unique-symbol member) must still widen that
        // unique-symbol member — the guard only triggers for >= 2 distinct ones.
        let interner = TypeInterner::new();
        let sym = interner.unique_symbol(SymbolRef(7));
        let union = interner.union(vec![sym, TypeId::STRING]);
        let widened = widen_unique_symbol_value_type_for_dts(&interner, union);
        match interner.lookup(widened) {
            Some(TypeData::Union(list_id)) => {
                let members = interner.type_list(list_id);
                assert!(
                    members.contains(&TypeId::SYMBOL),
                    "lone unique symbol widened to symbol"
                );
                assert!(members.contains(&TypeId::STRING));
                assert!(
                    !members.contains(&sym),
                    "the unique-symbol member must not survive"
                );
            }
            other => panic!("expected a widened union, got {other:?}"),
        }
    }
// TSZ_INLINE_TEST_END 64fe3b7d6f3e0c7359a35e969d2df0822ed5e6fe3a9c09a273bc90b3fa186e7c

// TSZ_INLINE_TEST_BEGIN fdfa6945a3f20831f4d3a8c75da893f0befbe5d5ab0c2a11f2ebecc0755f3cea 1341 distinct_unique_symbol_union_independent_of_symbolref_value
    #[test]
    fn distinct_unique_symbol_union_independent_of_symbolref_value() {
        // Renamed-binding equivalence: the guard keys on distinct `SymbolRef`s,
        // not on any particular id value or user-chosen name.
        let interner = TypeInterner::new();
        let sym_a = interner.unique_symbol(SymbolRef(999));
        let sym_b = interner.unique_symbol(SymbolRef(1000));
        let union = interner.union(vec![sym_a, sym_b]);
        assert_eq!(
            widen_unique_symbol_value_type_for_dts(&interner, union),
            union
        );
    }
// TSZ_INLINE_TEST_END fdfa6945a3f20831f4d3a8c75da893f0befbe5d5ab0c2a11f2ebecc0755f3cea
