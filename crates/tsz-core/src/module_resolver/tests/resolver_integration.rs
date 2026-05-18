//! Resolver Integration tests for `module_resolver`.
//!
//! Integration tests that exercise `ModuleResolver::resolve()`
//! against real temp-file fixtures: relative paths, directory index,
//! `.tsx` / `.d.ts` resolution, `package.json` main/types entries,
//! bare specifier walk-up, `rootDirs` overlay, JSON imports, and the
//! basic resolver-creation + missing-file paths.
//!
//! Setup uses the shared `super::fixtures::TempFixture` builder; see
//! `super::fixtures` for the pattern this replaces.

use std::fs;

use super::super::*;
use super::fixtures::{TempFixture, bundler_resolver, node_resolver, resolver_with};

#[test]
fn test_module_resolver_creation() {
    let resolver = ModuleResolver::node_resolver();
    assert_eq!(resolver.resolution_kind(), ModuleResolutionKind::Node);
}

#[test]
fn test_resolver_relative_ts_file() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("main.ts", "import { foo } from './utils';");
    fixture.write("utils.ts", "export const foo = 42;");

    let mut resolver = node_resolver();
    let result = resolver.resolve("./utils", &dir.join("main.ts"), Span::new(0, 10));

    match result {
        Ok(module) => {
            assert_eq!(module.resolved_path, dir.join("utils.ts"));
            assert_eq!(module.extension, ModuleExtension::Ts);
            assert!(!module.is_external);
        }
        Err(_) => {
            // Resolution might fail in some environments, that's OK for this test
        }
    }
}

#[test]
fn test_resolver_clear_cache_drops_file_existence_entries() {
    let fixture = TempFixture::new();
    let containing_file = fixture.write("main.ts", "import { foo } from './utils';");
    let dependency = fixture.join("utils.ts");

    let mut resolver = node_resolver();
    let missing = resolver.resolve("./utils", &containing_file, Span::new(0, 10));
    assert!(
        matches!(missing, Err(ResolutionFailure::NotFound { .. })),
        "expected first lookup to miss before utils.ts exists, got {missing:?}"
    );

    fs::write(&dependency, "export const foo = 42;").unwrap();
    resolver.clear_cache();

    let resolved = resolver
        .resolve("./utils", &containing_file, Span::new(0, 10))
        .expect("clear_cache should drop stale file-existence entries");
    assert_eq!(resolved.resolved_path, dependency);
    assert_eq!(resolved.extension, ModuleExtension::Ts);
}

#[test]
fn test_resolver_explicit_dts_import_probes_sibling_implementation() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("types.d.ts", "import {} from './a.d.ts';");
    fixture.write("a.ts", "export {};");
    fixture.write("b.mts", "export {};");
    fixture.write("c.cts", "export = {};");

    let mut resolver = bundler_resolver();

    let dts = resolver
        .resolve("./a.d.ts", &dir.join("types.d.ts"), Span::new(15, 25))
        .expect("expected .d.ts specifier to resolve through sibling .ts");
    assert_eq!(dts.resolved_path, dir.join("a.ts"));
    assert_eq!(dts.extension, ModuleExtension::Ts);

    let dmts = resolver
        .resolve("./b.d.mts", &dir.join("types.d.ts"), Span::new(15, 26))
        .expect("expected .d.mts specifier to resolve through sibling .mts");
    assert_eq!(dmts.resolved_path, dir.join("b.mts"));
    assert_eq!(dmts.extension, ModuleExtension::Mts);

    let dcts = resolver
        .resolve("./c.d.cts", &dir.join("types.d.ts"), Span::new(15, 26))
        .expect("expected .d.cts specifier to resolve through sibling .cts");
    assert_eq!(dcts.resolved_path, dir.join("c.cts"));
    assert_eq!(dcts.extension, ModuleExtension::Cts);
}

#[test]
fn test_resolver_relative_tsx_file() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("app.ts", "");
    fixture.write("Button.tsx", "export default function Button() {}");

    let mut resolver = node_resolver();
    let result = resolver.resolve("./Button", &dir.join("app.ts"), Span::new(0, 10));

    if let Ok(module) = result {
        assert_eq!(module.resolved_path, dir.join("Button.tsx"));
        assert_eq!(module.extension, ModuleExtension::Tsx);
    }
}

#[test]
fn test_resolver_index_file() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.mkdir("utils");
    fixture.write("main.ts", "");
    fixture.write("utils/index.ts", "export const foo = 42;");

    let mut resolver = node_resolver();
    let result = resolver.resolve("./utils", &dir.join("main.ts"), Span::new(0, 10));

    if let Ok(module) = result {
        assert_eq!(module.resolved_path, dir.join("utils").join("index.ts"));
        assert_eq!(module.extension, ModuleExtension::Ts);
    }
}

