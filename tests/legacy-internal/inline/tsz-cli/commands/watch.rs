//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/commands/watch.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 4cf28e476d401938ec4c1a6caadf121e272a3b20760ece332bc154b5acd891f7 636 format_12h_handles_midnight_noon_and_edge_minutes
    #[test]
    fn format_12h_handles_midnight_noon_and_edge_minutes() {
        assert_eq!(format_12h(0, 0, 0), "12:00:00 AM");
        assert_eq!(format_12h(11, 59, 59), "11:59:59 AM");
        assert_eq!(format_12h(12, 0, 0), "12:00:00 PM");
        assert_eq!(format_12h(23, 59, 59), "11:59:59 PM");
    }
// TSZ_INLINE_TEST_END 4cf28e476d401938ec4c1a6caadf121e272a3b20760ece332bc154b5acd891f7

// TSZ_INLINE_TEST_BEGIN 164af4da24252a9f04d46fd2f097244af67b8cf6324b6566a76290144d7df126 644 normalize_event_path_preserves_absolute_paths_and_joins_relative_paths
    #[test]
    fn normalize_event_path_preserves_absolute_paths_and_joins_relative_paths() {
        let base_dir = Path::new("/repo/project");

        assert_eq!(
            normalize_event_path(base_dir, Path::new("/tmp/file.ts")),
            PathBuf::from("/tmp/file.ts")
        );
        assert_eq!(
            normalize_event_path(base_dir, Path::new("src/file.ts")),
            PathBuf::from("/repo/project/src/file.ts")
        );
    }
// TSZ_INLINE_TEST_END 164af4da24252a9f04d46fd2f097244af67b8cf6324b6566a76290144d7df126

// TSZ_INLINE_TEST_BEGIN 32d2d49939ca20a9c5d9026329d369f42159598a11652370ed5ebe55d735f7c6 658 is_default_excluded_matches_nested_default_exclude_directories
    #[test]
    fn is_default_excluded_matches_nested_default_exclude_directories() {
        assert!(is_default_excluded(Path::new(
            "/repo/node_modules/pkg/index.ts"
        )));
        assert!(is_default_excluded(Path::new(
            "/repo/src/bower_components/lib.ts"
        )));
        assert!(!is_default_excluded(Path::new("/repo/src/app.ts")));
    }
// TSZ_INLINE_TEST_END 32d2d49939ca20a9c5d9026329d369f42159598a11652370ed5ebe55d735f7c6

// TSZ_INLINE_TEST_BEGIN 29e28fd1899e05c1119d906bdd485b032bde82190d83edb7ebdaaad900923630 669 resolve_explicit_files_joins_relative_entries_and_keeps_absolute_entries
    #[test]
    fn resolve_explicit_files_joins_relative_entries_and_keeps_absolute_entries() {
        let base_dir = Path::new("/repo/project");
        let files = vec![
            PathBuf::from("src/app.ts"),
            PathBuf::from("/external/shared.ts"),
        ];

        let resolved = resolve_explicit_files(base_dir, &files).expect("expected explicit files");
        assert!(resolved.contains(&PathBuf::from("/repo/project/src/app.ts")));
        assert!(resolved.contains(&PathBuf::from("/external/shared.ts")));
    }
// TSZ_INLINE_TEST_END 29e28fd1899e05c1119d906bdd485b032bde82190d83edb7ebdaaad900923630

// TSZ_INLINE_TEST_BEGIN bd6854c09f050928800d7ba4870df15fa9517bd9b0bf65bf242c78c9b0c8f1bd 682 collect_watch_roots_deduplicates_base_and_parent_directories
    #[test]
    fn collect_watch_roots_deduplicates_base_and_parent_directories() {
        let base_dir = Path::new("/repo/project");
        let mut explicit = FxHashSet::default();
        explicit.insert(PathBuf::from("/repo/project/src/app.ts"));
        explicit.insert(PathBuf::from("/repo/project/src/utils/helper.ts"));

        let roots = collect_watch_roots(base_dir, Some(&explicit));
        assert_eq!(
            roots,
            vec![
                PathBuf::from("/repo/project"),
                PathBuf::from("/repo/project/src"),
                PathBuf::from("/repo/project/src/utils"),
            ]
        );
    }
