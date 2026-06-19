//! Tests for the interaction between tsconfig `paths` and package.json
//! `imports` for `#`-prefixed module specifiers.
//!
//! Structural rule: when a specifier begins with `#`, tsc consults tsconfig
//! `paths` first and only falls back to package.json `imports` if no `paths`
//! mapping resolves. tsz used to short-circuit straight to `imports`, which
//! broke every project that aliases `#/...` through `paths` (a common
//! convention in Next.js / Vite / modern TS codebases).
//!
//! Coverage matrix:
//! - bundler / nodenext / node10 resolution modes
//! - exact and wildcard `paths` mappings
//! - both `import` and `declare module` specifiers resolve to the same target
//! - `paths` priority over `imports` when both could match
//! - imports fallback when `paths` exists but does not match the specifier
//! - existing #-imports behavior (no `paths`) is preserved
//! - synthetic test that varies the alias prefix to prove the rule isn't
//!   hardcoded to a single spelling
use super::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use tsz::config::{CompilerOptions, resolve_compiler_options};

fn resolve_options(opts: CompilerOptions) -> ResolvedCompilerOptions {
    resolve_compiler_options(Some(&opts)).expect("resolve compiler options")
}

/// Build a `ResolvedCompilerOptions` for tests that exercise tsconfig `paths`.
/// Hides the `(resolution, module, [(pattern, [target])])` boilerplate that
/// otherwise dominates the test bodies.
fn options_with_paths(
    resolution: &str,
    module: &str,
    paths: &[(&str, &[&str])],
) -> ResolvedCompilerOptions {
    let mut raw_paths = FxHashMap::default();
    for (pattern, targets) in paths {
        raw_paths.insert(
            (*pattern).to_string(),
            targets.iter().map(|t| (*t).to_string()).collect(),
        );
    }
    resolve_options(CompilerOptions {
        paths: Some(raw_paths),
        module_resolution: Some(resolution.to_string()),
        module: Some(module.to_string()),
        ..Default::default()
    })
}

#[test]
fn hash_prefix_specifier_resolves_through_tsconfig_paths_in_every_mode() {
    // Same rule across bundler / node10 / nodenext: a #-prefixed specifier
    // must consult tsconfig `paths` before falling back to package `imports`.
    // Parameterized so a regression that fires in one mode but not the others
    // surfaces with the failing mode named in the message.
    let cases = [
        ("bundler", "esnext"),
        ("node10", "esnext"),
        ("nodenext", "nodenext"),
    ];
    for (mode, module) in cases {
        let options = options_with_paths(mode, module, &[("#/tools/*", &["./tools-*.ts"])]);
        let base = PathBuf::from(format!("/tmp/tsz-test-hash-paths-{mode}"));
        let mut known_files: FxHashSet<PathBuf> = FxHashSet::default();
        known_files.insert(base.join("tools-78.ts"));

        let mut cache = ModuleResolutionCache::default();
        let resolved = resolve_module_specifier(
            &base.join("src/test.ts"),
            "#/tools/78",
            &options,
            &base,
            &mut cache,
            &known_files,
        );
        assert_eq!(
            resolved,
            Some(base.join("tools-78.ts")),
            "tsconfig paths must resolve #-prefixed wildcard specifiers in {mode} mode"
        );
    }
}

#[test]
fn hash_prefix_specifier_resolves_through_exact_tsconfig_path() {
    let options = options_with_paths(
        "bundler",
        "esnext",
        &[("#/tools/exact", &["./other-name.ts"])],
    );
    let base = PathBuf::from("/tmp/tsz-test-hash-paths-exact");
    let mut known_files: FxHashSet<PathBuf> = FxHashSet::default();
    known_files.insert(base.join("other-name.ts"));

    let mut cache = ModuleResolutionCache::default();
    let resolved = resolve_module_specifier(
        &base.join("src/test.ts"),
        "#/tools/exact",
        &options,
        &base,
        &mut cache,
        &known_files,
    );
    assert_eq!(
        resolved,
        Some(base.join("other-name.ts")),
        "exact #-prefixed path mappings must resolve through tsconfig paths"
    );
}

