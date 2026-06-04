fn list_files_only_unsupported_js_root_diagnostics(
    discovery: &tsz_cli::fs::FileDiscoveryOptions,
    files: &[std::path::PathBuf],
    files_from_config: bool,
) -> Vec<tsz::checker::diagnostics::Diagnostic> {
    use tsz::checker::diagnostics::{Diagnostic, diagnostic_codes};
    use tsz_common::file_extensions::is_js_file;

    if discovery.allow_js || !discovery.files_explicitly_set {
        return Vec::new();
    }

    files
        .iter()
        .filter(|file| is_js_file(file))
        .map(|file| {
            let file_name = file.display().to_string();
            let mut diagnostic = Diagnostic::from_code(
                diagnostic_codes::FILE_IS_A_JAVASCRIPT_FILE_DID_YOU_MEAN_TO_ENABLE_THE_ALLOWJS_OPTION,
                "",
                0,
                0,
                &[&file_name],
            );
            diagnostic
                .related_information
                .push(Diagnostic::related_message(
                    diagnostic_codes::THE_FILE_IS_IN_THE_PROGRAM_BECAUSE,
                    String::new(),
                    0,
                    0,
                    "The file is in the program because:",
                ));
            let (code, message): (u32, &str) = if files_from_config {
                (
                    diagnostic_codes::PART_OF_FILES_LIST_IN_TSCONFIG_JSON,
                    "Part of 'files' list in tsconfig.json",
                )
            } else {
                (
                    diagnostic_codes::ROOT_FILE_SPECIFIED_FOR_COMPILATION,
                    "Root file specified for compilation",
                )
            };
            diagnostic
                .related_information
                .push(Diagnostic::related_message(code, String::new(), 0, 0, message));
            diagnostic
        })
        .collect()
}

fn handle_build(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    use tsz::checker::diagnostics::DiagnosticCategory;
    use tsz_cli::build;
    use tsz_cli::project_refs::ProjectReferenceGraph;

    let tsconfig_path = args
        .project
        .as_ref()
        .map(|p| {
            if p.is_dir() {
                p.join("tsconfig.json")
            } else {
                p.clone()
            }
        })
        .unwrap_or_else(|| cwd.join("tsconfig.json"));

    if !tsconfig_path.exists() {
        // Match tsc behavior: TS5083 to stdout, exit code 1
        let display_path = if tsconfig_path.is_absolute() {
            tsconfig_path
        } else {
            cwd.join(&tsconfig_path)
        };
        println!(
            "error TS5083: Cannot read file '{}'.",
            display_path.display()
        );
        std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
    }

    let root_config_path = &tsconfig_path;

    // Load project reference graph
    let graph = match ProjectReferenceGraph::load(root_config_path) {
        Ok(g) => g,
        Err(e) => {
            println!("Warning: Could not load project references: {e}");
            // Fall back to single project build
            return handle_build_single_project(args, cwd, root_config_path);
        }
    };

    // Validate project reference constraints (TS6306, TS6310, TS6202)
    let ref_diagnostics = graph.validate();
    if !ref_diagnostics.is_empty() {
        let _pretty = args
            .pretty
            .unwrap_or_else(|| std::io::stdout().is_terminal());
        for diag in &ref_diagnostics {
            println!("error TS{}: {}", diag.code, diag.message);
        }
        std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
    }

    // Handle --clean: delete build artifacts for all projects
    if args.clean {
        return handle_build_clean(&graph, args.build_verbose);
    }

    // Get build order (topologically sorted)
    let build_order: Vec<tsz_cli::project_refs::ProjectId> = match graph.build_order() {
        Ok(order) => order,
        Err(e) => {
            println!("Error: {e}");
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
        }
    };

    // Handle --dry: show what would be built without building
    if args.dry {
        println!(
            "Dry run - would build {} project(s) in order:",
            build_order.len()
        );
        for (i, project_id) in build_order.iter().enumerate() {
            if let Some(project) = graph.get_project(*project_id) {
                println!("  {}. {}", i + 1, project.config_path.display());
            }
        }
        return Ok(());
    }

    // Build each project in dependency order
    let mut total_errors = 0;
    let mut built_count = 0;
    let mut skipped_count = 0;
    let pretty = args
        .pretty
        .unwrap_or_else(|| std::io::stdout().is_terminal());
    if args.pretty == Some(true) {
        Reporter::force_colors(true);
    }
    let mut reporter = Reporter::new(pretty);

    if args.build_verbose {
        println!("Checking {} project(s)...", build_order.len());
    }

    for project_id in &build_order {
        let Some(project) = graph.get_project(*project_id) else {
            continue;
        };

        // Check if project is up-to-date (unless --force is set)
        if !args.force && build::is_project_up_to_date(project, args) {
            if args.build_verbose {
                println!("✓ Up to date: {}", project.config_path.display());
            }
            skipped_count += 1;
            continue;
        }

        if args.build_verbose {
            println!("\nBuilding: {}", project.config_path.display());
        }

        // Compile the project using the project-specific tsconfig
        let project_cwd = project.root_dir.clone();

        // Use driver::compile_project which accepts the tsconfig path directly
        let result = driver::compile_project(args, &project_cwd, &project.config_path)?;

        // Count errors
        let error_count = result
            .diagnostics
            .iter()
            .filter(|d| d.category == DiagnosticCategory::Error)
            .count();

        if error_count > 0 {
            total_errors += error_count;
            if !result.diagnostics.is_empty() {
                let output = reporter.render(&result.diagnostics);
                if !output.is_empty() {
                    print!("{output}");
                }
            }

            // Stop on first error if --stopBuildOnErrors is set
            if args.stop_build_on_errors {
                println!(
                    "\nBuild stopped due to errors in {}",
                    project.config_path.display()
                );
                std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
            }
        }

        built_count += 1;
    }

    if args.build_verbose {
        println!(
            "\nBuilt {built_count} project(s), skipped {skipped_count} up-to-date project(s), {total_errors} error(s)"
        );
    }

    if total_errors > 0 {
        std::process::exit(if built_count > 0 {
            EXIT_DIAGNOSTICS_OUTPUTS_GENERATED
        } else {
            EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED
        });
    }

    Ok(())
}

