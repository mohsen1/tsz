//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/symbol_file_targets.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN f952cd13ddbd6f0d0b7c611b2ef0146f38955865eb35ccbe93af5a12c8440d90 206 child_snapshot_inherits_parent_without_sharing_delta
    #[test]
    fn child_snapshot_inherits_parent_without_sharing_delta() {
        let mut parent = SymbolFileTargetsOverlay::default();
        parent.insert(sid(1), 10);

        let mut child = SymbolFileTargetsOverlay::default();
        child.install_parent_snapshot(parent.snapshot_for_child());
        child.insert(sid(2), 20);

        assert_eq!(child.get(sid(1)), Some(10));
        assert_eq!(child.get(sid(2)), Some(20));
        assert_eq!(parent.get(sid(2)), None);
    }
// TSZ_INLINE_TEST_END f952cd13ddbd6f0d0b7c611b2ef0146f38955865eb35ccbe93af5a12c8440d90

// TSZ_INLINE_TEST_BEGIN 2e0684b63619b4a70b7365ee4f1fc41324a43df9d3bd0536ad3ac02ea193923a 220 merge_from_skips_unchanged_inherited_parent_entries
    #[test]
    fn merge_from_skips_unchanged_inherited_parent_entries() {
        let mut parent = SymbolFileTargetsOverlay::default();
        parent.insert(sid(1), 10);

        let mut child = SymbolFileTargetsOverlay::default();
        child.install_parent_snapshot(parent.snapshot_for_child());

        parent.merge_from(&child, true);

        assert!(parent.delta.is_empty());
        assert_eq!(parent.get(sid(1)), Some(10));
    }
// TSZ_INLINE_TEST_END 2e0684b63619b4a70b7365ee4f1fc41324a43df9d3bd0536ad3ac02ea193923a

// TSZ_INLINE_TEST_BEGIN c8860cbbeca0f86419f86200dbf1fee463605f18e91f1d99b606d8aabbacbf15 234 merge_from_keeps_child_delta_updates
    #[test]
    fn merge_from_keeps_child_delta_updates() {
        let mut parent = SymbolFileTargetsOverlay::default();
        parent.insert(sid(1), 10);

        let mut child = SymbolFileTargetsOverlay::default();
        child.install_parent_snapshot(parent.snapshot_for_child());
        child.insert(sid(1), 11);
        child.insert(sid(2), 20);

        parent.merge_from(&child, true);

        assert_eq!(parent.get(sid(1)), Some(11));
        assert_eq!(parent.get(sid(2)), Some(20));
    }
// TSZ_INLINE_TEST_END c8860cbbeca0f86419f86200dbf1fee463605f18e91f1d99b606d8aabbacbf15

// TSZ_INLINE_TEST_BEGIN 14a18030f88e1faabd5f5fee1fd1e1cef2baaf27ad1912f2ce54d01c745db534 250 restoring_absent_local_override_reveals_parent_owner
    #[test]
    fn restoring_absent_local_override_reveals_parent_owner() {
        let mut parent = SymbolFileTargetsOverlay::default();
        parent.insert(sid(1), 10);

        let mut child = SymbolFileTargetsOverlay::default();
        child.install_parent_snapshot(parent.snapshot_for_child());
        let previous = child.local_override(sid(1));
        child.insert(sid(1), 20);
        assert_eq!(child.get(sid(1)), Some(20));

        child.restore_local_override(sid(1), previous);
        assert_eq!(child.get(sid(1)), Some(10));
        assert_eq!(child.local_override(sid(1)), None);
    }
// TSZ_INLINE_TEST_END 14a18030f88e1faabd5f5fee1fd1e1cef2baaf27ad1912f2ce54d01c745db534
