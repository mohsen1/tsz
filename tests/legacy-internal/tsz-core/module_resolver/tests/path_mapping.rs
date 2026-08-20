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

/// Bundler-mode options with the given `paths` and explicit `baseUrl` /
/// `pathsBasePath` anchors. The `paths`-without-`baseUrl` cases vary the
/// anchors; every other field is shared here so the tests differ only in what
/// they exercise.
fn make_options_with_anchors(
    base_url: Option<&std::path::Path>,
    paths_base_path: Option<&std::path::Path>,
    mappings: Vec<PathMapping>,
) -> ResolvedCompilerOptions {
    ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        base_url: base_url.map(std::path::Path::to_path_buf),
        paths_base_path: paths_base_path.map(std::path::Path::to_path_buf),
        paths: Some(mappings),
        module_suffixes: vec![String::new()],
        ..Default::default()
    }
}

fn make_options(dir: &std::path::Path, mappings: Vec<PathMapping>) -> ResolvedCompilerOptions {
    make_options_with_anchors(Some(dir), None, mappings)
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
// Tests vary alias spelling, target shape, and entry point to keep the fix
// structural rather than name-keyed. See `normalize_path_segments` docs.

#[test]
fn test_path_mapping_target_canonicalised_into_resolved_path() {
    // Rule covers `./X` (leading curdir), `./X/../Y` (embedded parent), and
    // non-wildcard `./X/../Y/Z` (no `*` in target). Each must produce a
    // resolved path with no surviving CurDir/ParentDir components.
    type PathMappingCanonicalizationRow<'a> = (&'a str, &'a str, &'a str, &'a [&'a str], &'a str);
    let rows: &[PathMappingCanonicalizationRow<'_>] = &[
        (
            "@app/*",
            "@app/",
            "@app/widget",
            &["./src/*"],
            "src/widget.ts",
        ),
        (
            "@util/*",
            "@util/",
            "@util/helpers",
            &["./lib/../shared/*"],
            "shared/helpers.ts",
        ),
        (
            "pkg-alias",
            "pkg-alias",
            "pkg-alias",
            &["./sub/../pkg/index"],
            "pkg/index.ts",
        ),
    ];
    for (pattern, prefix, specifier, targets, file) in rows {
        let fx = TempFixture::new();
        fx.write(file, "export const v = 1;");
        fx.write("index.ts", "");

        let options = make_options(fx.path(), vec![pm(pattern, prefix, targets)]);
        let mut resolver = ModuleResolver::new(&options);
        let resolved = resolver
            .resolve(specifier, &fx.join("index.ts"), Span::new(0, 1))
            .unwrap_or_else(|_| panic!("{specifier} should resolve via {targets:?}"));

        assert_eq!(resolved.resolved_path, fx.join(file));
        assert!(
            !resolved.resolved_path.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::CurDir | std::path::Component::ParentDir
                )
            }),
            "{specifier}: no `.`/`..` in {:?}",
            resolved.resolved_path,
        );
    }
}

#[test]
fn test_path_mapping_same_file_via_two_paths_shares_resolved_path() {
    // Two resolutions of one physical file — via different alias branches or
    // via an alias and a relative import — must produce identical
    // `resolved_path`s. Splits in this invariant fork declaration identity
    // in the project file graph.
    struct Row {
        file: &'static str,
        mappings: Vec<PathMapping>,
        a: (&'static str, &'static str), // (specifier, importer_rel)
        b: (&'static str, &'static str),
    }
    let rows = [
        // Two distinct alias keys reach the same `.d.ts`, one via `./` and
        // one via an embedded `..` detour.
        Row {
            file: "shared/api.d.ts",
            mappings: vec![
                pm("@alpha/*", "@alpha/", &["./shared/*.d.ts"]),
                pm("@beta/*", "@beta/", &["./lib/../shared/*.d.ts"]),
            ],
            a: ("@alpha/api", "index.ts"),
            b: ("@beta/api", "index.ts"),
        },
        // Alias resolution and relative-import resolution converge on the
        // same file; the relative import climbs through a parent directory.
        Row {
            file: "src/api.ts",
            mappings: vec![pm("@app/*", "@app/", &["./src/*"])],
            a: ("@app/api", "index.ts"),
            b: ("../api", "src/sub/index.ts"),
        },
    ];
    for row in rows {
        let fx = TempFixture::new();
        fx.write(row.file, "export const v = 1;");
        for (_, importer) in [row.a, row.b] {
            fx.write(importer, "");
        }

        let options = make_options(fx.path(), row.mappings);
        let mut resolver = ModuleResolver::new(&options);
        let via_a = resolver
            .resolve(row.a.0, &fx.join(row.a.1), Span::new(0, 1))
            .unwrap_or_else(|_| panic!("{} must resolve", row.a.0));
        let via_b = resolver
            .resolve(row.b.0, &fx.join(row.b.1), Span::new(0, 1))
            .unwrap_or_else(|_| panic!("{} must resolve", row.b.0));

        assert_eq!(
            via_a.resolved_path, via_b.resolved_path,
            "{} and {} must produce equal resolved_path",
            row.a.0, row.b.0,
        );
        assert_eq!(via_a.resolved_path, fx.join(row.file));
    }
}

