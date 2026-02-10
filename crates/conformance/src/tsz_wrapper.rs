//! tsz compiler wrapper for conformance testing
//!
//! Provides a simple API to compile TypeScript code and extract error codes.

use std::collections::HashMap;
use std::path::Path;
use tsz::diagnostics::{Diagnostic, DiagnosticSeverity};
use tsz::span::Span;

/// Result of compiling a test file
#[derive(Debug, Clone)]
pub struct CompilationResult {
    /// Error codes (TSXXXX format, e.g., 2304 for TS2304)
    pub error_codes: Vec<u32>,
    /// Whether compilation crashed (panic)
    pub crashed: bool,
    /// Resolved compiler options used
    pub options: HashMap<String, String>,
}

/// Prepared test directory ready for async compilation.
/// The temp directory is deleted on drop, so keep this alive during compilation.
pub struct PreparedTest {
    /// Temp directory containing test files and tsconfig.json
    pub temp_dir: tempfile::TempDir,
    /// Compiler options used
    #[allow(dead_code)]
    pub options: HashMap<String, String>,
}

/// Compile a TypeScript file and extract error codes (used by tests only).
#[cfg(test)]
pub fn compile_test(
    content: &str,
    filenames: &[(String, String)],
    options: &HashMap<String, String>,
    tsz_binary_path: &str,
) -> anyhow::Result<CompilationResult> {
    use tempfile::TempDir;

    // Create temporary directory for test files
    let temp_dir = TempDir::new()?;
    let dir_path = temp_dir.path();

    // Detect if any filename uses absolute (virtual root) paths
    let has_absolute_filenames = filenames.iter().any(|(name, _)| name.starts_with('/'));
    let ts_tests_lib_dir = std::path::Path::new("TypeScript/tests/lib");

    if filenames.is_empty() {
        // Single-file test: write content to test.ts (strip directive comments)
        let stripped_content = strip_directive_comments(content);
        // Handle /.lib/ references and absolute reference paths in single-file tests
        let stripped_content =
            resolve_lib_references(&stripped_content, dir_path, ts_tests_lib_dir);
        let stripped_content = rewrite_absolute_reference_paths(&stripped_content);
        let main_file = dir_path.join("test.ts");
        std::fs::write(&main_file, stripped_content)?;
    } else {
        // Multi-file test: write only the files from @filename directives
        for (filename, file_content) in filenames {
            // Sanitize filename to prevent path traversal outside temp dir
            let sanitized = filename
                .replace("..", "_")
                .trim_start_matches('/')
                .to_string();
            let file_path = dir_path.join(&sanitized);

            // Verify the path is still inside temp_dir
            if !file_path.starts_with(dir_path) {
                continue; // Skip files that would escape the temp directory
            }

            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Rewrite absolute paths in content for virtual root tests
            let written_content = if has_absolute_filenames {
                let c = resolve_lib_references(file_content, dir_path, ts_tests_lib_dir);
                let c = rewrite_absolute_reference_paths(&c);
                let c = rewrite_absolute_imports(&c);
                rewrite_bare_specifiers(&c, filenames)
            } else {
                let c = resolve_lib_references(file_content, dir_path, ts_tests_lib_dir);
                let c = rewrite_absolute_reference_paths(&c);
                rewrite_bare_specifiers(&c, filenames)
            };

            std::fs::write(&file_path, written_content)?;
        }
    }

    // Create tsconfig.json with test options unless provided by the test itself
    let tsconfig_path = dir_path.join("tsconfig.json");
    let has_js_files = filenames.iter().any(|(name, _)| {
        let normalized = name.replace('\\', "/");
        // Don't count JS files in node_modules — they're external packages, not project sources.
        // tsc doesn't auto-infer allowJs from node_modules contents.
        if normalized.contains("/node_modules/") || normalized.starts_with("node_modules/") {
            return false;
        }
        let lower = normalized.to_lowercase();
        lower.ends_with(".js")
            || lower.ends_with(".jsx")
            || lower.ends_with(".mjs")
            || lower.ends_with(".cjs")
    });
    let has_tsconfig_file = filenames
        .iter()
        .any(|(name, _)| name.replace('\\', "/").ends_with("tsconfig.json"));
    // Only infer allowJs from JS file extensions when not explicitly set
    let explicit_allow_js = options.get("allowJs").or_else(|| options.get("allowjs"));
    let allow_js = match explicit_allow_js {
        Some(v) => v == "true",
        None => has_js_files,
    };
    // Include .cts/.mts (TypeScript CJS/ESM) alongside .ts/.tsx
    let include = if allow_js {
        serde_json::json!([
            "*.ts", "*.tsx", "*.cts", "*.mts", "*.js", "*.jsx", "*.mjs", "*.cjs", "**/*.ts",
            "**/*.tsx", "**/*.cts", "**/*.mts", "**/*.js", "**/*.jsx", "**/*.mjs", "**/*.cjs"
        ])
    } else {
        serde_json::json!([
            "*.ts", "*.tsx", "*.cts", "*.mts", "**/*.ts", "**/*.tsx", "**/*.cts", "**/*.mts"
        ])
    };
    if !has_tsconfig_file {
        let mut compiler_options = convert_options_to_tsconfig(options);
        if allow_js {
            if let serde_json::Value::Object(ref mut map) = compiler_options {
                map.entry("allowJs")
                    .or_insert(serde_json::Value::Bool(true));
            }
        }
        let tsconfig_content = serde_json::json!({
            "compilerOptions": compiler_options,
            "include": include,
            "exclude": ["node_modules"]
        });

        // Write tsconfig
        std::fs::write(
            &tsconfig_path,
            serde_json::to_string_pretty(&tsconfig_content)?,
        )?;
    } else {
        copy_tsconfig_to_root_if_needed(dir_path, filenames, options)?;
    }

    // Run tsz compiler using the tsz binary
    // Note: Spawning process is simpler than using the driver directly
    // and avoids reinitializing the compiler state
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compile_tsz_with_binary(dir_path, tsz_binary_path)
    }));

    match result {
        Ok(Ok(diagnostics)) => {
            // Extract error codes from diagnostics
            let error_codes = extract_error_codes(&diagnostics);
            Ok(CompilationResult {
                error_codes,
                crashed: false,
                options: options.clone(),
            })
        }
        Ok(Err(e)) => Err(e), // Fatal error
        Err(_) => Ok(CompilationResult {
            error_codes: vec![],
            crashed: true,
            options: options.clone(),
        }),
    }
}

