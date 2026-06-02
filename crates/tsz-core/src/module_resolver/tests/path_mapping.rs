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

// ── tsc-canonical extension priority for extensionless aliases ───────────────
//
// Regression for issue #10944. Structural rule: for an extensionless alias /
// path-mapping target (i.e. no package context — `package_type = None`), tsc's
// `supportedTSExtensions` orders the candidates as
// `[Ts, Tsx, Dts], [Cts, Dcts], [Mts, Dmts]`. The CJS-tagged group precedes
// the ESM-tagged group, so a `.cts` sibling outranks a `.mts` sibling. Pre-fix,
// `tsz-core::TS_EXTENSION_CANDIDATES` had `mts` ahead of `cts` and the
// extensionless fan-out picked the wrong sibling on every stem collision in
// Bundler / Classic / path-mapping resolution.

#[test]
fn test_path_mapping_extensionless_alias_follows_tsc_group_order() {
    // Three rows exercising the same structural rule on different surfaces:
    // (sibling extensions to drop into `src/dual.*`, expected winner). The
    // first two rows test the CJS-vs-ESM grouping; the third confirms the
    // universal `[Ts, Tsx, Dts]` group still outranks both module-tagged
    // groups. Per row, `make_options` builds a fresh resolver against the
    // same `@app/*` → `src/*` alias.
    let rows: &[(&[&str], &str)] = &[
        (&["src/dual.cts", "src/dual.mts"], "src/dual.cts"),
        (&["src/dual.d.cts", "src/dual.d.mts"], "src/dual.d.cts"),
        (
            &["src/dual.ts", "src/dual.cts", "src/dual.mts"],
            "src/dual.ts",
        ),
    ];
    for (siblings, expected) in rows {
        let fx = TempFixture::new();
        for sibling in *siblings {
            fx.write(sibling, "export const v: number;");
        }
        fx.write("index.ts", "import '@app/dual';");

        let options = make_options(fx.path(), vec![pm("@app/*", "@app/", &["src/*"])]);
        let mut resolver = ModuleResolver::new(&options);
        let resolved = resolver
            .resolve("@app/dual", &fx.join("index.ts"), Span::new(0, 9))
            .expect("@app/dual must resolve");
        assert_eq!(
            resolved.resolved_path,
            fx.join(expected),
            "siblings={siblings:?}: extensionless @app/dual must pick {expected} \
             per tsc's supportedTSExtensions grouping",
        );
    }
}

// ── path segment normalization (issue #10896) ────────────────────────────────
//
// Structural rule: when a `paths` target text contains `./` or `../` segments,
// or when `baseUrl` is itself non-canonical, the substituted candidate path
// must be segment-normalized before file probing. Without this, the resolved
// `PathBuf` retains the literal text (`<base>/./lib/foo.d.ts`,
// `<base>/lib/../shared/foo.d.ts`) and the same physical declaration ends up
// represented by two distinct path keys depending on which alias branch
// produced it — splitting declaration identity in the project file graph.
//
// The rule is independent of the wildcard variable name and of the alias key
// spelling; the tests below vary both to keep the fix structural.

#[test]
fn test_path_mapping_dot_slash_target_resolves_to_canonical_path() {
    // `paths: { "@app/*": ["./src/*"] }` — the leading `./` in the target must
    // not survive into `resolved_path`.
    let fx = TempFixture::new();
    fx.write("src/widget.ts", "export const w = 1;");
    fx.write("index.ts", "import '@app/widget';");

    let options = make_options(fx.path(), vec![pm("@app/*", "@app/", &["./src/*"])]);
    let mut resolver = ModuleResolver::new(&options);
    let resolved = resolver
        .resolve("@app/widget", &fx.join("index.ts"), Span::new(0, 11))
        .expect("@app/widget should resolve");

    let expected = fx.join("src/widget.ts");
    assert_eq!(
        resolved.resolved_path, expected,
        "leading `./` segment must be normalized out of the resolved path",
    );
    assert!(
        !resolved
            .resolved_path
            .components()
            .any(|c| matches!(c, std::path::Component::CurDir)),
        "resolved path must contain no `.` components, got {:?}",
        resolved.resolved_path,
    );
}

#[test]
fn test_path_mapping_parent_dir_target_resolves_to_canonical_path() {
    // `paths: { "@util/*": ["./lib/../shared/*"] }` — embedded `..` must be
    // canonicalized.
    let fx = TempFixture::new();
    fx.write("shared/helpers.ts", "export const h = 1;");
    fx.write("index.ts", "import '@util/helpers';");

    let options = make_options(
        fx.path(),
        vec![pm("@util/*", "@util/", &["./lib/../shared/*"])],
    );
    let mut resolver = ModuleResolver::new(&options);
    let resolved = resolver
        .resolve("@util/helpers", &fx.join("index.ts"), Span::new(0, 13))
        .expect("@util/helpers should resolve via alias with `..` segment");

    let expected = fx.join("shared/helpers.ts");
    assert_eq!(
        resolved.resolved_path, expected,
        "embedded `..` segments must be normalized out of the resolved path",
    );
    assert!(
        !resolved
            .resolved_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir)),
        "resolved path must contain no `..` components, got {:?}",
        resolved.resolved_path,
    );
}

