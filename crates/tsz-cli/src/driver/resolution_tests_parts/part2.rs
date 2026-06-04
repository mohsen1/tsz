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
fn test_resolve_module_specifier_paths_without_base_url_use_project_base() {
    let mut raw_paths = FxHashMap::default();
    raw_paths.insert("foo/*".to_string(), vec!["./dist/*".to_string()]);
    raw_paths.insert("baz/*.ts".to_string(), vec!["./types/*.d.ts".to_string()]);
    let compiler_options = CompilerOptions {
        paths: Some(raw_paths),
        module_resolution: Some("bundler".to_string()),
        module: Some("es2015".to_string()),
        ..Default::default()
    };
    let options =
        resolve_compiler_options(Some(&compiler_options)).expect("resolve compiler options");

    let base = PathBuf::from("/tmp/tsz-test-paths-without-baseurl");
    let mut known_files: FxHashSet<PathBuf> = FxHashSet::default();
    known_files.insert(base.join("dist/bar.ts"));
    known_files.insert(base.join("types/main.d.ts"));

    let mut cache = ModuleResolutionCache::default();
    let foo = resolve_module_specifier(
        &base.join("test.ts"),
        "foo/bar.ts",
        &options,
        &base,
        &mut cache,
        &known_files,
    );
    assert_eq!(foo, Some(base.join("dist/bar.ts")));

    let baz = resolve_module_specifier(
        &base.join("test.ts"),
        "baz/main.ts",
        &options,
        &base,
        &mut cache,
        &known_files,
    );
    assert_eq!(baz, Some(base.join("types/main.d.ts")));
}

#[test]
fn test_path_mapping_selection_cache_preserves_sorted_precedence() {
    let mut raw_paths = FxHashMap::default();
    raw_paths.insert("*".to_string(), vec!["fallback/*".to_string()]);
    raw_paths.insert("@scope/pkg/*".to_string(), vec!["wildcard/*".to_string()]);
    raw_paths.insert(
        "@scope/pkg/foo".to_string(),
        vec!["exact/foo.ts".to_string()],
    );
    for i in 0..64 {
        raw_paths.insert(format!("@scope/pkg-{i}/*"), vec![format!("pkg-{i}/*")]);
    }

    let compiler_options = CompilerOptions {
        paths: Some(raw_paths),
        module_resolution: Some("bundler".to_string()),
        module: Some("es2015".to_string()),
        ..Default::default()
    };
    let options =
        resolve_compiler_options(Some(&compiler_options)).expect("resolve compiler options");

    let base = PathBuf::from("/tmp/tsz-test-path-mapping-cache");
    let mut known_files: FxHashSet<PathBuf> = FxHashSet::default();
    known_files.insert(base.join("exact/foo.ts"));
    known_files.insert(base.join("wildcard/foo.ts"));
    known_files.insert(base.join("fallback/@scope/pkg/foo.ts"));

    let mut cache = ModuleResolutionCache::default();
    let resolved = resolve_module_specifier(
        &base.join("src/main.ts"),
        "@scope/pkg/foo",
        &options,
        &base,
        &mut cache,
        &known_files,
    );

    assert_eq!(resolved, Some(base.join("exact/foo.ts")));
    let cached = cache
        .path_mapping_by_specifier
        .get("@scope/pkg/foo")
        .and_then(Option::as_ref)
        .expect("path mapping selection should be cached");
    assert_eq!(
        options.paths.as_ref().unwrap()[cached.0].pattern,
        "@scope/pkg/foo",
        "exact mapping should win over wildcard mappings before caching"
    );

    let resolved_again = resolve_module_specifier(
        &base.join("src/other.ts"),
        "@scope/pkg/foo",
        &options,
        &base,
        &mut cache,
        &known_files,
    );
    assert_eq!(resolved_again, Some(base.join("exact/foo.ts")));
    assert_eq!(cache.path_mapping_by_specifier.len(), 1);
}

#[test]
fn test_resolve_module_specifier_root_dirs_overlay() {
    let base = PathBuf::from("/tmp/tsz-test-rootdirs");
    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        root_dirs: vec![base.join("src"), base.join("generated")],
        ..Default::default()
    };

    let mut known_files = FxHashSet::default();
    known_files.insert(base.join("generated/generated.ts"));
    let mut cache = ModuleResolutionCache::default();

    let resolved = resolve_module_specifier(
        &base.join("src/main.ts"),
        "./generated",
        &options,
        &base,
        &mut cache,
        &known_files,
    );

    assert_eq!(resolved, Some(base.join("generated/generated.ts")));
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