/// Prepare a test directory with files and tsconfig.json for compilation.
///
/// Returns a `PreparedTest` whose temp directory must be kept alive during compilation.
/// Use this with `tokio::process::Command` + `kill_on_drop(true)` for proper timeout handling.
pub fn prepare_test_dir(
    content: &str,
    filenames: &[(String, String)],
    options: &HashMap<String, String>,
) -> anyhow::Result<PreparedTest> {
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let dir_path = temp_dir.path();

    // Parse @symlink associations from raw content
    // Format: @filename: /path/to/file followed by @symlink: /link1,/link2
    let symlink_map = parse_symlink_associations(content);

    // Detect if any filename uses absolute (virtual root) paths
    let has_absolute_filenames = filenames.iter().any(|(name, _)| name.starts_with('/'));

    // Path to TypeScript test harness lib files (for /.lib/ references)
    let ts_tests_lib_dir = std::path::Path::new("TypeScript/tests/lib");

    if filenames.is_empty() {
        let stripped_content = strip_directive_comments(content);
        // Handle /.lib/ references and absolute reference paths in single-file tests
        let stripped_content =
            resolve_lib_references(&stripped_content, dir_path, ts_tests_lib_dir);
        let stripped_content = rewrite_absolute_reference_paths(&stripped_content);
        let main_file = dir_path.join("test.ts");
        std::fs::write(&main_file, stripped_content)?;
    } else {
        for (filename, file_content) in filenames {
            let sanitized = filename
                .replace("..", "_")
                .trim_start_matches('/')
                .to_string();
            let file_path = dir_path.join(&sanitized);
            if !file_path.starts_with(dir_path) {
                continue;
            }
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // When tests use absolute filenames, rewrite their content so that
            // absolute import specifiers and /// <reference> paths resolve within
            // the tmpdir (which acts as the virtual filesystem root).
            let written_content = if has_absolute_filenames {
                let c = resolve_lib_references(file_content, dir_path, ts_tests_lib_dir);
                let c = rewrite_absolute_reference_paths(&c);
                let c = rewrite_absolute_imports(&c);
                rewrite_bare_specifiers(&c, filenames)
            } else {
                // Even without absolute filenames, handle /.lib/ references and bare specifiers
                let c = resolve_lib_references(file_content, dir_path, ts_tests_lib_dir);
                let c = rewrite_absolute_reference_paths(&c);
                rewrite_bare_specifiers(&c, filenames)
            };

            std::fs::write(&file_path, written_content)?;
        }
    }

    // Create symlinks from @symlink directives
    for (source_filename, symlink_paths) in &symlink_map {
        for symlink_path in symlink_paths {
            let sanitized_link = symlink_path
                .replace("..", "_")
                .trim_start_matches('/')
                .to_string();
            let link_path = dir_path.join(&sanitized_link);
            let sanitized_source = source_filename
                .replace("..", "_")
                .trim_start_matches('/')
                .to_string();
            let source_path = dir_path.join(&sanitized_source);

            if source_path.exists() {
                if let Some(parent) = link_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                // Create symlink (Unix only)
                #[cfg(unix)]
                {
                    let _ = std::os::unix::fs::symlink(&source_path, &link_path);
                }
            }
        }
    }

    let tsconfig_path = dir_path.join("tsconfig.json");
    let has_js_files = filenames.iter().any(|(name, _)| {
        let normalized = name.replace('\\', "/");
        // Don't count JS files in node_modules — they're external packages, not project sources.
        // tsc doesn't auto-infer allowJs from node_modules contents.
        if normalized.contains("/node_modules/") || normalized.starts_with("node_modules/") {
            return false;
        }
        let lower = normalized.to_lowercase();
        lower.ends_with(".js")
            || lower.ends_with(".jsx")
            || lower.ends_with(".mjs")
            || lower.ends_with(".cjs")
    });
    let has_tsconfig_file = filenames
        .iter()
        .any(|(name, _)| name.replace('\\', "/").ends_with("tsconfig.json"));
    // Only infer allowJs from JS file extensions when not explicitly set
    let explicit_allow_js = options.get("allowJs").or_else(|| options.get("allowjs"));
    let allow_js = match explicit_allow_js {
        Some(v) => v == "true",
        None => has_js_files,
    };
    // Include .cts/.mts (TypeScript CJS/ESM) alongside .ts/.tsx
    let include = if allow_js {
        serde_json::json!([
            "*.ts", "*.tsx", "*.cts", "*.mts", "*.js", "*.jsx", "*.mjs", "*.cjs", "**/*.ts",
            "**/*.tsx", "**/*.cts", "**/*.mts", "**/*.js", "**/*.jsx", "**/*.mjs", "**/*.cjs"
        ])
    } else {
        serde_json::json!([
            "*.ts", "*.tsx", "*.cts", "*.mts", "**/*.ts", "**/*.tsx", "**/*.cts", "**/*.mts"
        ])
    };
    if !has_tsconfig_file {
        let mut compiler_options = convert_options_to_tsconfig(options);
        if allow_js {
            if let serde_json::Value::Object(ref mut map) = compiler_options {
                map.entry("allowJs")
                    .or_insert(serde_json::Value::Bool(true));
            }
        }
        let tsconfig_content = serde_json::json!({
            "compilerOptions": compiler_options,
            "include": include,
            "exclude": ["node_modules"]
        });
        std::fs::write(
            &tsconfig_path,
            serde_json::to_string_pretty(&tsconfig_content)?,
        )?;
    } else {
        copy_tsconfig_to_root_if_needed(dir_path, filenames, options)?;
    }

    Ok(PreparedTest {
        temp_dir,
        options: options.clone(),
    })
}

