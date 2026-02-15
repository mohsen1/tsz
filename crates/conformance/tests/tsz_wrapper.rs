use super::*;
use tsz::diagnostics::Diagnostic;
use tsz::span::Span;

fn compile_test(
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
                diagnostic_fingerprints: vec![],
                crashed: false,
                options: options.clone(),
            })
        }
        Ok(Err(e)) => Err(e), // Fatal error
        Err(_) => Ok(CompilationResult {
            error_codes: vec![],
            diagnostic_fingerprints: vec![],
            crashed: true,
            options: options.clone(),
        }),
    }
}

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
    // the output to extract diagnostic codes
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
                    diagnostics.push(Diagnostic::error(
                        "test.ts".to_string(),
                        Span::new(0, 0),
                        line.to_string(),
                        code,
                    ));
                }
            }
        }
    }

    diagnostics
}

fn extract_error_codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

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

    let prepared = prepare_test_dir(content, &filenames, &options, None).unwrap();
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

    let prepared = prepare_test_dir("", &filenames, &HashMap::new(), None).unwrap();
    let tsconfig_path = prepared.temp_dir.path().join("tsconfig.json");
    let tsconfig_contents = std::fs::read_to_string(tsconfig_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&tsconfig_contents).unwrap();

    assert_eq!(parsed["compilerOptions"]["moduleSuffixes"][0], ".ios");
    assert!(
        parsed.get("include").is_none(),
        "Expected provided tsconfig to be preserved without injected include"
    );
}

#[test]
fn test_normalize_diagnostic_path_strips_project_root() {
    let root = std::path::Path::new("/tmp/tsz-test");
    let raw = "/tmp/tsz-test/test.ts";
    assert_eq!(normalize_diagnostic_path(raw, root), "test.ts");
}

#[test]
fn test_normalize_diagnostic_path_handles_private_var_alias() {
    let root = std::path::Path::new("/var/folders/x/y/T/.tmp123");
    let raw = "/private/var/folders/x/y/T/.tmp123/src/test.ts";
    assert_eq!(normalize_diagnostic_path(raw, root), "src/test.ts");
}

#[test]
fn test_parse_diagnostics_from_text_extracts_error_codes() {
    let output = "test.ts(1,1): error TS2322: Type 'string' is not assignable to type 'number'.\n\
        tests/foo.ts(2,5): error TS2304: Cannot find name 'missing'.\n\
        note: unrelated text should be ignored";

    let diagnostics = parse_diagnostics_from_text(output);
    assert_eq!(extract_error_codes(&diagnostics), vec![2322, 2304]);
    assert_eq!(
        diagnostics[0].message,
        output.lines().next().unwrap().to_string()
    );
    assert_eq!(
        diagnostics[1].message,
        output.lines().nth(1).unwrap().to_string()
    );
}

#[test]
fn test_parse_diagnostics_from_text_ignores_non_error_lines() {
    let output = "tsserver: ready\n\
        no TS codes here\n\
        foo(1,1): warning: not parsed as error";

    let diagnostics = parse_diagnostics_from_text(output);
    assert_eq!(diagnostics.len(), 0);
}
