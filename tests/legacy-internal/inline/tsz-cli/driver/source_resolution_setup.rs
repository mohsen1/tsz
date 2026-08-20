//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/driver/source_resolution_setup.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2e0aa1d8a35208d16a1a99253c0f7df2db3463e3c19953d0518ab1f90b05f874 836 relative_unscoped_package_matches_its_own_augmentation
    #[test]
    fn relative_unscoped_package_matches_its_own_augmentation() {
        let p = Path::new("node_modules/acorn-walk/dist/walk.d.ts");
        assert!(looks_like_package_self_reference(p, "acorn-walk"));
    }
// TSZ_INLINE_TEST_END 2e0aa1d8a35208d16a1a99253c0f7df2db3463e3c19953d0518ab1f90b05f874

// TSZ_INLINE_TEST_BEGIN d6edbe728675122bd64a0abbfa28ef17a06cbbb5a424925ff424bc6c7d19de6f 842 absolute_unscoped_package_matches_its_own_augmentation
    #[test]
    fn absolute_unscoped_package_matches_its_own_augmentation() {
        let p = Path::new("/tmp/x/node_modules/acorn-walk/dist/walk.d.ts");
        assert!(looks_like_package_self_reference(p, "acorn-walk"));
    }
// TSZ_INLINE_TEST_END d6edbe728675122bd64a0abbfa28ef17a06cbbb5a424925ff424bc6c7d19de6f

// TSZ_INLINE_TEST_BEGIN da050193075908299c3977ee7e4b7c629bd5290a002e2217e953af46c4fa4cd9 848 scoped_package_matches_its_own_augmentation
    #[test]
    fn scoped_package_matches_its_own_augmentation() {
        let p = Path::new("node_modules/@scope/pkg/dist/index.d.ts");
        assert!(looks_like_package_self_reference(p, "@scope/pkg"));
        assert!(looks_like_package_self_reference(p, "@scope/pkg/subpath"));
    }
// TSZ_INLINE_TEST_END da050193075908299c3977ee7e4b7c629bd5290a002e2217e953af46c4fa4cd9

// TSZ_INLINE_TEST_BEGIN 6e84b7038863cd120da89f9b4c201a213715770af90237fb79ed132697c265d6 855 different_package_is_not_a_self_reference
    #[test]
    fn different_package_is_not_a_self_reference() {
        let p = Path::new("node_modules/gearbox/index.d.ts");
        assert!(!looks_like_package_self_reference(p, "acorn-walk"));
    }
// TSZ_INLINE_TEST_END 6e84b7038863cd120da89f9b4c201a213715770af90237fb79ed132697c265d6

// TSZ_INLINE_TEST_BEGIN 81553df82685e23e79b0b837a4f54260329be4d159d0d1be67787ddf3e2821bd 861 scoped_specifier_does_not_match_unscoped_directory
    #[test]
    fn scoped_specifier_does_not_match_unscoped_directory() {
        let p = Path::new("node_modules/pkg/index.d.ts");
        assert!(!looks_like_package_self_reference(p, "@scope/pkg"));
    }
// TSZ_INLINE_TEST_END 81553df82685e23e79b0b837a4f54260329be4d159d0d1be67787ddf3e2821bd

// TSZ_INLINE_TEST_BEGIN 9b5fdfbb4afa2d413dd8f06ba59e505d2bcba8b097d3a6380843ef2d294fbdca 867 file_outside_node_modules_is_never_a_self_reference
    #[test]
    fn file_outside_node_modules_is_never_a_self_reference() {
        let p = Path::new("src/augment.d.ts");
        assert!(!looks_like_package_self_reference(p, "acorn-walk"));
    }
// TSZ_INLINE_TEST_END 9b5fdfbb4afa2d413dd8f06ba59e505d2bcba8b097d3a6380843ef2d294fbdca
