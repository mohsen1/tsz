//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/recovery/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 73d57b6d5cfe27a096c627403400f5f8d1adde54cf38679efef3d54a78353515 157 empty_registry_has_no_recovery_sites
    #[test]
    fn empty_registry_has_no_recovery_sites() {
        let sites = RecoverySites::default();
        assert!(sites.is_empty());
        assert_eq!(sites.len(), 0);
        assert!(sites.get(node(0)).is_none());
        assert!(sites.get(node(42)).is_none());
    }
// TSZ_INLINE_TEST_END 73d57b6d5cfe27a096c627403400f5f8d1adde54cf38679efef3d54a78353515

// TSZ_INLINE_TEST_BEGIN e0ef2bb13a9bca91107dab3640a011dd7333450908d6bbc68647f04bc20fe08f 166 record_then_get_returns_reason
    #[test]
    fn record_then_get_returns_reason() {
        let mut sites = RecoverySites::default();
        sites.record(node(17), RecoveryReason::YieldOutsideGenerator);
        assert_eq!(
            sites.get(node(17)),
            Some(RecoveryReason::YieldOutsideGenerator)
        );
        assert_eq!(sites.len(), 1);
        assert!(!sites.is_empty());
    }
// TSZ_INLINE_TEST_END e0ef2bb13a9bca91107dab3640a011dd7333450908d6bbc68647f04bc20fe08f

// TSZ_INLINE_TEST_BEGIN 8c30f5afd98c34a8872854fe89dcd9794685a2e338fe825ee2c49b5a9f85ef2e 178 record_distinguishes_each_recovery_family
    #[test]
    fn record_distinguishes_each_recovery_family() {
        let mut sites = RecoverySites::default();
        sites.record(
            node(1),
            RecoveryReason::ThisUnresolvedClassOrObjectLiteralMember,
        );
        sites.record(node(2), RecoveryReason::ClassConstructorTargetUnresolved);
        sites.record(node(3), RecoveryReason::YieldOutsideGenerator);
        sites.record(node(4), RecoveryReason::YieldExpressionNoGeneratorContext);

        assert_eq!(sites.len(), 4);
        assert_eq!(
            sites.get(node(1)),
            Some(RecoveryReason::ThisUnresolvedClassOrObjectLiteralMember)
        );
        assert_eq!(
            sites.get(node(2)),
            Some(RecoveryReason::ClassConstructorTargetUnresolved)
        );
        assert_eq!(
            sites.get(node(3)),
            Some(RecoveryReason::YieldOutsideGenerator)
        );
        assert_eq!(
            sites.get(node(4)),
            Some(RecoveryReason::YieldExpressionNoGeneratorContext)
        );
    }
// TSZ_INLINE_TEST_END 8c30f5afd98c34a8872854fe89dcd9794685a2e338fe825ee2c49b5a9f85ef2e

// TSZ_INLINE_TEST_BEGIN dc4e38fea762be0c74d4467c5569cd1cfed2dcb2f9aada4a6029cc85de6cacd0 208 get_returns_none_for_unrecorded_node
    #[test]
    fn get_returns_none_for_unrecorded_node() {
        // Models "real declared `any`": a node that legitimately produced
        // TypeId::ANY through type evaluation rather than recovery is NOT
        // in the registry.
        let mut sites = RecoverySites::default();
        sites.record(
            node(7),
            RecoveryReason::ThisUnresolvedClassOrObjectLiteralMember,
        );
        assert!(sites.get(node(0)).is_none());
        assert!(sites.get(node(7)).is_some());
        assert!(sites.get(node(8)).is_none());
    }
// TSZ_INLINE_TEST_END dc4e38fea762be0c74d4467c5569cd1cfed2dcb2f9aada4a6029cc85de6cacd0

// TSZ_INLINE_TEST_BEGIN f326418a7c1843a03555d160037add06933bfb0ebff34774e6e2649e4c64d234 223 re_recording_same_node_with_same_reason_is_idempotent
    #[test]
    fn re_recording_same_node_with_same_reason_is_idempotent() {
        let mut sites = RecoverySites::default();
        sites.record(node(5), RecoveryReason::YieldOutsideGenerator);
        sites.record(node(5), RecoveryReason::YieldOutsideGenerator);
        assert_eq!(sites.len(), 1);
        assert_eq!(
            sites.get(node(5)),
            Some(RecoveryReason::YieldOutsideGenerator)
        );
    }
// TSZ_INLINE_TEST_END f326418a7c1843a03555d160037add06933bfb0ebff34774e6e2649e4c64d234

// TSZ_INLINE_TEST_BEGIN 5b0662b11796409a8b59d06301f888639058eb8d4fdd7bf87063197f1e0276c9 235 clear_removes_file_local_recovery_sites
    #[test]
    fn clear_removes_file_local_recovery_sites() {
        let mut sites = RecoverySites::default();
        sites.record(node(5), RecoveryReason::YieldOutsideGenerator);
        sites.record(node(6), RecoveryReason::ClassConstructorTargetUnresolved);

        sites.clear();

        assert!(sites.is_empty());
        assert!(sites.get(node(5)).is_none());
        assert!(sites.get(node(6)).is_none());
    }
// TSZ_INLINE_TEST_END 5b0662b11796409a8b59d06301f888639058eb8d4fdd7bf87063197f1e0276c9

// TSZ_INLINE_TEST_BEGIN 3ec72fbc6e0722a8fa3e850e3820f03ec8bb9471de6a0344328ce786743e7164 248 trace_site_labels_are_distinct_per_family
    #[test]
    fn trace_site_labels_are_distinct_per_family() {
        // Trace filters rely on these labels being unique per family;
        // guard against copy-paste collisions between sites.
        let labels = [
            RecoveryReason::ThisUnresolvedClassOrObjectLiteralMember.trace_site(),
            RecoveryReason::ClassConstructorTargetUnresolved.trace_site(),
            RecoveryReason::YieldOutsideGenerator.trace_site(),
            RecoveryReason::YieldExpressionNoGeneratorContext.trace_site(),
        ];
        for i in 0..labels.len() {
            for j in (i + 1)..labels.len() {
                assert_ne!(labels[i], labels[j], "duplicate trace_site label");
            }
        }
    }
// TSZ_INLINE_TEST_END 3ec72fbc6e0722a8fa3e850e3820f03ec8bb9471de6a0344328ce786743e7164

// TSZ_INLINE_TEST_BEGIN a95a7180dda254e4593c32b6d71ee08afef1d38a743ac630730503a2046bb02a 265 iter_yields_all_recorded_sites
    #[test]
    fn iter_yields_all_recorded_sites() {
        let mut sites = RecoverySites::default();
        sites.record(node(10), RecoveryReason::YieldOutsideGenerator);
        sites.record(node(11), RecoveryReason::ClassConstructorTargetUnresolved);
        let collected: Vec<_> = sites.iter().collect();
        assert_eq!(collected.len(), 2);
        assert!(collected.contains(&(node(10), RecoveryReason::YieldOutsideGenerator)));
        assert!(collected.contains(&(node(11), RecoveryReason::ClassConstructorTargetUnresolved)));
    }
// TSZ_INLINE_TEST_END a95a7180dda254e4593c32b6d71ee08afef1d38a743ac630730503a2046bb02a
