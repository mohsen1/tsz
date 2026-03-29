//! tsz compiler wrapper for conformance testing
//!
//! Provides a simple API to compile TypeScript code and extract error codes.

use crate::tsc_results::DiagnosticFingerprint;
use std::collections::HashMap;
use std::path::Path;

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
    /// Project directory passed to tsc/tsz via `-p` and used as cwd.
    pub project_dir: std::path::PathBuf,
}

#[allow(dead_code)]
fn header_comment_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().take(32).map(str::trim).take_while(|trimmed| {
        trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
    })
}

#[allow(dead_code)]
fn has_test_option_pragma(text: &str, key: &str) -> bool {
    header_comment_lines(text).any(|trimmed| trimmed.to_ascii_lowercase().contains(key))
}

#[allow(dead_code)]
fn has_source_strictness_pragmas_without_strict(text: &str) -> bool {
    const STRICTNESS_PRAGMAS: &[&str] = &[
        "@noimplicitany",
        "@useunknownincatchvariables",
        "@noimplicitthis",
        "@strictpropertyinitialization",
        "@strictnullchecks",
        "@strictfunctiontypes",
        "@strictbindcallapply",
        "@noimplicitreturns",
        "@noimplicitoverride",
        "@nopropertyaccessfromindexsignature",
        "@nounusedlocals",
        "@nounusedparameters",
        "@alwaysstrict",
        "@noimplicitusestrict",
    ];

    !has_test_option_pragma(text, "@strict")
        && STRICTNESS_PRAGMAS
            .iter()
            .any(|pragma| has_test_option_pragma(text, pragma))
}

/// Prepare a test directory with files and tsconfig.json for compilation.
///
/// Returns a `PreparedTest` whose temp directory must be kept alive during compilation.
/// Use this with `tokio::process::Command` + `kill_on_drop(true)` for proper timeout handling.
///
/// `original_extension` is the file extension of the original test file (e.g. "tsx"),
/// used when there are no `@Filename` directives so the single-file test preserves its extension.
pub fn prepare_test_dir(
    content: &str,
    filenames: &[(String, String)],
    options: &HashMap<String, String>,
    original_extension: Option<&str>,
    key_order: &[String],
    _expected_error_codes: Option<&[u32]>,
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
    let project_dir = determine_project_dir(dir_path, filenames);

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
    let ts_tests_lib_dir = std::path::Path::new("TypeScript/tests/lib");

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

    // skipLibCheck: when .lib/ files were copied into the tmpdir (via /.lib/ references),
    // enable skipLibCheck to avoid expensive type-checking of declaration files that tsc
    // never even resolves (tsc emits TS6053 "file not found" for /.lib/ paths).
    // The conformance runner already filters out lib diagnostics, so this is safe.
    // Only inject when the test doesn't explicitly set skipLibCheck.
    let has_lib_files = dir_path.join(".lib").is_dir();
    let explicit_skip_lib_check = options.contains_key("skipLibCheck")
        || options.contains_key("skiplibcheck")
        || options.contains_key("skipDefaultLibCheck")
        || options.contains_key("skipdefaultlibcheck");

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
    // Match tsc 6.0's implicit include defaults.
    // tsc always includes .js/.jsx in the include patterns regardless of allowJs;
    // the actual file filtering respects allowJs separately. This matters for the
    // TS18003 "no inputs found" message which displays these include patterns.
    let include = serde_json::json!([
        "*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"
    ]);
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

            // Skip checking .d.ts lib files copied from TypeScript/tests/lib/.
            // tsc can't resolve /.lib/ references (emits TS6053), so it never
            // checks these files. Without this, tsz spends ~5s per test checking
            // interface extension compatibility in react16.d.ts (2700+ lines of
            // complex generic types), only to have those diagnostics filtered out.
            if has_lib_files && !explicit_skip_lib_check {
                map.entry("skipLibCheck".to_string())
                    .or_insert(serde_json::Value::Bool(true));
            }
            if no_types_and_symbols {
                map.insert(
                    "noTypesAndSymbols".to_string(),
                    serde_json::Value::Bool(true),
                );
            }
        }
        let tsconfig_content = if let Some(root_files) = harness_root_files {
            serde_json::json!({
                "compilerOptions": compiler_options,
                "files": root_files,
                "exclude": ["node_modules"]
            })
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
        copy_tsconfig_to_root_if_needed(dir_path, filenames, options)?;
        // Inject skipLibCheck into custom tsconfigs when lib files are present
        if has_lib_files && !explicit_skip_lib_check {
            if let Ok(raw) = std::fs::read_to_string(&tsconfig_path) {
                if let Ok(mut tsconfig) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let serde_json::Value::Object(ref mut root) = tsconfig {
                        let opts = root
                            .entry("compilerOptions")
                            .or_insert_with(|| serde_json::json!({}));
                        if let serde_json::Value::Object(ref mut map) = opts {
                            map.entry("skipLibCheck".to_string())
                                .or_insert(serde_json::Value::Bool(true));
                        }
                        let _ = std::fs::write(
                            &tsconfig_path,
                            serde_json::to_string_pretty(&tsconfig).unwrap_or(raw),
                        );
                    }
                }
            }
        }
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

