//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/type_analysis/cross_file_query_types.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN bcbf8046c9444ed03ce4ba436f08b242917f449481a4325f7fae622744b301a9 120 key_implements_required_traits
    /// `CrossFileQueryKey` should be `Hash + PartialEq + Eq + Clone` so it
    /// can be used as a cache map key. Compile-time check via `_test()`
    /// keeps the contract enforced even if a future PR removes a derive.
    #[test]
    fn key_implements_required_traits() {
        fn _test<T: Clone + std::fmt::Debug + std::hash::Hash + Eq>() {}
        _test::<CrossFileQueryKey>();
    }
// TSZ_INLINE_TEST_END bcbf8046c9444ed03ce4ba436f08b242917f449481a4325f7fae622744b301a9

// TSZ_INLINE_TEST_BEGIN c1016a0a53a794a7200beb9c305120601af34f1940808a99385cd9d9f59f13f7 128 key_hash_and_eq_round_trip
    /// Two keys with identical fields must hash and compare equal so
    /// `HashMap<CrossFileQueryKey, _>` lookups round-trip.
    #[test]
    fn key_hash_and_eq_round_trip() {
        let key = CrossFileQueryKey {
            kind: CrossFileQueryKind::Symbol,
            target_file_idx: 7,
            symbol_id: SymbolId(42),
            request_key: None,
            options_fingerprint: 0xDEAD_BEEF,
        };
        let same = key.clone();
        assert_eq!(key, same);
        let mut map: std::collections::HashMap<CrossFileQueryKey, u32> =
            std::collections::HashMap::new();
        map.insert(key, 1);
        assert_eq!(map.get(&same), Some(&1));
    }
// TSZ_INLINE_TEST_END c1016a0a53a794a7200beb9c305120601af34f1940808a99385cd9d9f59f13f7

// TSZ_INLINE_TEST_BEGIN 61ff93c871d88bd5f9abaca870e8fab3ca848be806a5c27e1d70017ce7e9374d 147 answer_variants_constructible
    /// All five answer variants should be constructible. Smoke test that
    /// catches accidental variant removal during refactors.
    #[test]
    fn answer_variants_constructible() {
        let _t: CrossFileQueryAnswer = CrossFileQueryAnswer::Type(tsz_solver::TypeId::ANY);
        let _tp: CrossFileQueryAnswer = CrossFileQueryAnswer::TypeWithParams(
            tsz_solver::TypeId::ANY,
            Vec::<tsz_solver::TypeParamInfo>::new(),
        );
        let _m: CrossFileQueryAnswer = CrossFileQueryAnswer::MemberType {
            member: tsz_common::interner::Atom::default(),
            ty: tsz_solver::TypeId::ANY,
        };
        let _u: CrossFileQueryAnswer = CrossFileQueryAnswer::Unknown;
        let _e: CrossFileQueryAnswer = CrossFileQueryAnswer::Error;
    }
// TSZ_INLINE_TEST_END 61ff93c871d88bd5f9abaca870e8fab3ca848be806a5c27e1d70017ce7e9374d
