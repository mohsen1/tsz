//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/project/eviction.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 19370c5a2f24deebb8c85d609386c294db87e381355d908f9997a831443decd7 268 open_close_tracking
    #[test]
    fn open_close_tracking() {
        let mut project = make_project_with_files(&["a.ts", "b.ts"]);
        assert!(!project.is_file_open("a.ts"));

        project.mark_file_open("a.ts");
        assert!(project.is_file_open("a.ts"));
        assert!(!project.is_file_open("b.ts"));
        assert_eq!(project.open_file_count(), 1);

        project.mark_file_closed("a.ts");
        assert!(!project.is_file_open("a.ts"));
        assert_eq!(project.open_file_count(), 0);
    }
// TSZ_INLINE_TEST_END 19370c5a2f24deebb8c85d609386c294db87e381355d908f9997a831443decd7

// TSZ_INLINE_TEST_BEGIN f28cec6acc01a2086071c2c638db3c7febf69e0ccb6a2ca813d29317de892339 283 evict_under_pressure_no_eviction_when_under_target
    #[test]
    fn evict_under_pressure_no_eviction_when_under_target() {
        let mut project = make_project_with_files(&["a.ts"]);
        let result = project.evict_under_pressure(usize::MAX);
        assert!(result.evicted.is_empty());
        assert_eq!(result.bytes_freed, 0);
    }
// TSZ_INLINE_TEST_END f28cec6acc01a2086071c2c638db3c7febf69e0ccb6a2ca813d29317de892339

// TSZ_INLINE_TEST_BEGIN de7d2182d5bf24457cbe31e9ad080fd8d83786542f4296a07003e0ff60c5dfdb 291 evict_under_pressure_removes_files
    #[test]
    fn evict_under_pressure_removes_files() {
        let mut project = make_project_with_files(&["a.ts", "b.ts", "c.ts"]);
        assert_eq!(project.file_count(), 3);

        let result = project.evict_under_pressure(0);
        assert_eq!(result.evicted.len(), 3);
        assert_eq!(project.file_count(), 0);
        assert!(result.bytes_freed > 0);
        assert_eq!(result.bytes_remaining, 0);
    }
// TSZ_INLINE_TEST_END de7d2182d5bf24457cbe31e9ad080fd8d83786542f4296a07003e0ff60c5dfdb

// TSZ_INLINE_TEST_BEGIN 32b0c7999c3300525e80a1608a9fcd8d2e2e0a8d2a2d6b9e3fece82057730172 303 evict_under_pressure_skips_open_files
    #[test]
    fn evict_under_pressure_skips_open_files() {
        let mut project = make_project_with_files(&["a.ts", "b.ts", "c.ts"]);
        project.mark_file_open("b.ts");

        let result = project.evict_under_pressure(0);

        // b.ts should survive (it's open).
        assert_eq!(project.file_count(), 1);
        assert!(project.files.contains_key("b.ts"));

        // Only a.ts and c.ts should have been evicted.
        assert_eq!(result.evicted.len(), 2);
        assert!(result.evicted.iter().all(|e| e.file_name != "b.ts"));
    }
// TSZ_INLINE_TEST_END 32b0c7999c3300525e80a1608a9fcd8d2e2e0a8d2a2d6b9e3fece82057730172

// TSZ_INLINE_TEST_BEGIN eb1a7ce0b11b76e347cc573f742a690a977f1617803649ee8ec097dd9cfa6fd5 319 evict_partial_when_target_reached
    #[test]
    fn evict_partial_when_target_reached() {
        let mut project = make_project_with_files(&["a.ts", "b.ts", "c.ts", "d.ts"]);

        let total = project.total_estimated_bytes();
        let target = total / 2;

        let result = project.evict_under_pressure(target);

        assert!(!result.evicted.is_empty());
        assert!(project.file_count() > 0);
        assert!(result.bytes_remaining <= target);
    }
// TSZ_INLINE_TEST_END eb1a7ce0b11b76e347cc573f742a690a977f1617803649ee8ec097dd9cfa6fd5

