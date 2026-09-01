use super::*;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz::config::{CompilerOptions, resolve_compiler_options};
use tsz::emitter::ModuleKind;

#[path = "resolution_tests/fixtures.rs"]
mod fixtures;

use fixtures::*;

#[test]
fn test_module_resolution_file_exists_cache_is_per_cache() {
    use std::fs;

    let dir = tempfile::TempDir::new().expect("temp dir creation should succeed in test");
    let candidate = dir.path().join("late.ts");

    let mut cache = ModuleResolutionCache::default();
    assert!(!cache.file_exists(&candidate));

    fs::write(&candidate, "export {};").unwrap();
    assert!(
        !cache.file_exists(&candidate),
        "file-existence misses are a per-resolution-run snapshot"
    );

    let mut fresh_cache = ModuleResolutionCache::default();
    assert!(
        fresh_cache.file_exists(&candidate),
        "file-existence misses must not leak across resolution caches"
    );
}

#[test]
fn test_implied_resolution_mode_reuses_package_type_cache_for_sibling_dirs() {
    use std::fs;

    let dir = tempfile::TempDir::new().expect("temp dir creation should succeed in test");
    fs::write(dir.path().join("package.json"), r#"{"name":"fixture"}"#).unwrap();

    let mut cache = ModuleResolutionCache::default();
    for idx in 0..5 {
        let child = dir.path().join(format!("lib/part{idx}"));
        fs::create_dir_all(&child).unwrap();
        let file = child.join("index.ts");
        assert_eq!(
            implied_resolution_mode_for_file_with_cache(&file, dir.path(), &mut cache),
            "require"
        );
    }

    assert_eq!(
        cache.package_json_by_path.len(),
        1,
        "sibling package-type probes should parse only the nearest real package.json"
    );
    assert!(
        cache.package_type_by_dir.len() >= 5,
        "sibling directories should be memoized after the package-type walk"
    );
}

#[test]
fn test_preserve_symlinks_keeps_symlink_path_identity() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let dir = std::env::temp_dir().join("tsz_driver_resolution_preserve_symlinks");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("real")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(dir.join("real/index.d.ts"), "export interface Box {}").unwrap();
    symlink(dir.join("real"), dir.join("linked")).unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import type { Box } from '../linked';\nexport type T = Box;",
    )
    .unwrap();

    let symlink_path = dir.join("linked/index.d.ts");
    let real_path = canonicalize_or_owned(&dir.join("real/index.d.ts"));
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();

    let preserve_options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        preserve_symlinks: true,
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
    let mut preserve_cache = ModuleResolutionCache::default();
    let preserved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "../linked",
        &preserve_options,
        &dir,
        &mut preserve_cache,
        &known_files,
    );
    assert_eq!(preserved, Some(symlink_path));

    let realpath_options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        preserve_symlinks: false,
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
    let mut realpath_cache = ModuleResolutionCache::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "../linked",
        &realpath_options,
        &dir,
        &mut realpath_cache,
        &known_files,
    );
    assert_eq!(resolved, Some(real_path));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_normalize_resolved_path_preserves_symlink_ancestor_identity() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let dir = std::env::temp_dir().join("tsz_driver_resolution_symlink_ancestor");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("core/node_modules/package-a")).unwrap();
    fs::write(
        dir.join("core/node_modules/package-a/index.d.ts"),
        "export interface Box {}",
    )
    .unwrap();
    symlink(
        dir.join("core/node_modules/package-a"),
        dir.join("package-a"),
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        preserve_symlinks: false,
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

    let symlinked_path = dir.join("package-a/index.d.ts");
    assert_eq!(
        normalize_resolved_path(&symlinked_path, &options),
        symlinked_path,
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolve_module_specifier_from_node_modules_package_finds_sibling_package() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_node_modules_sibling");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/baz")).unwrap();
    fs::create_dir_all(dir.join("node_modules/foo")).unwrap();

    fs::write(
        dir.join("node_modules/baz/index.d.ts"),
        "export { T } from \"foo\";",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/foo/index.d.ts"),
        "export type T = number;",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();

    let resolved = resolve_module_specifier(
        &dir.join("node_modules/baz/index.d.ts"),
        "foo",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved,
        Some(canonicalize_or_owned(
            &dir.join("node_modules/foo/index.d.ts")
        ))
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `/// <reference types="..." />` from inside a pnpm-symlinked `@types/*`
/// package must reach the package's *transitive* `@types/*` sibling, which
/// lives only inside the realpath `.pnpm/<pkg>@<ver>/node_modules` sandbox and
/// is never hoisted to the top-level `node_modules/@types`.
#[test]
fn test_type_reference_resolves_transitive_sibling_in_pnpm_symlink_sandbox() {
    let (dir, sandbox) = setup_pnpm_express_sandbox(
        "tsz_driver_resolution_pnpm_triple_slash",
        "/// <reference types=\"express-serve-static-core\" />\nexport {};",
    );

    let options = pnpm_symlink_test_options(false);
    let mut cache = ModuleResolutionCache::default();

    // Resolve from the *symlink* path, exactly as file discovery records it.
    let from_file = dir.join("node_modules/@types/express/index.d.ts");
    let resolved = resolve_type_reference_from_node_modules_with_cache(
        "express-serve-static-core",
        &from_file,
        &dir,
        None,
        &options,
        &mut cache,
    );

    let expected = canonicalize_or_owned(&sandbox.join("express-serve-static-core/index.d.ts"));
    assert_eq!(resolved.map(|p| canonicalize_or_owned(&p)), Some(expected));

    let _ = std::fs::remove_dir_all(&dir);
}

/// `preserveSymlinks` keeps `tsc`'s symlink-path resolution: the transitive
/// sibling is *not* visible from the top-level symlink path, so the reference
/// stays unresolved. This pins the gate so the realpath anchor only applies
/// when symlinks are followed.
#[test]
fn test_type_reference_preserve_symlinks_does_not_reach_sandbox_sibling() {
    let (dir, _sandbox) = setup_pnpm_express_sandbox(
        "tsz_driver_resolution_pnpm_triple_slash_preserve",
        "export {};",
    );

    let options = pnpm_symlink_test_options(true);
    let mut cache = ModuleResolutionCache::default();
    let from_file = dir.join("node_modules/@types/express/index.d.ts");
    let resolved = resolve_type_reference_from_node_modules_with_cache(
        "express-serve-static-core",
        &from_file,
        &dir,
        None,
        &options,
        &mut cache,
    );

    assert_eq!(resolved, None);

    let _ = std::fs::remove_dir_all(&dir);
}

/// A bare `import`/`require` from inside a pnpm-symlinked package must reach the
/// package's transitive dependency in the realpath sandbox — the same root
/// cause as the triple-slash case, exercised through the bare-specifier
/// walk-up.
#[test]
fn test_bare_import_resolves_transitive_dependency_in_pnpm_symlink_sandbox() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let dir = std::env::temp_dir().join("tsz_driver_resolution_pnpm_bare_import");
    let _ = fs::remove_dir_all(&dir);
    let sandbox = dir.join("node_modules/.pnpm/@types+react@19.0.0/node_modules");
    fs::create_dir_all(sandbox.join("@types/react")).unwrap();
    fs::create_dir_all(sandbox.join("csstype")).unwrap();
    fs::write(
        sandbox.join("@types/react/index.d.ts"),
        "import type { Properties } from \"csstype\";\nexport type R = Properties;",
    )
    .unwrap();
    fs::write(
        sandbox.join("csstype/package.json"),
        "{\"name\":\"csstype\",\"version\":\"3.0.0\",\"types\":\"index.d.ts\"}",
    )
    .unwrap();
    fs::write(
        sandbox.join("csstype/index.d.ts"),
        "export interface Properties {}",
    )
    .unwrap();
    fs::create_dir_all(dir.join("node_modules/@types")).unwrap();
    symlink(
        sandbox.join("@types/react"),
        dir.join("node_modules/@types/react"),
    )
    .unwrap();

    let options = pnpm_symlink_test_options(false);
    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();

    let from_file = dir.join("node_modules/@types/react/index.d.ts");
    let resolved = resolve_module_specifier(
        &from_file,
        "csstype",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    let expected = canonicalize_or_owned(&sandbox.join("csstype/index.d.ts"));
    assert_eq!(resolved.map(|p| canonicalize_or_owned(&p)), Some(expected));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_normalize_resolved_path_collapses_segments_for_symlinked_package_identity() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let dir = std::env::temp_dir().join("tsz_driver_resolution_symlink_segment_normalization");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("core/node_modules/package-a/types")).unwrap();
    fs::write(
        dir.join("core/node_modules/package-a/index.d.ts"),
        "export interface Box {}",
    )
    .unwrap();
    symlink(
        dir.join("core/node_modules/package-a"),
        dir.join("package-a"),
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        preserve_symlinks: false,
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

    let raw_path = dir.join("package-a/./types/../index.d.ts");
    let normalized_path = dir.join("package-a/index.d.ts");
    assert_eq!(
        normalize_resolved_path(&raw_path, &options),
        normalized_path,
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_exports_js_target_substitutes_dts() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_exports_js_target");
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
        "pkg",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved,
        Some(canonicalize_or_owned(
            &dir.join("node_modules/pkg/entrypoint.d.ts"),
        )),
        "exports target with .js should resolve to an adjacent declaration file"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_exports_target_cannot_escape_package_root() {
    use std::fs;
    let dir = tempfile::TempDir::new().expect("temp dir creation should succeed in test");
    let root = dir.path();
    fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();

    fs::write(
        root.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{"./leak":"../leak.d.ts"}}"#,
    )
    .unwrap();
    fs::write(
        root.join("node_modules/leak.d.ts"),
        "export declare const value: number;",
    )
    .unwrap();
    fs::write(
        root.join("src/index.ts"),
        "import { value } from 'pkg/leak';",
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
    let resolved = resolve_module_specifier(
        &root.join("src/index.ts"),
        "pkg/leak",
        &options,
        root,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved, None,
        "exports target escaping the package root must not resolve"
    );
}

#[test]
fn test_package_imports_absolute_target_is_invalid() {
    use std::fs;
    let dir = tempfile::TempDir::new().expect("temp dir creation should succeed in test");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();

    let abs_target = root.join("abs.d.ts").to_string_lossy().into_owned();
    fs::write(root.join("abs.d.ts"), "export declare const value: number;").unwrap();
    fs::write(
        root.join("package.json"),
        serde_json::json!({
            "name": "app",
            "imports": {
                "#abs": abs_target
            }
        })
        .to_string(),
    )
    .unwrap();
    fs::write(root.join("src/index.ts"), "import { value } from '#abs';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_imports: true,
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
        &root.join("src/index.ts"),
        "#abs",
        &options,
        root,
        &mut cache,
        &known_files,
    );

    assert_eq!(resolved, None, "absolute imports target must not resolve");
}

#[test]
fn test_duplicate_package_redirects_prefer_stable_lexical_root_when_depth_ties() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_driver_resolution_duplicate_package_tie_break");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/@types/react")).unwrap();
    fs::create_dir_all(dir.join("tests/node_modules/@types/react")).unwrap();

    fs::write(
        dir.join("node_modules/@types/react/package.json"),
        r#"{"name":"@types/react","version":"16.4.6"}"#,
    )
    .unwrap();
    fs::write(
        dir.join("tests/node_modules/@types/react/package.json"),
        r#"{"name":"@types/react","version":"16.4.6"}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/@types/react/index.d.ts"),
        "declare global {}",
    )
    .unwrap();
    fs::write(dir.join("tests/node_modules/@types/react/index.d.ts"), "").unwrap();

    let options = ResolvedCompilerOptions {
        module_suffixes: vec![String::new()],
        ..Default::default()
    };

    let redirects = build_duplicate_package_redirects(
        &[
            dir.join("node_modules/@types/react/index.d.ts")
                .display()
                .to_string(),
            dir.join("tests/node_modules/@types/react/index.d.ts")
                .display()
                .to_string(),
        ],
        &options,
    );

    let root_index = canonicalize_or_owned(&dir.join("node_modules/@types/react/index.d.ts"));
    let tests_index =
        canonicalize_or_owned(&dir.join("tests/node_modules/@types/react/index.d.ts"));

    assert_eq!(
        redirects.get(&tests_index),
        Some(&root_index),
        "same-depth duplicate packages should deterministically redirect to the lexical root copy"
    );
    assert!(
        !redirects.contains_key(&root_index),
        "canonical root package file should not redirect away"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_duplicate_package_redirects_flatten_chains_to_final_canonical_root() {
    // Three duplicate package copies at three distinct depths. Iteration over
    // the package-roots set is hash-order, so any intermediate "winner" must
    // not leave a stale redirect pointing at it: every non-canonical copy
    // must rewrite directly to the depth-1 canonical, regardless of which
    // order the build pass visited them.
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_driver_resolution_duplicate_package_chain");
    let _ = fs::remove_dir_all(&dir);

    let [file1, file2, file3] = make_duplicate_package_copies(
        &dir,
        "pkg",
        "2.3.4",
        [
            "node_modules/pkg",
            "node_modules/host/node_modules/pkg",
            "node_modules/host/node_modules/sub/node_modules/pkg",
        ],
    );

    let options = dup_pkg_resolver_options();

    let redirects = build_duplicate_package_redirects(
        &[
            file1.display().to_string(),
            file2.display().to_string(),
            file3.display().to_string(),
        ],
        &options,
    );

    let canonical1 = canonicalize_or_owned(&file1);
    let canonical2 = canonicalize_or_owned(&file2);
    let canonical3 = canonicalize_or_owned(&file3);

    assert!(
        !redirects.contains_key(&canonical1),
        "the depth-1 canonical copy must never redirect away from itself, got: {redirects:?}"
    );
    assert_eq!(
        redirects.get(&canonical2),
        Some(&canonical1),
        "depth-2 copy must redirect directly to the depth-1 canonical (no chains), got: {redirects:?}"
    );
    assert_eq!(
        redirects.get(&canonical3),
        Some(&canonical1),
        "depth-3 copy must redirect directly to the depth-1 canonical (no chains), got: {redirects:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_duplicate_package_redirects_deterministic_across_input_orders() {
    // The redirect map must be a pure function of the input file set: shuffling
    // the input order must produce the same redirect map. This guards against
    // hash-set iteration order leaking into the canonical-root choice.
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_driver_resolution_duplicate_package_determinism");
    let _ = fs::remove_dir_all(&dir);

    let [file1_path, file2_path, file3_path] = make_duplicate_package_copies(
        &dir,
        "det-pkg",
        "0.1.0",
        [
            "node_modules/det-pkg",
            "node_modules/wrap/node_modules/det-pkg",
            "node_modules/wrap/node_modules/inner/node_modules/det-pkg",
        ],
    );

    let options = dup_pkg_resolver_options();

    let file1 = file1_path.display().to_string();
    let file2 = file2_path.display().to_string();
    let file3 = file3_path.display().to_string();

    // The pure-function property is "any reordering yields the same map".
    // Three representative orderings cover the regression: sorted, reversed,
    // and a rotation that visits the depth-3 (deepest) copy first — the
    // ordering that originally exposed the stale-chain redirect bug.
    let permutations = [
        [file1.clone(), file2.clone(), file3.clone()],
        [file3.clone(), file2.clone(), file1.clone()],
        [file3, file1, file2],
    ];

    let mut maps = Vec::new();
    for inputs in &permutations {
        let map = build_duplicate_package_redirects(inputs, &options);
        let mut entries: Vec<(PathBuf, PathBuf)> = map.into_iter().collect();
        entries.sort();
        maps.push(entries);
    }
    let first = &maps[0];
    for (i, m) in maps.iter().enumerate().skip(1) {
        assert_eq!(
            first, m,
            "duplicate-package redirects must not depend on input order; permutation {i} diverged"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_exports_runtime_targets_substitute_matching_declaration_sidecars() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_exports_sidecars");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{
            "name":"pkg",
            "type":"module",
            "exports":{
                ".":"./index.js",
                "./mjs":"./entry.mjs",
                "./cjs":"./entry.cjs"
            }
        }"#,
    )
    .unwrap();
    fs::write(dir.join("node_modules/pkg/index.d.ts"), "export {};").unwrap();
    fs::write(dir.join("node_modules/pkg/entry.d.mts"), "export {};").unwrap();
    fs::write(dir.join("node_modules/pkg/entry.d.cts"), "export = 1;").unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import 'pkg'; import 'pkg/mjs'; import 'pkg/cjs';",
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

    assert_eq!(
        resolve_module_specifier(
            &dir.join("src/index.ts"),
            "pkg",
            &options,
            &dir,
            &mut cache,
            &known_files,
        ),
        Some(canonicalize_or_owned(
            &dir.join("node_modules/pkg/index.d.ts"),
        ))
    );
    assert_eq!(
        resolve_module_specifier(
            &dir.join("src/index.ts"),
            "pkg/mjs",
            &options,
            &dir,
            &mut cache,
            &known_files,
        ),
        Some(canonicalize_or_owned(
            &dir.join("node_modules/pkg/entry.d.mts"),
        ))
    );
    assert_eq!(
        resolve_module_specifier(
            &dir.join("src/index.ts"),
            "pkg/cjs",
            &options,
            &dir,
            &mut cache,
            &known_files,
        ),
        Some(canonicalize_or_owned(
            &dir.join("node_modules/pkg/entry.d.cts"),
        ))
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_exports_directory_key_does_not_expose_arbitrary_subpaths() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_exports_directory_key");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/inner")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/inner/package.json"),
        r#"{
            "name":"inner",
            "type":"module",
            "exports":{
                "./":"./"
            }
        }"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/other.d.ts"),
        "export interface Thing {}\n",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/index.d.ts"),
        "export const x: number;\n",
    )
    .unwrap();
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
    let resolved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "inner/other",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved, None,
        "a bare './' exports entry should not expose arbitrary package subpaths"
    );

    let resolved_index = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "inner/index.js",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );
    assert_eq!(
        resolved_index,
        Some(canonicalize_or_owned(
            &dir.join("node_modules/inner/index.d.ts"),
        )),
        "a bare './' exports entry should still expose explicit file-like subpaths"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_root_types_js_is_ignored_for_module_resolution() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_package_types_js_ignored");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/foo")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/foo/package.json"),
        r#"{"name":"foo","types":"foo.js"}"#,
    )
    .unwrap();
    fs::write(dir.join("node_modules/foo/foo.js"), "module.exports = {};").unwrap();
    fs::write(dir.join("src/index.ts"), "import 'foo';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "foo",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved, None,
        "package.json types entries should not resolve runtime JS files"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_root_main_js_still_resolves_for_module_resolution() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_package_main_js_runtime");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/foo")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/foo/package.json"),
        r#"{"name":"foo","main":"foo.js"}"#,
    )
    .unwrap();
    fs::write(dir.join("node_modules/foo/foo.js"), "module.exports = {};").unwrap();
    fs::write(dir.join("src/index.ts"), "import 'foo';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        allow_js: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();
    let resolved = resolve_module_specifier(
        &dir.join("src/index.ts"),
        "foo",
        &options,
        &dir,
        &mut cache,
        &known_files,
    );

    assert_eq!(
        resolved,
        Some(canonicalize_or_owned(&dir.join("node_modules/foo/foo.js"))),
        "package.json main entries should still resolve runtime JS files"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_extensionless_json_import_does_not_resolve_with_resolve_json_module() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_driver_resolution_extensionless_json");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(dir.join("src/index.ts"), "import data = require('./data');").unwrap();
    fs::write(dir.join("src/data.json"), "{\"value\": 42}").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };

    let mut cache = ModuleResolutionCache::default();
    let known_files: FxHashSet<PathBuf> = FxHashSet::default();

    assert_eq!(
        resolve_module_specifier(
            &dir.join("src/index.ts"),
            "./data",
            &options,
            &dir,
            &mut cache,
            &known_files,
        ),
        None,
        "extensionless relative imports should not fall through to data.json"
    );

    assert_eq!(
        resolve_module_specifier(
            &dir.join("src/index.ts"),
            "./data.json",
            &options,
            &dir,
            &mut cache,
            &known_files,
        ),
        Some(canonicalize_or_owned(&dir.join("src/data.json"))),
        "explicit .json imports should still resolve when resolveJsonModule is enabled"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[path = "resolution_tests/source_discovery.rs"]
mod resolution_tests_source_discovery;

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
fn test_default_type_roots_walks_past_ancestor_tsconfig() {
    use std::fs;

    // Monorepo layout: the project's tsconfig lives at `apps/web/tsconfig.json`
    // while `@types/*` is hoisted to the workspace root `node_modules/@types`.
    // tsc's `getDefaultTypeRoots` has no tsconfig boundary, so the workspace
    // root must still be discovered even though an ancestor hosts a tsconfig.
    let dir = tempfile::TempDir::new().expect("temp dir creation should succeed in test");
    let repo_root = dir.path();
    let app_dir = repo_root.join("apps").join("web");
    let root_types = repo_root.join("node_modules").join("@types");

    fs::create_dir_all(&app_dir).unwrap();
    fs::create_dir_all(&root_types).unwrap();
    // tsconfig at every level between the app and the workspace root.
    fs::write(app_dir.join("tsconfig.json"), "{}").unwrap();
    fs::write(repo_root.join("apps").join("tsconfig.json"), "{}").unwrap();
    fs::write(repo_root.join("tsconfig.json"), "{}").unwrap();

    let roots = default_type_roots(&app_dir);
    let root_canonical = canonicalize_or_owned(&root_types);
    assert!(
        roots.contains(&root_canonical),
        "workspace-root @types must be discovered across ancestor tsconfig.json files, got: {roots:?}"
    );
}

#[test]
fn test_default_type_roots_collects_nested_and_hoisted() {
    use std::fs;

    // Both a nested app-local `@types` and a hoisted workspace-root `@types`
    // exist; tsc collects every ancestor root, nearest first.
    let dir = tempfile::TempDir::new().expect("temp dir creation should succeed in test");
    let repo_root = dir.path();
    let app_dir = repo_root.join("apps").join("web");
    let app_types = app_dir.join("node_modules").join("@types");
    let root_types = repo_root.join("node_modules").join("@types");

    fs::create_dir_all(&app_types).unwrap();
    fs::create_dir_all(&root_types).unwrap();
    fs::write(app_dir.join("tsconfig.json"), "{}").unwrap();

    let roots = default_type_roots(&app_dir);
    let app_canonical = canonicalize_or_owned(&app_types);
    let root_canonical = canonicalize_or_owned(&root_types);

    assert_eq!(
        roots.first(),
        Some(&app_canonical),
        "nearest @types root should come first, got: {roots:?}"
    );
    assert!(
        roots.contains(&root_canonical),
        "hoisted workspace-root @types should still be included, got: {roots:?}"
    );
}

#[path = "resolution_tests_jsdoc.rs"]
mod jsdoc_import_type_specifier_collection_tests;

#[path = "resolution_tests_path_mappings.rs"]
mod resolution_tests_path_mappings;

#[path = "resolution_tests_package_exports.rs"]
mod resolution_tests_package_exports;
