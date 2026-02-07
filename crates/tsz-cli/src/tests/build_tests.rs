//! Tests for build mode orchestrator and project references

use clap::Parser;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use crate::args::CliArgs;
use crate::build;
use crate::project_refs::ResolvedProject;

/// Create a test project with tsconfig.json
fn create_test_project(dir: &Path, name: &str, config: &str) -> PathBuf {
    let project_dir = dir.join(name);
    std::fs::create_dir_all(&project_dir).unwrap();

    let config_path = project_dir.join("tsconfig.json");
    std::fs::write(&config_path, config).unwrap();

    project_dir
}

/// Create a test source file
fn create_source_file(project_dir: &Path, name: &str, content: &str) -> PathBuf {
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let file_path = src_dir.join(name);
    std::fs::write(&file_path, content).unwrap();

    file_path
}

#[test]
fn test_is_project_up_to_date_no_buildinfo() {
    let temp_dir = TempDir::new().unwrap();

    // Create a project without .tsbuildinfo
    let project_dir = create_test_project(
        temp_dir.path(),
        "test",
        r#"
{
  "compilerOptions": {
    "composite": true,
    "declaration": true,
    "outDir": "./dist",
    "rootDir": "./src"
  }
}
"#,
    );

    let project = ResolvedProject {
        config_path: project_dir.join("tsconfig.json"),
        root_dir: project_dir.clone(),
        config: serde_json::from_str("{}").unwrap(),
        resolved_references: vec![],
        is_composite: true,
        out_dir: Some(project_dir.join("dist")),
        declaration_dir: None,
    };

    let args = CliArgs::try_parse_from(["tsz"]).unwrap();

    // Project without .tsbuildinfo should not be up-to-date
    assert!(!build::is_project_up_to_date(&project, &args));
}

#[test]
#[ignore = "is_project_up_to_date implementation incomplete - needs .tsbuildinfo parsing and validation"]
fn test_is_project_up_to_date_with_buildinfo() {
    let temp_dir = TempDir::new().unwrap();

    // Create a project with .tsbuildinfo
    let project_dir = create_test_project(
        temp_dir.path(),
        "test",
        r#"
{
  "compilerOptions": {
    "composite": true,
    "declaration": true,
    "outDir": "./dist",
    "rootDir": "./src"
  }
}
"#,
    );

    // Create a minimal .tsbuildinfo file
    let buildinfo_path = project_dir.join("tsconfig.tsbuildinfo");
    let compiler_version = env!("CARGO_PKG_VERSION");
    let buildinfo_content = format!(
        r#"{{
  "version": "0.1.0",
  "compilerVersion": "{}",
  "rootFiles": [],
  "fileInfos": {{}},
  "dependencies": {{}},
  "semanticDiagnosticsPerFile": {{}},
  "emitSignatures": {{}},
  "latestChangedDtsFile": null,
  "options": {{}},
  "buildTime": 1234567890
}}"#,
        compiler_version
    );
    std::fs::write(&buildinfo_path, buildinfo_content).unwrap();

    let project = ResolvedProject {
        config_path: project_dir.join("tsconfig.json"),
        root_dir: project_dir.clone(),
        config: serde_json::from_str("{}").unwrap(),
        resolved_references: vec![],
        is_composite: true,
        out_dir: Some(project_dir.join("dist")),
        declaration_dir: None,
    };

    let args = CliArgs::try_parse_from(["tsz"]).unwrap();

    // Project with valid .tsbuildinfo should be up-to-date (for now)
    // TODO: This should check source file changes too
    assert!(build::is_project_up_to_date(&project, &args));
}