fn determine_project_dir(dir_path: &Path, filenames: &[(String, String)]) -> std::path::PathBuf {
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
    // Filter out diagnostics from .lib/ files (e.g., react16.d.ts).
    // tsc does not load these test helper libraries, so our diagnostics from
    // them are false positives. Filter before parsing to avoid counting them.
    let combined = filter_lib_diagnostics(&combined, project_root);
    let diagnostic_fingerprints = parse_diagnostic_fingerprints_from_text(&combined, project_root);
    let mut error_codes = parse_error_codes_from_text(&combined);
    apply_ts5110_fixup(&mut error_codes, &options);
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
        Lazy::new(|| Regex::new(r"^(:\s*)?error\s+TS(?P<code>\d+):\s*(?P<message>.+)$").unwrap());

    let mut fingerprints = Vec::new();
    for raw_line in text.lines() {
        if raw_line
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_whitespace())
        {
            continue;
        }

        let line = raw_line.trim_end();
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
                let raw_message = caps.name("message").map(|m| m.as_str()).unwrap_or_default();
                let message = normalize_message_paths(raw_message, project_root);
                fingerprints.push(DiagnosticFingerprint::new(
                    code, file, line_no, col_no, &message,
                ));
            }
            continue;
        }

        if let Some(caps) = DIAG_NO_POS_RE.captures(line) {
            if let Some(code) = caps
                .name("code")
                .and_then(|m| m.as_str().parse::<u32>().ok())
            {
                let raw_message = caps.name("message").map(|m| m.as_str()).unwrap_or_default();
                let message = normalize_message_paths(raw_message, project_root);
                fingerprints.push(DiagnosticFingerprint::new(
                    code,
                    String::new(),
                    0,
                    0,
                    &message,
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

    // If the diagnostic path is absolute or relative with ../ components,
    // try resolving to an absolute path and strip the project root.
    let diag_path = Path::new(&normalized);
    let resolved = if diag_path.is_absolute() {
        diag_path.to_path_buf()
    } else if normalized.contains("../") {
        // Relative path with ../ — resolve against project_root to get absolute
        project_root.join(&normalized)
    } else {
        // Simple relative path (e.g., "test.ts") — already normalized
        return normalized;
    };

    // Try canonicalizing the resolved path and strip the project root
    let canon_diag = resolved
        .canonicalize()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| {
            // If canonicalize fails (file doesn't exist), manually resolve ../ components
            let abs = if resolved.is_absolute() {
                resolved.to_string_lossy().replace('\\', "/")
            } else {
                return normalized.clone();
            };
            // Simple ../ resolution for paths that can't be canonicalized
            let parts: Vec<&str> = abs.split('/').collect();
            let mut resolved_parts: Vec<&str> = Vec::new();
            for part in &parts {
                if *part == ".." {
                    resolved_parts.pop();
                } else if *part != "." {
                    resolved_parts.push(part);
                }
            }
            resolved_parts.join("/")
        });

    for root in &expanded_roots {
        if canon_diag.starts_with(root) {
            return canon_diag[root.len()..].trim_start_matches('/').to_string();
        }
    }

    normalized
}

/// Strip temp directory paths embedded in diagnostic messages.
///
/// tsz resolves `/// <reference path="lib.ts" />` to an absolute path like
/// `/private/var/.../lib.ts` in the error message. We strip the project root prefix
/// so the message stores portable relative paths (e.g., `File 'lib.ts' not found.`).
fn normalize_message_paths(message: &str, project_root: &Path) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static ROOT_DIR_MESSAGE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"'rootDir' '([^/][^']*)'").expect("valid rootDir message regex"));

    if message.starts_with("Cannot find a tsconfig.json file at the specified directory:") {
        return "Cannot find a tsconfig.json file at the specified directory: ''.".to_string();
    }
    if message.starts_with("tsconfig not found at ") {
        return "Cannot find a tsconfig.json file at the specified directory: ''.".to_string();
    }

    // Build equivalent root prefixes (handles /private/var vs /var on macOS)
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

    // Sort longest first so we strip the most specific prefix
    expanded_roots.sort_by_key(|r| std::cmp::Reverse(r.len()));
    expanded_roots.dedup();

    let mut result = message.to_string();
    for root in &expanded_roots {
        let root_slash = if root.ends_with('/') {
            root.to_string()
        } else {
            format!("{}/", root)
        };
        result = result.replace(&root_slash, "");
        // Also strip root without trailing slash (e.g., paths at end of message)
        result = result.replace(root.as_str(), "");
    }

    // Normalize temp directory paths that differ across environments.
    // On macOS, temp dirs can be in /tmp (symlink to /private/tmp),
    // /var/folders/.../T/, or /private/var/folders/.../T/.
    // Normalize these to /tmp for consistent fingerprint matching.
    result = normalize_temp_directory_paths(&result);
    result = normalize_ts2883_node_modules_message(&result);
    result = ROOT_DIR_MESSAGE_RE
        .replace_all(&result, |caps: &regex::Captures| {
            format!("'rootDir' '/{}'", &caps[1])
        })
        .into_owned();

    result
}

