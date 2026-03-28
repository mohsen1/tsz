use super::*;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz::config::{CompilerOptions, resolve_compiler_options};
use tsz::emitter::ModuleKind;

#[test]
fn test_preserve_symlinks_keeps_symlink_path_identity() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let dir = std::env::temp_dir().join("tsz_driver_resolution_preserve_symlinks");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("real")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(dir.join("real/index.d.ts"), "export interface Box {}").unwrap();
    symlink(dir.join("real"), dir.join("linked")).unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import type { Box } from '../linked';\nexport type T = Box;",
    )
    .unwrap();

    let symlink_path = dir.join("linked/index.d.ts");
    let real_path = canonicalize_or_owned(&dir.join("real/index.d.ts"));
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();

    let preserve_options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        preserve_symlinks: true,
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut preserve_cache = ModuleResolutionCache::default();
    let preserved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "../linked",
        &preserve_options,
        &dir,
        &mut preserve_cache,
        &known_files,
    );
    assert_eq!(preserved, Some(symlink_path.clone()));

    let realpath_options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        preserve_symlinks: false,
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut realpath_cache = ModuleResolutionCache::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "../linked",
        &realpath_options,
        &dir,
        &mut realpath_cache,
        &known_files,
    );
    assert_eq!(resolved, Some(real_path));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_exports_js_target_substitutes_dts() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_exports_js_target");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","version":"0.0.1","exports":"./entrypoint.js"}"#,
    )
    .unwrap();
    fs::write(dir.join("node_modules/pkg/entrypoint.d.ts"), "export {};").unwrap();
    fs::write(dir.join("src/index.ts"), "import * as p from 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "pkg",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved,
        Some(canonicalize_or_owned(
            &dir.join("node_modules/pkg/entrypoint.d.ts"),
        )),
        "exports target with .js should resolve to an adjacent declaration file"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_exports_runtime_targets_substitute_matching_declaration_sidecars() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_exports_sidecars");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{
            "name":"pkg",
            "type":"module",
            "exports":{
                ".":"./index.js",
                "./mjs":"./entry.mjs",
                "./cjs":"./entry.cjs"
            }
        }"#,
    )
    .unwrap();
    fs::write(dir.join("node_modules/pkg/index.d.ts"), "export {};").unwrap();
    fs::write(dir.join("node_modules/pkg/entry.d.mts"), "export {};").unwrap();
    fs::write(dir.join("node_modules/pkg/entry.d.cts"), "export = 1;").unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import 'pkg'; import 'pkg/mjs'; import 'pkg/cjs';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();

    assert_eq!(
        resolve_module_specifier(
            &dir.join("src/index.ts"),
            "pkg",
            &options,
            &dir,
            &mut cache,
            &known_files,
        ),
        Some(canonicalize_or_owned(
            &dir.join("node_modules/pkg/index.d.ts"),
        ))
    );
    assert_eq!(
        resolve_module_specifier(
            &dir.join("src/index.ts"),
            "pkg/mjs",
            &options,
            &dir,
            &mut cache,
            &known_files,
        ),
        Some(canonicalize_or_owned(
            &dir.join("node_modules/pkg/entry.d.mts"),
        ))
    );
    assert_eq!(
        resolve_module_specifier(
            &dir.join("src/index.ts"),
            "pkg/cjs",
            &options,
            &dir,
            &mut cache,
            &known_files,
        ),
        Some(canonicalize_or_owned(
            &dir.join("node_modules/pkg/entry.d.cts"),
        ))
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_exports_directory_key_does_not_expose_arbitrary_subpaths() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_exports_directory_key");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/inner")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/inner/package.json"),
        r#"{
            "name":"inner",
            "type":"module",
            "exports":{
                "./":"./"
            }
        }"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/other.d.ts"),
        "export interface Thing {}\n",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/index.d.ts"),
        "export const x: number;\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import { Thing } from 'inner/other';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "inner/other",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved, None,
        "a bare './' exports entry should not expose arbitrary package subpaths"
    );

    let resolved_index = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "inner/index.js",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );
    assert_eq!(
        resolved_index,
        Some(canonicalize_or_owned(
            &dir.join("node_modules/inner/index.d.ts"),
        )),
        "a bare './' exports entry should still expose explicit file-like subpaths"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_root_types_js_is_ignored_for_module_resolution() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_package_types_js_ignored");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/foo")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/foo/package.json"),
        r#"{"name":"foo","types":"foo.js"}"#,
    )
    .unwrap();
    fs::write(dir.join("node_modules/foo/foo.js"), "module.exports = {};").unwrap();
    fs::write(dir.join("src/index.ts"), "import 'foo';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "foo",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved, None,
        "package.json types entries should not resolve runtime JS files"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_root_main_js_still_resolves_for_module_resolution() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_package_main_js_runtime");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/foo")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/foo/package.json"),
        r#"{"name":"foo","main":"foo.js"}"#,
    )
    .unwrap();
    fs::write(dir.join("node_modules/foo/foo.js"), "module.exports = {};").unwrap();
    fs::write(dir.join("src/index.ts"), "import 'foo';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        allow_js: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "foo",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved,
        Some(canonicalize_or_owned(&dir.join("node_modules/foo/foo.js"))),
        "package.json main entries should still resolve runtime JS files"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_extensionless_json_import_does_not_resolve_with_resolve_json_module() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_extensionless_json");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(dir.join("src/index.ts"), "import data = require('./data');").unwrap();
    fs::write(dir.join("src/data.json"), "{\"value\": 42}").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();

    assert_eq!(
        resolve_module_specifier(
            &dir.join("src/index.ts"),
            "./data",
            &options,
            &dir,
            &mut cache,
            &known_files,
        ),
        None,
        "extensionless relative imports should not fall through to data.json"
    );

    assert_eq!(
        resolve_module_specifier(
            &dir.join("src/index.ts"),
            "./data.json",
            &options,
            &dir,
            &mut cache,
            &known_files,
        ),
        Some(canonicalize_or_owned(&dir.join("src/data.json"))),
        "explicit .json imports should still resolve when resolveJsonModule is enabled"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_collect_module_specifiers_finds_dynamic_imports() {
    let text = r#"import("./foo").then(x => x);"#;
    let path = Path::new("test.mts");
    let specifiers = collect_module_specifiers_from_text(path, text);
    assert!(
        specifiers.contains(&"./foo".to_string()),
        "Should find dynamic import specifier './foo', got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_finds_plain_require_calls() {
    let text = r#"const data = require("./data.json");"#;
    let path = Path::new("test.js");
    let specifiers = collect_module_specifiers_from_text(path, text);
    assert!(
        specifiers.contains(&"./data.json".to_string()),
        "Should find require specifier './data.json', got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_require_has_correct_kind() {
    use tsz::module_resolver::ImportKind;
    let text = r#"const data = require("./data.json");"#;
    let file_name = "test.js".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);
    let requires: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::CjsRequire)
        .map(|(s, _, _, _)| s.as_str())
        .collect();
    assert!(
        requires.contains(&"./data.json"),
        "Should find CommonJS require, got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_dynamic_import_has_correct_kind() {
    use tsz::module_resolver::ImportKind;
    let text = r#"import("./foo").then(x => x);"#;
    let file_name = "test.mts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);
    let dynamic_imports: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::DynamicImport)
        .collect();
    assert_eq!(
        dynamic_imports.len(),
        1,
        "Should find exactly one DynamicImport, got: {specifiers:?}"
    );
    assert_eq!(dynamic_imports[0].0, "./foo");
}

#[test]
fn test_collect_module_specifiers_mixed_import_kinds() {
    use tsz::module_resolver::ImportKind;
    let text = r#"
import { foo } from "./static-import";
import("./dynamic-import");
export { bar } from "./re-export";
"#;
    let file_name = "test.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    let static_imports: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmImport)
        .map(|(s, _, _, _)| s.as_str())
        .collect();
    assert!(
        static_imports.contains(&"./static-import"),
        "Should find static import, got: {static_imports:?}"
    );

    let dynamic_imports: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::DynamicImport)
        .map(|(s, _, _, _)| s.as_str())
        .collect();
    assert!(
        dynamic_imports.contains(&"./dynamic-import"),
        "Should find dynamic import, got: {dynamic_imports:?}"
    );

    let re_exports: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmReExport)
        .map(|(s, _, _, _)| s.as_str())
        .collect();
    assert!(
        re_exports.contains(&"./re-export"),
        "Should find re-export, got: {re_exports:?}"
    );
}

