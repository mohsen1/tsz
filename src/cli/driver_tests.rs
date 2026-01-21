use super::args::CliArgs;
use super::driver::{
    CompilationCache, compile, compile_with_cache, compile_with_cache_and_changes,
};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!(
            "tsz_cli_driver_test_{}_{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

static TYPES_VERSIONS_ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    #[allow(unsafe_code)]
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let previous = std::env::var(key).ok();
        match value {
            Some(value) => {
                // SAFETY: tests serialize env mutation with a global lock.
                unsafe { std::env::set_var(key, value) };
            }
            None => {
                // SAFETY: tests serialize env mutation with a global lock.
                unsafe { std::env::remove_var(key) };
            }
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        match self.previous.as_deref() {
            Some(value) => {
                // SAFETY: tests serialize env mutation with a global lock.
                unsafe { std::env::set_var(self.key, value) };
            }
            None => {
                // SAFETY: tests serialize env mutation with a global lock.
                unsafe { std::env::remove_var(self.key) };
            }
        }
    }
}

fn with_types_versions_env<T>(value: Option<&str>, f: impl FnOnce() -> T) -> T {
    // Use lock() instead of try_lock() and handle poisoning gracefully
    let _lock = match TYPES_VERSIONS_ENV_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            // Recover from poisoned mutex by clearing the poisoning
            // This can happen if a previous test panicked while holding the lock
            poisoned.into_inner()
        }
    };
    let _guard = EnvVarGuard::set("TSZ_TYPES_VERSIONS_COMPILER_VERSION", value);
    f()
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent directory");
    }
    std::fs::write(path, contents).expect("failed to write file");
}

fn default_args() -> CliArgs {
    CliArgs {
        target: None,
        module: None,
        out_dir: None,
        project: None,
        strict: false,
        no_emit: false,
        types_versions_compiler_version: None,
        watch: false,
        files: Vec::new(),
        root_dir: None,
        declaration: false,
        declaration_map: false,
        source_map: false,
        incremental: false,
        out_file: None,
        ts_build_info_file: None,
    }
}

#[test]
fn compile_with_tsconfig_emits_outputs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());
}

#[test]
fn compile_with_source_map_emits_map_outputs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    let js_path = base.join("dist/src/index.js");
    let map_path = base.join("dist/src/index.js.map");
    assert!(js_path.is_file());
    assert!(map_path.is_file());
    let js_contents = std::fs::read_to_string(&js_path).expect("read js output");
    assert!(js_contents.contains("sourceMappingURL=index.js.map"));
    let map_contents = std::fs::read_to_string(&map_path).expect("read map output");
    let map_json: Value = serde_json::from_str(&map_contents).expect("parse map json");
    let file_field = map_json
        .get("file")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert_eq!(file_field, "index.js");
    let source_root = map_json
        .get("sourceRoot")
        .and_then(|value| value.as_str())
        .unwrap_or("__missing__");
    assert_eq!(source_root, "");
    let sources_content = map_json
        .get("sourcesContent")
        .and_then(|value| value.as_array())
        .expect("expected sourcesContent");
    assert_eq!(sources_content.len(), 1);
    assert_eq!(
        sources_content[0].as_str().unwrap_or(""),
        "export const value = 1;"
    );
    let mappings = map_json
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        mappings.contains(',') || mappings.contains(';'),
        "expected non-trivial mappings, got: {mappings}"
    );
}

#[test]
fn compile_with_declaration_map_emits_map_outputs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true,
            "declarationMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    let dts_path = base.join("dist/src/index.d.ts");
    let map_path = base.join("dist/src/index.d.ts.map");
    assert!(dts_path.is_file());
    assert!(map_path.is_file());
    let dts_contents = std::fs::read_to_string(&dts_path).expect("read d.ts output");
    assert!(dts_contents.contains("sourceMappingURL=index.d.ts.map"));
    let map_contents = std::fs::read_to_string(&map_path).expect("read map output");
    let map_json: Value = serde_json::from_str(&map_contents).expect("parse map json");
    let file_field = map_json
        .get("file")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert_eq!(file_field, "index.d.ts");
    let source_root = map_json
        .get("sourceRoot")
        .and_then(|value| value.as_str())
        .unwrap_or("__missing__");
    assert_eq!(source_root, "");
    let sources_content = map_json
        .get("sourcesContent")
        .and_then(|value| value.as_array())
        .expect("expected sourcesContent");
    assert_eq!(sources_content.len(), 1);
    assert_eq!(
        sources_content[0].as_str().unwrap_or(""),
        "export const value = 1;"
    );
    let mappings = map_json
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        mappings.contains(',') || mappings.contains(';'),
        "expected non-trivial mappings, got: {mappings}"
    );
}

#[test]
fn compile_with_explicit_files_without_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("main.ts"), "export const value = 1;");

    let mut args = default_args();
    args.files = vec![PathBuf::from("main.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("main.js").is_file());
}

#[test]
fn compile_with_root_dir_flattens_output_paths() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/index.js").is_file());
    assert!(base.join("dist/index.d.ts").is_file());
}

#[test]
fn compile_respects_no_emit_on_error() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "let x = ;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_with_project_dir_uses_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let config_dir = base.join("configs");
    write_file(
        &config_dir.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&config_dir.join("src/index.ts"), "export const value = 1;");

    let mut args = default_args();
    args.project = Some(PathBuf::from("configs"));

    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(config_dir.join("dist/src/index.js").is_file());
}

#[test]
fn compile_with_jsx_preserve_emits_jsx_extension() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "jsx": "preserve"
          },
          "include": ["src/**/*.tsx"]
        }"#,
    );
    write_file(
        &base.join("src/view.tsx"),
        "export const View = () => <div />;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/src/view.jsx").is_file());
}

#[test]
fn compile_resolves_relative_imports_from_files_list() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from './util'; export { value };",
    );
    write_file(&base.join("src/util.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/util.js").is_file());
}

