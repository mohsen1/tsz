// Copyright 2025 tsz authors. All rights reserved.
// MIT License.

use std::path::PathBuf;
use tracing::{info, warn};

use crate::args::CliArgs;
use crate::incremental::BuildInfo;
use crate::project_refs::{ResolvedProject, load_project};

/// Check if a project is up-to-date by examining its .tsbuildinfo file
/// and the outputs of its referenced projects.
pub fn is_project_up_to_date(project: &ResolvedProject, args: &CliArgs) -> bool {
    use crate::fs::{FileDiscoveryOptions, discover_ts_files};
    use crate::incremental::ChangeTracker;

    // Load BuildInfo for this project
    let build_info_path = match get_build_info_path(project) {
        Some(path) => path,
        None => return false,
    };

    if !build_info_path.exists() {
        if args.build_verbose {
            info!("No .tsbuildinfo found at {}", build_info_path.display());
        }
        return false;
    }

    // Try to load BuildInfo
    let build_info = match BuildInfo::load(&build_info_path) {
        Ok(Some(info)) => info,
        Ok(None) => {
            if args.build_verbose {
                info!("BuildInfo version mismatch, needs rebuild");
            }
            return false;
        }
        Err(e) => {
            if args.build_verbose {
                warn!(
                    "Failed to load BuildInfo from {}: {}",
                    build_info_path.display(),
                    e
                );
            }
            return false;
        }
    };

    // Check if source files have changed using ChangeTracker
    let root_dir = &project.root_dir;

    // Discover the configured project root files, rather than doing a fresh
    // default scan. Build mode should not treat unlisted files as new roots.
    let discovery_options = FileDiscoveryOptions::from_tsconfig(
        &project.config_path,
        &project.config.base,
        project.out_dir.as_deref(),
    );

    let current_files = match discover_ts_files(&discovery_options) {
        Ok(files) => files,
        Err(e) => {
            if args.build_verbose {
                warn!(
                    "Failed to discover source files in {}: {}",
                    root_dir.display(),
                    e
                );
            }
            // If we can't scan files, assume we need to rebuild
            return false;
        }
    };

    // Use ChangeTracker to detect modifications
    // Note: We pass absolute paths for file reading, but ChangeTracker compares using relative paths
    let mut tracker = ChangeTracker::new();
    if let Err(e) = tracker.compute_changes_with_base(&build_info, &current_files, root_dir) {
        if args.build_verbose {
            warn!("Failed to compute changes: {}", e);
        }
        return false;
    }

    if tracker.has_changes() {
        if args.build_verbose {
            info!(
                "Project has changes: {} changed, {} new, {} deleted",
                tracker.changed_files().len(),
                tracker.new_files().len(),
                tracker.deleted_files().len()
            );
        }
        return false;
    }

    // Check if referenced projects' outputs are still valid
    if !are_referenced_projects_uptodate(project, &build_info, args) {
        return false;
    }

    true
}

/// Check if all referenced projects are up-to-date
/// by examining their .tsbuildinfo files and output timestamps.
fn are_referenced_projects_uptodate(
    project: &ResolvedProject,
    build_info: &BuildInfo,
    args: &CliArgs,
) -> bool {
    // For each referenced project
    for reference in &project.resolved_references {
        let ref_project = match load_project(&reference.config_path) {
            Ok(project) => project,
            Err(e) => {
                if args.build_verbose {
                    warn!(
                        "Failed to load referenced project {}: {}",
                        reference.config_path.display(),
                        e
                    );
                }
                return false;
            }
        };
        let project_dir = &ref_project.root_dir;
        let Some(ref_build_info_path) = get_build_info_path(&ref_project) else {
            return false;
        };

        if !ref_build_info_path.exists() {
            if args.build_verbose {
                let project_name = reference
                    .config_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");
                info!("Referenced project not built: {}", project_name);
            }
            return false;
        }

        match BuildInfo::load(&ref_build_info_path) {
            Ok(Some(ref_build_info)) => {
                // Check if the referenced project's latest .d.ts file is newer
                // than our build time, which would mean we need to rebuild
                if let Some(ref latest_dts) = ref_build_info.latest_changed_dts_file {
                    // Convert relative path to absolute path
                    let dts_absolute_path = project_dir.join(latest_dts);

                    // The referenced project's BuildInfo names this .d.ts as its
                    // latest declaration output. If we cannot read its modification
                    // time — typically because the file was deleted, replaced, or is
                    // temporarily unreadable — we cannot prove the parent project is
                    // up-to-date, so treat the reference as stale and force a rebuild.
                    let dts_modified = match std::fs::metadata(&dts_absolute_path)
                        .and_then(|metadata| metadata.modified())
                    {
                        Ok(modified) => modified,
                        Err(error) => {
                            if args.build_verbose {
                                let project_name = reference
                                    .config_path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("unknown");
                                info!(
                                    "Referenced project's recorded latest .d.ts is unavailable, treating as stale: {} at {} ({})",
                                    project_name,
                                    dts_absolute_path.display(),
                                    error,
                                );
                            }
                            return false;
                        }
                    };

                    // mtime predating the Unix epoch is essentially unreachable on
                    // real filesystems but we still cannot derive a comparable
                    // timestamp, so treat as stale rather than silently passing.
                    let dts_secs = match dts_modified.duration_since(std::time::UNIX_EPOCH) {
                        Ok(d) => d,
                        Err(_) => return false,
                    };
                    let dts_timestamp = dts_secs.as_secs();

                    // Compare with our build time. We intentionally use `>=`
                    // rather than `>` so that a referenced project that
                    // rebuilds within the same Unix second as our recorded
                    // build_time still forces a parent rebuild — at second
                    // resolution we cannot tell "ref finished a millisecond
                    // before us" from "ref finished a millisecond after",
                    // and in that ambiguity the only safe option is to
                    // rebuild. The "ref dts is genuinely older" path keeps
                    // working because mtime < build_time still produces a
                    // false comparison. See issue #4754.
                    if dts_timestamp >= build_info.build_time {
                        if args.build_verbose {
                            let project_name = reference
                                .config_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("unknown");
                            info!(
                                "Referenced project's .d.ts is newer or same-second: {} ({} >= {})",
                                project_name, dts_timestamp, build_info.build_time
                            );
                        }
                        return false;
                    }
                }
            }
            Ok(None) => {
                if args.build_verbose {
                    let project_name = reference
                        .config_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown");
                    info!("Referenced project has version mismatch: {}", project_name);
                }
                return false;
            }
            Err(e) => {
                if args.build_verbose {
                    let project_name = reference
                        .config_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown");
                    warn!("Failed to load BuildInfo for {}: {}", project_name, e);
                }
                return false;
            }
        }
    }

    true
}