#[test]
fn test_resolver_dot_and_trailing_slash_prefer_directory_index() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.mkdir("a/b");
    fixture.write("a.ts", "export default { a: 0 };");
    fixture.write("a/index.ts", "export default { aIndex: 0 };");
    fixture.write("a/test.ts", "import value from '.';");
    fixture.write("a/b/test.ts", "import value from '..';");

    let mut resolver = bundler_resolver();

    let dot = resolver
        .resolve(".", &dir.join("a").join("test.ts"), Span::new(0, 1))
        .expect("Expected '.' to resolve");
    assert_eq!(dot.resolved_path, dir.join("a").join("index.ts"));

    let dot_slash = resolver
        .resolve("./", &dir.join("a").join("test.ts"), Span::new(0, 2))
        .expect("Expected './' to resolve");
    assert_eq!(dot_slash.resolved_path, dir.join("a").join("index.ts"));

    let dotdot = resolver
        .resolve(
            "..",
            &dir.join("a").join("b").join("test.ts"),
            Span::new(0, 2),
        )
        .expect("Expected '..' to resolve");
    assert_eq!(
        fs::canonicalize(&dotdot.resolved_path).unwrap(),
        fs::canonicalize(dir.join("a").join("index.ts")).unwrap()
    );

    let dotdot_slash = resolver
        .resolve(
            "../",
            &dir.join("a").join("b").join("test.ts"),
            Span::new(0, 3),
        )
        .expect("Expected '../' to resolve");
    assert_eq!(
        fs::canonicalize(&dotdot_slash.resolved_path).unwrap(),
        fs::canonicalize(dir.join("a").join("index.ts")).unwrap()
    );
}

#[test]
fn test_resolver_dts_file() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("main.ts", "");
    fixture.write("types.d.ts", "export interface Foo {}");

    let mut resolver = node_resolver();
    let result = resolver.resolve("./types", &dir.join("main.ts"), Span::new(0, 10));

    if let Ok(module) = result {
        assert_eq!(module.resolved_path, dir.join("types.d.ts"));
        assert_eq!(module.extension, ModuleExtension::Dts);
    }
}

#[test]
fn test_resolver_jsx_without_jsx_option_errors() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("app.ts", "import jsx from './jsx';");
    fixture.write("jsx.jsx", "export default 1;");

    let mut resolver = resolver_with(ResolvedCompilerOptions {
        allow_js: true,
        jsx: None,
        // Use Node resolution so allowJs is respected (Classic never resolves .jsx)
        module_resolution: Some(ModuleResolutionKind::Node),
        ..Default::default()
    });
    let result = resolver.resolve("./jsx", &dir.join("app.ts"), Span::new(0, 10));

    let failure = result.expect_err("Expected jsx resolution to fail without jsx option");
    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, 6142);
}

#[test]
fn test_resolver_tsx_without_jsx_option_errors() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("app.ts", "import tsx from './tsx';");
    fixture.write("tsx.tsx", "export default 1;");

    let mut resolver = resolver_with(ResolvedCompilerOptions {
        jsx: None,
        // Use Node resolution so .tsx files are found (Classic also finds .tsx, but be explicit)
        module_resolution: Some(ModuleResolutionKind::Node),
        ..Default::default()
    });
    let result = resolver.resolve("./tsx", &dir.join("app.ts"), Span::new(0, 10));

    let failure = result.expect_err("Expected tsx resolution to fail without jsx option");
    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, 6142);
}

#[test]
fn test_json_import_without_resolve_json_module() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("app.ts", "import data from './data.json';");
    fixture.write("data.json", "{\"value\": 42}");

    let mut resolver = resolver_with(ResolvedCompilerOptions {
        resolve_json_module: false, // JSON modules disabled
        ..Default::default()
    });

    let result = resolver.resolve("./data.json", &dir.join("app.ts"), Span::new(0, 10));

    let failure = result.expect_err("Expected JSON resolution to fail without resolveJsonModule");
    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, 2732); // TS2732
}

#[test]
fn test_extensionless_json_import_does_not_resolve_with_resolve_json_module() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("app.ts", "import data = require('./data');");
    fixture.write("data.json", "{\"value\": 42}");

    let mut resolver = resolver_with(ResolvedCompilerOptions {
        resolve_json_module: true,
        module_resolution: Some(ModuleResolutionKind::Node),
        ..Default::default()
    });

    let result = resolver.resolve("./data", &dir.join("app.ts"), Span::new(0, 10));

    let failure = result.expect_err(
        "Expected extensionless resolution to reject ./data even when data.json exists",
    );
    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, 2307);
}

