//! tsz compiler wrapper for conformance testing
//!
//! Provides a simple API to compile TypeScript code and extract error codes.

use crate::compiler_options::directives_to_tsconfig;
use crate::tsc_results::DiagnosticFingerprint;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod path_helpers;
use path_helpers::is_windows_absolute_path;

const SEMANTIC_COMPLETION_MARKER_PREFIX: &str = "---TSZ-SEMANTIC-COMPLETION:";

/// Process-level semantic verdict reported by the clean-slate compiler.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SemanticCompletion {
    Complete,
    Deferred,
    Cycle,
    Limit,
    /// Fresh-process exit 3 carries the nonclaim but intentionally does not
    /// add protocol text to ordinary CLI output.
    #[default]
    #[serde(other)]
    Incomplete,
}

impl SemanticCompletion {
    pub const fn is_complete(self) -> bool {
        matches!(self, Self::Complete)
    }
}

/// Result of compiling a test file
#[derive(Debug, Clone)]
pub struct CompilationResult {
    /// Error codes (TSXXXX format, e.g., 2304 for TS2304)
    pub error_codes: Vec<u32>,
    /// Diagnostic fingerprints for richer mismatch tracking.
    pub diagnostic_fingerprints: Vec<DiagnosticFingerprint>,
    /// Whether compilation crashed (panic)
    pub crashed: bool,
    /// Explicit nonclaim when checking escaped without a definitive semantic
    /// result. This never contributes a synthetic diagnostic code.
    pub semantic_completion: SemanticCompletion,
    /// Exact ordinary process exits, in TS7 configuration-selector order.
    /// Fresh compilation owns one value; aggregate rows own one per variant.
    /// Pooled transports cannot provide this evidence and leave it empty.
    pub ordinary_exit_statuses: Vec<u8>,
    /// Resolved compiler options used
    pub options: HashMap<String, String>,
}

/// Prepared test directory ready for async compilation.
/// The temp directory is deleted on drop, so keep this alive during compilation.
pub struct PreparedTest {
    /// Temp directory containing test files and tsconfig.json
    pub temp_dir: tempfile::TempDir,
    /// Project directory passed to tsc/tsz via `-p` and used as cwd.
    pub project_dir: std::path::PathBuf,
}

/// Prepare a test directory with files and tsconfig.json for compilation.
///
/// Returns a `PreparedTest` whose temp directory must be kept alive during compilation.
/// Use this with `tokio::process::Command` + `kill_on_drop(true)` for proper timeout handling.
///
/// `original_extension` is the file extension of the original test file (e.g. "tsx"),
/// used when there are no `@Filename` directives so the single-file test preserves its extension.
// Dead in the lib/bin build; the thin wrapper over `prepare_test_dir_with_lib_dir`
// is exercised only by `tests/tsz_wrapper.rs`, so `allow` (not `expect`) is correct.
#[allow(dead_code)]
pub fn prepare_test_dir(
    content: &str,
    filenames: &[(String, String)],
    options: &HashMap<String, String>,
    original_extension: Option<&str>,
    key_order: &[String],
) -> anyhow::Result<PreparedTest> {
    prepare_test_dir_with_lib_dir(
        content,
        filenames,
        options,
        original_extension,
        key_order,
        None,
    )
}

/// Derive the TypeScript harness `tests/lib` directory from a conformance
/// `tests/cases` directory.
pub fn tests_lib_dir_for_cases_dir(test_dir: &Path) -> std::path::PathBuf {
    let test_dir = test_dir
        .canonicalize()
        .unwrap_or_else(|_| test_dir.to_path_buf());
    if test_dir.file_name().is_some_and(|name| name == "cases") {
        if let Some(tests_dir) = test_dir.parent() {
            return tests_dir.join("lib");
        }
    }
    std::path::PathBuf::from("TypeScript/tests/lib")
}