// TSZ_INLINE_TEST_BEGIN 35385f77e1d93f94f85bd911b7d26ae81d1644562aca1a7154bc86de0b88eac0 333 eviction_result_bytes_accounting
    #[test]
    fn eviction_result_bytes_accounting() {
        let mut project = make_project_with_files(&["a.ts", "b.ts"]);
        let total_before = project.total_estimated_bytes();

        let result = project.evict_under_pressure(0);

        assert_eq!(result.bytes_freed, total_before);
        assert_eq!(result.bytes_remaining, 0);
    }
// TSZ_INLINE_TEST_END 35385f77e1d93f94f85bd911b7d26ae81d1644562aca1a7154bc86de0b88eac0

// TSZ_INLINE_TEST_BEGIN 4ecb7cc3497fef793d8c62a2888e8e2de2c881a5139501f6cf68b7a850eac876 344 declaration_files_evicted_after_source_files
    #[test]
    fn declaration_files_evicted_after_source_files() {
        let mut project = Project::new();
        // Add source and declaration files with similar content.
        project.set_file(
            "lib.d.ts".to_string(),
            "declare const x: number;".to_string(),
        );
        project.set_file("app.ts".to_string(), "const x: number = 42;".to_string());

        let total = project.total_estimated_bytes();
        // Set target so only one file is evicted.
        let target = total / 2;

        let result = project.evict_under_pressure(target);

        // app.ts (source) should be evicted before lib.d.ts (declaration).
        assert_eq!(result.evicted.len(), 1);
        assert_eq!(result.evicted[0].file_name, "app.ts");
        assert!(project.files.contains_key("lib.d.ts"));
    }
// TSZ_INLINE_TEST_END 4ecb7cc3497fef793d8c62a2888e8e2de2c881a5139501f6cf68b7a850eac876

// TSZ_INLINE_TEST_BEGIN 6db5556a9e418490cc5d55010c2f562b19e8920deb5fc02bed15b2e5784b37c1 366 multiple_open_files_all_protected
    #[test]
    fn multiple_open_files_all_protected() {
        let mut project = make_project_with_files(&["a.ts", "b.ts", "c.ts"]);
        project.mark_file_open("a.ts");
        project.mark_file_open("b.ts");
        project.mark_file_open("c.ts");

        let result = project.evict_under_pressure(0);

        // All files are open, so none should be evicted.
        assert!(result.evicted.is_empty());
        assert_eq!(project.file_count(), 3);
    }
// TSZ_INLINE_TEST_END 6db5556a9e418490cc5d55010c2f562b19e8920deb5fc02bed15b2e5784b37c1

// TSZ_INLINE_TEST_BEGIN 3783322a9eec50418c8a6df7cd18bf5014d80eb7efc153f7fa3ebfcaf6c7acfb 416 evict_if_over_budget_is_noop_without_budget
    #[test]
    fn evict_if_over_budget_is_noop_without_budget() {
        // Default: no budget configured -> eviction never runs.
        let mut project = make_project_with_files(&["a.ts", "b.ts", "c.ts"]);
        assert_eq!(project.memory_budget_bytes(), None);

        let result = project.evict_if_over_budget();

        assert!(result.evicted.is_empty());
        assert_eq!(project.file_count(), 3);
    }
// TSZ_INLINE_TEST_END 3783322a9eec50418c8a6df7cd18bf5014d80eb7efc153f7fa3ebfcaf6c7acfb

// TSZ_INLINE_TEST_BEGIN 12206c9ac903b25091e2831d9519dc700b61c407fb8b73e3d92590ae9cfdbd81 428 evict_if_over_budget_drops_clean_disk_backed_files
    #[test]
    fn evict_if_over_budget_drops_clean_disk_backed_files() {
        let tmp = TempDir::new("clean");
        let a = tmp.write("a.ts", "export const a = 1;\n");
        let b = tmp.write("b.ts", "export const b = 2;\n");

        let mut project = Project::new();
        project.set_file(a.clone(), "export const a = 1;\n".to_string());
        project.set_file(b, "export const b = 2;\n".to_string());
        assert_eq!(project.file_count(), 2);

        // Budget below the current footprint forces eviction. Neither file is
        // open or imported, and both match disk, so both are safe to drop.
        project.set_memory_budget(Some(0));
        let result = project.evict_if_over_budget();

        assert!(!result.evicted.is_empty());
        assert_eq!(project.file_count(), 0);

        // An evicted, on-disk file rehydrates transparently.
        assert!(project.ensure_file_loaded(&a));
        assert_eq!(project.file_count(), 1);
        assert!(project.file(&a).is_some());
    }
