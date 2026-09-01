//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-binder/src/state/export_surface.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 1d0e0674c77902d5584f901ef90b0c2080a809630ae41f2cf018f3c17b555d1a 434 empty_surface
    #[test]
    fn empty_surface() {
        let surface = ExportSurface::default();
        assert!(!surface.has_public_api_scope);
        assert!(!surface.has_export_equals);
        assert_eq!(surface.public_api_size(), 0);
        assert!(surface.exported_names().is_empty());
    }
// TSZ_INLINE_TEST_END 1d0e0674c77902d5584f901ef90b0c2080a809630ae41f2cf018f3c17b555d1a

// TSZ_INLINE_TEST_BEGIN 3c16717265e280b998b43ef4b5788b20d2753eeba7002f1bfae1295f847d0a69 443 empty_surface_query_methods_are_negative
    #[test]
    fn empty_surface_query_methods_are_negative() {
        let surface = ExportSurface::default();
        assert!(!surface.is_exported("anything"));
        assert!(!surface.is_type_only_export("anything"));
        assert!(!surface.has_overloads("anything"));
        assert!(surface.symbol_for_export("anything").is_none());
        assert!(surface.default_export.is_none());
    }
// TSZ_INLINE_TEST_END 3c16717265e280b998b43ef4b5788b20d2753eeba7002f1bfae1295f847d0a69

// TSZ_INLINE_TEST_BEGIN c5d4fbe5c82fccf782f618f23c47225bde6160b8b2227014d9c186e4e57c8bd2 455 is_exported_true_for_module_export_only
    #[test]
    fn is_exported_true_for_module_export_only() {
        let mut surface = ExportSurface::default();
        surface
            .module_exports
            .insert("foo".to_string(), exp(1, 0, false));
        assert!(surface.is_exported("foo"));
        assert!(!surface.is_exported("bar"));
    }
// TSZ_INLINE_TEST_END c5d4fbe5c82fccf782f618f23c47225bde6160b8b2227014d9c186e4e57c8bd2

// TSZ_INLINE_TEST_BEGIN 3fede91afa0b31a176afd44ee4a4f99e2fc4f0fc4613549b064610436483911d 465 is_exported_true_for_file_local_only
    #[test]
    fn is_exported_true_for_file_local_only() {
        let mut surface = ExportSurface::default();
        surface
            .file_exported_locals
            .insert("foo".to_string(), exp(1, 0, false));
        assert!(surface.is_exported("foo"));
        assert!(!surface.is_exported("bar"));
    }
// TSZ_INLINE_TEST_END 3fede91afa0b31a176afd44ee4a4f99e2fc4f0fc4613549b064610436483911d

// TSZ_INLINE_TEST_BEGIN 6c051cf1dad1a3598413bf080d6a71699f27723c0842b59a918e2f04f3245a74 475 is_exported_true_when_in_both_populations
    #[test]
    fn is_exported_true_when_in_both_populations() {
        let mut surface = ExportSurface::default();
        surface
            .module_exports
            .insert("foo".to_string(), exp(1, 0, false));
        surface
            .file_exported_locals
            .insert("foo".to_string(), exp(1, 0, false));
        assert!(surface.is_exported("foo"));
    }
// TSZ_INLINE_TEST_END 6c051cf1dad1a3598413bf080d6a71699f27723c0842b59a918e2f04f3245a74

// TSZ_INLINE_TEST_BEGIN 055525be6a0fa711232d76a98f4a25583f8a878a87af1519ae11ab819e5a69ed 489 is_type_only_export_reads_module_exports_first
    #[test]
    fn is_type_only_export_reads_module_exports_first() {
        let mut surface = ExportSurface::default();
        // module_exports is checked first; its is_type_only wins.
        surface
            .module_exports
            .insert("T".to_string(), exp(1, 0, true));
        // Even if file_exported_locals disagrees, module_exports value wins.
        surface
            .file_exported_locals
            .insert("T".to_string(), exp(1, 0, false));
        assert!(surface.is_type_only_export("T"));
    }
// TSZ_INLINE_TEST_END 055525be6a0fa711232d76a98f4a25583f8a878a87af1519ae11ab819e5a69ed

// TSZ_INLINE_TEST_BEGIN 90878dc27faf4afa11abe4683fc8f1adb87f871d60d8608b8405babd90da3db3 503 is_type_only_export_falls_back_to_file_locals
    #[test]
    fn is_type_only_export_falls_back_to_file_locals() {
        let mut surface = ExportSurface::default();
        surface
            .file_exported_locals
            .insert("T".to_string(), exp(1, 0, true));
        assert!(surface.is_type_only_export("T"));
    }