/// Prepare a test directory, using an explicit TypeScript harness lib directory
/// for `/.lib/...` references when the caller knows the source test root.
pub fn prepare_test_dir_with_lib_dir(
    content: &str,
    filenames: &[(String, String)],
    options: &HashMap<String, String>,
    original_extension: Option<&str>,
    key_order: &[String],
    ts_tests_lib_dir: Option<&Path>,
) -> anyhow::Result<PreparedTest> {
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let dir_path = temp_dir.path();
    if std::env::var_os("TSZ_DEBUG_PREPARE_DIR").is_some() {
        eprintln!(
            "[tsz_wrapper] prepare_test_dir temp_dir={}",
            dir_path.display()
        );
    }

    // Parse @symlink associations from raw content
    // Format: @filename: /path/to/file followed by @symlink: /link1,/link2
    let symlink_map = parse_symlink_associations(content);
    let link_map = parse_link_associations(content);

    // Detect if any filename uses absolute (virtual root) paths
    // Includes both Unix-style (/foo) and Windows-style (A:/foo) absolute paths
    let has_absolute_filenames = filenames
        .iter()
        .any(|(name, _)| name.starts_with('/') || is_windows_absolute_path(name));
    let project_dir = determine_project_dir(dir_path, filenames, options);

    // Check if ALL filenames are Windows-style absolute paths (e.g., A:/foo/bar.ts).
    // These represent paths on a separate drive root that cannot exist on Unix.
    // tsc's virtual filesystem can't find files at these paths via include patterns,
    // so it emits TS18003 ("No inputs found"). We replicate this by not writing
    // Windows-path files, leaving the temp dir empty.
    let all_windows_paths = !filenames.is_empty()
        && filenames
            .iter()
            .filter(|(name, _)| !name.replace('\\', "/").ends_with("tsconfig.json"))
            .all(|(name, _)| is_windows_absolute_path(name));

    // Path to TypeScript test harness lib files (for /.lib/ references)
    let fallback_ts_tests_lib_dir = std::path::PathBuf::from("TypeScript/tests/lib");
    let ts_tests_lib_dir = ts_tests_lib_dir.unwrap_or(fallback_ts_tests_lib_dir.as_path());

    if filenames.is_empty() {
        let stripped_content = strip_directive_comments(content);
        // Handle /.lib/ references and absolute reference paths in single-file tests
        let stripped_content =
            resolve_lib_references(&stripped_content, dir_path, ts_tests_lib_dir);
        let stripped_content = rewrite_absolute_reference_paths(&stripped_content);
        let ext = original_extension.unwrap_or("ts");
        let main_file = dir_path.join(format!("test.{ext}"));
        std::fs::write(&main_file, stripped_content)?;
    } else {
        for (filename, file_content) in filenames {
            // Skip Windows-style absolute paths when ALL non-tsconfig files use them.
            // These paths refer to a different drive root that can't exist on Unix;
            // tsc doesn't find these files and emits TS18003.
            if all_windows_paths && is_windows_absolute_path(filename) {
                continue;
            }
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
                rewrite_bare_specifiers(&c, filename, filenames)
            } else {
                // Even without absolute filenames, handle /.lib/ references and bare specifiers
                let c = resolve_lib_references(file_content, dir_path, ts_tests_lib_dir);
                let c = rewrite_absolute_reference_paths(&c);
                rewrite_bare_specifiers(&c, filename, filenames)
            };

            std::fs::write(&file_path, written_content)?;
            if std::env::var_os("TSZ_DEBUG_PREPARE_DIR").is_some() {
                eprintln!(
                    "[tsz_wrapper] wrote {} (orig={})",
                    file_path.display(),
                    filename
                );
            }
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
                // Copy file instead of symlinking to match tsc's VFS behavior.
                // tsc's test harness creates separate file instances for symlinked paths,
                // so each copy gets its own SymbolIds and private brands, which is needed
                // for TS2322 diagnostics on classes with private members.
                let _ = std::fs::copy(&source_path, &link_path);
            }
        }
    }

    // Create path aliases from @link directives. Unlike @symlink metadata, these
    // need real symlink behavior because package-resolution tests depend on the
    // link path being preserved separately from the target's real path.
    for (target_path, link_path) in &link_map {
        let sanitized_link = link_path
            .replace("..", "_")
            .trim_start_matches('/')
            .to_string();
        let link_path = dir_path.join(&sanitized_link);
        let sanitized_target = target_path
            .replace("..", "_")
            .trim_start_matches('/')
            .to_string();
        let target_path = dir_path.join(&sanitized_target);

        if !target_path.exists() {
            continue;
        }
        if let Some(parent) = link_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = std::fs::remove_file(&link_path);
        let _ = std::fs::remove_dir(&link_path);
        create_symlink_path(&target_path, &link_path)?;
    }

    let tsconfig_path = project_dir.join("tsconfig.json");
    if let Some(parent) = tsconfig_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let has_tsconfig_file = filenames
        .iter()
        .any(|(name, _)| name.replace('\\', "/").ends_with("tsconfig.json"));
    // Set allowJs when explicitly requested via @allowJs directive.
    // Do not force allowJs=true when checkJs=true if allowJs is explicitly false;
    // tsc emits TS5052 in that configuration.
    let explicit_allow_js = options.get("allowJs").or_else(|| options.get("allowjs"));
    let check_js = options
        .get("checkJs")
        .or_else(|| options.get("checkjs"))
        .is_some_and(|v| v == "true");
    let allow_js = matches!(explicit_allow_js, Some(v) if v == "true");
    let no_implicit_references = options
        .get("noImplicitReferences")
        .or_else(|| options.get("noimplicitreferences"))
        .is_some_and(|v| v == "true");
    let no_types_and_symbols = no_types_and_symbols_enabled(options);
    let harness_root_files: Option<Vec<String>> = if no_implicit_references && !filenames.is_empty()
    {
        let files: Vec<String> = filenames
            .iter()
            .filter_map(|(name, _)| {
                let normalized = name.replace('\\', "/");
                if normalized.ends_with("tsconfig.json") {
                    return None;
                }
                // When types is set, @types files are discovered via that
                // mechanism — don't also add them as explicit root files.
                // tsc's harness only adds non-node_modules files as roots.
                if normalized.contains("/node_modules/") || normalized.starts_with("node_modules/")
                {
                    return None;
                }
                // Package roots linked into node_modules are resolution inputs,
                // not explicit roots. Keeping their declarations out of the
                // root list preserves declaration-emit provenance for package
                // references while leaving normal authored .d.ts roots intact.
                if declaration_file_linked_into_node_modules(&normalized, &link_map) {
                    return None;
                }
                // tsc's harness also excludes typings/ directories and package.json
                // when noImplicitReferences is set — only user source files are roots.
                if normalized.starts_with("typings/") || normalized.contains("/typings/") {
                    return None;
                }
                if normalized.ends_with("/package.json") || normalized == "package.json" {
                    return None;
                }
                Some(name.replace("..", "_").trim_start_matches('/').to_string())
            })
            .collect();
        if files.is_empty() {
            None
        } else {
            Some(files)
        }
    } else {
        None
    };
    // Match tsc's test harness default include patterns.
    // tsc's harness generates: ["*.ts","*.tsx","*.js","*.jsx","**/*.ts","**/*.tsx","**/*.js","**/*.jsx"]
    // Note: these patterns do NOT include .mts/.cts/.mjs/.cjs extensions.
    // In standard glob semantics, *.ts does NOT match .mts files.
    // tsc's real default include is ["**/*"] which matches everything (then filters
    // by supported extensions), but the test harness uses explicit extension patterns.
    // This means tests with ONLY .mts/.cts files correctly get TS18003 "no inputs found",
    // matching tsc behavior.
    let include = serde_json::json!([
        "*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"
    ]);
    // For multi-file tests with authored virtual files, prefer an explicit
    // "files" list over discovery-only include globs when the fixture shape
    // can't be represented by the harness defaults:
    // - .mts/.cts/.mjs/.cjs files are not matched by the narrow include globs
    // - files under node_modules are intentionally authored fixture inputs
    //
    // tsc's harness passes the authored files directly to the compiler; using an
    // explicit root-file list keeps our synthetic tsconfig aligned with that shape.
    //
    let needs_explicit_root_files = !filenames.is_empty()
        && filenames.iter().any(|(name, _)| {
            let lower = name.to_lowercase().replace('\\', "/");
            lower.ends_with(".mts")
                || lower.ends_with(".cts")
                || lower.ends_with(".mjs")
                || lower.ends_with(".cjs")
                || lower.contains("/node_modules/")
                || lower.starts_with("node_modules/")
        });
    // Names listed in `compilerOptions.types` (when not the `*` wildcard).
    // Each name like "node" maps to `node_modules/@types/node/...`. When such
    // a file is BOTH listed in `files` and discoverable via typeRoots, tsc
    // can load it twice via different absolute paths (e.g. on macOS,
    // `/var/.../node_modules/@types/node/index.d.ts` from `files` and
    // `/private/var/.../node_modules/@types/node/index.d.ts` from typeRoots
    // canonicalization). The double-load produces a synthetic TS2451 for any
    // block-scoped global like `declare const require`. tsz canonicalizes
    // paths uniformly and dedupes correctly, so we mirror that here by not
    // adding the @types file to the explicit `files` array when its package
    // name is already covered by `types`. typeRoots discovery still loads it.
    let types_packages_in_options: Vec<String> = options
        .get("types")
        .map(|raw| {
            raw.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && s != "*")
                .collect()
        })
        .unwrap_or_default();
    let explicit_root_files: Option<Vec<String>> = if needs_explicit_root_files {
        let root_files: Vec<String> = filenames
            .iter()
            .filter_map(|(name, _)| {
                let lower = name.to_lowercase().replace('\\', "/");
                if lower.ends_with("tsconfig.json") || lower.ends_with("package.json") {
                    return None;
                }
                // When noTypesAndSymbols is set, tsc's harness does NOT
                // include @types files as root files — they remain on disk
                // for module resolution but aren't loaded into the program
                // unless the `types` config allows auto-discovery.
                // Without this filter, ambient module declarations in
                // @types packages pollute the global scope unconditionally.
                if no_types_and_symbols
                    && (lower.contains("/node_modules/@types/")
                        || lower.starts_with("node_modules/@types/"))
                {
                    return None;
                }
                // When this @types file is covered by `compilerOptions.types`,
                // skip listing it explicitly in `files`. Listing it both
                // places causes tsc on macOS to double-load it (because of
                // /var → /private/var symlink canonicalization differences
                // between the `files` and typeRoots resolution paths),
                // producing spurious TS2451 diagnostics for ambient
                // block-scoped globals like `declare const require`.
                if !types_packages_in_options.is_empty()
                    && atypes_package_in(lower.as_str())
                        .is_some_and(|pkg| types_packages_in_options.contains(&pkg))
                {
                    return None;
                }
                Some(name.replace("..", "_").trim_start_matches('/').to_string())
            })
            .collect();
        if root_files.is_empty() {
            None
        } else {
            Some(root_files)
        }
    } else {
        None
    };
    if !has_tsconfig_file {
        let mut compiler_options = convert_options_to_tsconfig(options, key_order);
        if let serde_json::Value::Object(ref mut map) = compiler_options {
            // TypeScript 6.0+ defaults all strict-family flags to true.
            // No synthetic non-strict baseline is needed; the compiler
            // handles these defaults correctly via resolve_bool.
            // Remap virtual absolute compiler-option paths to real tmpdir paths.
            // Tests use `/...` paths in a virtual FS rooted at the harness cwd;
            // our wrapper writes those files under `<tmpdir>/...`, so options
            // that point at absolute virtual paths need the same translation.
            if has_absolute_filenames {
                for key in [
                    "baseUrl",
                    "declarationDir",
                    "mapRoot",
                    "outDir",
                    "rootDir",
                    "sourceRoot",
                ] {
                    if let Some(value) = map.get_mut(key) {
                        match value {
                            serde_json::Value::String(value) if value.starts_with('/') => {
                                *value = dir_path
                                    .join(value.trim_start_matches('/'))
                                    .to_string_lossy()
                                    .into_owned();
                            }
                            _ => {}
                        }
                    }
                }
                if let Some(serde_json::Value::Array(ref mut roots)) = map.get_mut("rootDirs") {
                    for root in roots.iter_mut() {
                        if let serde_json::Value::String(s) = root {
                            if s.starts_with('/') {
                                *s = dir_path
                                    .join(s.trim_start_matches('/'))
                                    .to_string_lossy()
                                    .into_owned();
                            }
                        }
                    }
                }
                if let Some(serde_json::Value::Array(ref mut roots)) = map.get_mut("typeRoots") {
                    for root in roots.iter_mut() {
                        if let serde_json::Value::String(s) = root {
                            if s.starts_with('/') {
                                *s = dir_path
                                    .join(s.trim_start_matches('/'))
                                    .to_string_lossy()
                                    .into_owned();
                            }
                        }
                    }
                }
            }
            if check_js {
                if explicit_allow_js.is_none() {
                    // Keep historical harness behavior for tests that set checkJs
                    // without explicitly specifying allowJs.
                    map.insert("allowJs".to_string(), serde_json::Value::Bool(true));
                }
                map.insert("checkJs".to_string(), serde_json::Value::Bool(true));
            } else if allow_js {
                map.entry("allowJs")
                    .or_insert(serde_json::Value::Bool(true));
            }
        }
        let tsconfig_content = if let Some(root_files) = harness_root_files {
            serde_json::json!({
                "compilerOptions": compiler_options,
                "files": root_files,
                "exclude": ["node_modules"]
            })
        } else if let Some(root_files) = &explicit_root_files {
            // Keep authored fixture files explicit so mixed-extension and
            // node_modules-backed tests match the TypeScript harness project
            // shape. Include globs stay in place for default-library discovery
            // and TS18003 parity on non-explicit files.
            if no_types_and_symbols {
                // When noTypesAndSymbols is set, exclude @types from include
                // discovery to match tsc's harness behavior where @types
                // packages aren't auto-included in the program.
                serde_json::json!({
                    "compilerOptions": compiler_options,
                    "include": include,
                    "files": root_files,
                    "exclude": ["node_modules/@types"]
                })
            } else {
                serde_json::json!({
                    "compilerOptions": compiler_options,
                    "include": include,
                    "files": root_files
                })
            }
        } else {
            serde_json::json!({
                "compilerOptions": compiler_options,
                "include": include,
                "exclude": ["node_modules"]
            })
        };
        std::fs::write(
            &tsconfig_path,
            serde_json::to_string_pretty(&tsconfig_content)?,
        )?;
        if std::env::var_os("TSZ_DEBUG_PREPARE_DIR").is_some() {
            eprintln!(
                "[tsz_wrapper] wrote default tsconfig at {}",
                tsconfig_path.display()
            );
            if let Ok(content) = std::fs::read_to_string(&tsconfig_path) {
                eprintln!("[tsz_wrapper] tsconfig content:\n{}", content);
            }
        }
    } else {
        copy_tsconfig_to_project_if_needed(dir_path, &project_dir, filenames, options)?;
        if std::env::var_os("TSZ_DEBUG_PREPARE_DIR").is_some() {
            eprintln!(
                "[tsz_wrapper] copied tsconfig to root at {}",
                tsconfig_path.display()
            );
            if let Ok(content) = std::fs::read_to_string(&tsconfig_path) {
                eprintln!("[tsz_wrapper] tsconfig content:\n{}", content);
            }
        }
    }

    Ok(PreparedTest {
        temp_dir,
        project_dir,
    })
}

