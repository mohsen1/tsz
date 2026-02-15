use super::*;
use rustc_hash::FxHashSet;
use tsz::emitter::ModuleKind;

#[test]
fn test_exports_js_target_does_not_substitute_dts() {
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

    assert!(
        resolved.is_none(),
        "exports target with .js should not substitute to .d.ts"
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
        .filter(|(_, _, kind)| *kind == ImportKind::DynamicImport)
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
        .filter(|(_, _, kind)| *kind == ImportKind::EsmImport)
        .map(|(s, _, _)| s.as_str())
        .collect();
    assert!(
        static_imports.contains(&"./static-import"),
        "Should find static import, got: {static_imports:?}"
    );

    let dynamic_imports: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind)| *kind == ImportKind::DynamicImport)
        .map(|(s, _, _)| s.as_str())
        .collect();
    assert!(
        dynamic_imports.contains(&"./dynamic-import"),
        "Should find dynamic import, got: {dynamic_imports:?}"
    );

    let re_exports: Vec<_> = specifiers
        .iter()
        .filter(|(_, _, kind)| *kind == ImportKind::EsmReExport)
        .map(|(s, _, _)| s.as_str())
        .collect();
    assert!(
        re_exports.contains(&"./re-export"),
        "Should find re-export, got: {re_exports:?}"
    );
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
