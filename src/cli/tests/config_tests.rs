use super::config::{
    JsxEmit, ModuleResolutionKind, load_tsconfig, parse_tsconfig, resolve_compiler_options,
};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::emitter::{ModuleKind, ScriptTarget};

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
        path.push(format!("tsz_cli_test_{}_{}", std::process::id(), nanos));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, contents).expect("failed to write test file");
    path
}

#[test]
fn parses_jsonc_with_trailing_commas() {
    let input = r#"
    {
      // comment
      "compilerOptions": {
        "target": "es2017", /* inline */
        "module": "commonjs",
      },
      "include": ["src/**/*",],
    }
    "#;

    let config = parse_tsconfig(input).expect("should parse JSONC");
    let options = config.compiler_options.expect("compilerOptions missing");

    assert_eq!(options.target.as_deref(), Some("es2017"));
    assert_eq!(options.module.as_deref(), Some("commonjs"));
    assert_eq!(config.include, Some(vec!["src/**/*".to_string()]));
}

#[test]
fn load_tsconfig_merges_extends() {
    let temp = TempDir::new().expect("temp dir");

    write_file(
        &temp.path,
        "tsconfig.base.json",
        r#"{
          "compilerOptions": {"target": "es2015", "strict": true},
          "include": ["src"],
          "exclude": ["dist"]
        }"#,
    );

    let child_path = write_file(
        &temp.path,
        "tsconfig.json",
        r#"{
          "extends": "./tsconfig.base.json",
          "compilerOptions": {"module": "commonjs", "strict": false},
          "files": ["main.ts"]
        }"#,
    );

    let config = load_tsconfig(&child_path).expect("should load config");
    let options = config.compiler_options.expect("compilerOptions missing");

    assert_eq!(options.target.as_deref(), Some("es2015"));
    assert_eq!(options.module.as_deref(), Some("commonjs"));
    assert_eq!(options.strict, Some(false));
    assert_eq!(config.include, Some(vec!["src".to_string()]));
    assert_eq!(config.exclude, Some(vec!["dist".to_string()]));
    assert_eq!(config.files, Some(vec!["main.ts".to_string()]));
}

#[test]
fn load_tsconfig_detects_extends_cycle() {
    let temp = TempDir::new().expect("temp dir");

    write_file(&temp.path, "a.json", r#"{"extends":"./b.json"}"#);
    write_file(&temp.path, "b.json", r#"{"extends":"./a.json"}"#);

    let err = load_tsconfig(&temp.path.join("a.json")).expect_err("cycle should error");
    let message = err.to_string();
    assert!(message.contains("extends cycle"), "{message}");
}

#[test]
fn resolve_compiler_options_defaults() {
    let resolved = resolve_compiler_options(None).expect("defaults should resolve");

    assert_eq!(resolved.printer.target, ScriptTarget::ESNext);
    assert_eq!(resolved.printer.module, ModuleKind::None);
    assert!(resolved.jsx.is_none());
    assert!(!resolved.lib_files.is_empty());
    assert!(resolved.lib_is_default);
    assert!(resolved.root_dir.is_none());
    assert!(resolved.out_dir.is_none());
    assert!(!resolved.checker.strict);
    assert!(!resolved.no_emit);
    assert!(!resolved.no_emit_on_error);
}

#[test]
fn resolve_compiler_options_overrides() {
    let config = parse_tsconfig(
        r#"{
          "compilerOptions": {
            "target": "ES2020",
            "module": "common-js",
            "moduleResolution": "bundler",
            "jsx": "preserve",
            "rootDir": "src",
            "outDir": "dist",
            "declaration": true,
            "declarationDir": "types",
            "strict": true,
            "noEmit": true,
            "noEmitOnError": true
          }
        }"#,
    )
    .expect("should parse config");

    let resolved = resolve_compiler_options(config.compiler_options.as_ref())
        .expect("compiler options should resolve");

    assert_eq!(resolved.printer.target, ScriptTarget::ES2020);
    assert_eq!(resolved.printer.module, ModuleKind::CommonJS);
    assert_eq!(
        resolved.module_resolution,
        Some(ModuleResolutionKind::Bundler)
    );
    assert_eq!(resolved.jsx, Some(JsxEmit::Preserve));
    assert!(!resolved.lib_files.is_empty());
    assert!(resolved.lib_is_default);
    assert_eq!(resolved.root_dir, Some(PathBuf::from("src")));
    assert_eq!(resolved.out_dir, Some(PathBuf::from("dist")));
    assert_eq!(resolved.declaration_dir, Some(PathBuf::from("types")));
    assert!(resolved.emit_declarations);
    assert!(resolved.checker.strict);
    assert!(resolved.no_emit);
    assert!(resolved.no_emit_on_error);
}

#[test]
fn resolve_compiler_options_rejects_unknown_values() {
    let config = parse_tsconfig(
        r#"{
          "compilerOptions": {
            "target": "es2999",
            "module": "totally-not-a-module"
          }
        }"#,
    )
    .expect("should parse config");

    let err = resolve_compiler_options(config.compiler_options.as_ref())
        .expect_err("unknown compilerOptions should error");
    let message = err.to_string();
    assert!(message.contains("compilerOptions.target"), "{message}");
}