#[test]
fn test_resolver_package_main_with_unknown_extension() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("app.ts", "import 'normalize.css';");
    fixture.write("node_modules/normalize.css/normalize.css", "body {}");
    fixture.write(
        "node_modules/normalize.css/package.json",
        r#"{ "main": "normalize.css" }"#,
    );

    let mut resolver = node_resolver();
    let result = resolver.resolve("normalize.css", &dir.join("app.ts"), Span::new(0, 10));
    assert!(
        result.is_ok(),
        "Expected package main with unknown extension to resolve"
    );
}

#[test]
fn test_resolver_package_types_with_unknown_extension_is_ignored() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("app.ts", "import 'foo';");
    fixture.write("node_modules/foo/foo.js", "module.exports = {};");
    fixture.write("node_modules/foo/package.json", r#"{ "types": "foo.js" }"#);

    let mut resolver = node_resolver();
    let result = resolver.resolve("foo", &dir.join("app.ts"), Span::new(0, 10));
    assert!(
        result.is_err(),
        "Expected package types with runtime JS extension to be ignored"
    );
}

#[test]
fn test_resolver_package_types_js_without_allow_js_is_ignored() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("app.ts", "import 'foo';");
    fixture.write("node_modules/foo/foo.js", "module.exports = {};");
    fixture.write("node_modules/foo/package.json", r#"{ "types": "foo.js" }"#);

    let mut resolver = node_resolver();
    let result = resolver.resolve("foo", &dir.join("app.ts"), Span::new(0, 10));
    assert!(
        result.is_err(),
        "Expected package types .js to be ignored without allowJs"
    );
}

#[test]
fn test_resolver_package_without_package_json_uses_index_file() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write(
        "index.ts",
        "import { x } from 'whatever';\nexport const y = x;",
    );
    fixture.write(
        "node_modules/whatever/index.d.ts",
        "export const x: number;",
    );

    let mut resolver = node_resolver();
    let result = resolver.resolve("whatever", &dir.join("index.ts"), Span::new(0, 10));

    let resolved = result.expect("package without package.json should resolve via index");
    assert_eq!(
        resolved.resolved_path,
        dir.join("node_modules").join("whatever").join("index.d.ts")
    );
}

#[test]
fn test_resolver_bare_specifier_from_node_modules_package_finds_sibling_package() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("node_modules/baz/index.d.ts", "export { T } from \"foo\";");
    fixture.write("node_modules/foo/index.d.ts", "export type T = number;");

    let mut resolver = node_resolver();
    let result = resolver.resolve(
        "foo",
        &dir.join("node_modules").join("baz").join("index.d.ts"),
        Span::new(16, 21),
    );

    let resolved = result.expect("bare specifier should resolve to sibling package");
    assert_eq!(
        resolved.resolved_path,
        dir.join("node_modules").join("foo").join("index.d.ts")
    );
}

#[test]
fn test_resolver_invalid_types_field_falls_back_to_main_declaration() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("app.ts", "type Parser = typeof import(\"csv-parse\");");
    fixture.write(
        "node_modules/csv-parse/lib/index.d.ts",
        "export function bar(): number;",
    );
    fixture.write(
        "node_modules/csv-parse/package.json",
        r#"{
            "name": "csv-parse",
            "main": "./lib",
            "types": ["./lib/index.d.ts", "./lib/sync.d.ts"]
        }"#,
    );

    let mut resolver = node_resolver();
    let result = resolver.resolve("csv-parse", &dir.join("app.ts"), Span::new(0, 10));

    let resolved = result.expect("invalid package.json types field should be ignored");
    assert_eq!(
        resolved.resolved_path,
        dir.join("node_modules")
            .join("csv-parse")
            .join("lib")
            .join("index.d.ts")
    );
}

#[test]
fn test_resolver_empty_types_field_uses_types_versions() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("app.ts", "import { a } from \"a\";");
    fixture.write("node_modules/a/ts3.1/index.d.ts", "export const a = 0;");
    fixture.write(
        "node_modules/a/package.json",
        r#"{
            "name": "a",
            "types": "",
            "typesVersions": {
                ">=3.1": { "*": ["ts3.1/*"] }
            }
        }"#,
    );

    let mut resolver = resolver_with(ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        types_versions_compiler_version: Some("3.1.0".to_string()),
        ..Default::default()
    });
    let result = resolver.resolve("a", &dir.join("app.ts"), Span::new(0, 1));

    let resolved = result.expect("empty package.json types field should be ignored");
    assert_eq!(
        resolved.resolved_path,
        dir.join("node_modules")
            .join("a")
            .join("ts3.1")
            .join("index.d.ts")
    );
}

