//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/objects/collect.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b61cb9e395a0d23a6e2b97355169d08cf539d28844fb0c0ada2ca73c07dfd994 974 depth_state_enters_new_type_below_limit
    #[test]
    fn depth_state_enters_new_type_below_limit() {
        let stack = [TypeId::STRING, TypeId::NUMBER];

        assert_eq!(
            collect_properties_depth_state(&stack, TypeId::BOOLEAN),
            CollectPropertiesDepthState::Entered
        );
    }
// TSZ_INLINE_TEST_END b61cb9e395a0d23a6e2b97355169d08cf539d28844fb0c0ada2ca73c07dfd994

// TSZ_INLINE_TEST_BEGIN 748d21deb42585e1cf81f88d8d64c39da9614731e0bbabf28b1c0014e15c89fe 984 depth_state_reports_active_position
    #[test]
    fn depth_state_reports_active_position() {
        let stack = [TypeId::NUMBER, TypeId::STRING, TypeId::BOOLEAN];

        assert_eq!(
            collect_properties_depth_state(&stack, TypeId::STRING),
            CollectPropertiesDepthState::AlreadyActive { position: 1 }
        );
    }
// TSZ_INLINE_TEST_END 748d21deb42585e1cf81f88d8d64c39da9614731e0bbabf28b1c0014e15c89fe

// TSZ_INLINE_TEST_BEGIN d78b26dd69fb1afc4a7eb8b097ac69dade4b1b3d46c4f7a88f327cc44b4240da 994 depth_state_reports_stack_limit_before_active_reentry
    #[test]
    fn depth_state_reports_stack_limit_before_active_reentry() {
        let stack = vec![TypeId::STRING; MAX_COLLECT_PROPERTIES_DEPTH];

        assert_eq!(
            collect_properties_depth_state(&stack, TypeId::STRING),
            CollectPropertiesDepthState::StackLimitExceeded
        );
    }
// TSZ_INLINE_TEST_END d78b26dd69fb1afc4a7eb8b097ac69dade4b1b3d46c4f7a88f327cc44b4240da

// TSZ_INLINE_TEST_BEGIN 402447f1189a635de4df83c5b72c9ef0554b500c2db13477c86c9f2e13620012 1004 depth_guard_records_active_reentry_position
    #[test]
    fn depth_guard_records_active_reentry_position() {
        reset_collect_properties_stack();
        COLLECT_PROPERTIES_STACK.with_borrow_mut(|stack| {
            stack.push(TypeId::NUMBER);
            stack.push(TypeId::STRING);
        });

        assert!(matches!(
            CollectPropertiesDepthGuard::enter(TypeId::STRING),
            Err(CollectPropertiesDepthState::AlreadyActive { position: 1 })
        ));
        assert_eq!(
            COLLECT_PROPERTIES_MIN_TRUNCATION.with(std::cell::Cell::get),
            1
        );
        assert_eq!(COLLECT_PROPERTIES_STACK.with_borrow(Vec::len), 2);

        reset_collect_properties_stack();
    }
// TSZ_INLINE_TEST_END 402447f1189a635de4df83c5b72c9ef0554b500c2db13477c86c9f2e13620012

// TSZ_INLINE_TEST_BEGIN 57d6c0a3532c992722e3717c24c89577ed988654f8b85fc1b36e4711e3d09dd6 1025 worklist_state_continues_below_limit
    #[test]
    fn worklist_state_continues_below_limit() {
        assert_eq!(
            collect_properties_worklist_state(MAX_COLLECT_PROPERTIES_DEPTH - 1),
            CollectPropertiesWorklistState::Continue
        );
    }
// TSZ_INLINE_TEST_END 57d6c0a3532c992722e3717c24c89577ed988654f8b85fc1b36e4711e3d09dd6

// TSZ_INLINE_TEST_BEGIN 810f63c39b7dcac6d0af7d447171dc218ebe0e888e68b24b53e2244ebc133636 1033 worklist_state_limits_at_limit
    #[test]
    fn worklist_state_limits_at_limit() {
        assert_eq!(
            collect_properties_worklist_state(MAX_COLLECT_PROPERTIES_DEPTH),
            CollectPropertiesWorklistState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END 810f63c39b7dcac6d0af7d447171dc218ebe0e888e68b24b53e2244ebc133636

// TSZ_INLINE_TEST_BEGIN f7804aab25c31e4881a81257283a620755f958b238e829b6d1316b09cd566c3f 1041 no_query_cache_never_publishes
    #[test]
    fn no_query_cache_never_publishes() {
        let verdict = PropertyCollectionCacheVerdict::from_truncation(false, usize::MAX, 0);

        assert_eq!(verdict, PropertyCollectionCacheVerdict::NoQueryCache);
        assert!(!verdict.should_publish());
    }
// TSZ_INLINE_TEST_END f7804aab25c31e4881a81257283a620755f958b238e829b6d1316b09cd566c3f

// TSZ_INLINE_TEST_BEGIN 8d66ec72657a3cb82adc101fa1cacbce776cbbb8cadceb609ab9cc2f0595a2ef 1049 own_frame_truncation_is_context_free
    #[test]
    fn own_frame_truncation_is_context_free() {
        let verdict = PropertyCollectionCacheVerdict::from_truncation(true, 3, 3);

        assert_eq!(verdict, PropertyCollectionCacheVerdict::ContextFree);
        assert!(verdict.should_publish());
    }
// TSZ_INLINE_TEST_END 8d66ec72657a3cb82adc101fa1cacbce776cbbb8cadceb609ab9cc2f0595a2ef

// TSZ_INLINE_TEST_BEGIN dacb0f1f9081a9137ffd5b75cc96a7f4b74dc081c3341a58d2d16684e3e325e9 1057 outer_ancestor_truncation_blocks_publication
    #[test]
    fn outer_ancestor_truncation_blocks_publication() {
        let verdict = PropertyCollectionCacheVerdict::from_truncation(true, 2, 3);

        assert_eq!(
            verdict,
            PropertyCollectionCacheVerdict::OuterAncestorTruncation
        );
        assert!(!verdict.should_publish());
    }
// TSZ_INLINE_TEST_END dacb0f1f9081a9137ffd5b75cc96a7f4b74dc081c3341a58d2d16684e3e325e9
