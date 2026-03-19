// Copyright 2025 tsz authors. All rights reserved.
// MIT License.

use std::path::PathBuf;
use tracing::{info, warn};

use crate::args::CliArgs;
use crate::incremental::BuildInfo;
use crate::project_refs::ResolvedProject;

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

    // Discover all TypeScript source files in the project
    // Note: out_dir is passed so output files are excluded from discovery
    let discovery_options = FileDiscoveryOptions {
        base_dir: root_dir.clone(),
        files: Vec::new(),
        files_explicitly_set: false,
        include: None,
        exclude: None,
        out_dir: project.out_dir.clone(),
        follow_links: false,
        allow_js: false,
    };

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
        let project_dir = reference
            .config_path
            .parent()
            .unwrap_or(reference.config_path.as_path());

        let ref_build_info_path = project_dir.join("tsconfig.tsbuildinfo");

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

                    // Get the modification time of the .d.ts file
                    if let Ok(metadata) = std::fs::metadata(&dts_absolute_path)
                        && let Ok(dts_modified) = metadata.modified()
                    {
                        // Convert the .d.ts modification time to seconds since epoch
                        if let Ok(dts_secs) = dts_modified.duration_since(std::time::UNIX_EPOCH) {
                            let dts_timestamp = dts_secs.as_secs();

                            // Compare with our build time
                            if dts_timestamp > build_info.build_time {
                                if args.build_verbose {
                                    let project_name = reference
                                        .config_path
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("unknown");
                                    info!(
                                        "Referenced project's .d.ts is newer: {} ({} > {})",
                                        project_name, dts_timestamp, build_info.build_time
                                    );
                                }
                                return false;
                            }
                        }
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
fn get_build_info_path(project: &ResolvedProject) -> Option<PathBuf> {
    use crate::incremental::default_build_info_path;

    // Use the same logic as incremental.rs
    let out_dir = project.out_dir.as_deref();
    Some(default_build_info_path(&project.config_path, out_dir))
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
        let dir = tempfile::Builder::new()
            .prefix(&format!("tsz_build_{name}_"))
            .tempdir()
            .unwrap();
        dir
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
}
