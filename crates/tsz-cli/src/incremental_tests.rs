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

#[test]
fn test_build_info_save_creates_parent_dir_and_preserves_optional_fields() {
    use tempfile::TempDir;

    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let path = temp.path().join("nested/output/test.tsbuildinfo");

    let build_info = BuildInfo {
        version: BUILD_INFO_VERSION.to_string(),
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
        latest_changed_dts_file: Some("types/index.d.ts".to_string()),
        options: BuildInfoOptions {
            target: Some("es2022".to_string()),
            module: Some("commonjs".to_string()),
            declaration: Some(true),
            strict: Some(true),
        },
        ..Default::default()
    };

    build_info.save(&path).unwrap();
    assert!(path.exists());

    let loaded = BuildInfo::load(&path).unwrap().unwrap();
    assert_eq!(
        loaded.latest_changed_dts_file.as_deref(),
        Some("types/index.d.ts")
    );
    assert_eq!(loaded.options.target.as_deref(), Some("es2022"));
    assert_eq!(loaded.options.module.as_deref(), Some("commonjs"));
    assert_eq!(loaded.options.declaration, Some(true));
    assert_eq!(loaded.options.strict, Some(true));
}

#[test]
fn test_change_tracker_marks_dependents_of_changed_files_as_affected() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let changed = temp.path().join("changed.ts");
    let dependent = temp.path().join("dependent.ts");
    std::fs::write(&changed, "export const value = 2;").unwrap();
    std::fs::write(&dependent, "import { value } from './changed';\n").unwrap();

    let dependent_version = compute_file_version(&dependent).unwrap();
    let mut build_info = BuildInfo::new();
    build_info.set_file_info(
        &changed.to_string_lossy(),
        FileInfo {
            version: "stale-version".to_string(),
            signature: None,
            affected_files_pending_emit: false,
            implied_format: None,
        },
    );
    build_info.set_file_info(
        &dependent.to_string_lossy(),
        FileInfo {
            version: dependent_version,
            signature: None,
            affected_files_pending_emit: false,
            implied_format: None,
        },
    );
    build_info.set_dependencies(
        &dependent.to_string_lossy(),
        vec![changed.to_string_lossy().into_owned()],
    );

    let mut tracker = ChangeTracker::new();
    tracker
        .compute_changes(&build_info, &[changed.clone(), dependent.clone()])
        .unwrap();

    assert!(tracker.changed_files().contains(&changed));
    assert!(tracker.affected_files().contains(&changed));
    assert!(tracker.affected_files().contains(&dependent));
    assert!(tracker.has_changes());
}

#[test]
fn test_change_tracker_marks_dependents_of_deleted_files_as_affected() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let deleted = temp.path().join("deleted.ts");
    let dependent = temp.path().join("dependent.ts");
    std::fs::write(&dependent, "import './deleted';\n").unwrap();

    let dependent_version = compute_file_version(&dependent).unwrap();
    let mut build_info = BuildInfo::new();
    build_info.set_file_info(
        &deleted.to_string_lossy(),
        FileInfo {
            version: "old-version".to_string(),
            signature: None,
            affected_files_pending_emit: false,
            implied_format: None,
        },
    );
    build_info.set_file_info(
        &dependent.to_string_lossy(),
        FileInfo {
            version: dependent_version,
            signature: None,
            affected_files_pending_emit: false,
            implied_format: None,
        },
    );
    build_info.set_dependencies(
        &dependent.to_string_lossy(),
        vec![deleted.to_string_lossy().into_owned()],
    );

    let mut tracker = ChangeTracker::new();
    tracker
        .compute_changes(&build_info, std::slice::from_ref(&dependent))
        .unwrap();

    assert!(tracker.deleted_files().contains(&deleted));
    assert!(tracker.affected_files().contains(&dependent));
    assert!(!tracker.changed_files().contains(&dependent));
    assert!(tracker.has_changes());
}

#[test]
fn test_compute_changes_with_base_ignores_files_outside_base_dir() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let base = temp.path().join("project");
    let inside = base.join("src/index.ts");
    let outside = temp.path().join("external.ts");
    std::fs::create_dir_all(inside.parent().unwrap()).unwrap();
    std::fs::write(&inside, "export const inside = 1;\n").unwrap();
    std::fs::write(&outside, "export const outside = 1;\n").unwrap();

    let mut build_info = BuildInfo::new();
    build_info.set_file_info(
        "src/index.ts",
        FileInfo {
            version: "stale-version".to_string(),
            signature: None,
            affected_files_pending_emit: false,
            implied_format: None,
        },
    );

    let mut tracker = ChangeTracker::new();
    tracker
        .compute_changes_with_base(&build_info, &[inside.clone(), outside.clone()], &base)
        .unwrap();

    assert!(tracker.changed_files().contains(&inside));
    assert!(tracker.affected_files().contains(&inside));
    assert!(!tracker.new_files().contains(&outside));
    assert!(!tracker.changed_files().contains(&outside));
}

#[test]
fn test_build_info_builder_normalizes_relative_paths_and_preserves_options() {
    let temp = TempDir::new().expect("temp dir creation should succeed in test");
    let base_dir = temp.path().join("project");
    let file = base_dir.join("src/nested/main.ts");
    let dep = base_dir.join("src/shared/dep.ts");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::create_dir_all(dep.parent().unwrap()).unwrap();
    std::fs::write(&file, "export const main = 1;\n").unwrap();
    std::fs::write(&dep, "export const dep = 1;\n").unwrap();

    let mut builder = BuildInfoBuilder::new(base_dir.clone());
    builder
        .set_root_files(vec!["src/nested/main.ts".to_string()])
        .set_options(BuildInfoOptions {
            target: Some("es2020".to_string()),
            module: Some("esnext".to_string()),
            declaration: Some(false),
            strict: Some(true),
        })
        .add_file(&file, &["main".to_string()])
        .unwrap()
        .set_file_dependencies(&file, vec![dep.clone()]);

    let build_info = builder.build();

    assert!(build_info.file_infos.contains_key("src/nested/main.ts"));
    assert_eq!(
        build_info
            .dependencies
            .get("src/nested/main.ts")
            .map(Vec::as_slice),
        Some(&["src/shared/dep.ts".to_string()][..])
    );
    assert_eq!(build_info.options.target.as_deref(), Some("es2020"));
    assert_eq!(build_info.options.module.as_deref(), Some("esnext"));
    assert_eq!(build_info.options.declaration, Some(false));
    assert_eq!(build_info.options.strict, Some(true));
}
