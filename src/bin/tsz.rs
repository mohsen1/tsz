use anyhow::{Context, Result};
use clap::Parser;
use std::ffi::OsString;
use std::io::IsTerminal;
use std::time::Duration;

use wasm::cli::args::CliArgs;
use wasm::cli::{driver, reporter::Reporter, watch};

/// tsc exit status codes (matching TypeScript's ExitStatus enum)
const EXIT_SUCCESS: i32 = 0;
const EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED: i32 = 1;
const EXIT_DIAGNOSTICS_OUTPUTS_GENERATED: i32 = 2;

fn main() -> Result<()> {
    // Initialize tracing with RUST_LOG environment variable support
    // Use RUST_LOG=debug for detailed tracing, RUST_LOG=trace for everything
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    // Preprocess args for tsc compatibility:
    // - Convert -v to -V (tsc uses lowercase -v for version, clap uses -V)
    // - Expand @file response files
    let preprocessed = preprocess_args(std::env::args_os().collect());
    let args = CliArgs::parse_from(preprocessed);
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;

    // Handle --init: create tsconfig.json
    if args.init {
        return handle_init(&cwd);
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

    // Handle --generateTrace: generate event trace file
    if let Some(ref trace_path) = args.generate_trace {
        // TODO: Implement full trace generation
        eprintln!(
            "--generateTrace not yet fully implemented, would write to: {}",
            trace_path.display()
        );
    }

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

    // Handle --explainFiles: print files with reasons
    if args.explain_files {
        for file in &result.files_read {
            println!("{}", file.display());
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
        let color = args
            .pretty
            .unwrap_or_else(|| std::io::stdout().is_terminal());
        let mut reporter = Reporter::new(color);
        let output = reporter.render(&result.diagnostics);
        if !output.is_empty() {
            eprintln!("{output}");
        }
    }

    let has_errors = result
        .diagnostics
        .iter()
        .any(|diag| diag.category == wasm::checker::types::diagnostics::DiagnosticCategory::Error);

    if has_errors {
        // Match tsc exit codes:
        // Exit 1 (DiagnosticsPresent_OutputsSkipped): errors found, no output generated
        // Exit 2 (DiagnosticsPresent_OutputsGenerated): errors found, output was still generated
        if !result.emitted_files.is_empty() {
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
        } else if name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
        {
            lines_of_definitions += count;
        } else if name.ends_with(".ts") || name.ends_with(".tsx") || name.ends_with(".mts") || name.ends_with(".cts") {
            lines_of_typescript += count;
        } else if name.ends_with(".js") || name.ends_with(".jsx") || name.ends_with(".mjs") || name.ends_with(".cjs") {
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
    println!("Total time:                    {:.2}s", elapsed.as_secs_f64());

    if extended {
        // Use process memory info if available
        let memory_used = get_memory_usage_kb();
        println!("Emitted files:                 {}", result.emitted_files.len());
        println!("Total diagnostics:             {}", result.diagnostics.len());
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

fn handle_init(cwd: &std::path::Path) -> Result<()> {
    let tsconfig_path = cwd.join("tsconfig.json");
    if tsconfig_path.exists() {
        eprintln!(
            "A tsconfig.json file is already defined at: {}",
            tsconfig_path.display()
        );
        std::process::exit(1);
    }

    let default_config = r#"{
  "compilerOptions": {
    /* Visit https://aka.ms/tsconfig to read more about this file */

    /* Projects */
    // "incremental": true,                              /* Save .tsbuildinfo files to allow for incremental compilation of projects. */
    // "composite": true,                                /* Enable constraints that allow a TypeScript project to be used with project references. */
    // "tsBuildInfoFile": "./.tsbuildinfo",              /* Specify the path to .tsbuildinfo incremental compilation file. */
    // "disableSourceOfProjectReferenceRedirect": true,  /* Disable preferring source files instead of declaration files when referencing composite projects. */
    // "disableSolutionSearching": true,                 /* Opt a project out of multi-project reference checking when editing. */
    // "disableReferencedProjectLoad": true,             /* Reduce the number of projects loaded automatically by TypeScript. */

    /* Language and Environment */
    "target": "es2016",                                  /* Set the JavaScript language version for emitted JavaScript and include compatible library declarations. */
    // "lib": [],                                        /* Specify a set of bundled library declaration files that describe the target runtime environment. */
    // "jsx": "preserve",                                /* Specify what JSX code is generated. */
    // "experimentalDecorators": true,                   /* Enable experimental support for legacy experimental decorators. */
    // "emitDecoratorMetadata": true,                    /* Emit design-type metadata for decorated declarations in source files. */
    // "jsxFactory": "",                                 /* Specify the JSX factory function used when targeting React JSX emit, e.g. 'React.createElement' or 'h'. */
    // "jsxFragmentFactory": "",                         /* Specify the JSX Fragment reference used for fragments when targeting React JSX emit e.g. 'React.Fragment' or 'Fragment'. */
    // "jsxImportSource": "",                            /* Specify module specifier used to import the JSX factory functions when using 'jsx: react-jsx*'. */
    // "reactNamespace": "",                             /* Specify the object invoked for 'createElement'. This only applies when targeting 'react' JSX emit. */
    // "noLib": true,                                    /* Disable including any library files, including the default lib.d.ts. */
    // "useDefineForClassFields": true,                  /* Emit ECMAScript-standard-compliant class fields. */
    // "moduleDetection": "auto",                        /* Control what method is used to detect module-format JS files. */

    /* Modules */
    "module": "commonjs",                                /* Specify what module code is generated. */
    // "rootDir": "./",                                  /* Specify the root folder within your source files. */
    // "moduleResolution": "node10",                     /* Specify how TypeScript looks up a file from a given module specifier. */
    // "baseUrl": "./",                                  /* Specify the base directory to resolve non-relative module names. */
    // "paths": {},                                      /* Specify a set of entries that re-map imports to additional lookup locations. */
    // "rootDirs": [],                                   /* Allow multiple folders to be treated as one when resolving modules. */
    // "typeRoots": [],                                  /* Specify multiple folders that act like './node_modules/@types'. */
    // "types": [],                                      /* Specify type package names to be included without being referenced in a source file. */
    // "allowUmdGlobalAccess": true,                     /* Allow accessing UMD globals from modules. */
    // "moduleSuffixes": [],                             /* List of file name suffixes to search when resolving a module. */
    // "allowImportingTsExtensions": true,               /* Allow imports to include TypeScript file extensions. Requires '--moduleResolution bundler' and either '--noEmit' or '--emitDeclarationOnly' to be set. */
    // "resolvePackageJsonExports": true,                /* Use the package.json 'exports' field when resolving package imports. */
    // "resolvePackageJsonImports": true,                /* Use the package.json 'imports' field when resolving imports. */
    // "customConditions": [],                           /* Conditions to set in addition to the resolver-specific defaults when resolving imports. */
    // "resolveJsonModule": true,                        /* Enable importing .json files. */
    // "allowArbitraryExtensions": true,                 /* Enable importing files with any extension, provided a declaration file is present. */
    // "noResolve": true,                                /* Disallow 'import's, 'require's or '<reference>'s from expanding the number of files TypeScript should add to a project. */

    /* JavaScript Support */
    // "allowJs": true,                                  /* Allow JavaScript files to be a part of your program. Use the 'checkJs' option to get errors from these files. */
    // "checkJs": true,                                  /* Enable error reporting in type-checked JavaScript files. */
    // "maxNodeModuleJsDepth": 1,                        /* Specify the maximum folder depth used for checking JavaScript files from 'node_modules'. Only applicable with 'allowJs'. */

    /* Emit */
    // "declaration": true,                              /* Generate .d.ts files from TypeScript and JavaScript files in your project. */
    // "declarationMap": true,                           /* Create sourcemaps for d.ts files. */
    // "emitDeclarationOnly": true,                      /* Only output d.ts files and not JavaScript files. */
    // "sourceMap": true,                                /* Create source map files for emitted JavaScript files. */
    // "inlineSourceMap": true,                          /* Include sourcemap files inside the emitted JavaScript. */
    // "outFile": "./",                                  /* Specify a file that bundles all outputs into one JavaScript file. If 'declaration' is true, also designates a file that bundles all .d.ts output. */
    // "outDir": "./",                                   /* Specify an output folder for all emitted files. */
    // "removeComments": true,                           /* Disable emitting comments. */
    // "noEmit": true,                                   /* Disable emitting files from a compilation. */
    // "importHelpers": true,                            /* Allow importing helper functions from tslib once per project, instead of including them per-file. */
    // "downlevelIteration": true,                       /* Emit more compliant, but verbose and less performant JavaScript for iteration. */
    // "sourceRoot": "",                                 /* Specify the root path for debuggers to find the reference source code. */
    // "mapRoot": "",                                    /* Specify the location where debugger should locate map files instead of generated locations. */
    // "inlineSources": true,                            /* Include source code in the sourcemaps inside the emitted JavaScript. */
    // "emitBOM": true,                                  /* Emit a UTF-8 Byte Order Mark (BOM) in the beginning of output files. */
    // "newLine": "crlf",                                /* Set the newline character for emitting files. */
    // "stripInternal": true,                            /* Disable emitting declarations that have '@internal' in their JSDoc comments. */
    // "noEmitHelpers": true,                            /* Disable generating custom helper functions like '__extends' in compiled output. */
    // "noEmitOnError": true,                            /* Disable emitting files if any type checking errors are reported. */
    // "preserveConstEnums": true,                       /* Disable erasing 'const enum' declarations in generated code. */
    // "declarationDir": "./",                           /* Specify the output directory for generated declaration files. */

    /* Interop Constraints */
    // "isolatedModules": true,                          /* Ensure that each file can be safely transpiled without relying on other imports. */
    // "verbatimModuleSyntax": true,                     /* Do not transform or elide any imports or exports not marked as type-only, ensuring they are written in the output file's format based on the 'module' setting. */
    // "isolatedDeclarations": true,                     /* Require sufficient annotation on exports so other tools can trivially generate declaration files. */
    // "allowSyntheticDefaultImports": true,             /* Allow 'import x from y' when a module doesn't have a default export. */
    "esModuleInterop": true,                             /* Emit additional JavaScript to ease support for importing CommonJS modules. This enables 'allowSyntheticDefaultImports' for type compatibility. */
    // "preserveSymlinks": true,                         /* Disable resolving symlinks to their realpath. This correlates to the same flag in node. */
    "forceConsistentCasingInFileNames": true,            /* Ensure that casing is correct in imports. */

    /* Type Checking */
    "strict": true,                                      /* Enable all strict type-checking options. */
    // "noImplicitAny": true,                            /* Enable error reporting for expressions and declarations with an implied 'any' type. */
    // "strictNullChecks": true,                         /* When type checking, take into account 'null' and 'undefined'. */
    // "strictFunctionTypes": true,                      /* When assigning functions, check to ensure parameters and the return values are subtype-compatible. */
    // "strictBindCallApply": true,                      /* Check that the arguments for 'bind', 'call', and 'apply' methods match the original function. */
    // "strictPropertyInitialization": true,             /* Check for class properties that are declared but not set in the constructor. */
    // "noImplicitThis": true,                           /* Enable error reporting when 'this' is given the type 'any'. */
    // "useUnknownInCatchVariables": true,               /* Default catch clause variables as 'unknown' instead of 'any'. */
    // "alwaysStrict": true,                             /* Ensure 'use strict' is always emitted. */
    // "noUnusedLocals": true,                           /* Enable error reporting when local variables aren't read. */
    // "noUnusedParameters": true,                       /* Raise an error when a function parameter isn't read. */
    // "exactOptionalPropertyTypes": true,               /* Interpret optional property types as written, rather than adding 'undefined'. */
    // "noImplicitReturns": true,                        /* Enable error reporting for codepaths that do not explicitly return in a function. */
    // "noFallthroughCasesInSwitch": true,               /* Enable error reporting for fallthrough cases in switch statements. */
    // "noUncheckedIndexedAccess": true,                 /* Add 'undefined' to a type when accessed using an index. */
    // "noImplicitOverride": true,                       /* Ensure overriding members in derived classes are marked with an override modifier. */
    // "noPropertyAccessFromIndexSignature": true,       /* Enforces using indexed accessors for keys declared using an indexed type. */
    // "allowUnusedLabels": true,                        /* Disable error reporting for unused labels. */
    // "allowUnreachableCode": true,                     /* Disable error reporting for unreachable code. */

    /* Completeness */
    // "skipDefaultLibCheck": true,                      /* Skip type checking .d.ts files that are included with TypeScript. */
    "skipLibCheck": true                                 /* Skip type checking all .d.ts files. */
  }
}
"#;

    std::fs::write(&tsconfig_path, default_config).with_context(|| {
        format!(
            "failed to write tsconfig.json to {}",
            tsconfig_path.display()
        )
    })?;

    println!(
        "Created a new tsconfig.json with:
                                                                                                                     TS
  target: es2016
  module: commonjs
  strict: true
  esModuleInterop: true
  skipLibCheck: true
  forceConsistentCasingInFileNames: true


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
    top.insert(
        "compilerOptions".into(),
        serde_json::Value::Object(opts),
    );

    // Include files/include/exclude from tsconfig
    if let Some(ref cfg) = config {
        if let Some(ref files) = cfg.files {
            top.insert(
                "files".into(),
                serde_json::Value::Array(
                    files.iter().map(|f| serde_json::Value::String(f.clone())).collect(),
                ),
            );
        }
        if let Some(ref include) = cfg.include {
            top.insert(
                "include".into(),
                serde_json::Value::Array(
                    include.iter().map(|f| serde_json::Value::String(f.clone())).collect(),
                ),
            );
        }
        if let Some(ref exclude) = cfg.exclude {
            top.insert(
                "exclude".into(),
                serde_json::Value::Array(
                    exclude.iter().map(|f| serde_json::Value::String(f.clone())).collect(),
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
    use std::fs;
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

    let base_dir = tsconfig_path
        .as_ref()
        .and_then(|p| p.parent())
        .unwrap_or(cwd);

    // Handle --clean: delete build artifacts
    if args.clean {
        // Delete .tsbuildinfo files
        if let Some(ref tsconfig) = tsconfig_path {
            let buildinfo_path = tsconfig.with_extension("tsbuildinfo");
            if buildinfo_path.exists() {
                fs::remove_file(&buildinfo_path)?;
                println!("Deleted: {}", buildinfo_path.display());
            }
        }

        // Delete output directories if specified
        if let Some(ref out_dir) = resolved.out_dir {
            let full_out_dir = base_dir.join(out_dir);
            if full_out_dir.exists() {
                fs::remove_dir_all(&full_out_dir)?;
                println!("Deleted: {}", full_out_dir.display());
            }
        }

        if let Some(ref declaration_dir) = resolved.declaration_dir {
            let full_decl_dir = base_dir.join(declaration_dir);
            if full_decl_dir.exists() {
                fs::remove_dir_all(&full_decl_dir)?;
                println!("Deleted: {}", full_decl_dir.display());
            }
        }

        println!("Build cleaned successfully.");
        return Ok(());
    }

    // Handle --dry: show what would be built without building
    if args.dry {
        println!("Dry run - would build:");
        if let Some(project) = &args.project {
            println!("  Project: {}", project.display());
        } else if let Some(ref tsconfig) = tsconfig_path {
            println!("  Project: {}", tsconfig.display());
        } else {
            println!("  Project: {}/tsconfig.json", cwd.display());
        }
        // TODO: Show project references that would be built
        println!("(Full project reference support not yet implemented)");
        return Ok(());
    }

    // For now, just run normal compilation
    // TODO: Implement full build mode with project references
    let result = driver::compile(args, cwd)?;

    if args.build_verbose {
        println!("Projects in this build: ");
        if let Some(project) = &args.project {
            println!("  * {}", project.display());
        } else {
            println!("  * {}/tsconfig.json", cwd.display());
        }
    }

    if !result.diagnostics.is_empty() {
        let color = args
            .pretty
            .unwrap_or_else(|| std::io::stdout().is_terminal());
        let mut reporter = Reporter::new(color);
        let output = reporter.render(&result.diagnostics);
        if !output.is_empty() {
            eprintln!("{output}");
        }
    }

    let has_errors = result
        .diagnostics
        .iter()
        .any(|diag| diag.category == wasm::checker::types::diagnostics::DiagnosticCategory::Error);

    if has_errors {
        if !result.emitted_files.is_empty() {
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
