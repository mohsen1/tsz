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
use super::fixtures::TempFixture;

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
    // Regression for https://github.com/tsz-org/tsz/issues/3564.
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
fn test_exports_pattern_target_substitutes_every_star() {
    // A single-`*` pattern KEY whose TARGET carries multiple `*` must substitute
    // the captured subpath into ALL of them (Node `PACKAGE_TARGET_RESOLVE` /
    // tsc `replace(/\*/g, subpath)`). The prior first-`*`-only substitution left
    // a literal `*` in the resolved path, so the file was never found → a
    // spurious TS2307 on a perfectly valid exports map. This exercises the core
    // resolver's `exports`-map entry point.
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write(
        "node_modules/pkg/package.json",
        r#"{"name":"pkg","exports":{"./*":"./dist/*/*.d.ts"}}"#,
    );
    fixture.write(
        "node_modules/pkg/dist/button/button.d.ts",
        "export declare const value: number;",
    );
    fixture.write("src/index.ts", "import { value } from 'pkg/button';");

    let mut resolver = ModuleResolver::new(&ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    });
    let result = resolver
        .resolve("pkg/button", &dir.join("src/index.ts"), Span::new(22, 32))
        .expect("multi-star exports target should substitute every '*' with the subpath");
    assert_eq!(
        result.resolved_path,
        dir.join("node_modules/pkg/dist/button/button.d.ts")
    );
}