// ── single best-pattern selection (issue #11577) ─────────────────────────────
//
// tsc selects exactly one `paths` pattern (`matchPatternOrExact` ->
// `findBestPatternMatch`) and probes only that pattern's targets. A missing
// target under the chosen pattern must NOT fall through to a less-specific
// pattern (notably a catch-all `"*"`). The witness rows vary the binder names
// so the rule stays structural rather than fixture-keyed.

#[test]
fn test_path_mapping_missing_specific_target_does_not_fall_through_to_catch_all() {
    // The specific pattern matches but its on-disk target is missing. tsc
    // commits to that pattern and reports the module unresolved; it does NOT
    // resolve via the catch-all `"*"`. A specifier that only matches the
    // catch-all still resolves through it.
    let rows: &[(&str, &str, &[&str])] = &[
        ("next/dist/*", "next/dist/", &["./src/*"]),
        ("@scope/*", "@scope/", &["./packages/*"]),
        ("lib/*", "lib/", &["./internal/*"]),
    ];
    for (pattern, prefix, targets) in rows {
        let fx = TempFixture::new();
        // The catch-all target exists; the specific target does not.
        fx.write("external.d.ts", "declare const v: any; export default v;");
        fx.write("index.ts", "");

        let options = make_options(
            fx.path(),
            vec![
                pm(pattern, prefix, targets),
                pm("*", "", &["./external.d.ts"]),
            ],
        );
        let mut resolver = ModuleResolver::new(&options);

        // Matches the specific pattern, whose target is missing: must fail
        // rather than fall through to the catch-all `external.d.ts`.
        let specific_specifier = format!("{prefix}missing-entry");
        let missed = resolver.resolve(&specific_specifier, &fx.join("index.ts"), Span::new(0, 1));
        assert!(
            missed.is_err(),
            "{specific_specifier}: a missing target under {pattern} must not fall through to the \"*\" catch-all",
        );

        // A specifier that only the catch-all matches still resolves via it.
        let catch_all = resolver
            .resolve(
                "totally-unrelated-pkg",
                &fx.join("index.ts"),
                Span::new(0, 1),
            )
            .expect("catch-all \"*\" must still resolve specifiers no other pattern matches");
        assert_eq!(catch_all.resolved_path, fx.join("external.d.ts"));
    }
}

#[test]
fn test_path_mapping_exact_key_beats_equal_prefix_wildcard() {
    // `matchPatternOrExact` returns an exact, wildcard-free key before
    // consulting any wildcard. `"foo"` and `"foo*"` tie on prefix length for
    // the specifier `"foo"`, but the literal key must win — even when the
    // wildcard is listed first and the wildcard target also exists on disk.
    let fx = TempFixture::new();
    fx.write("exact.ts", "export const exact = 1;");
    fx.write("wild.ts", "export const wild = 1;");
    fx.write("index.ts", "");

    let options = make_options(
        fx.path(),
        vec![
            pm("alias*", "alias", &["./wild.ts"]),
            pm("alias", "alias", &["./exact.ts"]),
        ],
    );
    let mut resolver = ModuleResolver::new(&options);
    let resolved = resolver
        .resolve("alias", &fx.join("index.ts"), Span::new(0, 1))
        .expect("exact key \"alias\" must resolve");
    assert_eq!(
        resolved.resolved_path,
        fx.join("exact.ts"),
        "exact wildcard-free key must outrank an equal-prefix wildcard",
    );
}