// TSZ_INLINE_TEST_END 12206c9ac903b25091e2831d9519dc700b61c407fb8b73e3d92590ae9cfdbd81

// TSZ_INLINE_TEST_BEGIN 5486eaf7911b5d2691ad3166ccd7cd00617cab5ea6fc6c8155fd891a6d857dfc 453 evict_if_over_budget_keeps_files_with_unsaved_changes
    #[test]
    fn evict_if_over_budget_keeps_files_with_unsaved_changes() {
        let tmp = TempDir::new("dirty");
        // On-disk content differs from the in-memory (edited-but-unsaved) buffer.
        let a = tmp.write("a.ts", "export const a = 1;\n");

        let mut project = Project::new();
        project.set_file(a, "export const a = 999; // unsaved edit\n".to_string());

        project.set_memory_budget(Some(0));
        let result = project.evict_if_over_budget();

        // Dropping it would lose the unsaved edit, so it must be retained.
        assert!(result.evicted.is_empty());
        assert_eq!(project.file_count(), 1);
    }
// TSZ_INLINE_TEST_END 5486eaf7911b5d2691ad3166ccd7cd00617cab5ea6fc6c8155fd891a6d857dfc

// TSZ_INLINE_TEST_BEGIN 600399add4f50710cd14a85a5bc4e42ea4b85c93b926a0b73b431b1b207cd70a 470 evict_if_over_budget_keeps_files_with_dependents
    #[test]
    fn evict_if_over_budget_keeps_files_with_dependents() {
        let tmp = TempDir::new("deps");
        let lib = tmp.write("lib.ts", "export const v = 1;\n");
        let app = tmp.write("app.ts", "export const v = 1;\n");

        let mut project = Project::new();
        project.set_file(lib.clone(), "export const v = 1;\n".to_string());
        project.set_file(app.clone(), "export const v = 1;\n".to_string());
        // `app` imports `lib`, so `lib` has a dependent and must not be evicted
        // (its removal would break `app`'s analysis); `app` itself is a leaf.
        project.dependency_graph.add_dependency(&app, &lib);

        project.set_memory_budget(Some(0));
        let result = project.evict_if_over_budget();

        assert!(project.file(&lib).is_some(), "imported file must survive");
        assert!(
            result.evicted.iter().all(|e| e.file_name != lib),
            "imported file must not be evicted"
        );
        assert!(project.file(&app).is_none(), "leaf file should be evicted");
    }
// TSZ_INLINE_TEST_END 600399add4f50710cd14a85a5bc4e42ea4b85c93b926a0b73b431b1b207cd70a

// TSZ_INLINE_TEST_BEGIN fc96b914fa0fdc7d21e1241ad3234028087e795e45711b508679b1917fb28804 494 evict_if_over_budget_protects_open_files
    #[test]
    fn evict_if_over_budget_protects_open_files() {
        let tmp = TempDir::new("open");
        let a = tmp.write("a.ts", "export const a = 1;\n");

        let mut project = Project::new();
        project.set_file(a.clone(), "export const a = 1;\n".to_string());
        project.mark_file_open(&a);

        project.set_memory_budget(Some(0));
        let result = project.evict_if_over_budget();

        assert!(result.evicted.is_empty());
        assert!(project.file(&a).is_some());
    }
// TSZ_INLINE_TEST_END fc96b914fa0fdc7d21e1241ad3234028087e795e45711b508679b1917fb28804

// TSZ_INLINE_TEST_BEGIN f259d894b78f0d4abf46966d60440dc23e86ae8c89e522092b6f7e7e2a2756fe 510 ensure_file_loaded_returns_false_for_missing_file
    #[test]
    fn ensure_file_loaded_returns_false_for_missing_file() {
        let mut project = Project::new();
        assert!(!project.ensure_file_loaded("/no/such/path/does-not-exist.ts"));
        assert_eq!(project.file_count(), 0);
    }
// TSZ_INLINE_TEST_END f259d894b78f0d4abf46966d60440dc23e86ae8c89e522092b6f7e7e2a2756fe