#[test]
fn test_imports_pattern_target_substitutes_every_star() {
    // Same Node `PACKAGE_TARGET_RESOLVE` rule on the `#imports` side. This
    // exercises the core resolver's distinct `imports`-map entry point, which
    // routes through the same `apply_wildcard_substitution` chokepoint.
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write(
        "package.json",
        r##"{"name":"app","private":true,"imports":{"#feat/*":"./src/*/*.js"}}"##,
    );
    fixture.write("src/btn/btn.d.ts", "export declare const value: number;");
    fixture.write("src/index.ts", "import { value } from '#feat/btn';");

    let mut resolver = ModuleResolver::new(&ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_imports: true,
        ..Default::default()
    });
    let result = resolver
        .resolve("#feat/btn", &dir.join("src/index.ts"), Span::new(22, 32))
        .expect("multi-star imports target should substitute every '*' with the subpath");
    assert!(
        result.resolved_path.ends_with("src/btn/btn.d.ts"),
        "expected src/btn/btn.d.ts, got {}",
        result.resolved_path.display()
    );
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
fn test_package_imports_resolve_only_against_nearest_package_scope() {
    // Per Node.js LOOKUP_PACKAGE_SCOPE + PACKAGE_IMPORTS_RESOLVE (and tsc's
    // `getPackageScopeForPath` / `loadModuleFromImports`), a `#`-prefixed
    // specifier resolves ONLY against the nearest enclosing `package.json`. If
    // that nearest scope has no `imports` field (or no matching key), resolution
    // fails — the resolver must NOT keep walking up to an ancestor package that
    // happens to define a matching `#import`.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_imports_nearest_scope_only");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("packages/inner/src")).unwrap();

    // Outer package defines `#shared`, with a real target on disk.
    fs::write(
        dir.join("package.json"),
        r##"{"name":"root","imports":{"#shared":"./shared.d.ts"}}"##,
    )
    .unwrap();
    fs::write(dir.join("shared.d.ts"), "export declare const v: number;").unwrap();

    // Nearest package scope for the importer has NO `imports` field.
    fs::write(
        dir.join("packages/inner/package.json"),
        r##"{"name":"inner"}"##,
    )
    .unwrap();
    fs::write(
        dir.join("packages/inner/src/index.ts"),
        "import { v } from '#shared'; v;",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve(
        "#shared",
        &dir.join("packages/inner/src/index.ts"),
        Span::new(0, 7),
    );

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "#shared must NOT resolve against an ancestor package once a nearer \
         package scope (without a matching import) is found, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_imports_no_match_in_nearest_scope_does_not_fall_through_to_ancestor() {
    // A variant where the nearest scope DOES have an `imports` field, but the
    // specifier does not match any of its keys. tsc fails here too — it does not
    // continue searching ancestor package scopes for a matching key.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_imports_nearest_scope_nomatch");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("packages/inner/src")).unwrap();

    fs::write(
        dir.join("package.json"),
        r##"{"name":"root","imports":{"#shared":"./shared.d.ts"}}"##,
    )
    .unwrap();
    fs::write(dir.join("shared.d.ts"), "export declare const v: number;").unwrap();

    // Nearest scope has an `imports` map, but only an unrelated key.
    fs::write(
        dir.join("packages/inner/package.json"),
        r##"{"name":"inner","imports":{"#local":"./local.d.ts"}}"##,
    )
    .unwrap();
    fs::write(
        dir.join("packages/inner/local.d.ts"),
        "export declare const w: number;",
    )
    .unwrap();
    fs::write(
        dir.join("packages/inner/src/index.ts"),
        "import { v } from '#shared'; v;",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    // The unrelated `#local` key in the nearest scope resolves fine.
    let local = resolver.resolve(
        "#local",
        &dir.join("packages/inner/src/index.ts"),
        Span::new(0, 6),
    );
    assert!(
        local.is_ok(),
        "#local should resolve against the nearest scope, got {local:?}"
    );

    // But `#shared`, defined only in the ancestor, must not be reached.
    let shared = resolver.resolve(
        "#shared",
        &dir.join("packages/inner/src/index.ts"),
        Span::new(0, 7),
    );
    assert!(
        matches!(shared, Err(ResolutionFailure::NotFound { .. })),
        "#shared (only in ancestor) must not fall through past the nearest \
         import-bearing scope, got {shared:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_imports_resolve_from_subdir_within_same_scope() {
    // Control: walking UP to find the nearest package.json is still correct when
    // the importer lives in a subdirectory of its own package. `#shared` must
    // resolve from `src/nested/` against the single enclosing package scope.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_imports_same_scope_subdir");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/nested")).unwrap();

    fs::write(
        dir.join("package.json"),
        r##"{"name":"app","imports":{"#shared":"./shared.d.ts"}}"##,
    )
    .unwrap();
    fs::write(dir.join("shared.d.ts"), "export declare const v: number;").unwrap();
    fs::write(
        dir.join("src/nested/index.ts"),
        "import { v } from '#shared'; v;",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("#shared", &dir.join("src/nested/index.ts"), Span::new(0, 7));

    let resolved = result.expect("#shared should resolve from a subdir of its own package");
    assert_eq!(resolved.resolved_path, dir.join("shared.d.ts"));

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

/// A `*` pattern key must outrank an equal-base directory key regardless of
/// JSON declaration order. Both packages below expose the same two keys for
/// `./foo` — a `"./*"` wildcard (→ `dist/foo`) and a `"./"` directory prefix
/// (→ `src/foo`) — differing only in declaration order. Per Node.js
/// `PATTERN_KEY_COMPARE`, `"./*"` (base length 3) always beats `"./"`
/// (base length 2), so both must resolve to the same physical `dist` file.
/// Before the fix the two keys tied on `(prefix_len, suffix_len)` and the
/// winner flipped with key order, so the same specifier resolved to different
/// physical files between rows.
#[test]
fn test_wildcard_export_beats_directory_key_independent_of_declaration_order() {
    use std::fs;

    fn resolve_foo(dir: &std::path::Path, exports_json: &str) -> std::path::PathBuf {
        let _ = fs::remove_dir_all(dir);
        fs::create_dir_all(dir.join("node_modules/pkg/dist")).unwrap();
        fs::create_dir_all(dir.join("node_modules/pkg/src")).unwrap();
        fs::create_dir_all(dir.join("app")).unwrap();

        fs::write(
            dir.join("node_modules/pkg/package.json"),
            format!(r#"{{"name":"pkg","exports":{exports_json}}}"#),
        )
        .unwrap();
        // Both candidate targets exist on disk, so selection — not mere
        // existence — decides which physical file wins.
        fs::write(
            dir.join("node_modules/pkg/dist/foo.d.ts"),
            "export declare const from_dist: number;",
        )
        .unwrap();
        fs::write(
            dir.join("node_modules/pkg/src/foo.d.ts"),
            "export declare const from_src: number;",
        )
        .unwrap();
        fs::write(dir.join("app/index.ts"), "import { x } from 'pkg/foo';").unwrap();

        let options = ResolvedCompilerOptions {
            module_resolution: Some(ModuleResolutionKind::Node16),
            resolve_package_json_exports: true,
            ..Default::default()
        };
        let mut resolver = ModuleResolver::new(&options);
        let resolved = resolver
            .resolve("pkg/foo", &dir.join("app/index.ts"), Span::new(15, 24))
            .expect("pkg/foo must resolve through the `./*` wildcard export")
            .resolved_path;
        let _ = fs::remove_dir_all(dir);
        resolved
    }

    // Wildcard declared AFTER the directory key (the order that regressed).
    let dir_then_star = std::env::temp_dir().join("tsz_test_exports_dir_then_star");
    let resolved_a = resolve_foo(&dir_then_star, r#"{"./":"./src/","./*":"./dist/*.js"}"#);
    assert_eq!(
        resolved_a,
        dir_then_star.join("node_modules/pkg/dist/foo.d.ts"),
        "`./*` must win over `./` even when declared second"
    );

    // Wildcard declared BEFORE the directory key — same winner.
    let star_then_dir = std::env::temp_dir().join("tsz_test_exports_star_then_dir");
    let resolved_b = resolve_foo(&star_then_dir, r#"{"./*":"./dist/*.js","./":"./src/"}"#);
    assert_eq!(
        resolved_b,
        star_then_dir.join("node_modules/pkg/dist/foo.d.ts"),
        "`./*` must win over `./` regardless of declaration order"
    );
}

// ===========================================================================
// `exports` authority over the legacy `typesVersions` field
//
// tsc's `loadModuleFromSpecificNodeModulesDirectory` returns from
// `loadModuleFromExports` unconditionally when a package declares `exports`
// ("package exports are higher priority than file/directory/typesVersions
// lookups and ... blocks them"). So under Node16/NodeNext/Bundler the legacy
// `typesVersions` field must NOT be consulted as a fallback when `exports` is
// present: a subpath the `exports` map does not expose is unresolved (TS2307),
// even if a `typesVersions` pattern would otherwise map it to an existing file.
// `typesVersions` still applies to packages that declare NO `exports` map.
// ===========================================================================

/// A non-exported subpath must not fall back to `typesVersions` when the
/// package declares an `exports` map (Node16). Even though the `typesVersions`
/// target file exists on disk, `exports` authority blocks it.
#[test]
fn exports_present_blocks_types_versions_fallback_for_subpath_node16() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("src/index.ts", "import {} from 'widget/internals';");
    fixture.write(
        "node_modules/widget/package.json",
        r#"{
            "name": "widget",
            "exports": { "./panel": "./panel.js" },
            "typesVersions": { ">=3.1": { "*": ["typed/*"] } }
        }"#,
    );
    // The `typesVersions` target exists, but `exports` is authoritative.
    fixture.write(
        "node_modules/widget/typed/internals.d.ts",
        "export const x = 0;",
    );
    fixture.write("node_modules/widget/panel.d.ts", "export const panel = 0;");

    let mut resolver = ModuleResolver::new(&ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        types_versions_compiler_version: Some("3.1.0".to_string()),
        ..Default::default()
    });
    let result = resolver.resolve(
        "widget/internals",
        &dir.join("src/index.ts"),
        Span::new(0, 1),
    );

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "exports is authoritative: a non-exported subpath must not fall back to \
         typesVersions, got {result:?}"
    );
}

/// Same `exports`-authority rule under Bundler resolution, with a wildcard
/// `exports` map that still does not cover the requested subpath.
#[test]
fn exports_present_blocks_types_versions_fallback_for_subpath_bundler() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("app/main.ts", "import {} from 'gadget/secret';");
    fixture.write(
        "node_modules/gadget/package.json",
        r#"{
            "name": "gadget",
            "exports": { "./public/*": "./lib/public/*.js" },
            "typesVersions": { "*": { "*": ["legacy/*"] } }
        }"#,
    );
    fixture.write(
        "node_modules/gadget/legacy/secret.d.ts",
        "export const s = 0;",
    );

    let mut resolver = ModuleResolver::new(&ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_exports: true,
        ..Default::default()
    });
    let result = resolver.resolve("gadget/secret", &dir.join("app/main.ts"), Span::new(0, 1));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "Bundler: a subpath outside the exports map must not resolve via \
         typesVersions, got {result:?}"
    );
}