/// Prepare a test directory from raw (non-UTF8) bytes.
///
/// Binary fixtures are intentionally preserved as bytes so `tsz` can run its
/// own binary-file diagnostics (TS1490) on the test content.
pub fn prepare_binary_test_dir(
    bytes: &[u8],
    ext: &str,
    options: &HashMap<String, String>,
) -> anyhow::Result<PreparedTest> {
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let dir_path = temp_dir.path();

    let main_file = dir_path.join(format!("test.{}", ext));
    std::fs::write(&main_file, bytes)?;

    let tsconfig_path = dir_path.join("tsconfig.json");
    let has_tsconfig_file = options
        .get("tsconfig")
        .is_some_and(|value| value == "false");

    if !has_tsconfig_file {
        // Match tsc 6.0's include defaults — always list .js/.jsx extensions.
        let include = serde_json::json!([
            "*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"
        ]);

        let compiler_options = convert_options_to_tsconfig(options, &[]);

        let tsconfig_content = serde_json::json!({
            "compilerOptions": compiler_options,
            "include": include,
            "exclude": ["node_modules"]
        });
        std::fs::write(
            &tsconfig_path,
            serde_json::to_string_pretty(&tsconfig_content)?,
        )?;
    }

    Ok(PreparedTest {
        project_dir: dir_path.to_path_buf(),
        temp_dir,
    })
}

