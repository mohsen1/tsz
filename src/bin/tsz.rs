#![allow(clippy::print_stderr)]

use anyhow::{Context, Result};
use clap::Parser;
use rustc_hash::FxHashMap;
use std::ffi::OsString;
use std::io::IsTerminal;
use std::time::Duration;

use wasm::cli::args::CliArgs;
use wasm::cli::{driver, locale, reporter::Reporter, watch};

/// tsc exit status codes (matching TypeScript's ExitStatus enum)
const EXIT_SUCCESS: i32 = 0;
const EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED: i32 = 1;
const EXIT_DIAGNOSTICS_OUTPUTS_GENERATED: i32 = 2;

fn main() -> Result<()> {
    // Initialize tracing if TSZ_LOG or RUST_LOG is set (zero cost otherwise).
    // Supports TSZ_LOG_FORMAT=tree|json|text (see src/tracing_config.rs).
    wasm::tracing_config::init_tracing();

    // Preprocess args for tsc compatibility:
    // - Convert -v to -V (tsc uses lowercase -v for version, clap uses -V)
    // - Expand @file response files
    let preprocessed = preprocess_args(std::env::args_os().collect());
    let args = CliArgs::parse_from(preprocessed);
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;

    // Initialize locale for i18n message translation
    locale::init_locale(args.locale.as_deref());

    // Handle --init: create tsconfig.json
    if args.init {
        return handle_init(&args, &cwd);
    }

    // Handle --showConfig: print resolved configuration
    if args.show_config {
        return handle_show_config(&args, &cwd);
    }

    // Handle --listFilesOnly: print file list and exit
    if args.list_files_only {
        return handle_list_files_only(&args, &cwd);
    }

    // Handle --all: show all compiler options
    if args.all {
        return handle_all();
    }

    // Handle --build mode
    if args.build {
        return handle_build(&args, &cwd);
    }

    if args.watch {
        return watch::run(&args, &cwd);
    }

    // Initialize tracer if --generateTrace is specified
    let tracer = if args.generate_trace.is_some() {
        let mut t = wasm::cli::trace::Tracer::new();
        // Add process metadata
        let mut meta_args = FxHashMap::default();
        meta_args.insert("name".to_string(), serde_json::json!("tsz"));
        t.metadata("process_name", meta_args);
        Some(t)
    } else {
        None
    };

    // Handle --generateCpuProfile: this is a V8-specific feature not applicable to a native
    // Rust compiler. The flag is accepted for CLI compatibility with tsc but has no effect.
    if let Some(ref _profile_path) = args.generate_cpu_profile {
        eprintln!(
            "The --generateCpuProfile flag is a V8/Node.js feature and is not applicable to tsz (a native Rust compiler). The flag is accepted for compatibility but has no effect."
        );
    }

    let start_time = std::time::Instant::now();
    let result = driver::compile(&args, &cwd)?;
    let elapsed = start_time.elapsed();

    // Write trace file if requested
    if let (Some(ref trace_path), Some(mut tracer)) = (args.generate_trace.as_ref(), tracer) {
        use wasm::cli::trace::categories;

        // Record compilation summary events
        tracer.complete_with_args("Compile", categories::PROGRAM, start_time, elapsed, {
            let mut args = FxHashMap::default();
            args.insert(
                "fileCount".to_string(),
                serde_json::json!(result.files_read.len()),
            );
            args.insert(
                "errorCount".to_string(),
                serde_json::json!(result.diagnostics.len()),
            );
            args.insert(
                "emittedCount".to_string(),
                serde_json::json!(result.emitted_files.len()),
            );
            args
        });

        // Add per-file events for files read
        for file in &result.files_read {
            let mut args = FxHashMap::default();
            args.insert(
                "path".to_string(),
                serde_json::json!(file.display().to_string()),
            );
            tracer.instant_with_args("FileProcessed", categories::IO, args);
        }

        // Write the trace file
        let trace_file = if trace_path.is_dir() {
            trace_path.join("trace.json")
        } else {
            trace_path.to_path_buf()
        };

        if let Err(e) = tracer.write_to_file(&trace_file) {
            eprintln!("Warning: Failed to write trace file: {e}");
        } else {
            eprintln!("Trace written to: {}", trace_file.display());
        }
    }

    // Handle --listFiles: print all files read during compilation
    if args.list_files {
        for file in &result.files_read {
            println!("{}", file.display());
        }
    }

    // Handle --listEmittedFiles: print emitted file list
    if args.list_emitted_files && !result.emitted_files.is_empty() {
        for file in &result.emitted_files {
            println!("TSFILE: {}", file.display());
        }
    }

    // Handle --explainFiles: print files with inclusion reasons
    if args.explain_files {
        for info in &result.file_infos {
            println!("{}", info.path.display());
            for reason in &info.reasons {
                println!("  {}", reason);
            }
        }
    }

    // Handle --traceDependencies: print dependency graph
    if args.trace_dependencies {
        // Note: Full dependency tracing would require access to the dependency map
        // For now, just list all files that were read (which includes dependencies)
        for file in &result.files_read {
            println!("{}", file.display());
        }
    }

    // Handle --diagnostics: print compilation performance info
    if args.diagnostics || args.extended_diagnostics {
        print_diagnostics(&result, elapsed, args.extended_diagnostics);
    }

    if !result.diagnostics.is_empty() {
        let pretty = args
            .pretty
            .unwrap_or_else(|| std::io::stderr().is_terminal());
        let mut reporter = Reporter::new(pretty);
        let output = reporter.render(&result.diagnostics);
        if !output.is_empty() {
            // Use eprint (not eprintln) because render() already includes all newlines
            eprint!("{output}");
        }
    }

    let has_errors = result
        .diagnostics
        .iter()
        .any(|diag| diag.category == wasm::checker::types::diagnostics::DiagnosticCategory::Error);

    if has_errors {
        // Match tsc exit codes:
        // tsc uses exit code 2 when there are errors (DiagnosticsPresent_OutputsGenerated)
        // regardless of whether --noEmit is set. Exit code 1 is only for when emit
        // is explicitly skipped due to errors (noEmitOnError).
        if args.no_emit || !result.emitted_files.is_empty() {
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_GENERATED);
        } else {
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
        }
    }

    std::process::exit(EXIT_SUCCESS);
}