/// Parse tsz process output into a CompilationResult.
pub fn parse_tsz_output(
    output: &std::process::Output,
    options: HashMap<String, String>,
) -> CompilationResult {
    if output.status.success() {
        return CompilationResult {
            error_codes: vec![],
            crashed: false,
            options,
        };
    }

    // Check if process was killed by a signal (crash, not type errors)
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if output.status.signal().is_some() {
            return CompilationResult {
                error_codes: vec![],
                crashed: true,
                options,
            };
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    let diagnostics = parse_diagnostics_from_text(&combined);
    let mut error_codes = extract_error_codes(&diagnostics);
    const TS5110: u32 = 5110;
    if !error_codes.contains(&TS5110) {
        if let (Some(module_resolution), Some(module)) =
            (options.get("moduleresolution"), options.get("module"))
        {
            let resolution = module_resolution
                .split(',')
                .next()
                .unwrap_or(module_resolution)
                .trim()
                .to_lowercase();
            let module = module
                .split(',')
                .next()
                .unwrap_or(module)
                .trim()
                .to_lowercase();
            let needs_match = resolution == "node16" || resolution == "nodenext";
            if needs_match && module != resolution {
                error_codes.push(TS5110);
            }
        }
    }
    CompilationResult {
        error_codes,
        crashed: false,
        options,
    }
}

/// Test harness-specific directives that should NOT be passed to tsconfig.json
const HARNESS_ONLY_DIRECTIVES: &[&str] = &[
    "filename",
    "allowNonTsExtensions",
    "useCaseSensitiveFileNames",
    "baselineFile",
    "noErrorTruncation",
    "suppressOutputPathCheck",
    "noImplicitReferences",
    "currentDirectory",
    "traceResolution",
    "symlink",
    "link",
    "noTypesAndSymbols",
    "fullEmitPaths",
    "noCheck",
    "nocheck",
    "reportDiagnostics",
    "captureSuggestions",
    "typeScriptVersion",
    "skip",
];

/// List-type compiler options that accept comma-separated values
const LIST_OPTIONS: &[&str] = &[
    "lib",
    "types",
    "typeRoots",
    "rootDirs",
    "moduleSuffixes",
    "customConditions",
];

/// Convert test directive options to tsconfig compiler options
///
/// Handles:
/// - Boolean options (true/false)
/// - List options (comma-separated values like @lib: es6,dom)
/// - String/enum options (target, module, etc.)
/// - Filters out test harness-specific directives
fn convert_options_to_tsconfig(options: &HashMap<String, String>) -> serde_json::Value {
    let mut opts = serde_json::Map::new();

    for (key, value) in options {
        // Skip test harness-specific directives
        let key_lower = key.to_lowercase();
        if HARNESS_ONLY_DIRECTIVES
            .iter()
            .any(|&d| d.to_lowercase() == key_lower)
        {
            continue;
        }

        let canonical_key = canonical_option_name(&key_lower);
        let json_value = if value == "true" {
            serde_json::Value::Bool(true)
        } else if value == "false" {
            serde_json::Value::Bool(false)
        } else if LIST_OPTIONS
            .iter()
            .any(|&opt| opt.to_lowercase() == key_lower)
        {
            // Parse comma-separated list
            let items: Vec<serde_json::Value> = value
                .split(',')
                .map(|s| serde_json::Value::String(s.trim().to_string()))
                .collect();
            serde_json::Value::Array(items)
        } else if let Ok(num) = value.parse::<i64>() {
            // Handle numeric options (e.g., maxNodeModuleJsDepth)
            serde_json::Value::Number(num.into())
        } else {
            serde_json::Value::String(value.clone())
        };

        opts.insert(canonical_key.to_string(), json_value);
    }

    serde_json::Value::Object(opts)
}

fn canonical_option_name(key_lower: &str) -> &str {
    match key_lower {
        "allowarbitraryextensions" => "allowArbitraryExtensions",
        "allowimportingtsextensions" => "allowImportingTsExtensions",
        "allowjs" => "allowJs",
        "allowsyntheticdefaultimports" => "allowSyntheticDefaultImports",
        "allowumdglobalaccess" => "allowUmdGlobalAccess",
        "allowunreachablecode" => "allowUnreachableCode",
        "allowunusedlabels" => "allowUnusedLabels",
        "alwaysstrict" => "alwaysStrict",
        "baseurl" => "baseUrl",
        "checkjs" => "checkJs",
        "customconditions" => "customConditions",
        "declaration" => "declaration",
        "declarationdir" => "declarationDir",
        "declarationmap" => "declarationMap",
        "downleveliteration" => "downlevelIteration",
        "emitdeclarationonly" => "emitDeclarationOnly",
        "emitdecoratormetadata" => "emitDecoratorMetadata",
        "erasablesyntaxonly" => "erasableSyntaxOnly",
        "esmoduleinterop" => "esModuleInterop",
        "exactoptionalpropertytypes" => "exactOptionalPropertyTypes",
        "experimentaldecorators" => "experimentalDecorators",
        "ignoredeprecations" => "ignoreDeprecations",
        "importhelpers" => "importHelpers",
        "importsnotusedasvalues" => "importsNotUsedAsValues",
        "incremental" => "incremental",
        "inlinesourcemap" => "inlineSourceMap",
        "inlinesources" => "inlineSources",
        "isolateddeclarations" => "isolatedDeclarations",
        "isolatedmodules" => "isolatedModules",
        "jsx" => "jsx",
        "jsxfactory" => "jsxFactory",
        "jsxfragmentfactory" => "jsxFragmentFactory",
        "jsximportsource" => "jsxImportSource",
        "keyofstringsonly" => "keyofStringsOnly",
        "lib" => "lib",
        "libreplacement" => "libReplacement",
        "maproot" => "mapRoot",
        "maxnodemodulejsdepth" => "maxNodeModuleJsDepth",
        "module" => "module",
        "moduledetection" => "moduleDetection",
        "moduleresolution" => "moduleResolution",
        "modulesuffixes" => "moduleSuffixes",
        "newline" => "newLine",
        "nocheck" => "noCheck",
        "noemit" => "noEmit",
        "noemithelpers" => "noEmitHelpers",
        "noemitonerror" => "noEmitOnError",
        "noerrortruncation" => "noErrorTruncation",
        "nofallthrough" => "noFallthroughCasesInSwitch",
        "nofallthroughcasesinswitch" => "noFallthroughCasesInSwitch",
        "noimplicitany" => "noImplicitAny",
        "noimplicitoverride" => "noImplicitOverride",
        "noimplicitreturns" => "noImplicitReturns",
        "noimplicitthis" => "noImplicitThis",
        "noimplicitusestrict" => "noImplicitUseStrict",
        "nolib" => "noLib",
        "nopropertyaccessfromindexsignature" => "noPropertyAccessFromIndexSignature",
        "noresolve" => "noResolve",
        "nostrictgenericchecks" => "noStrictGenericChecks",
        "notypesandsymbols" => "noTypesAndSymbols",
        "nouncheckedindexedaccess" => "noUncheckedIndexedAccess",
        "nouncheckedsideeffectimports" => "noUncheckedSideEffectImports",
        "nounusedlocals" => "noUnusedLocals",
        "nounusedparameters" => "noUnusedParameters",
        "outdir" => "outDir",
        "outfile" => "outFile",
        "paths" => "paths",
        "preserveconstenums" => "preserveConstEnums",
        "preservesymlinks" => "preserveSymlinks",
        "removecomments" => "removeComments",
        "resolvejsonmodule" => "resolveJsonModule",
        "resolvepackagejsonexports" => "resolvePackageJsonExports",
        "resolvepackagejsonimports" => "resolvePackageJsonImports",
        "rewriterelativeimportextensions" => "rewriteRelativeImportExtensions",
        "rootdir" => "rootDir",
        "rootdirs" => "rootDirs",
        "skiplibcheck" => "skipLibCheck",
        "sourcemap" => "sourceMap",
        "sourceroot" => "sourceRoot",
        "strict" => "strict",
        "strictbindcallapply" => "strictBindCallApply",
        "strictbuiltiniteratorreturn" => "strictBuiltinIteratorReturn",
        "strictfunctiontypes" => "strictFunctionTypes",
        "strictnullchecks" => "strictNullChecks",
        "strictpropertyinitialization" => "strictPropertyInitialization",
        "stripinternal" => "stripInternal",
        "suppressexcesspropertyerrors" => "suppressExcessPropertyErrors",
        "suppressimplicitanyindexerrors" => "suppressImplicitAnyIndexErrors",
        "target" => "target",
        "traceresolution" => "traceResolution",
        "tsbuildinfofile" => "tsBuildInfoFile",
        "typeroots" => "typeRoots",
        "types" => "types",
        "usedefineforclassfields" => "useDefineForClassFields",
        "useunknownincatchvariables" => "useUnknownInCatchVariables",
        "verbatimmodulesyntax" => "verbatimModuleSyntax",
        _ => key_lower,
    }
}

/// Compile with tsz binary (used by compile_test for tests only)
#[cfg(test)]
fn compile_tsz_with_binary(
    base_dir: &std::path::Path,
    tsz_path: &str,
) -> anyhow::Result<Vec<Diagnostic>> {
    use std::process::Command;

    // Run tsz with --pretty false for machine-readable output
    let output = Command::new(tsz_path)
        .arg("--project")
        .arg(base_dir)
        .arg("--noEmit")
        .arg("--pretty")
        .arg("false")
        .output()?;

    // Parse diagnostics from stderr and stdout
    // This is a simplified version - real implementation would need to parse
    // the error output to extract diagnostic codes
    if output.status.success() {
        Ok(Vec::new())
    } else {
        // Parse error codes from both stdout and stderr
        // Format: "file.ts(1,1): error TS2304: Cannot find name 'foo'"
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);
        let diagnostics = parse_diagnostics_from_text(&combined);
        Ok(diagnostics)
    }
}

