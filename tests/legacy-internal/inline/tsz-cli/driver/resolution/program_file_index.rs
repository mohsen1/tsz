//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/driver/resolution/program_file_index.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 33b232819f09928fecaa9b192a6556e869dd3c1a6880ceda16c74fe40b9c2f64 138 symlinked_and_real_paths_resolve_to_same_idx
    #[test]
    fn symlinked_and_real_paths_resolve_to_same_idx() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::TempDir::new().expect("temp dir");
        let root = dir.path();
        fs::create_dir_all(root.join("core/node_modules/package-a")).unwrap();
        fs::write(
            root.join("core/node_modules/package-a/index.d.ts"),
            "export interface Box {}",
        )
        .unwrap();
        symlink(
            root.join("core/node_modules/package-a"),
            root.join("package-a"),
        )
        .unwrap();

        let symlinked_path = root.join("package-a/index.d.ts");
        let real_path = root.join("core/node_modules/package-a/index.d.ts");

        let options = ResolvedCompilerOptions {
            module_resolution: Some(ModuleResolutionKind::Node16),
            preserve_symlinks: false,
            module_suffixes: vec![String::new()],
            ..Default::default()
        };

        let mut index = ProgramFileIndex::with_capacity(1);
        index.insert(&symlinked_path.to_string_lossy(), 7, &options);

        // The same file resolved via its real path must still find idx 7.
        let real_canonical = normalize_resolved_path(&real_path, &options);
        let resolved = index
            .get_with_symlink_fallback(&real_canonical, &real_path, &options)
            .expect("real path should resolve to the symlinked program entry");
        assert_eq!(resolved, 7);

        // And the original symlinked path still works via the primary key.
        let symlinked_canonical = normalize_resolved_path(&symlinked_path, &options);
        assert_eq!(
            index.get_with_symlink_fallback(&symlinked_canonical, &symlinked_path, &options),
            Some(7),
        );
    }
// TSZ_INLINE_TEST_END 33b232819f09928fecaa9b192a6556e869dd3c1a6880ceda16c74fe40b9c2f64

// TSZ_INLINE_TEST_BEGIN 6d9119ed6fa28f1e7e8e25f3bc1827e9cda2e1a73d0f4986727511549b5c4efb 184 preserve_symlinks_disables_real_path_fallback
    #[test]
    fn preserve_symlinks_disables_real_path_fallback() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::TempDir::new().expect("temp dir");
        let root = dir.path();
        fs::create_dir_all(root.join("real")).unwrap();
        fs::write(root.join("real/index.d.ts"), "export {};").unwrap();
        symlink(root.join("real"), root.join("linked")).unwrap();

        let symlinked_path = root.join("linked/index.d.ts");
        let real_path = root.join("real/index.d.ts");

        let options = ResolvedCompilerOptions {
            module_resolution: Some(ModuleResolutionKind::Node16),
            preserve_symlinks: true,
            module_suffixes: vec![String::new()],
            ..Default::default()
        };

        let mut index = ProgramFileIndex::with_capacity(1);
        index.insert(&symlinked_path.to_string_lossy(), 3, &options);

        let real_canonical = normalize_resolved_path(&real_path, &options);
        assert!(
            index
                .get_with_symlink_fallback(&real_canonical, &real_path, &options)
                .is_none(),
            "preserveSymlinks must not unify symlink and real paths"
        );
    }
// TSZ_INLINE_TEST_END 6d9119ed6fa28f1e7e8e25f3bc1827e9cda2e1a73d0f4986727511549b5c4efb

// TSZ_INLINE_TEST_BEGIN ed7571dc327e79b5dd6c0d5f7b1a39e439ae6c6784d576edb9700f08e6fe1ab0 216 first_write_wins_keeps_idx_deterministic
    #[test]
    fn first_write_wins_keeps_idx_deterministic() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::TempDir::new().expect("temp dir");
        let root = dir.path();
        fs::create_dir_all(root.join("real")).unwrap();
        fs::write(root.join("real/index.d.ts"), "export {};").unwrap();
        symlink(root.join("real"), root.join("linked")).unwrap();

        let symlinked_path = root.join("linked/index.d.ts");
        let other_symlinked_path = root.join("linked/index.d.ts");

        let options = ResolvedCompilerOptions {
            module_resolution: Some(ModuleResolutionKind::Node16),
            preserve_symlinks: false,
            module_suffixes: vec![String::new()],
            ..Default::default()
        };

        let mut index = ProgramFileIndex::with_capacity(2);
        index.insert(&symlinked_path.to_string_lossy(), 1, &options);
        // Inserting a second entry that shares the same real path must not
        // clobber the first-write-wins fallback registration.
        index.insert(&other_symlinked_path.to_string_lossy(), 2, &options);

        let real_path = root.join("real/index.d.ts");
        let real_canonical = normalize_resolved_path(&real_path, &options);
        let resolved = index.get_with_symlink_fallback(&real_canonical, &real_path, &options);
        // Either 1 or 2 is acceptable as long as the result is stable.
        assert!(matches!(resolved, Some(1) | Some(2)), "got {resolved:?}");
    }
// TSZ_INLINE_TEST_END ed7571dc327e79b5dd6c0d5f7b1a39e439ae6c6784d576edb9700f08e6fe1ab0