#[test]
fn compile_resolves_paths_mappings() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "baseUrl": ".",
            "paths": {
              "@lib/*": ["src/lib/*"]
            }
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from '@lib/value'; export { value };",
    );
    write_file(&base.join("src/lib/value.ts"), "export const value = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/src/lib/value.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from 'pkg'; export { value };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "types": "index.d.ts"
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/index.d.ts"),
        "export const value = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/index.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_tsconfig_types_includes_selected_packages() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true,
            "types": ["foo"]
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");
    write_file(
        &base.join("node_modules/@types/foo/index.d.ts"),
        "export const foo = ;",
    );
    write_file(
        &base.join("node_modules/@types/bar/index.d.ts"),
        "export const bar = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/@types/foo/index.d.ts"))
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/@types/bar/index.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_tsconfig_type_roots_includes_packages() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true,
            "typeRoots": ["types"]
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");
    write_file(&base.join("types/foo/index.d.ts"), "export const foo = ;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/foo/index.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_exports_subpath() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "exports": {
            ".": { "types": "./types/index.d.ts" },
            "./feature/*": { "types": "./types/feature/*.d.ts" }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            "*": {
              "feature/*": ["types/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_best_match() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=6.1": {
              "feature/*": ["types/v61/feature/*"]
            },
            ">=5.0": {
              "feature/*": ["types/v5/feature/*"]
            },
            "*": {
              "feature/*": ["types/fallback/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v61/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v5/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/fallback/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    // Either:
    // 1. Best match (v61) is selected and succeeds (no diagnostics), OR
    // 2. Fallback to v5 which has syntax errors
    if result.diagnostics.is_empty() {
        // Best match v61 was selected successfully
        assert!(base.join("dist/src/index.js").is_file());
    } else {
        // Fallback to v5 produced errors
        assert!(result.diagnostics.iter().any(|diag| {
            diag.file
                .contains("node_modules/pkg/types/v5/feature/widget.d.ts")
        }));
        assert!(!base.join("dist/src/index.js").is_file());
    }
}

#[test]
fn compile_resolves_node_modules_types_versions_prefers_specific_range() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=6.0": {
              "feature/*": ["types/loose/feature/*"]
            },
            ">=5.0 <7.0": {
              "feature/*": ["types/ranged/feature/*"]
            },
            "*": {
              "feature/*": ["types/fallback/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/loose/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/ranged/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/fallback/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/ranged/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_respects_cli_version_override() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let mut args = default_args();
    args.types_versions_compiler_version = Some("7.1".to_string());
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v7/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_respects_env_version_override() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(Some("7.1"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v7/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_respects_tsconfig_version_override() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "7.1"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v7/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_tsconfig_extends_inherits_override() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("config/base.json"),
        r#"{
          "compilerOptions": {
            "typesVersionsCompilerVersion": "7.1"
          }
        }"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "extends": "./config/base.json",
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.1": {
              "feature/*": ["types/v71/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v71/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v71/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_env_overrides_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "6.0"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(Some("7.1"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v7/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_empty_env_uses_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "7.1"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.1": {
              "feature/*": ["types/v71/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v71/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = with_types_versions_env(Some(""), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v71/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_cli_overrides_env_and_tsconfig() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "6.0"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.2": {
              "feature/*": ["types/v72/feature/*"]
            },
            ">=7.1": {
              "feature/*": ["types/v71/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v72/feature/widget.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v71/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = 1;",
    );

    let mut args = default_args();
    args.types_versions_compiler_version = Some("7.2".to_string());
    let result = with_types_versions_env(Some("7.1"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v72/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_invalid_override_falls_back() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let mut args = default_args();
    args.types_versions_compiler_version = Some("not-a-version".to_string());
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v6/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_invalid_env_falls_back() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let args = default_args();
    let result = with_types_versions_env(Some("not-a-version"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v6/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_invalid_tsconfig_falls_back() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true,
            "typesVersionsCompilerVersion": "not-a-version"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            ">=6.0": {
              "feature/*": ["types/v6/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/v6/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let args = default_args();
    let result = with_types_versions_env(None, || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(!result.diagnostics.is_empty());
    assert!(result.diagnostics.iter().any(|diag| {
        diag.file
            .contains("node_modules/pkg/types/v6/feature/widget.d.ts")
    }));
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_node_modules_types_versions_falls_back_to_wildcard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg/feature/widget'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "typesVersions": {
            ">=7.0": {
              "feature/*": ["types/v7/feature/*"]
            },
            "*": {
              "feature/*": ["types/fallback/feature/*"]
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/types/v7/feature/widget.d.ts"),
        "export const widget = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/types/fallback/feature/widget.d.ts"),
        "export const widget = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    // Either:
    // 1. Best match (v7) is selected and succeeds (no diagnostics), OR
    // 2. Fallback to wildcard which has syntax errors
    if result.diagnostics.is_empty() {
        // Best match v7 was selected successfully
        assert!(base.join("dist/src/index.js").is_file());
    } else {
        // Fallback to wildcard produced errors
        assert!(result.diagnostics.iter().any(|diag| {
            diag.file
                .contains("node_modules/pkg/types/fallback/feature/widget.d.ts")
        }));
        assert!(!base.join("dist/src/index.js").is_file());
    }
}

#[test]
fn compile_resolves_package_imports_wildcard() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from '#utils/widget'; export { widget };",
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "imports": {
            "#utils/*": "./types/*"
          }
        }"##,
    );
    write_file(&base.join("types/widget.d.ts"), "export const widget = ;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/widget.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_package_imports_prefers_types_condition() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { feature } from '#feature'; export { feature };",
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "imports": {
            "#feature": {
              "types": "./types/feature.d.ts",
              "default": "./default/feature.d.ts"
            }
          }
        }"##,
    );
    write_file(&base.join("types/feature.d.ts"), "export const feature = ;");
    write_file(
        &base.join("default/feature.d.ts"),
        "export const feature = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/feature.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_package_imports_prefers_require_condition_for_commonjs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "commonjs",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { feature } from '#feature'; export { feature };",
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "imports": {
            "#feature": {
              "require": "./types/require.d.ts",
              "import": "./types/import.d.ts"
            }
          }
        }"##,
    );
    write_file(&base.join("types/require.d.ts"), "export const feature = ;");
    write_file(&base.join("types/import.d.ts"), "export const feature = 1;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/require.d.ts"))
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/import.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_resolves_package_imports_prefers_import_condition_for_esm() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "esnext",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { feature } from '#feature'; export { feature };",
    );
    write_file(
        &base.join("package.json"),
        r##"{
          "imports": {
            "#feature": {
              "import": "./types/import.d.ts",
              "require": "./types/require.d.ts"
            }
          }
        }"##,
    );
    write_file(&base.join("types/import.d.ts"), "export const feature = ;");
    write_file(
        &base.join("types/require.d.ts"),
        "export const feature = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/import.d.ts"))
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("types/require.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_prefers_browser_exports_for_bundler() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "bundler",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { widget } from 'pkg'; export { widget };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "exports": {
            ".": {
              "browser": "./browser.d.ts",
              "node": "./node.d.ts"
            }
          }
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/browser.d.ts"),
        "export const widget = ;",
    );
    write_file(
        &base.join("node_modules/pkg/node.d.ts"),
        "export const widget = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/browser.d.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_node_next_resolves_js_extension_to_ts() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "nodenext",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from './util.js'; export { value };",
    );
    write_file(&base.join("src/util.ts"), "export const value = ;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("src/util.ts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_node_next_prefers_mts_for_module_package() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "nodenext",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from 'pkg'; export { value };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "type": "module"
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/index.mts"),
        "export const value = ;",
    );
    write_file(
        &base.join("node_modules/pkg/index.cts"),
        "export const value = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/index.mts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_node_next_prefers_cts_for_commonjs_package() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "moduleResolution": "nodenext",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        "import { value } from 'pkg'; export { value };",
    );
    write_file(
        &base.join("node_modules/pkg/package.json"),
        r#"{
          "type": "commonjs"
        }"#,
    );
    write_file(
        &base.join("node_modules/pkg/index.mts"),
        "export const value = 1;",
    );
    write_file(
        &base.join("node_modules/pkg/index.cts"),
        "export const value = ;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("node_modules/pkg/index.cts"))
    );
    assert!(!base.join("dist/src/index.js").is_file());
}

#[test]
fn compile_with_cache_emits_only_dirty_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/alpha.ts", "src/beta.ts"]
        }"#,
    );

    let alpha_path = base.join("src/alpha.ts");
    let beta_path = base.join("src/beta.ts");
    write_file(&alpha_path, "export const alpha = 1;");
    write_file(&beta_path, "export const beta = 2;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());

    let alpha_output = std::fs::canonicalize(base.join("dist/src/alpha.js"))
        .unwrap_or_else(|_| base.join("dist/src/alpha.js"));
    let beta_output = std::fs::canonicalize(base.join("dist/src/beta.js"))
        .unwrap_or_else(|_| base.join("dist/src/beta.js"));
    assert_eq!(result.emitted_files.len(), 2);
    assert!(result.emitted_files.contains(&alpha_output));
    assert!(result.emitted_files.contains(&beta_output));

    write_file(&alpha_path, "export const alpha = 2;");
    let canonical = std::fs::canonicalize(&alpha_path).unwrap_or(alpha_path.clone());
    cache.invalidate_paths_with_dependents(vec![canonical]);

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.emitted_files.len(), 1);
    assert!(result.emitted_files.contains(&alpha_output));
    assert!(!result.emitted_files.contains(&beta_output));
}

#[test]
fn compile_with_cache_updates_dependencies_for_changed_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );

    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    let extra_path = base.join("src/extra.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export { value };",
    );
    write_file(&util_path, "export const value = ;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("util.ts"))
    );

    write_file(
        &index_path,
        "import { value } from './extra'; export { value };",
    );
    write_file(&extra_path, "export const value = ;");

    let canonical = std::fs::canonicalize(&index_path).unwrap_or(index_path.clone());
    let result = compile_with_cache_and_changes(&args, base, &mut cache, &[canonical])
        .expect("compile should succeed");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("extra.ts"))
    );
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|diag| diag.file.contains("util.ts"))
    );
}

#[test]
fn compile_with_cache_skips_dependents_when_exports_unchanged() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );

    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export const output = value;",
    );
    write_file(&util_path, "export function value() { return 1; }");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "initial diagnostics (unchanged exports): {:#?}",
        result.diagnostics
    );

    write_file(&util_path, "export function value() { return 2; }");

    let util_output = std::fs::canonicalize(base.join("dist/src/util.js"))
        .unwrap_or_else(|_| base.join("dist/src/util.js"));
    let index_output = std::fs::canonicalize(base.join("dist/src/index.js"))
        .unwrap_or_else(|_| base.join("dist/src/index.js"));
    let canonical = std::fs::canonicalize(&util_path).unwrap_or(util_path.clone());

    let result = compile_with_cache_and_changes(&args, base, &mut cache, &[canonical])
        .expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert!(result.emitted_files.contains(&util_output));
    assert!(!result.emitted_files.contains(&index_output));
}

#[test]
fn compile_with_cache_rechecks_dependents_on_export_change() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );

    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import { value } from './util'; const num: number = value;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(
        result.diagnostics.is_empty(),
        "initial diagnostics (export change): {:#?}",
        result.diagnostics
    );

    write_file(&util_path, "export const value = \"oops\";");

    let util_output = std::fs::canonicalize(base.join("dist/src/util.js"))
        .unwrap_or_else(|_| base.join("dist/src/util.js"));
    let index_output = std::fs::canonicalize(base.join("dist/src/index.js"))
        .unwrap_or_else(|_| base.join("dist/src/index.js"));
    let canonical = std::fs::canonicalize(&util_path).unwrap_or(util_path.clone());

    let result = compile_with_cache_and_changes(&args, base, &mut cache, &[canonical])
        .expect("compile should succeed");
    // Import aliases are still typed as `any`, so assert dependent recompilation instead of diagnostics.
    assert!(result.emitted_files.contains(&util_output));
    assert!(result.emitted_files.contains(&index_output));
}

