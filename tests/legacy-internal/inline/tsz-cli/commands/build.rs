//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/commands/build.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e145b50fcdb673bc3f655687d03668452719d04167427dc5b828d6042e43504e 371 get_build_info_path_uses_config_dir_or_out_dir
    #[test]
    fn get_build_info_path_uses_config_dir_or_out_dir() {
        let temp = create_project_dir("paths");
        let root_dir = temp.path().to_path_buf();
        let config_path = write_project_config(&root_dir);
        let project = make_project(config_path.clone(), root_dir.clone(), Vec::new(), None);
        assert_eq!(
            get_build_info_path(&project),
            Some(root_dir.join("tsconfig.tsbuildinfo"))
        );

        let out_dir = root_dir.join("dist");
        let project_with_out_dir =
            make_project(config_path, root_dir, Vec::new(), Some(out_dir.clone()));
        assert_eq!(
            get_build_info_path(&project_with_out_dir),
            Some(out_dir.join("tsconfig.tsbuildinfo"))
        );
    }
// TSZ_INLINE_TEST_END e145b50fcdb673bc3f655687d03668452719d04167427dc5b828d6042e43504e

// TSZ_INLINE_TEST_BEGIN bdf070956db7ac94f4d1b36fdc66819333dfe36c8c34bf74f13dac0fdf362753 391 get_build_info_path_uses_explicit_tsbuildinfo_file
    #[test]
    fn get_build_info_path_uses_explicit_tsbuildinfo_file() {
        let temp = create_project_dir("explicit_path");
        let root_dir = temp.path().to_path_buf();
        let config_path = root_dir.join("tsconfig.json");
        fs::write(
            &config_path,
            r#"{"compilerOptions":{"composite":true,"tsBuildInfoFile":"custom.info"}}"#,
        )
        .unwrap();

        let project = load_project(&config_path).unwrap();
        assert_eq!(
            get_build_info_path(&project),
            Some(project.root_dir.join("custom.info"))
        );
    }
// TSZ_INLINE_TEST_END bdf070956db7ac94f4d1b36fdc66819333dfe36c8c34bf74f13dac0fdf362753

// TSZ_INLINE_TEST_BEGIN 19fcc87adc7b3d0d65810bb1e5b9f92ab98c042d5912badd23b5f0ee0906997c 409 is_project_up_to_date_returns_false_for_root_buildinfo_version_mismatch
    #[test]
    fn is_project_up_to_date_returns_false_for_root_buildinfo_version_mismatch() {
        let temp = create_project_dir("version_mismatch");
        let root_dir = temp.path().to_path_buf();
        let config_path = write_project_config(&root_dir);
        let source_path = write_source_file(&root_dir, "src/index.ts", "export const x = 1;");
        let mut build_info = BuildInfo::new();
        build_info.version = "0.0.0".to_string();
        build_info
            .save(&root_dir.join("tsconfig.tsbuildinfo"))
            .unwrap();

        let project = make_project(config_path, root_dir, Vec::new(), None);
        let _ = source_path;

        assert!(!is_project_up_to_date(&project, &cli_args()));
    }
// TSZ_INLINE_TEST_END 19fcc87adc7b3d0d65810bb1e5b9f92ab98c042d5912badd23b5f0ee0906997c

// TSZ_INLINE_TEST_BEGIN b4d675cead3308fbe4a5063537871abf3eddc44f604ec76ea5252955b781c483 427 is_project_up_to_date_returns_false_for_invalid_root_buildinfo
    #[test]
    fn is_project_up_to_date_returns_false_for_invalid_root_buildinfo() {
        let temp = create_project_dir("invalid_buildinfo");
        let root_dir = temp.path().to_path_buf();
        let config_path = write_project_config(&root_dir);
        write_source_file(&root_dir, "src/index.ts", "export const x = 1;");
        fs::write(root_dir.join("tsconfig.tsbuildinfo"), "{ not json").unwrap();

        let project = make_project(config_path, root_dir, Vec::new(), None);
        assert!(!is_project_up_to_date(&project, &cli_args()));
    }
// TSZ_INLINE_TEST_END b4d675cead3308fbe4a5063537871abf3eddc44f604ec76ea5252955b781c483