// TSZ_INLINE_TEST_END bd6854c09f050928800d7ba4870df15fa9517bd9b0bf65bf242c78c9b0c8f1bd

// TSZ_INLINE_TEST_BEGIN bda0f8c93fbc92b1c4d4fbfa79e0ddc3074a4256ea00fb891d53771fd701c2e3 700 watch_filter_respects_project_config_explicit_files_and_exclusions
    #[test]
    fn watch_filter_respects_project_config_explicit_files_and_exclusions() {
        let base_dir = PathBuf::from("/repo/project");
        let project_config = base_dir.join("tsconfig.json");

        let explicit_file = base_dir.join("src/app.ts");
        let ignored_file = base_dir.join("dist/generated.ts");
        let excluded_file = base_dir.join("src/skip.ts");
        let other_file = base_dir.join("src/other.ts");

        let mut explicit_files = FxHashSet::default();
        explicit_files.insert(explicit_file.clone());

        let mut exclude_files = FxHashSet::default();
        exclude_files.insert(excluded_file.clone());

        let filter = WatchFilter::new(
            Some(explicit_files),
            vec![base_dir.join("dist")],
            Some(project_config.clone()),
            Some(exclude_files),
        );

        assert!(filter.should_record(&project_config));
        assert!(filter.should_record(&explicit_file));
        assert!(!filter.should_record(&ignored_file));
        assert!(!filter.should_record(&excluded_file));
        assert!(!filter.should_record(&other_file));
    }
// TSZ_INLINE_TEST_END bda0f8c93fbc92b1c4d4fbfa79e0ddc3074a4256ea00fb891d53771fd701c2e3

// TSZ_INLINE_TEST_BEGIN 31a0a2a5a22822b112aa969df24d5f7f2327c725952ca5028186befa986dcb4f 730 watch_filter_marks_emitted_paths_as_ineligible
    #[test]
    fn watch_filter_marks_emitted_paths_as_ineligible() {
        let base_dir = PathBuf::from("/repo/project");
        let path = base_dir.join("src/app.ts");

        let mut explicit_files = FxHashSet::default();
        explicit_files.insert(path.clone());

        let mut filter = WatchFilter::new(Some(explicit_files), Vec::new(), None, None);
        assert!(filter.should_record(&path));

        filter.set_last_emitted(vec![path.clone()]);
        assert!(!filter.should_record(&path));
    }
// TSZ_INLINE_TEST_END 31a0a2a5a22822b112aa969df24d5f7f2327c725952ca5028186befa986dcb4f

// TSZ_INLINE_TEST_BEGIN a4b6f6bb9ea900107bc130b9a988faa9a4c0c324ece7b071ecfe55e7b457de99 745 debouncer_coalesces_events_and_clears_after_flush_or_removal
    #[test]
    fn debouncer_coalesces_events_and_clears_after_flush_or_removal() {
        let mut debouncer = Debouncer::new(Duration::from_millis(200));
        let now = Instant::now();
        let path_a = PathBuf::from("/repo/project/src/a.ts");
        let path_b = PathBuf::from("/repo/project/src/b.ts");

        debouncer.record_at(now, path_a.clone());
        debouncer.record_at(now, path_b.clone());
        debouncer.record_at(now, path_a.clone());

        assert!(
            debouncer
                .flush_ready(now + Duration::from_millis(150))
                .is_none()
        );

        let flushed = debouncer
            .flush_ready(now + Duration::from_millis(200))
            .expect("expected pending paths to flush");
        let flushed: FxHashSet<_> = flushed.into_iter().collect();
        assert_eq!(flushed.len(), 2);
        assert!(flushed.contains(&path_a));
        assert!(flushed.contains(&path_b));

        debouncer.record_at(now + Duration::from_millis(250), path_a.clone());
        let mut remove = FxHashSet::default();
        remove.insert(path_a);
        debouncer.remove_paths(&remove);
        assert!(
            debouncer
                .flush_ready(now + Duration::from_millis(500))
                .is_none()
        );
    }
// TSZ_INLINE_TEST_END a4b6f6bb9ea900107bc130b9a988faa9a4c0c324ece7b071ecfe55e7b457de99