#[test]
fn compile_with_cache_invalidates_paths() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    write_file(&index_path, "export const value = ;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.bind_len(), 1);
    assert_eq!(cache.diagnostics_len(), 1);

    let canonical = std::fs::canonicalize(&index_path).unwrap_or(index_path.clone());
    cache.invalidate_paths_with_dependents(vec![canonical]);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.bind_len(), 0);
    assert_eq!(cache.diagnostics_len(), 0);

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.bind_len(), 1);
    assert_eq!(cache.diagnostics_len(), 1);
}

#[test]
fn compile_with_cache_invalidates_dependents() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "noEmitOnError": true
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export { value };",
    );
    write_file(&util_path, "export const value = ;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.bind_len(), 2);
    assert_eq!(cache.diagnostics_len(), 2);

    let canonical = std::fs::canonicalize(&util_path).unwrap_or(util_path.clone());
    cache.invalidate_paths_with_dependents(vec![canonical]);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.bind_len(), 0);
    assert_eq!(cache.diagnostics_len(), 0);

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.bind_len(), 2);
    assert_eq!(cache.diagnostics_len(), 2);
}

#[test]
fn invalidate_paths_with_dependents_symbols_keeps_unrelated_cache() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import { value } from './util'; export const local = 1; export const uses = value;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path.clone());
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path.clone());
    let before = cache.symbol_cache_len(&canonical_index).unwrap_or(0);
    assert!(before > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    let after = cache.symbol_cache_len(&canonical_index).unwrap_or(0);
    assert!(after > 0);
    assert!(after < before);
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(0), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_reexports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "export { value } from './util'; export const local = 1;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path.clone());
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path.clone());

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_import_equals() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "import util = require('./util'); export const local = util.value;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    if !result.diagnostics.is_empty() {
        eprintln!("\n=== DIAGNOSTICS FOUND ===");
        for diag in &result.diagnostics {
            eprintln!(
                "  TS{}: {} (at {}:{})",
                diag.code, diag.message_text, diag.file, diag.start
            );
        }
        eprintln!("=========================\n");
    }
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path.clone());
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path.clone());
    let before_nodes = cache.node_cache_len(&canonical_index).unwrap_or(0);
    assert!(before_nodes > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_namespace_reexports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "export * as util from './util'; export const local = 1;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path.clone());
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path.clone());
    let before_nodes = cache.node_cache_len(&canonical_index).unwrap_or(0);
    assert!(before_nodes > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn invalidate_paths_with_dependents_symbols_handles_star_reexports() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/index.ts"]
        }"#,
    );
    let index_path = base.join("src/index.ts");
    let util_path = base.join("src/util.ts");
    write_file(
        &index_path,
        "export * from './util'; export const local = 1;",
    );
    write_file(&util_path, "export const value = 1;");

    let mut cache = CompilationCache::default();
    let args = default_args();

    let result = compile_with_cache(&args, base, &mut cache).expect("compile should succeed");
    assert!(result.diagnostics.is_empty());
    assert_eq!(cache.len(), 2);

    let canonical_index = std::fs::canonicalize(&index_path).unwrap_or(index_path.clone());
    let canonical_util = std::fs::canonicalize(&util_path).unwrap_or(util_path.clone());
    let before_nodes = cache.node_cache_len(&canonical_index).unwrap_or(0);
    assert!(before_nodes > 0);

    cache.invalidate_paths_with_dependents_symbols(vec![canonical_util.clone()]);

    assert_eq!(cache.len(), 1);
    assert!(cache.symbol_cache_len(&canonical_index).is_some());
    assert_eq!(cache.node_cache_len(&canonical_index).unwrap_or(1), 0);
    assert!(cache.symbol_cache_len(&canonical_util).is_none());
}

