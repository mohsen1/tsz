//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/narrowing/request.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6ae8ba213a2beb7505f340d48e7db34de2475ec9f6f5ecd3c3391d63d3d0c8df 121 narrowing_options_default_flags_are_clear
    #[test]
    fn narrowing_options_default_flags_are_clear() {
        let opts = NarrowingOptions::new();
        assert!(!opts.no_unchecked_indexed_access());
        assert!(!opts.exact_optional_property_types());
    }
// TSZ_INLINE_TEST_END 6ae8ba213a2beb7505f340d48e7db34de2475ec9f6f5ecd3c3391d63d3d0c8df

// TSZ_INLINE_TEST_BEGIN b1e7261a27518835343286ce3fb5957e2f66ffbaf9af714ac65af333d21b8174 128 narrowing_options_no_unchecked_flag_is_independent
    #[test]
    fn narrowing_options_no_unchecked_flag_is_independent() {
        let opts_on = NarrowingOptions::new().with_no_unchecked_indexed_access(true);
        let opts_off = NarrowingOptions::new();
        assert_ne!(
            opts_on, opts_off,
            "no_unchecked_indexed_access flag must distinguish options"
        );
        assert!(!opts_on.exact_optional_property_types());
    }
// TSZ_INLINE_TEST_END b1e7261a27518835343286ce3fb5957e2f66ffbaf9af714ac65af333d21b8174

// TSZ_INLINE_TEST_BEGIN 1d7a91719e26ded9a4a5fe6a2bfc912fe4978c34d2b2c38d3e75f12cd28788ad 139 narrowing_options_exact_optional_flag_is_independent
    #[test]
    fn narrowing_options_exact_optional_flag_is_independent() {
        let opts_on = NarrowingOptions::new().with_exact_optional_property_types(true);
        let opts_off = NarrowingOptions::new();
        assert_ne!(
            opts_on, opts_off,
            "exact_optional_property_types flag must distinguish options"
        );
        assert!(!opts_on.no_unchecked_indexed_access());
    }
// TSZ_INLINE_TEST_END 1d7a91719e26ded9a4a5fe6a2bfc912fe4978c34d2b2c38d3e75f12cd28788ad

// TSZ_INLINE_TEST_BEGIN 039bc5d454234e46f8685c955b0a66e00d789e2327092b443061a2d5ea57df8a 150 narrowing_options_both_flags_independent_of_each_other
    #[test]
    fn narrowing_options_both_flags_independent_of_each_other() {
        let only_unchecked = NarrowingOptions::new().with_no_unchecked_indexed_access(true);
        let only_exact = NarrowingOptions::new().with_exact_optional_property_types(true);
        let both = NarrowingOptions::new()
            .with_no_unchecked_indexed_access(true)
            .with_exact_optional_property_types(true);
        assert_ne!(only_unchecked, only_exact);
        assert_ne!(only_unchecked, both);
        assert_ne!(only_exact, both);
    }
// TSZ_INLINE_TEST_END 039bc5d454234e46f8685c955b0a66e00d789e2327092b443061a2d5ea57df8a

// TSZ_INLINE_TEST_BEGIN f45574df9151ef80ebf583f06871f893ba13892e1039b0d161705e64c617b29e 162 narrowing_request_stable_cache_key_omits_resolver_generation
    #[test]
    fn narrowing_request_stable_cache_key_omits_resolver_generation() {
        let guard = TypeGuard::Typeof(TypeofKind::String);
        let req = NarrowingRequest::new(TypeId::NUMBER, guard, GuardSense::Positive);
        let opts = NarrowingOptions::new();
        let key0 = req.stable_cache_key(opts);
        let key1 = req.stable_cache_key(opts);
        assert_eq!(key0, key1, "resolver generation is a memo stamp now");
    }
// TSZ_INLINE_TEST_END f45574df9151ef80ebf583f06871f893ba13892e1039b0d161705e64c617b29e

// TSZ_INLINE_TEST_BEGIN 5b26f8a382a8b9fdd26414f9b340f08fadee3001f53acff02294b9d890f0dda0 172 narrowing_request_cache_key_reflects_options
    #[test]
    fn narrowing_request_cache_key_reflects_options() {
        let guard = TypeGuard::Typeof(TypeofKind::Number);
        let req = NarrowingRequest::new(TypeId::ANY, guard, GuardSense::Negative);
        let opts_default = NarrowingOptions::new();
        let opts_unchecked = NarrowingOptions::new().with_no_unchecked_indexed_access(true);
        let key_default = req.stable_cache_key(opts_default);
        let key_unchecked = req.stable_cache_key(opts_unchecked);
        assert_ne!(
            key_default, key_unchecked,
            "different options must produce different cache keys"
        );
    }
// TSZ_INLINE_TEST_END 5b26f8a382a8b9fdd26414f9b340f08fadee3001f53acff02294b9d890f0dda0

// TSZ_INLINE_TEST_BEGIN 75dc94dbe641b970f911560bf96731ad6079bfe6d5b00809bdbbc11709d54119 186 narrowing_request_same_inputs_produce_equal_cache_keys
    #[test]
    fn narrowing_request_same_inputs_produce_equal_cache_keys() {
        let make_req =
            || NarrowingRequest::new(TypeId::STRING, TypeGuard::Truthy, GuardSense::Positive);
        let opts = NarrowingOptions::new();
        let k1 = make_req().stable_cache_key(opts);
        let k2 = make_req().stable_cache_key(opts);
        assert_eq!(k1, k2, "equal inputs must produce equal cache keys");
    }
// TSZ_INLINE_TEST_END 75dc94dbe641b970f911560bf96731ad6079bfe6d5b00809bdbbc11709d54119
