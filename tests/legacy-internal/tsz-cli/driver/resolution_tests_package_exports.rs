use super::*;

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