/// Simple parser to extract error codes from tsz output
fn parse_diagnostics_from_text(text: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for line in text.lines() {
        // Look for error pattern: "TSXXXX:"
        if let Some(start) = line.find("TS") {
            if let Some(end) = line[start..].find(':') {
                let code_str = &line[start + 2..start + end];
                if let Ok(code) = code_str.parse::<u32>() {
                    // Create a simple diagnostic placeholder
                    // Real implementation would parse the full diagnostic
                    diagnostics.push(Diagnostic {
                        file_name: "test.ts".to_string(),
                        span: Span::new(0, 0),
                        message: line.to_string(),
                        severity: DiagnosticSeverity::Error,
                        code,
                        related: Vec::new(),
                        source: Some("typescript".to_string()),
                    });
                }
            }
        }
    }

    diagnostics
}

fn copy_tsconfig_to_root_if_needed(
    dir_path: &Path,
    filenames: &[(String, String)],
    options: &HashMap<String, String>,
) -> anyhow::Result<()> {
    let root_tsconfig = dir_path.join("tsconfig.json");
    // If the tsconfig was already written (e.g. at the root via @filename), read and merge
    let base_content = if root_tsconfig.is_file() {
        std::fs::read_to_string(&root_tsconfig)?
    } else {
        // Find the tsconfig.json from filenames
        let content = filenames
            .iter()
            .find(|(name, _)| name.replace('\\', "/").ends_with("tsconfig.json"))
            .map(|(_, content)| content.clone());
        match content {
            Some(c) => c,
            None => return Ok(()),
        }
    };

    // Merge directive options into the tsconfig's compilerOptions
    let directive_opts = convert_options_to_tsconfig(options);
    if let serde_json::Value::Object(ref directive_map) = directive_opts {
        if !directive_map.is_empty() {
            let mut tsconfig: serde_json::Value =
                serde_json::from_str(&base_content).unwrap_or_else(|_| serde_json::json!({}));
            if let serde_json::Value::Object(ref mut root) = tsconfig {
                let compiler_options = root
                    .entry("compilerOptions")
                    .or_insert_with(|| serde_json::json!({}));
                if let serde_json::Value::Object(ref mut opts) = compiler_options {
                    for (key, value) in directive_map {
                        opts.insert(key.clone(), value.clone());
                    }
                }
            }
            std::fs::write(&root_tsconfig, serde_json::to_string_pretty(&tsconfig)?)?;
            return Ok(());
        }
    }

    // No directive options to merge, just write the original content
    if !root_tsconfig.is_file() {
        std::fs::write(&root_tsconfig, base_content)?;
    }
    Ok(())
}

