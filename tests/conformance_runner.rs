//! Conformance Test Runner - Rust-based runner for TypeScript conformance tests
//!
//! This module provides infrastructure for running TypeScript conformance tests
//! from the Rust side, enabling faster iteration and better integration with
//! the Rust testing framework.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wasm::checker::context::CheckerOptions;
use wasm::thin_parser::ThinParserState;

/// Default timeout for conformance tests (30 seconds)
const CONFORMANCE_TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// A parsed test directive from a conformance test file
#[derive(Debug, Clone, Default)]
pub struct TestDirectives {
    /// Enable strict mode
    pub strict: Option<bool>,
    /// noImplicitAny option
    pub no_implicit_any: Option<bool>,
    /// strictNullChecks option
    pub strict_null_checks: Option<bool>,
    /// strictFunctionTypes option
    pub strict_function_types: Option<bool>,
    /// Target ES version
    pub target: Option<String>,
    /// Module system
    pub module: Option<String>,
    /// Whether this is a multi-file test
    pub is_multi_file: bool,
    /// Files in multi-file tests
    pub files: Vec<TestFile>,
    /// Expected errors (error codes we should produce)
    pub expected_errors: Vec<u32>,
    /// Raw directive map for custom directives
    pub raw: HashMap<String, String>,
}

/// A file within a multi-file test
#[derive(Debug, Clone)]
pub struct TestFile {
    pub name: String,
    pub content: String,
}

/// Result of running a conformance test
#[derive(Debug)]
pub struct ConformanceResult {
    /// Test name/path
    pub name: String,
    /// Whether the test passed
    pub passed: bool,
    /// Parse errors produced
    pub parse_errors: Vec<String>,
    /// Type errors produced
    pub type_errors: Vec<String>,
    /// Expected errors that were missing
    pub missing_errors: Vec<u32>,
    /// Unexpected errors that were produced
    pub extra_errors: Vec<u32>,
    /// Time taken to run the test
    pub duration: Duration,
    /// Whether the test timed out
    pub timed_out: bool,
}

/// Parse test directives from a conformance test file
pub fn parse_directives(source: &str) -> TestDirectives {
    let mut directives = TestDirectives::default();
    let lines: Vec<&str> = source.lines().collect();
    let mut current_file: Option<TestFile> = None;
    let mut file_content = Vec::new();

    for line in &lines {
        let trimmed = line.trim();

        // Check for @filename directive (multi-file test)
        if let Some(rest) = trimmed.strip_prefix("// @filename:") {
            // Save previous file if any
            if let Some(mut file) = current_file.take() {
                file.content = file_content.join("\n");
                directives.files.push(file);
                file_content.clear();
            }

            directives.is_multi_file = true;
            current_file = Some(TestFile {
                name: rest.trim().to_string(),
                content: String::new(),
            });
            continue;
        }

        // Check for compiler option directives
        if let Some(rest) = trimmed.strip_prefix("// @") {
            if let Some((key, value)) = rest.split_once(':') {
                let key = key.trim().to_lowercase();
                let value = value.trim();

                match key.as_str() {
                    "strict" => directives.strict = Some(value == "true"),
                    "noimplicitany" => directives.no_implicit_any = Some(value == "true"),
                    "strictnullchecks" => directives.strict_null_checks = Some(value == "true"),
                    "strictfunctiontypes" => directives.strict_function_types = Some(value == "true"),
                    "target" => directives.target = Some(value.to_string()),
                    "module" => directives.module = Some(value.to_string()),
                    _ => {
                        directives.raw.insert(key, value.to_string());
                    }
                }
            } else {
                // Boolean directive like // @strict
                let key = rest.trim().to_lowercase();
                match key.as_str() {
                    "strict" => directives.strict = Some(true),
                    "noimplicitany" => directives.no_implicit_any = Some(true),
                    "strictnullchecks" => directives.strict_null_checks = Some(true),
                    "strictfunctiontypes" => directives.strict_function_types = Some(true),
                    _ => {
                        directives.raw.insert(key, "true".to_string());
                    }
                }
            }
            continue;
        }

        // Collect file content for multi-file tests
        if current_file.is_some() {
            file_content.push(*line);
        }
    }

    // Save last file if any
    if let Some(mut file) = current_file.take() {
        file.content = file_content.join("\n");
        directives.files.push(file);
    }

    directives
}

/// Convert test directives to checker options
pub fn directives_to_options(directives: &TestDirectives) -> CheckerOptions {
    let strict = directives.strict.unwrap_or(false);
    CheckerOptions {
        strict,
        no_implicit_any: directives.no_implicit_any.unwrap_or(strict),
        no_implicit_returns: false,
        strict_null_checks: directives.strict_null_checks.unwrap_or(strict),
        strict_function_types: directives.strict_function_types.unwrap_or(strict),
        strict_property_initialization: strict,
        no_implicit_this: strict,
        use_unknown_in_catch_variables: strict,
        isolated_modules: false,
        no_unchecked_indexed_access: false,
        strict_bind_call_apply: false,
        exact_optional_property_types: false,
    }
}