// TSZ_INLINE_TEST_END 90878dc27faf4afa11abe4683fc8f1adb87f871d60d8608b8405babd90da3db3

// TSZ_INLINE_TEST_BEGIN f474ec5b3d863a3ab880bf947e6d43566dfedf810ada6548376e63fdc2bfa34d 512 is_type_only_export_false_for_value_export
    #[test]
    fn is_type_only_export_false_for_value_export() {
        let mut surface = ExportSurface::default();
        surface
            .module_exports
            .insert("v".to_string(), exp(1, 0, false));
        assert!(!surface.is_type_only_export("v"));
    }
// TSZ_INLINE_TEST_END f474ec5b3d863a3ab880bf947e6d43566dfedf810ada6548376e63fdc2bfa34d

// TSZ_INLINE_TEST_BEGIN d76083fa400ebeadf9791b9ed1a0ae2fd8f31ec528e87898269ded7c8a4a41cc 521 is_type_only_export_false_for_unknown_name
    #[test]
    fn is_type_only_export_false_for_unknown_name() {
        let surface = ExportSurface::default();
        assert!(!surface.is_type_only_export("missing"));
    }
// TSZ_INLINE_TEST_END d76083fa400ebeadf9791b9ed1a0ae2fd8f31ec528e87898269ded7c8a4a41cc

// TSZ_INLINE_TEST_BEGIN 956d1c9b9dacc506839af0d325cd2b6a715af400f645eb2f2f0aa0b616151fcd 529 exported_names_sorts_alphabetically
    #[test]
    fn exported_names_sorts_alphabetically() {
        let mut surface = ExportSurface::default();
        surface
            .module_exports
            .insert("zeta".to_string(), exp(1, 0, false));
        surface
            .module_exports
            .insert("alpha".to_string(), exp(2, 0, false));
        surface
            .module_exports
            .insert("middle".to_string(), exp(3, 0, false));
        let names = surface.exported_names();
        assert_eq!(names, vec!["alpha", "middle", "zeta"]);
    }
// TSZ_INLINE_TEST_END 956d1c9b9dacc506839af0d325cd2b6a715af400f645eb2f2f0aa0b616151fcd

// TSZ_INLINE_TEST_BEGIN 9f728502af36ac97791f9b70994e48ed9e666e5d2a9443b698437dd943e30fd9 545 exported_names_dedups_overlap_between_populations
    #[test]
    fn exported_names_dedups_overlap_between_populations() {
        let mut surface = ExportSurface::default();
        surface
            .module_exports
            .insert("foo".to_string(), exp(1, 0, false));
        surface
            .file_exported_locals
            .insert("foo".to_string(), exp(1, 0, false));
        let names = surface.exported_names();
        assert_eq!(names, vec!["foo"]);
    }
// TSZ_INLINE_TEST_END 9f728502af36ac97791f9b70994e48ed9e666e5d2a9443b698437dd943e30fd9

// TSZ_INLINE_TEST_BEGIN 5250a9253e4c79afef3ee01f22ecae87fb2ba1511a0bc2cb3fb8a96b0f2cdd9c 558 exported_names_unions_distinct_entries_from_both_populations
    #[test]
    fn exported_names_unions_distinct_entries_from_both_populations() {
        let mut surface = ExportSurface::default();
        surface
            .module_exports
            .insert("a".to_string(), exp(1, 0, false));
        surface
            .file_exported_locals
            .insert("b".to_string(), exp(2, 0, false));
        let names = surface.exported_names();
        assert_eq!(names, vec!["a", "b"]);
    }
// TSZ_INLINE_TEST_END 5250a9253e4c79afef3ee01f22ecae87fb2ba1511a0bc2cb3fb8a96b0f2cdd9c

// TSZ_INLINE_TEST_BEGIN 6b5bbc433310204c6960dfbb16ade361848cd383acb83e4f714e0e360b33cd39 571 exported_names_excludes_reexports
    #[test]
    fn exported_names_excludes_reexports() {
        // Re-exports do NOT participate in exported_names() — only direct
        // exports do. Lock that contract.
        let mut surface = ExportSurface::default();
        surface.named_reexports.push(nre("re", "./m", Some("orig")));
        surface.wildcard_reexports.push(wre("./other", false));
        assert!(surface.exported_names().is_empty());
    }
