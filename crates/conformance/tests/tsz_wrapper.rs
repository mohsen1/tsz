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
        if let serde_json::Value::Object(ref mut map) = compiler_options {
            map.entry("skipLibCheck")
                .or_insert(serde_json::Value::Bool(true));
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
fn test_tests_lib_dir_for_cases_dir_uses_sibling_lib_directory() {
    let temp = tempfile::tempdir().unwrap();
    let cases_dir = temp.path().join("TypeScript/tests/cases");
    std::fs::create_dir_all(&cases_dir).unwrap();

    assert_eq!(
        tests_lib_dir_for_cases_dir(&cases_dir),
        temp.path()
            .canonicalize()
            .unwrap()
            .join("TypeScript/tests/lib")
    );
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
fn test_normalize_message_paths_keeps_current_ts2883_node_modules_shape() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let message = "The inferred type of 'Form' cannot be named without a reference to 'HTMLAttributes' from './node_modules/react'. This is likely not portable. A type annotation is necessary.";

    let normalized = normalize_message_paths(message, project_root);

    assert_eq!(
        normalized,
        "The inferred type of 'Form' cannot be named without a reference to 'HTMLAttributes' from '../../../../../..node_modules/react'. This is likely not portable. A type annotation is necessary."
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
fn test_prepare_test_dir_no_implicit_references_keeps_authored_declaration_roots() {
    let content = "";
    let filenames = vec![
        (
            "foo.d.ts".to_string(),
            "export var x: number; export as namespace Foo;".to_string(),
        ),
        ("a.ts".to_string(), "Foo.x;".to_string()),
    ];
    let options: HashMap<String, String> =
        HashMap::from([("noImplicitReferences".to_string(), "true".to_string())]);

    let prepared = prepare_test_dir(content, &filenames, &options, None, &[], None).unwrap();
    let tsconfig_path = prepared.temp_dir.path().join("tsconfig.json");
    let tsconfig_raw = std::fs::read_to_string(tsconfig_path).unwrap();
    let tsconfig_json: serde_json::Value = serde_json::from_str(&tsconfig_raw).unwrap();
    let files = tsconfig_json["files"].as_array().expect("files array");
    let file_values: Vec<_> = files.iter().filter_map(|value| value.as_str()).collect();

    assert!(
        file_values.contains(&"foo.d.ts"),
        "authored declaration roots should stay in noImplicitReferences files, got {file_values:?}"
    );
    assert!(
        file_values.contains(&"a.ts"),
        "source roots should stay in noImplicitReferences files, got {file_values:?}"
    );
}

#[test]
fn test_prepare_test_dir_no_implicit_references_keeps_type_roots_declarations() {
    let content = "";
    let filenames = vec![
        (
            "/a/types/jquery/index.d.ts".to_string(),
            "declare var $: { foo(): void };".to_string(),
        ),
        (
            "/a/types/jquery2/index.d.ts".to_string(),
            "declare var $2: { foo(): void };".to_string(),
        ),
        (
            "/a/b/consumer.ts".to_string(),
            "$.foo(); $2.foo();".to_string(),
        ),
    ];
    let options: HashMap<String, String> = HashMap::from([
        ("noImplicitReferences".to_string(), "true".to_string()),
        ("types".to_string(), "jquery".to_string()),
        ("typeRoots".to_string(), "/a/types".to_string()),
    ]);

    let prepared = prepare_test_dir(content, &filenames, &options, None, &[], None).unwrap();
    let tsconfig_path = prepared.temp_dir.path().join("tsconfig.json");
    let tsconfig_raw = std::fs::read_to_string(tsconfig_path).unwrap();
    let tsconfig_json: serde_json::Value = serde_json::from_str(&tsconfig_raw).unwrap();
    let files = tsconfig_json["files"].as_array().expect("files array");
    let file_values: Vec<_> = files.iter().filter_map(|value| value.as_str()).collect();

    assert!(
        file_values.contains(&"a/types/jquery/index.d.ts"),
        "declared typeRoots package should stay in noImplicitReferences files, got {file_values:?}"
    );
    assert!(
        file_values.contains(&"a/types/jquery2/index.d.ts"),
        "adjacent typeRoots package should stay in noImplicitReferences files, got {file_values:?}"
    );
    assert!(
        file_values.contains(&"a/b/consumer.ts"),
        "consumer should stay in noImplicitReferences files, got {file_values:?}"
    );
}

#[test]
fn test_prepare_test_dir_no_implicit_references_excludes_linked_package_declarations() {
    let content = r#"
// @filename: Folder/monorepo/package-a/index.d.ts
export declare const styles: import("styled-components").InterpolationValue[];

// @filename: Folder/monorepo/core/index.ts
import { styles } from "package-a";

// @link: Folder/monorepo/package-a -> Folder/monorepo/core/node_modules/package-a
"#;
    let filenames = vec![
        (
            "Folder/monorepo/package-a/index.d.ts".to_string(),
            "export declare const styles: number;".to_string(),
        ),
        (
            "Folder/monorepo/core/index.ts".to_string(),
            "import { styles } from 'package-a';".to_string(),
        ),
    ];
    let options: HashMap<String, String> =
        HashMap::from([("noImplicitReferences".to_string(), "true".to_string())]);

    let prepared =
        prepare_test_dir(content, &filenames, &options, None, &[], Some(&[2883])).unwrap();
    let tsconfig_path = prepared.temp_dir.path().join("tsconfig.json");
    let tsconfig_raw = std::fs::read_to_string(tsconfig_path).unwrap();
    let tsconfig_json: serde_json::Value = serde_json::from_str(&tsconfig_raw).unwrap();
    let files = tsconfig_json["files"].as_array().expect("files array");
    let file_values: Vec<_> = files.iter().filter_map(|value| value.as_str()).collect();

    assert!(
        !file_values.contains(&"Folder/monorepo/package-a/index.d.ts"),
        "linked package declarations should remain resolution-only, got {file_values:?}"
    );
    assert!(
        file_values.contains(&"Folder/monorepo/core/index.ts"),
        "source file should remain a root, got {file_values:?}"
    );
    assert!(
        prepared
            .temp_dir
            .path()
            .join("Folder/monorepo/core/node_modules/package-a/index.d.ts")
            .exists(),
        "linked package declaration should still be available through node_modules"
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

#[test]
fn test_prepare_test_dir_no_types_and_symbols_excludes_at_types_from_root_files() {
    let content = "";
    let filenames = vec![
        (
            "usage.ts".to_string(),
            r#"import { parse } from "url";"#.to_string(),
        ),
        (
            "/node_modules/@types/node/index.d.ts".to_string(),
            r#"declare module "url" { export function parse(): void; }"#.to_string(),
        ),
    ];
    let options: HashMap<String, String> =
        HashMap::from([("noTypesAndSymbols".to_string(), "true".to_string())]);

    let prepared = prepare_test_dir(content, &filenames, &options, None, &[], None).unwrap();
    let tsconfig_path = prepared.temp_dir.path().join("tsconfig.json");
    let tsconfig_raw = std::fs::read_to_string(&tsconfig_path).unwrap();
    let tsconfig_json: serde_json::Value = serde_json::from_str(&tsconfig_raw).unwrap();

    // The "files" array should NOT contain the @types/node file when
    // noTypesAndSymbols is set — tsc's harness excludes @types from roots.
    if let Some(files) = tsconfig_json
        .get("files")
        .and_then(serde_json::Value::as_array)
    {
        let has_types_file = files
            .iter()
            .any(|f| f.as_str().is_some_and(|s| s.contains("@types")));
        assert!(
            !has_types_file,
            "noTypesAndSymbols should exclude @types files from root files list, got files: {files:?}"
        );
    }
}

fn find_tsz_binary() -> String {
    // Try common build locations relative to workspace root
    let candidates = [
        ".target/dist-fast/tsz",
        ".target/debug/tsz",
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
fn test_prepare_test_dir_preserves_target_default_lib_resolution() {
    let options = HashMap::from([("target".to_string(), "esnext".to_string())]);
    let prepared = prepare_test_dir("", &[], &options, Some("ts"), &[], Some(&[])).unwrap();
    let tsconfig = std::fs::read_to_string(prepared.temp_dir.path().join("tsconfig.json"))
        .expect("tsconfig should be written");
    let parsed: serde_json::Value =
        serde_json::from_str(&tsconfig).expect("tsconfig should be valid json");
    assert!(
        parsed["compilerOptions"]["target"] == "esnext",
        "target should be preserved in generated tsconfig: {parsed:?}"
    );
    assert!(
        parsed["compilerOptions"].get("lib").is_none(),
        "target-only options must leave lib absent so tsz resolves the same default full lib set as tsc: {parsed:?}"
    );
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
fn test_prepare_test_dir_implicit_include_matches_tsc_harness() {
    let filenames = vec![
        ("/index.js".to_string(), "export {};".to_string()),
        ("/index.mjs".to_string(), "export {};".to_string()),
        ("/index.cjs".to_string(), "export {};".to_string()),
        ("/index.ts".to_string(), "export {};".to_string()),
        (
            "/node_modules/pkg/index.d.ts".to_string(),
            "export declare const x: number;".to_string(),
        ),
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
    assert!(
        file_values.contains(&"index.ts"),
        "plain .ts roots should stay in files when module-extension inputs are explicit"
    );
    assert!(
        file_values.contains(&"node_modules/pkg/index.d.ts"),
        "authored node_modules fixtures should stay in files when explicit roots are used"
    );
    assert!(
        parsed.get("exclude").is_none(),
        "explicit root-file tests should not exclude node_modules"
    );
}

#[test]
fn test_prepare_test_dir_ts2883_keeps_node_modules_declarations_resolution_only() {
    let filenames = vec![
        (
            "node_modules/pkg/index.d.ts".to_string(),
            "export declare const x: number;".to_string(),
        ),
        (
            "index.ts".to_string(),
            "import { x } from 'pkg'; x;".to_string(),
        ),
    ];

    let prepared =
        prepare_test_dir("", &filenames, &HashMap::new(), None, &[], Some(&[2883])).unwrap();
    let tsconfig_path = prepared.temp_dir.path().join("tsconfig.json");
    let tsconfig_contents = std::fs::read_to_string(tsconfig_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&tsconfig_contents).unwrap();
    let files = parsed["files"].as_array().expect("files array");
    let file_values: Vec<_> = files.iter().filter_map(|v| v.as_str()).collect();

    assert!(file_values.contains(&"index.ts"));
    assert!(
        !file_values.contains(&"node_modules/pkg/index.d.ts"),
        "TS2883 portability fixtures should resolve package declarations through imports, not root files"
    );
    assert!(
        prepared
            .temp_dir
            .path()
            .join("node_modules/pkg/index.d.ts")
            .exists(),
        "node_modules declaration should still be available on disk"
    );
}

#[test]
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
fn test_normalize_message_paths_normalizes_ts5057_specified_directory() {
    let root = std::path::Path::new("/tmp/tsz-test");
    let raw = "Cannot find a tsconfig.json file at the specified directory: 'empty-dir'.";
    assert_eq!(
        normalize_message_paths(raw, root),
        "Cannot find a tsconfig.json file at the specified directory: ''."
    );
}

#[test]
fn test_normalize_message_paths_normalizes_ts5058_path_does_not_exist() {
    let root = std::path::Path::new("/tmp/tsz-test");
    let raw = "The specified path does not exist: 'missing/tsconfig.json'.";
    assert_eq!(
        normalize_message_paths(raw, root),
        "The specified path does not exist: ''."
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
fn test_parse_error_codes_ignores_bare_no_pos_diagnostics() {
    let output = "error TS2468: Cannot find global value 'Promise'.\n\
: error TS5057: Cannot find a tsconfig.json file at the specified directory: ''.\n\
test.ts(1,1): error TS2304: Cannot find name 'missing'.";

    assert_eq!(parse_error_codes_from_text(output), vec![5057, 2304]);
}

#[test]
fn test_parse_batch_output_does_not_synthesize_ts5110() {
    let output = "test.ts(1,1): error TS2304: Cannot find name 'missing'.";
    let options = HashMap::from([
        ("module".to_string(), "esnext".to_string()),
        ("moduleresolution".to_string(), "node16".to_string()),
    ]);
    let root = std::path::Path::new("/tmp/tsz-test");

    let result = parse_batch_output(output, root, options);

    assert_eq!(result.error_codes, vec![2304]);
    assert!(
        !result.error_codes.contains(&5110),
        "parse_batch_output should not inject synthetic TS5110"
    );
}

#[test]
fn test_parse_batch_output_preserves_typescript_builtin_lib_diagnostics() {
    let output = "TypeScript/lib/lib.dom.d.ts(13729,101): error TS2344: Type 'HTMLElementTagNameMap[K]' does not satisfy the constraint 'Element'.\n\
test.ts(1,1): error TS2304: Cannot find name 'missing'.";
    let root = std::path::Path::new("/tmp/tsz-test");

    let result = parse_batch_output(output, root, HashMap::new());

    let mut error_codes = result.error_codes.clone();
    error_codes.sort_unstable();
    assert_eq!(error_codes, vec![2304, 2344]);
    assert_eq!(result.diagnostic_fingerprints.len(), 2);
    assert!(result
        .diagnostic_fingerprints
        .iter()
        .any(|fp| fp.code == 2344));
    assert!(result
        .diagnostic_fingerprints
        .iter()
        .any(|fp| fp.code == 2304));
}

#[test]
fn test_parse_batch_output_retains_ts2430_alongside_other_codes() {
    // TS2430 is a retained code: all matching lines must survive parse_batch_output
    // alongside other codes like TS2304.
    let output = "test.ts(1,1): error TS2430: Interface 'I' incorrectly extends interface 'A'.\n\
test.ts(2,1): error TS2304: Cannot find name 'missing'.";
    let root = std::path::Path::new("/tmp/tsz-test");

    let result = parse_batch_output(output, root, HashMap::new());

    assert!(
        result.error_codes.contains(&2430),
        "TS2430 should be retained in error_codes"
    );
    assert!(
        result.error_codes.contains(&2304),
        "TS2304 should be retained in error_codes"
    );
    assert_eq!(result.diagnostic_fingerprints.len(), 2);
}

#[test]
fn test_parse_batch_output_retains_all_ts2430_lines_uniformly() {
    // TS2430 is retained regardless of which interface name appears in the message.
    let output = "test.ts(1,1): error TS2430: Interface 'I' incorrectly extends interface 'A'.\n\
test.ts(2,1): error TS2430: Interface 'Kept' incorrectly extends interface 'Base'.";
    let root = std::path::Path::new("/tmp/tsz-test");

    let result = parse_batch_output(output, root, HashMap::new());

    assert_eq!(result.diagnostic_fingerprints.len(), 2);
    assert_eq!(result.diagnostic_fingerprints[0].code, 2430);
    assert_eq!(result.diagnostic_fingerprints[0].line, 1);
    assert_eq!(result.diagnostic_fingerprints[1].code, 2430);
    assert_eq!(result.diagnostic_fingerprints[1].line, 2);
}

#[test]
fn test_parse_diagnostic_fingerprints_retains_ts2430_lines() {
    // TS2430 is a retained diagnostic code: fingerprints should include it.
    let output = "test.ts(1,1): error TS2430: Interface 'I' incorrectly extends interface 'A'.";
    let root = std::path::Path::new("/tmp/tsz-test");

    let fingerprints = parse_diagnostic_fingerprints_from_text(output, root);

    assert_eq!(
        fingerprints.len(),
        1,
        "TS2430 fingerprint should be retained"
    );
    assert_eq!(fingerprints[0].code, 2430);
}

#[test]
fn test_parse_tsz_output_does_not_synthesize_ts5110() {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    let output = std::process::Output {
        status: {
            #[cfg(unix)]
            {
                std::process::ExitStatus::from_raw(1 << 8)
            }
            #[cfg(windows)]
            {
                std::process::ExitStatus::from_raw(1)
            }
        },
        stdout: b"test.ts(1,1): error TS2304: Cannot find name 'missing'.\n".to_vec(),
        stderr: Vec::new(),
    };
    let options = HashMap::from([
        ("module".to_string(), "esnext".to_string()),
        ("moduleresolution".to_string(), "node16".to_string()),
    ]);
    let root = std::path::Path::new("/tmp/tsz-test");

    let result = parse_tsz_output(&output, root, options);

    assert_eq!(result.error_codes, vec![2304]);
    assert!(
        !result.error_codes.contains(&5110),
        "parse_tsz_output should not inject synthetic TS5110"
    );
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

#[test]
fn test_parse_batch_output_retains_bare_no_pos_diagnostics() {
    let root = std::path::Path::new("/tmp/tsz-test");
    let output = "error TS2468: Cannot find global value 'Promise'.";

    let result = parse_batch_output(output, root, HashMap::new());

    assert!(
        result.error_codes.is_empty(),
        "bare program-level diagnostics are compared as fingerprints, not code-list entries",
    );
    assert_eq!(result.diagnostic_fingerprints.len(), 1);
    let fp = &result.diagnostic_fingerprints[0];
    assert_eq!(fp.code, 2468);
    assert_eq!(fp.file, "");
    assert_eq!(fp.line, 0);
    assert_eq!(fp.column, 0);
    assert_eq!(fp.message_key, "Cannot find global value 'Promise'.");
}

#[test]
fn test_atypes_package_in_extracts_simple_package() {
    assert_eq!(
        atypes_package_in("/some/path/node_modules/@types/node/index.d.ts"),
        Some("node".to_string())
    );
    assert_eq!(
        atypes_package_in("node_modules/@types/node/index.d.ts"),
        Some("node".to_string())
    );
}

#[test]
fn test_atypes_package_in_extracts_scoped_package() {
    // tsc de-mangles `@scope/pkg` to `@types/scope__pkg` on disk.
    assert_eq!(
        atypes_package_in("/x/node_modules/@types/scope__pkg/index.d.ts"),
        Some("@scope/pkg".to_string())
    );
}

#[test]
fn test_atypes_package_in_returns_none_for_non_atypes_path() {
    assert_eq!(atypes_package_in("/foo/bar/baz.ts"), None);
    assert_eq!(atypes_package_in("node_modules/foo/index.d.ts"), None);
    assert_eq!(atypes_package_in(""), None);
}

#[test]
fn test_atypes_package_in_handles_subdir_paths() {
    // Sub-paths inside the @types package still resolve to the package name.
    assert_eq!(
        atypes_package_in("/p/node_modules/@types/node/fs/promises.d.ts"),
        Some("node".to_string())
    );
}

// ── normalize_file_not_found_message_key ──────────────────────────────────────

#[test]
fn test_normalize_file_not_found_message_key_handles_windows_backslashes() {
    // Triple-slash reference with Windows-style backslashes should normalize
    // to a forward-slash relative path.
    let msg = r"File '..\..\..\src\harness\external\mocha.d.ts' not found.";
    assert_eq!(
        normalize_file_not_found_message_key(msg),
        "File 'src/harness/external/mocha.d.ts' not found."
    );
}

#[test]
fn test_normalize_file_not_found_message_key_strips_macos_var_folders() {
    // Paths stored in the tsc cache on macOS include machine-specific
    // /var/folders/XX/ prefixes that should be stripped.
    // macOS CI temp dirs sit at /var/folders/XX/YYYY/T/test-ZZZ/. A reference
    // path with 3x ../ lands at /var/folders/XX/ (one hash component above the
    // meaningful path). The cache stores the resolved path with that one prefix.
    let msg = "File '/var/folders/6z/src/harness/external/mocha.d.ts' not found.";
    assert_eq!(
        normalize_file_not_found_message_key(msg),
        "File 'src/harness/external/mocha.d.ts' not found."
    );
}

#[test]
fn test_normalize_file_not_found_message_key_strips_private_var_folders() {
    // macOS resolves /var/... to /private/var/... via symlink.
    let msg = "File '/private/var/folders/6z/src/harness/external/mocha.d.ts' not found.";
    assert_eq!(
        normalize_file_not_found_message_key(msg),
        "File 'src/harness/external/mocha.d.ts' not found."
    );
}

#[test]
fn test_normalize_file_not_found_message_key_strips_leading_slash_on_linux() {
    // On Linux, an escaped temp path produces an absolute path at the filesystem
    // root like /src/harness/... (when temp dir is only 1-2 levels deep).
    let msg = "File '/src/harness/external/mocha.d.ts' not found.";
    assert_eq!(
        normalize_file_not_found_message_key(msg),
        "File 'src/harness/external/mocha.d.ts' not found."
    );
}

#[test]
fn test_normalize_file_not_found_message_key_strips_leading_dotdot() {
    // Relative paths with leading ../ should have those stripped.
    let msg = "File '../../../src/harness/external/mocha.d.ts' not found.";
    assert_eq!(
        normalize_file_not_found_message_key(msg),
        "File 'src/harness/external/mocha.d.ts' not found."
    );
}

#[test]
fn test_normalize_file_not_found_message_key_preserves_simple_relative_path() {
    // A simple relative path (no escaping) should be left unchanged.
    let msg = "File 'lib.d.ts' not found.";
    assert_eq!(normalize_file_not_found_message_key(msg), msg);
}

#[test]
fn test_normalize_file_not_found_message_key_preserves_project_relative_path() {
    // A relative path within the project should be left unchanged.
    let msg = "File 'src/utils.ts' not found.";
    assert_eq!(normalize_file_not_found_message_key(msg), msg);
}

#[test]
fn test_normalize_file_not_found_message_key_does_not_alter_non_file_not_found_messages() {
    // Only "File 'X' not found." patterns should be normalized; other messages untouched.
    let msg = "Cannot find name 'foo'.";
    assert_eq!(normalize_file_not_found_message_key(msg), msg);
}

#[test]
fn test_normalize_file_not_found_message_key_both_sides_converge() {
    // The Linux actual output and macOS-cache expected output should normalize
    // to the same canonical form, making fingerprint comparison succeed.
    // linux_actual: resolved from /tmp/xxx/ going 3 levels up → /src/harness/...
    // macos_cache:  as stored in the tsc CI cache (one hash component after /var/folders/)
    // backslash_actual: tsz output before the directive.rs backslash-normalization fix
    let linux_actual = "File '/src/harness/external/mocha.d.ts' not found.";
    let macos_cache = "File '/var/folders/6z/src/harness/external/mocha.d.ts' not found.";
    let backslash_actual = r"File '..\..\..\src\harness\external\mocha.d.ts' not found.";

    let canonical = "File 'src/harness/external/mocha.d.ts' not found.";
    assert_eq!(
        normalize_file_not_found_message_key(linux_actual),
        canonical
    );
    assert_eq!(normalize_file_not_found_message_key(macos_cache), canonical);
    assert_eq!(
        normalize_file_not_found_message_key(backslash_actual),
        canonical
    );
}

// ---------------------------------------------------------------------------
// Parity fingerprint catalog tests (#8286)
//
// Each catalog entry must:
//   - match the diagnostic shape it was created for,
//   - link to a real parity issue,
//   - have a one-sentence structural reason,
//   - drive the documented `parse_tsz_output` behavior end-to-end.
//
// The tests below pin each of those properties so a careless edit to
// `crates/conformance/src/parity/fingerprints.rs` fails CI before it can hide
// new divergences.
// ---------------------------------------------------------------------------

use crate::parity::fingerprints::{
    classify_parity, MatchScope, ParityAction, ParityFingerprintRule, KNOWN_PARITY_FINGERPRINTS,
};

fn classify_normalized(code: u32, message: &str) -> Option<&'static ParityFingerprintRule> {
    classify_parity(code, message, MatchScope::NormalizedMessage)
}

fn classify_raw(code: u32, line: &str) -> Option<&'static ParityFingerprintRule> {
    classify_parity(code, line, MatchScope::RawLine)
}

#[test]
fn parity_fingerprint_catalog_entries_all_link_to_parity_issues() {
    assert!(
        !KNOWN_PARITY_FINGERPRINTS.is_empty(),
        "catalog must not be empty while the wrapper still drops or remaps diagnostics"
    );
    for rule in KNOWN_PARITY_FINGERPRINTS {
        assert!(
            rule.parity_issue.number() > 0,
            "parity_issue for TS{} must be a non-zero tsz issue number",
            rule.code,
        );
        assert!(
            !rule.reason.is_empty(),
            "parity rule for TS{} (message {:?}) is missing a structural reason",
            rule.code,
            rule.message
        );
    }
}

#[test]
fn parity_fingerprint_catalog_drop_entries_resolve_to_expected_issue() {
    // (code, scope, text, expected parity issue number).
    // `NormalizedMessage` cases pass the bare normalized diagnostic message.
    let cases: &[(u32, MatchScope, &str, u32)] = &[
        (
            2322,
            MatchScope::NormalizedMessage,
            "Type '(number | (ValueOrArray<number>)[] | (number | (ValueOrArray<number>)[])[])[]' is not assignable to type 'ValueOrArray<number>'.",
            8423,
        ),
    ];

    for (code, scope, text, expected_issue) in cases {
        let rule = classify_parity(*code, text, *scope).unwrap_or_else(|| {
            panic!("no parity rule found for TS{code} ({scope:?}); text={text:?}")
        });
        assert!(
            matches!(rule.action, ParityAction::Drop),
            "TS{code} entry must be a Drop action, got {:?}",
            rule.action
        );
        assert_eq!(
            rule.parity_issue.number(),
            *expected_issue,
            "TS{code} entry must link to parity issue {expected_issue}",
        );
    }
}

#[test]
fn parity_fingerprint_catalog_passes_unrelated_diagnostics() {
    // A totally unrelated TS2322 must not be classified by the catalog.
    let unrelated = classify_normalized(2322, "Type 'string' is not assignable to type 'number'.");
    assert!(unrelated.is_none());

    // The classifier must be code-sensitive: matching the message text under
    // a different code is not a catalog hit.
    let wrong_code = classify_normalized(
        2345,
        "Type 'number | undefined' is not assignable to type 'number'.",
    );
    assert!(wrong_code.is_none());
}

#[test]
fn parity_fingerprint_catalog_line_classifier_is_code_sensitive() {
    // Exact catalog entries are normalized-message entries; raw diagnostic
    // lines must not classify even if they contain a catalog message.
    let line = "/tmp/test.ts(9,7): error TS2322: Type \
        '(number | (ValueOrArray<number>)[] | (number | (ValueOrArray<number>)[])[])[]' \
        is not assignable to type 'ValueOrArray<number>'.";
    assert!(classify_raw(2322, line).is_none());
}

#[test]
fn parity_fingerprint_catalog_drops_recursive_alias_fingerprint_end_to_end() {
    // Exact entries drop only from the fingerprint comparison surface. The raw
    // error-code list sees position-prefixed lines and remains unfiltered.
    let raw = "/tmp/test.ts(23,7): error TS2322: Type '(number | \
        (ValueOrArray<number>)[] | (number | (ValueOrArray<number>)[])[])[]' \
        is not assignable to type 'ValueOrArray<number>'.\n\
        /tmp/test.ts(99,1): error TS2322: Type 'string' is not assignable \
        to type 'number'.\n";
    let result = parse_batch_output(raw, Path::new("/tmp"), HashMap::new());

    assert_eq!(
        result.error_codes,
        vec![2322, 2322],
        "exact catalog entries do not filter raw error_codes",
    );

    let codes: Vec<u32> = result
        .diagnostic_fingerprints
        .iter()
        .map(|fp| fp.code)
        .collect();
    assert_eq!(
        codes,
        vec![2322],
        "recursive alias fingerprint was not dropped",
    );
    assert!(
        result.diagnostic_fingerprints[0]
            .message_key
            .contains("'string'"),
        "the unrelated TS2322 must survive parsing",
    );
}

#[test]
fn tsz_wrapper_has_no_ad_hoc_extra_fingerprint_helpers() {
    // Architecture guard: the catalog in `parity/fingerprints.rs` is the only
    // sanctioned place to hardcode fingerprint shapes. New ad-hoc
    // `is_extra_*` predicates in `tsz_wrapper.rs` recreate the §25 anti-pattern.
    //
    // Match the function name regardless of visibility (`fn`, `pub fn`,
    // `pub(crate) fn`, `pub(super) fn`, ...). The pattern is intentionally
    // permissive so visibility renames or attribute-prefixed forms still
    // trip the guard.
    let source = include_str!("../src/tsz_wrapper.rs");
    let needle = "fn is_extra_";
    let mut ad_hoc = Vec::new();
    for (start, _) in source.match_indices(needle) {
        // Require the preceding character to be whitespace or a visibility
        // marker so we don't match `is_extra_*` inside a doc string.
        let preceded_by_decl_boundary = start == 0
            || source[..start]
                .chars()
                .last()
                .is_some_and(|c| c.is_whitespace() || c == ')');
        if !preceded_by_decl_boundary {
            continue;
        }
        let rest = &source[start + needle.len()..];
        let name = rest
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");
        ad_hoc.push(format!("is_extra_{name}"));
    }
    assert!(
        ad_hoc.is_empty(),
        "ad-hoc parity suppressor helpers found in crates/conformance/src/tsz_wrapper.rs: {:?}\n\
         Add a `ParityFingerprintRule` entry to crates/conformance/src/parity/fingerprints.rs \
         instead and link the underlying parity issue.",
        ad_hoc,
    );
}