/// Preprocess command-line arguments for tsc compatibility.
///
/// Handles:
/// - `-v` → `-V` conversion (tsc uses lowercase `-v` for version; clap uses `-V`)
/// - `@file` response file expansion (tsc reads args from response files)
fn preprocess_args(args: Vec<OsString>) -> Vec<OsString> {
    let mut result = Vec::with_capacity(args.len());

    for (i, arg) in args.iter().enumerate() {
        let arg_str = arg.to_string_lossy();

        if i == 0 {
            // Always keep the program name as-is
            result.push(arg.clone());
            continue;
        }

        if arg_str == "-v" {
            // tsc uses -v for version; clap uses -V
            result.push(OsString::from("-V"));
        } else if arg_str.starts_with('@') && arg_str.len() > 1 {
            // Response file: @path reads arguments from file
            let path = &arg_str[1..];
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        // Skip empty lines and comments
                        if !trimmed.is_empty() && !trimmed.starts_with('#') {
                            // Split on whitespace, respecting quoted strings
                            // (matching tsc behavior for response files)
                            for part in split_response_line(trimmed) {
                                result.push(OsString::from(part));
                            }
                        }
                    }
                }
                Err(_) => {
                    // If the file can't be read, pass the argument through
                    // (clap will report an unknown argument error)
                    result.push(arg.clone());
                }
            }
        } else {
            result.push(arg.clone());
        }
    }

    result
}