// TSZ_INLINE_TEST_END 6b5bbc433310204c6960dfbb16ade361848cd383acb83e4f714e0e360b33cd39

// TSZ_INLINE_TEST_BEGIN 575d01847c062b65167266ad83ffa26bd8d96807ee416045828b688eb943589f 583 has_overloads_membership
    #[test]
    fn has_overloads_membership() {
        let mut surface = ExportSurface::default();
        surface.overloaded_functions.insert("f".to_string());
        assert!(surface.has_overloads("f"));
        assert!(!surface.has_overloads("g"));
    }
// TSZ_INLINE_TEST_END 575d01847c062b65167266ad83ffa26bd8d96807ee416045828b688eb943589f

// TSZ_INLINE_TEST_BEGIN f4803a6b972eb4d1487485dcaba7a40e59924c58c6d8c297bee6cd27f84a83db 591 has_overloads_empty_set
    #[test]
    fn has_overloads_empty_set() {
        let surface = ExportSurface::default();
        assert!(!surface.has_overloads(""));
        assert!(!surface.has_overloads("any"));
    }
// TSZ_INLINE_TEST_END f4803a6b972eb4d1487485dcaba7a40e59924c58c6d8c297bee6cd27f84a83db

// TSZ_INLINE_TEST_BEGIN 673f28db27cae6fb954b316ab01e3d75168a3a2715c27304f9df53c6eae59055 600 symbol_for_export_finds_module_export_first
    #[test]
    fn symbol_for_export_finds_module_export_first() {
        let mut surface = ExportSurface::default();
        // module_exports has higher priority — its SymbolId wins on overlap.
        surface
            .module_exports
            .insert("foo".to_string(), exp(7, 0, false));
        surface
            .file_exported_locals
            .insert("foo".to_string(), exp(99, 0, false));
        assert_eq!(surface.symbol_for_export("foo"), Some(SymbolId(7)));
    }
// TSZ_INLINE_TEST_END 673f28db27cae6fb954b316ab01e3d75168a3a2715c27304f9df53c6eae59055

// TSZ_INLINE_TEST_BEGIN f59303fdb6757ad267a1ab54c8c46c9269a7179740d904416006b01b3944ce2e 613 symbol_for_export_falls_back_to_file_locals
    #[test]
    fn symbol_for_export_falls_back_to_file_locals() {
        let mut surface = ExportSurface::default();
        surface
            .file_exported_locals
            .insert("foo".to_string(), exp(42, 0, false));
        assert_eq!(surface.symbol_for_export("foo"), Some(SymbolId(42)));
    }
// TSZ_INLINE_TEST_END f59303fdb6757ad267a1ab54c8c46c9269a7179740d904416006b01b3944ce2e

// TSZ_INLINE_TEST_BEGIN 00e064a46d29f09b748c9519ce14a16525fb283297a34016495dc16b67222a74 622 symbol_for_export_returns_none_for_unknown
    #[test]
    fn symbol_for_export_returns_none_for_unknown() {
        let surface = ExportSurface::default();
        assert!(surface.symbol_for_export("missing").is_none());
    }
// TSZ_INLINE_TEST_END 00e064a46d29f09b748c9519ce14a16525fb283297a34016495dc16b67222a74

// TSZ_INLINE_TEST_BEGIN 87445a7c64fa7c90cae57dae2b2d11e915653829daf5779b3d60fe6c9588ea8a 630 public_api_size_counts_each_population_once
    #[test]
    fn public_api_size_counts_each_population_once() {
        let mut surface = ExportSurface::default();
        surface
            .module_exports
            .insert("a".to_string(), exp(1, 0, false));
        surface
            .module_exports
            .insert("b".to_string(), exp(2, 0, false));
        surface
            .file_exported_locals
            .insert("c".to_string(), exp(3, 0, false));
        surface.named_reexports.push(nre("d", "./m", None));
        surface.wildcard_reexports.push(wre("./other", false));
        // 2 module_exports + 1 unique file_local + 1 named + 1 wildcard.
        assert_eq!(surface.public_api_size(), 5);
    }
// TSZ_INLINE_TEST_END 87445a7c64fa7c90cae57dae2b2d11e915653829daf5779b3d60fe6c9588ea8a