fn determine_project_dir(
    dir_path: &Path,
    filenames: &[(String, String)],
    options: &HashMap<String, String>,
) -> std::path::PathBuf {
    // Check for currentDirectory directive - it overrides the default project dir
    // when all files are within that directory
    let current_dir = options.get("currentdirectory").and_then(|s| {
        let normalized = s.replace('\\', "/");
        // Handle "/" specially - it should remain as "/" not empty string
        if normalized == "/" {
            Some("/".to_string())
        } else {
            let trimmed = normalized.trim_start_matches('/').to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
    });

    let mut top_level_dir: Option<String> = None;
    let mut saw_package_json = false;

    for (name, _) in filenames {
        let normalized = name.replace('\\', "/");
        if !normalized.starts_with('/') {
            continue;
        }

        let trimmed = normalized.trim_start_matches('/');
        let mut parts = trimmed.split('/');
        let Some(first) = parts.next() else {
            continue;
        };
        let Some(second) = parts.next() else {
            continue;
        };

        if first == "node_modules" {
            continue;
        }

        match &top_level_dir {
            Some(existing) if existing != first => return dir_path.to_path_buf(),
            None => top_level_dir = Some(first.to_string()),
            _ => {}
        }

        if second == "package.json" {
            saw_package_json = true;
        }
    }

    // If currentDirectory is specified and all files are within it, use it as project dir
    if let Some(current_dir) = current_dir {
        // Special case: "/" means root, so use the temp dir itself
        if current_dir == "/" {
            return dir_path.to_path_buf();
        }
        if let Some(ref top_level) = top_level_dir {
            // currentDirectory might be a path like "src" - check if files are under it
            if top_level == &current_dir || current_dir.starts_with(&format!("{}/", top_level)) {
                return dir_path.join(&current_dir);
            }
        }
        // If files are not absolute or currentDirectory doesn't match, check if we can use it anyway
        // when there's a matching subdirectory
        let candidate = dir_path.join(&current_dir);
        if candidate.is_dir() {
            return candidate;
        }
    }

    if saw_package_json {
        if let Some(top_level_dir) = top_level_dir {
            return dir_path.join(top_level_dir);
        }
    }

    dir_path.to_path_buf()
}

/// Parse tsz process output into a CompilationResult.
pub fn parse_tsz_output(
    output: &std::process::Output,
    project_root: &Path,
    options: HashMap<String, String>,
) -> CompilationResult {
    if std::env::var_os("TSZ_DEBUG_CONFORMANCE_OUTPUT").is_some() {
        eprintln!("----- tsz output for {} -----", project_root.display());
        eprintln!("--- stdout\n{}", String::from_utf8_lossy(&output.stdout));
        eprintln!("--- stderr\n{}", String::from_utf8_lossy(&output.stderr));
    }

    if output.status.success() && output.stdout.is_empty() && output.stderr.is_empty() {
        return CompilationResult {
            error_codes: vec![],
            diagnostic_fingerprints: vec![],
            crashed: false,
            semantic_completion: SemanticCompletion::Complete,
            ordinary_exit_statuses: vec![0],
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
                semantic_completion: SemanticCompletion::Incomplete,
                ordinary_exit_statuses: Vec::new(),
                options,
            };
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut parsed_stdout = parse_diagnostic_output(&stdout, project_root);
    let parsed_stderr = parse_diagnostic_output(&stderr, project_root);
    let fully_covered = std::str::from_utf8(&output.stdout).is_ok()
        && std::str::from_utf8(&output.stderr).is_ok()
        && parsed_stdout.fully_covered
        && parsed_stderr.fully_covered;
    let cross_stream_diagnostics = !parsed_stdout.diagnostic_fingerprints.is_empty()
        && !parsed_stderr.diagnostic_fingerprints.is_empty();
    parsed_stdout.error_codes.extend(parsed_stderr.error_codes);
    parsed_stdout
        .diagnostic_fingerprints
        .extend(parsed_stderr.diagnostic_fingerprints);
    let has_diagnostics = !parsed_stdout.diagnostic_fingerprints.is_empty();
    let status_code = output.status.code();
    let crashed = match status_code {
        Some(1 | 2) => !fully_covered || !has_diagnostics || cross_stream_diagnostics,
        Some(0 | 3) => !fully_covered || cross_stream_diagnostics,
        Some(_) | None => true,
    };
    CompilationResult {
        error_codes: parsed_stdout.error_codes,
        diagnostic_fingerprints: parsed_stdout.diagnostic_fingerprints,
        crashed,
        semantic_completion: if !crashed && matches!(status_code, Some(0..=2)) {
            SemanticCompletion::Complete
        } else {
            SemanticCompletion::Incomplete
        },
        ordinary_exit_statuses: if !crashed {
            status_code
                .and_then(|status| u8::try_from(status).ok())
                .filter(|status| *status <= 2)
                .into_iter()
                .collect()
        } else {
            Vec::new()
        },
        options,
    }
}

struct ParsedDiagnosticOutput {
    error_codes: Vec<u32>,
    diagnostic_fingerprints: Vec<DiagnosticFingerprint>,
    fully_covered: bool,
}

fn parse_diagnostic_output(text: &str, project_root: &Path) -> ParsedDiagnosticOutput {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static SUMMARY_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^Found (?P<count>\d+) errors?(?: in \d+ files?)?\.$")
            .expect("valid diagnostic summary regex")
    });

    let mut error_codes = Vec::new();
    let mut diagnostic_fingerprints = Vec::new();
    let mut current: Option<DiagnosticFingerprint> = None;
    let mut fully_covered = true;
    let mut summary_count = None;

    for raw_line in text.split_inclusive('\n') {
        let line = raw_line.strip_suffix('\n').map_or(raw_line, |line| {
            // Normalize a CRLF transport delimiter, but preserve a bare
            // carriage return because it is part of the diagnostic payload.
            line.strip_suffix('\r').unwrap_or(line)
        });
        if line.chars().all(char::is_whitespace) {
            if let Some(fingerprint) = current.as_mut() {
                fingerprint
                    .continuations
                    .push(normalize_message_paths(line, project_root));
            } else {
                fully_covered = false;
            }
            continue;
        }

        if line
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_whitespace())
        {
            if let Some(fingerprint) = current.as_mut() {
                fingerprint
                    .continuations
                    .push(normalize_message_paths(line, project_root));
            } else {
                fully_covered = false;
            }
            continue;
        }

        if let Some(fingerprint) = parse_primary_diagnostic(line, project_root) {
            error_codes.push(fingerprint.code);
            if let Some(previous) = current.replace(fingerprint) {
                diagnostic_fingerprints.push(previous);
            }
            continue;
        }

        if let Some(previous) = current.take() {
            diagnostic_fingerprints.push(previous);
        }
        if let Some(captures) = SUMMARY_RE.captures(line) {
            let observed = captures
                .name("count")
                .and_then(|value| value.as_str().parse::<usize>().ok());
            if summary_count.replace(observed).is_some() || observed.is_none() {
                fully_covered = false;
            }
        } else {
            fully_covered = false;
        }
    }

    if let Some(previous) = current {
        diagnostic_fingerprints.push(previous);
    }
    if summary_count
        .flatten()
        .is_some_and(|count| count != diagnostic_fingerprints.len())
    {
        fully_covered = false;
    }
    ParsedDiagnosticOutput {
        error_codes,
        diagnostic_fingerprints,
        fully_covered,
    }
}

