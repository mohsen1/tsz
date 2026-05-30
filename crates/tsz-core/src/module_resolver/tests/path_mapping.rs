//! Path mapping resolution tests.
//!
//! Covers `paths` / `baseUrl` behavior:
//!
//! - Wildcard targets without extension (baseline, existing behavior)
//! - Fixed declaration targets (`.d.ts`) with no wildcard in target
//! - Wildcard targets with explicit extension suffix (`.d.ts`, `.ts`, `.js`)
//! - Catch-all `"*"` pattern mapping to a fixed declaration file
//! - Multiple fallback targets — first hit wins
//! - Specificity ordering (longer prefix wins)
//! - Nested sub-paths captured by wildcard (`"components/Button"`)

use super::super::*;
use super::fixtures::TempFixture;
use crate::config::PathMapping;

// ── helpers ───────────────────────────────────────────────────────────────────

fn make_options(dir: &std::path::Path, mappings: Vec<PathMapping>) -> ResolvedCompilerOptions {
    ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        base_url: Some(dir.to_path_buf()),
        paths: Some(mappings),
        module_suffixes: vec![String::new()],
        ..Default::default()
    }
}

/// Convenience constructor for a single `PathMapping` with no suffix.
fn pm(pattern: &str, prefix: &str, targets: &[&str]) -> PathMapping {
    PathMapping {
        pattern: pattern.to_string(),
        prefix: prefix.to_string(),
        suffix: String::new(),
        targets: targets.iter().map(|s| s.to_string()).collect(),
    }
}

// ── wildcard extensionless target (baseline) ──────────────────────────────────

#[test]
fn test_path_mapping_wildcard_extensionless_target() {
    let fx = TempFixture::new();
    fx.write("src/widget.ts", "export const w = 1;");
    fx.write("index.ts", "import '@app/widget';");

    let options = make_options(fx.path(), vec![pm("@app/*", "@app/", &["src/*"])]);
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("@app/widget", &fx.join("index.ts"), Span::new(0, 11));
    assert_eq!(
        result
            .expect("@app/widget should resolve to src/widget.ts")
            .resolved_path,
        fx.join("src/widget.ts"),
    );
}

// ── fixed .d.ts target — previously broken ────────────────────────────────────

#[test]
fn test_path_mapping_exact_dts_target() {
    // A fixed `.d.ts` target (no wildcard in the target string) must resolve.
    // Previously `has_path_mapping_target_extension` silently skipped it.
    let fx = TempFixture::new();
    fx.write(
        "external.d.ts",
        "declare const value: any; export default value;",
    );
    fx.write("index.ts", "import 'next';");

    let options = make_options(fx.path(), vec![pm("next", "next", &["./external.d.ts"])]);
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("next", &fx.join("index.ts"), Span::new(0, 6));
    assert_eq!(
        result
            .expect("exact .d.ts target must resolve")
            .resolved_path,
        fx.join("external.d.ts"),
    );
}

#[test]
fn test_path_mapping_catch_all_wildcard_with_fixed_dts_target() {
    // `"*": ["./external.d.ts"]` — a catch-all pattern mapping ALL specifiers
    // to a single fixed declaration file. Models the nextjs guard config pattern.
    let fx = TempFixture::new();
    fx.write(
        "external.d.ts",
        "declare const defaultExport: any; export default defaultExport;",
    );
    fx.write("index.ts", "import 'some-pkg';");

    let options = make_options(fx.path(), vec![pm("*", "", &["./external.d.ts"])]);
    let mut resolver = ModuleResolver::new(&options);

    for specifier in &["some-pkg", "react", "next/image", "lodash/fp"] {
        let result = resolver.resolve(specifier, &fx.join("index.ts"), Span::new(0, 8));
        assert_eq!(
            result
                .unwrap_or_else(|_| panic!("{specifier} should resolve via catch-all mapping"))
                .resolved_path,
            fx.join("external.d.ts"),
            "{specifier} must map to external.d.ts"
        );
    }
}

