//! tsz compiler wrapper for conformance testing
//!
//! Provides a simple API to compile TypeScript code and extract error codes.

use std::collections::HashMap;
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
                rewrite_absolute_imports(&c)
            } else {
                let c = resolve_lib_references(file_content, dir_path, ts_tests_lib_dir);
                rewrite_absolute_reference_paths(&c)
            };

            std::fs::write(&file_path, written_content)?;
        }
    }

    // Create tsconfig.json with test options
    let tsconfig_path = dir_path.join("tsconfig.json");
    let has_js_files = filenames.iter().any(|(name, _)| {
        let lower = name.to_lowercase();
        lower.ends_with(".js")
            || lower.ends_with(".jsx")
            || lower.ends_with(".mjs")
            || lower.ends_with(".cjs")
    });
    // Only infer allowJs from JS file extensions when not explicitly set
    let explicit_allow_js = options.get("allowjs");
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
                rewrite_absolute_imports(&c)
            } else {
                // Even without absolute filenames, handle /.lib/ references
                let c = resolve_lib_references(file_content, dir_path, ts_tests_lib_dir);
                rewrite_absolute_reference_paths(&c)
            };

            std::fs::write(&file_path, written_content)?;
        }
    }

    let tsconfig_path = dir_path.join("tsconfig.json");
    let has_js_files = filenames.iter().any(|(name, _)| {
        let lower = name.to_lowercase();
        lower.ends_with(".js")
            || lower.ends_with(".jsx")
            || lower.ends_with(".mjs")
            || lower.ends_with(".cjs")
    });
    // Only infer allowJs from JS file extensions when not explicitly set
    let explicit_allow_js = options.get("allowjs");
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
    let error_codes = extract_error_codes(&diagnostics);
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
/// - Normalizes lowercase directive keys to proper camelCase for tsconfig.json
fn convert_options_to_tsconfig(options: &HashMap<String, String>) -> serde_json::Value {
    let mut opts = serde_json::Map::new();

    for (key, value) in options {
        // Skip test harness-specific directives (keys are already lowercase)
        let key_lower = key.to_lowercase();
        if HARNESS_ONLY_DIRECTIVES
            .iter()
            .any(|&d| d.to_lowercase() == key_lower)
        {
            continue;
        }

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
        } else {
            // For non-list options, take only the first comma-separated value.
            // TSC runs multi-value directives like `@target: esnext, es2022` as
            // separate test runs; our harness runs once with the first value.
            let effective_value = value.split(',').next().unwrap_or(value).trim();
            if let Ok(num) = effective_value.parse::<i64>() {
                // Handle numeric options (e.g., maxNodeModuleJsDepth)
                serde_json::Value::Number(num.into())
            } else {
                serde_json::Value::String(effective_value.to_string())
            }
        };

        // Convert lowercase key to proper camelCase for tsconfig.json
        let tsconfig_key = lowercase_to_camel_case(&key_lower);
        opts.insert(tsconfig_key, json_value);
    }

    serde_json::Value::Object(opts)
}