#[test]
fn test_collect_module_specifiers_extracts_resolution_mode_override() {
    use tsz::module_resolver::ImportingModuleKind;

    let text = r#"import type { Foo } from "pkg" with { "resolution-mode": "import" };"#;
    let file_name = "test.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    assert_eq!(
        specifiers.len(),
        1,
        "Expected exactly one import: {specifiers:?}"
    );
    assert_eq!(specifiers[0].0, "pkg");
    assert_eq!(specifiers[0].3, Some(ImportingModuleKind::Esm));
}

#[test]
fn test_collect_module_specifiers_finds_import_type_dependencies() {
    use tsz::module_resolver::ImportKind;

    let text = r#"export type SomeType = import("./inner").SomeType;"#;
    let file_name = "index.d.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    let import_types: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmImport)
        .map(|(s, _, _, _)| s.as_str())
        .collect();

    assert!(
        import_types.contains(&"./inner"),
        "Should find import type dependency './inner', got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_extracts_import_type_resolution_mode_override() {
    use tsz::module_resolver::{ImportKind, ImportingModuleKind};

    let text =
        r#"export type SomeType = import("pkg", { with: { "resolution-mode": "require" } }).Foo;"#;
    let file_name = "index.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    let import_types: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmImport)
        .collect();

    assert_eq!(
        import_types.len(),
        1,
        "Expected one import type, got: {specifiers:?}"
    );
    assert_eq!(import_types[0].0, "pkg");
    assert_eq!(import_types[0].3, Some(ImportingModuleKind::CommonJs));
}

#[test]
fn test_collect_module_specifiers_finds_typeof_import_dependencies() {
    use tsz::module_resolver::ImportKind;

    let text = r#"const parserRef: typeof import("csv-parse") = null as any;"#;
    let file_name = "index.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    let import_types: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmImport)
        .map(|(s, _, _, _)| s.as_str())
        .collect();

    assert!(
        import_types.contains(&"csv-parse"),
        "Should find bare typeof import dependency 'csv-parse', got: {specifiers:?}"
    );
}

#[test]
fn test_collect_module_specifiers_extracts_typeof_import_resolution_mode_override() {
    use tsz::module_resolver::{ImportKind, ImportingModuleKind};

    let text = r#"type Parser = typeof import("pkg", { with: { "resolution-mode": "require" } });"#;
    let file_name = "index.ts".to_string();
    let mut parser = tsz::parser::ParserState::new(file_name, text.to_string());
    let source_file = parser.parse_source_file();
    let (arena, _diagnostics) = parser.into_parts();
    let specifiers = collect_module_specifiers(&arena, source_file);

    let import_types: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind, _)| *kind == ImportKind::EsmImport)
        .collect();

    assert_eq!(
        import_types.len(),
        1,
        "Expected one typeof import, got: {specifiers:?}"
    );
    assert_eq!(import_types[0].0, "pkg");
    assert_eq!(import_types[0].3, Some(ImportingModuleKind::CommonJs));
}