#[test]
fn test_path_mapping_longest_prefix_wins_on_miss_without_fallthrough() {
    // Three nested patterns match `next/dist/compiled/x`. The longest-prefix
    // pattern (`next/dist/compiled/*`) is chosen; when its target is missing,
    // resolution fails rather than retrying `next/dist/*` or `"*"`.
    let fx = TempFixture::new();
    // Targets for the two *less* specific patterns exist; only the chosen
    // (most specific) pattern's target is missing.
    fx.write("src/compiled/react.ts", "export const r = 1;");
    fx.write("external.d.ts", "declare const v: any; export default v;");
    fx.write("index.ts", "");

    let options = make_options(
        fx.path(),
        vec![
            pm(
                "next/dist/compiled/*",
                "next/dist/compiled/",
                &["./missing/*"],
            ),
            pm("next/dist/*", "next/dist/", &["./src/*"]),
            pm("*", "", &["./external.d.ts"]),
        ],
    );
    let mut resolver = ModuleResolver::new(&options);
    let missed = resolver.resolve(
        "next/dist/compiled/react",
        &fx.join("index.ts"),
        Span::new(0, 1),
    );
    assert!(
        missed.is_err(),
        "longest-prefix pattern is chosen; its missing target must not fall through to next/dist/* \
         (which has ./src/compiled/react.ts) or the \"*\" catch-all",
    );
}

#[test]
fn test_path_mapping_matched_pattern_miss_skips_base_url_fallback() {
    // tsc reaches the bare `baseUrl` join only when NO `paths` pattern matched.
    // When a pattern matched but its target is missing, baseUrl is skipped. The
    // control row (a specifier matching no pattern) still resolves via baseUrl.
    let fx = TempFixture::new();
    // `baseUrl + "shared/widget"` exists on disk, so the only way these
    // resolutions could succeed is the (incorrect) baseUrl fallback.
    fx.write("shared/widget.ts", "export const w = 1;");
    fx.write("loose/thing.ts", "export const t = 1;");
    fx.write("index.ts", "");

    let options = make_options(
        fx.path(),
        vec![pm("shared/*", "shared/", &["./nonexistent/*"])],
    );
    let mut resolver = ModuleResolver::new(&options);

    // `shared/widget` matches `shared/*`; its target `./nonexistent/widget` is
    // missing. baseUrl must be skipped, so resolution fails.
    let matched_miss = resolver.resolve("shared/widget", &fx.join("index.ts"), Span::new(0, 1));
    assert!(
        matched_miss.is_err(),
        "a matched-but-missing pattern must skip the baseUrl fallback (tsc commits to the pattern)",
    );

    // `loose/thing` matches no pattern, so baseUrl resolution still applies.
    let unmatched = resolver
        .resolve("loose/thing", &fx.join("index.ts"), Span::new(0, 1))
        .expect("a specifier matching no pattern must still resolve via baseUrl");
    assert_eq!(unmatched.resolved_path, fx.join("loose/thing.ts"));
}

// ── `paths` without `baseUrl` (TypeScript 4.1+) ───────────────────────────────
//
// Since TS 4.1, `paths` may be configured without `baseUrl`. Relative
// substitutions then resolve against the directory of the tsconfig that
// declared them — tsc's `pathsBasePath` (`getPathsBasePath` returns
// `baseUrl ?? pathsBasePath`). Non-relative substitutions stay rejected at the
// config layer, and the bare `baseUrl` join fallback must NOT activate, so an
// unmapped specifier still fails rather than resolving against the config dir.

/// Options with `paths` but no `baseUrl`; `paths_base_path` carries the config
/// directory (what the CLI driver supplies as tsc's `pathsBasePath`).
fn make_options_paths_only(
    config_dir: &std::path::Path,
    mappings: Vec<PathMapping>,
) -> ResolvedCompilerOptions {
    make_options_with_anchors(None, Some(config_dir), mappings)
}

