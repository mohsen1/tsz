#[derive(Clone, Copy)]
enum DiagnosticLineMode {
    CodeList,
    Fingerprint,
}

fn retained_diagnostic_code_from_line(line: &str, mode: DiagnosticLineMode) -> Option<u32> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static DIAG_CODE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"^(?:.+\(\d+,\d+\):\s+error\s+TS(?P<code>\d+):.*|:\s*error\s+TS(?P<code2>\d+):.*|error\s+TS(?P<code3>\d+):.*)$",
        )
        .expect("valid regex")
    });

    let caps = DIAG_CODE_RE.captures(line)?;
    if caps.name("code3").is_some() && matches!(mode, DiagnosticLineMode::CodeList) {
        return None;
    }
    let code = caps
        .name("code")
        .or_else(|| caps.name("code2"))
        .or_else(|| caps.name("code3"))
        .and_then(|m| m.as_str().parse::<u32>().ok())?;
    Some(code)
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
    let error_codes = parse_error_codes_from_text(&text);
    let diagnostic_fingerprints = parse_diagnostic_fingerprints_from_text(&text, project_root);

    CompilationResult {
        error_codes,
        diagnostic_fingerprints,
        crashed: false,
        options,
    }
}