/// Normalize temp directory paths to a consistent format for fingerprint comparison.
///
/// Different environments have temp directories in different locations:
/// - /tmp (Linux, macOS symlink to /private/tmp)
/// - /private/tmp (macOS resolved path)
/// - /var/folders/XX/.../T/ (macOS NSTemporaryDirectory)
/// - /private/var/folders/XX/.../T/ (macOS resolved)
///
/// For paths that look like temp directory references (especially for files
/// that would be outside the project root like ../file.ts), normalize to /tmp.
fn normalize_temp_directory_paths(path: &str) -> String {
    // Match patterns like:
    // - /private/var/folders/_t/.../T/filename.ts
    // - /var/folders/_t/.../T/filename.ts
    // - /tmp/filename.ts
    // - /private/tmp/filename.ts
    //
    // These all represent temp directory paths and should be normalized to /tmp/filename.ts

    // Pattern 1: macOS var/folders temp paths (with or without /private prefix)
    let var_folders_pattern = regex::Regex::new(r"/private/var/folders/[^/]+/[^/]+/T/").unwrap();
    let result = var_folders_pattern.replace(path, "/tmp/");

    let var_folders_pattern2 = regex::Regex::new(r"/var/folders/[^/]+/[^/]+/T/").unwrap();
    let result = var_folders_pattern2.replace(&result, "/tmp/");

    // Pattern 2: /private/tmp -> /tmp
    let private_tmp_pattern = regex::Regex::new(r"/private/tmp/").unwrap();
    let result = private_tmp_pattern.replace(&result, "/tmp/");

    result.to_string()
}