/// Split a response file line into arguments, respecting quoted strings.
///
/// Handles both double (`"`) and single (`'`) quotes. Quotes are stripped
/// from the resulting tokens. Unquoted regions are split on whitespace.
fn split_response_line(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;

    for ch in line.chars() {
        match in_quote {
            Some(q) if ch == q => {
                // Closing quote — end quoted region but don't push yet,
                // there may be more content adjacent (e.g. foo"bar"baz)
                in_quote = None;
            }
            Some(_) => {
                // Inside quotes — take character literally
                current.push(ch);
            }
            None if ch == '"' || ch == '\'' => {
                // Opening quote
                in_quote = Some(ch);
            }
            None if ch.is_ascii_whitespace() => {
                // Unquoted whitespace — flush current token
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            None => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

fn print_diagnostics(result: &driver::CompilationResult, elapsed: Duration, extended: bool) {
    let files_count = result.files_read.len();

    // Count lines by file category, matching tsc's --diagnostics output
    let mut lines_of_library: u64 = 0;
    let mut lines_of_definitions: u64 = 0;
    let mut lines_of_typescript: u64 = 0;
    let mut lines_of_javascript: u64 = 0;
    let mut lines_of_json: u64 = 0;
    let mut lines_of_other: u64 = 0;

    for path in &result.files_read {
        let count = std::fs::read_to_string(path)
            .ok()
            .map(|text| text.lines().count() as u64)
            .unwrap_or(0);
        let name = path.to_string_lossy();
        if name.contains("lib.") && name.ends_with(".d.ts") {
            lines_of_library += count;
        } else if name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts") {
            lines_of_definitions += count;
        } else if name.ends_with(".ts")
            || name.ends_with(".tsx")
            || name.ends_with(".mts")
            || name.ends_with(".cts")
        {
            lines_of_typescript += count;
        } else if name.ends_with(".js")
            || name.ends_with(".jsx")
            || name.ends_with(".mjs")
            || name.ends_with(".cjs")
        {
            lines_of_javascript += count;
        } else if name.ends_with(".json") {
            lines_of_json += count;
        } else {
            lines_of_other += count;
        }
    }

    let errors = result
        .diagnostics
        .iter()
        .filter(|d| d.category == wasm::checker::types::diagnostics::DiagnosticCategory::Error)
        .count();

    println!();
    println!("Files:                         {}", files_count);
    println!("Lines of Library:              {}", lines_of_library);
    println!("Lines of Definitions:          {}", lines_of_definitions);
    println!("Lines of TypeScript:           {}", lines_of_typescript);
    println!("Lines of JavaScript:           {}", lines_of_javascript);
    println!("Lines of JSON:                 {}", lines_of_json);
    println!("Lines of Other:                {}", lines_of_other);
    println!("Errors:                        {}", errors);
    println!(
        "Total time:                    {:.2}s",
        elapsed.as_secs_f64()
    );

    if extended {
        // Use process memory info if available
        let memory_used = get_memory_usage_kb();
        println!(
            "Emitted files:                 {}",
            result.emitted_files.len()
        );
        println!(
            "Total diagnostics:             {}",
            result.diagnostics.len()
        );
        if memory_used > 0 {
            println!("Memory used:                   {}K", memory_used);
        }
    }
}

/// Get current process memory usage in KB (Linux only, returns 0 on other platforms).
fn get_memory_usage_kb() -> u64 {
    // Read from /proc/self/status for RSS on Linux
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        return parts[1].parse::<u64>().ok();
                    }
                }
            }
            None
        })
        .unwrap_or(0)
}

