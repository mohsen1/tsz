//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/request.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN f883af678f19af8f6a29ac295068bc6dd29e616f8f46a0649b0f092f362497d6 242 default_request_cache_key_disables_no_unchecked_indexed_access
    #[test]
    fn default_request_cache_key_disables_no_unchecked_indexed_access() {
        let request = EvaluationRequest::new(TypeId::STRING);

        assert_eq!(request.type_id(), TypeId::STRING);
        assert_eq!(request.resolver_generation(), 0);
        assert_eq!(request.type_database_identity(), 0);
        assert_eq!(request.resolver_identity(), 0);
        assert!(!request.no_unchecked_indexed_access());
        assert_eq!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::STRING, false, false)
        );
        assert_eq!(request.cache_key().type_id(), TypeId::STRING);
        assert_eq!(request.cache_key().resolver_generation(), 0);
        assert_eq!(request.cache_key().type_database_identity(), 0);
        assert_eq!(request.cache_key().resolver_identity(), 0);
        assert!(!request.cache_key().no_unchecked_indexed_access());
    }
// TSZ_INLINE_TEST_END f883af678f19af8f6a29ac295068bc6dd29e616f8f46a0649b0f092f362497d6

// TSZ_INLINE_TEST_BEGIN 6adfeab4d14716b97dd95a2457604b5fbf979da0cd01a2f348f9452e71ccbe88 262 request_cache_key_tracks_no_unchecked_indexed_access
    #[test]
    fn request_cache_key_tracks_no_unchecked_indexed_access() {
        let request = EvaluationRequest::with_options(
            TypeId::NUMBER,
            EvaluationOptions::new().with_no_unchecked_indexed_access(true),
        );

        assert!(request.no_unchecked_indexed_access());
        assert_eq!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::NUMBER, true, false)
        );
        assert_eq!(
            request.with_type_id(TypeId::BOOLEAN).cache_key(),
            EvaluationCacheKey::new(TypeId::BOOLEAN, true, false)
        );
    }
// TSZ_INLINE_TEST_END 6adfeab4d14716b97dd95a2457604b5fbf979da0cd01a2f348f9452e71ccbe88

// TSZ_INLINE_TEST_BEGIN b677734a20cafe2cc0556997cea9f975e92a0ce3b115f44c14c0619059e3c30c 280 request_cache_key_tracks_exact_optional_property_types
    #[test]
    fn request_cache_key_tracks_exact_optional_property_types() {
        let request = EvaluationRequest::with_options(
            TypeId::NUMBER,
            EvaluationOptions::new().with_exact_optional_property_types(true),
        );

        assert!(request.exact_optional_property_types());
        assert!(!request.no_unchecked_indexed_access());
        assert!(request.cache_key().exact_optional_property_types());
        assert!(!request.cache_key().no_unchecked_indexed_access());
        assert_eq!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::NUMBER, false, true)
        );
        // The two option flags are independent discriminants: flipping one must
        // not collide with the other being set.
        assert_ne!(
            EvaluationCacheKey::new(TypeId::NUMBER, true, false),
            EvaluationCacheKey::new(TypeId::NUMBER, false, true)
        );
    }
// TSZ_INLINE_TEST_END b677734a20cafe2cc0556997cea9f975e92a0ce3b115f44c14c0619059e3c30c

// TSZ_INLINE_TEST_BEGIN 9ea50aec357e200f2df1f0addc895ef41d320317b5250be661ac9c68fd89fb62 303 request_cache_key_tracks_resolver_generation
    #[test]
    fn request_cache_key_tracks_resolver_generation() {
        let request = EvaluationRequest::new(TypeId::STRING).with_resolver_generation(7);

        assert_eq!(request.resolver_generation(), 7);
        assert_eq!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::STRING, false, false).with_resolver_generation(7)
        );
        assert_ne!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::STRING, false, false)
        );
        assert_eq!(
            request.with_type_id(TypeId::NUMBER).cache_key(),
            EvaluationCacheKey::new(TypeId::NUMBER, false, false).with_resolver_generation(7)
        );
    }
// TSZ_INLINE_TEST_END 9ea50aec357e200f2df1f0addc895ef41d320317b5250be661ac9c68fd89fb62