/// Run a single conformance test (parse only - type checking requires more infrastructure)
pub fn run_conformance_test(source: &str, file_name: &str) -> ConformanceResult {
    let start = Instant::now();
    let directives = parse_directives(source);

    // For multi-file tests, we currently just test the first file
    let test_source = if directives.is_multi_file && !directives.files.is_empty() {
        &directives.files[0].content
    } else {
        source
    };

    // Parse
    let mut parser = ThinParserState::new(file_name.to_string(), test_source.to_string());
    parser.parse_source_file();

    let parse_errors: Vec<String> = parser
        .get_diagnostics()
        .iter()
        .map(|d| format!("TS{}: {}", d.code, d.message))
        .collect();

    let duration = start.elapsed();

    // For now, consider a test passed if there are no parse errors
    // Real conformance would compare with baseline files
    let passed = parse_errors.is_empty() && duration < CONFORMANCE_TEST_TIMEOUT;

    ConformanceResult {
        name: file_name.to_string(),
        passed,
        parse_errors,
        type_errors: Vec::new(), // Type checking deferred
        missing_errors: Vec::new(),
        extra_errors: Vec::new(),
        duration,
        timed_out: duration >= CONFORMANCE_TEST_TIMEOUT,
    }
}

/// Find all TypeScript test files in a directory
pub fn find_test_files(dir: &Path, max_files: usize) -> Vec<PathBuf> {
    let mut files = Vec::new();

    fn walk(dir: &Path, files: &mut Vec<PathBuf>, max_files: usize) {
        if files.len() >= max_files {
            return;
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if files.len() >= max_files {
                    break;
                }

                let path = entry.path();
                if path.is_dir() {
                    walk(&path, files, max_files);
                } else if let Some(ext) = path.extension() {
                    if ext == "ts" && !path.to_string_lossy().ends_with(".d.ts") {
                        files.push(path);
                    }
                }
            }
        }
    }

    walk(dir, &mut files, max_files);
    files
}

/// Summary of conformance test results
#[derive(Debug, Default)]
pub struct ConformanceSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub timed_out: usize,
    pub parse_errors: usize,
    pub total_duration: Duration,
}

impl ConformanceSummary {
    pub fn add(&mut self, result: &ConformanceResult) {
        self.total += 1;
        if result.passed {
            self.passed += 1;
        } else {
            self.failed += 1;
        }
        if result.timed_out {
            self.timed_out += 1;
        }
        if !result.parse_errors.is_empty() {
            self.parse_errors += 1;
        }
        self.total_duration += result.duration;
    }

    pub fn pass_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.passed as f64 / self.total as f64) * 100.0
        }
    }

    pub fn print(&self) {
        println!("\n=== Conformance Test Summary ===");
        println!("Total:        {}", self.total);
        println!("Passed:       {} ({:.1}%)", self.passed, self.pass_rate());
        println!("Failed:       {}", self.failed);
        println!("Timed out:    {}", self.timed_out);
        println!("Parse errors: {}", self.parse_errors);
        println!("Duration:     {:?}", self.total_duration);
    }
}

// Tests for the conformance runner itself
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_directives_strict() {
        let source = r#"
// @strict: true
// @target: es2020

const x: number = 42;
"#;
        let directives = parse_directives(source);
        assert_eq!(directives.strict, Some(true));
        assert_eq!(directives.target, Some("es2020".to_string()));
    }

    #[test]
    fn test_parse_directives_multi_file() {
        let source = r#"
// @filename: a.ts
export const a = 1;

// @filename: b.ts
import { a } from './a';
console.log(a);
"#;
        let directives = parse_directives(source);
        assert!(directives.is_multi_file);
        assert_eq!(directives.files.len(), 2);
        assert_eq!(directives.files[0].name, "a.ts");
        assert_eq!(directives.files[1].name, "b.ts");
    }

    #[test]
    fn test_run_simple_conformance() {
        let source = "const x: number = 42;";
        let result = run_conformance_test(source, "test.ts");
        assert!(result.parse_errors.is_empty());
        assert!(!result.timed_out);
    }

    #[test]
    fn test_run_with_parse_error() {
        let source = "const x: = 42;"; // Missing type
        let result = run_conformance_test(source, "test.ts");
        assert!(!result.parse_errors.is_empty());
    }

    #[test]
    fn test_directives_to_options() {
        let mut directives = TestDirectives::default();
        directives.strict = Some(true);

        let options = directives_to_options(&directives);
        assert!(options.strict);
        assert!(options.no_implicit_any);
        assert!(options.strict_null_checks);
    }
}

// Integration tests that run against actual conformance test files
// These are marked as ignored by default since they require the TypeScript test files
#[test]
#[ignore = "Requires TypeScript conformance test files"]
fn test_conformance_files() {
    let conformance_dir = PathBuf::from("TypeScript/tests/cases/conformance");
    if !conformance_dir.exists() {
        println!("Skipping: TypeScript conformance directory not found");
        return;
    }

    let files = find_test_files(&conformance_dir, 100);
    let mut summary = ConformanceSummary::default();

    for file in files {
        if let Ok(source) = fs::read_to_string(&file) {
            let file_name = file.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown.ts".to_string());

            let result = run_conformance_test(&source, &file_name);
            summary.add(&result);

            if !result.passed {
                println!("FAIL: {} - {:?}", file_name, result.parse_errors);
            }
        }
    }

    summary.print();
    assert!(summary.pass_rate() > 50.0, "Pass rate should be above 50%");
}