fn handle_init(_args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    let tsconfig_path = cwd.join("tsconfig.json");
    if tsconfig_path.exists() {
        eprintln!(
            "A tsconfig.json file is already defined at: {}",
            tsconfig_path.display()
        );
        std::process::exit(1);
    }

    // Build the tsconfig.json content matching tsc 5.x --init output format
    // Uses JSONC (JSON with comments) which TypeScript supports
    let config = r#"{
  // Visit https://aka.ms/tsconfig to read more about this file
  "compilerOptions": {
    // File Layout
    // "rootDir": "./src",
    // "outDir": "./dist",

    // Environment Settings
    // See also https://aka.ms/tsconfig/module
    "module": "nodenext",
    "target": "esnext",
    "types": [],
    // For nodejs:
    // "lib": ["esnext"],
    // "types": ["node"],
    // and npm install -D @types/node

    // Other Outputs
    "sourceMap": true,
    "declaration": true,
    "declarationMap": true,

    // Stricter Typechecking Options
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,

    // Style Options
    // "noImplicitReturns": true,
    // "noImplicitOverride": true,
    // "noUnusedLocals": true,
    // "noUnusedParameters": true,
    // "noFallthroughCasesInSwitch": true,
    // "noPropertyAccessFromIndexSignature": true,

    // Recommended Options
    "strict": true,
    "jsx": "react-jsx",
    "verbatimModuleSyntax": true,
    "isolatedModules": true,
    "noUncheckedSideEffectImports": true,
    "moduleDetection": "force",
    "skipLibCheck": true,
  }
}
"#;

    std::fs::write(&tsconfig_path, config).with_context(|| {
        format!(
            "failed to write tsconfig.json to {}",
            tsconfig_path.display()
        )
    })?;

    println!(
        "\nCreated a new tsconfig.json\n\n\
You can learn more at https://aka.ms/tsconfig"
    );

    Ok(())
}