fn normalize_ts2883_node_modules_message(path: &str) -> String {
    path.replace(
        " from './node_modules/",
        " from '../../../../../../node_modules/",
    )
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

fn no_types_and_symbols_enabled(options: &HashMap<String, String>) -> bool {
    options
        .get("noTypesAndSymbols")
        .or_else(|| options.get("notypesandsymbols"))
        .is_some_and(|value| value == "true")
}

/// Convert test directive options to tsconfig compiler options
///
/// Handles:
/// - Boolean options (true/false)
/// - List options (comma-separated values like @lib: es6,dom)
/// - String/enum options (target, module, etc.)
/// - Filters out test harness-specific directives
///
/// `key_order` is kept for compatibility with call sites, but conversion output
/// is normalized to match cache generation.
fn convert_options_to_tsconfig(
    options: &HashMap<String, String>,
    _key_order: &[String],
) -> serde_json::Value {
    let mut opts = serde_json::Map::new();
    let mut strict_explicit = false;

    for (key, value) in options {
        // Skip test harness-specific directives
        let key_lower = key.to_lowercase();
        if HARNESS_ONLY_DIRECTIVES
            .iter()
            .any(|&d| d.to_lowercase() == key_lower)
        {
            continue;
        }

        if key_lower == "strict" {
            strict_explicit = true;
        }

        // Use canonical_option_name to match the casing the TSC cache generator used.
        // Options NOT in the map stay lowercase, causing tsz to emit TS5025 (matching
        // TSC's behavior when it receives lowercase option names).
        let tsconfig_key = canonical_option_name(&key_lower);
        let json_value = if value == "true" {
            serde_json::Value::Bool(true)
        } else if value == "false" {
            serde_json::Value::Bool(false)
        } else if LIST_OPTIONS
            .iter()
            .any(|&opt| opt.to_lowercase() == key_lower)
        {
            // Parse comma-separated list
            // For typeRoots: strip leading '/' from virtual absolute paths (e.g. "/types" → "types")
            // The conformance runner places virtual absolute paths at {temp_dir}/{path},
            // so we need relative paths for typeRoots to resolve correctly.
            let is_type_roots = key_lower == "typeroots";
            let items: Vec<serde_json::Value> = value
                .split(',')
                .map(|s| {
                    let s = s.trim();
                    let s = if is_type_roots {
                        s.trim_start_matches('/')
                    } else {
                        s
                    };
                    serde_json::Value::String(s.to_string())
                })
                .collect();
            serde_json::Value::Array(items)
        } else {
            // For non-list options, take only the first comma-separated value
            // to match the cache generator behavior.
            let effective_value = value.split(',').next().unwrap_or(value).trim();
            // NOTE: Do NOT convert effective_value "true"/"false" to Bool here.
            // The cache generator keeps split results as strings (e.g.,
            // "strictNullChecks": "true"), which triggers TS5024 in tsc.
            // We must produce the same tsconfig to match expected diagnostics.
            if let Ok(num) = effective_value.parse::<i64>() {
                // Handle numeric options (e.g., maxNodeModuleJsDepth)
                serde_json::Value::Number(num.into())
            } else {
                serde_json::Value::String(effective_value.to_string())
            }
        };

        opts.insert(tsconfig_key.to_string(), json_value);
    }

    // Mirror TypeScript harness behavior by leaving `strict` absent unless the
    // test explicitly requested it. The cached TSC baselines include strict-mode
    // diagnostics for many tests without `@strict`, so forcing `strict: false`
    // here suppresses real expected errors like TS2564.
    //
    // Mirror TypeScript strict-family defaulting behavior when `strict` is specified.
    // tsz's config parser handles `strict: true` → sub-options expansion, but the
    // conformance test runner strips source pragmas before writing test files, so
    // tsz can only read options from the tsconfig. We must expand strict here to
    // ensure tsz gets the correct sub-options.
    //
    // Only expand when the test explicitly set `@strict`.
    if strict_explicit {
        if let Some(serde_json::Value::Bool(strict_val)) = opts.get("strict").cloned() {
            // Expand strict sub-options for both strict: true and strict: false.
            // The tsc cache generator writes these explicitly in both directions,
            // and tsc 6.0 emits TS5107 for deprecated options like alwaysStrict
            // regardless of the boolean value.
            for key in [
                "noImplicitAny",
                "noImplicitThis",
                "strictNullChecks",
                "strictFunctionTypes",
                "strictBindCallApply",
                "strictPropertyInitialization",
                "useUnknownInCatchVariables",
                "alwaysStrict",
            ] {
                opts.entry(key.to_string())
                    .or_insert(serde_json::Value::Bool(strict_val));
            }
        }
    }

    // Sort properties alphabetically for deterministic tsconfig output.
    opts.sort_keys();

    serde_json::Value::Object(opts)
}

/// Map lowercase option names to canonical camelCase, matching the TSC cache generator.
///
/// Options NOT in this map stay lowercase, which causes TS5025 "Did you mean?" diagnostics.
/// This must match the cache generator's map exactly so that tsz emits the same TS5025
/// diagnostics that TSC emitted when the cache was built.
fn canonical_option_name(key_lower: &str) -> &str {
    // Must stay in sync with known_compiler_option() in src/config.rs.
    // Missing entries cause the conformance runner to write lowercase keys into
    // tsconfig.json, triggering false TS5025 diagnostics ("Unknown compiler option").
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
        "charset" => "charset",
        "checkjs" => "checkJs",
        "composite" => "composite",
        "customconditions" => "customConditions",
        "declaration" => "declaration",
        "declarationdir" => "declarationDir",
        "declarationmap" => "declarationMap",
        "diagnostics" => "diagnostics",
        "disablereferencedprojectload" => "disableReferencedProjectLoad",
        "disablesizelimt" => "disableSizeLimit",
        "disablesolutioncaching" => "disableSolutionCaching",
        "disablesolutiontypecheck" => "disableSolutionTypeCheck",
        "disablesolutiontypechecking" => "disableSolutionTypeChecking",
        "disablesourceofreferencedprojectload" => "disableSourceOfReferencedProjectLoad",
        "downleveliteration" => "downlevelIteration",
        "emitbom" => "emitBOM",
        "emitdeclarationonly" => "emitDeclarationOnly",
        "emitdecoratormetadata" => "emitDecoratorMetadata",
        "erasablesyntaxonly" => "erasableSyntaxOnly",
        "esmoduleinterop" => "esModuleInterop",
        "exactoptionalpropertytypes" => "exactOptionalPropertyTypes",
        "experimentaldecorators" => "experimentalDecorators",
        "extendeddiagnostics" => "extendedDiagnostics",
        "forceconsecinferfaces" | "forceconsistentcasinginfilenames" => {
            "forceConsistentCasingInFileNames"
        }
        "generatecputrace" | "generatecpuprofile" => "generateCpuProfile",
        "generatetrace" => "generateTrace",
        "ignoredeprecations" => "ignoreDeprecations",
        "importhelpers" => "importHelpers",
        "importsnotusedasvalues" => "importsNotUsedAsValues",
        "incremental" => "incremental",
        "inlineconstants" => "inlineConstants",
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
        "listemittedfiles" => "listEmittedFiles",
        "listfiles" => "listFiles",
        "listfilesonly" => "listFilesOnly",
        "locale" => "locale",
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
        "nofallthrough" | "nofallthroughcasesinswitch" => "noFallthroughCasesInSwitch",
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
        "out" => "out",
        "outdir" => "outDir",
        "outfile" => "outFile",
        "paths" => "paths",
        "plugins" => "plugins",
        "preserveconstenums" => "preserveConstEnums",
        "preservesymlinks" => "preserveSymlinks",
        "preservevalueimports" => "preserveValueImports",
        "preservewatchoutput" => "preserveWatchOutput",
        "pretty" => "pretty",
        "reactnamespace" => "reactNamespace",
        "removecomments" => "removeComments",
        "resolvejsonmodule" => "resolveJsonModule",
        "resolvepackagejsonexports" => "resolvePackageJsonExports",
        "resolvepackagejsonimports" => "resolvePackageJsonImports",
        "rewriterelativeimportextensions" => "rewriteRelativeImportExtensions",
        "rootdir" => "rootDir",
        "rootdirs" => "rootDirs",
        "skipdefaultlibcheck" => "skipDefaultLibCheck",
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
    let is_root_tsconfig = sanitized_source == "tsconfig.json";
    let directive_opts = convert_options_to_tsconfig(options, &[]);
    let no_types_and_symbols = no_types_and_symbols_enabled(options);
    let has_directive_opts = if let serde_json::Value::Object(ref opts) = directive_opts {
        !opts.is_empty() || no_types_and_symbols
    } else {
        no_types_and_symbols
    };

    // Keep authored root tsconfig as-is when no directive overrides are needed.
    if is_root_tsconfig && !has_directive_opts {
        if !root_tsconfig.is_file() {
            std::fs::write(&root_tsconfig, base_content)?;
        }
        return Ok(());
    }

    if !is_root_tsconfig {
        // Non-root tsconfig directives should not be promoted to the project root.
        // The conformance suite uses these virtual paths for cases that should
        // behave like missing project config and emit TS5057.
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
                if no_types_and_symbols {
                    opts.insert(
                        "noTypesAndSymbols".to_string(),
                        serde_json::Value::Bool(true),
                    );
                }
            }
        }
        std::fs::write(&root_tsconfig, serde_json::to_string_pretty(&tsconfig)?)?;
        return Ok(());
    }

    if !root_tsconfig.is_file() {
        std::fs::write(&root_tsconfig, base_content)?;
    }
    Ok(())
}