// TSZ_INLINE_TEST_BEGIN 8b2f7c2924ab6039fd420a771940f8d5174ffdd765aecb0e56e6ab18e900759e 439 is_project_up_to_date_returns_false_when_referenced_buildinfo_is_missing
    #[test]
    fn is_project_up_to_date_returns_false_when_referenced_buildinfo_is_missing() {
        let temp = create_project_dir("missing_ref_buildinfo");
        let root_dir = temp.path().to_path_buf();
        let config_path = write_project_config(&root_dir);
        let source_path = write_source_file(&root_dir, "src/index.ts", "export const x = 1;");
        write_root_build_info(&root_dir, &source_path, None, None);

        let ref_dir = root_dir.join("ref");
        fs::create_dir_all(&ref_dir).unwrap();
        let ref_config_path = ref_dir.join("tsconfig.json");
        fs::write(&ref_config_path, "{}").unwrap();

        let project = make_project(
            config_path,
            root_dir,
            vec![resolved_reference(ref_config_path)],
            None,
        );

        assert!(!is_project_up_to_date(&project, &cli_args()));
    }
// TSZ_INLINE_TEST_END 8b2f7c2924ab6039fd420a771940f8d5174ffdd765aecb0e56e6ab18e900759e

// TSZ_INLINE_TEST_BEGIN 3fa3f5f4f42eb9e8ffb1de7a8cb11ce85c5b69a9bb6e56509552731aa1dcc01a 462 is_project_up_to_date_allows_referenced_project_without_latest_changed_dts_file
    #[test]
    fn is_project_up_to_date_allows_referenced_project_without_latest_changed_dts_file() {
        let temp = create_project_dir("missing_latest_dts");
        let root_dir = temp.path().to_path_buf();
        let config_path = write_project_config(&root_dir);
        let source_path = write_source_file(&root_dir, "src/index.ts", "export const x = 1;");
        write_root_build_info(&root_dir, &source_path, None, None);

        let ref_dir = root_dir.join("ref");
        fs::create_dir_all(ref_dir.join("dist")).unwrap();
        let ref_config_path = ref_dir.join("tsconfig.json");
        fs::write(&ref_config_path, "{}").unwrap();
        write_reference_build_info(&ref_dir, None);

        let project = make_project(
            config_path,
            root_dir,
            vec![resolved_reference(ref_config_path)],
            None,
        );

        assert!(is_project_up_to_date(&project, &cli_args()));
    }
// TSZ_INLINE_TEST_END 3fa3f5f4f42eb9e8ffb1de7a8cb11ce85c5b69a9bb6e56509552731aa1dcc01a

// TSZ_INLINE_TEST_BEGIN dbba4ec8c45d48e09706351a266d0f55db9beea27e2d18cbf0e7b2a596fef984 486 is_project_up_to_date_uses_referenced_explicit_tsbuildinfo_file
    #[test]
    fn is_project_up_to_date_uses_referenced_explicit_tsbuildinfo_file() {
        let temp = create_project_dir("ref_explicit_buildinfo");
        let root_dir = temp.path().to_path_buf();
        let config_path = write_project_config(&root_dir);
        let source_path = write_source_file(&root_dir, "src/index.ts", "export const x = 1;");
        write_root_build_info(&root_dir, &source_path, None, None);

        let ref_dir = root_dir.join("ref");
        fs::create_dir_all(&ref_dir).unwrap();
        let ref_config_path = ref_dir.join("tsconfig.json");
        fs::write(
            &ref_config_path,
            r#"{"compilerOptions":{"composite":true,"tsBuildInfoFile":"custom.info"}}"#,
        )
        .unwrap();
        let mut ref_build_info = BuildInfo::new();
        ref_build_info.latest_changed_dts_file = None;
        ref_build_info.save(&ref_dir.join("custom.info")).unwrap();

        let project = make_project(
            config_path,
            root_dir,
            vec![resolved_reference(ref_config_path)],
            None,
        );

        assert!(is_project_up_to_date(&project, &cli_args()));
    }
// TSZ_INLINE_TEST_END dbba4ec8c45d48e09706351a266d0f55db9beea27e2d18cbf0e7b2a596fef984

// TSZ_INLINE_TEST_BEGIN 697c6048e09f17f04b14ab257682e31331913635eea588dfdff246443ac4ff88 521 is_project_up_to_date_returns_false_when_referenced_dts_output_is_missing
    // Regression for issue #4753: when a referenced project records a
    // latest_changed_dts_file but that file no longer exists on disk,
    // the parent project must NOT be reported as up-to-date. Previously,
    // metadata/modified() failures fell through silently and the parent
    // project was incorrectly considered fresh.
    #[test]
    fn is_project_up_to_date_returns_false_when_referenced_dts_output_is_missing() {
        let temp = create_project_dir("missing_referenced_dts");
        let root_dir = temp.path().join("main");
        let ref_dir = temp.path().join("ref");
        fs::create_dir_all(&root_dir).unwrap();
        fs::create_dir_all(&ref_dir).unwrap();
        let config_path = write_project_config(&root_dir);
        let source_path = write_source_file(&root_dir, "src/index.ts", "export const x = 1;");
        // u64::MAX so the test cannot accidentally pass via timestamp comparison
        // even if the artifact happened to exist.
        write_root_build_info(&root_dir, &source_path, None, Some(u64::MAX));

        let ref_config_path = ref_dir.join("tsconfig.json");
        fs::write(&ref_config_path, "{}").unwrap();
        // Deliberately do NOT create dist/index.d.ts so the metadata read fails.
        write_reference_build_info(&ref_dir, Some("dist/index.d.ts"));
        assert!(
            !ref_dir.join("dist/index.d.ts").exists(),
            "test precondition: referenced .d.ts should be absent"
        );

        let project = make_project(
            config_path,
            root_dir,
            vec![resolved_reference(ref_config_path)],
            None,
        );

        assert!(!is_project_up_to_date(&project, &cli_args()));
    }