#[test]
fn test_path_mapping_without_base_url_resolves_against_config_dir() {
    // A wildcard alias with a relative substitution resolves against the
    // config directory even though `baseUrl` is unset. Binder names vary across
    // rows so the rule stays structural, not keyed on any alias spelling.
    struct Row {
        pattern: &'static str,
        prefix: &'static str,
        target: &'static str,
        on_disk: &'static str,
        specifier: &'static str,
    }
    let rows = [
        Row {
            pattern: "@app/*",
            prefix: "@app/",
            target: "./src/*",
            on_disk: "src/widget.ts",
            specifier: "@app/widget",
        },
        Row {
            pattern: "~/*",
            prefix: "~/",
            target: "./lib/*",
            on_disk: "lib/nested/thing.ts",
            specifier: "~/nested/thing",
        },
        Row {
            pattern: "internal",
            prefix: "internal",
            target: "./types/internal.d.ts",
            on_disk: "types/internal.d.ts",
            specifier: "internal",
        },
    ];
    for row in rows {
        let fx = TempFixture::new();
        fx.write(row.on_disk, "export const value = 1;");
        fx.write("index.ts", "");

        let options =
            make_options_paths_only(fx.path(), vec![pm(row.pattern, row.prefix, &[row.target])]);
        let mut resolver = ModuleResolver::new(&options);
        let resolved = resolver
            .resolve(row.specifier, &fx.join("index.ts"), Span::new(0, 1))
            .unwrap_or_else(|e| {
                panic!(
                    "{}: paths without baseUrl must resolve against the config dir, got {e:?}",
                    row.specifier,
                )
            });
        assert_eq!(
            resolved.resolved_path,
            fx.join(row.on_disk),
            "{}",
            row.specifier
        );
    }
}

#[test]
fn test_path_mapping_without_base_url_unmapped_specifier_does_not_fall_back() {
    // The bare `baseUrl` join fallback must stay anchored on `baseUrl`. With
    // `paths` but no `baseUrl`, a specifier that matches no pattern must NOT
    // resolve against the config dir — `shared/widget.ts` exists on disk, so
    // the only way it could resolve is the (incorrect) baseUrl-style fallback.
    let fx = TempFixture::new();
    fx.write("shared/widget.ts", "export const w = 1;");
    fx.write("index.ts", "");

    let options = make_options_paths_only(fx.path(), vec![pm("@app/*", "@app/", &["./src/*"])]);
    let mut resolver = ModuleResolver::new(&options);

    let unmapped = resolver.resolve("shared/widget", &fx.join("index.ts"), Span::new(0, 1));
    assert!(
        unmapped.is_err(),
        "without baseUrl, an unmapped specifier must not resolve via a baseUrl-style \
         fallback against the config dir, got {unmapped:?}",
    );
}

#[test]
fn test_path_mapping_without_base_url_or_paths_base_is_skipped() {
    // Defensive: with neither `baseUrl` nor `paths_base_path`, path mapping has
    // no anchor and is skipped entirely (resolution fails) rather than panicking
    // or resolving against an arbitrary directory.
    let fx = TempFixture::new();
    fx.write("src/widget.ts", "export const w = 1;");
    fx.write("index.ts", "");

    let options = make_options_with_anchors(None, None, vec![pm("@app/*", "@app/", &["./src/*"])]);
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("@app/widget", &fx.join("index.ts"), Span::new(0, 1));
    assert!(
        result.is_err(),
        "path mapping with no baseUrl and no paths base must be skipped, got {result:?}",
    );
}

#[test]
fn test_path_mapping_base_url_takes_precedence_over_paths_base() {
    // When both are present, `baseUrl` wins (tsc's `baseUrl ?? pathsBasePath`).
    // The alias target resolves against `baseUrl`'s directory, not the config
    // dir, proving the precedence.
    let base = TempFixture::new();
    base.write("src/widget.ts", "export const fromBaseUrl = 1;");
    base.write("index.ts", "");

    let other = TempFixture::new();
    other.write("src/widget.ts", "export const fromConfigDir = 1;");

    let options = make_options_with_anchors(
        Some(base.path()),
        Some(other.path()),
        vec![pm("@app/*", "@app/", &["./src/*"])],
    );
    let mut resolver = ModuleResolver::new(&options);
    let resolved = resolver
        .resolve("@app/widget", &base.join("index.ts"), Span::new(0, 1))
        .expect("alias must resolve via baseUrl when both anchors are set");
    assert_eq!(resolved.resolved_path, base.join("src/widget.ts"));
}