/// Control: an exported subpath still resolves through `exports` (the fix must
/// not break exports-based subpath resolution).
#[test]
fn exports_present_still_resolves_exported_subpath() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("src/app.ts", "import {} from 'gizmo/panel';");
    fixture.write(
        "node_modules/gizmo/package.json",
        r#"{
            "name": "gizmo",
            "exports": { "./panel": "./panel.js" },
            "typesVersions": { ">=3.1": { "*": ["typed/*"] } }
        }"#,
    );
    fixture.write("node_modules/gizmo/panel.d.ts", "export const panel = 0;");

    let mut resolver = ModuleResolver::new(&ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        types_versions_compiler_version: Some("3.1.0".to_string()),
        ..Default::default()
    });
    let exported_subpath = resolver
        .resolve("gizmo/panel", &dir.join("src/app.ts"), Span::new(0, 1))
        .expect("an exported subpath must still resolve through exports");

    assert_eq!(
        exported_subpath.resolved_path,
        dir.join("node_modules/gizmo/panel.d.ts")
    );
}

/// Control: a package that declares NO `exports` map still honors the legacy
/// `typesVersions` field for subpaths (the fix only blocks typesVersions when
/// exports is present).
#[test]
fn no_exports_subpath_still_uses_types_versions() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("src/app.ts", "import {} from 'doohickey/internals';");
    fixture.write(
        "node_modules/doohickey/package.json",
        r#"{
            "name": "doohickey",
            "typesVersions": { ">=3.1": { "*": ["typed/*"] } }
        }"#,
    );
    fixture.write(
        "node_modules/doohickey/typed/internals.d.ts",
        "export const x = 0;",
    );

    let mut resolver = ModuleResolver::new(&ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        types_versions_compiler_version: Some("3.1.0".to_string()),
        ..Default::default()
    });
    let legacy_types_version_subpath = resolver
        .resolve(
            "doohickey/internals",
            &dir.join("src/app.ts"),
            Span::new(0, 1),
        )
        .expect("without an exports map, typesVersions still resolves the subpath");

    assert_eq!(
        legacy_types_version_subpath.resolved_path,
        dir.join("node_modules/doohickey/typed/internals.d.ts")
    );
}