#[test]
fn hash_prefix_augmentation_specifier_resolves_same_target_as_import() {
    // The kysely/nextjs/zod bench rows all hit the same chain: the *same*
    // `#/...` specifier appears in both `import type ... from '#/...'` and
    // `declare module '#/...' {}`. tsc resolves both to the same file index,
    // which is what makes the augmentation-merge machinery pair them up.
    // tsz used to drop both through the #-branch unconditionally, which is
    // why the augmented members never reached the imported alias.
    let options = options_with_paths("bundler", "esnext", &[("#/tools/*", &["./tools-*.ts"])]);
    let base = PathBuf::from("/tmp/tsz-test-hash-paths-augment-pair");
    let mut known_files: FxHashSet<PathBuf> = FxHashSet::default();
    known_files.insert(base.join("tools-78.ts"));

    let mut cache = ModuleResolutionCache::default();
    let resolved_import = resolve_module_specifier(
        &base.join("src/import-site.ts"),
        "#/tools/78",
        &options,
        &base,
        &mut cache,
        &known_files,
    );
    let resolved_augment = resolve_module_specifier(
        &base.join("src/augment-site.ts"),
        "#/tools/78",
        &options,
        &base,
        &mut cache,
        &known_files,
    );
    assert_eq!(
        resolved_import, resolved_augment,
        "the import-site and augmentation-site specifiers must resolve to the same target file \
         so that declaration merging can attach the augmented members to the imported alias"
    );
    assert_eq!(resolved_import, Some(base.join("tools-78.ts")));
}

#[test]
fn alias_prefix_is_not_hardcoded_to_hash_or_at() {
    // §25 ANTI-HARDCODING: the rule is "starts_with('#')", not "spelled
    // exactly '#/tools/78'". Vary the alias and the mapped target shape so a
    // regression that re-introduces a literal string would be visible.
    let cases = [
        ("#/feature/inner", "./feature-mod.ts"),
        ("#proto", "./proto.ts"),
        ("#util/deep/path", "./util-deep-path.ts"),
    ];
    for (specifier, target) in cases {
        let options = options_with_paths("bundler", "esnext", &[(specifier, &[target])]);
        let base = PathBuf::from("/tmp/tsz-test-hash-paths-vary");
        let resolved_target = base.join(target.trim_start_matches("./"));
        let mut known_files: FxHashSet<PathBuf> = FxHashSet::default();
        known_files.insert(resolved_target.clone());
        let mut cache = ModuleResolutionCache::default();
        let resolved = resolve_module_specifier(
            &base.join("src/test.ts"),
            specifier,
            &options,
            &base,
            &mut cache,
            &known_files,
        );
        assert_eq!(
            resolved,
            Some(resolved_target),
            "all #-prefixed specifiers should consult tsconfig paths, not just one spelling"
        );
    }
}

#[test]
fn hash_prefix_falls_back_to_package_imports_when_paths_does_not_match() {
    // `paths` exists but has nothing for `#/missing/*`. tsz must fall through
    // to package.json `imports`, not return None early.
    let dir = TempDir::new().expect("temp dir creation should succeed in test");
    let dir = dir.path();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("package.json"),
        r##"{"name":"demo","type":"module","imports":{"#/from-imports":"./from-imports.ts"}}"##,
    )
    .unwrap();
    // `from-imports.ts` only needs to exist on disk for the resolver's
    // `is_file()` probe; its contents are never read here.
    fs::write(dir.join("from-imports.ts"), "").unwrap();

    let mut raw_paths = FxHashMap::default();
    raw_paths.insert("#/other/*".to_string(), vec!["./other-*.ts".to_string()]);
    let options = resolve_options(CompilerOptions {
        paths: Some(raw_paths),
        module_resolution: Some("nodenext".to_string()),
        module: Some("nodenext".to_string()),
        ..Default::default()
    });

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/main.ts"),
        "#/from-imports",
        &options,
        dir,
        &mut cache,
        &known_files,
    );
    assert_eq!(
        resolved.map(|path| canonicalize_or_owned(&path)),
        Some(canonicalize_or_owned(&dir.join("from-imports.ts"))),
        "specifiers that don't match any `paths` entry must still resolve through \
         package.json `imports`"
    );
}