#[cfg(test)]
fn parse_diagnostic_fingerprints_from_text(
    text: &str,
    project_root: &Path,
) -> Vec<DiagnosticFingerprint> {
    parse_diagnostic_output(text, project_root).diagnostic_fingerprints
}

fn parse_primary_diagnostic(line: &str, project_root: &Path) -> Option<DiagnosticFingerprint> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static DIAG_WITH_POS_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^(?P<file>.+?)\((?P<line>\d+),(?P<col>\d+)\):\s+(?:error|warning|suggestion|message)\s+TS(?P<code>\d+): ?(?P<message>.*)$")
            .expect("valid regex")
    });
    static DIAG_NO_POS_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"^(?::\s*)?(?:error|warning|suggestion|message)\s+TS(?P<code>\d+): ?(?P<message>.*)$",
        )
        .unwrap()
    });

    if let Some(caps) = DIAG_WITH_POS_RE.captures(line) {
        let code = caps.name("code")?.as_str().parse::<u32>().ok()?;
        let line_no = caps.name("line")?.as_str().parse::<u32>().ok()?;
        let col_no = caps.name("col")?.as_str().parse::<u32>().ok()?;
        let file = normalize_diagnostic_path(caps.name("file")?.as_str(), project_root);
        let raw_message = caps.name("message").map(|m| m.as_str()).unwrap_or_default();
        let message = normalize_message_paths(raw_message, project_root);
        return Some(DiagnosticFingerprint::new(
            code, file, line_no, col_no, &message,
        ));
    }

    let caps = DIAG_NO_POS_RE.captures(line)?;
    let code = caps.name("code")?.as_str().parse::<u32>().ok()?;
    let raw_message = caps.name("message").map(|m| m.as_str()).unwrap_or_default();
    let message = normalize_message_paths(raw_message, project_root);
    Some(DiagnosticFingerprint::new(
        code,
        String::new(),
        0,
        0,
        &message,
    ))
}