#[test]
fn test_resolve_type_package_entry_with_exports_map() {
    use std::fs;
    let dir = tempfile::TempDir::new().expect("temp dir creation should succeed in test");
    let pkg_dir = dir.path().join("node_modules/@types/foo");
    fs::create_dir_all(&pkg_dir).unwrap();

    fs::write(
        pkg_dir.join("package.json"),
        r#"{
                "name": "@types/foo",
                "version": "1.0.0",
                "exports": {
                    ".": {
                        "import": "./index.d.mts",
                        "require": "./index.d.cts"
                    }
                }
            }"#,
    )
    .unwrap();
    fs::write(pkg_dir.join("index.d.mts"), "export {};").unwrap();
    fs::write(pkg_dir.join("index.d.cts"), "export {};").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::ESNext,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::ESNext,
            ..Default::default()
        },
        ..Default::default()
    };

    let result = resolve_type_package_entry(&pkg_dir, &options);
    assert!(
        result.is_some(),
        "Should resolve type package entry via exports map"
    );
    let resolved = result.expect("resolution should succeed in test");
    assert!(
        resolved.to_string_lossy().contains("index.d.mts"),
        "Should resolve to index.d.mts (import condition), got: {}",
        resolved.display()
    );
}

#[test]
fn test_resolve_type_package_entry_node10_restricted_extensions() {
    use std::fs;
    let dir = tempfile::TempDir::new().expect("temp dir creation should succeed in test");
    let pkg_dir = dir.path().join("node_modules/@types/bar");
    fs::create_dir_all(&pkg_dir).unwrap();

    fs::write(
        pkg_dir.join("package.json"),
        r#"{ "name": "@types/bar", "version": "1.0.0" }"#,
    )
    .unwrap();
    fs::write(pkg_dir.join("index.d.mts"), "export {};").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        ..Default::default()
    };

    let result = resolve_type_package_entry(&pkg_dir, &options);
    assert!(
        result.is_none(),
        "Node10 should not resolve .d.mts files, got: {result:?}"
    );

    // Now add an index.d.ts - should be found
    fs::write(pkg_dir.join("index.d.ts"), "export {};").unwrap();
    let result = resolve_type_package_entry(&pkg_dir, &options);
    assert!(result.is_some(), "Node10 should resolve index.d.ts");
}