#[test]
fn hash_prefix_paths_take_priority_over_package_imports() {
    // Both `paths` and `imports` could match `#/conflict`. tsc honors
    // `paths` first — make sure tsz does the same.
    let dir = TempDir::new().expect("temp dir creation should succeed in test");
    let dir = dir.path();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("package.json"),
        r##"{"name":"demo","type":"module","imports":{"#/conflict":"./from-imports.ts"}}"##,
    )
    .unwrap();
    // Both targets need to exist on disk; the resolver only probes existence.
    fs::write(dir.join("from-imports.ts"), "").unwrap();
    fs::write(dir.join("from-paths.ts"), "").unwrap();

    let mut raw_paths = FxHashMap::default();
    raw_paths.insert(
        "#/conflict".to_string(),
        vec!["./from-paths.ts".to_string()],
    );
    let options = resolve_options(CompilerOptions {
        paths: Some(raw_paths),
        module_resolution: Some("nodenext".to_string()),
        module: Some("nodenext".to_string()),
        ..Default::default()
    });

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/main.ts"),
        "#/conflict",
        &options,
        dir,
        &mut cache,
        &known_files,
    );
    assert_eq!(
        resolved.map(|path| canonicalize_or_owned(&path)),
        Some(canonicalize_or_owned(&dir.join("from-paths.ts"))),
        "tsconfig `paths` must take priority over package.json `imports` for \
         #-prefixed specifiers"
    );
}

#[test]
fn hash_prefix_without_paths_still_uses_package_imports() {
    // Regression guard: removing the early return must not break the
    // existing #-only-imports flow.
    let dir = TempDir::new().expect("temp dir creation should succeed in test");
    let dir = dir.path();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("package.json"),
        r##"{"name":"demo","type":"module","imports":{"#/util":"./util.ts"}}"##,
    )
    .unwrap();
    fs::write(dir.join("util.ts"), "").unwrap();

    let options = resolve_options(CompilerOptions {
        module_resolution: Some("nodenext".to_string()),
        module: Some("nodenext".to_string()),
        ..Default::default()
    });

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/main.ts"),
        "#/util",
        &options,
        dir,
        &mut cache,
        &known_files,
    );
    assert_eq!(
        resolved.map(|path| canonicalize_or_owned(&path)),
        Some(canonicalize_or_owned(&dir.join("util.ts"))),
        "with no `paths` configured, #-specifiers must still resolve through \
         package.json `imports`"
    );
}

#[test]
fn hash_prefix_imports_resolve_only_against_nearest_package_scope() {
    // Per Node.js LOOKUP_PACKAGE_SCOPE, a `#`-import resolves only against the
    // nearest enclosing package.json. When that nearest scope has no `imports`
    // field, resolution fails — the resolver must NOT keep walking up to an
    // ancestor package whose `imports` happens to define the specifier. This is
    // the monorepo shape: a nested package without `imports`, under a root
    // package that defines `#shared`.
    let dir = TempDir::new().expect("temp dir creation should succeed in test");
    let dir = dir.path();
    fs::create_dir_all(dir.join("packages/inner/src")).unwrap();

    // Root package DOES define `#/shared`, with a real target on disk.
    fs::write(
        dir.join("package.json"),
        r##"{"name":"root","type":"module","imports":{"#/shared":"./shared.ts"}}"##,
    )
    .unwrap();
    fs::write(dir.join("shared.ts"), "").unwrap();

    // Nearest scope for the importer has NO `imports` field.
    fs::write(
        dir.join("packages/inner/package.json"),
        r##"{"name":"inner","type":"module"}"##,
    )
    .unwrap();

    let options = resolve_options(CompilerOptions {
        module_resolution: Some("nodenext".to_string()),
        module: Some("nodenext".to_string()),
        ..Default::default()
    });

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &dir.join("packages/inner/src/main.ts"),
        "#/shared",
        &options,
        dir,
        &mut cache,
        &known_files,
    );
    assert_eq!(
        resolved, None,
        "#/shared must not resolve against the ancestor root package once a \
         nearer package scope (without a matching import) is found"
    );
}