fn normalize_diagnostic_path(raw: &str, project_root: &Path) -> String {
    if raw.is_empty() {
        return String::new();
    }
    strip_exact_transport_root(raw, project_root).unwrap_or_else(|| raw.to_string())
}

/// Strip only the exact invocation-owned temp root embedded in messages.
///
/// tsz resolves `/// <reference path="lib.ts" />` to an absolute path like
/// `/private/var/.../lib.ts` in the error message. We strip the project root prefix
/// so the message stores portable relative paths (e.g., `File 'lib.ts' not found.`).
/// Every other path and every non-path message byte remains observable.
fn normalize_message_paths(message: &str, project_root: &Path) -> String {
    exact_transport_roots(project_root)
        .into_iter()
        .fold(message.to_string(), |text, root| {
            let windows_root = root.replace('/', "\\");
            text.replace(&format!("{root}/"), "")
                .replace(&format!("{windows_root}\\"), "")
                .replace(&format!("'{root}'"), "''")
                .replace(&format!("'{windows_root}'"), "''")
        })
}

fn exact_transport_roots(project_root: &Path) -> Vec<String> {
    let root = project_root.to_string_lossy().replace('\\', "/");
    if root.is_empty() {
        return Vec::new();
    }
    let mut roots = vec![root.clone()];
    if let Some(without_private) = root.strip_prefix("/private/var/") {
        roots.push(format!("/var/{without_private}"));
    } else if root.starts_with("/var/") {
        roots.push(format!("/private{root}"));
    }
    roots.sort_by_key(|value| std::cmp::Reverse(value.len()));
    roots.dedup();
    roots
}

