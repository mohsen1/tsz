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