// TSZ_INLINE_TEST_END 697c6048e09f17f04b14ab257682e31331913635eea588dfdff246443ac4ff88

// TSZ_INLINE_TEST_BEGIN f7d5b929f1f6d9bb2c6508855b7554f0d137028825dac1a8ef9c9c6caed07cb6 553 is_project_up_to_date_allows_referenced_project_with_older_dts_output
    #[test]
    fn is_project_up_to_date_allows_referenced_project_with_older_dts_output() {
        let temp = create_project_dir("older_dts");
        let root_dir = temp.path().join("main");
        let ref_dir = temp.path().join("ref");
        fs::create_dir_all(&root_dir).unwrap();
        fs::create_dir_all(&ref_dir).unwrap();
        let config_path = write_project_config(&root_dir);
        let source_path = write_source_file(&root_dir, "src/index.ts", "export const x = 1;");
        write_root_build_info(&root_dir, &source_path, None, Some(u64::MAX));

        let dts_path = write_source_file(
            &ref_dir,
            "dist/index.d.ts",
            "export declare const y: number;",
        );
        let ref_config_path = ref_dir.join("tsconfig.json");
        fs::write(&ref_config_path, "{}").unwrap();
        write_reference_build_info(&ref_dir, Some("dist/index.d.ts"));
        let _ = dts_path;

        let project = make_project(
            config_path,
            root_dir,
            vec![resolved_reference(ref_config_path)],
            None,
        );

        assert!(is_project_up_to_date(&project, &cli_args()));
    }
// TSZ_INLINE_TEST_END f7d5b929f1f6d9bb2c6508855b7554f0d137028825dac1a8ef9c9c6caed07cb6

// TSZ_INLINE_TEST_BEGIN 45382219ff0501879d3b791481a5d61416ed6f2bad1767e1da50159055cc51f3 589 is_project_up_to_date_returns_false_when_referenced_dts_matches_build_time_at_second_resolution
    // Regression for issue #4754: when a referenced project's
    // latest_changed_dts_file has an mtime in exactly the same Unix
    // second as the parent's recorded build_time, the parent must NOT
    // be reported as up-to-date. Pre-fix, the strict `>` comparison
    // returned false here and silently skipped a needed rebuild.
    #[test]
    fn is_project_up_to_date_returns_false_when_referenced_dts_matches_build_time_at_second_resolution()
     {
        let temp = create_project_dir("same_second_dts");
        let root_dir = temp.path().join("main");
        let ref_dir = temp.path().join("ref");
        fs::create_dir_all(&root_dir).unwrap();
        fs::create_dir_all(&ref_dir).unwrap();
        let config_path = write_project_config(&root_dir);
        let source_path = write_source_file(&root_dir, "src/index.ts", "export const x = 1;");

        // Write the referenced .d.ts first so we can read its actual
        // mtime — that is the precise second we need build_time to
        // collide with.
        let dts_path = write_source_file(
            &ref_dir,
            "dist/index.d.ts",
            "export declare const y: number;",
        );
        let dts_mtime_secs = fs::metadata(&dts_path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Set parent build_time to exactly the dts mtime second to
        // simulate "ref project rebuilt within the same Unix second
        // as the parent build". Pre-fix this collides as `dts > bt`
        // -> false; post-fix it triggers `dts >= bt` -> rebuild.
        write_root_build_info(&root_dir, &source_path, None, Some(dts_mtime_secs));

        let ref_config_path = ref_dir.join("tsconfig.json");
        fs::write(&ref_config_path, "{}").unwrap();
        write_reference_build_info(&ref_dir, Some("dist/index.d.ts"));

        let project = make_project(
            config_path,
            root_dir,
            vec![resolved_reference(ref_config_path)],
            None,
        );

        assert!(
            !is_project_up_to_date(&project, &cli_args()),
            "expected same-second match to force a rebuild (issue #4754)"
        );
    }
// TSZ_INLINE_TEST_END 45382219ff0501879d3b791481a5d61416ed6f2bad1767e1da50159055cc51f3