// TSZ_INLINE_TEST_BEGIN 45455b3c04d5379383b89f80206d405a95ec3b67978bfa8cd00c6d7cedd4a0df 322 request_cache_key_tracks_arena_and_resolver_identity
    #[test]
    fn request_cache_key_tracks_arena_and_resolver_identity() {
        let request = EvaluationRequest::new(TypeId::STRING)
            .with_type_database_identity(11)
            .with_resolver_identity(22)
            .with_resolver_generation(7);

        assert_eq!(request.type_database_identity(), 11);
        assert_eq!(request.resolver_identity(), 22);
        assert_eq!(request.cache_key().type_database_identity(), 11);
        assert_eq!(request.cache_key().resolver_identity(), 22);
        assert_eq!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::STRING, false, false)
                .with_type_database_identity(11)
                .with_resolver_identity(22)
                .with_resolver_generation(7)
        );
        assert_ne!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::STRING, false, false)
                .with_type_database_identity(12)
                .with_resolver_identity(22)
                .with_resolver_generation(7)
        );
        assert_ne!(
            request.cache_key(),
            EvaluationCacheKey::new(TypeId::STRING, false, false)
                .with_type_database_identity(11)
                .with_resolver_identity(23)
                .with_resolver_generation(7)
        );
    }
// TSZ_INLINE_TEST_END 45455b3c04d5379383b89f80206d405a95ec3b67978bfa8cd00c6d7cedd4a0df

// TSZ_INLINE_TEST_BEGIN 8ce792dc1c8addc33a2aa16658447adc1a4de64884fd4463199318c870224cf4 356 request_routes_no_unchecked_indexed_access_option
    #[test]
    fn request_routes_no_unchecked_indexed_access_option() {
        let interner = TypeInterner::new();
        let array = interner.array(TypeId::STRING);
        let indexed = interner.index_access(array, TypeId::NUMBER);

        let default_result = evaluate_type_with_request(&interner, EvaluationRequest::new(indexed));
        assert_eq!(default_result, TypeId::STRING);

        let no_unchecked_result = evaluate_type_with_request(
            &interner,
            EvaluationRequest::new(indexed).with_no_unchecked_indexed_access(true),
        );
        let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
        assert_eq!(no_unchecked_result, expected);
    }
// TSZ_INLINE_TEST_END 8ce792dc1c8addc33a2aa16658447adc1a4de64884fd4463199318c870224cf4

// TSZ_INLINE_TEST_BEGIN 30450cb0faa4debb7207ee1aaef9fa124abad920ee6f8a0438c00635c9f74e1d 373 request_routes_exact_optional_property_types_option
    #[test]
    fn request_routes_exact_optional_property_types_option() {
        let interner = TypeInterner::new();
        let prop = interner.intern_string("value");
        let key_name = interner.intern_string("K");
        let key_param_info = TypeParamInfo {
            name: key_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::User,
        };
        let key_param = interner.type_param(key_param_info);
        let number_or_undefined = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
        let source = interner.object(vec![PropertyInfo::opt(prop, number_or_undefined)]);
        let mapped = interner.mapped(MappedType {
            type_param: key_param_info,
            constraint: interner.keyof(source),
            name_type: None,
            template: interner.index_access(source, key_param),
            optional_modifier: Some(MappedModifier::Remove),
            readonly_modifier: None,
        });

        let legacy_result = evaluate_type_with_request(&interner, EvaluationRequest::new(mapped));
        let exact_result = evaluate_type_with_request(
            &interner,
            EvaluationRequest::new(mapped).with_exact_optional_property_types(true),
        );

        assert_eq!(
            mapped_property_type(&interner, legacy_result, prop),
            TypeId::NUMBER,
            "legacy optional mode strips top-level undefined when -? removes optionality"
        );
        assert_eq!(
            mapped_property_type(&interner, exact_result, prop),
            number_or_undefined,
            "exact optional mode preserves explicit undefined under -?"
        );
    }
// TSZ_INLINE_TEST_END 30450cb0faa4debb7207ee1aaef9fa124abad920ee6f8a0438c00635c9f74e1d