/// Extract error codes from diagnostics
fn extract_error_codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    let mut codes = Vec::new();

    for diag in diagnostics {
        // Only collect errors, not warnings or suggestions
        if diag.severity != DiagnosticSeverity::Error {
            continue;
        }

        // The code field already contains the numeric error code
        codes.push(diag.code);
    }

    codes
}

/// Parse @symlink associations from raw test file content.
/// Returns a map of source filename -> list of symlink paths.
/// Format in test files: @filename: /path followed by @symlink: /link1,/link2
fn parse_symlink_associations(content: &str) -> Vec<(String, Vec<String>)> {
    let mut result = Vec::new();
    let mut current_filename: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        // Match @filename or @Filename
        if let Some(rest) = trimmed
            .strip_prefix("// @filename:")
            .or_else(|| trimmed.strip_prefix("// @Filename:"))
            .or_else(|| trimmed.strip_prefix("//@filename:"))
            .or_else(|| trimmed.strip_prefix("//@Filename:"))
        {
            current_filename = Some(rest.trim().to_string());
        }
        // Match @symlink or @Symlink
        if let Some(rest) = trimmed
            .strip_prefix("// @symlink:")
            .or_else(|| trimmed.strip_prefix("// @Symlink:"))
            .or_else(|| trimmed.strip_prefix("//@symlink:"))
            .or_else(|| trimmed.strip_prefix("//@Symlink:"))
        {
            if let Some(ref filename) = current_filename {
                let links: Vec<String> = rest
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if !links.is_empty() {
                    result.push((filename.clone(), links));
                }
            }
        }
    }

    result
}

