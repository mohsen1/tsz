// Copyright 2025 tsz authors. All rights reserved.
// MIT License.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

use crate::cli::args::CliArgs;
use crate::cli::incremental::BuildInfo;
use crate::cli::project_refs::{ProjectReferenceGraph, ResolvedProject};

/// Build mode orchestrator for TypeScript project references.
///
/// This is the entry point for `--build` mode, which:
/// 1. Loads the project reference graph
/// 2. Determines build order via topological sort
/// 3. Checks up-to-date status for each project
/// 4. Compiles dirty projects in dependency order
pub fn build_solution(args: &CliArgs, cwd: &Path, _root_names: &[String]) -> Result<bool> {
    // Determine root tsconfig path
    let root_config = if let Some(project) = args.project.as_deref() {
        cwd.join(project)
    } else {
        // Find tsconfig.json in current directory
        find_tsconfig(cwd).ok_or_else(|| {
            anyhow::anyhow!("No tsconfig.json found in {}", cwd.display())
        })?
    };

    info!("Loading project reference graph from: {}", root_config.display());

    // Load project reference graph
    let graph = ProjectReferenceGraph::load(&root_config)
        .context("Failed to load project reference graph")?;

    // Get build order (topological sort)
    let build_order = graph.build_order()
        .context("Failed to determine build order (circular dependencies?)")?;

    info!("Build order: {} projects", build_order.len());

    // Track overall success
    let mut all_success = true;
    let mut all_diagnostics = Vec::new();

    // Build projects in dependency order
    for project_id in build_order {
        let project = graph.get_project(project_id)
            .ok_or_else(|| anyhow::anyhow!("Project not found: {:?}", project_id))?;

        // Check if project is up-to-date
        if !args.force && is_project_up_to_date(project, args) {
            info!("✓ Project is up to date: {}", project.config_path.display());
            continue;
        }

        info!("Building project: {}", project.config_path.display());

        // Compile this project
        let result = crate::cli::driver::compile_project(args, &project.root_dir, &project.config_path)
            .with_context(|| format!("Failed to build project: {}", project.config_path.display()))?;

        // Collect diagnostics
        if !result.diagnostics.is_empty() {
            all_diagnostics.extend(result.diagnostics.clone());

            // Check for errors
            let has_errors = result.diagnostics.iter()
                .any(|d| d.category == crate::checker::types::diagnostics::DiagnosticCategory::Error);

            if has_errors {
                all_success = false;
                warn!("✗ Project has errors: {}", project.config_path.display());

                // Stop on first error unless --force
                if !args.force {
                    // Print diagnostics
                    for diag in &result.diagnostics {
                        eprintln!("  {:?}", diag);
                    }
                    return Ok(false);
                }
            } else {
                info!("✓ Project built with warnings: {}", project.config_path.display());
            }
        } else {
            info!("✓ Project built successfully: {}", project.config_path.display());
        }
    }

    // Print all diagnostics at the end
    if !all_diagnostics.is_empty() {
        eprintln!("\n=== Diagnostics ===");
        for diag in &all_diagnostics {
            eprintln!("{:?}", diag);
        }
    }

    Ok(all_success)
}

/// Check if a project is up-to-date by examining its .tsbuildinfo file
/// and the outputs of its referenced projects.
pub fn is_project_up_to_date(project: &ResolvedProject, args: &CliArgs) -> bool {
    use crate::cli::incremental::ChangeTracker;
    use crate::cli::fs::{discover_ts_files, FileDiscoveryOptions};

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
                warn!("Failed to load BuildInfo from {}: {}", build_info_path.display(), e);
            }
            return false;
        }
    };

    // Check if source files have changed using ChangeTracker
    let root_dir = &project.root_dir;

    // Discover all TypeScript source files in the project
    let discovery_options = FileDiscoveryOptions {
        base_dir: root_dir.clone(),
        files: Vec::new(),
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: false,
    };

    let current_files = match discover_ts_files(&discovery_options) {
        Ok(files) => files,
        Err(e) => {
            if args.build_verbose {
                warn!("Failed to discover source files in {}: {}", root_dir.display(), e);
            }
            // If we can't scan files, assume we need to rebuild
            return false;
        }
    };

    // Use ChangeTracker to detect modifications
    let mut tracker = ChangeTracker::new();
    if let Err(e) = tracker.compute_changes(&build_info, &current_files) {
        if args.build_verbose {
            warn!("Failed to compute changes: {}", e);
        }
        return false;
    }

    if tracker.has_changes() {
        if args.build_verbose {
            info!("Project has changes: {} changed, {} new, {} deleted",
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
    _build_info: &BuildInfo,
    args: &CliArgs,
) -> bool {
    // For each referenced project
    for reference in &project.resolved_references {
        let project_dir = reference.config_path.parent()
            .unwrap_or(reference.config_path.as_path());

        let ref_build_info_path = project_dir.join("tsconfig.tsbuildinfo");

        if !ref_build_info_path.exists() {
            if args.build_verbose {
                let project_name = reference.config_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");
                info!("Referenced project not built: {}", project_name);
            }
            return false;
        }

        match BuildInfo::load(&ref_build_info_path) {
            Ok(Some(_ref_build_info)) => {
                // TODO: Check latestChangedDtsFile field (Phase 3)
                // For now, skip the timestamp check
            }
            Ok(None) => {
                if args.build_verbose {
                    let project_name = reference.config_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown");
                    info!("Referenced project has version mismatch: {}", project_name);
                }
                return false;
            }
            Err(e) => {
                if args.build_verbose {
                    let project_name = reference.config_path
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
    // TODO: This should match the logic in incremental.rs
    // For now, use a simple heuristic: tsconfig.tsbuildinfo next to the config file
    let config_dir = project.config_path.parent()?;
    let config_name = project.config_path.file_stem()?.to_str()?;
    Some(config_dir.join(format!("{}.tsbuildinfo", config_name)))
}

/// Find a tsconfig.json file in the given directory
fn find_tsconfig(dir: &Path) -> Option<PathBuf> {
    let config = dir.join("tsconfig.json");
    if config.exists() {
        Some(config)
    } else {
        None
    }
}