/// Map lowercase directive keys to proper camelCase tsconfig.json keys.
///
/// TypeScript compiler options use camelCase (e.g., `noImplicitAny`), but our
/// directive parser normalizes keys to lowercase. This function maps them back.
fn lowercase_to_camel_case(key: &str) -> String {
    // Exhaustive map of known compiler options (lowercase → camelCase)
    match key {
        // Simple single-word options (no change needed)
        "strict" | "target" | "module" | "jsx" | "lib" | "declaration" | "incremental"
        | "types" | "paths" => key.to_string(),

        // Strict family
        "noimplicitany" => "noImplicitAny".to_string(),
        "noimplicitreturns" => "noImplicitReturns".to_string(),
        "noimplicitthis" => "noImplicitThis".to_string(),
        "strictnullchecks" => "strictNullChecks".to_string(),
        "strictfunctiontypes" => "strictFunctionTypes".to_string(),
        "strictpropertyinitialization" => "strictPropertyInitialization".to_string(),
        "strictbindcallapply" => "strictBindCallApply".to_string(),
        "strictbuiltiniteratorreturn" => "strictBuiltinIteratorReturn".to_string(),
        "useunknownincatchvariables" => "useUnknownInCatchVariables".to_string(),
        "alwaysstrict" => "alwaysStrict".to_string(),
        "exactoptionalpropertytypes" => "exactOptionalPropertyTypes".to_string(),

        // Module resolution
        "moduleresolution" => "moduleResolution".to_string(),
        "modulesuffixes" => "moduleSuffixes".to_string(),
        "moduledetection" => "moduleDetection".to_string(),
        "resolvepackagejsonexports" => "resolvePackageJsonExports".to_string(),
        "resolvepackagejsonimports" => "resolvePackageJsonImports".to_string(),
        "resolvejsonmodule" => "resolveJsonModule".to_string(),
        "customconditions" => "customConditions".to_string(),
        "typeroots" => "typeRoots".to_string(),
        "baseurl" => "baseUrl".to_string(),
        "rootdir" => "rootDir".to_string(),
        "rootdirs" => "rootDirs".to_string(),

        // Emit
        "outdir" => "outDir".to_string(),
        "outfile" => "outFile".to_string(),
        "sourcemap" => "sourceMap".to_string(),
        "declarationmap" => "declarationMap".to_string(),
        "declarationdir" => "declarationDir".to_string(),
        "noemit" => "noEmit".to_string(),
        "noemitonerror" => "noEmitOnError".to_string(),
        "tsbuildinfofile" => "tsBuildInfoFile".to_string(),

        // Interop
        "esmoduleinterop" => "esModuleInterop".to_string(),
        "allowsyntheticdefaultimports" => "allowSyntheticDefaultImports".to_string(),
        "isolatedmodules" => "isolatedModules".to_string(),
        "experimentaldecorators" => "experimentalDecorators".to_string(),

        // JavaScript
        "allowjs" => "allowJs".to_string(),
        "checkjs" => "checkJs".to_string(),
        "maxnodemodulejsdepth" => "maxNodeModuleJsDepth".to_string(),

        // Checking
        "nolib" => "noLib".to_string(),
        "nounusedlocals" => "noUnusedLocals".to_string(),
        "nounusedparameters" => "noUnusedParameters".to_string(),
        "allowunreachablecode" => "allowUnreachableCode".to_string(),
        "nofallthrough casesinswitch" => "noFallthroughCasesInSwitch".to_string(),
        "nouncheckedindexedaccess" => "noUncheckedIndexedAccess".to_string(),
        "nopropertyaccessfromindexsignature" => "noPropertyAccessFromIndexSignature".to_string(),
        "skiplibrarycheck" | "skiplibcheck" => "skipLibCheck".to_string(),
        "skipdefaultlibcheck" => "skipDefaultLibCheck".to_string(),
        "suppressexcesspropertyerrors" => "suppressExcessPropertyErrors".to_string(),
        "suppressimplicitanyindexerrors" => "suppressImplicitAnyIndexErrors".to_string(),
        "noerrortruncation" => "noErrorTruncation".to_string(),
        "forceconsitentnaminginfilenames" | "forceconsistentcasinginfilenames" => {
            "forceConsistentCasingInFileNames".to_string()
        }

        // JSX
        "jsxfactory" => "jsxFactory".to_string(),
        "jsxfragmentfactory" => "jsxFragmentFactory".to_string(),
        "jsximportsource" => "jsxImportSource".to_string(),

        // Other
        "downleveliteration" => "downlevelIteration".to_string(),
        "importhelpers" => "importHelpers".to_string(),
        "preserveconstenums" => "preserveConstEnums".to_string(),
        "verbatimmodulesyntax" => "verbatimModuleSyntax".to_string(),
        "usedefineforclassfields" => "useDefineForClassFields".to_string(),
        "emitdecoratormetadata" => "emitDecoratorMetadata".to_string(),
        "allowimportingtsextensions" => "allowImportingTsExtensions".to_string(),
        "allowarbitraryextensions" => "allowArbitraryExtensions".to_string(),

        // Fallback: return as-is (already single word or unknown)
        other => other.to_string(),
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
    let output = Command::new(&tsz_path)
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
    use once_cell::sync::Lazy;
    use regex::Regex;

    // Match: "error TS1234:" pattern in diagnostic output
    static DIAG_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"error TS(\d+):").unwrap());

    let mut diagnostics = Vec::new();

    for line in text.lines() {
        if let Some(caps) = DIAG_RE.captures(line) {
            if let Ok(code) = caps[1].parse::<u32>() {
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

    diagnostics
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

/// Strip @ directive comments from test file content
/// Removes lines like `// @strict: true` from the code
/// Uses the same regex as the test parser for precise matching
fn strip_directive_comments(content: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    // Must match exactly the directive pattern: // @word: ...
    static DIRECTIVE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^\s*//\s*@\w+\s*:").unwrap());

    content
        .lines()
        .filter(|line| !DIRECTIVE_RE.is_match(line))
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
    static FROM_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(from\s+)(['"])/((?:[^'"])*)['"]"#).unwrap());

    // Match: import '/...' or import "/..." (side-effect imports)
    static IMPORT_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(import\s+)(['"])/((?:[^'"])*)['"]"#).unwrap());

    // Match: require('/...') or require("/...")
    static REQUIRE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(require\()(['"])/((?:[^'"])*)['"]([)])"#).unwrap());

    // Match: import('/...')
    static DYNAMIC_IMPORT_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(import\()(['"])/((?:[^'"\)])*?)['"]"#).unwrap());

    let result = FROM_RE.replace_all(content, "${1}${2}./${3}${2}");
    let result = IMPORT_RE.replace_all(&result, "${1}${2}./${3}${2}");
    let result = REQUIRE_RE.replace_all(&result, "${1}${2}./${3}${2}${4}");
    let result = DYNAMIC_IMPORT_RE.replace_all(&result, "${1}${2}./${3}${2}");
    result.into_owned()
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
    static LIB_REF_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(///\s*<reference\s+path\s*=\s*)(['"])/.lib/((?:[^'"])*)(?:'|")"#).unwrap()
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

    // Match: /// <reference path="/..." /> (any absolute path)
    static ABS_REF_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(///\s*<reference\s+path\s*=\s*)(['"])/((?:[^'"])*)(?:'|")"#)
            .unwrap()
    });

    // Replace all absolute reference paths EXCEPT /.lib/ (handled separately)
    ABS_REF_RE
        .replace_all(content, |caps: &regex::Captures| {
            let path = &caps[3];
            if path.starts_with(".lib/") {
                // Don't rewrite /.lib/ references — handled by resolve_lib_references
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
    fn test_compile_simple_error() {
        let content = r#"
// @strict: true
const x: number = "string";
"#;
        let result = compile_test(content, &[], &HashMap::new(), "../target/release/tsz").unwrap();
        // Should have type error (TS2322)
        assert!(!result.error_codes.is_empty());
    }

    #[test]
    fn test_compile_no_errors() {
        let content = r#"
// @strict: true
const x: number = 42;
"#;
        let result = compile_test(content, &[], &HashMap::new(), "../target/release/tsz").unwrap();
        // Should have no errors
        assert!(result.error_codes.is_empty());
    }
}
