//! Node16 Modes tests for `module_resolver`.
//!
//! Tests for Node16 / NodeNext / Node10 specific resolution rules:
//!
//! - ESM vs CJS package subpath / directory index behavior
//! - Extension-aware diagnostic upgrades
//! - CJS-require vs dynamic-import cache separation
//! - Node10 default for `package.json#imports`

use super::super::*;

#[test]
fn test_node16_pattern_exports_resolves_with_dts() {
    // Pattern exports like "./cjs/*": "./*.cjs" should resolve when
    // the wildcard-substituted path has a corresponding .d.cts file.
    // This tests the fix: wildcard substitution must happen BEFORE
    // try_export_target, not after.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_pattern_exports_resolve");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/inner")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    // Package with pattern exports
    fs::write(
        dir.join("node_modules/inner/package.json"),
        r#"{"name":"inner","exports":{"./cjs/*":"./*.cjs","./mjs/*":"./*.mjs"}}"#,
    )
    .unwrap();
    // Declaration files that should be found via extension substitution
    fs::write(
        dir.join("node_modules/inner/index.d.cts"),
        "export declare const cjsSource: boolean;",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/index.d.mts"),
        "export declare const mjsSource: boolean;",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import * as x from 'inner/cjs/index';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };

    let mut resolver = ModuleResolver::new(&options);

    // ./cjs/* pattern matches ./cjs/index, wildcard = "index"
    // Target ./*.cjs becomes ./index.cjs after substitution
    // Extension substitution: index.cjs -> index.d.cts (exists)
    let result = resolver.resolve(
        "inner/cjs/index",
        &dir.join("src/index.ts"),
        Span::new(24, 40),
    );
    assert!(
        result.is_ok(),
        "Pattern export ./cjs/* should resolve via .d.cts: {:?}",
        result.err()
    );

    // Also test ./mjs/* pattern
    let result = resolver.resolve(
        "inner/mjs/index",
        &dir.join("src/index.ts"),
        Span::new(24, 40),
    );
    assert!(
        result.is_ok(),
        "Pattern export ./mjs/* should resolve via .d.mts: {:?}",
        result.err()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_nodenext_bare_package_index_fallback_rejects_mts_entry() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_nodenext_pkg_index_mts_fallback");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();

    fs::write(dir.join("src/index.ts"), "import { value } from 'pkg';").unwrap();
    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"type":"module"}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/index.mts"),
        "export const value = 1;",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("pkg", &dir.join("src/index.ts"), Span::new(22, 27));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "NodeNext bare package fallback must not resolve index.mts, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_esm_package_no_directory_index_for_subpath() {
    // In Node16/NodeNext, ESM packages (type: "module") without an exports
    // field should NOT resolve subpaths through directory index (e.g.,
    // pkg/dist/dir → pkg/dist/dir/index.d.ts). Node.js ESM does not
    // support directory index resolution.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_esm_no_dir_index");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // Root package.json
    fs::write(dir.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(dir.join("index.ts"), "import 'test-pkg/dist/dir';").unwrap();

    // Package without exports field
    let pkg_dir = dir.join("node_modules/test-pkg");
    fs::create_dir_all(pkg_dir.join("dist/dir")).unwrap();
    fs::write(
        pkg_dir.join("package.json"),
        r#"{"name":"test-pkg","type":"module","main":"dist/index.js"}"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.join("dist/index.d.ts"),
        "export declare const a: number;",
    )
    .unwrap();
    fs::write(
        pkg_dir.join("dist/dir/index.d.ts"),
        "export declare const b: number;",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    // Should NOT resolve through directory index in ESM package
    let result = resolver.resolve_with_kind(
        "test-pkg/dist/dir",
        &dir.join("index.ts"),
        Span::new(8, 26),
        ImportKind::EsmImport,
    );

    assert!(
        result.is_err(),
        "ESM package subpath should not resolve through directory index: {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_cjs_package_allows_directory_index_for_subpath() {
    // CJS packages (no "type": "module") should still allow directory
    // index resolution for subpaths, even in Node16/NodeNext mode.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_cjs_allows_dir_index");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(dir.join("index.ts"), "import 'test-cjs/lib/sub';").unwrap();

    let pkg_dir = dir.join("node_modules/test-cjs");
    fs::create_dir_all(pkg_dir.join("lib/sub")).unwrap();
    // CJS package (no "type" field → defaults to commonjs)
    fs::write(
        pkg_dir.join("package.json"),
        r#"{"name":"test-cjs","main":"lib/index.js"}"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.join("lib/index.d.ts"),
        "export declare const a: number;",
    )
    .unwrap();
    fs::write(
        pkg_dir.join("lib/sub/index.d.ts"),
        "export declare const b: number;",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    // CJS package subpath SHOULD resolve through directory index
    let result = resolver.resolve_with_kind(
        "test-cjs/lib/sub",
        &dir.join("index.ts"),
        Span::new(8, 25),
        ImportKind::EsmImport,
    );

    assert!(
        result.is_ok(),
        "CJS package subpath should resolve through directory index: {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_bare_directory_specifier_emits_ts2307_not_ts2834() {
    // In Node16/NodeNext, `./` and `../` specifiers that resolve via directory
    // index should emit TS2307 (Cannot find module), not TS2834/TS2835, because
    // there is no filename component to attach an extension suggestion to.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_node16_bare_dir_ts2307");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("sub")).unwrap();

    fs::write(dir.join("index.mts"), "").unwrap();
    fs::write(dir.join("sub/index.mts"), "export const x = 1;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    // `./` in ESM file → TS2307 (bare directory, has no filename to suggest)
    let result = resolver.resolve("./", &dir.join("index.mts"), Span::new(0, 2));
    match result {
        Err(ResolutionFailure::NotFound { .. }) => {} // expected TS2307
        Err(ResolutionFailure::ImportPathNeedsExtension { .. }) => {
            panic!("Bare './' should emit TS2307, not TS2834/TS2835");
        }
        other => panic!("Expected TS2307, got {:?}", other.err()),
    }

    // `./sub/` in ESM file → TS2834 (directory with index, no filename to suggest)
    let result = resolver.resolve("./sub/", &dir.join("index.mts"), Span::new(0, 2));
    match result {
        Err(ResolutionFailure::ImportPathNeedsExtension {
            suggested_extension,
            ..
        }) => {
            assert!(
                suggested_extension.is_empty(),
                "Directory './sub/' resolves via index, should have no suggestion"
            );
        }
        other => panic!("Expected TS2834, got {:?}", other.err()),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_direct_index_file_gets_extension_suggestion() {
    // `./sub/index` should emit TS2835 with a suggested extension when it
    // resolves directly to `sub/index.mts` (direct file, not directory index).
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_node16_direct_index_sugg");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("sub")).unwrap();

    fs::write(dir.join("index.mts"), "").unwrap();
    fs::write(dir.join("sub/index.mts"), "export const x = 1;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let result = resolver.resolve("./sub/index", &dir.join("index.mts"), Span::new(0, 2));
    match result {
        Err(ResolutionFailure::ImportPathNeedsExtension {
            suggested_extension,
            ..
        }) => {
            assert!(
                !suggested_extension.is_empty(),
                "Direct './sub/index' should have an extension suggestion"
            );
        }
        other => panic!("Expected TS2835 with suggestion, got {:?}", other.err()),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_module_suffix_index_file_emits_ts2834_without_suggestion() {
    // `./pkg` resolves through `pkg/index.native.ts` via moduleSuffixes. Adding
    // `./pkg.js` would not reach that file, so this must be TS2834 rather than
    // TS2835 with a suggested extension.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_node16_modulesuffix_index_ts2834");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("pkg")).unwrap();

    fs::write(dir.join("main.mts"), "import { x } from './pkg';").unwrap();
    fs::write(dir.join("pkg/index.native.ts"), "export const x = 1;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![".native".to_string(), String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let result = resolver.resolve("./pkg", &dir.join("main.mts"), Span::new(0, 7));
    let failure = result.expect_err("Expected TS2834 without suggestion");
    match &failure {
        ResolutionFailure::ImportPathNeedsExtension {
            suggested_extension,
            ..
        } => {
            assert!(
                suggested_extension.is_empty(),
                "Module-suffix directory index should not get a suggestion"
            );
        }
        other => panic!("Expected TS2834 without suggestion, got {other:?}"),
    }
    assert_eq!(failure.to_diagnostic().code, IMPORT_PATH_NEEDS_EXTENSION);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_cache_separates_dynamic_import_from_cjs_require() {
    // The same extensionless specifier in the same ESM file can be illegal for
    // dynamic import() but legal for require-style resolution. The resolver
    // cache must include ImportKind so those requests do not poison each other.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_node16_cache_import_kind_dynamic_first");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.mts"), "").unwrap();
    fs::write(dir.join("target.mts"), "export const x = 1;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let containing_file = dir.join("main.mts");

    let dynamic_result = resolver.resolve_with_kind(
        "./target",
        &containing_file,
        Span::new(0, 10),
        ImportKind::DynamicImport,
    );
    match dynamic_result {
        Err(ResolutionFailure::ImportPathNeedsExtension {
            suggested_extension,
            ..
        }) => assert_eq!(suggested_extension, ".mjs"),
        other => panic!("Dynamic import should require an extension, got {other:?}"),
    }

    let require_result = resolver.resolve_with_kind(
        "./target",
        &containing_file,
        Span::new(20, 30),
        ImportKind::CjsRequire,
    );
    assert!(
        require_result.is_ok(),
        "CJS require-style resolution should not reuse the dynamic-import error: {require_result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_cache_separates_cjs_require_from_dynamic_import() {
    // Verify the reverse order too: a successful require-style lookup must not
    // make a later dynamic import lookup skip Node ESM extension validation.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_node16_cache_import_kind_require_first");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.mts"), "").unwrap();
    fs::write(dir.join("target.mts"), "export const x = 1;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let containing_file = dir.join("main.mts");

    let require_result = resolver.resolve_with_kind(
        "./target",
        &containing_file,
        Span::new(0, 10),
        ImportKind::CjsRequire,
    );
    assert!(
        require_result.is_ok(),
        "CJS require-style resolution should resolve without extension validation: {require_result:?}"
    );

    let dynamic_result = resolver.resolve_with_kind(
        "./target",
        &containing_file,
        Span::new(20, 30),
        ImportKind::DynamicImport,
    );
    match dynamic_result {
        Err(ResolutionFailure::ImportPathNeedsExtension {
            suggested_extension,
            ..
        }) => assert_eq!(suggested_extension, ".mjs"),
        other => panic!("Dynamic import should not reuse require-style success, got {other:?}"),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_cjs_require_uses_target_package_scope_extension_priority() {
    // Regression test for nodeModules1.ts. A require-like relative lookup from
    // a module package still uses Node's require resolution, but the extension
    // priority is based on the target package scope. That makes module package
    // targets resolve to ESM files while commonjs/default package targets keep
    // their CJS candidates ahead of .mts.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_node16_cjs_target_package_scope");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("subfolder")).unwrap();
    fs::create_dir_all(dir.join("subfolder2/another")).unwrap();

    fs::write(dir.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(dir.join("index.mts"), "export const x = 1;").unwrap();
    fs::write(dir.join("index.cts"), "export const x = 1;").unwrap();

    fs::write(dir.join("subfolder/package.json"), r#"{"type":"commonjs"}"#).unwrap();
    fs::write(dir.join("subfolder/index.mts"), "export const x = 1;").unwrap();
    fs::write(dir.join("subfolder/index.cts"), "export const x = 1;").unwrap();

    fs::write(dir.join("subfolder2/package.json"), "{}").unwrap();
    fs::write(dir.join("subfolder2/index.mts"), "export const x = 1;").unwrap();
    fs::write(dir.join("subfolder2/index.cts"), "export const x = 1;").unwrap();

    fs::write(
        dir.join("subfolder2/another/package.json"),
        r#"{"type":"module"}"#,
    )
    .unwrap();
    fs::write(
        dir.join("subfolder2/another/index.mts"),
        "export const x = 1;",
    )
    .unwrap();
    fs::write(
        dir.join("subfolder2/another/index.cts"),
        "export const x = 1;",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let containing_file = dir.join("index.mts");

    let resolve_with_kind = |resolver: &mut ModuleResolver,
                             containing_file: &std::path::Path,
                             specifier: &str,
                             import_kind: ImportKind| {
        resolver
            .resolve_with_kind(
                specifier,
                containing_file,
                Span::new(0, specifier.len() as u32),
                import_kind,
            )
            .unwrap_or_else(|err| panic!("expected {specifier} to resolve, got {err:?}"))
            .resolved_path
    };
    let resolve_require = |resolver: &mut ModuleResolver, specifier: &str| {
        resolve_with_kind(
            resolver,
            &containing_file,
            specifier,
            ImportKind::CjsRequire,
        )
    };

    assert_eq!(resolve_require(&mut resolver, "./"), dir.join("index.mts"));
    assert_eq!(
        resolve_require(&mut resolver, "./subfolder"),
        dir.join("subfolder/index.cts")
    );
    assert_eq!(
        resolve_require(&mut resolver, "./subfolder2"),
        dir.join("subfolder2/index.cts")
    );
    assert_eq!(
        resolve_require(&mut resolver, "./subfolder2/another"),
        dir.join("subfolder2/another/index.mts")
    );

    let containing_cjs_file = dir.join("index.cts");
    let resolve_cjs_static_import = |resolver: &mut ModuleResolver, specifier: &str| {
        resolve_with_kind(
            resolver,
            &containing_cjs_file,
            specifier,
            ImportKind::EsmImport,
        )
    };

    assert_eq!(
        resolve_cjs_static_import(&mut resolver, "./"),
        dir.join("index.mts")
    );
    assert_eq!(
        resolve_cjs_static_import(&mut resolver, "./subfolder"),
        dir.join("subfolder/index.cts")
    );
    assert_eq!(
        resolve_cjs_static_import(&mut resolver, "./subfolder2/another"),
        dir.join("subfolder2/another/index.mts")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node10_does_not_resolve_package_imports_by_default() {
    // Per tsc 6.0, legacy `moduleResolution: "node"` (a.k.a. node10) does NOT
    // resolve `package.json#imports` unless `resolvePackageJsonImports` is
    // explicitly set to true. tsz must mirror that default.
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_node10_imports_default_off");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("package.json"),
        r##"{"name":"app","imports":{"#mapped":"./src/mapped.d.ts"}}"##,
    )
    .unwrap();
    fs::write(
        dir.join("src/mapped.d.ts"),
        "export declare const value: 1;",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import { value } from '#mapped';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        // Intentionally not setting `resolve_package_json_imports`; default
        // for Node should be false in tsc 6.0.
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("#mapped", &dir.join("src/index.ts"), Span::new(22, 30));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "node10 must not resolve `#mapped` via package.json#imports by default, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node10_resolves_package_imports_when_explicitly_enabled() {
    // When the user opts in via `resolvePackageJsonImports: true`, the
    // imports field should be honored even under node10.
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_node10_imports_explicit_on");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("package.json"),
        r##"{"name":"app","imports":{"#mapped":"./src/mapped.d.ts"}}"##,
    )
    .unwrap();
    fs::write(
        dir.join("src/mapped.d.ts"),
        "export declare const value: 1;",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import { value } from '#mapped';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let resolved = resolver
        .resolve("#mapped", &dir.join("src/index.ts"), Span::new(22, 30))
        .expect("explicit opt-in should resolve `#mapped`");
    assert_eq!(resolved.resolved_path, dir.join("src/mapped.d.ts"));

    let _ = fs::remove_dir_all(&dir);
}