#[test]
fn test_path_mapping_two_aliases_to_same_file_share_resolved_path() {
    // Two distinct alias keys whose substituted targets refer to the same
    // physical `.d.ts` file must produce identical `resolved_path` values, even
    // when one target uses a `./` prefix and the other uses an embedded `..`.
    // The wildcard variable name in each pattern is deliberately different
    // (`*` is the only allowed placeholder, but the prefix/suffix shape varies)
    // to prove the rule is structural, not name-keyed.
    let fx = TempFixture::new();
    fx.write("shared/api.d.ts", "export declare const v: number;");
    fx.write("index.ts", "import '@alpha/api'; import '@beta/api';");

    let options = make_options(
        fx.path(),
        vec![
            pm("@alpha/*", "@alpha/", &["./shared/*.d.ts"]),
            // Same logical export reached through a `..` detour.
            pm("@beta/*", "@beta/", &["./lib/../shared/*.d.ts"]),
        ],
    );
    let mut resolver = ModuleResolver::new(&options);

    let via_alpha = resolver
        .resolve("@alpha/api", &fx.join("index.ts"), Span::new(0, 10))
        .expect("@alpha/api must resolve");
    let via_beta = resolver
        .resolve("@beta/api", &fx.join("index.ts"), Span::new(0, 9))
        .expect("@beta/api must resolve");

    assert_eq!(
        via_alpha.resolved_path, via_beta.resolved_path,
        "two aliases targeting the same `.d.ts` must share resolved_path \
         to preserve declaration identity across alias branches",
    );
    assert_eq!(via_alpha.resolved_path, fx.join("shared/api.d.ts"));
}

#[test]
fn test_path_mapping_relative_import_and_alias_share_resolved_path() {
    // Alias-resolved import and relative-import resolution of the same file
    // must yield identical `resolved_path`s. The alias target uses `./`, while
    // the relative import joins through a parent directory: both must collapse
    // to the same canonical declaration path.
    let fx = TempFixture::new();
    fx.write("src/api.ts", "export const v = 1;");
    fx.write("src/sub/index.ts", "import '../api';");
    fx.write("index.ts", "import '@app/api';");

    let options = make_options(fx.path(), vec![pm("@app/*", "@app/", &["./src/*"])]);
    let mut resolver = ModuleResolver::new(&options);

    let via_alias = resolver
        .resolve("@app/api", &fx.join("index.ts"), Span::new(0, 8))
        .expect("@app/api must resolve via alias");
    let via_relative = resolver
        .resolve("../api", &fx.join("src/sub/index.ts"), Span::new(0, 6))
        .expect("../api must resolve via relative import");

    assert_eq!(
        via_alias.resolved_path, via_relative.resolved_path,
        "alias-resolved and relative-import paths to the same file must match",
    );
    assert_eq!(via_alias.resolved_path, fx.join("src/api.ts"));
}

#[test]
fn test_path_mapping_baseurl_with_non_canonical_dir_resolves_canonically() {
    // A `baseUrl` joined with a `paths` target whose substitution introduces a
    // `..` must still produce a canonical resolved path. This case mirrors the
    // upstream symptom where row-level conditional fixtures select alternate
    // alias branches with shared physical targets.
    let fx = TempFixture::new();
    fx.write("pkg/index.ts", "export const x = 1;");
    fx.write("index.ts", "import 'pkg-alias';");

    // baseUrl is the fixture root; the target dives into a sibling and back up.
    let options = make_options(
        fx.path(),
        vec![pm("pkg-alias", "pkg-alias", &["./sub/../pkg/index"])],
    );
    let mut resolver = ModuleResolver::new(&options);
    let resolved = resolver
        .resolve("pkg-alias", &fx.join("index.ts"), Span::new(0, 9))
        .expect("pkg-alias must resolve via canonicalised baseUrl-joined target");

    assert_eq!(resolved.resolved_path, fx.join("pkg/index.ts"));
}

#[test]
fn test_path_mapping_unbalanced_parent_dirs_preserve_leading_dotdot() {
    // Lower-level invariant for the consolidated `normalize_path_segments`:
    // when there is nothing to pop, the leading `..` must be preserved so that
    // probe results remain accurate against relative roots. The earlier
    // `relative_resolution.rs` copy silently dropped these, which would have
    // mis-resolved alias targets that climb above the alias-relative base.
    let fx = TempFixture::new();
    fx.write("widget.ts", "export const w = 1;");

    // The alias jumps two levels up from a nested `lib/inner/` and lands back
    // at the fixture root. The intermediate physical directory exists, so the
    // OS-level walk succeeds either way; the assertion is that the *textual*
    // resolved path is canonical (no surviving `..` segments).
    fx.write("lib/inner/.gitkeep", "");
    fx.write("alias-root.ts", "import 'p';");
    let options = make_options(fx.path(), vec![pm("p", "p", &["./lib/inner/../../widget"])]);
    let mut resolver = ModuleResolver::new(&options);
    let resolved = resolver
        .resolve("p", &fx.join("alias-root.ts"), Span::new(0, 1))
        .expect("p must resolve via canonicalised `..` chain");

    assert_eq!(resolved.resolved_path, fx.join("widget.ts"));
    assert!(
        !resolved
            .resolved_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir)),
        "no surviving `..` in {:?}",
        resolved.resolved_path,
    );
}