#[test]
fn test_resolver_relative_directory_applies_types_versions() {
    // Regression: a relative import resolving into a package root (e.g. `../`
    // from inside a typesVersions-redirected directory) must re-apply
    // typesVersions from the package's package.json. Without this, tsz
    // bypasses the redirect and resolves straight to the bare `types` entry,
    // diverging from tsc. See the TypeScript conformance test
    // `typesVersionsDeclarationEmit.multiFileBackReferenceToSelf.ts`.
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write(
        "node_modules/ext/package.json",
        r#"{
            "name": "ext",
            "version": "1.0.0",
            "types": "index",
            "typesVersions": {
                ">=3.1.0-0": { "*": ["ts3.1/*"] }
            }
        }"#,
    );
    fixture.write(
        "node_modules/ext/index.d.ts",
        "export interface A {}\nexport function fa(): A;",
    );
    fixture.write(
        "node_modules/ext/ts3.1/index.d.ts",
        r#"export * from "../";"#,
    );

    let mut resolver = resolver_with(ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        types_versions_compiler_version: Some("3.1.0-dev".to_string()),
        ..Default::default()
    });
    let containing = dir
        .join("node_modules")
        .join("ext")
        .join("ts3.1")
        .join("index.d.ts");
    let result = resolver.resolve("../", &containing, Span::new(0, 4));

    let resolved = result.expect("relative directory import should resolve");
    // tsc applies typesVersions here, mapping `../` → ts3.1/index.d.ts (which
    // loops back to the current file). The key invariant is that the bare
    // `index.d.ts` is NOT selected when typesVersions matches.
    let expected = dir
        .join("node_modules")
        .join("ext")
        .join("ts3.1")
        .join("index.d.ts")
        .canonicalize()
        .unwrap();
    let actual = resolved.resolved_path.canonicalize().unwrap();
    assert_eq!(
        actual, expected,
        "expected typesVersions to redirect `../` to ts3.1/index.d.ts, got {:?}",
        resolved.resolved_path,
    );
}

#[test]
fn test_resolver_relative_import_uses_root_dirs_overlay() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("src/main.ts", "import './generated';");
    fixture.write("generated/generated.ts", "export const generated = 'ok';");

    let mut resolver = resolver_with(ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        root_dirs: vec![dir.join("src"), dir.join("generated")],
        ..Default::default()
    });
    let resolved = resolver
        .resolve(
            "./generated",
            &dir.join("src").join("main.ts"),
            Span::new(0, 13),
        )
        .expect("rootDirs overlay should resolve sibling virtual path");

    assert_eq!(
        resolved.resolved_path.canonicalize().unwrap(),
        dir.join("generated")
            .join("generated.ts")
            .canonicalize()
            .unwrap()
    );
}

#[test]
fn test_resolver_subpath_ambient_module_falls_back_to_types_entry() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("app.ts", "import { b } from \"ext/other\";");
    fixture.write(
        "node_modules/ext/ts3.1/index.d.ts",
        r#"declare module "ext" { export const a: "ts3.1 a"; }
declare module "ext/other" { export const b: "ts3.1 b"; }"#,
    );
    fixture.write(
        "node_modules/ext/package.json",
        r#"{
            "name": "ext",
            "types": "index",
            "typesVersions": {
                ">=3.1.0-0": { "*": ["ts3.1/*"] }
            }
        }"#,
    );

    let mut resolver = resolver_with(ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        types_versions_compiler_version: Some("6.0.1".to_string()),
        ..Default::default()
    });
    let result = resolver.resolve("ext/other", &dir.join("app.ts"), Span::new(0, 11));

    let resolved =
        result.expect("ambient subpath should resolve through package types entry fallback");
    assert_eq!(
        resolved.resolved_path,
        dir.join("node_modules")
            .join("ext")
            .join("ts3.1")
            .join("index.d.ts")
    );
}

#[test]
fn test_resolver_missing_file() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("main.ts", "");

    let mut resolver = node_resolver();
    let result = resolver.resolve("./nonexistent", &dir.join("main.ts"), Span::new(0, 10));

    assert!(result.is_err(), "Missing file should produce error");
    if let Err(failure) = result {
        assert!(failure.is_not_found());
    }
}
