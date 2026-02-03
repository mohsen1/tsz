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

/// Compile a TypeScript file and extract error codes
///
/// # Arguments
/// * `content` - Main file content
/// * `filenames` - Additional files from @filename directives [(filename, content)]
/// * `options` - Compiler options (strict, target, module, etc.)
/// * `tsz_binary_path` - Path to tsz binary
///
/// # Returns
/// * `Ok(CompilationResult)` - Compilation succeeded (may have errors)
/// * `Err(...)` - Fatal error (not a type error)
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

    // Write main file
    let main_file = dir_path.join("test.ts");
    std::fs::write(&main_file, content)?;

    // Write additional files from @filename directives
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

    // Create tsconfig.json with test options
    let tsconfig_path = dir_path.join("tsconfig.json");
    let tsconfig_content = serde_json::json!({
        "compilerOptions": convert_options_to_tsconfig(options),
        "include": ["*.ts", "*.tsx", "**/*.ts", "**/*.tsx"],
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

/// Convert test directive options to tsconfig compiler options
fn convert_options_to_tsconfig(
    options: &HashMap<String, String>,
) -> serde_json::Value {
    let mut opts = serde_json::Map::new();

    for (key, value) in options {
        let json_value = match value.as_str() {
            "true" => serde_json::Value::Bool(true),
            "false" => serde_json::Value::Bool(false),
            _ => serde_json::Value::String(value.clone()),
        };
        opts.insert(key.clone(), json_value);
    }

    serde_json::Value::Object(opts)
}

/// Compile with tsz binary (internal function)
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
fn parse_diagnostics_from_text(
    text: &str,
) -> Vec<Diagnostic> {
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
fn extract_error_codes(
    diagnostics: &[Diagnostic],
) -> Vec<u32> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_simple_error() {
        let content = r#"
// @strict: true
const x: number = "string";
"#;
        let result = compile_test(content, &[], &HashMap::new(), "../target/release/tsz")
            .unwrap();
        // Should have type error (TS2322)
        assert!(!result.error_codes.is_empty());
    }

    #[test]
    fn test_compile_no_errors() {
        let content = r#"
// @strict: true
const x: number = 42;
"#;
        let result = compile_test(content, &[], &HashMap::new(), "../target/release/tsz")
            .unwrap();
        // Should have no errors
        assert!(result.error_codes.is_empty());
    }
}
