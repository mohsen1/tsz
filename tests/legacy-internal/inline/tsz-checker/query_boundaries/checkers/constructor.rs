//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/checkers/constructor.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 624ae50c81ebc87b5d05de217f7ba1713b6d40e2f41daea1dce700835167b0ad 319 rewrites_self_application_member_to_the_self_application
    #[test]
    fn rewrites_self_application_member_to_the_self_application() {
        let db = TypeInterner::new();
        let own_def = DefId(7001);
        let self_app = db.application(db.lazy(own_def), vec![TypeId::ANY, TypeId::ANY]);
        let provisional_return = db.intersection(vec![self_app, rough_instance(&db, "alpha")]);
        let ctor = ctor_with_construct_return(&db, provisional_return);

        let sanitized_ctor =
            sanitized(&db, ctor, own_def).expect("wrapped provisional return is sanitized");
        assert_eq!(construct_return(&db, sanitized_ctor), self_app);
    }
// TSZ_INLINE_TEST_END 624ae50c81ebc87b5d05de217f7ba1713b6d40e2f41daea1dce700835167b0ad

// TSZ_INLINE_TEST_BEGIN bceea58dbed582333d878b6e526de632209b9a6adc931e001457aa97baf06190 332 ignores_bare_self_lazy_member
    #[test]
    fn ignores_bare_self_lazy_member() {
        // A bare `Lazy(own_def)` member is the value-side self-reference (for
        // class expressions it resolves to `typeof C`, not the instance);
        // rewriting to it would clobber the instance members, so the
        // intersection stays untouched.
        let db = TypeInterner::new();
        let own_def = DefId(7002);
        let self_ref = db.lazy(own_def);
        let provisional_return = db.intersection(vec![self_ref, rough_instance(&db, "beta")]);
        let ctor = ctor_with_construct_return(&db, provisional_return);

        assert!(sanitized(&db, ctor, own_def).is_none());
    }
// TSZ_INLINE_TEST_END bceea58dbed582333d878b6e526de632209b9a6adc931e001457aa97baf06190

// TSZ_INLINE_TEST_BEGIN f280fea46c05a175d68d14aa2fc29e45a661e40eb0f27c4c0a94774e66e263a6 347 ignores_intersection_of_other_definitions
    #[test]
    fn ignores_intersection_of_other_definitions() {
        let db = TypeInterner::new();
        let own_def = DefId(7003);
        let other_def = DefId(7004);
        let other_app = db.application(db.lazy(other_def), vec![TypeId::ANY]);
        let mixed_return = db.intersection(vec![other_app, rough_instance(&db, "gamma")]);
        let ctor = ctor_with_construct_return(&db, mixed_return);

        assert!(sanitized(&db, ctor, own_def).is_none());
    }
// TSZ_INLINE_TEST_END f280fea46c05a175d68d14aa2fc29e45a661e40eb0f27c4c0a94774e66e263a6

// TSZ_INLINE_TEST_BEGIN 8f9dfe3c417a325d6bef4be97d032f4bc6f8a452c7c5de991313282517c47ba6 359 ignores_plain_instance_construct_return
    #[test]
    fn ignores_plain_instance_construct_return() {
        let db = TypeInterner::new();
        let own_def = DefId(7005);
        let instance = db.application(db.lazy(own_def), vec![TypeId::ANY]);
        let ctor = ctor_with_construct_return(&db, instance);

        assert!(sanitized(&db, ctor, own_def).is_none());
    }
// TSZ_INLINE_TEST_END 8f9dfe3c417a325d6bef4be97d032f4bc6f8a452c7c5de991313282517c47ba6

// TSZ_INLINE_TEST_BEGIN 9f9b4d086464f2fef37df45c3f7145a347ece4b4f69d999a3cf17d40bc015514 369 ignores_non_callable_types
    #[test]
    fn ignores_non_callable_types() {
        let db = TypeInterner::new();
        assert!(sanitized(&db, TypeId::OBJECT, DefId(7006)).is_none());
    }
// TSZ_INLINE_TEST_END 9f9b4d086464f2fef37df45c3f7145a347ece4b4f69d999a3cf17d40bc015514