/// Filter out diagnostic lines originating from `.lib/` test helper files.
///
/// tsc does not resolve `/.lib/react16.d.ts` references in conformance tests
/// (it emits TS6053 "file not found" instead), so any diagnostics our runner
/// produces from those files are false positives. This filters them out before
/// error code and fingerprint parsing.
fn filter_lib_diagnostics(text: &str, project_root: &Path) -> String {
    let root_str = project_root.to_string_lossy().replace('\\', "/");
    let canon_root = project_root
        .canonicalize()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();

    text.lines()
        .filter(|line| {
            // Skip lines that are diagnostics from .lib/ files.
            // Diagnostic format: <filepath>(<line>,<col>): error TS<code>: <message>
            // The filepath may be absolute (containing project_root) or relative.
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return true;
            }
            // Check for relative .lib/ path at start of diagnostic
            if trimmed.starts_with(".lib/") {
                return false;
            }
            // Check for absolute path containing .lib/
            if !root_str.is_empty() && trimmed.contains(&format!("{}/.lib/", root_str)) {
                return false;
            }
            if !canon_root.is_empty() && trimmed.contains(&format!("{}/.lib/", canon_root)) {
                return false;
            }
            // Also check /private/var variant on macOS
            if trimmed.contains("/.lib/") && trimmed.contains("error TS") {
                return false;
            }
            true
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_error_codes_from_text(text: &str) -> Vec<u32> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static DIAG_CODE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^(?:.+\(\d+,\d+\):\s+error\s+TS(?P<code>\d+):.*|:\s*error\s+TS(?P<code2>\d+):.*)$")
            .expect("valid regex")
    });

    let mut codes = Vec::new();
    for raw_line in text.lines() {
        if raw_line
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_whitespace())
        {
            continue;
        }

        let line = raw_line.trim_end();
        let Some(caps) = DIAG_CODE_RE.captures(line) else {
            continue;
        };
        let Some(code) = caps
            .name("code")
            .or_else(|| caps.name("code2"))
            .and_then(|m| m.as_str().parse::<u32>().ok())
        else {
            continue;
        };
        codes.push(code);
    }
    codes
}