#[test]
fn test_resolve_type_package_entry_with_mode_require() {
    use std::fs;
    let dir = tempfile::TempDir::new().expect("temp dir creation should succeed in test");
    let pkg_dir = dir.path().join("node_modules/@types/foo");
    fs::create_dir_all(&pkg_dir).unwrap();

    fs::write(
        pkg_dir.join("package.json"),
        r#"{
                "name": "@types/foo",
                "version": "1.0.0",
                "exports": {
                    ".": {
                        "import": "./index.d.mts",
                        "require": "./index.d.cts"
                    }
                }
            }"#,
    )
    .unwrap();
    fs::write(pkg_dir.join("index.d.mts"), "export {};").unwrap();
    fs::write(pkg_dir.join("index.d.cts"), "export {};").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_exports: true,
        ..Default::default()
    };

    let result = resolve_type_package_entry_with_mode(&pkg_dir, "require", &options);
    assert!(result.is_some(), "Should resolve with require mode");
    let resolved = result.expect("resolution should succeed in test");
    assert!(
        resolved.to_string_lossy().contains("index.d.cts"),
        "Should resolve to index.d.cts (require condition), got: {}",
        resolved.display()
    );
}

#[test]
fn test_default_type_roots_walks_parent_directories() {
    use std::fs;

    let dir = tempfile::TempDir::new().expect("temp dir creation should succeed in test");
    let repo_root = dir.path();
    let app_dir = repo_root.join("packages").join("app");
    let local_types = app_dir.join("node_modules").join("@types");
    let parent_types = repo_root.join("node_modules").join("@types");

    fs::create_dir_all(&local_types).unwrap();
    fs::create_dir_all(&parent_types).unwrap();

    let roots = default_type_roots(&app_dir);
    let local_canonical = canonicalize_or_owned(&local_types);
    let parent_canonical = canonicalize_or_owned(&parent_types);

    assert_eq!(
        roots.first(),
        Some(&local_canonical),
        "Nearest @types root should come first"
    );
    assert!(
        roots.contains(&parent_canonical),
        "Should include parent @types root"
    );
}

#[test]
fn test_resolve_module_specifier_classic_path_mapping_falls_back_to_root() {
    let mut raw_paths = FxHashMap::default();
    raw_paths.insert(
        "*".to_string(),
        vec!["*".to_string(), "generated/*".to_string()],
    );
    let compiler_options = CompilerOptions {
        base_url: Some("c:/root".to_string()),
        paths: Some(raw_paths),
        module: Some("amd".to_string()),
        ..Default::default()
    };
    let options =
        resolve_compiler_options(Some(&compiler_options)).expect("resolve compiler options");
    tracing::debug!(
        "resolved options: base_url={:?} paths={:?} resolution={:?}",
        options.base_url,
        options
            .paths
            .as_ref()
            .map(|paths| paths.iter().map(|m| m.pattern.clone()).collect::<Vec<_>>()),
        options.effective_module_resolution()
    );

    let base = PathBuf::from("/tmp/tsz-test-absolute");
    let mut known_files: FxHashSet<PathBuf> = FxHashSet::default();
    known_files.insert(base.join("c:/root/folder2/file1.ts"));
    known_files.insert(base.join("c:/root/generated/folder3/file2.ts"));
    known_files.insert(base.join("c:/root/shared/components/file3.ts"));
    known_files.insert(base.join("c:/file4.ts"));
    known_files.insert(base.join("c:/root/folder1/file1.ts"));

    let mut cache = ModuleResolutionCache::default();
    let resolved = resolve_module_specifier(
        &base.join("c:/root/folder1/file1.ts"),
        "file4",
        &options,
        &base,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved,
        Some(base.join("c:/file4.ts")),
        "classic path-mapping fallback should resolve file4 to c:/file4.ts"
    );
}

