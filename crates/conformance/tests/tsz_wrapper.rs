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
                rewrite_bare_specifiers(&c, filename, filenames)
            } else {
                let c = resolve_lib_references(file_content, dir_path, ts_tests_lib_dir);
                let c = rewrite_absolute_reference_paths(&c);
                rewrite_bare_specifiers(&c, filename, filenames)
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
    // Match tsc's implicit include defaults: no .mts/.cts/.mjs/.cjs roots.
    let include = if allow_js {
        serde_json::json!([
            "*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"
        ])
    } else {
        serde_json::json!(["*.ts", "*.tsx", "**/*.ts", "**/*.tsx"])
    };
    if !has_tsconfig_file {
        let mut compiler_options = convert_options_to_tsconfig(options, &[]);
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
    let mut command = Command::new(tsz_path);
    command
        .arg("--project")
        .arg(base_dir)
        .arg("--noEmit")
        .arg("--pretty")
        .arg("false");

    // Pass environment variables for tracing
    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        command.env("RUST_LOG", rust_log);
    }
    if let Ok(rust_backtrace) = std::env::var("RUST_BACKTRACE") {
        command.env("RUST_BACKTRACE", rust_backtrace);
    }

    let output = command.output()?;

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
fn test_prepare_test_dir_copies_root_tsconfig_to_root() {
    let content = "";
    let filenames = vec![
        (
            "tsconfig.json".to_string(),
            r#"{"compilerOptions": {}}"#.to_string(),
        ),
        ("src/app.ts".to_string(), "export const x = 1;".to_string()),
    ];
    let options: HashMap<String, String> = HashMap::new();

    let prepared = prepare_test_dir(content, &filenames, &options, None, &[], None).unwrap();
    let root_tsconfig = prepared.temp_dir.path().join("tsconfig.json");
    assert!(
        root_tsconfig.is_file(),
        "tsconfig should exist at project root"
    );
}

#[test]
fn test_prepare_test_dir_does_not_copy_non_root_tsconfig_to_root() {
    let content = "";
    let filenames = vec![
        (
            "configs/tsconfig.json".to_string(),
            r#"{"compilerOptions": {}}"#.to_string(),
        ),
        ("src/app.ts".to_string(), "export const x = 1;".to_string()),
    ];
    let options: HashMap<String, String> = HashMap::new();

    let prepared = prepare_test_dir(content, &filenames, &options, None, &[], None).unwrap();
    let root_tsconfig = prepared.temp_dir.path().join("tsconfig.json");
    assert!(
        !root_tsconfig.exists(),
        "non-root tsconfig should not be promoted to project root"
    );
}

#[test]
fn test_prepare_test_dir_preserves_explicit_allow_js_false_with_check_js() {
    let content = "var x;";
    let filenames: Vec<(String, String)> = Vec::new();
    let options: HashMap<String, String> = HashMap::from([
        ("checkJs".to_string(), "true".to_string()),
        ("allowJs".to_string(), "false".to_string()),
    ]);

    let prepared = prepare_test_dir(content, &filenames, &options, None, &[], None).unwrap();
    let tsconfig_path = prepared.temp_dir.path().join("tsconfig.json");
    let tsconfig_raw = std::fs::read_to_string(tsconfig_path).unwrap();
    let tsconfig_json: serde_json::Value = serde_json::from_str(&tsconfig_raw).unwrap();
    let compiler_options = tsconfig_json
        .get("compilerOptions")
        .and_then(serde_json::Value::as_object)
        .expect("compilerOptions object should exist");

    assert_eq!(
        compiler_options.get("checkJs"),
        Some(&serde_json::Value::Bool(true))
    );
    assert_eq!(
        compiler_options.get("allowJs"),
        Some(&serde_json::Value::Bool(false))
    );
}

#[test]
fn test_prepare_test_dir_no_implicit_references_uses_last_unit_as_root_file() {
    let content = "";
    let filenames = vec![
        (
            "/typings/phaser/types/phaser.d.ts".to_string(),
            "declare module \"phaser\" { export const a2: number; }".to_string(),
        ),
        (
            "/typings/phaser/package.json".to_string(),
            r#"{ "name": "phaser", "version": "1.2.3", "types": "types/phaser.d.ts" }"#.to_string(),
        ),
        (
            "a.ts".to_string(),
            "import { a2 } from \"phaser\";".to_string(),
        ),
    ];
    let options: HashMap<String, String> =
        HashMap::from([("noImplicitReferences".to_string(), "true".to_string())]);

    let prepared = prepare_test_dir(content, &filenames, &options, None, &[], None).unwrap();
    let tsconfig_path = prepared.temp_dir.path().join("tsconfig.json");
    let tsconfig_raw = std::fs::read_to_string(tsconfig_path).unwrap();
    let tsconfig_json: serde_json::Value = serde_json::from_str(&tsconfig_raw).unwrap();

    assert_eq!(
        tsconfig_json.get("files"),
        Some(&serde_json::json!(["a.ts"])),
        "noImplicitReferences should keep only the last unit as a root file, got {tsconfig_raw}"
    );
    assert!(
        tsconfig_json.get("include").is_none(),
        "noImplicitReferences root-file mode should not synthesize include globs, got {tsconfig_raw}"
    );
}

#[test]
fn test_prepare_test_dir_threads_no_types_and_symbols_into_generated_tsconfig() {
    let content = "";
    let filenames = vec![("usage.ts".to_string(), "export {};".to_string())];
    let options: HashMap<String, String> =
        HashMap::from([("noTypesAndSymbols".to_string(), "true".to_string())]);

    let prepared = prepare_test_dir(content, &filenames, &options, None, &[], None).unwrap();
    let tsconfig_path = prepared.temp_dir.path().join("tsconfig.json");
    let tsconfig_raw = std::fs::read_to_string(tsconfig_path).unwrap();
    let tsconfig_json: serde_json::Value = serde_json::from_str(&tsconfig_raw).unwrap();
    let compiler_options = tsconfig_json
        .get("compilerOptions")
        .and_then(serde_json::Value::as_object)
        .expect("compilerOptions object should exist");

    assert_eq!(
        compiler_options.get("noTypesAndSymbols"),
        Some(&serde_json::Value::Bool(true))
    );
}

#[test]
fn test_prepare_test_dir_threads_no_types_and_symbols_into_root_tsconfig_merge() {
    let content = "";
    let filenames = vec![
        (
            "tsconfig.json".to_string(),
            r#"{"compilerOptions":{"module":"commonjs"}}"#.to_string(),
        ),
        ("usage.ts".to_string(), "export {};".to_string()),
    ];
    let options: HashMap<String, String> =
        HashMap::from([("noTypesAndSymbols".to_string(), "true".to_string())]);

    let prepared = prepare_test_dir(content, &filenames, &options, None, &[], None).unwrap();
    let tsconfig_path = prepared.temp_dir.path().join("tsconfig.json");
    let tsconfig_raw = std::fs::read_to_string(tsconfig_path).unwrap();
    let tsconfig_json: serde_json::Value = serde_json::from_str(&tsconfig_raw).unwrap();
    let compiler_options = tsconfig_json
        .get("compilerOptions")
        .and_then(serde_json::Value::as_object)
        .expect("compilerOptions object should exist");

    assert_eq!(
        compiler_options.get("module"),
        Some(&serde_json::Value::String("commonjs".to_string()))
    );
    assert_eq!(
        compiler_options.get("noTypesAndSymbols"),
        Some(&serde_json::Value::Bool(true))
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
#[ignore = "requires tsz binary: cargo build --profile dist-fast -p tsz-cli"]
fn test_compile_simple_error() {
    let _ = tracing_subscriber::fmt::try_init();
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
#[ignore = "requires tsz binary: cargo build --profile dist-fast -p tsz-cli"]
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
fn test_convert_options_leaves_strict_absent_when_not_explicit() {
    let opts = convert_options_to_tsconfig(&HashMap::new(), &[]);
    let compiler_options = opts
        .as_object()
        .expect("compilerOptions should be an object");
    assert!(
        !compiler_options.contains_key("strict"),
        "strict should stay omitted when the test does not specify @strict"
    );
}

#[test]
fn test_convert_options_expands_explicit_strict_false() {
    let options = HashMap::from([("strict".to_string(), "false".to_string())]);
    let opts = convert_options_to_tsconfig(&options, &[]);
    let compiler_options = opts
        .as_object()
        .expect("compilerOptions should be an object");
    assert_eq!(
        compiler_options.get("strict"),
        Some(&serde_json::Value::Bool(false))
    );
    assert_eq!(
        compiler_options.get("strictPropertyInitialization"),
        Some(&serde_json::Value::Bool(false))
    );
}

#[test]
fn test_prepare_test_dir_does_not_inject_strict_overrides_for_source_pragmas() {
    // TypeScript 6.0+ defaults strict-family flags to true, so the conformance
    // wrapper no longer synthesizes a non-strict baseline.  Verify that
    // strictness flags are NOT injected into the tsconfig when the source only
    // uses non-strict source pragmas (like @noImplicitReturns).
    let content = r#"
// @noImplicitReturns: true
class C<T> {
    foo: T;
}
"#;

    let prepared =
        prepare_test_dir(content, &[], &HashMap::new(), None, &[], Some(&[2754])).unwrap();
    let tsconfig = std::fs::read_to_string(prepared.temp_dir.path().join("tsconfig.json"))
        .expect("tsconfig should be written");
    let parsed: serde_json::Value =
        serde_json::from_str(&tsconfig).expect("tsconfig should be valid json");
    let compiler_options = parsed["compilerOptions"]
        .as_object()
        .expect("compilerOptions should be an object");

    // None of these should be synthesized — TS 6.0 defaults them to true.
    for key in [
        "noImplicitAny",
        "strictNullChecks",
        "strictFunctionTypes",
        "strictBindCallApply",
        "useUnknownInCatchVariables",
        "noImplicitThis",
        "strict",
        "alwaysStrict",
    ] {
        assert!(
            !compiler_options.contains_key(key),
            "{key} should not be injected into tsconfig for source-pragma-only tests"
        );
    }
}

#[test]
fn test_prepare_test_dir_keeps_target_only_tsconfig_minimal() {
    let content = r#"
// @target: es2015
class C {
    static value = function () { return this; }
}
"#;

    let prepared =
        prepare_test_dir(content, &[], &HashMap::new(), None, &[], Some(&[2564])).unwrap();
    let tsconfig = std::fs::read_to_string(prepared.temp_dir.path().join("tsconfig.json"))
        .expect("tsconfig should be written");
    let parsed: serde_json::Value =
        serde_json::from_str(&tsconfig).expect("tsconfig should be valid json");
    let compiler_options = parsed["compilerOptions"]
        .as_object()
        .expect("compilerOptions should be an object");

    for key in [
        "noImplicitThis",
        "noImplicitAny",
        "strictNullChecks",
        "strictFunctionTypes",
        "strictBindCallApply",
        "strictPropertyInitialization",
        "useUnknownInCatchVariables",
    ] {
        assert!(
            !compiler_options.contains_key(key),
            "Did not expect {key} to be synthesized for target-only source pragmas"
        );
    }
}

#[test]
fn test_rewrite_bare_specifiers() {
    let filenames = vec![
        ("server.ts".to_string(), "export class c {}".to_string()),
        ("client.ts".to_string(), "".to_string()),
    ];

    // Test export * from
    let content = r#"export * from "server";"#;
    let result = rewrite_bare_specifiers(content, "client.ts", &filenames);
    assert_eq!(result, r#"export * from "./server";"#);

    // Test import from
    let content = r#"import { x } from "server";"#;
    let result = rewrite_bare_specifiers(content, "client.ts", &filenames);
    assert_eq!(result, r#"import { x } from "./server";"#);

    // Test side-effect import
    let content = r#"import "server";"#;
    let result = rewrite_bare_specifiers(content, "client.ts", &filenames);
    assert_eq!(result, r#"import "server";"#);

    // Test require
    let content = r#"const x = require("server");"#;
    let result = rewrite_bare_specifiers(content, "client.ts", &filenames);
    assert_eq!(result, r#"const x = require("./server");"#);

    // Should NOT rewrite npm packages
    let content = r#"import { x } from "lodash";"#;
    let result = rewrite_bare_specifiers(content, "client.ts", &filenames);
    assert_eq!(result, r#"import { x } from "lodash";"#);

    // Should NOT rewrite relative paths
    let content = r#"import { x } from "./server";"#;
    let result = rewrite_bare_specifiers(content, "client.ts", &filenames);
    assert_eq!(result, r#"import { x } from "./server";"#);

    // Should NOT rewrite absolute paths
    let content = r#"import { x } from "/server";"#;
    let result = rewrite_bare_specifiers(content, "client.ts", &filenames);
    assert_eq!(result, r#"import { x } from "/server";"#);

    // Should NOT rewrite scoped packages
    let content = r#"import { x } from "@scope/package";"#;
    let result = rewrite_bare_specifiers(content, "client.ts", &filenames);
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
    let result = rewrite_bare_specifiers(content, "b.ts", &filenames);
    assert_eq!(result, r#"import * as a from "./a";"#);

    // Test with .d.cts
    let filenames = vec![
        ("types.d.cts".to_string(), "export {};".to_string()),
        ("index.cts".to_string(), "".to_string()),
    ];

    let content = r#"import { T } from "types";"#;
    let result = rewrite_bare_specifiers(content, "index.cts", &filenames);
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
    let result = rewrite_bare_specifiers(content, "/a.ts", &filenames);
    assert_eq!(result, r#"import "foo";"#);
}

#[test]
fn test_rewrite_bare_specifiers_skips_self_name_package_imports() {
    let filenames = vec![
        (
            "index.js".to_string(),
            r#"import * as self from "package";"#.to_string(),
        ),
        (
            "package.json".to_string(),
            r#"{"name":"package","private":true,"type":"module","exports":"./index.js"}"#
                .to_string(),
        ),
    ];

    let content = r#"import * as self from "package";"#;
    let result = rewrite_bare_specifiers(content, "index.js", &filenames);
    assert_eq!(result, content);
}

#[test]
fn test_rewrite_bare_specifiers_skips_package_root_self_name_with_ts_variants() {
    let filenames = vec![
        (
            "index.ts".to_string(),
            r#"import * as self from "package";"#.to_string(),
        ),
        (
            "index.mts".to_string(),
            r#"import * as self from "package";"#.to_string(),
        ),
        (
            "index.cts".to_string(),
            r#"import * as self from "package";"#.to_string(),
        ),
        (
            "package.json".to_string(),
            r#"{"name":"package","private":true,"type":"module","exports":"./index.js"}"#
                .to_string(),
        ),
    ];

    let content = r#"import * as self from "package";"#;
    assert_eq!(
        rewrite_bare_specifiers(content, "index.ts", &filenames),
        content
    );
    assert_eq!(
        rewrite_bare_specifiers(content, "index.mts", &filenames),
        content
    );
    assert_eq!(
        rewrite_bare_specifiers(content, "index.cts", &filenames),
        content
    );
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

    let prepared = prepare_test_dir("", &filenames, &HashMap::new(), None, &[], None).unwrap();
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
fn test_prepare_test_dir_implicit_include_includes_module_extensions() {
    let filenames = vec![
        ("/index.js".to_string(), "export {};".to_string()),
        ("/index.mjs".to_string(), "export {};".to_string()),
        ("/index.cjs".to_string(), "export {};".to_string()),
    ];
    let options = HashMap::from([
        ("allowjs".to_string(), "true".to_string()),
        ("checkjs".to_string(), "true".to_string()),
    ]);

    let prepared = prepare_test_dir("", &filenames, &options, None, &[], None).unwrap();
    let tsconfig_path = prepared.temp_dir.path().join("tsconfig.json");
    let tsconfig_contents = std::fs::read_to_string(tsconfig_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&tsconfig_contents).unwrap();
    let include = parsed["include"].as_array().expect("include array");
    let include_values: Vec<_> = include.iter().filter_map(|v| v.as_str()).collect();

    // Include patterns match tsc's harness (narrow, no .mts/.cts/.mjs/.cjs).
    // Module-extension files are listed in "files" instead.
    assert!(include_values.contains(&"*.ts"));
    assert!(include_values.contains(&"*.tsx"));
    assert!(
        !include_values.contains(&"*.mts"),
        "*.mts should not be in include"
    );
    assert!(
        !include_values.contains(&"*.cts"),
        "*.cts should not be in include"
    );
    assert!(include_values.contains(&"*.js"));
    assert!(include_values.contains(&"*.jsx"));
    assert!(
        !include_values.contains(&"*.mjs"),
        "*.mjs should not be in include"
    );
    assert!(
        !include_values.contains(&"*.cjs"),
        "*.cjs should not be in include"
    );

    // Module-extension files are listed explicitly in "files"
    let files = parsed["files"]
        .as_array()
        .expect("files array for module-ext tests");
    let file_values: Vec<_> = files.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        file_values.contains(&"index.mjs"),
        "index.mjs should be in files"
    );
    assert!(
        file_values.contains(&"index.cjs"),
        "index.cjs should be in files"
    );
}

#[test]
#[ignore = "requires tsz binary: cargo build --profile dist-fast -p tsz-cli"]
fn test_compile_prepared_dir_mts_only_emits_ts18003() {
    let content = r#"
// @target: es2015
// @module: esnext
// @moduleResolution: node16
// @allowJs: true
// @noEmit: true

// @Filename: /index.mts
export const x = 1;
"#;
    let filenames = vec![("/index.mts".to_string(), "export const x = 1;".to_string())];
    let options = HashMap::from([
        ("target".to_string(), "es2015".to_string()),
        ("module".to_string(), "esnext".to_string()),
        ("moduleresolution".to_string(), "node16".to_string()),
        ("allowJs".to_string(), "true".to_string()),
        ("noEmit".to_string(), "true".to_string()),
    ]);

    let tsz = find_tsz_binary();
    let result = compile_test(content, &filenames, &options, &tsz).unwrap();

    // tsc's test harness include patterns (*.ts, *.tsx, *.js, *.jsx, etc.) do NOT
    // match .mts files via glob. So an .mts-only test with wrapper-generated include
    // correctly gets TS18003 "no inputs found", matching tsc's test harness behavior.
    assert!(
        result.error_codes.contains(&18003),
        "mts-only input should trigger TS18003 (matches tsc), got: {:?}",
        result.error_codes
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
fn test_normalize_message_paths_normalizes_ts5057_directory() {
    let root = std::path::Path::new("/tmp/tsz-test");
    let raw = "Cannot find a tsconfig.json file at the specified directory: '/a/b/c'.";
    assert_eq!(
        normalize_message_paths(raw, root),
        "Cannot find a tsconfig.json file at the specified directory: ''."
    );
}

#[test]
fn test_is_windows_absolute_path_drive_letters() {
    assert!(is_windows_absolute_path("A:/foo/bar.ts"));
    assert!(is_windows_absolute_path("C:/Users/test.ts"));
    assert!(is_windows_absolute_path("B:\\foo\\bar.ts"));
    assert!(is_windows_absolute_path("a:/lowercase.ts"));
    assert!(is_windows_absolute_path("Z:/last.ts"));
}

#[test]
fn test_is_windows_absolute_path_rejects_non_windows() {
    assert!(!is_windows_absolute_path("/unix/path.ts"));
    assert!(!is_windows_absolute_path("relative/path.ts"));
    assert!(!is_windows_absolute_path("test.ts"));
    assert!(!is_windows_absolute_path(""));
    assert!(!is_windows_absolute_path("A:"));
    assert!(!is_windows_absolute_path("1:/digit.ts"));
    assert!(!is_windows_absolute_path("AB:/two.ts"));
}

#[test]
fn test_prepare_test_dir_skips_windows_absolute_path_files() {
    // When ALL filenames are Windows-style absolute paths, no source files should
    // be written to the temp directory (matching tsc's behavior of emitting TS18003).
    let filenames = vec![
        ("A:/foo/bar.ts".to_string(), "var x: number;".to_string()),
        ("A:/foo/baz.ts".to_string(), "var y: number;".to_string()),
    ];
    let options: HashMap<String, String> =
        HashMap::from([("target".to_string(), "es2015".to_string())]);

    let prepared = prepare_test_dir("", &filenames, &options, None, &[], None).unwrap();
    let dir = prepared.temp_dir.path();

    // Only tsconfig.json should exist, no source files
    let ts_files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "ts").unwrap_or(false))
        .collect();
    assert!(
        ts_files.is_empty(),
        "No .ts files should be written for Windows-path-only tests, found: {:?}",
        ts_files.iter().map(|e| e.path()).collect::<Vec<_>>()
    );

    // tsconfig.json should still exist
    assert!(dir.join("tsconfig.json").exists());
}

#[test]
fn test_prepare_test_dir_keeps_mixed_path_files() {
    // When filenames mix Windows paths with normal paths, normal files should still be written.
    let filenames = vec![
        ("A:/foo/bar.ts".to_string(), "var x: number;".to_string()),
        ("test.ts".to_string(), "var y: number;".to_string()),
    ];
    let options: HashMap<String, String> = HashMap::new();

    let prepared = prepare_test_dir("", &filenames, &options, None, &[], None).unwrap();
    let dir = prepared.temp_dir.path();

    // test.ts should be written
    assert!(dir.join("test.ts").exists());
}

#[test]
fn test_prepare_test_dir_applies_link_directives_as_symlinks() {
    let content = r#"
// @link: /packages/search -> /node_modules/search
"#;
    let filenames = vec![(
        "/packages/search/package.json".to_string(),
        r#"{"name":"search"}"#.to_string(),
    )];
    let options: HashMap<String, String> = HashMap::new();

    let prepared = prepare_test_dir(content, &filenames, &options, None, &[], None).unwrap();
    let link_path = prepared.temp_dir.path().join("node_modules/search");
    let target_path = prepared.temp_dir.path().join("packages/search");

    let metadata = std::fs::symlink_metadata(&link_path).expect("link should exist");
    assert!(metadata.file_type().is_symlink(), "expected a symlink");
    let resolved = std::fs::canonicalize(&link_path).expect("symlink should resolve");
    let canonical_target = std::fs::canonicalize(&target_path).expect("target should resolve");
    assert_eq!(resolved, canonical_target);
}

#[test]
fn test_prepare_test_dir_remaps_virtual_absolute_path_options() {
    let content = "";
    let filenames = vec![(
        "/packages/app/src/index.ts".to_string(),
        "export const x = 1;".to_string(),
    )];
    let options: HashMap<String, String> = HashMap::from([
        ("rootDir".to_string(), "/packages/app/src".to_string()),
        ("outDir".to_string(), "/packages/app/lib".to_string()),
        (
            "declarationDir".to_string(),
            "/packages/app/types".to_string(),
        ),
    ]);

    let prepared = prepare_test_dir(content, &filenames, &options, None, &[], None).unwrap();
    let tsconfig_raw = std::fs::read_to_string(prepared.project_dir.join("tsconfig.json"))
        .expect("tsconfig should exist");
    let tsconfig_json: serde_json::Value = serde_json::from_str(&tsconfig_raw).unwrap();
    let compiler_options = tsconfig_json["compilerOptions"]
        .as_object()
        .expect("compilerOptions should be an object");

    assert_eq!(
        compiler_options
            .get("rootDir")
            .and_then(serde_json::Value::as_str),
        Some(
            prepared
                .temp_dir
                .path()
                .join("packages/app/src")
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(
        compiler_options
            .get("outDir")
            .and_then(serde_json::Value::as_str),
        Some(
            prepared
                .temp_dir
                .path()
                .join("packages/app/lib")
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(
        compiler_options
            .get("declarationDir")
            .and_then(serde_json::Value::as_str),
        Some(
            prepared
                .temp_dir
                .path()
                .join("packages/app/types")
                .to_string_lossy()
                .as_ref()
        )
    );
}

#[test]
fn test_prepare_test_dir_uses_package_root_as_project_dir_for_absolute_package_tests() {
    let content = "";
    let filenames = vec![
        (
            "/pkg/package.json".to_string(),
            r#"{
  "name": "@this/package",
  "type": "module",
  "exports": {
    ".": {
      "default": "./dist/index.js",
      "types": "./types/index.d.ts"
    }
  }
}"#
            .to_string(),
        ),
        (
            "/pkg/src/index.ts".to_string(),
            r#"import * as me from "@this/package";
me.thing();
export function thing(): void {}"#
                .to_string(),
        ),
    ];
    let options: HashMap<String, String> = HashMap::from([
        ("target".to_string(), "es2015".to_string()),
        ("module".to_string(), "nodenext".to_string()),
        ("outDir".to_string(), "/pkg/dist".to_string()),
        ("declarationDir".to_string(), "/pkg/types".to_string()),
        ("declaration".to_string(), "true".to_string()),
        ("rootDir".to_string(), "/pkg/src".to_string()),
    ]);

    let prepared = prepare_test_dir(content, &filenames, &options, None, &[], None).unwrap();
    assert_eq!(prepared.project_dir, prepared.temp_dir.path().join("pkg"));
    assert!(prepared.project_dir.join("tsconfig.json").is_file());
}

#[test]
fn test_normalize_message_paths_normalizes_ts5057_not_found() {
    let root = std::path::Path::new("/tmp/tsz-test");
    let raw = "tsconfig not found at /tmp/tsz-test/tsconfig.json";
    assert_eq!(
        normalize_message_paths(raw, root),
        "Cannot find a tsconfig.json file at the specified directory: ''."
    );
}

#[test]
fn test_normalize_message_paths_preserves_virtual_absolute_root_dir_prefix() {
    let root = std::path::Path::new("/tmp/tsz-test");
    let raw = "File 'packages/search/lib/index.d.ts' is not under 'rootDir' 'packages/search-prefix/src'. 'rootDir' is expected to contain all source files.";
    assert_eq!(
        normalize_message_paths(raw, root),
        "File 'packages/search/lib/index.d.ts' is not under 'rootDir' '/packages/search-prefix/src'. 'rootDir' is expected to contain all source files."
    );
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

#[test]
fn test_parse_error_codes_ignores_indented_related_diagnostics() {
    let output = "test.ts(3,1): error TS2322: Type 'B' is not assignable to type 'A'.\n  test.ts(3,5): error TS2328: Types of parameters 'cb' and 'cb' are incompatible.\n  Types of property 'f' are incompatible.";

    assert_eq!(parse_error_codes_from_text(output), vec![2322]);
}

#[test]
fn test_parse_diagnostic_fingerprints_ignores_indented_related_diagnostics() {
    let root = std::path::Path::new("/tmp/tsz-test");
    let output = "test.ts(3,1): error TS2322: Type 'B' is not assignable to type 'A'.\n  test.ts(3,5): error TS2328: Types of parameters 'cb' and 'cb' are incompatible.";

    let fingerprints = parse_diagnostic_fingerprints_from_text(output, root);
    assert_eq!(fingerprints.len(), 1);
    assert_eq!(fingerprints[0].code, 2322);
}

#[test]
fn test_parse_diagnostic_fingerprints_from_text_handles_colon_prefixed_no_pos() {
    let root = std::path::Path::new("/tmp/tsz-test");
    let output = ": error TS5057: tsconfig not found at /var/tmp/tsconfig.json";
    let fingerprints = parse_diagnostic_fingerprints_from_text(output, root);
    assert_eq!(fingerprints.len(), 1);
    let fp = &fingerprints[0];
    assert_eq!(
        fp.display_key(),
        "TS5057 <unknown>:0:0 Cannot find a tsconfig.json file at the specified directory: ''."
    );
}
