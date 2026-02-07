//! tsz compiler wrapper for conformance testing
//!
//! Provides a simple API to compile TypeScript code and extract error codes.

use std::collections::HashMap;
use wasm::diagnostics::{Diagnostic, DiagnosticSeverity};
use wasm::span::Span;

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

    if filenames.is_empty() {
        // Single-file test: write content to test.ts (strip directive comments)
        let stripped_content = strip_directive_comments(content);
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
            std::fs::write(&file_path, file_content)?;
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
    let explicit_allow_js = options
        .get("allowJs")
        .or_else(|| options.get("allowjs"));
    let allow_js = match explicit_allow_js {
        Some(v) => v == "true",
        None => has_js_files,
    };
    // Include .cts/.mts (TypeScript CJS/ESM) alongside .ts/.tsx
    let include = if allow_js {
        serde_json::json!([
            "*.ts", "*.tsx", "*.cts", "*.mts", "*.js", "*.jsx", "*.mjs", "*.cjs",
            "**/*.ts", "**/*.tsx", "**/*.cts", "**/*.mts", "**/*.js", "**/*.jsx", "**/*.mjs", "**/*.cjs"
        ])
    } else {
        serde_json::json!(["*.ts", "*.tsx", "*.cts", "*.mts", "**/*.ts", "**/*.tsx", "**/*.cts", "**/*.mts"])
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

    if filenames.is_empty() {
        let stripped_content = strip_directive_comments(content);
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
            std::fs::write(&file_path, file_content)?;
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
    let explicit_allow_js = options
        .get("allowJs")
        .or_else(|| options.get("allowjs"));
    let allow_js = match explicit_allow_js {
        Some(v) => v == "true",
        None => has_js_files,
    };
    // Include .cts/.mts (TypeScript CJS/ESM) alongside .ts/.tsx
    let include = if allow_js {
        serde_json::json!([
            "*.ts", "*.tsx", "*.cts", "*.mts", "*.js", "*.jsx", "*.mjs", "*.cjs",
            "**/*.ts", "**/*.tsx", "**/*.cts", "**/*.mts", "**/*.js", "**/*.jsx", "**/*.mjs", "**/*.cjs"
        ])
    } else {
        serde_json::json!(["*.ts", "*.tsx", "*.cts", "*.mts", "**/*.ts", "**/*.tsx", "**/*.cts", "**/*.mts"])
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

        opts.insert(key.clone(), json_value);
    }

    serde_json::Value::Object(opts)
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