#[test]
fn test_resolve_module_specifier_classic_path_mapping_absolute_target_fallback() {
    let mut raw_paths = FxHashMap::default();
    raw_paths.insert(
        "*".to_string(),
        vec!["*".to_string(), "c:/shared/*".to_string()],
    );
    raw_paths.insert(
        "templates/*".to_string(),
        vec!["generated/src/templates/*".to_string()],
    );

    let compiler_options = CompilerOptions {
        base_url: Some("c:/root/src".to_string()),
        paths: Some(raw_paths),
        module: Some("amd".to_string()),
        ..Default::default()
    };
    let options =
        resolve_compiler_options(Some(&compiler_options)).expect("resolve compiler options");

    let mut known_files: FxHashSet<PathBuf> = FxHashSet::default();
    known_files.insert(PathBuf::from("c:/root/src/file3.d.ts"));
    known_files.insert(PathBuf::from("c:/shared/module1.d.ts"));
    known_files.insert(PathBuf::from("c:/root/generated/src/templates/module2.ts"));
    known_files.insert(PathBuf::from("c:/module3.d.ts"));
    known_files.insert(PathBuf::from("c:/root/src/file1.ts"));
    known_files.insert(PathBuf::from("c:/root/generated/src/project/file2.ts"));

    let mut cache = ModuleResolutionCache::default();
    let resolved = resolve_module_specifier(
        &PathBuf::from("c:/root/src/file1.ts"),
        "module3",
        &options,
        &PathBuf::from("c:/root/src"),
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved,
        Some(PathBuf::from("c:/module3.d.ts")),
        "absolute path mapping fallback should prefer shared module declarations"
    );
}

#[test]
fn test_exports_blocks_subpath_resolution() {
    use std::fs;
    let dir = tempfile::TempDir::new().expect("temp dir creation should succeed in test");
    let dir = dir.path();
    let pkg_dir = dir.join("node_modules/inner");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    // Package has exports map — only root "." is exported
    fs::write(
        pkg_dir.join("package.json"),
        r#"{"name":"inner","type":"module","exports":{".":{"types":"./index.d.ts","default":"./index.js"}}}"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.join("index.d.ts"),
        "export declare function x(): void;",
    )
    .unwrap();
    // "other.d.ts" exists on disk but is NOT in the exports map
    fs::write(pkg_dir.join("other.d.ts"), "export interface Thing {}").unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import { Thing } from 'inner/other';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();

    // Subpath "inner/other" should NOT resolve because exports blocks it
    let resolved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "inner/other",
        &options,
        dir,
        &mut cache,
        &known_files,
    );
    assert!(
        resolved.is_none(),
        "exports field should block subpath 'inner/other' even though other.d.ts exists on disk"
    );

    // Root import "inner" should still resolve
    let resolved_root = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "inner",
        &options,
        dir,
        &mut cache,
        &known_files,
    );
    assert!(
        resolved_root.is_some(),
        "root import 'inner' should still resolve via exports"
    );
}