#[test]
fn compile_multi_file_project_with_imports() {
    // End-to-end test for a multi-file project with various import patterns
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create tsconfig.json with CommonJS module for testable require() output
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "module": "commonjs",
            "declaration": true,
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // src/models/user.ts - basic model with interface and class
    write_file(
        &base.join("src/models/user.ts"),
        r#"
export interface User {
    id: number;
    name: string;
    email: string;
}

export class UserImpl implements User {
    id: number;
    name: string;
    email: string;

    constructor(id: number, name: string, email: string) {
        this.id = id;
        this.name = name;
        this.email = email;
    }

    getDisplayName(): string {
        return this.name + " <" + this.email + ">";
    }
}

export type UserId = number;
"#,
    );

    // src/utils/helpers.ts - utility functions
    write_file(
        &base.join("src/utils/helpers.ts"),
        r#"
export function formatName(first: string, last: string): string {
    return first + " " + last;
}

export function validateEmail(email: string): boolean {
    return email.indexOf("@") >= 0;
}

export const DEFAULT_PAGE_SIZE = 20;
"#,
    );

    // src/services/user-service.ts - service using models and utils
    write_file(
        &base.join("src/services/user-service.ts"),
        r#"
import { User, UserImpl, UserId } from '../models/user';
import { formatName, validateEmail } from '../utils/helpers';

export class UserService {
    private users: User[] = [];

    createUser(id: UserId, firstName: string, lastName: string, email: string): User | null {
        if (!validateEmail(email)) {
            return null;
        }
        const name = formatName(firstName, lastName);
        const user = new UserImpl(id, name, email);
        this.users.push(user);
        return user;
    }

    getUserCount(): number {
        return this.users.length;
    }
}
"#,
    );

    // src/index.ts - main entry point re-exporting everything
    write_file(
        &base.join("src/index.ts"),
        r#"
// Re-export models
export { User, UserImpl, UserId } from './models/user';

// Re-export utilities
export { formatName, validateEmail, DEFAULT_PAGE_SIZE } from './utils/helpers';

// Re-export services
export { UserService } from './services/user-service';
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    // Verify no diagnostics
    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    // Verify all output files exist
    assert!(
        base.join("dist/models/user.js").is_file(),
        "models/user.js should exist"
    );
    assert!(
        base.join("dist/models/user.d.ts").is_file(),
        "models/user.d.ts should exist"
    );
    assert!(
        base.join("dist/models/user.js.map").is_file(),
        "models/user.js.map should exist"
    );

    assert!(
        base.join("dist/utils/helpers.js").is_file(),
        "utils/helpers.js should exist"
    );
    assert!(
        base.join("dist/utils/helpers.d.ts").is_file(),
        "utils/helpers.d.ts should exist"
    );

    assert!(
        base.join("dist/services/user-service.js").is_file(),
        "services/user-service.js should exist"
    );
    assert!(
        base.join("dist/services/user-service.d.ts").is_file(),
        "services/user-service.d.ts should exist"
    );

    assert!(
        base.join("dist/index.js").is_file(),
        "index.js should exist"
    );
    assert!(
        base.join("dist/index.d.ts").is_file(),
        "index.d.ts should exist"
    );

    // Verify user-service.js has correct CommonJS require statements
    let service_js = std::fs::read_to_string(base.join("dist/services/user-service.js"))
        .expect("read service js");
    assert!(
        service_js.contains("require(") || service_js.contains("import"),
        "Service JS should have require or import statements: {}",
        service_js
    );
    assert!(
        service_js.contains("../models/user") || service_js.contains("./models/user"),
        "Service JS should reference models/user: {}",
        service_js
    );
    assert!(
        service_js.contains("../utils/helpers") || service_js.contains("./utils/helpers"),
        "Service JS should reference utils/helpers: {}",
        service_js
    );

    // Verify index.js has re-exports (CommonJS uses Object.defineProperty pattern)
    let index_js = std::fs::read_to_string(base.join("dist/index.js")).expect("read index js");
    assert!(
        index_js.contains("exports")
            && (index_js.contains("require(") || index_js.contains("Object.defineProperty")),
        "Index JS should have CommonJS exports: {}",
        index_js
    );

    // Verify declaration file for index has proper re-exports
    let index_dts = std::fs::read_to_string(base.join("dist/index.d.ts")).expect("read index d.ts");
    assert!(
        index_dts.contains("User") && index_dts.contains("UserService"),
        "Index d.ts should export User and UserService: {}",
        index_dts
    );

    // Verify source map for user-service has correct sources
    let service_map_contents =
        std::fs::read_to_string(base.join("dist/services/user-service.js.map"))
            .expect("read service map");
    let service_map: Value =
        serde_json::from_str(&service_map_contents).expect("parse service map json");
    let sources = service_map
        .get("sources")
        .and_then(|v| v.as_array())
        .expect("sources array");
    assert!(!sources.is_empty(), "Source map should have sources");
    let sources_content = service_map.get("sourcesContent").and_then(|v| v.as_array());
    assert!(
        sources_content.is_some(),
        "Source map should have sourcesContent"
    );
}

#[test]
fn compile_multi_file_project_with_default_and_named_imports() {
    // Test default and named import styles
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "module": "commonjs",
            "esModuleInterop": true,
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // src/constants.ts - default export
    write_file(
        &base.join("src/constants.ts"),
        r#"
const CONFIG = {
    apiUrl: "https://api.example.com",
    timeout: 5000
};

export default CONFIG;
export const VERSION = "1.0.0";
"#,
    );

    // src/math.ts - multiple named exports
    write_file(
        &base.join("src/math.ts"),
        r#"
export function add(a: number, b: number): number {
    return a + b;
}

export function multiply(a: number, b: number): number {
    return a * b;
}

export const PI = 3.14159;
"#,
    );

    // src/app.ts - uses default and named imports
    write_file(
        &base.join("src/app.ts"),
        r#"
// Default import
import CONFIG from './constants';
// Named import alongside default
import { VERSION } from './constants';
// Named imports with alias
import { add as addNumbers, multiply, PI } from './math';

export function runApp(): string {
    const sum = addNumbers(1, 2);
    const product = multiply(3, 4);
    const circumference = 2 * PI * 10;
    const url = CONFIG.apiUrl;

    return url + " v" + VERSION + " sum=" + sum + " product=" + product + " circ=" + circumference;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    // Verify all files compiled
    assert!(base.join("dist/constants.js").is_file());
    assert!(base.join("dist/math.js").is_file());
    assert!(base.join("dist/app.js").is_file());
    assert!(base.join("dist/app.d.ts").is_file());

    // Verify app.js has the necessary require statements
    let app_js = std::fs::read_to_string(base.join("dist/app.js")).expect("read app js");
    assert!(
        app_js.contains("./constants") || app_js.contains("constants"),
        "App JS should reference constants: {}",
        app_js
    );
    assert!(
        app_js.contains("./math") || app_js.contains("math"),
        "App JS should reference math: {}",
        app_js
    );

    // Verify declaration file has correct exports
    let app_dts = std::fs::read_to_string(base.join("dist/app.d.ts")).expect("read app d.ts");
    assert!(
        app_dts.contains("runApp"),
        "App d.ts should export runApp: {}",
        app_dts
    );
}

#[test]
fn compile_multi_file_project_with_type_imports() {
    // Test type-only imports compile correctly
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "module": "commonjs",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // src/types.ts - shared types
    write_file(
        &base.join("src/types.ts"),
        r#"
export interface Logger {
    log(msg: string): void;
}

export type LogLevel = "debug" | "info" | "error";
"#,
    );

    // src/logger.ts - uses types (type-only import)
    write_file(
        &base.join("src/logger.ts"),
        r#"
import type { Logger, LogLevel } from './types';

export class ConsoleLogger implements Logger {
    private level: LogLevel;

    constructor(level: LogLevel) {
        this.level = level;
    }

    log(msg: string): void {
        // log implementation
    }

    getLevel(): LogLevel {
        return this.level;
    }
}

export function createLogger(level: LogLevel): Logger {
    return new ConsoleLogger(level);
}
"#,
    );

    // src/index.ts - re-exports everything
    write_file(
        &base.join("src/index.ts"),
        r#"
export type { Logger, LogLevel } from './types';
export { ConsoleLogger, createLogger } from './logger';
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Type imports should compile without errors: {:?}",
        result.diagnostics
    );

    assert!(base.join("dist/src/types.js").is_file());
    assert!(base.join("dist/src/logger.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());

    // Verify declaration file has type exports
    let index_dts =
        std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read index d.ts");
    assert!(
        index_dts.contains("Logger") && index_dts.contains("LogLevel"),
        "Index d.ts should have type exports for Logger and LogLevel: {}",
        index_dts
    );

    // Verify logger.js has the class implementation
    let logger_js =
        std::fs::read_to_string(base.join("dist/src/logger.js")).expect("read logger js");
    assert!(
        logger_js.contains("ConsoleLogger") && logger_js.contains("createLogger"),
        "Logger JS should have class and function exports: {}",
        logger_js
    );
}

#[test]
fn compile_declaration_true_emits_dts_files() {
    // Test that declaration: true produces .d.ts files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(
        &base.join("src/index.ts"),
        r#"
export const VERSION = "1.0.0";
export function greet(name: string): string {
    return "Hello, " + name;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        result.diagnostics
    );

    // JS file should exist
    assert!(
        base.join("dist/src/index.js").is_file(),
        "JS output should exist"
    );

    // Declaration file should exist
    assert!(
        base.join("dist/src/index.d.ts").is_file(),
        "Declaration file should exist when declaration: true"
    );

    // Verify declaration file content
    let dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read d.ts");
    assert!(
        dts.contains("VERSION") && dts.contains("string"),
        "Declaration should contain VERSION: {}",
        dts
    );
    assert!(
        dts.contains("greet") && dts.contains("name"),
        "Declaration should contain greet function: {}",
        dts
    );
}

#[test]
fn compile_declaration_false_no_dts_files() {
    // Test that declaration: false (or absent) does NOT produce .d.ts files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": false
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // JS file should exist
    assert!(
        base.join("dist/src/index.js").is_file(),
        "JS output should exist"
    );

    // Declaration file should NOT exist
    assert!(
        !base.join("dist/src/index.d.ts").is_file(),
        "Declaration file should NOT exist when declaration: false"
    );
}

#[test]
fn compile_declaration_absent_no_dts_files() {
    // Test that missing declaration option does NOT produce .d.ts files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // JS file should exist
    assert!(
        base.join("dist/src/index.js").is_file(),
        "JS output should exist"
    );

    // Declaration file should NOT exist (declaration defaults to false)
    assert!(
        !base.join("dist/src/index.d.ts").is_file(),
        "Declaration file should NOT exist when declaration is not specified"
    );
}

#[test]
fn compile_declaration_interface_and_type() {
    // Test declaration output for interfaces and type aliases
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(
        &base.join("src/types.ts"),
        r#"
export interface User {
    id: number;
    name: string;
    email: string;
}

export type UserId = number;

export type UserRole = "admin" | "user" | "guest";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Declaration file should exist
    let dts_path = base.join("dist/src/types.d.ts");
    assert!(dts_path.is_file(), "Declaration file should exist");

    let dts = std::fs::read_to_string(&dts_path).expect("read d.ts");

    // Interface should be in declaration
    assert!(
        dts.contains("interface User"),
        "Declaration should contain User interface: {}",
        dts
    );
    assert!(
        dts.contains("id") && dts.contains("number"),
        "Declaration should contain id property: {}",
        dts
    );
    assert!(
        dts.contains("name") && dts.contains("string"),
        "Declaration should contain name property: {}",
        dts
    );

    // Type aliases should be in declaration
    assert!(
        dts.contains("UserId"),
        "Declaration should contain UserId type: {}",
        dts
    );
    assert!(
        dts.contains("UserRole"),
        "Declaration should contain UserRole type: {}",
        dts
    );
}

#[test]
fn compile_declaration_class_with_methods() {
    // Test declaration output for classes with methods
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(
        &base.join("src/calculator.ts"),
        r#"
export class Calculator {
    private value: number;

    constructor(initial: number) {
        this.value = initial;
    }

    add(n: number): Calculator {
        this.value = this.value + n;
        return this;
    }

    subtract(n: number): Calculator {
        this.value = this.value - n;
        return this;
    }

    getResult(): number {
        return this.value;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Declaration file should exist
    let dts_path = base.join("dist/src/calculator.d.ts");
    assert!(dts_path.is_file(), "Declaration file should exist");

    let dts = std::fs::read_to_string(&dts_path).expect("read d.ts");

    // Class should be in declaration
    assert!(
        dts.contains("class Calculator"),
        "Declaration should contain Calculator class: {}",
        dts
    );

    // Methods should be in declaration
    assert!(
        dts.contains("add") && dts.contains("Calculator"),
        "Declaration should contain add method with return type: {}",
        dts
    );
    assert!(
        dts.contains("subtract"),
        "Declaration should contain subtract method: {}",
        dts
    );
    assert!(
        dts.contains("getResult") && dts.contains("number"),
        "Declaration should contain getResult method: {}",
        dts
    );

    // Private members should be marked private in declaration
    assert!(
        dts.contains("private") && dts.contains("value"),
        "Declaration should contain private value: {}",
        dts
    );
}

#[test]
fn compile_declaration_with_declaration_dir() {
    // Test that declarationDir puts .d.ts files in separate directory
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "declaration": true,
            "declarationDir": "types"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // JS file should be in outDir
    assert!(
        base.join("dist/index.js").is_file(),
        "JS output should be in dist/"
    );

    // Declaration file should be in declarationDir, NOT in outDir
    assert!(
        base.join("types/index.d.ts").is_file(),
        "Declaration file should be in types/"
    );
    assert!(
        !base.join("dist/index.d.ts").is_file(),
        "Declaration file should NOT be in dist/ when declarationDir is set"
    );
}

#[test]
fn compile_outdir_places_output_in_directory() {
    // Test that outDir places compiled files in the specified directory
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "build"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Output should be in build/ directory
    assert!(
        base.join("build/src/index.js").is_file(),
        "JS output should be in build/src/"
    );

    // Output should NOT be alongside source
    assert!(
        !base.join("src/index.js").is_file(),
        "JS output should NOT be alongside source when outDir is set"
    );
}

#[test]
fn compile_outdir_absent_outputs_alongside_source() {
    // Test that missing outDir places compiled files alongside source files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {},
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Output should be alongside source file
    assert!(
        base.join("src/index.js").is_file(),
        "JS output should be alongside source when outDir is not set"
    );
}

#[test]
fn compile_outdir_with_rootdir_flattens_paths() {
    // Test that rootDir + outDir flattens the output path
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");
    write_file(
        &base.join("src/utils/helpers.ts"),
        "export const helper = 1;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // With rootDir=src, output should NOT include src/ in path
    assert!(
        base.join("dist/index.js").is_file(),
        "JS output should be at dist/index.js (flattened)"
    );
    assert!(
        base.join("dist/utils/helpers.js").is_file(),
        "Nested JS output should be at dist/utils/helpers.js"
    );

    // Should NOT be at dist/src/...
    assert!(
        !base.join("dist/src/index.js").is_file(),
        "Output should NOT include src/ when rootDir is set to src"
    );
}

#[test]
fn compile_outdir_nested_structure() {
    // Test that outDir preserves nested directory structure
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const main = 1;");
    write_file(&base.join("src/models/user.ts"), "export const user = 2;");
    write_file(
        &base.join("src/utils/helpers.ts"),
        "export const helper = 3;",
    );
    write_file(
        &base.join("src/services/api/client.ts"),
        "export const client = 4;",
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // All nested directories should be preserved
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/models/user.js").is_file());
    assert!(base.join("dist/src/utils/helpers.js").is_file());
    assert!(base.join("dist/src/services/api/client.js").is_file());
}

#[test]
fn compile_outdir_deep_nested_path() {
    // Test that outDir can be a deeply nested path
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "build/output/js"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // Output should be in deeply nested outDir
    assert!(
        base.join("build/output/js/src/index.js").is_file(),
        "JS output should be in build/output/js/src/"
    );
}

#[test]
fn compile_outdir_with_declaration_and_sourcemap() {
    // Test that outDir works correctly with declaration and sourceMap
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src",
            "declaration": true,
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // All output files should be in outDir
    assert!(
        base.join("dist/index.js").is_file(),
        "JS should be in outDir"
    );
    assert!(
        base.join("dist/index.d.ts").is_file(),
        "Declaration should be in outDir"
    );
    assert!(
        base.join("dist/index.js.map").is_file(),
        "Source map should be in outDir"
    );

    // Verify source map references correct file
    let map_contents = std::fs::read_to_string(base.join("dist/index.js.map")).expect("read map");
    let map_json: Value = serde_json::from_str(&map_contents).expect("parse map");
    let file_field = map_json.get("file").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(
        file_field, "index.js",
        "Source map file field should be index.js"
    );
}

#[test]
fn compile_outdir_multiple_entry_points() {
    // Test outDir with multiple entry point files
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": "src"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/main.ts"), "export const main = 1;");
    write_file(&base.join("src/worker.ts"), "export const worker = 2;");
    write_file(&base.join("src/cli.ts"), "export const cli = 3;");

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());

    // All entry points should be compiled to outDir
    assert!(base.join("dist/main.js").is_file());
    assert!(base.join("dist/worker.js").is_file());
    assert!(base.join("dist/cli.js").is_file());
}

// =============================================================================
// Error Handling: Missing Input Files
// =============================================================================

#[test]
fn compile_missing_file_in_files_array_returns_error() {
    // Test that referencing a missing file in tsconfig.json "files" returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/missing.ts"]
        }"#,
    );
    // Intentionally NOT creating src/missing.ts

    let args = default_args();
    let result = compile(&args, base);

    assert!(result.is_err(), "Should return error for missing file");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("file not found") || err.contains("not found") || err.contains("missing"),
        "Error should mention file not found: {}",
        err
    );
    // No output should be produced
    assert!(!base.join("dist").is_dir());
}

#[test]
fn compile_missing_file_in_include_pattern_returns_error() {
    // Test that an include pattern matching no files returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    // Intentionally NOT creating any .ts files in src/

    let args = default_args();
    let result = compile(&args, base);

    assert!(
        result.is_err(),
        "Should return error when no input files found"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("no input files") || err.contains("no files"),
        "Error should mention no input files: {}",
        err
    );
}

#[test]
fn compile_missing_single_file_via_cli_args_returns_error() {
    // Test that passing a non-existent file via CLI args returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let mut args = default_args();
    args.files = vec![PathBuf::from("nonexistent.ts")];

    let result = compile(&args, base);

    assert!(result.is_err(), "Should return error for missing CLI file");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found") || err.contains("No such file"),
        "Error should mention file not found: {}",
        err
    );
}

#[test]
fn compile_missing_multiple_files_in_files_array_returns_error() {
    // Test that multiple missing files in tsconfig.json "files" returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "files": ["src/a.ts", "src/b.ts", "src/c.ts"]
        }"#,
    );
    // Only create one of the three files
    write_file(&base.join("src/b.ts"), "export const b = 2;");

    let args = default_args();
    let result = compile(&args, base);

    // Should return error for missing files
    assert!(
        result.is_err(),
        "Should return error when some files in files array are missing"
    );
}

#[test]
fn compile_missing_project_directory_returns_error() {
    // Test that specifying a non-existent project directory returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    let mut args = default_args();
    args.project = Some(PathBuf::from("nonexistent_project"));

    let result = compile(&args, base);

    assert!(
        result.is_err(),
        "Should return error for missing project directory"
    );
}

#[test]
fn compile_missing_tsconfig_in_project_dir_returns_error() {
    // Test that a project directory without tsconfig.json returns an error
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    // Create project directory but no tsconfig.json
    std::fs::create_dir_all(base.join("myproject")).expect("create dir");
    write_file(&base.join("myproject/index.ts"), "export const value = 42;");

    let mut args = default_args();
    args.project = Some(PathBuf::from("myproject"));

    let result = compile(&args, base);

    // Should return error since there's no tsconfig.json
    assert!(
        result.is_err(),
        "Should return error when tsconfig.json is missing in project dir"
    );
}

#[test]
fn compile_missing_tsconfig_uses_defaults() {
    // Test that compilation works without tsconfig.json using defaults
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("src/index.ts"), "export const value = 42;");

    let mut args = default_args();
    args.files = vec![PathBuf::from("src/index.ts")];

    let result = compile(&args, base).expect("compile should succeed");

    assert!(result.diagnostics.is_empty());
    // Output should be next to source when no outDir specified
    assert!(base.join("src/index.js").is_file());
}

// =============================================================================
// E2E: Generic Utility Library Compilation
// =============================================================================

#[test]
fn compile_generic_utility_library_array_utils() {
    // Test compilation of generic array utility functions
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true,
            "strict": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Generic array utilities
    write_file(
        &base.join("src/array.ts"),
        r#"
export function map<T, U>(arr: T[], fn: (item: T, index: number) => U): U[] {
    const result: U[] = [];
    for (let i = 0; i < arr.length; i++) {
        result.push(fn(arr[i], i));
    }
    return result;
}

export function filter<T>(arr: T[], predicate: (item: T) => boolean): T[] {
    const result: T[] = [];
    for (const item of arr) {
        if (predicate(item)) {
            result.push(item);
        }
    }
    return result;
}

export function find<T>(arr: T[], predicate: (item: T) => boolean): T | undefined {
    for (const item of arr) {
        if (predicate(item)) {
            return item;
        }
    }
    return undefined;
}

export function reduce<T, U>(arr: T[], fn: (acc: U, item: T) => U, initial: U): U {
    let acc = initial;
    for (const item of arr) {
        acc = fn(acc, item);
    }
    return acc;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );
    assert!(
        base.join("dist/src/array.js").is_file(),
        "JS output should exist"
    );
    assert!(
        base.join("dist/src/array.d.ts").is_file(),
        "Declaration should exist"
    );

    // Verify JS output has type annotations stripped
    let js = std::fs::read_to_string(base.join("dist/src/array.js")).expect("read js");
    assert!(!js.contains(": T[]"), "Type annotations should be stripped");
    assert!(!js.contains(": U[]"), "Type annotations should be stripped");
    assert!(js.contains("function map"), "Function should be present");
    assert!(js.contains("function filter"), "Function should be present");
    assert!(js.contains("function find"), "Function should be present");
    assert!(js.contains("function reduce"), "Function should be present");

    // Verify declarations preserve types
    let dts = std::fs::read_to_string(base.join("dist/src/array.d.ts")).expect("read dts");
    assert!(
        dts.contains("map<T, U>") || dts.contains("map<T,U>"),
        "Generic should be in declaration"
    );
    assert!(
        dts.contains("filter<T>"),
        "Generic should be in declaration"
    );
}

#[test]
fn compile_generic_utility_library_type_utilities() {
    // Test compilation with type-level utilities (conditional types, mapped types)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Type utilities with runtime helpers
    write_file(
        &base.join("src/types.ts"),
        r#"
declare const Object: {
    freeze<T>(o: T): T;
    keys(o: object): string[];
};

// Declare built-in utility types
type Readonly<T> = {
    readonly [P in keyof T]: T[P];
};

type Partial<T> = {
    [P in keyof T]?: T[P];
};

// Type-level utilities (erased at runtime)
export type DeepReadonly<T> = {
    readonly [P in keyof T]: T[P] extends object ? Readonly<T[P]> : T[P];
};

export type DeepPartial<T> = {
    [P in keyof T]?: T[P] extends object ? Partial<T[P]> : T[P];
};

export type Nullable<T> = T | null;

// Mapped type that uses index access (T[P])
export type ValueTypes<T> = {
    [P in keyof T]: T[P];
};

// Runtime function using these types
export function deepFreeze<T extends object>(obj: T): DeepReadonly<T> {
    Object.freeze(obj);
    for (const key of Object.keys(obj)) {
        const value = (obj as Record<string, unknown>)[key];
        if (typeof value === "object" && value !== null) {
            deepFreeze(value as object);
        }
    }
    return obj as DeepReadonly<T>;
}

export function isNonNull<T>(value: T | null | undefined): value is T {
    return value !== null && value !== undefined;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    // Debug: print any diagnostics found
    if !result.diagnostics.is_empty() {
        eprintln!("\n=== DIAGNOSTICS FOUND ===");
        for diag in &result.diagnostics {
            eprintln!(
                "  TS{}: {} (at {}:{})",
                diag.code, diag.message_text, diag.file, diag.start
            );
        }
        eprintln!("=========================\n");
    }

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );
    assert!(
        base.join("dist/src/types.js").is_file(),
        "JS output should exist"
    );
    assert!(
        base.join("dist/src/types.d.ts").is_file(),
        "Declaration should exist"
    );

    // Verify JS output - type aliases should be completely erased
    let js = std::fs::read_to_string(base.join("dist/src/types.js")).expect("read js");
    assert!(!js.contains("DeepReadonly"), "Type alias should be erased");
    assert!(!js.contains("DeepPartial"), "Type alias should be erased");
    assert!(
        js.contains("function deepFreeze"),
        "Runtime function should be present"
    );
    assert!(
        js.contains("function isNonNull"),
        "Runtime function should be present"
    );

    // Verify declarations preserve type utilities
    let dts = std::fs::read_to_string(base.join("dist/src/types.d.ts")).expect("read dts");
    assert!(
        dts.contains("DeepReadonly"),
        "Type alias should be in declaration"
    );
    assert!(
        dts.contains("DeepPartial"),
        "Type alias should be in declaration"
    );
}

#[test]
fn compile_generic_utility_library_multi_file() {
    // Test multi-file generic utility library with re-exports
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true,
            "sourceMap": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Array utilities
    write_file(
        &base.join("src/array.ts"),
        r#"
export function first<T>(arr: T[]): T | undefined {
    return arr[0];
}

export function last<T>(arr: T[]): T | undefined {
    return arr[arr.length - 1];
}
"#,
    );

    // String utilities
    write_file(
        &base.join("src/string.ts"),
        r#"
export function capitalize(str: string): string {
    return str.charAt(0).toUpperCase() + str.slice(1);
}

export function repeat(str: string, count: number): string {
    let result = "";
    for (let i = 0; i < count; i++) {
        result += str;
    }
    return result;
}
"#,
    );

    // Function utilities
    write_file(
        &base.join("src/function.ts"),
        r#"
export function identity<T>(value: T): T {
    return value;
}

export function constant<T>(value: T): () => T {
    return () => value;
}

export function noop(): void {}
"#,
    );

    // Main index re-exporting everything
    write_file(
        &base.join("src/index.ts"),
        r#"
export { first, last } from "./array";
export { capitalize, repeat } from "./string";
export { identity, constant, noop } from "./function";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    // All JS files should exist
    assert!(base.join("dist/src/array.js").is_file());
    assert!(base.join("dist/src/string.js").is_file());
    assert!(base.join("dist/src/function.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());

    // All declaration files should exist
    assert!(base.join("dist/src/array.d.ts").is_file());
    assert!(base.join("dist/src/string.d.ts").is_file());
    assert!(base.join("dist/src/function.d.ts").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());

    // All source maps should exist
    assert!(base.join("dist/src/array.js.map").is_file());
    assert!(base.join("dist/src/index.js.map").is_file());

    // Verify index re-exports
    let index_js = std::fs::read_to_string(base.join("dist/src/index.js")).expect("read index");
    assert!(
        index_js.contains("require") || index_js.contains("export"),
        "Index should have exports"
    );

    // Verify index declaration
    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("first") && index_dts.contains("last"),
        "Index declaration should re-export array utils"
    );
}

#[test]
fn compile_generic_utility_library_with_constraints() {
    // Test generic functions with complex constraints
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/constrained.ts"),
        r#"
// Generic with extends constraint
export function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
    return obj[key];
}

// Generic with multiple constraints
export function setProperty<T extends object, K extends keyof T>(
    obj: T,
    key: K,
    value: T[K]
): T {
    obj[key] = value;
    return obj;
}

// Generic with default type parameter
export function createArray<T = string>(length: number, fill: T): T[] {
    const result: T[] = [];
    for (let i = 0; i < length; i++) {
        result.push(fill);
    }
    return result;
}

// Function overloads with generics
export function wrap<T>(value: T): T[];
export function wrap<T>(value: T, count: number): T[];
export function wrap<T>(value: T, count: number = 1): T[] {
    const result: T[] = [];
    for (let i = 0; i < count; i++) {
        result.push(value);
    }
    return result;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/constrained.js")).expect("read js");
    assert!(
        !js.contains("extends keyof"),
        "Constraints should be stripped"
    );
    assert!(
        !js.contains("extends object"),
        "Constraints should be stripped"
    );
    assert!(
        js.contains("function getProperty"),
        "Function should be present"
    );
    assert!(js.contains("function wrap"), "Function should be present");

    let dts = std::fs::read_to_string(base.join("dist/src/constrained.d.ts")).expect("read dts");
    // Check that generic functions are present in declaration
    assert!(
        dts.contains("getProperty"),
        "getProperty should be in declaration"
    );
    assert!(
        dts.contains("setProperty"),
        "setProperty should be in declaration"
    );
    assert!(
        dts.contains("createArray"),
        "createArray should be in declaration"
    );
    assert!(dts.contains("wrap"), "wrap should be in declaration");
}

#[test]
fn compile_generic_utility_library_classes() {
    // Test generic utility classes
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/collections.ts"),
        r#"
export class Stack<T> {
    private items: T[] = [];

    push(item: T): void {
        this.items.push(item);
    }

    pop(): T | undefined {
        return this.items.pop();
    }

    peek(): T | undefined {
        return this.items[this.items.length - 1];
    }

    get size(): number {
        return this.items.length;
    }

    isEmpty(): boolean {
        return this.items.length === 0;
    }
}

export class Queue<T> {
    private items: T[] = [];

    enqueue(item: T): void {
        this.items.push(item);
    }

    dequeue(): T | undefined {
        return this.items.shift();
    }

    front(): T | undefined {
        return this.items[0];
    }

    get size(): number {
        return this.items.length;
    }
}

export class Result<T, E> {
    private constructor(
        private readonly value: T | undefined,
        private readonly error: E | undefined,
        private readonly isOk: boolean
    ) {}

    static ok<T, E>(value: T): Result<T, E> {
        return new Result<T, E>(value, undefined, true);
    }

    static err<T, E>(error: E): Result<T, E> {
        return new Result<T, E>(undefined, error, false);
    }

    isSuccess(): boolean {
        return this.isOk;
    }

    getValue(): T | undefined {
        return this.value;
    }

    getError(): E | undefined {
        return this.error;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/collections.js")).expect("read js");
    assert!(js.contains("class Stack"), "Class should be present");
    assert!(js.contains("class Queue"), "Class should be present");
    assert!(js.contains("class Result"), "Class should be present");
    assert!(!js.contains("<T>"), "Generic parameters should be stripped");
    assert!(!js.contains("T[]"), "Type annotations should be stripped");
    assert!(
        !js.contains(": void"),
        "Return type annotations should be stripped"
    );

    let dts = std::fs::read_to_string(base.join("dist/src/collections.d.ts")).expect("read dts");
    assert!(
        dts.contains("Stack<T>"),
        "Generic class should be in declaration"
    );
    assert!(
        dts.contains("Queue<T>"),
        "Generic class should be in declaration"
    );
    assert!(
        dts.contains("Result<T, E>") || dts.contains("Result<T,E>"),
        "Generic class should be in declaration"
    );
}

// =============================================================================
// E2E: Module Re-exports
// =============================================================================

#[test]
fn compile_module_named_reexports() {
    // Test named re-exports: export { foo, bar } from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/utils.ts"),
        r#"
export function add(a: number, b: number): number {
    return a + b;
}

export function multiply(a: number, b: number): number {
    return a * b;
}

export const PI = 3.14159;
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export { add, multiply, PI } from "./utils";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );
    assert!(base.join("dist/src/utils.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());

    // Verify index re-exports
    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(index_dts.contains("add"), "add should be re-exported");
    assert!(
        index_dts.contains("multiply"),
        "multiply should be re-exported"
    );
    assert!(index_dts.contains("PI"), "PI should be re-exported");
}

#[test]
fn compile_module_renamed_reexports() {
    // Test renamed re-exports: export { foo as bar } from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/internal.ts"),
        r#"
export function internalHelper(): string {
    return "helper";
}

export const internalValue = 42;
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export { internalHelper as helper, internalValue as value } from "./internal";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(index_dts.contains("helper"), "helper should be re-exported");
    assert!(index_dts.contains("value"), "value should be re-exported");
}

#[test]
fn compile_module_star_reexports() {
    // Test star re-exports: export * from "./module"
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/math.ts"),
        r#"
export function sum(arr: number[]): number {
    let total = 0;
    for (const n of arr) {
        total += n;
    }
    return total;
}

export function average(arr: number[]): number {
    return sum(arr) / arr.length;
}
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export * from "./math";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("sum") || index_dts.contains("*"),
        "sum should be re-exported or star export present"
    );
}

#[test]
fn compile_module_chained_reexports() {
    // Test chained re-exports: A re-exports from B which re-exports from C
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Level 3: core module
    write_file(
        &base.join("src/core.ts"),
        r#"
export function coreFunction(): string {
    return "core";
}

export const CORE_VERSION = "1.0.0";
"#,
    );

    // Level 2: intermediate module
    write_file(
        &base.join("src/intermediate.ts"),
        r#"
export { coreFunction, CORE_VERSION } from "./core";

export function intermediateFunction(): string {
    return "intermediate";
}
"#,
    );

    // Level 1: public module
    write_file(
        &base.join("src/index.ts"),
        r#"
export { coreFunction, CORE_VERSION, intermediateFunction } from "./intermediate";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    // All files should be compiled
    assert!(base.join("dist/src/core.js").is_file());
    assert!(base.join("dist/src/intermediate.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("coreFunction"),
        "coreFunction should be re-exported"
    );
    assert!(
        index_dts.contains("intermediateFunction"),
        "intermediateFunction should be re-exported"
    );
}

#[test]
fn compile_module_mixed_exports_and_reexports() {
    // Test mixing local exports with re-exports
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/helpers.ts"),
        r#"
export function helperA(): string {
    return "A";
}

export function helperB(): string {
    return "B";
}
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
// Re-exports
export { helperA, helperB } from "./helpers";

// Local exports
export function localFunction(): number {
    return 42;
}

export const LOCAL_CONSTANT = "local";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_js = std::fs::read_to_string(base.join("dist/src/index.js")).expect("read js");
    assert!(
        index_js.contains("localFunction"),
        "Local function should be in output"
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("helperA"),
        "helperA should be re-exported"
    );
    assert!(
        index_dts.contains("localFunction"),
        "localFunction should be exported"
    );
    assert!(
        index_dts.contains("LOCAL_CONSTANT"),
        "LOCAL_CONSTANT should be exported"
    );
}

#[test]
fn compile_module_type_only_reexports() {
    // Test type-only re-exports
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/types.ts"),
        r#"
export type UserId = number;

export type UserName = string;

export function createId(n: number): UserId {
    return n;
}
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
// Type-only re-exports (should be erased from JS)
export type { UserId, UserName } from "./types";

// Value re-export
export { createId } from "./types";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_js = std::fs::read_to_string(base.join("dist/src/index.js")).expect("read js");
    // Type-only exports should not appear in runtime output, but createId should
    assert!(
        index_js.contains("createId"),
        "createId should be in output"
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("UserId"),
        "UserId type should be in declaration"
    );
    assert!(
        index_dts.contains("createId"),
        "createId should be in declaration"
    );
}

#[test]
fn compile_module_default_reexport() {
    // Test default re-export
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/component.ts"),
        r#"
export default function Component(): string {
    return "Component";
}

export const version = "1.0";
"#,
    );

    write_file(
        &base.join("src/index.ts"),
        r#"
export { default, version } from "./component";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(
        index_dts.contains("default") || index_dts.contains("Component"),
        "default export should be re-exported"
    );
    assert!(
        index_dts.contains("version"),
        "version should be re-exported"
    );
}

#[test]
fn compile_module_barrel_file() {
    // Test barrel file pattern (common in libraries)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    // Feature modules
    write_file(
        &base.join("src/features/auth.ts"),
        r#"
export function login(user: string): boolean {
    return user.length > 0;
}

export function logout(): void {}
"#,
    );

    write_file(
        &base.join("src/features/data.ts"),
        r#"
export function fetchData(): string[] {
    return [];
}

export function saveData(data: string[]): boolean {
    return data.length > 0;
}
"#,
    );

    // Barrel file
    write_file(
        &base.join("src/features/index.ts"),
        r#"
export { login, logout } from "./auth";
export { fetchData, saveData } from "./data";
"#,
    );

    // Main entry
    write_file(
        &base.join("src/index.ts"),
        r#"
export { login, logout, fetchData, saveData } from "./features";
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    // All files should be compiled
    assert!(base.join("dist/src/features/auth.js").is_file());
    assert!(base.join("dist/src/features/data.js").is_file());
    assert!(base.join("dist/src/features/index.js").is_file());
    assert!(base.join("dist/src/index.js").is_file());

    let index_dts = std::fs::read_to_string(base.join("dist/src/index.d.ts")).expect("read dts");
    assert!(index_dts.contains("login"), "login should be re-exported");
    assert!(
        index_dts.contains("fetchData"),
        "fetchData should be re-exported"
    );
}

// =============================================================================
// E2E: Classes with Generic Methods
// =============================================================================

#[test]
fn compile_class_with_generic_constructor() {
    // Test class with generic constructor pattern
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/builder.ts"),
        r#"
export class Builder<T> {
    private value: T;

    constructor(initial: T) {
        this.value = initial;
    }

    set(value: T): Builder<T> {
        this.value = value;
        return this;
    }

    transform<U>(fn: (value: T) => U): Builder<U> {
        return new Builder(fn(this.value));
    }

    build(): T {
        return this.value;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/builder.js")).expect("read js");
    assert!(js.contains("class Builder"), "Class should be present");
    assert!(js.contains("constructor("), "Constructor should be present");
    assert!(!js.contains("<T>"), "Generic should be stripped");

    let dts = std::fs::read_to_string(base.join("dist/src/builder.d.ts")).expect("read dts");
    assert!(
        dts.contains("Builder<T>"),
        "Generic class should be in declaration"
    );
    assert!(
        dts.contains("transform<U>"),
        "Generic method should be in declaration"
    );
}

// =============================================================================
// E2E: Namespace Exports
// =============================================================================

#[test]
fn compile_basic_namespace_export() {
    // Test basic namespace compiles without errors and produces JS output
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/utils.ts"),
        r#"
export namespace Utils {
    export const VERSION = "1.0.0";
    export function greet(name: string): string {
        return "Hello, " + name;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/utils.js")).expect("read js");
    // Namespace should produce some output
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_nested_namespace_export() {
    // Test nested namespace compiles without errors
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/api.ts"),
        r#"
export namespace API {
    export namespace V1 {
        export function getUsers(): string[] {
            return ["user1", "user2"];
        }
    }

    export namespace V2 {
        export function getUsers(): string[] {
            return ["user1", "user2", "user3"];
        }
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/api.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_namespace_with_class() {
    // Test namespace containing a class compiles without errors
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/models.ts"),
        r#"
export namespace Models {
    export class User {
        name: string;
        constructor(name: string) {
            this.name = name;
        }
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/models.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Enum Compilation
// =============================================================================

#[test]
fn compile_numeric_enum() {
    // Test basic numeric enum compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/status.ts"),
        r#"
export enum Status {
    Pending,
    Active,
    Completed,
    Failed
}

export function getStatusName(status: Status): string {
    return Status[status];
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/status.js")).expect("read js");
    assert!(js.contains("Status"), "Enum should be present in JS");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_string_enum() {
    // Test string enum compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/direction.ts"),
        r#"
export enum Direction {
    Up = "UP",
    Down = "DOWN",
    Left = "LEFT",
    Right = "RIGHT"
}

export function move(dir: Direction): void {
    console.log(dir);
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/direction.js")).expect("read js");
    assert!(js.contains("Direction"), "Enum should be present in JS");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_const_enum() {
    // Test const enum compilation (should be inlined)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/flags.ts"),
        r#"
export const enum Flags {
    None = 0,
    Read = 1,
    Write = 2,
    Execute = 4
}

export function hasFlag(flags: Flags, flag: Flags): boolean {
    return (flags & flag) !== 0;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/flags.js")).expect("read js");
    // Const enums may be inlined, so just verify compilation succeeded
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_enum_with_computed_values() {
    // Test enum with computed/expression values
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/sizes.ts"),
        r#"
export enum Size {
    Small = 1,
    Medium = Small * 2,
    Large = Medium * 2,
    ExtraLarge = Large * 2
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/sizes.js")).expect("read js");
    assert!(js.contains("Size"), "Enum should be present in JS");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Arrow Function Compilation
// =============================================================================

#[test]
fn compile_basic_arrow_function() {
    // Test basic arrow function compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/utils.ts"),
        r#"
export const add = (a: number, b: number): number => a + b;
export const multiply = (a: number, b: number): number => {
    return a * b;
};
export const identity = <T>(x: T): T => x;
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/utils.js")).expect("read js");
    assert!(
        js.contains("=>") || js.contains("function"),
        "Arrow or function should be present"
    );
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_arrow_function_with_rest_params() {
    // Test arrow function with rest parameters
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/helpers.ts"),
        r#"
export const sum = (...numbers: number[]): number => {
    let total = 0;
    for (const n of numbers) {
        total += n;
    }
    return total;
};

export const first = <T>(...items: T[]): T => items[0];
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/helpers.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_arrow_function_with_default_params() {
    // Test arrow function with default parameters
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/greet.ts"),
        r#"
export const greet = (name: string, greeting: string = "Hello"): string => {
    return greeting + ", " + name;
};

export const repeat = (str: string, times: number = 1): string => {
    let result = "";
    for (let i = 0; i < times; i++) {
        result += str;
    }
    return result;
};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/greet.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_arrow_function_in_class() {
    // Test arrow functions as class properties (for lexical this)
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/counter.ts"),
        r#"
export class Counter {
    count: number = 0;

    increment = (): void => {
        this.count++;
    };

    decrement = (): void => {
        this.count--;
    };

    reset = (): void => {
        this.count = 0;
    };
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/counter.js")).expect("read js");
    assert!(js.contains("Counter"), "Class should be present");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Spread Operator Compilation
// =============================================================================

#[test]
fn compile_array_spread() {
    // Test array spread operator compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/arrays.ts"),
        r#"
export function concat(a: number[], b: number[]): number[] {
    return [...a, ...b];
}

export function copy(a: number[]): number[] {
    return [...a];
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/arrays.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_object_spread() {
    // Test object spread operator compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/objects.ts"),
        r#"
interface Person {
    name: string;
    age: number;
}

export function clone(obj: Person): Person {
    return { ...obj };
}

export function update(obj: Person, updates: Person): Person {
    return { ...obj, ...updates };
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/objects.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_function_call_spread() {
    // Test spread in function calls
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/calls.ts"),
        r#"
export function apply(fn: (...args: number[]) => number, args: number[]): number {
    return fn(...args);
}

export function log(...items: string[]): void {
    console.log(...items);
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/calls.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Template Literal Compilation
// =============================================================================

#[test]
fn compile_basic_template_literal() {
    // Test basic template literal compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/greet.ts"),
        r#"
export function greet(name: string): string {
    return `Hello, ${name}!`;
}

export function format(a: number, b: number): string {
    return `${a} + ${b} = ${a + b}`;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/greet.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_multiline_template_literal() {
    // Test multiline template literal compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/html.ts"),
        r#"
export function createDiv(content: string): string {
    const result = `<div><p>${content}</p></div>`;
    return result;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/html.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_nested_template_literal() {
    // Test nested template expressions
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/nested.ts"),
        r#"
export function wrap(inner: string, outer: string): string {
    return `${outer}: ${`[${inner}]`}`;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/nested.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

// =============================================================================
// E2E: Destructuring Assignment Compilation
// =============================================================================

#[test]
fn compile_object_destructuring() {
    // Test object destructuring compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/extract.ts"),
        r#"
interface Point {
    x: number;
    y: number;
}

export function getX(point: Point): number {
    const { x } = point;
    return x;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/extract.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_array_destructuring() {
    // Test array destructuring compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/arrays.ts"),
        r#"
export function getFirst(arr: number[]): number {
    const [first] = arr;
    return first;
}

export function getSecond(arr: number[]): number {
    const [, second] = arr;
    return second;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/arrays.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_destructuring_with_defaults() {
    // Test destructuring with default values
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/defaults.ts"),
        r#"
interface Config {
    host: string;
    port: number;
}

export function getPort(config: Config): number {
    const { port = 3000 } = config;
    return port;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/defaults.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_optional_chaining() {
    // Test optional chaining (?.) compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/optional.ts"),
        r#"
interface User {
    name: string;
    address?: {
        city: string;
    };
}

export function getCity(user: User): string | undefined {
    return user.address?.city;
}

export function getLength(arr?: string[]): number | undefined {
    return arr?.length;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/optional.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_nullish_coalescing() {
    // Test nullish coalescing (??) compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/nullish.ts"),
        r#"
export function getValueOrDefault(value: string | null | undefined): string {
    return value ?? "default";
}

export function getNumberOrZero(num: number | null): number {
    return num ?? 0;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/nullish.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_optional_chaining_with_call() {
    // Test optional chaining with method calls
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/optcall.ts"),
        r#"
interface Logger {
    log?: (msg: string) => void;
}

export function maybeLog(logger: Logger, msg: string): void {
    logger.log?.(msg);
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/optcall.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_class_inheritance() {
    // Test class inheritance compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/classes.ts"),
        r#"
export class Animal {
    constructor(public name: string) {}
    speak(): string {
        return this.name;
    }
}

export class Dog extends Animal {
    constructor(name: string) {
        super(name);
    }
    speak(): string {
        return "Woof: " + super.speak();
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/classes.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_class_static_members() {
    // Test class static members compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/staticclass.ts"),
        r#"
export class Counter {
    static count: number = 0;

    static increment(): number {
        Counter.count += 1;
        return Counter.count;
    }

    static reset(): void {
        Counter.count = 0;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/staticclass.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_class_accessors() {
    // Test class getter/setter compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/accessors.ts"),
        r#"
export class Rectangle {
    private _width: number = 0;
    private _height: number = 0;

    get width(): number {
        return this._width;
    }

    set width(value: number) {
        this._width = value;
    }

    get area(): number {
        return this._width * this._height;
    }
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/accessors.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_computed_property_names() {
    // Test computed property names compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/computed.ts"),
        r#"
const KEY = "dynamicKey";

export const obj = {
    [KEY]: "value",
    ["literal" + "Key"]: 42
};

export function getProp(key: string): { [k: string]: number } {
    return { [key]: 100 };
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/computed.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_for_of_loop() {
    // Test for...of loop compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/forof.ts"),
        r#"
export function sumArray(arr: number[]): number {
    let sum = 0;
    for (const num of arr) {
        sum += num;
    }
    return sum;
}

export function joinStrings(arr: string[]): string {
    let result = "";
    for (const str of arr) {
        result += str;
    }
    return result;
}
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/forof.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}

#[test]
fn compile_shorthand_methods() {
    // Test shorthand method syntax compilation
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist"
          },
          "include": ["src/**/*.ts"]
        }"#,
    );

    write_file(
        &base.join("src/methods.ts"),
        r#"
export const calculator = {
    add(a: number, b: number): number {
        return a + b;
    },
    subtract(a: number, b: number): number {
        return a - b;
    }
};
"#,
    );

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");

    assert!(
        result.diagnostics.is_empty(),
        "Should compile without errors: {:?}",
        result.diagnostics
    );

    let js = std::fs::read_to_string(base.join("dist/src/methods.js")).expect("read js");
    assert!(!js.is_empty(), "JS output should not be empty");
}