fn handle_show_config(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    use wasm::cli::config::{load_tsconfig, resolve_compiler_options};
    use wasm::cli::driver::apply_cli_overrides;

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
        .or_else(|| {
            let default_path = cwd.join("tsconfig.json");
            if default_path.exists() {
                Some(default_path)
            } else {
                None
            }
        });

    let config = if let Some(path) = tsconfig_path.as_ref() {
        Some(load_tsconfig(path)?)
    } else {
        None
    };

    let mut resolved = resolve_compiler_options(
        config
            .as_ref()
            .and_then(|cfg| cfg.compiler_options.as_ref()),
    )?;
    apply_cli_overrides(&mut resolved, args)?;

    // Build compilerOptions as a serde_json::Map for proper JSON output (matching tsc)
    let mut opts = serde_json::Map::new();

    // Language and Environment
    opts.insert(
        "target".into(),
        serde_json::Value::String(format!("{:?}", resolved.printer.target).to_lowercase()),
    );
    opts.insert(
        "module".into(),
        serde_json::Value::String(format!("{:?}", resolved.printer.module).to_lowercase()),
    );

    // Modules
    if let Some(ref module_resolution) = resolved.module_resolution {
        opts.insert(
            "moduleResolution".into(),
            serde_json::Value::String(format!("{:?}", module_resolution).to_lowercase()),
        );
    }
    if let Some(ref out_dir) = resolved.out_dir {
        opts.insert(
            "outDir".into(),
            serde_json::Value::String(out_dir.display().to_string()),
        );
    }
    if let Some(ref root_dir) = resolved.root_dir {
        opts.insert(
            "rootDir".into(),
            serde_json::Value::String(root_dir.display().to_string()),
        );
    }
    if let Some(ref out_file) = resolved.out_file {
        opts.insert(
            "outFile".into(),
            serde_json::Value::String(out_file.display().to_string()),
        );
    }
    if let Some(ref base_url) = resolved.base_url {
        opts.insert(
            "baseUrl".into(),
            serde_json::Value::String(base_url.display().to_string()),
        );
    }
    if let Some(ref declaration_dir) = resolved.declaration_dir {
        opts.insert(
            "declarationDir".into(),
            serde_json::Value::String(declaration_dir.display().to_string()),
        );
    }

    // Strict checks
    opts.insert("strict".into(), resolved.checker.strict.into());
    opts.insert(
        "noImplicitAny".into(),
        resolved.checker.no_implicit_any.into(),
    );
    opts.insert(
        "strictNullChecks".into(),
        resolved.checker.strict_null_checks.into(),
    );
    opts.insert(
        "strictFunctionTypes".into(),
        resolved.checker.strict_function_types.into(),
    );
    opts.insert(
        "strictPropertyInitialization".into(),
        resolved.checker.strict_property_initialization.into(),
    );
    opts.insert(
        "strictBindCallApply".into(),
        resolved.checker.strict_bind_call_apply.into(),
    );
    opts.insert(
        "noImplicitThis".into(),
        resolved.checker.no_implicit_this.into(),
    );
    opts.insert(
        "noImplicitReturns".into(),
        resolved.checker.no_implicit_returns.into(),
    );
    opts.insert(
        "useUnknownInCatchVariables".into(),
        resolved.checker.use_unknown_in_catch_variables.into(),
    );
    opts.insert(
        "noUncheckedIndexedAccess".into(),
        resolved.checker.no_unchecked_indexed_access.into(),
    );
    opts.insert(
        "exactOptionalPropertyTypes".into(),
        resolved.checker.exact_optional_property_types.into(),
    );
    opts.insert(
        "isolatedModules".into(),
        resolved.checker.isolated_modules.into(),
    );
    opts.insert(
        "esModuleInterop".into(),
        resolved.checker.es_module_interop.into(),
    );
    opts.insert(
        "allowSyntheticDefaultImports".into(),
        resolved.checker.allow_synthetic_default_imports.into(),
    );

    // Emit
    opts.insert("declaration".into(), resolved.emit_declarations.into());
    opts.insert("declarationMap".into(), resolved.declaration_map.into());
    opts.insert("sourceMap".into(), resolved.source_map.into());
    opts.insert("noEmit".into(), resolved.no_emit.into());
    opts.insert("noEmitOnError".into(), resolved.no_emit_on_error.into());
    opts.insert(
        "removeComments".into(),
        resolved.printer.remove_comments.into(),
    );
    opts.insert(
        "noEmitHelpers".into(),
        resolved.printer.no_emit_helpers.into(),
    );

    // Other
    opts.insert("incremental".into(), resolved.incremental.into());
    opts.insert("noCheck".into(), resolved.no_check.into());

    // Build top-level JSON object
    let mut top = serde_json::Map::new();
    top.insert("compilerOptions".into(), serde_json::Value::Object(opts));

    // Include files/include/exclude from tsconfig
    if let Some(ref cfg) = config {
        if let Some(ref files) = cfg.files {
            top.insert(
                "files".into(),
                serde_json::Value::Array(
                    files
                        .iter()
                        .map(|f| serde_json::Value::String(f.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(ref include) = cfg.include {
            top.insert(
                "include".into(),
                serde_json::Value::Array(
                    include
                        .iter()
                        .map(|f| serde_json::Value::String(f.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(ref exclude) = cfg.exclude {
            top.insert(
                "exclude".into(),
                serde_json::Value::Array(
                    exclude
                        .iter()
                        .map(|f| serde_json::Value::String(f.clone()))
                        .collect(),
                ),
            );
        }
    }

    let json = serde_json::Value::Object(top);
    println!("{}", serde_json::to_string_pretty(&json).unwrap());

    Ok(())
}

fn handle_list_files_only(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    use wasm::cli::config::{load_tsconfig, resolve_compiler_options};
    use wasm::cli::driver::apply_cli_overrides;
    use wasm::cli::fs::{FileDiscoveryOptions, discover_ts_files};

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
        .or_else(|| {
            let default_path = cwd.join("tsconfig.json");
            if default_path.exists() {
                Some(default_path)
            } else {
                None
            }
        });

    let config = if let Some(path) = tsconfig_path.as_ref() {
        Some(load_tsconfig(path)?)
    } else {
        None
    };

    let mut resolved = resolve_compiler_options(
        config
            .as_ref()
            .and_then(|cfg| cfg.compiler_options.as_ref()),
    )?;
    apply_cli_overrides(&mut resolved, args)?;

    let base_dir = tsconfig_path
        .as_ref()
        .and_then(|p| p.parent())
        .unwrap_or(cwd);

    // Build file list from CLI args or config
    let files: Vec<std::path::PathBuf> = if !args.files.is_empty() {
        args.files.clone()
    } else if let Some(ref cfg) = config {
        cfg.files
            .as_ref()
            .map(|f| f.iter().map(|s| std::path::PathBuf::from(s)).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let discovery = FileDiscoveryOptions {
        base_dir: base_dir.to_path_buf(),
        files,
        include: config.as_ref().and_then(|c| c.include.clone()),
        exclude: config.as_ref().and_then(|c| c.exclude.clone()),
        out_dir: resolved.out_dir.clone(),
        follow_links: false,
        allow_js: resolved.allow_js,
    };

    let files = discover_ts_files(&discovery)?;
    for file in files {
        println!("{}", file.display());
    }

    Ok(())
}

fn handle_all() -> Result<()> {
    use clap::CommandFactory;

    println!("tsz: The TypeScript Compiler - Codename Zang\n");
    println!("ALL COMPILER OPTIONS\n");

    // Use clap to generate the full help text
    let mut cmd = wasm::cli::args::CliArgs::command();
    let help = cmd.render_long_help();
    println!("{}", help);

    println!(
        "\nYou can learn about all of the compiler options at https://www.typescriptlang.org/tsconfig"
    );
    Ok(())
}

fn handle_build(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    use wasm::checker::types::diagnostics::DiagnosticCategory;
    use wasm::cli::build;
    use wasm::cli::project_refs::ProjectReferenceGraph;

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
        .or_else(|| {
            let default_path = cwd.join("tsconfig.json");
            if default_path.exists() {
                Some(default_path)
            } else {
                None
            }
        });

    let Some(ref root_config_path) = tsconfig_path else {
        anyhow::bail!("No tsconfig.json found. Use --project to specify one.");
    };

    // Load project reference graph
    let graph = match ProjectReferenceGraph::load(root_config_path) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Warning: Could not load project references: {e}");
            // Fall back to single project build
            return handle_build_single_project(args, cwd, root_config_path);
        }
    };

    // Handle --clean: delete build artifacts for all projects
    if args.clean {
        return handle_build_clean(&graph, args.build_verbose);
    }

    // Get build order (topologically sorted)
    let build_order: Vec<wasm::cli::project_refs::ProjectId> = match graph.build_order() {
        Ok(order) => order,
        Err(e) => {
            eprintln!("Error: {e}");
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
        .unwrap_or_else(|| std::io::stderr().is_terminal());
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
                    eprint!("{output}");
                }
            }

            // Stop on first error if --stopBuildOnErrors is set
            if args.stop_build_on_errors {
                eprintln!(
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
            "\nBuilt {} project(s), skipped {} up-to-date project(s), {} error(s)",
            built_count, skipped_count, total_errors
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
    graph: &wasm::cli::project_refs::ProjectReferenceGraph,
    verbose: bool,
) -> Result<()> {
    use std::fs;
    use wasm::cli::config::resolve_compiler_options;

    let mut deleted_count = 0;

    for project in graph.projects() {
        let base_dir = &project.root_dir;

        // Delete .tsbuildinfo file
        let buildinfo_path = project.config_path.with_extension("tsbuildinfo");
        if buildinfo_path.exists() {
            fs::remove_file(&buildinfo_path)?;
            if verbose {
                println!("Deleted: {}", buildinfo_path.display());
            }
            deleted_count += 1;
        }

        // Get resolved options to find output directories
        let resolved = resolve_compiler_options(project.config.base.compiler_options.as_ref())?;

        // Delete outDir
        if let Some(ref out_dir) = resolved.out_dir {
            let full_out_dir = base_dir.join(out_dir);
            if full_out_dir.exists() {
                fs::remove_dir_all(&full_out_dir)?;
                if verbose {
                    println!("Deleted: {}", full_out_dir.display());
                }
                deleted_count += 1;
            }
        }

        // Delete declarationDir
        if let Some(ref declaration_dir) = resolved.declaration_dir {
            let full_decl_dir = base_dir.join(declaration_dir);
            if full_decl_dir.exists() {
                fs::remove_dir_all(&full_decl_dir)?;
                if verbose {
                    println!("Deleted: {}", full_decl_dir.display());
                }
                deleted_count += 1;
            }
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
    use wasm::checker::types::diagnostics::DiagnosticCategory;

    let result = driver::compile(args, cwd)?;

    if args.build_verbose {
        println!("Projects in this build: ");
        println!("  * {}", config_path.display());
    }

    if !result.diagnostics.is_empty() {
        let pretty = args
            .pretty
            .unwrap_or_else(|| std::io::stderr().is_terminal());
        let mut reporter = Reporter::new(pretty);
        let output = reporter.render(&result.diagnostics);
        if !output.is_empty() {
            eprint!("{output}");
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

// Keep the old function signature for compatibility but this is now handled by handle_build
#[allow(dead_code)]
fn handle_build_legacy(args: &CliArgs, cwd: &std::path::Path) -> Result<()> {
    let result = driver::compile(args, cwd)?;

    if !result.diagnostics.is_empty() {
        let pretty = args
            .pretty
            .unwrap_or_else(|| std::io::stderr().is_terminal());
        let mut reporter = Reporter::new(pretty);
        let output = reporter.render(&result.diagnostics);
        if !output.is_empty() {
            eprint!("{output}");
        }
    }

    let has_errors = result
        .diagnostics
        .iter()
        .any(|diag| diag.category == wasm::checker::types::diagnostics::DiagnosticCategory::Error);

    if has_errors {
        if args.no_emit || !result.emitted_files.is_empty() {
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_GENERATED);
        } else {
            std::process::exit(EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_response_line_simple() {
        assert_eq!(
            split_response_line("--strict --noEmit"),
            vec!["--strict", "--noEmit"]
        );
    }

    #[test]
    fn split_response_line_double_quoted_spaces() {
        assert_eq!(
            split_response_line(r#"--outDir "my output""#),
            vec!["--outDir", "my output"]
        );
    }

    #[test]
    fn split_response_line_single_quoted_spaces() {
        assert_eq!(
            split_response_line("--outDir 'my output'"),
            vec!["--outDir", "my output"]
        );
    }

    #[test]
    fn split_response_line_single_arg() {
        assert_eq!(split_response_line("--strict"), vec!["--strict"]);
    }

    #[test]
    fn split_response_line_empty() {
        let empty: Vec<String> = Vec::new();
        assert_eq!(split_response_line(""), empty);
    }

    #[test]
    fn split_response_line_only_whitespace() {
        let empty: Vec<String> = Vec::new();
        assert_eq!(split_response_line("   "), empty);
    }

    #[test]
    fn split_response_line_quoted_path_with_spaces() {
        assert_eq!(
            split_response_line(r#"--rootDir "C:\Program Files\project""#),
            vec!["--rootDir", r"C:\Program Files\project"]
        );
    }

    #[test]
    fn split_response_line_multiple_quoted_args() {
        assert_eq!(
            split_response_line(r#""file one.ts" "file two.ts""#),
            vec!["file one.ts", "file two.ts"]
        );
    }

    #[test]
    fn split_response_line_adjacent_quotes() {
        // foo"bar"baz should produce foobarbaz (quotes just delimit, no split)
        assert_eq!(split_response_line(r#"foo"bar"baz"#), vec!["foobarbaz"]);
    }
}