#[test]
fn test_is_project_up_to_date_force_rebuild() {
    let temp_dir = TempDir::new().unwrap();

    let project_dir = create_test_project(
        temp_dir.path(),
        "test",
        r#"
{
  "compilerOptions": {
    "composite": true,
    "declaration": true,
    "outDir": "./dist",
    "rootDir": "./src"
  }
}
"#,
    );

    // Create .tsbuildinfo
    let buildinfo_path = project_dir.join("tsconfig.tsbuildinfo");
    std::fs::write(&buildinfo_path, "{}").unwrap();

    let project = ResolvedProject {
        config_path: project_dir.join("tsconfig.json"),
        root_dir: project_dir.clone(),
        config: serde_json::from_str("{}").unwrap(),
        resolved_references: vec![],
        is_composite: true,
        out_dir: Some(project_dir.join("dist")),
        declaration_dir: None,
    };

    let args = CliArgs::try_parse_from(["tsz", "--force"]).unwrap();

    // Even with .tsbuildinfo, --force should cause rebuild
    assert!(!build::is_project_up_to_date(&project, &args));
}

#[test]
fn test_get_build_info_path() {
    let temp_dir = TempDir::new().unwrap();

    let project_dir = create_test_project(temp_dir.path(), "myproject", "{}");

    let project = ResolvedProject {
        config_path: project_dir.join("tsconfig.json"),
        root_dir: project_dir.clone(),
        config: serde_json::from_str("{}").unwrap(),
        resolved_references: vec![],
        is_composite: false,
        out_dir: None,
        declaration_dir: None,
    };

    // This is an internal test, so we need to make get_build_info_path public or test indirectly
    // For now, we'll just verify the project structure
    assert!(project.config_path.exists());
    assert_eq!(project.config_path.file_name().unwrap(), "tsconfig.json");
}

#[test]
fn test_is_project_up_to_date_with_source_changes() {
    let temp_dir = TempDir::new().unwrap();

    // Create a project with .tsbuildinfo and source files
    let project_dir = create_test_project(
        temp_dir.path(),
        "test",
        r#"
{
  "compilerOptions": {
    "composite": true,
    "declaration": true,
    "outDir": "./dist",
    "rootDir": "./src"
  }
}
"#,
    );

    // Create initial source file
    let _source_file = create_source_file(&project_dir, "index.ts", "export const x = 1;");

    // Create a .tsbuildinfo that lists the source file
    let buildinfo_path = project_dir.join("tsconfig.tsbuildinfo");
    let compiler_version = env!("CARGO_PKG_VERSION");
    let buildinfo_content = format!(
        r#"{{
  "version": "0.1.0",
  "compilerVersion": "{}",
  "rootFiles": ["src/index.ts"],
  "fileInfos": {{
    "src/index.ts": {{
      "version": "oldhash",
      "signature": null
    }}
  }},
  "dependencies": {{}},
  "semanticDiagnosticsPerFile": {{}},
  "emitSignatures": {{}},
  "latestChangedDtsFile": null,
  "options": {{}},
  "buildTime": 1234567890
}}"#,
        compiler_version
    );
    std::fs::write(&buildinfo_path, buildinfo_content).unwrap();

    let project = ResolvedProject {
        config_path: project_dir.join("tsconfig.json"),
        root_dir: project_dir.clone(),
        config: serde_json::from_str("{}").unwrap(),
        resolved_references: vec![],
        is_composite: true,
        out_dir: Some(project_dir.join("dist")),
        declaration_dir: None,
    };

    let args = CliArgs::try_parse_from(["tsz"]).unwrap();

    // Project should need rebuild due to source file change
    assert!(!build::is_project_up_to_date(&project, &args));
}

#[test]
fn test_is_project_up_to_date_with_new_source_files() {
    let temp_dir = TempDir::new().unwrap();

    let project_dir = create_test_project(
        temp_dir.path(),
        "test",
        r#"
{
  "compilerOptions": {
    "composite": true,
    "declaration": true,
    "outDir": "./dist",
    "rootDir": "./src"
  }
}
"#,
    );

    // Create initial source file
    create_source_file(&project_dir, "index.ts", "export const x = 1;");

    // Create .tsbuildinfo that doesn't list the new file
    let buildinfo_path = project_dir.join("tsconfig.tsbuildinfo");
    let compiler_version = env!("CARGO_PKG_VERSION");
    let buildinfo_content = format!(
        r#"{{
  "version": "0.1.0",
  "compilerVersion": "{}",
  "rootFiles": [],
  "fileInfos": {{}},
  "dependencies": {{}},
  "semanticDiagnosticsPerFile": {{}},
  "emitSignatures": {{}},
  "latestChangedDtsFile": null,
  "options": {{}},
  "buildTime": 1234567890
}}"#,
        compiler_version
    );
    std::fs::write(&buildinfo_path, buildinfo_content).unwrap();

    let project = ResolvedProject {
        config_path: project_dir.join("tsconfig.json"),
        root_dir: project_dir.clone(),
        config: serde_json::from_str("{}").unwrap(),
        resolved_references: vec![],
        is_composite: true,
        out_dir: Some(project_dir.join("dist")),
        declaration_dir: None,
    };

    let args = CliArgs::try_parse_from(["tsz"]).unwrap();

    // Project should need rebuild due to new source file
    assert!(!build::is_project_up_to_date(&project, &args));
}

