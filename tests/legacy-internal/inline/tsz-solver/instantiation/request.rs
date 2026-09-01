//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/instantiation/request.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b29c533263df6b3f622da0e91aa612e0ba7686522ff4993acd91d249119558d7 160 default_options_mode_bits_are_zero
    #[test]
    fn default_options_mode_bits_are_zero() {
        let options = InstantiationOptions::new();
        assert_eq!(options.mode_bits(), 0);
        assert!(!options.substitute_infer());
        assert!(!options.preserve_meta_types());
        assert!(!options.preserve_unsubstituted_type_params());
        assert!(!options.shallow_this_only());
    }
// TSZ_INLINE_TEST_END b29c533263df6b3f622da0e91aa612e0ba7686522ff4993acd91d249119558d7

// TSZ_INLINE_TEST_BEGIN 3e2c043b4db7bd53a1254afe0bb1d5cdd3e8656ca6d01b39c344736f22edaac6 170 unstamped_request_key_keeps_the_identity_domain_pointer_empty
    #[test]
    fn unstamped_request_key_keeps_the_identity_domain_pointer_empty() {
        let interner = TypeInterner::new();
        let name = interner.intern_string("T");
        let substitution = TypeSubstitution::from_signature_args(
            &interner,
            &[TypeParamInfo::simple(name)],
            &[TypeId::STRING],
        );
        let key = InstantiationRequest::new(TypeId::OBJECT, &substitution).cache_key();

        assert!(key.identity_domain.is_none());
        assert_eq!(
            std::mem::size_of_val(&key.identity_domain),
            std::mem::size_of::<usize>(),
            "the common key pays one nullable pointer and no domain allocation"
        );
    }
// TSZ_INLINE_TEST_END 3e2c043b4db7bd53a1254afe0bb1d5cdd3e8656ca6d01b39c344736f22edaac6

// TSZ_INLINE_TEST_BEGIN b9a3a9afea4a59367f7e0e537fa6212f6c9c8c1252ef3c25cbc8cdbe6639dd08 189 mode_bits_match_legacy_constants
    #[test]
    fn mode_bits_match_legacy_constants() {
        // These values must stay in sync with the private MODE_* constants in
        // `instantiate.rs`. If either side moves, the cache key shape changes
        // and cross-version entries would alias incorrectly.
        assert_eq!(
            InstantiationOptions::new()
                .with_substitute_infer(true)
                .mode_bits(),
            0b0001
        );
        assert_eq!(
            InstantiationOptions::new()
                .with_preserve_meta_types(true)
                .mode_bits(),
            0b0010
        );
        assert_eq!(
            InstantiationOptions::new()
                .with_preserve_unsubstituted_type_params(true)
                .mode_bits(),
            0b0100
        );
        assert_eq!(
            InstantiationOptions::new()
                .with_shallow_this_only(true)
                .mode_bits(),
            0b1000
        );
    }
// TSZ_INLINE_TEST_END b9a3a9afea4a59367f7e0e537fa6212f6c9c8c1252ef3c25cbc8cdbe6639dd08

// TSZ_INLINE_TEST_BEGIN a76712cb6e52f51f3fee193a9b40b417a427dbf489d819a3d0118f076b1effc9 220 combined_options_pack_into_one_byte
    #[test]
    fn combined_options_pack_into_one_byte() {
        let options = InstantiationOptions::new()
            .with_preserve_unsubstituted_type_params(true)
            .with_shallow_this_only(true);
        assert_eq!(options.mode_bits(), 0b1100);
        assert!(options.preserve_unsubstituted_type_params());
        assert!(options.shallow_this_only());
        assert!(!options.substitute_infer());
        assert!(!options.preserve_meta_types());
    }
// TSZ_INLINE_TEST_END a76712cb6e52f51f3fee193a9b40b417a427dbf489d819a3d0118f076b1effc9

// TSZ_INLINE_TEST_BEGIN cb94da430ebd13de754fa7135a2bb302d9930892d937ecca44c0cee15aa7e947 232 default_request_cache_key_is_empty_substitution
    #[test]
    fn default_request_cache_key_is_empty_substitution() {
        let subst = TypeSubstitution::new();
        let request = InstantiationRequest::new(TypeId::STRING, &subst);
        let expected = InstantiationCacheKey::new(TypeId::STRING, CanonicalSubst::empty(), 0, None);
        assert_eq!(request.cache_key(), expected);
        assert_eq!(request.type_id(), TypeId::STRING);
        assert!(request.this_type().is_none());
    }
// TSZ_INLINE_TEST_END cb94da430ebd13de754fa7135a2bb302d9930892d937ecca44c0cee15aa7e947

// TSZ_INLINE_TEST_BEGIN 5dc51cd0895f665f294fa783d3c15879d19fbe8da1cad5fb76790f2d19a5dab5 242 request_cache_key_includes_options_and_this_type
    #[test]
    fn request_cache_key_includes_options_and_this_type() {
        let subst = TypeSubstitution::new();
        let options = InstantiationOptions::new()
            .with_preserve_unsubstituted_type_params(true)
            .with_shallow_this_only(true);
        let request = InstantiationRequest::new(TypeId::STRING, &subst)
            .with_options(options)
            .with_this_type(TypeId::NUMBER);
        let key = request.cache_key();
        assert_eq!(key.type_id, TypeId::STRING);
        assert_eq!(key.mode_bits, 0b1100);
        assert_eq!(key.this_type, Some(TypeId::NUMBER));
        assert!(key.subst.is_empty());
    }
// TSZ_INLINE_TEST_END 5dc51cd0895f665f294fa783d3c15879d19fbe8da1cad5fb76790f2d19a5dab5

// TSZ_INLINE_TEST_BEGIN 94069e042a278d50619bacc603f0c5762f86f731bf5466f4aa1f7d7d903728a5 258 request_engine_substitutes_type_parameter
    #[test]
    fn request_engine_substitutes_type_parameter() {
        // The staged request boundary must produce the same `TypeId` as the
        // legacy `instantiate_type` entry for an ordinary substitution.
        let interner = TypeInterner::new();
        let t_name = interner.intern_string("T");
        let t_param = interner.type_param(TypeParamInfo {
            is_const: false,
            name: t_name,
            constraint: None,
            default: None,
            origin: crate::types::TypeParamOrigin::User,
        });
        let array_of_t = interner.array(t_param);

        let mut subst = TypeSubstitution::new();
        subst.insert(t_name, TypeId::NUMBER);

        let legacy = instantiate_type(&interner, array_of_t, &subst);
        let staged =
            instantiate_type_with_request(&interner, InstantiationRequest::new(array_of_t, &subst));
        assert!(!staged.depth_exceeded());
        assert_eq!(staged.type_id(), legacy);
        assert_eq!(legacy, interner.array(TypeId::NUMBER));
    }
// TSZ_INLINE_TEST_END 94069e042a278d50619bacc603f0c5762f86f731bf5466f4aa1f7d7d903728a5