/// Get the path to the .tsbuildinfo file for a project
pub fn get_build_info_path(project: &ResolvedProject) -> Option<PathBuf> {
    use crate::incremental::default_build_info_path;

    if let Some(explicit_path) = project
        .config
        .base
        .compiler_options
        .as_ref()
        .and_then(|opts| opts.ts_build_info_file.as_deref())
        .filter(|path| !path.is_empty())
    {
        return Some(project.root_dir.join(explicit_path));
    }

    // Use the same logic as incremental.rs. rootDir from compilerOptions is
    // resolved relative to the project's tsconfig directory so we can pass an
    // absolute path that matches `tsc`'s `getTsBuildInfoEmitOutputFilePath`.
    let out_dir = project.out_dir.as_deref();
    let root_dir = project
        .config
        .base
        .compiler_options
        .as_ref()
        .and_then(|opts| opts.root_dir.as_ref())
        .map(|rd| project.root_dir.join(rd));
    Some(default_build_info_path(
        &project.config_path,
        out_dir,
        root_dir.as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::incremental::{BuildInfo, BuildInfoBuilder};
    use crate::project_refs::{ProjectReference, ResolvedProjectReference};
    use clap::Parser;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    fn cli_args() -> CliArgs {
        CliArgs::try_parse_from(["tsz"]).unwrap()
    }

    fn create_project_dir(name: &str) -> TempDir {
        tempfile::Builder::new()
            .prefix(&format!("tsz_build_{name}_"))
            .tempdir()
            .unwrap()
    }

    fn write_project_config(dir: &Path) -> PathBuf {
        let config_path = dir.join("tsconfig.json");
        fs::write(&config_path, "{}").unwrap();
        config_path
    }

    fn write_source_file(dir: &Path, relative: &str, content: &str) -> PathBuf {
        let source_path = dir.join(relative);
        if let Some(parent) = source_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&source_path, content).unwrap();
        source_path
    }

    fn write_root_build_info(
        dir: &Path,
        source_path: &Path,
        latest_changed_dts: Option<&str>,
        build_time: Option<u64>,
    ) -> PathBuf {
        let mut builder = BuildInfoBuilder::new(dir.to_path_buf());
        builder.add_file(source_path, &[]).unwrap();
        let mut build_info = builder.build();
        build_info.latest_changed_dts_file = latest_changed_dts.map(|s| s.to_string());
        if let Some(build_time) = build_time {
            build_info.build_time = build_time;
        }

        let build_info_path = dir.join("tsconfig.tsbuildinfo");
        build_info.save(&build_info_path).unwrap();
        build_info_path
    }

    fn write_reference_build_info(dir: &Path, latest_changed_dts: Option<&str>) -> PathBuf {
        let mut build_info = BuildInfo::new();
        build_info.latest_changed_dts_file = latest_changed_dts.map(|s| s.to_string());
        let build_info_path = dir.join("tsconfig.tsbuildinfo");
        build_info.save(&build_info_path).unwrap();
        build_info_path
    }

    fn resolved_reference(config_path: PathBuf) -> ResolvedProjectReference {
        ResolvedProjectReference {
            config_path: config_path.clone(),
            original: ProjectReference {
                path: config_path.to_string_lossy().to_string(),
                prepend: false,
                circular: false,
            },
            is_valid: true,
            error: None,
        }
    }

    fn make_project(
        config_path: PathBuf,
        root_dir: PathBuf,
        resolved_references: Vec<ResolvedProjectReference>,
        out_dir: Option<PathBuf>,
    ) -> ResolvedProject {
        ResolvedProject {
            config_path,
            root_dir,
            config: serde_json::from_str("{}").unwrap(),
            resolved_references,
            is_composite: true,
            no_emit: false,
            declaration_dir: None,
            out_dir,
        }
    }

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
}