#[test]
fn test_path_mapping_wildcard_with_explicit_dts_suffix_in_target() {
    // `"@types/*": ["./stubs/*.d.ts"]` — the target template itself ends in `.d.ts`.
    // After substitution `"utils"` → `"./stubs/utils.d.ts"` the file must be found.
    let fx = TempFixture::new();
    fx.write("stubs/utils.d.ts", "export declare function util(): void;");
    fx.write("index.ts", "import '@types/utils';");

    let options = make_options(
        fx.path(),
        vec![pm("@types/*", "@types/", &["./stubs/*.d.ts"])],
    );
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("@types/utils", &fx.join("index.ts"), Span::new(0, 14));
    assert_eq!(
        result
            .expect("@types/utils should resolve to stubs/utils.d.ts")
            .resolved_path,
        fx.join("stubs/utils.d.ts"),
    );
}

#[test]
fn test_path_mapping_wildcard_with_explicit_ts_suffix_in_target() {
    // `"@src/*": ["./source/*.ts"]` — explicit `.ts` extension in target template.
    let fx = TempFixture::new();
    fx.write("source/helpers.ts", "export const x = 1;");
    fx.write("index.ts", "import '@src/helpers';");

    let options = make_options(fx.path(), vec![pm("@src/*", "@src/", &["./source/*.ts"])]);
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("@src/helpers", &fx.join("index.ts"), Span::new(0, 14));
    assert_eq!(
        result
            .expect("@src/helpers should resolve to source/helpers.ts")
            .resolved_path,
        fx.join("source/helpers.ts"),
    );
}

#[test]
fn test_path_mapping_wildcard_captures_nested_sub_path() {
    // When the wildcard captures a multi-segment path like `"server/app-page"`,
    // the substituted target `"./src/server/app-page"` must still resolve.
    let fx = TempFixture::new();
    fx.write("src/server/app-page.ts", "export type AppPage = {};");
    fx.write("index.ts", "import 'next/dist/server/app-page';");

    let options = make_options(
        fx.path(),
        vec![pm("next/dist/*", "next/dist/", &["./src/*"])],
    );
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve(
        "next/dist/server/app-page",
        &fx.join("index.ts"),
        Span::new(0, 26),
    );
    assert_eq!(
        result
            .expect("nested sub-path should resolve via wildcard mapping")
            .resolved_path,
        fx.join("src/server/app-page.ts"),
    );
}

// ── specificity ordering ──────────────────────────────────────────────────────

#[test]
fn test_path_mapping_more_specific_pattern_wins() {
    // `"next/dist/compiled/*"` (prefix len 22) beats `"next/dist/*"` (prefix len 10)
    // beats `"*"` (prefix len 0).  The external.d.ts is the expected result only
    // for `"next/dist/compiled/..."` specifiers.
    let fx = TempFixture::new();
    fx.write("src/router.ts", "export {};");
    fx.write("external.d.ts", "declare const v: any; export default v;");
    fx.write("index.ts", "");

    let options = make_options(
        fx.path(),
        vec![
            pm(
                "next/dist/compiled/*",
                "next/dist/compiled/",
                &["./external.d.ts"],
            ),
            pm("next/dist/*", "next/dist/", &["./src/*"]),
            pm("*", "", &["./external.d.ts"]),
        ],
    );
    let mut resolver = ModuleResolver::new(&options);

    // Most specific: hits the compiled wildcard → external.d.ts
    let compiled = resolver
        .resolve(
            "next/dist/compiled/react",
            &fx.join("index.ts"),
            Span::new(0, 1),
        )
        .expect("next/dist/compiled/* should map to external.d.ts");
    assert_eq!(compiled.resolved_path, fx.join("external.d.ts"));

    // Medium specificity: hits next/dist/* → src/router.ts
    let server = resolver
        .resolve("next/dist/router", &fx.join("index.ts"), Span::new(0, 1))
        .expect("next/dist/* should map to src/router.ts");
    assert_eq!(server.resolved_path, fx.join("src/router.ts"));

    // Least specific: * catch-all → external.d.ts
    let unrelated = resolver
        .resolve("lodash", &fx.join("index.ts"), Span::new(0, 1))
        .expect("* catch-all should map to external.d.ts");
    assert_eq!(unrelated.resolved_path, fx.join("external.d.ts"));
}

// ── multiple fallback targets ─────────────────────────────────────────────────