#[test]
fn test_path_mapping_unbalanced_parent_dirs_preserve_leading_dotdot() {
    // When there is nothing to pop, the leading `..` must be preserved. The
    // earlier `relative_resolution.rs` copy silently dropped these, which
    // would have mis-resolved alias targets that climb above the alias base.
    // Normalization is filesystem-independent, so the intermediate
    // `lib/inner/` need not exist on disk.
    let fx = TempFixture::new();
    fx.write("widget.ts", "export const w = 1;");
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

// ── relative / rooted specifiers bypass `paths` ──────────────────────────────
//
// tsc consults tsconfig `paths`/`baseUrl` only for module names that are NOT
// relative and NOT rooted (`tryLoadModuleUsingOptionalResolutionSettings` is
// gated on `!isExternalModuleNameRelative`, i.e. `pathIsRelative ||
// isRootedDiskPath`). A catch-all `"*"` pattern matches *every* string,
// including `./sibling`, so without that guard the core resolver intercepts
// relative imports and resolves them to the catch-all stub — a wrong module
// identity that surfaces as false `TS2307`/`TS2614`/`TS2305`. The witness rows
// vary the binder names and the relative shape so the rule stays structural.

#[test]
fn test_path_mapping_catch_all_does_not_intercept_relative_imports() {
    // The catch-all `"*"` must not capture relative imports (see banner above).
    struct Row {
        sibling: &'static str,
        importer: &'static str,
        specifier: &'static str,
    }
    let rows = [
        Row {
            sibling: "sibling.ts",
            importer: "main.ts",
            specifier: "./sibling",
        },
        Row {
            sibling: "shared/api.ts",
            importer: "shared/sub/consumer.ts",
            specifier: "../api",
        },
        Row {
            sibling: "feature/nested/widget.ts",
            importer: "feature/host.ts",
            specifier: "./nested/widget",
        },
    ];
    for row in rows {
        let fx = TempFixture::new();
        fx.write(row.sibling, "export const value = 1;");
        fx.write(row.importer, "");
        // The catch-all target exists on disk; the relative target must still
        // win, proving `paths` was not consulted for the relative specifier.
        fx.write("stub.d.ts", "declare const v: any; export default v;");

        let options = make_options(fx.path(), vec![pm("*", "", &["./stub.d.ts"])]);
        let mut resolver = ModuleResolver::new(&options);
        let resolved = resolver
            .resolve(row.specifier, &fx.join(row.importer), Span::new(0, 1))
            .unwrap_or_else(|_| panic!("{} must resolve relative to its importer", row.specifier));
        assert_eq!(
            resolved.resolved_path,
            fx.join(row.sibling),
            "{}: a relative import must resolve to its sibling file, not the \
             catch-all \"*\" stub",
            row.specifier,
        );
    }
}

#[test]
fn test_path_mapping_catch_all_still_resolves_bare_specifiers_alongside_relative() {
    // Control for the guard above: with the same catch-all `"*"` mapping, a
    // *bare* (non-relative) specifier must still resolve through it. The guard
    // only excludes relative/rooted names, not bare module names.
    let fx = TempFixture::new();
    fx.write("sibling.ts", "export const value = 1;");
    fx.write("stub.d.ts", "declare const v: any; export default v;");
    fx.write("main.ts", "");

    let options = make_options(fx.path(), vec![pm("*", "", &["./stub.d.ts"])]);
    let mut resolver = ModuleResolver::new(&options);

    // Relative: resolves to the sibling, bypassing the catch-all.
    let relative = resolver
        .resolve("./sibling", &fx.join("main.ts"), Span::new(0, 1))
        .expect("./sibling must resolve to the sibling file");
    assert_eq!(relative.resolved_path, fx.join("sibling.ts"));

    // Bare: still flows through the catch-all to the stub.
    let bare = resolver
        .resolve("some-bare-pkg", &fx.join("main.ts"), Span::new(0, 1))
        .expect("a bare specifier must still resolve through the catch-all");
    assert_eq!(bare.resolved_path, fx.join("stub.d.ts"));
}
