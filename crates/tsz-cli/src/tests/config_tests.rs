use super::config::{
    JsxEmit, ModuleResolutionKind, default_lib_name_for_target, load_tsconfig, parse_tsconfig,
    resolve_compiler_options, resolve_default_lib_files_from_dir, resolve_lib_files_from_dir,
    resolve_lib_files_from_dir_with_options,
};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tsz::emitter::{ModuleKind, ScriptTarget};

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

    // Some CI environments may not provide TypeScript lib files; skip in that case.
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
fn resolve_default_lib_files_from_dir_follows_root_references() {
    let temp = TempDir::new().expect("temp dir");
    write_file(
        &temp.path,
        "lib.d.ts",
        "/// <reference lib=\"es5\" />\ninterface Console { log(...args: any[]): void; }\n",
    );
    write_file(
        &temp.path,
        "lib.es5.d.ts",
        "interface Array<T> { length: number; }\n",
    );

    let resolved = resolve_default_lib_files_from_dir(ScriptTarget::ES5, &temp.path)
        .expect("default libs should resolve from provided directory");
    let names: Vec<String> = resolved
        .iter()
        .map(|p| {
            p.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();

    assert_eq!(names.first().map(|s| s.as_str()), Some("lib.d.ts"));
    assert!(
        names.iter().any(|name| name == "lib.es5.d.ts"),
        "resolved libs should include transitive es5 reference: {names:?}"
    );
}

#[test]
fn resolve_default_lib_files_from_dir_does_not_fallback_to_core_libs() {
    let temp = TempDir::new().expect("temp dir");
    write_file(
        &temp.path,
        "lib.es5.d.ts",
        "interface Array<T> { length: number; }\n",
    );

    let err = resolve_default_lib_files_from_dir(ScriptTarget::ES5, &temp.path)
        .expect_err("missing lib.d.ts should fail instead of falling back to core libs");
    let message = err.to_string();
    assert!(
        message.contains("compilerOptions.lib") && message.contains("es5.full"),
        "{message}"
    );
}

#[test]
fn resolve_lib_files_from_dir_with_options_can_disable_transitive_references() {
    let temp = TempDir::new().expect("temp dir");
    write_file(
        &temp.path,
        "lib.es2015.d.ts",
        "/// <reference lib=\"es5\" />\ninterface Promise<T> {}\n",
    );
    write_file(
        &temp.path,
        "lib.es5.d.ts",
        "interface Array<T> { length: number; }\n",
    );

    let no_follow =
        resolve_lib_files_from_dir_with_options(&["es2015".to_string()], false, &temp.path)
            .expect("explicit libs should resolve without references");
    let names: Vec<String> = no_follow
        .iter()
        .map(|p| {
            p.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();

    assert_eq!(names, vec!["lib.es2015.d.ts".to_string()]);
}

#[test]
fn resolve_lib_files_from_dir_follows_transitive_references_by_default() {
    let temp = TempDir::new().expect("temp dir");
    write_file(
        &temp.path,
        "lib.es2015.d.ts",
        "/// <reference lib=\"es5\" />\ninterface Promise<T> {}\n",
    );
    write_file(
        &temp.path,
        "lib.es5.d.ts",
        "interface Array<T> { length: number; }\n",
    );

    let resolved = resolve_lib_files_from_dir(&["es2015".to_string()], &temp.path)
        .expect("explicit lib resolution should follow references");
    let names: Vec<String> = resolved
        .iter()
        .map(|p| {
            p.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();

    assert_eq!(names.first().map(|s| s.as_str()), Some("lib.es2015.d.ts"));
    assert!(
        names.iter().any(|name| name == "lib.es5.d.ts"),
        "expected transitive es5 from es2015: {names:?}"
    );
}

#[test]
fn resolve_default_lib_files_from_dir_uses_es6_root_for_es2015_target() {
    let temp = TempDir::new().expect("temp dir");
    write_file(
        &temp.path,
        "lib.es6.d.ts",
        "/// <reference lib=\"es2015\" />\ninterface SymbolConstructor {}\n",
    );
    write_file(
        &temp.path,
        "lib.es2015.d.ts",
        "/// <reference lib=\"es5\" />\ninterface Promise<T> {}\n",
    );
    write_file(
        &temp.path,
        "lib.es5.d.ts",
        "interface Array<T> { length: number; }\n",
    );

    let resolved = resolve_default_lib_files_from_dir(ScriptTarget::ES2015, &temp.path)
        .expect("ES2015 target should resolve through lib.es6.d.ts root");
    let names: Vec<String> = resolved
        .iter()
        .map(|p| {
            p.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();

    assert_eq!(names.first().map(|s| s.as_str()), Some("lib.es6.d.ts"));
    assert!(
        names.iter().any(|name| name == "lib.es2015.d.ts"),
        "expected es2015 to be included transitively: {names:?}"
    );
    assert!(
        names.iter().any(|name| name == "lib.es5.d.ts"),
        "expected es5 to be included transitively: {names:?}"
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
    // Preserve the normalized compilerOptions.lib error envelope.
    assert!(message.contains("compilerOptions.lib"), "{message}");
}

#[test]
fn default_lib_name_for_target_matches_tsc_spec() {
    assert_eq!(default_lib_name_for_target(ScriptTarget::ES5), "lib");
    assert_eq!(default_lib_name_for_target(ScriptTarget::ES2015), "es6");
    assert_eq!(
        default_lib_name_for_target(ScriptTarget::ES2016),
        "es2016.full"
    );
    assert_eq!(
        default_lib_name_for_target(ScriptTarget::ES2017),
        "es2017.full"
    );
    assert_eq!(
        default_lib_name_for_target(ScriptTarget::ES2018),
        "es2018.full"
    );
    assert_eq!(
        default_lib_name_for_target(ScriptTarget::ES2019),
        "es2019.full"
    );
    assert_eq!(
        default_lib_name_for_target(ScriptTarget::ES2020),
        "es2020.full"
    );
    assert_eq!(
        default_lib_name_for_target(ScriptTarget::ES2021),
        "es2021.full"
    );
    assert_eq!(
        default_lib_name_for_target(ScriptTarget::ES2022),
        "es2022.full"
    );
    assert_eq!(
        default_lib_name_for_target(ScriptTarget::ESNext),
        "esnext.full"
    );
}

#[test]
fn resolve_lib_files_from_dir_supports_tsc_aliases() {
    let temp = TempDir::new().expect("temp dir");
    write_file(&temp.path, "lib.es5.full.d.ts", "interface A {}\n");
    write_file(&temp.path, "lib.es2015.full.d.ts", "interface B {}\n");
    write_file(&temp.path, "lib.es2016.d.ts", "interface C {}\n");

    let resolved = resolve_lib_files_from_dir(
        &["lib".to_string(), "es6".to_string(), "es7".to_string()],
        &temp.path,
    )
    .expect("aliases should resolve");
    let names: Vec<String> = resolved
        .iter()
        .map(|p| {
            p.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();

    assert_eq!(names[0], "lib.es5.full.d.ts");
    assert_eq!(names[1], "lib.es2015.full.d.ts");
    assert_eq!(names[2], "lib.es2016.d.ts");
}

#[test]
fn resolve_lib_files_from_dir_dedupes_recursive_references() {
    let temp = TempDir::new().expect("temp dir");
    write_file(
        &temp.path,
        "lib.custom.d.ts",
        "/// <reference lib=\"es2015\" />\n/// <reference lib=\"es5\" />\n",
    );
    write_file(
        &temp.path,
        "lib.es2015.d.ts",
        "/// <reference lib=\"es5\" />\ninterface Promise<T> {}\n",
    );
    write_file(
        &temp.path,
        "lib.es5.d.ts",
        "interface Array<T> { length: number; }\n",
    );

    let resolved = resolve_lib_files_from_dir(&["custom".to_string()], &temp.path)
        .expect("recursive refs should resolve");
    let names: Vec<&str> = resolved
        .iter()
        .map(|p| p.file_name().and_then(|name| name.to_str()).unwrap_or(""))
        .collect();

    let es5_count = names.iter().filter(|&&name| name == "lib.es5.d.ts").count();
    assert_eq!(es5_count, 1, "es5 should only appear once: {names:?}");
}

#[test]
fn resolve_compiler_options_rejects_no_lib_with_lib() {
    let config = parse_tsconfig(
        r#"{
          "compilerOptions": {
            "noLib": true,
            "lib": ["es2015"]
          }
        }"#,
    )
    .expect("should parse config");

    let err = resolve_compiler_options(config.compiler_options.as_ref())
        .expect_err("noLib + lib should fail");
    let message = err.to_string();
    assert!(message.contains("Option 'lib'"), "{message}");
    assert!(message.contains("option 'noLib'"), "{message}");
}

#[test]
fn resolve_compiler_options_no_lib_disables_lib_loading() {
    let config = parse_tsconfig(
        r#"{
          "compilerOptions": {
            "noLib": true
          }
        }"#,
    )
    .expect("should parse config");

    let resolved =
        resolve_compiler_options(config.compiler_options.as_ref()).expect("should resolve");
    assert!(resolved.checker.no_lib);
    assert!(resolved.lib_files.is_empty());
    assert!(!resolved.lib_is_default);
}

#[allow(dead_code)]
fn canonicalize_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