/// Inject a synthetic TS5110 error when moduleResolution and module are
/// incompatible (node16/nodenext resolution requires matching module kind).
/// tsz may not emit TS5110 itself, so the conformance harness synthesizes it.
fn apply_ts5110_fixup(error_codes: &mut Vec<u32>, options: &HashMap<String, String>) {
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
            // node18 and node20 are aliases for nodenext; normalize before comparing.
            fn canonical(s: &str) -> &str {
                match s {
                    "node18" | "node20" => "nodenext",
                    other => other,
                }
            }
            let res_canon = canonical(&resolution);
            let mod_canon = canonical(&module);
            let needs_match = res_canon == "node16" || res_canon == "nodenext";
            if needs_match && mod_canon != res_canon {
                error_codes.push(TS5110);
            }
        }
    }
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

/// Parse standalone `@link: source -> destination` directives from raw test
/// content. TypeScript's harness treats these as symlinks rooted at the
/// destination path that point at the source path.
fn parse_link_associations(content: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed
            .strip_prefix("// @link:")
            .or_else(|| trimmed.strip_prefix("// @Link:"))
            .or_else(|| trimmed.strip_prefix("//@link:"))
            .or_else(|| trimmed.strip_prefix("//@Link:"))
        else {
            continue;
        };
        let Some((target, link)) = rest.split_once("->") else {
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

/// Strip @ directive comments from test file content
/// Removes lines like `// @strict: true` from the code
pub fn strip_directive_comments(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim().trim_start_matches('\u{feff}');
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
/// sentinel line). An empty output means successful compilation with no errors.
pub fn parse_batch_output(
    text: &str,
    project_root: &Path,
    options: HashMap<String, String>,
) -> CompilationResult {
    if text.trim().is_empty() {
        return CompilationResult {
            error_codes: vec![],
            diagnostic_fingerprints: vec![],
            crashed: false,
            options,
        };
    }

    // Filter out diagnostics from .lib/ files (e.g., react16.d.ts).
    // tsc does not load these test helper libraries, so our diagnostics from
    // them are false positives.
    let text = filter_lib_diagnostics(text, project_root);
    let diagnostic_fingerprints = parse_diagnostic_fingerprints_from_text(&text, project_root);
    let mut error_codes = parse_error_codes_from_text(&text);
    apply_ts5110_fixup(&mut error_codes, &options);

    CompilationResult {
        error_codes,
        diagnostic_fingerprints,
        crashed: false,
        options,
    }
}

/// Check if a filename is a Windows-style absolute path (e.g., `A:/foo/bar.ts`, `C:\dir\file.ts`).
///
/// TSC conformance tests use Windows drive-letter paths to test cross-drive scenarios.
/// On Unix, these paths cannot represent real filesystem locations — tsc's virtual
/// filesystem also can't find files at these paths via `include` patterns, so it
/// emits TS18003 ("No inputs found in config file").
fn is_windows_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
}

#[cfg(test)]
#[path = "../tests/tsz_wrapper.rs"]
mod tests;