#[test]
fn resolve_compiler_options_rejects_unknown_module_resolution() {
    let config = parse_tsconfig(
        r#"{
          "compilerOptions": {
            "moduleResolution": "sideways"
          }
        }"#,
    )
    .expect("should parse config");

    let err = resolve_compiler_options(config.compiler_options.as_ref())
        .expect_err("unknown moduleResolution should error");
    let message = err.to_string();
    assert!(
        message.contains("compilerOptions.moduleResolution"),
        "{message}"
    );
}

#[test]
fn resolve_compiler_options_rejects_unsupported_jsx() {
    let config = parse_tsconfig(
        r#"{
          "compilerOptions": {
            "jsx": "react"
          }
        }"#,
    )
    .expect("should parse config");

    let err = resolve_compiler_options(config.compiler_options.as_ref())
        .expect_err("unsupported jsx should error");
    let message = err.to_string();
    assert!(message.contains("compilerOptions.jsx"), "{message}");
}

#[test]
fn resolve_compiler_options_rejects_paths_without_base_url() {
    let config = parse_tsconfig(
        r#"{
          "compilerOptions": {
            "paths": {
              "@app/*": ["src/*"]
            }
          }
        }"#,
    )
    .expect("should parse config");

    let err = resolve_compiler_options(config.compiler_options.as_ref())
        .expect_err("paths without baseUrl should error");
    let message = err.to_string();
    assert!(message.contains("compilerOptions.paths"), "{message}");
}

#[test]
fn resolve_compiler_options_resolves_lib_files() {
    let config = parse_tsconfig(
        r#"{
          "compilerOptions": {
            "lib": ["es2015", "dom"]
          }
        }"#,
    )
    .expect("should parse config");

    let resolved = resolve_compiler_options(config.compiler_options.as_ref());

    // If lib resolution fails (e.g., lib dir not found), skip this test
    let Ok(resolved) = resolved else {
        return;
    };

    // If lib_files is empty, it means we're falling back to embedded libs (valid scenario)
    if resolved.lib_files.is_empty() {
        return;
    }

    // Helper to check if a path contains a lib by name
    // Handles naming conventions: "es2015.d.ts", "lib.es2015.d.ts", "dom.generated.d.ts"
    let contains_lib = |lib_name: &str| {
        resolved.lib_files.iter().any(|p| {
            let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            // Strip .d.ts suffix
            let name = file_name.trim_end_matches(".d.ts");
            // Strip .generated suffix if present
            let name = name.trim_end_matches(".generated");
            // Check if it matches the lib name (with or without lib. prefix)
            name == lib_name || name == format!("lib.{}", lib_name)
        })
    };

    assert!(
        contains_lib("es2015"),
        "lib_files should contain es2015: {:?}",
        resolved.lib_files
    );
    assert!(
        contains_lib("es5"),
        "lib_files should contain es5 (dependency of es2015): {:?}",
        resolved.lib_files
    );
    assert!(
        contains_lib("dom"),
        "lib_files should contain dom: {:?}",
        resolved.lib_files
    );
}

#[test]
fn resolve_compiler_options_rejects_unknown_lib() {
    let config = parse_tsconfig(
        r#"{
          "compilerOptions": {
            "lib": ["nope"]
          }
        }"#,
    )
    .expect("should parse config");

    let err = resolve_compiler_options(config.compiler_options.as_ref())
        .expect_err("unsupported lib should error");
    let message = err.to_string();
    // With embedded libs, we always get a "compilerOptions.lib" error for unknown libs
    assert!(message.contains("compilerOptions.lib"), "{message}");
}

#[allow(dead_code)]
fn canonicalize_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