/// Strip @ directive comments from test file content
/// Removes lines like `// @strict: true` from the code
fn strip_directive_comments(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Keep lines that are not @ directives
            // Directives start with // @key: value
            !(trimmed.starts_with("//") && trimmed.contains("@") && trimmed.contains(":"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Rewrite absolute import specifiers to relative ones.
///
/// TSC conformance tests use a virtual filesystem where `@Filename: /foo.ts`
/// creates a file at virtual path `/foo.ts`. Imports like `from '/foo'` resolve
/// via the VFS. Our harness writes files to a tmpdir (stripping the leading `/`),
/// so `/foo.ts` becomes `<tmpdir>/foo.ts`. We rewrite absolute specifiers to
/// relative so the compiler resolves them within the tmpdir.
///
/// Transforms:
/// - `from '/foo'`  →  `from './foo'`
/// - `import '/foo'` → `import './foo'`
/// - `require('/foo')` → `require('./foo')`
fn rewrite_absolute_imports(content: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    // Match: from '/...' or from "/..."
    // Note: Rust regex doesn't support backreferences (\2), so match any quote at the end
    static FROM_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(from\s+)(['"])/((?:[^'"])*?)['"]"#).unwrap());

    // Match: import '/...' or import "/..." (side-effect imports)
    static IMPORT_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(import\s+)(['"])/((?:[^'"])*?)['"]"#).unwrap());

    // Match: require('/...') or require("/...")
    static REQUIRE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(require\()(['"])/((?:[^'"])*?)['"](\))"#).unwrap());

    let result = FROM_RE.replace_all(content, "${1}${2}./${3}${2}");
    let result = IMPORT_RE.replace_all(&result, "${1}${2}./${3}${2}");
    let result = REQUIRE_RE.replace_all(&result, "${1}${2}./${3}${2}${4}");
    result.into_owned()
}

/// Rewrite bare module specifiers to relative paths for multi-file tests.
///
/// TSC conformance tests often use bare specifiers like `from "server"` to reference
/// sibling files defined via `@filename` directives. These should resolve to `"./server"`
/// when the files are in the same directory.
///
/// Transforms:
/// - `from "foo"` → `from "./foo"` (if foo.ts/.tsx/.d.ts exists in filenames)
/// - `import "foo"` → `import "./foo"`
/// - `require("foo")` → `require("./foo")`
///
/// Does NOT rewrite:
/// - Relative paths (already start with `.` or `..`)
/// - Absolute paths (start with `/`)
/// - Scoped packages (start with `@`)
/// - Node built-ins or known npm packages (we check if file exists in filenames)
fn rewrite_bare_specifiers(content: &str, filenames: &[(String, String)]) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    // If no multi-file test, nothing to rewrite
    if filenames.is_empty() {
        return content.to_string();
    }

    // Build a set of available file basenames (without extension)
    let mut available_files = std::collections::HashSet::new();
    let mut declared_modules = std::collections::HashSet::new();
    static DECLARE_MODULE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"declare\s+module\s+['"]([^'"]+)['"]"#).unwrap());
    for (filename, _) in filenames {
        let normalized = filename.replace('\\', "/");
        if normalized.contains("/node_modules/") || normalized.starts_with("node_modules/") {
            continue;
        }
        // Extract basename without extension
        // Handle .d.ts specially since file_stem() on "a.d.ts" returns "a.d", not "a"
        let basename = if filename.ends_with(".d.ts") {
            filename.trim_end_matches(".d.ts")
        } else if filename.ends_with(".d.cts") {
            filename.trim_end_matches(".d.cts")
        } else if filename.ends_with(".d.mts") {
            filename.trim_end_matches(".d.mts")
        } else {
            std::path::Path::new(filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(filename)
        };
        available_files.insert(basename.to_string());
    }
    for (_, content) in filenames {
        for cap in DECLARE_MODULE_RE.captures_iter(content) {
            declared_modules.insert(cap[1].to_string());
        }
    }

    // Match: from "module" or from 'module'
    // Captures: (from )(quote)(module)(quote)
    static FROM_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(from\s+)(['"])([^'"\./][^'"]*)['"]"#).unwrap());

    // Match: import "module" or import 'module' (side-effect imports)
    static IMPORT_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(import\s+)(['"])([^'"\./][^'"]*)['"]"#).unwrap());

    // Match: require("module") or require('module')
    static REQUIRE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(require\()(['"])([^'"\./][^'"]*)['"](\))"#).unwrap());

    // Match: export * from "module"
    static EXPORT_FROM_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(export\s+\*\s+from\s+)(['"])([^'"\./][^'"]*)['"]"#).unwrap());

    let mut result = content.to_string();

    // Helper to check if a specifier should be rewritten
    let should_rewrite = |specifier: &str| -> bool {
        // Don't rewrite if it starts with @, ., /, or contains @/ (scoped package)
        if specifier.starts_with('@')
            || specifier.starts_with('.')
            || specifier.starts_with('/')
            || specifier.contains("@/")
        {
            return false;
        }

        // Check if this matches one of our test files (with or without extension)
        if declared_modules.contains(specifier) {
            return false;
        }
        available_files.contains(specifier)
            || available_files.contains(specifier.trim_end_matches(".js"))
            || available_files.contains(specifier.trim_end_matches(".ts"))
            || available_files.contains(specifier.trim_end_matches(".tsx"))
            || available_files.contains(specifier.trim_end_matches(".d.ts"))
    };

    // Rewrite each pattern
    result = FROM_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let specifier = &caps[3];
            if should_rewrite(specifier) {
                format!("{}{}./{}{}", &caps[1], &caps[2], specifier, &caps[2])
            } else {
                caps[0].to_string()
            }
        })
        .into_owned();

    result = IMPORT_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let specifier = &caps[3];
            if should_rewrite(specifier) {
                format!("{}{}./{}{}", &caps[1], &caps[2], specifier, &caps[2])
            } else {
                caps[0].to_string()
            }
        })
        .into_owned();

    result = REQUIRE_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let specifier = &caps[3];
            if should_rewrite(specifier) {
                format!(
                    "{}{}./{}{}{}",
                    &caps[1], &caps[2], specifier, &caps[2], &caps[4]
                )
            } else {
                caps[0].to_string()
            }
        })
        .into_owned();

    result = EXPORT_FROM_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let specifier = &caps[3];
            if should_rewrite(specifier) {
                format!("{}{}./{}{}", &caps[1], &caps[2], specifier, &caps[2])
            } else {
                caps[0].to_string()
            }
        })
        .into_owned();

    result
}

/// Rewrite `/// <reference path="/.lib/...">` directives to point to a local copy
/// of the test harness library, and copy the referenced file into the tmpdir.
///
/// TSC tests reference shared type definitions via absolute VFS paths like
/// `/.lib/react16.d.ts`. These live in `TypeScript/tests/lib/` in the repo.
/// We copy them into the tmpdir and rewrite the reference to a relative path.
fn resolve_lib_references(
    content: &str,
    dir_path: &std::path::Path,
    ts_tests_lib_dir: &std::path::Path,
) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    // Match: /// <reference path="/.lib/react16.d.ts" />
    // Note: Rust regex doesn't support backreferences, so we match any quote at the end
    static LIB_REF_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(///\s*<reference\s+path\s*=\s*)(['"])/.lib/((?:[^'"]*))['"]"#).unwrap()
    });

    let mut result = content.to_string();

    for caps in LIB_REF_RE.captures_iter(content) {
        let lib_file = &caps[3]; // e.g., "react16.d.ts"
        let src = ts_tests_lib_dir.join(lib_file);

        if src.exists() {
            // Create .lib directory in tmpdir and copy the file
            let lib_dir = dir_path.join(".lib");
            let _ = std::fs::create_dir_all(&lib_dir);
            let dest = lib_dir.join(lib_file);
            let _ = std::fs::copy(&src, &dest);
        }

        // Rewrite the reference path to be relative (whether or not file exists)
        let old = caps.get(0).unwrap().as_str();
        let new = format!("{}{}/.lib/{}{}", &caps[1], &caps[2], lib_file, &caps[2]);
        result = result.replace(old, &new);
    }

    result
}

/// Rewrite `/// <reference path="/absolute/path">` directives to relative paths.
///
/// After stripping leading `/` from @Filename paths, any `/// <reference path="/...">`
/// pointing to another test file should become relative.
fn rewrite_absolute_reference_paths(content: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    // Match: /// <reference path="/..." />
    // Note: Rust regex doesn't support backreferences or lookahead, so we match all and filter
    static ABS_REF_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(///\s*<reference\s+path\s*=\s*)(['"])/([^'"]*?)['"]"#).unwrap()
    });

    ABS_REF_RE
        .replace_all(content, |caps: &regex::Captures| {
            let path = &caps[3];
            // Skip .lib/ paths - they're handled by resolve_lib_references
            if path.starts_with(".lib/") {
                caps[0].to_string()
            } else {
                format!("{}{}./{}{}", &caps[1], &caps[2], path, &caps[2])
            }
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_test_dir_copies_absolute_tsconfig_to_root() {
        let content = "";
        let filenames = vec![
            (
                "/project/tsconfig.json".to_string(),
                r#"{"compilerOptions": {}}"#.to_string(),
            ),
            (
                "/project/src/app.ts".to_string(),
                "export const x = 1;".to_string(),
            ),
        ];
        let options: HashMap<String, String> = HashMap::new();

        let prepared = prepare_test_dir(content, &filenames, &options).unwrap();
        let root_tsconfig = prepared.temp_dir.path().join("tsconfig.json");
        assert!(
            root_tsconfig.is_file(),
            "tsconfig should exist at project root"
        );
    }

    fn find_tsz_binary() -> String {
        // Try common build locations relative to workspace root
        let candidates = [
            ".target/dist-fast/tsz",
            ".target/release/tsz",
            "target/release/tsz",
            "target/debug/tsz",
        ];
        // Workspace root is two levels up from crates/conformance/
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("Could not find workspace root");
        for candidate in &candidates {
            let path = workspace_root.join(candidate);
            if path.exists() {
                return path.to_string_lossy().to_string();
            }
        }
        panic!("tsz binary not found. Build with: cargo build --profile dist-fast -p tsz-cli");
    }

    #[test]
    fn test_compile_simple_error() {
        let content = r#"
// @strict: true
const x: number = "string";
"#;
        let tsz = find_tsz_binary();
        let result = compile_test(content, &[], &HashMap::new(), &tsz).unwrap();
        // Should have type error (TS2322)
        assert!(!result.error_codes.is_empty());
    }

    #[test]
    fn test_compile_no_errors() {
        let content = r#"
// @strict: true
const x: number = 42;
"#;
        let tsz = find_tsz_binary();
        let result = compile_test(content, &[], &HashMap::new(), &tsz).unwrap();
        // Should have no errors
        assert!(result.error_codes.is_empty());
    }

    #[test]
    fn test_rewrite_bare_specifiers() {
        let filenames = vec![
            ("server.ts".to_string(), "export class c {}".to_string()),
            ("client.ts".to_string(), "".to_string()),
        ];

        // Test export * from
        let content = r#"export * from "server";"#;
        let result = rewrite_bare_specifiers(content, &filenames);
        assert_eq!(result, r#"export * from "./server";"#);

        // Test import from
        let content = r#"import { x } from "server";"#;
        let result = rewrite_bare_specifiers(content, &filenames);
        assert_eq!(result, r#"import { x } from "./server";"#);

        // Test side-effect import
        let content = r#"import "server";"#;
        let result = rewrite_bare_specifiers(content, &filenames);
        assert_eq!(result, r#"import "./server";"#);

        // Test require
        let content = r#"const x = require("server");"#;
        let result = rewrite_bare_specifiers(content, &filenames);
        assert_eq!(result, r#"const x = require("./server");"#);

        // Should NOT rewrite npm packages
        let content = r#"import { x } from "lodash";"#;
        let result = rewrite_bare_specifiers(content, &filenames);
        assert_eq!(result, r#"import { x } from "lodash";"#);

        // Should NOT rewrite relative paths
        let content = r#"import { x } from "./server";"#;
        let result = rewrite_bare_specifiers(content, &filenames);
        assert_eq!(result, r#"import { x } from "./server";"#);

        // Should NOT rewrite absolute paths
        let content = r#"import { x } from "/server";"#;
        let result = rewrite_bare_specifiers(content, &filenames);
        assert_eq!(result, r#"import { x } from "/server";"#);

        // Should NOT rewrite scoped packages
        let content = r#"import { x } from "@scope/package";"#;
        let result = rewrite_bare_specifiers(content, &filenames);
        assert_eq!(result, r#"import { x } from "@scope/package";"#);
    }

    #[test]
    fn test_rewrite_bare_specifiers_with_d_ts() {
        // Test .d.ts file handling
        let filenames = vec![
            ("a.d.ts".to_string(), "export = {};".to_string()),
            ("b.ts".to_string(), "".to_string()),
        ];

        // Should rewrite bare specifier for .d.ts file
        let content = r#"import * as a from "a";"#;
        let result = rewrite_bare_specifiers(content, &filenames);
        assert_eq!(result, r#"import * as a from "./a";"#);

        // Test with .d.cts
        let filenames = vec![
            ("types.d.cts".to_string(), "export {};".to_string()),
            ("index.cts".to_string(), "".to_string()),
        ];

        let content = r#"import { T } from "types";"#;
        let result = rewrite_bare_specifiers(content, &filenames);
        assert_eq!(result, r#"import { T } from "./types";"#);
    }

    #[test]
    fn test_rewrite_bare_specifiers_skips_node_modules_packages() {
        let filenames = vec![
            (
                "/node_modules/foo/foo.js".to_string(),
                "module.exports = {}".to_string(),
            ),
            ("/a.ts".to_string(), "import \"foo\";".to_string()),
        ];

        let content = r#"import "foo";"#;
        let result = rewrite_bare_specifiers(content, &filenames);
        assert_eq!(result, r#"import "foo";"#);
    }

    #[test]
    fn test_prepare_test_dir_preserves_tsconfig() {
        let filenames = vec![
            (
                "/tsconfig.json".to_string(),
                r#"{ "compilerOptions": { "moduleSuffixes": [".ios"] } }"#.to_string(),
            ),
            (
                "/index.ts".to_string(),
                "import { ios } from \"./foo\";".to_string(),
            ),
        ];

        let prepared = prepare_test_dir("", &filenames, &HashMap::new()).unwrap();
        let tsconfig_path = prepared.temp_dir.path().join("tsconfig.json");
        let tsconfig_contents = std::fs::read_to_string(tsconfig_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&tsconfig_contents).unwrap();

        assert_eq!(parsed["compilerOptions"]["moduleSuffixes"][0], ".ios");
        assert!(
            parsed.get("include").is_none(),
            "Expected provided tsconfig to be preserved without injected include"
        );
    }
}