/// Handle --build --clean for all projects in the graph
fn handle_build_clean(
    graph: &tsz_cli::project_refs::ProjectReferenceGraph,
    verbose: bool,
) -> Result<()> {
    use std::fs;
    use tsz_cli::build::get_build_info_path;

    let mut deleted_count = 0;

    for project in graph.projects() {
        // Use the same build-info path logic as the build/driver paths so that
        // `--clean` removes the file the build actually wrote. Previously this
        // always wrote next to the tsconfig, which missed the case where
        // `outDir` relocates the .tsbuildinfo file.
        let Some(buildinfo_path) = get_build_info_path(project) else {
            continue;
        };
        if buildinfo_path.exists() {
            fs::remove_file(&buildinfo_path)?;
            if verbose {
                println!("Deleted: {}", buildinfo_path.display());
            }
            deleted_count += 1;
        }

        // `ResolvedProject` already stores absolute out/declaration dirs
        // resolved against `root_dir`, so re-running `resolve_compiler_options`
        // only duplicates work and risks drifting from the build path.
        if let Some(ref out_dir) = project.out_dir
            && out_dir.exists()
        {
            fs::remove_dir_all(out_dir)?;
            if verbose {
                println!("Deleted: {}", out_dir.display());
            }
            deleted_count += 1;
        }

        if let Some(ref declaration_dir) = project.declaration_dir
            && declaration_dir.exists()
        {
            fs::remove_dir_all(declaration_dir)?;
            if verbose {
                println!("Deleted: {}", declaration_dir.display());
            }
            deleted_count += 1;
        }
    }

    println!(
        "Build cleaned successfully ({} project(s), {} item(s) deleted).",
        graph.project_count(),
        deleted_count
    );
    Ok(())
}

/// Fallback to single project build when no references are found
fn handle_build_single_project(
    args: &CliArgs,
    cwd: &std::path::Path,
    config_path: &std::path::Path,
) -> Result<()> {
    use tsz::checker::diagnostics::DiagnosticCategory;

    let result = driver::compile(args, cwd)?;

    if args.build_verbose {
        println!("Projects in this build: ");
        println!("  * {}", config_path.display());
    }

    if !result.diagnostics.is_empty() {
        let pretty = args
            .pretty
            .unwrap_or_else(|| std::io::stdout().is_terminal());
        if args.pretty == Some(true) {
            Reporter::force_colors(true);
        }
        let mut reporter = Reporter::new(pretty);
        let output = reporter.render(&result.diagnostics);
        if !output.is_empty() {
            print!("{output}");
        }
    }

    let has_errors = result
        .diagnostics
        .iter()
        .any(|d| d.category == DiagnosticCategory::Error);

    if has_errors {
        std::process::exit(if result.emitted_files.is_empty() {
            EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED
        } else {
            EXIT_DIAGNOSTICS_OUTPUTS_GENERATED
        });
    }

    Ok(())
}





#[cfg(test)]
use arg_preprocess::split_response_line;