#[test]
fn hash_prefix_imports_no_match_in_nearest_scope_does_not_reach_ancestor() {
    // Variant: the nearest scope HAS an `imports` map, but no matching key.
    // tsc fails here too — it does not continue to ancestor package scopes.
    let dir = TempDir::new().expect("temp dir creation should succeed in test");
    let dir = dir.path();
    fs::create_dir_all(dir.join("packages/inner/src")).unwrap();

    fs::write(
        dir.join("package.json"),
        r##"{"name":"root","type":"module","imports":{"#/shared":"./shared.ts"}}"##,
    )
    .unwrap();
    fs::write(dir.join("shared.ts"), "").unwrap();

    // Nearest scope defines only an unrelated `#/local` key.
    fs::write(
        dir.join("packages/inner/package.json"),
        r##"{"name":"inner","type":"module","imports":{"#/local":"./local.ts"}}"##,
    )
    .unwrap();
    fs::write(dir.join("packages/inner/local.ts"), "").unwrap();

    let options = resolve_options(CompilerOptions {
        module_resolution: Some("nodenext".to_string()),
        module: Some("nodenext".to_string()),
        ..Default::default()
    });

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();

    // The nearest scope's own key resolves fine.
    let local = resolve_module_specifier(
        &dir.join("packages/inner/src/main.ts"),
        "#/local",
        &options,
        dir,
        &mut cache,
        &known_files,
    );
    assert_eq!(
        local.map(|path| canonicalize_or_owned(&path)),
        Some(canonicalize_or_owned(&dir.join("packages/inner/local.ts"))),
        "#/local must resolve against the nearest package scope"
    );

    // But the ancestor-only `#/shared` must not be reached.
    let shared = resolve_module_specifier(
        &dir.join("packages/inner/src/main.ts"),
        "#/shared",
        &options,
        dir,
        &mut cache,
        &known_files,
    );
    assert_eq!(
        shared, None,
        "#/shared (defined only in the ancestor) must not fall through past the \
         nearest import-bearing scope"
    );
}

#[test]
fn hash_prefix_invalid_for_node16_still_blocked_for_imports_after_paths_miss() {
    // In `node16` resolution, the bare `#/foo` form is invalid for package
    // imports. If `paths` doesn't match either, the function must return
    // None — it must NOT silently fall through to some other resolution
    // path that would resolve a relative file.
    let dir = TempDir::new().expect("temp dir creation should succeed in test");
    let dir = dir.path();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("package.json"),
        r##"{"name":"demo","type":"module","imports":{"#/foo":"./foo.ts"}}"##,
    )
    .unwrap();
    fs::write(dir.join("foo.ts"), "").unwrap();

    let options = resolve_options(CompilerOptions {
        module_resolution: Some("node16".to_string()),
        module: Some("node16".to_string()),
        ..Default::default()
    });

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/main.ts"),
        "#/foo",
        &options,
        dir,
        &mut cache,
        &known_files,
    );
    assert_eq!(
        resolved, None,
        "node16 must keep blocking #/-style specifiers when there is no `paths` match \
         and the form is invalid for package `imports`"
    );
}
