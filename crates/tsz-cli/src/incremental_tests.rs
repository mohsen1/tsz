use super::*;
use tempfile::TempDir;

#[test]
fn test_build_info_roundtrip() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let path = temp.path().join("test.tsbuildinfo");

    let mut build_info = BuildInfo::new();
    build_info.root_files = vec!["src/index.ts".to_string()];
    build_info.set_file_info(
        "src/index.ts",
        FileInfo {
            version: "abc123".to_string(),
            signature: Some("sig456".to_string()),
            affected_files_pending_emit: false,
            implied_format: None,
        },
    );
    build_info.set_dependencies("src/index.ts", vec!["src/utils.ts".to_string()]);

    // Save
    build_info.save(&path).unwrap();

    // Load
    let loaded = BuildInfo::load(&path).unwrap();
    let loaded = loaded.expect("Should load valid build info");

    assert_eq!(loaded.root_files, build_info.root_files);
    assert_eq!(loaded.file_infos.len(), 1);
    assert!(loaded.file_infos.contains_key("src/index.ts"));
}

#[test]
fn test_file_change_detection() {
    let mut build_info = BuildInfo::new();
    build_info.set_file_info(
        "src/index.ts",
        FileInfo {
            version: "v1".to_string(),
            signature: None,
            affected_files_pending_emit: false,
            implied_format: None,
        },
    );

    // Same version - not changed
    assert!(!build_info.has_file_changed("src/index.ts", "v1"));

    // Different version - changed
    assert!(build_info.has_file_changed("src/index.ts", "v2"));

    // New file - changed
    assert!(build_info.has_file_changed("src/new.ts", "v1"));
}

#[test]
fn test_dependent_tracking() {
    let mut build_info = BuildInfo::new();

    // index.ts depends on utils.ts
    build_info.set_dependencies("src/index.ts", vec!["src/utils.ts".to_string()]);
    // main.ts also depends on utils.ts
    build_info.set_dependencies("src/main.ts", vec!["src/utils.ts".to_string()]);

    let dependents = build_info.get_dependents("src/utils.ts");
    assert_eq!(dependents.len(), 2);
    assert!(dependents.contains(&"src/index.ts".to_string()));
    assert!(dependents.contains(&"src/main.ts".to_string()));
}

#[test]
fn test_change_tracker() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");

    // Create test files
    let file1 = temp.path().join("file1.ts");
    let file2 = temp.path().join("file2.ts");
    std::fs::write(&file1, "content1").unwrap();
    std::fs::write(&file2, "content2").unwrap();

    // Build info with file1
    let mut build_info = BuildInfo::new();
    let version1 = compute_file_version(&file1).unwrap();
    build_info.set_file_info(
        "file1.ts",
        FileInfo {
            version: version1,
            signature: None,
            affected_files_pending_emit: false,
            implied_format: None,
        },
    );

    // Track changes - file2 is new
    let mut tracker = ChangeTracker::new();
    tracker
        .compute_changes(&build_info, &[file1.clone(), file2.clone()])
        .unwrap();

    assert!(tracker.new_files().contains(&file2));
    assert!(!tracker.changed_files().contains(&file1));
    assert!(tracker.has_changes());
}

#[test]
fn test_build_info_builder() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let file = temp.path().join("test.ts");
    std::fs::write(&file, "export const x = 1;").unwrap();

    let mut builder = BuildInfoBuilder::new(temp.path().to_path_buf());
    builder
        .set_root_files(vec!["test.ts".to_string()])
        .add_file(&file, &["x".to_string()])
        .unwrap()
        .set_file_dependencies(&file, vec![]);

    let build_info = builder.build();

    assert_eq!(build_info.root_files, vec!["test.ts"]);
    assert!(build_info.file_infos.contains_key("test.ts"));
    assert!(build_info.file_infos["test.ts"].signature.is_some());
}

#[test]
fn test_default_build_info_path() {
    let config = Path::new("/project/tsconfig.json");

    // Without outDir
    let path = default_build_info_path(config, None);
    assert_eq!(path, PathBuf::from("/project/tsconfig.tsbuildinfo"));

    // With outDir
    let path = default_build_info_path(config, Some(Path::new("/project/dist")));
    assert_eq!(path, PathBuf::from("/project/dist/tsconfig.tsbuildinfo"));
}

#[test]
fn test_build_info_version_mismatch_returns_none() {
    use tempfile::TempDir;

    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let path = temp.path().join("test.tsbuildinfo");

    // Create a build info with wrong version
    let build_info = BuildInfo {
        version: "0.0.0-wrong".to_string(), // Wrong version
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
        ..Default::default()
    };
    build_info.save(&path).unwrap();

    // Loading should return Ok(None) for version mismatch
    let result = BuildInfo::load(&path).unwrap();
    assert!(result.is_none(), "Version mismatch should return None");
}

#[test]
fn test_build_info_compiler_version_mismatch_returns_none() {
    use tempfile::TempDir;

    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let path = temp.path().join("test.tsbuildinfo");

    // Create a build info with wrong compiler version
    let build_info = BuildInfo {
        version: BUILD_INFO_VERSION.to_string(),
        compiler_version: "0.0.0-wrong".to_string(), // Wrong version
        ..Default::default()
    };
    build_info.save(&path).unwrap();

    // Loading should return Ok(None) for compiler version mismatch
    let result = BuildInfo::load(&path).unwrap();
    assert!(
        result.is_none(),
        "Compiler version mismatch should return None"
    );
}

#[test]
fn test_build_info_valid_versions_returns_some() {
    use tempfile::TempDir;

    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let path = temp.path().join("test.tsbuildinfo");

    // Create a valid build info
    let build_info = BuildInfo {
        version: BUILD_INFO_VERSION.to_string(),
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
        root_files: vec!["src/index.ts".to_string()],
        ..Default::default()
    };
    build_info.save(&path).unwrap();

    // Loading should return Ok(Some(build_info))
    let result = BuildInfo::load(&path).unwrap();
    assert!(result.is_some(), "Valid build info should return Some");

    let loaded = result.expect("operation should succeed in test");
    assert_eq!(loaded.version, BUILD_INFO_VERSION);
    assert_eq!(loaded.compiler_version, env!("CARGO_PKG_VERSION"));
}
