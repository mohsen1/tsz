//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/type_predicates.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2f48a932c6f57ae585662528a09cc6a6bf71dba1134250d9507e18943fa57826 299 top_level_error_or_error_union_member_detects_error_shapes
    #[test]
    fn top_level_error_or_error_union_member_detects_error_shapes() {
        let db = TypeInterner::new();
        let error_union = db.union(vec![TypeId::STRING, TypeId::ERROR]);
        let non_error_union = db.union(vec![TypeId::STRING, TypeId::NUMBER]);

        assert!(is_top_level_error_or_error_union_member(&db, TypeId::ERROR));
        assert!(is_top_level_error_or_error_union_member(&db, error_union));
        assert!(!is_top_level_error_or_error_union_member(
            &db,
            TypeId::STRING
        ));
        assert!(!is_top_level_error_or_error_union_member(
            &db,
            non_error_union
        ));
    }
// TSZ_INLINE_TEST_END 2f48a932c6f57ae585662528a09cc6a6bf71dba1134250d9507e18943fa57826

// TSZ_INLINE_TEST_BEGIN 723d75f2a36cd1ce027d5c2f48ffd009b4b5f00d14877719c361cb8d693f6b6f 317 present_primitive_index_keys_detects_direct_primitive_keys
    #[test]
    fn present_primitive_index_keys_detects_direct_primitive_keys() {
        let db = TypeInterner::new();
        assert_eq!(
            present_primitive_index_keys(&db, &[TypeId::STRING]),
            vec![TypeId::STRING]
        );
        assert_eq!(
            present_primitive_index_keys(&db, &[TypeId::NUMBER]),
            vec![TypeId::NUMBER]
        );
        assert_eq!(
            present_primitive_index_keys(&db, &[TypeId::SYMBOL]),
            vec![TypeId::SYMBOL]
        );
    }
// TSZ_INLINE_TEST_END 723d75f2a36cd1ce027d5c2f48ffd009b4b5f00d14877719c361cb8d693f6b6f

// TSZ_INLINE_TEST_BEGIN f781221777fecbda0bd3940ef76c85ef0150709941da5bfc2019fd477639256e 334 present_primitive_index_keys_is_per_key_not_per_base
    #[test]
    fn present_primitive_index_keys_is_per_key_not_per_base() {
        // The core regression guard: `string | number` admits string and number
        // but must NOT admit symbol. A rendered-string match on the *base*
        // ("string | number") would have falsely admitted symbol.
        let db = TypeInterner::new();
        let string_number = db.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let present = present_primitive_index_keys(&db, &[string_number]);
        assert!(present.contains(&TypeId::STRING));
        assert!(present.contains(&TypeId::NUMBER));
        assert!(!present.contains(&TypeId::SYMBOL));
    }
// TSZ_INLINE_TEST_END f781221777fecbda0bd3940ef76c85ef0150709941da5bfc2019fd477639256e

// TSZ_INLINE_TEST_BEGIN 79ce9f8caabc688791a2284fd1ec86eb841a3c7af9bcff4308b0e2e08bd89dfa 347 present_primitive_index_keys_is_independent_of_union_spelling
    #[test]
    fn present_primitive_index_keys_is_independent_of_union_spelling() {
        // Order/spelling of the union members must not change the structural
        // answer (no rendered-text dependence).
        let db = TypeInterner::new();
        let forward = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);
        let reversed = db.union(vec![TypeId::SYMBOL, TypeId::NUMBER, TypeId::STRING]);
        let mut a = present_primitive_index_keys(&db, &[forward]);
        let mut b = present_primitive_index_keys(&db, &[reversed]);
        a.sort_by_key(|t| t.0);
        b.sort_by_key(|t| t.0);
        assert_eq!(a, b);
        assert_eq!(a.len(), 3);
    }
// TSZ_INLINE_TEST_END 79ce9f8caabc688791a2284fd1ec86eb841a3c7af9bcff4308b0e2e08bd89dfa

// TSZ_INLINE_TEST_BEGIN 6018ca76de0d997d26f8557e482b697e5c47278002450d2e9907cc18050a5b27 362 present_primitive_index_keys_ignores_non_primitive_members
    #[test]
    fn present_primitive_index_keys_ignores_non_primitive_members() {
        let db = TypeInterner::new();
        let mixed = db.union(vec![TypeId::STRING, TypeId::BOOLEAN, TypeId::OBJECT]);
        assert_eq!(
            present_primitive_index_keys(&db, &[mixed]),
            vec![TypeId::STRING]
        );

        let non_primitive = db.union(vec![TypeId::BOOLEAN, TypeId::OBJECT]);
        assert!(present_primitive_index_keys(&db, &[non_primitive]).is_empty());
        assert!(present_primitive_index_keys(&db, &[TypeId::BOOLEAN]).is_empty());
    }
// TSZ_INLINE_TEST_END 6018ca76de0d997d26f8557e482b697e5c47278002450d2e9907cc18050a5b27

// TSZ_INLINE_TEST_BEGIN 0324b41bf36c9f3f226bc0843c125ec999f4042ee8f4e5da4444c16820425b6a 376 present_primitive_index_keys_recovers_key_from_any_form
    #[test]
    fn present_primitive_index_keys_recovers_key_from_any_form() {
        // A primitive present only in a later (e.g. evaluated) form is still
        // recognized — this is the raw + evaluated base coverage callers rely on.
        let db = TypeInterner::new();
        let evaluated = db.union(vec![TypeId::NUMBER, TypeId::SYMBOL]);
        let present = present_primitive_index_keys(&db, &[TypeId::BOOLEAN, evaluated]);
        assert!(present.contains(&TypeId::NUMBER));
        assert!(present.contains(&TypeId::SYMBOL));
        assert!(!present.contains(&TypeId::STRING));
    }
// TSZ_INLINE_TEST_END 0324b41bf36c9f3f226bc0843c125ec999f4042ee8f4e5da4444c16820425b6a

// TSZ_INLINE_TEST_BEGIN 7de93d6336c0566c058a5db1d0f1081cdb709717de124ff7c6c8e5e86b6ee91d 388 base_admits_any_primitive_index_key_matches_presence
    #[test]
    fn base_admits_any_primitive_index_key_matches_presence() {
        let db = TypeInterner::new();
        let string_number = db.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let non_primitive = db.union(vec![TypeId::BOOLEAN, TypeId::OBJECT]);

        assert!(base_admits_any_primitive_index_key(&db, &[string_number]));
        assert!(base_admits_any_primitive_index_key(&db, &[TypeId::SYMBOL]));
        assert!(!base_admits_any_primitive_index_key(&db, &[non_primitive]));
        assert!(!base_admits_any_primitive_index_key(
            &db,
            &[TypeId::BOOLEAN]
        ));
    }
// TSZ_INLINE_TEST_END 7de93d6336c0566c058a5db1d0f1081cdb709717de124ff7c6c8e5e86b6ee91d
