//! tsz compiler wrapper for conformance testing
//!
//! Provides a simple API to compile TypeScript code and extract error codes.

use crate::tsc_results::DiagnosticFingerprint;
use std::collections::HashMap;
use std::path::Path;
use tsz::diagnostics::{Diagnostic, DiagnosticSeverity};
use tsz::span::Span;

/// Result of compiling a test file
#[derive(Debug, Clone)]
pub struct CompilationResult {
    /// Error codes (TSXXXX format, e.g., 2304 for TS2304)
    pub error_codes: Vec<u32>,
    /// Diagnostic fingerprints for richer mismatch tracking.
    pub diagnostic_fingerprints: Vec<DiagnosticFingerprint>,
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
    let has_tsconfig_file = filenames
        .iter()
        .any(|(name, _)| name.replace('\\', "/").ends_with("tsconfig.json"));
    // Set allowJs when explicitly requested via @allowJs directive,
    // or when @checkJs is true (checkJs implies allowJs, matching tsc's test harness behavior).
    let explicit_allow_js = options.get("allowJs").or_else(|| options.get("allowjs"));
    let check_js = options
        .get("checkJs")
        .or_else(|| options.get("checkjs"))
        .map(|v| v == "true")
        .unwrap_or(false);
    let allow_js = matches!(explicit_allow_js, Some(v) if v == "true") || check_js;
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

    Ok(PreparedTest { temp_dir })
}

/// Parse tsz process output into a CompilationResult.
pub fn parse_tsz_output(
    output: &std::process::Output,
    project_root: &Path,
    options: HashMap<String, String>,
) -> CompilationResult {
    if output.status.success() {
        return CompilationResult {
            error_codes: vec![],
            diagnostic_fingerprints: vec![],
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
                diagnostic_fingerprints: vec![],
                crashed: true,
                options,
            };
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    let diagnostic_fingerprints = parse_diagnostic_fingerprints_from_text(&combined, project_root);
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
        diagnostic_fingerprints,
        crashed: false,
        options,
    }
}

fn parse_diagnostic_fingerprints_from_text(
    text: &str,
    project_root: &Path,
) -> Vec<DiagnosticFingerprint> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static DIAG_WITH_POS_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^(?P<file>.+?)\((?P<line>\d+),(?P<col>\d+)\):\s+error\s+TS(?P<code>\d+):\s*(?P<message>.+)$")
            .expect("valid regex")
    });
    static DIAG_NO_POS_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^error\s+TS(?P<code>\d+):\s*(?P<message>.+)$").unwrap());

    let mut fingerprints = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(caps) = DIAG_WITH_POS_RE.captures(line) {
            let code = caps
                .name("code")
                .and_then(|m| m.as_str().parse::<u32>().ok());
            let line_no = caps
                .name("line")
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);
            let col_no = caps
                .name("col")
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);
            if let Some(code) = code {
                let file = normalize_diagnostic_path(
                    caps.name("file").map(|m| m.as_str()).unwrap_or_default(),
                    project_root,
                );
                let message = caps.name("message").map(|m| m.as_str()).unwrap_or_default();
                fingerprints.push(DiagnosticFingerprint::new(
                    code, file, line_no, col_no, message,
                ));
            }
            continue;
        }

        if let Some(caps) = DIAG_NO_POS_RE.captures(line) {
            if let Some(code) = caps
                .name("code")
                .and_then(|m| m.as_str().parse::<u32>().ok())
            {
                let message = caps.name("message").map(|m| m.as_str()).unwrap_or_default();
                fingerprints.push(DiagnosticFingerprint::new(
                    code,
                    String::new(),
                    0,
                    0,
                    message,
                ));
            }
        }
    }

    fingerprints.sort_by(|a, b| {
        (
            a.code,
            a.file.as_str(),
            a.line,
            a.column,
            a.message_key.as_str(),
        )
            .cmp(&(
                b.code,
                b.file.as_str(),
                b.line,
                b.column,
                b.message_key.as_str(),
            ))
    });
    fingerprints.dedup();
    fingerprints
}

fn parse_diagnostics_from_text(text: &str) -> Vec<Diagnostic> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static DIAG_WITH_POS_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^(?P<file>.+?)\((?P<line>\d+),(?P<col>\d+)\):\s+error\s+TS(?P<code>\d+):\s*(?P<message>.+)$")
            .expect("valid regex")
    });
    static DIAG_NO_POS_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^error\s+TS(?P<code>\d+):\s*(?P<message>.+)$").unwrap());

    let mut diagnostics = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(caps) = DIAG_WITH_POS_RE.captures(line) {
            if let Some(code) = caps
                .name("code")
                .and_then(|m| m.as_str().parse::<u32>().ok())
            {
                let file = caps.name("file").map(|m| m.as_str()).unwrap_or_default();
                let message = caps.name("message").map(|m| m.as_str()).unwrap_or_default();
                diagnostics.push(Diagnostic::error(file, Span::dummy(), message, code));
            }
            continue;
        }

        if let Some(caps) = DIAG_NO_POS_RE.captures(line) {
            if let Some(code) = caps
                .name("code")
                .and_then(|m| m.as_str().parse::<u32>().ok())
            {
                let message = caps.name("message").map(|m| m.as_str()).unwrap_or_default();
                diagnostics.push(Diagnostic::error("", Span::dummy(), message, code));
            }
        }
    }

    diagnostics
}

fn normalize_diagnostic_path(raw: &str, project_root: &Path) -> String {
    let normalized = raw.trim().replace('\\', "/");
    if normalized.is_empty() {
        return normalized;
    }

    // Build a set of equivalent root prefixes. On macOS, the same temp directory
    // may appear as either /var/... or /private/var/... in diagnostics.
    let mut roots = Vec::new();
    roots.push(project_root.to_string_lossy().replace('\\', "/"));
    if let Ok(canon_root) = project_root.canonicalize() {
        roots.push(canon_root.to_string_lossy().replace('\\', "/"));
    }

    let mut expanded_roots = Vec::new();
    for root in roots {
        if root.is_empty() {
            continue;
        }
        expanded_roots.push(root.clone());
        if let Some(stripped) = root.strip_prefix("/private") {
            if stripped.starts_with("/var/") {
                expanded_roots.push(stripped.to_string());
            }
        }
        if root.starts_with("/var/") {
            expanded_roots.push(format!("/private{}", root));
        }
    }

    expanded_roots.sort_by_key(|r| std::cmp::Reverse(r.len()));
    expanded_roots.dedup();

    for root in &expanded_roots {
        if normalized.starts_with(root) {
            return normalized[root.len()..].trim_start_matches('/').to_string();
        }
    }

    // If the diagnostic path is absolute, try canonicalizing it and strip again.
    let diag_path = Path::new(&normalized);
    if diag_path.is_absolute() {
        if let Ok(canon_diag) = diag_path.canonicalize() {
            let canon_diag = canon_diag.to_string_lossy().replace('\\', "/");
            for root in &expanded_roots {
                if canon_diag.starts_with(root) {
                    return canon_diag[root.len()..].trim_start_matches('/').to_string();
                }
            }
            return canon_diag;
        }
    }

    normalized
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

    // Match tsc default compiler behavior for tests that omit @target.
    // TypeScript defaults target to ES5 when not specified.
    if !opts.contains_key("target") {
        opts.insert(
            "target".to_string(),
            serde_json::Value::String("es5".to_string()),
        );
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
#[path = "tests/tsz_wrapper.rs"]
mod tests;
