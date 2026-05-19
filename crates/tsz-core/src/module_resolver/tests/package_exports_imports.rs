//! Package Exports Imports tests for `module_resolver`.
//!
//! Tests for the `package.json#exports` / `#imports` algorithms:
//!
//! - Pattern exports (wildcard `*` keys, declaration sidecars)
//! - Conditional resolution and ordered fallback
//! - Versioned types branches and `typesVersions` selectors
//! - Self-reference exports
//! - Target validation (parent-escape, `node_modules` segment,
//!   absolute targets, bare-imports validity)

use super::super::*;

#[test]
fn test_resolver_rejects_root_slash_package_import_with_wildcard() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_package_import_root_slash");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("package.json"),
        r##"{
            "name": "package",
            "private": true,
            "imports": {
                "#/*": "./src/*"
            }
        }"##,
    )
    .unwrap();
    fs::write(dir.join("src/foo.ts"), "export const foo = 'foo';").unwrap();
    fs::write(dir.join("index.ts"), "import { foo } from '#/foo.js'; foo;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("#/foo.js", &dir.join("index.ts"), Span::new(0, 8));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "Expected #/foo.js to be rejected as an invalid package import specifier, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_imports_pattern_key_is_not_treated_as_exact_match_for_literal_star_specifier() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_imports_literal_star_specifier");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("package.json"),
        r##"{
            "name": "package",
            "private": true,
            "imports": {
                "#a/*/b/*": "./src/value.js"
            }
        }"##,
    )
    .unwrap();
    fs::write(
        dir.join("src/value.d.ts"),
        "export declare const v: number;",
    )
    .unwrap();
    fs::write(dir.join("index.ts"), "import { v } from '#a/*/b/*'; v;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("#a/*/b/*", &dir.join("index.ts"), Span::new(0, 10));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "Pattern imports key must not exact-match a literal-* specifier, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_imports_conditional_falls_back_after_missing_target() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_imports_conditional_missing_fallback");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(
        dir.join("package.json"),
        r##"{
            "name": "app",
            "imports": {
                "#x": {
                    "import": "./missing.d.ts",
                    "default": "./ok.d.ts"
                }
            }
        }"##,
    )
    .unwrap();
    fs::write(dir.join("ok.d.ts"), "export declare const v: number;").unwrap();
    fs::write(dir.join("index.ts"), "import { v } from '#x';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("#x", &dir.join("index.ts"), Span::new(0, 2));

    let resolved = result.expect("default condition should resolve after missing import target");
    assert_eq!(resolved.resolved_path, dir.join("ok.d.ts"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_imports_conditional_prefers_versioned_types_branch() {
    // Regression for https://github.com/mohsen1/tsz/issues/3564.
    //
    // The package.json#imports field supports the same conditional key syntax
    // as the exports field, including versioned `types@<range>` keys. tsc
    // honors the highest-matching versioned `types@...` branch before falling
    // back to the plain `types` key. Previously, the imports path matched
    // condition keys via simple equality, so `types@>=1` could never match
    // and the resolver fell through to `./old.d.ts`.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_imports_versioned_types_condition");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(
        dir.join("package.json"),
        r##"{
            "name": "app",
            "type": "module",
            "imports": {
                "#x": {
                    "types@>=1": "./new.d.ts",
                    "types": "./old.d.ts",
                    "default": "./x.js"
                }
            }
        }"##,
    )
    .unwrap();
    fs::write(
        dir.join("new.d.ts"),
        "export declare function onlyNew(): void;",
    )
    .unwrap();
    fs::write(
        dir.join("old.d.ts"),
        "export declare function onlyOld(): void;",
    )
    .unwrap();
    fs::write(dir.join("x.js"), "export function onlyNew() {}").unwrap();
    fs::write(
        dir.join("main.ts"),
        "import { onlyNew } from '#x'; onlyNew();",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("#x", &dir.join("main.ts"), Span::new(0, 2));

    let resolved =
        result.expect("versioned types@>=1 branch should resolve before plain types fallback");
    assert!(
        resolved.resolved_path.ends_with("new.d.ts"),
        "expected versioned types branch (new.d.ts), got {}",
        resolved.resolved_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_imports_versioned_types_skips_when_range_does_not_match() {
    // Companion to the above: when the compiler version is *below* the
    // declared `types@<range>` floor, the versioned branch must be skipped
    // and the plain `types` fallback must win.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_imports_versioned_types_skip");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(
        dir.join("package.json"),
        r##"{
            "name": "app",
            "type": "module",
            "imports": {
                "#x": {
                    "types@>=10000": "./future.d.ts",
                    "types": "./old.d.ts",
                    "default": "./x.js"
                }
            }
        }"##,
    )
    .unwrap();
    fs::write(
        dir.join("future.d.ts"),
        "export declare const future: number;",
    )
    .unwrap();
    fs::write(dir.join("old.d.ts"), "export declare const old: number;").unwrap();
    fs::write(dir.join("x.js"), "export const old = 1;").unwrap();
    fs::write(dir.join("main.ts"), "import { old } from '#x'; old;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("#x", &dir.join("main.ts"), Span::new(0, 2));

    let resolved = result.expect("plain types branch should win when versioned range mismatches");
    assert!(
        resolved.resolved_path.ends_with("old.d.ts"),
        "expected plain types fallback (old.d.ts), got {}",
        resolved.resolved_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_bundler_package_exports_apply_module_suffixes_to_declaration_sidecars() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_exports_module_suffixes_dts");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{"./foo":"./foo.js"}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/foo.native.d.ts"),
        "export declare const value: number;",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import { value } from 'pkg/foo';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        module_suffixes: vec![".native".to_string(), String::new()],
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let result = resolver
        .resolve("pkg/foo", &dir.join("src/index.ts"), Span::new(22, 29))
        .expect("package exports target should resolve through suffixed declaration sidecar");
    assert_eq!(
        result.resolved_path,
        dir.join("node_modules/pkg/foo.native.d.ts")
    );
    assert_eq!(result.extension, ModuleExtension::Dts);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_exports_pattern_key_is_not_treated_as_exact_match_for_literal_star_specifier() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_exports_literal_star_specifier");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/double-asterisk")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/double-asterisk/package.json"),
        r#"{
            "name":"double-asterisk",
            "exports":{"./a/*/b/*/c/*":"./example.js"}
        }"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/double-asterisk/example.d.ts"),
        "export {};",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import {} from 'double-asterisk/a/*/b/*/c/*';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve(
        "double-asterisk/a/*/b/*/c/*",
        &dir.join("src/index.ts"),
        Span::new(0, 28),
    );

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "Pattern exports key must not exact-match a literal-* specifier, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_exports_target_cannot_escape_package_root() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_exports_target_escape");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{"./leak":"../leak.d.ts"}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/leak.d.ts"),
        "export declare const value: number;",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import { value } from 'pkg/leak';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("pkg/leak", &dir.join("src/index.ts"), Span::new(0, 28));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "export target escaping the package root must not resolve, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_exports_target_cannot_contain_node_modules_segment() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_exports_target_node_modules");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg/node_modules")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{"./secret":"./node_modules/secret.d.ts"}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/node_modules/secret.d.ts"),
        "export declare const value: number;",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import { value } from 'pkg/secret';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("pkg/secret", &dir.join("src/index.ts"), Span::new(0, 31));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "export target containing node_modules must not resolve, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_imports_absolute_target_is_invalid() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_imports_absolute_target");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(dir.join("abs.d.ts"), "export declare const value: number;").unwrap();
    fs::write(
        dir.join("package.json"),
        serde_json::json!({
            "name": "app",
            "imports": {
                "#abs": dir.join("abs.d.ts").to_string_lossy()
            }
        })
        .to_string(),
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import { value } from '#abs';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("#abs", &dir.join("src/index.ts"), Span::new(0, 28));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "absolute imports target must not resolve, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_imports_target_cannot_contain_node_modules_segment() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_imports_target_node_modules");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("package.json"),
        r##"{"name":"app","imports":{"#secret":"./node_modules/secret.d.ts"}}"##,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/secret.d.ts"),
        "export declare const value: number;",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import { value } from '#secret';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("#secret", &dir.join("src/index.ts"), Span::new(0, 29));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "imports target containing node_modules must not resolve, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_exports_js_target_substitutes_dts() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_exports_js_target");
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
        ..Default::default()
    };

    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("pkg", &dir.join("src/index.ts"), Span::new(0, 3));

    // TypeScript resolves export targets with declaration substitution:
    // exports: "./entrypoint.js" → finds entrypoint.d.ts
    let resolved = result.expect("Expected exports .js target to resolve via .d.ts substitution");
    assert!(resolved.resolved_path.ends_with("entrypoint.d.ts"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_imports_exact_mapping_marks_ts_extension_usage_when_key_ends_with_ts() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_test_package_imports_exact_ts_usage");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("package.json"),
        r##"{
            "name": "pkg",
            "type": "module",
            "imports": {
                "#foo.ts": "./src/foo.ts"
            }
        }"##,
    )
    .unwrap();
    fs::write(dir.join("src/foo.ts"), "export {};").unwrap();
    fs::write(dir.join("index.ts"), "import {} from \"#foo.ts\";").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_imports: true,
        rewrite_relative_import_extensions: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let request = ModuleLookupRequest {
        specifier: "#foo.ts",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(0, 9),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let outcome = resolver
        .lookup(&request, |_, _| None, |_| false, None)
        .classify();
    assert!(outcome.resolved_path.is_some());
    // The exact key `#foo.ts` literally ends in `.ts`, so the package author
    // opted into the `.ts` mapping. Mirrors tsc's `resolvedUsingTsExtension`
    // and lets the checker's TS2877 gate suppress the rewrite warning.
    assert!(
        outcome.resolved_using_ts_extension,
        "exact package imports key ending in .ts should mark resolvedUsingTsExtension"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_imports_array_falls_back_after_missing_target() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_test_package_imports_array_fallback");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("package.json"),
        r##"{
            "name": "pkg",
            "type": "module",
            "imports": {
                "#x": ["./missing.d.ts", "./ok.d.ts"]
            }
        }"##,
    )
    .unwrap();
    fs::write(dir.join("ok.d.ts"), "export declare const value: 1;").unwrap();
    fs::write(dir.join("main.ts"), "import { value } from '#x'; value;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver
        .resolve("#x", &dir.join("main.ts"), Span::new(0, 3))
        .unwrap();

    assert_eq!(result.resolved_path, dir.join("ok.d.ts"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_imports_pattern_does_not_mark_ts_extension_when_key_lacks_ts_suffix() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_test_package_imports_pattern_ts_usage");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("internal")).unwrap();
    fs::write(
        dir.join("package.json"),
        r##"{
            "name": "pkg",
            "type": "module",
            "imports": {
                "#internal/*": "./internal/*"
            }
        }"##,
    )
    .unwrap();
    fs::write(dir.join("internal/foo.ts"), "export {};").unwrap();
    fs::write(dir.join("index.ts"), "import {} from \"#internal/foo.ts\";").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_imports: true,
        rewrite_relative_import_extensions: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let request = ModuleLookupRequest {
        specifier: "#internal/foo.ts",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(0, 18),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let outcome = resolver
        .lookup(&request, |_, _| None, |_| false, None)
        .classify();
    assert!(outcome.resolved_path.is_some());
    // Pattern key `#internal/*` does NOT end in `.ts`. The wildcard captured
    // `foo.ts` and substituted it into the target — the `.ts` was preserved
    // through to the resolved file rather than consumed by the package
    // author's mapping. That's exactly the situation TS2877 warns about, so
    // `resolvedUsingTsExtension` must be `false`.
    assert!(
        !outcome.resolved_using_ts_extension,
        "pattern imports key without .ts suffix must not mark resolvedUsingTsExtension"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_self_reference_exports_pattern_with_ts_key_marks_ts_extension_usage() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_test_self_reference_exports_ts_pattern");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("package.json"),
        r##"{
            "name": "pkg",
            "type": "module",
            "exports": {
                "./*.ts": { "source": "./*.ts", "default": "./*.js" }
            }
        }"##,
    )
    .unwrap();
    fs::write(dir.join("foo.ts"), "export {};").unwrap();
    fs::write(dir.join("index.ts"), "import {} from \"pkg/foo.ts\";").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_exports: true,
        rewrite_relative_import_extensions: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let request = ModuleLookupRequest {
        specifier: "pkg/foo.ts",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(0, 12),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let outcome = resolver
        .lookup(&request, |_, _| None, |_| false, None)
        .classify();
    assert!(
        outcome.resolved_path.is_some(),
        "self-reference via exports must resolve, got {outcome:?}"
    );
    // Exports key `./*.ts` literally ends in `.ts` and the matching default
    // condition rewrites it to `.js` at runtime — the package author opted
    // into the `.ts` → `.js` mapping. TS2877 must be suppressed.
    assert!(
        outcome.resolved_using_ts_extension,
        "self-reference via `./*.ts` exports key must mark resolvedUsingTsExtension"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_exports_target_rejects_parent_escape() {
    // A `package.json#exports` target that escapes the package root via
    // `../` is invalid per Node.js PACKAGE_TARGET_RESOLVE; resolution must
    // fail rather than silently traverse outside the package.
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_exports_target_parent_escape");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{"./leak":"../leak.d.ts"}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/leak.d.ts"),
        "export declare const value: number;",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import { value } from 'pkg/leak';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("pkg/leak", &dir.join("src/index.ts"), Span::new(22, 32));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "exports target `../leak.d.ts` must be rejected, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_exports_target_rejects_node_modules_segment() {
    // A `package.json#exports` target that contains a `node_modules` path
    // segment is invalid per Node.js PACKAGE_TARGET_RESOLVE.
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_exports_target_node_modules");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg/node_modules/dep")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{"./inner":"./node_modules/dep/index.d.ts"}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/node_modules/dep/index.d.ts"),
        "export declare const value: number;",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import { value } from 'pkg/inner';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("pkg/inner", &dir.join("src/index.ts"), Span::new(22, 33));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "exports target containing `node_modules` segment must be rejected, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_imports_target_rejects_absolute_path() {
    // A `package.json#imports` target that is an absolute filesystem path
    // is invalid per Node.js PACKAGE_IMPORTS_RESOLVE; resolution must fail.
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_imports_target_absolute");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    let abs_target = dir.join("abs.d.ts");
    fs::write(&abs_target, "export declare const value: number;").unwrap();
    let package_json = format!(
        r##"{{"name":"app","imports":{{"#abs":{}}}}}"##,
        serde_json::to_string(&abs_target.to_string_lossy().to_string()).unwrap()
    );
    fs::write(dir.join("package.json"), package_json).unwrap();
    fs::write(dir.join("src/index.ts"), "import { value } from '#abs';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_imports: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("#abs", &dir.join("src/index.ts"), Span::new(22, 28));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "imports target with an absolute path must be rejected, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_imports_target_rejects_parent_escape() {
    // An imports target that escapes the project via `../` is invalid.
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_imports_target_parent_escape");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("..").join("escape")).ok();
    fs::write(
        dir.join("package.json"),
        r##"{"name":"app","imports":{"#leak":"../leak.d.ts"}}"##,
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import { value } from '#leak';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_imports: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("#leak", &dir.join("src/index.ts"), Span::new(22, 29));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "imports target containing `..` segment must be rejected, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_is_valid_relative_package_target_rejects_invalid_targets() {
    use super::super::exports_imports::is_valid_relative_package_target;

    assert!(is_valid_relative_package_target("./foo.d.ts"));
    assert!(is_valid_relative_package_target("./lib/inner/foo.d.ts"));

    // No leading "./" prefix.
    assert!(!is_valid_relative_package_target("foo.d.ts"));
    assert!(!is_valid_relative_package_target("../leak.d.ts"));
    // Absolute paths.
    assert!(!is_valid_relative_package_target("/abs/foo.d.ts"));

    // `..` segments anywhere are invalid.
    assert!(!is_valid_relative_package_target("./../leak.d.ts"));
    assert!(!is_valid_relative_package_target("./lib/../leak.d.ts"));

    // `node_modules` segments are invalid.
    assert!(!is_valid_relative_package_target("./node_modules/dep.d.ts"));
    assert!(!is_valid_relative_package_target(
        "./lib/node_modules/dep.d.ts"
    ));
}

#[test]
fn test_is_valid_bare_imports_target_rejects_absolute_and_relative() {
    use super::super::exports_imports::is_valid_bare_imports_target;

    assert!(is_valid_bare_imports_target("some-package"));
    assert!(is_valid_bare_imports_target("@scope/pkg"));
    assert!(is_valid_bare_imports_target("@scope/pkg/sub"));

    // Empty string is invalid.
    assert!(!is_valid_bare_imports_target(""));
    // Relative-looking targets must be handled by the relative-target path.
    assert!(!is_valid_bare_imports_target("./local.d.ts"));
    assert!(!is_valid_bare_imports_target("../parent.d.ts"));
    // Absolute paths.
    assert!(!is_valid_bare_imports_target("/abs/path.d.ts"));
    assert!(!is_valid_bare_imports_target("\\abs\\path.d.ts"));
    // Windows drive paths.
    assert!(!is_valid_bare_imports_target("C:/abs.d.ts"));
}