#[test]
fn test_exports_directory_slash_pattern_resolves() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_exports_directory_slash_pattern");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/inner")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    // Package has directory-slash exports pattern
    fs::write(
        dir.join("node_modules/inner/package.json"),
        r#"{"name":"inner","exports":{"./":"./"}}"#,
    )
    .unwrap();
    fs::write(dir.join("node_modules/inner/index.d.ts"), "export {};").unwrap();
    fs::write(
        dir.join("node_modules/inner/other.d.ts"),
        "export interface Thing {}",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import { Thing } from 'inner/other.d.ts';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();

    // Import with explicit extension through directory pattern should resolve
    let resolved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "inner/other.d.ts",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    assert!(
        resolved.is_some(),
        "subpath 'inner/other.d.ts' should resolve through './' directory pattern"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_exports_versioned_types_condition_resolves() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_exports_versioned_types_condition");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/inner")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    // Package has versioned types conditions in exports:
    // - types@>=10000 → future types (should NOT match, version too high)
    // - types@>=1 → new types (SHOULD match, our version >= 1)
    // - types → old types (fallback, should NOT be reached)
    fs::write(
        dir.join("node_modules/inner/package.json"),
        r#"{
            "name": "inner",
            "exports": {
                ".": {
                    "types@>=10000": "./future-types.d.ts",
                    "types@>=1": "./new-types.d.ts",
                    "types": "./old-types.d.ts",
                    "import": "./index.mjs",
                    "node": "./index.js"
                }
            }
        }"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/old-types.d.ts"),
        "export const noVersionApplied = true;",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/new-types.d.ts"),
        "export const correctVersionApplied = true;",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/future-types.d.ts"),
        "export const futureVersionApplied = true;",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import * as mod from 'inner';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "inner",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    let resolved_path = resolved.expect("should resolve 'inner' via versioned types condition");
    assert!(
        resolved_path.ends_with("new-types.d.ts"),
        "should resolve to new-types.d.ts (types@>=1), got: {resolved_path:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_self_name_resolution_remaps_declaration_output_to_source() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_driver_resolution_self_name_outdir");
    let package_dir = dir.join("pkg");
    let src_dir = package_dir.join("src");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(
        package_dir.join("package.json"),
        r#"{
            "name":"@this/package",
            "type":"module",
            "exports": {
                ".": {
                    "default": "./dist/index.js",
                    "types": "./types/index.d.ts"
                }
            }
        }"#,
    )
    .unwrap();
    fs::write(
        src_dir.join("index.ts"),
        "import * as me from '@this/package';\nme;\n",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_exports: true,
        root_dir: Some(src_dir.clone()),
        out_dir: Some(package_dir.join("dist")),
        declaration_dir: Some(package_dir.join("types")),
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::NodeNext,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::NodeNext,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &src_dir.join("index.ts"),
        "@this/package",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved,
        Some(canonicalize_or_owned(&src_dir.join("index.ts"))),
        "self-name package exports should remap output targets back to the source file"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_self_name_resolution_remaps_virtual_absolute_output_paths() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_driver_resolution_self_name_virtual_abs");
    let package_dir = dir.join("pkg");
    let src_dir = package_dir.join("src");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(
        package_dir.join("package.json"),
        r#"{
            "name":"@this/package",
            "type":"module",
            "exports": {
                ".": {
                    "default": "./dist/index.js",
                    "types": "./types/index.d.ts"
                }
            }
        }"#,
    )
    .unwrap();
    fs::write(
        src_dir.join("index.ts"),
        "import * as me from '@this/package';\nme;\n",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_exports: true,
        root_dir: Some(PathBuf::from("/pkg/src")),
        out_dir: Some(PathBuf::from("/pkg/dist")),
        declaration_dir: Some(PathBuf::from("/pkg/types")),
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::NodeNext,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::NodeNext,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &src_dir.join("index.ts"),
        "@this/package",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved,
        Some(canonicalize_or_owned(&src_dir.join("index.ts"))),
        "virtual absolute rootDir/outDir/declarationDir should remap export targets back to source files"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_self_name_resolution_remaps_virtual_absolute_output_paths_from_package_root() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_driver_resolution_self_name_virtual_abs_pkg_root");
    let package_dir = dir.join("pkg");
    let src_dir = package_dir.join("src");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(
        package_dir.join("package.json"),
        r#"{
            "name":"@this/package",
            "type":"module",
            "exports": {
                ".": {
                    "default": "./dist/index.js",
                    "types": "./types/index.d.ts"
                }
            }
        }"#,
    )
    .unwrap();
    fs::write(
        src_dir.join("index.ts"),
        "import * as me from '@this/package';\nme;\n",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_exports: true,
        root_dir: Some(PathBuf::from("/pkg/src")),
        out_dir: Some(PathBuf::from("/pkg/dist")),
        declaration_dir: Some(PathBuf::from("/pkg/types")),
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::NodeNext,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::NodeNext,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &src_dir.join("index.ts"),
        "@this/package",
        &options,
        &package_dir,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved,
        Some(canonicalize_or_owned(&src_dir.join("index.ts"))),
        "virtual absolute self-name remap should work when the project base dir is the package root"
    );

    let _ = fs::remove_dir_all(&dir);
}