#[test]
fn test_path_mapping_falls_through_missing_targets_to_first_existing() {
    // When a mapping lists multiple targets, the first one that resolves on disk wins.
    let fx = TempFixture::new();
    // Only the second target file exists.
    fx.write("fallback.d.ts", "export {};");
    fx.write("index.ts", "import 'pkg';");

    let options = make_options(
        fx.path(),
        vec![pm("pkg", "pkg", &["./missing.d.ts", "./fallback.d.ts"])],
    );
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("pkg", &fx.join("index.ts"), Span::new(0, 5));
    assert_eq!(
        result
            .expect("second fallback target should resolve when first is missing")
            .resolved_path,
        fx.join("fallback.d.ts"),
    );
}

// ── extension classification ──────────────────────────────────────────────────

#[test]
fn test_path_mapping_explicit_dts_target_classifies_as_dts() {
    let fx = TempFixture::new();
    fx.write("stub.d.ts", "export declare const n: number;");
    fx.write("index.ts", "import 'pkg';");

    let options = make_options(fx.path(), vec![pm("pkg", "pkg", &["./stub.d.ts"])]);
    let mut resolver = ModuleResolver::new(&options);
    let module = resolver
        .resolve("pkg", &fx.join("index.ts"), Span::new(0, 5))
        .expect("explicit .d.ts target must resolve");

    assert_eq!(module.resolved_path, fx.join("stub.d.ts"));
    assert_eq!(
        module.extension,
        ModuleExtension::Dts,
        "resolved extension must be Dts, not Unknown"
    );
}

// ── nextjs-fixture pattern ────────────────────────────────────────────────────

#[test]
fn test_path_mapping_nextjs_guard_config_pattern() {
    // Reproduces the nextjs guard tsconfig.tsz-guard.json path mapping:
    //
    //   "next/dist/compiled/*" → ["./external.d.ts"]
    //   "next/dist/*"          → ["./src/*"]
    //   "*"                    → ["./external.d.ts"]
    //
    // Before the fix, the first and third entries had `.d.ts` targets that
    // `has_path_mapping_target_extension` skipped, so any import not matching
    // `"next/dist/*"` silently fell through to bare-specifier resolution and
    // produced TS2307 / no-module divergence from tsc.
    let fx = TempFixture::new();
    fx.write(
        "external.d.ts",
        "declare const defaultExport: any; export default defaultExport;",
    );
    fx.write("src/server/app-page.ts", "export type AppPage = {};");
    fx.write("index.ts", "");

    let options = make_options(
        fx.path(),
        vec![
            pm(
                "next/dist/compiled/*",
                "next/dist/compiled/",
                &["./external.d.ts"],
            ),
            pm("next/dist/*", "next/dist/", &["./src/*"]),
            pm("*", "", &["./external.d.ts"]),
        ],
    );
    let mut resolver = ModuleResolver::new(&options);

    // "next/dist/compiled/react" → external.d.ts (most-specific fixed target)
    let compiled = resolver
        .resolve(
            "next/dist/compiled/react",
            &fx.join("index.ts"),
            Span::new(0, 1),
        )
        .expect("next/dist/compiled/* must resolve to external.d.ts");
    assert_eq!(compiled.resolved_path, fx.join("external.d.ts"));

    // "next/dist/server/app-page" → src/server/app-page.ts (wildcard extensionless)
    let server_page = resolver
        .resolve(
            "next/dist/server/app-page",
            &fx.join("index.ts"),
            Span::new(0, 1),
        )
        .expect("next/dist/* must resolve to src/server/app-page.ts");
    assert_eq!(server_page.resolved_path, fx.join("src/server/app-page.ts"));

    // "next" → external.d.ts (catch-all "*" pattern)
    let next_root = resolver
        .resolve("next", &fx.join("index.ts"), Span::new(0, 1))
        .expect("\"*\" catch-all must resolve next to external.d.ts");
    assert_eq!(next_root.resolved_path, fx.join("external.d.ts"));

    // "react" → external.d.ts (catch-all "*" pattern)
    let react = resolver
        .resolve("react", &fx.join("index.ts"), Span::new(0, 1))
        .expect("\"*\" catch-all must resolve react to external.d.ts");
    assert_eq!(react.resolved_path, fx.join("external.d.ts"));
}