// TSZ_INLINE_TEST_BEGIN 7820f40091ad99caaaba8b5d1064b889b5f2ef4ec03979c48ed0e07c7e36458d 648 public_api_size_does_not_double_count_overlap
    #[test]
    fn public_api_size_does_not_double_count_overlap() {
        let mut surface = ExportSurface::default();
        // Same name "foo" appears in both populations — must only be counted once.
        surface
            .module_exports
            .insert("foo".to_string(), exp(1, 0, false));
        surface
            .file_exported_locals
            .insert("foo".to_string(), exp(1, 0, false));
        assert_eq!(surface.public_api_size(), 1);
    }
// TSZ_INLINE_TEST_END 7820f40091ad99caaaba8b5d1064b889b5f2ef4ec03979c48ed0e07c7e36458d

// TSZ_INLINE_TEST_BEGIN 747e0d96992ddcfc6136c668f0abff5a52759f3a578a457aa5e4ba772e13dd5f 661 public_api_size_with_only_reexports
    #[test]
    fn public_api_size_with_only_reexports() {
        let mut surface = ExportSurface::default();
        surface.named_reexports.push(nre("a", "./m", None));
        surface.named_reexports.push(nre("b", "./m", None));
        surface.wildcard_reexports.push(wre("./n", false));
        assert_eq!(surface.public_api_size(), 3);
    }
// TSZ_INLINE_TEST_END 747e0d96992ddcfc6136c668f0abff5a52759f3a578a457aa5e4ba772e13dd5f

// TSZ_INLINE_TEST_BEGIN eb91c5d26b130334dbb0ac82d6c1d2df71a8d8d46bac35f10864a129119d8a26 670 public_api_size_partial_overlap_unique_locals_counted
    #[test]
    fn public_api_size_partial_overlap_unique_locals_counted() {
        let mut surface = ExportSurface::default();
        surface
            .module_exports
            .insert("shared".to_string(), exp(1, 0, false));
        surface
            .file_exported_locals
            .insert("shared".to_string(), exp(1, 0, false));
        surface
            .file_exported_locals
            .insert("unique".to_string(), exp(2, 0, false));
        // 1 module export ("shared") + 1 unique file-local ("unique"). Overlap
        // does NOT add to module count.
        assert_eq!(surface.public_api_size(), 2);
    }
// TSZ_INLINE_TEST_END eb91c5d26b130334dbb0ac82d6c1d2df71a8d8d46bac35f10864a129119d8a26

// TSZ_INLINE_TEST_BEGIN 0e79625d4b946f14299d31b616289ed54ef8f514e0491a7163c9c3562f650fb9 689 default_collections_are_empty
    #[test]
    fn default_collections_are_empty() {
        let s = ExportSurface::default();
        assert!(s.module_exports.is_empty());
        assert!(s.file_exported_locals.is_empty());
        assert!(s.named_reexports.is_empty());
        assert!(s.wildcard_reexports.is_empty());
        assert!(s.global_augmentations.is_empty());
        assert!(s.module_augmentations.is_empty());
        assert!(s.overloaded_functions.is_empty());
    }
// TSZ_INLINE_TEST_END 0e79625d4b946f14299d31b616289ed54ef8f514e0491a7163c9c3562f650fb9

// TSZ_INLINE_TEST_BEGIN 09a2ff8c4a64a08a3a2b923da136d1f60d4cf07225a41ca85ef6457f2aaf1207 703 clone_preserves_all_query_results
    #[test]
    fn clone_preserves_all_query_results() {
        let mut surface = ExportSurface::default();
        surface
            .module_exports
            .insert("foo".to_string(), exp(1, 0, true));
        surface
            .file_exported_locals
            .insert("bar".to_string(), exp(2, 0, false));
        surface.overloaded_functions.insert("over".to_string());
        surface.named_reexports.push(nre("re", "./m", None));
        surface.has_export_equals = true;
        surface.default_export = Some(SymbolId(99));

        let cloned = surface.clone();
        assert!(cloned.is_exported("foo"));
        assert!(cloned.is_type_only_export("foo"));
        assert!(cloned.is_exported("bar"));
        assert!(!cloned.is_type_only_export("bar"));
        assert!(cloned.has_overloads("over"));
        assert_eq!(cloned.symbol_for_export("foo"), Some(SymbolId(1)));
        assert_eq!(cloned.public_api_size(), 3);
        assert!(cloned.has_export_equals);
        assert_eq!(cloned.default_export, Some(SymbolId(99)));
    }
// TSZ_INLINE_TEST_END 09a2ff8c4a64a08a3a2b923da136d1f60d4cf07225a41ca85ef6457f2aaf1207