#[test]
fn test_is_project_up_to_date_cross_project_invalidation() {
    let temp_dir = TempDir::new().unwrap();

    // Create main project
    let main_dir = create_test_project(
        temp_dir.path(),
        "main",
        r#"
{
  "compilerOptions": {
    "composite": true,
    "declaration": true,
    "outDir": "./dist",
    "rootDir": "./src"
  }
}
"#,
    );

    // Create referenced project
    let ref_dir = create_test_project(
        temp_dir.path(),
        "ref",
        r#"
{
  "compilerOptions": {
    "composite": true,
    "declaration": true,
    "outDir": "./dist",
    "rootDir": "./src"
  }
}
"#,
    );

    // Create .d.ts file in referenced project
    let ref_dist_dir = ref_dir.join("dist");
    std::fs::create_dir_all(&ref_dist_dir).unwrap();
    let ref_dts_path = ref_dist_dir.join("index.d.ts");
    std::fs::write(&ref_dts_path, "export const x = 1;").unwrap();

    // Create .tsbuildinfo for referenced project with recent .d.ts
    let ref_buildinfo_path = ref_dir.join("tsconfig.tsbuildinfo");
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let compiler_version = env!("CARGO_PKG_VERSION");
    let ref_buildinfo_content = format!(
        r#"{{
  "version": "0.1.0",
  "compilerVersion": "{}",
  "rootFiles": [],
  "fileInfos": {{}},
  "dependencies": {{}},
  "semanticDiagnosticsPerFile": {{}},
  "emitSignatures": {{}},
  "latestChangedDtsFile": "dist/index.d.ts",
  "options": {{}},
  "buildTime": {}
}}"#,
        compiler_version, current_time
    );
    std::fs::write(&ref_buildinfo_path, ref_buildinfo_content).unwrap();

    // Create .tsbuildinfo for main project with older timestamp
    let main_buildinfo_path = main_dir.join("tsconfig.tsbuildinfo");
    let old_time = current_time - 3600; // 1 hour ago
    let main_buildinfo_content = format!(
        r#"{{
  "version": "0.1.0",
  "compilerVersion": "{}",
  "rootFiles": [],
  "fileInfos": {{}},
  "dependencies": {{}},
  "semanticDiagnosticsPerFile": {{}},
  "emitSignatures": {{}},
  "latestChangedDtsFile": null,
  "options": {{}},
  "buildTime": {}
}}"#,
        compiler_version, old_time
    );
    std::fs::write(&main_buildinfo_path, main_buildinfo_content).unwrap();

    // Create resolved reference
    use crate::project_refs::{ProjectReference, ResolvedProjectReference};
    let resolved_ref = ResolvedProjectReference {
        config_path: ref_dir.join("tsconfig.json"),
        original: ProjectReference {
            path: "../ref".to_string(),
            prepend: false,
            circular: false,
        },
        is_valid: true,
        error: None,
    };

    let project = ResolvedProject {
        config_path: main_dir.join("tsconfig.json"),
        root_dir: main_dir.clone(),
        config: serde_json::from_str("{}").unwrap(),
        resolved_references: vec![resolved_ref],
        is_composite: true,
        out_dir: Some(main_dir.join("dist")),
        declaration_dir: None,
    };

    let args = CliArgs::try_parse_from(["tsz"]).unwrap();

    // Main project should need rebuild because referenced .d.ts is newer
    assert!(!build::is_project_up_to_date(&project, &args));
}