fn strip_exact_transport_root(path: &str, project_root: &Path) -> Option<String> {
    exact_transport_roots(project_root)
        .into_iter()
        .flat_map(|root| {
            let windows_root = root.replace('/', "\\");
            [root, windows_root]
        })
        .find_map(|root| {
            path.strip_prefix(&root)
                .filter(|rest| rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\'))
                .map(|rest| rest.trim_start_matches(['/', '\\']).to_string())
        })
}

fn no_types_and_symbols_enabled(options: &HashMap<String, String>) -> bool {
    options
        .get("noTypesAndSymbols")
        .or_else(|| options.get("notypesandsymbols"))
        .is_some_and(|value| value == "true")
}

/// Extract the `@types/<package>` name from a path containing the `@types/`
/// segment. Handles both regular packages (`@types/node`) and scoped packages
/// (`@types/scope__pkg`, the de-mangled form of `@scope/pkg`). The returned
/// name is normalized to the form that appears in `compilerOptions.types`
/// (e.g. `node`, `@scope/pkg`).
fn atypes_package_in(lower_path: &str) -> Option<String> {
    const NEEDLE_SLASH: &str = "/node_modules/@types/";
    const NEEDLE_PREFIX: &str = "node_modules/@types/";
    let rest = if let Some(idx) = lower_path.find(NEEDLE_SLASH) {
        &lower_path[idx + NEEDLE_SLASH.len()..]
    } else {
        lower_path.strip_prefix(NEEDLE_PREFIX)?
    };
    let segment = rest.split('/').next()?;
    if segment.is_empty() {
        return None;
    }
    if let Some((scope, pkg)) = segment.split_once("__") {
        Some(format!("@{scope}/{pkg}"))
    } else {
        Some(segment.to_string())
    }
}

fn declaration_file_linked_into_node_modules(
    normalized_path: &str,
    link_map: &[(String, String)],
) -> bool {
    let lower_path = normalized_path.to_ascii_lowercase();
    if !(lower_path.ends_with(".d.ts")
        || lower_path.ends_with(".d.mts")
        || lower_path.ends_with(".d.cts"))
    {
        return false;
    }

    link_map.iter().any(|(target, link)| {
        let target = target
            .replace("..", "_")
            .trim_start_matches('/')
            .replace('\\', "/");
        let link = link
            .replace("..", "_")
            .trim_start_matches('/')
            .replace('\\', "/");
        (link.contains("/node_modules/") || link.starts_with("node_modules/"))
            && (normalized_path == target
                || normalized_path
                    .strip_prefix(target.as_str())
                    .is_some_and(|rest| rest.starts_with('/')))
    })
}

/// Convert test directive options to tsconfig compiler options.
///
/// `key_order` is kept for compatibility with call sites. The actual conversion
/// is shared with the cache generators so runner/cache option shapes cannot
/// drift.
fn convert_options_to_tsconfig(
    options: &HashMap<String, String>,
    _key_order: &[String],
) -> serde_json::Value {
    directives_to_tsconfig(options)
}

fn copy_tsconfig_to_project_if_needed(
    dir_path: &Path,
    project_dir: &Path,
    filenames: &[(String, String)],
    options: &HashMap<String, String>,
) -> anyhow::Result<()> {
    let target_tsconfig = project_dir.join("tsconfig.json");
    let tsconfig_source = filenames
        .iter()
        .find(|(name, _)| name.replace('\\', "/").ends_with("tsconfig.json"));
    let Some((filename, base_content)) = tsconfig_source else {
        return Ok(());
    };

    let sanitized_source = filename
        .replace("..", "_")
        .trim_start_matches('/')
        .to_string();
    let project_tsconfig = project_dir
        .strip_prefix(dir_path)
        .ok()
        .map(|relative| relative.join("tsconfig.json"))
        .unwrap_or_else(|| PathBuf::from("tsconfig.json"))
        .to_string_lossy()
        .replace('\\', "/");
    let is_project_tsconfig =
        sanitized_source == "tsconfig.json" || sanitized_source == project_tsconfig;
    let directive_opts = convert_options_to_tsconfig(options, &[]);
    let no_types_and_symbols = no_types_and_symbols_enabled(options);
    let has_directive_opts = if let serde_json::Value::Object(ref opts) = directive_opts {
        !opts.is_empty() || no_types_and_symbols
    } else {
        no_types_and_symbols
    };

    // Keep the authored project tsconfig as-is when no directive overrides are needed.
    if is_project_tsconfig && !has_directive_opts {
        if !target_tsconfig.is_file() {
            std::fs::write(&target_tsconfig, base_content)?;
        }
        return Ok(());
    }

    if !is_project_tsconfig {
        // Non-project tsconfig directives should not be promoted to the active
        // project. The conformance suite uses these virtual paths for cases
        // that should behave like missing project config and emit TS5057.
        return Ok(());
    }

    // Merge directive options into a root tsconfig's compilerOptions
    if has_directive_opts {
        let mut tsconfig: serde_json::Value =
            serde_json::from_str(base_content).unwrap_or_else(|_| serde_json::json!({}));
        if let serde_json::Value::Object(ref mut root) = tsconfig {
            let compiler_options = root
                .entry("compilerOptions")
                .or_insert_with(|| serde_json::json!({}));
            if let serde_json::Value::Object(ref mut opts) = compiler_options {
                if let serde_json::Value::Object(ref directive_map) = directive_opts {
                    for (key, value) in directive_map {
                        opts.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        std::fs::write(&target_tsconfig, serde_json::to_string_pretty(&tsconfig)?)?;
        return Ok(());
    }

    if !target_tsconfig.is_file() {
        std::fs::write(&target_tsconfig, base_content)?;
    }
    Ok(())
}

#[cfg(test)]
fn parse_error_codes_from_text(text: &str) -> Vec<u32> {
    parse_diagnostic_output(text, Path::new("")).error_codes
}

/// Parse @symlink associations from raw test file content.
/// Returns a map of source filename -> list of symlink paths.
/// Format in test files: @filename: /path followed by @symlink: /link1,/link2
fn parse_symlink_associations(content: &str) -> Vec<(String, Vec<String>)> {
    use crate::test_directives::{parse_directive_line, split_list_values};

    let mut result = Vec::new();
    let mut current_filename: Option<String> = None;

    for line in content.lines() {
        // Canonical recognizer: must agree with the `@filename` splitting in
        // `test_parser::parse_test_file` (any key casing) so a symlink is
        // associated with the same file section the splitter produces.
        let Some(directive) = parse_directive_line(line) else {
            continue;
        };
        if directive.key_is("filename") {
            current_filename = Some(directive.value.to_string());
        } else if directive.key_is("symlink") {
            if let Some(ref filename) = current_filename {
                let links: Vec<String> = split_list_values(directive.value)
                    .map(str::to_string)
                    .collect();
                if !links.is_empty() {
                    result.push((filename.clone(), links));
                }
            }
        }
    }

    result
}

/// Parse standalone `@link: source -> destination` directives from raw test
/// content. TypeScript's harness treats these as symlinks rooted at the
/// destination path that point at the source path.
fn parse_link_associations(content: &str) -> Vec<(String, String)> {
    use crate::test_directives::parse_directive_line;

    let mut result = Vec::new();

    for line in content.lines() {
        let Some(directive) = parse_directive_line(line) else {
            continue;
        };
        if !directive.key_is("link") {
            continue;
        }
        let Some((target, link)) = directive.value.split_once("->") else {
            continue;
        };
        let target = target.trim();
        let link = link.trim();
        if target.is_empty() || link.is_empty() {
            continue;
        }
        result.push((target.to_string(), link.to_string()));
    }

    result
}

fn create_symlink_path(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    {
        if target.is_dir() {
            std::os::windows::fs::symlink_dir(target, link)
        } else {
            std::os::windows::fs::symlink_file(target, link)
        }
    }
}

/// Strip @ directive comments from test file content.
/// Removes lines like `// @strict: true` from the code entirely
/// (not just blanked) so that diagnostic line numbers match the
/// TSC cache, which was generated with line removal.
pub fn strip_directive_comments(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim().trim_start_matches('\u{feff}');
            // Keep lines that are not @ directives
            // Directives start with // @key: value (but not /// triple-slash refs)
            !(trimmed.starts_with("//")
                && !trimmed.starts_with("///")
                && trimmed.contains("@")
                && trimmed.contains(":"))
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
fn rewrite_bare_specifiers(
    content: &str,
    current_filename: &str,
    filenames: &[(String, String)],
) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;
    use std::collections::HashMap;
    let normalized_current = current_filename.replace('\\', "/");
    let current_dir = std::path::Path::new(&normalized_current)
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();

    // If no multi-file test, nothing to rewrite
    if filenames.is_empty() {
        return content.to_string();
    }

    // Build a map of available file basenames (without extension) to their directories.
    let mut available_files: HashMap<String, Vec<std::path::PathBuf>> = HashMap::new();
    let mut declared_modules = std::collections::HashSet::new();
    let mut package_names_by_dir: HashMap<std::path::PathBuf, String> = HashMap::new();
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
        let filename_path = std::path::Path::new(&normalized).to_path_buf();
        let parent = filename_path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        available_files
            .entry(basename.to_string())
            .or_default()
            .push(parent);
    }
    for (filename, content) in filenames {
        for cap in DECLARE_MODULE_RE.captures_iter(content) {
            declared_modules.insert(cap[1].to_string());
        }
        if filename.replace('\\', "/").ends_with("package.json") {
            if let Ok(package_json) = serde_json::from_str::<serde_json::Value>(content) {
                if let Some(name) = package_json.get("name").and_then(serde_json::Value::as_str) {
                    let package_dir = std::path::Path::new(&filename.replace('\\', "/"))
                        .parent()
                        .map(std::path::Path::to_path_buf)
                        .unwrap_or_default();
                    package_names_by_dir.insert(package_dir, name.to_string());
                }
            }
        }
    }

    let nearest_package_name = current_dir.ancestors().find_map(|ancestor| {
        package_names_by_dir
            .get(ancestor)
            .map(std::string::String::as_str)
    });

    // Match: from "module" or from 'module'
    // Captures: (from )(quote)(module)(quote)
    static FROM_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(from\s+)(['"])([^'"\./][^'"]*)['"]"#).unwrap());

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
        if nearest_package_name == Some(specifier) {
            return false;
        }
        let candidates = [
            specifier,
            specifier.trim_end_matches(".js"),
            specifier.trim_end_matches(".ts"),
            specifier.trim_end_matches(".tsx"),
            specifier.trim_end_matches(".d.ts"),
        ];
        for candidate in candidates {
            if let Some(candidate_dirs) = available_files.get(candidate) {
                if candidate_dirs
                    .iter()
                    .any(|directory| directory == &current_dir)
                {
                    return true;
                }
            }
        }
        false
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

        // Rewrite the reference path from absolute (/.lib/) to relative (.lib/)
        let old = caps.get(0).unwrap().as_str();
        let new = format!("{}{}.lib/{}{}", &caps[1], &caps[2], lib_file, &caps[2]);
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
            format!("{}{}./{}{}", &caps[1], &caps[2], path, &caps[2])
        })
        .into_owned()
}

/// Parse batch-mode output text into a `CompilationResult`.
///
/// Unlike `parse_tsz_output` which takes a `process::Output`, this takes the
/// raw text collected from a batch worker's stdout (everything before the
/// sentinel line). Exactly one completion marker is mandatory; missing,
/// duplicate, or unknown markers are capability nonclaims.
pub fn parse_batch_output(
    text: &str,
    project_root: &Path,
    options: HashMap<String, String>,
) -> CompilationResult {
    let (semantic_completion, text) = strip_semantic_completion_marker(text);
    if text.is_empty() {
        return CompilationResult {
            error_codes: vec![],
            diagnostic_fingerprints: vec![],
            crashed: false,
            semantic_completion,
            ordinary_exit_statuses: Vec::new(),
            options,
        };
    }

    let parsed = parse_diagnostic_output(&text, project_root);
    let crashed = !parsed.fully_covered;

    CompilationResult {
        error_codes: parsed.error_codes,
        diagnostic_fingerprints: parsed.diagnostic_fingerprints,
        crashed,
        semantic_completion: if crashed {
            SemanticCompletion::Incomplete
        } else {
            semantic_completion
        },
        ordinary_exit_statuses: Vec::new(),
        options,
    }
}

fn strip_semantic_completion_marker(text: &str) -> (SemanticCompletion, String) {
    let mut completion = None;
    let mut valid = true;
    let mut diagnostics = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let observed = match trimmed {
            "---TSZ-SEMANTIC-COMPLETION:complete---" => Some(SemanticCompletion::Complete),
            "---TSZ-SEMANTIC-COMPLETION:deferred---" => Some(SemanticCompletion::Deferred),
            "---TSZ-SEMANTIC-COMPLETION:cycle---" => Some(SemanticCompletion::Cycle),
            "---TSZ-SEMANTIC-COMPLETION:limit---" => Some(SemanticCompletion::Limit),
            _ if trimmed
                .strip_prefix(SEMANTIC_COMPLETION_MARKER_PREFIX)
                .is_some_and(|payload| payload.ends_with("---")) =>
            {
                valid = false;
                None
            }
            _ => None,
        };
        if let Some(observed) = observed {
            if completion.replace(observed).is_some() {
                valid = false;
            }
        } else if trimmed.starts_with(SEMANTIC_COMPLETION_MARKER_PREFIX) {
            valid = false;
        } else {
            diagnostics.push_str(line);
        }
    }
    (
        if valid {
            completion.unwrap_or(SemanticCompletion::Incomplete)
        } else {
            SemanticCompletion::Incomplete
        },
        diagnostics,
    )
}

#[cfg(test)]
#[path = "../tests/tsz_wrapper.rs"]
mod tests;
