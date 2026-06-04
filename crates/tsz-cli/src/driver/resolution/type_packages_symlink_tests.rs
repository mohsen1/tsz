//! Regression coverage for triple-slash `@types/*` resolution through pnpm
//! (and other) symlink layouts.
//!
//! When `node_modules/@types/<pkg>` is a symlink into a pnpm `.pnpm` sandbox,
//! the sibling `@types/*` packages it references via
//! `/// <reference types="..." />` live next to the symlink *target*, not next
//! to the symlink. With `preserveSymlinks: false` (the default), `tsc`
//! resolves them by walking the real (`realpath`) location of the importing
//! `.d.ts`. These tests pin that behavior.
//!
//! Package names are intentionally synthetic (`foo`, `foo-core`, `foo-bar`)
//! rather than the real `express` triple to prove the fix is structural and
//! not keyed on any particular package name.

use super::*;
use crate::config::ModuleResolutionKind;
use std::fs;
use std::os::unix::fs::symlink;

fn node16_options() -> ResolvedCompilerOptions {
    ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        preserve_symlinks: false,
        module_suffixes: vec![String::new()],
        ..Default::default()
    }
}

/// Create an `@types/<pkg>` package (a `package.json` pointing at
/// `index.d.ts`, plus the `index.d.ts` body) under `at_types`.
fn write_types_package(at_types: &Path, pkg: &str, index_dts: &str) {
    let dir = at_types.join(pkg);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("package.json"),
        r#"{"name":"@types/PKG","version":"1.0.0","types":"index.d.ts"}"#.replace("PKG", pkg),
    )
    .unwrap();
    fs::write(dir.join("index.d.ts"), index_dts).unwrap();
}

/// Build a pnpm-style layout where `@types/foo` is hoisted to the top-level
/// `node_modules/@types` via a symlink into the `.pnpm` sandbox, and the
/// sibling `@types/foo-core` / `@types/foo-bar` packages it references only
/// exist inside that sandbox.
///
/// Returns `(canonical_root, symlinked_foo_index_dts)`.
fn build_pnpm_layout(root: &Path) -> (PathBuf, PathBuf) {
    let pnpm_pkg = root.join("node_modules/.pnpm/@types+foo@1.0.0/node_modules/@types");

    // foo references its two siblings via triple-slash type references; the
    // siblings only exist inside this .pnpm sandbox.
    write_types_package(
        &pnpm_pkg,
        "foo",
        "/// <reference types=\"foo-core\" />\n\
         /// <reference types=\"foo-bar\" />\n\
         export {};\n",
    );
    write_types_package(&pnpm_pkg, "foo-core", "export interface Core {}\n");
    write_types_package(&pnpm_pkg, "foo-bar", "export interface Bar {}\n");

    // Only `@types/foo` is hoisted to the top-level node_modules, as a symlink
    // pointing into the .pnpm sandbox (the transitive siblings are NOT hoisted).
    let top_at_types = root.join("node_modules/@types");
    fs::create_dir_all(&top_at_types).unwrap();
    symlink(pnpm_pkg.join("foo"), top_at_types.join("foo")).unwrap();

    let canonical_root = canonicalize_or_owned(root);
    let symlinked_foo_index = root.join("node_modules/@types/foo/index.d.ts");
    (canonical_root, symlinked_foo_index)
}

#[test]
fn triple_slash_at_types_sibling_resolved_through_pnpm_symlink() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let (base_dir, symlinked_foo_index) = build_pnpm_layout(dir.path());

    let options = node16_options();

    for sibling in ["foo-core", "foo-bar"] {
        let mut cache = ModuleResolutionCache::default();
        let resolved = resolve_type_reference_from_node_modules_with_cache(
            sibling,
            &symlinked_foo_index,
            &base_dir,
            None,
            &options,
            &mut cache,
        );

        let expected = canonicalize_or_owned(
            &base_dir
                .join("node_modules/.pnpm/@types+foo@1.0.0/node_modules/@types")
                .join(sibling)
                .join("index.d.ts"),
        );

        assert_eq!(
            resolved,
            Some(expected),
            "sibling `{sibling}` must resolve through the pnpm symlink realpath"
        );
    }
}

#[test]
fn triple_slash_at_types_sibling_not_resolved_with_preserve_symlinks() {
    // With `preserveSymlinks: true`, `tsc` keeps the symlink-relative identity
    // and does NOT reach into the `.pnpm` sandbox, so the sibling is unresolved
    // (it would report TS2688). Parity requires the same here.
    let dir = tempfile::TempDir::new().expect("temp dir");
    let (base_dir, symlinked_foo_index) = build_pnpm_layout(dir.path());

    let options = ResolvedCompilerOptions {
        preserve_symlinks: true,
        ..node16_options()
    };

    let mut cache = ModuleResolutionCache::default();
    let resolved = resolve_type_reference_from_node_modules_with_cache(
        "foo-core",
        &symlinked_foo_index,
        &base_dir,
        None,
        &options,
        &mut cache,
    );

    assert_eq!(
        resolved, None,
        "preserveSymlinks must not walk into the symlink target's sandbox"
    );
}

#[test]
fn top_level_at_types_sibling_still_resolves_without_symlink_walk() {
    // A non-symlinked layout (sibling hoisted next to the importer) must keep
    // resolving via the literal walk — the realpath fallback must not regress
    // the common case.
    let dir = tempfile::TempDir::new().expect("temp dir");
    let root = dir.path();
    let at_types = root.join("node_modules/@types");

    write_types_package(
        &at_types,
        "foo",
        "/// <reference types=\"foo-core\" />\nexport {};\n",
    );
    write_types_package(&at_types, "foo-core", "export interface Core {}\n");

    let base_dir = canonicalize_or_owned(root);
    let from_file = root.join("node_modules/@types/foo/index.d.ts");

    let options = node16_options();
    let mut cache = ModuleResolutionCache::default();
    let resolved = resolve_type_reference_from_node_modules_with_cache(
        "foo-core", &from_file, &base_dir, None, &options, &mut cache,
    );

    let expected = canonicalize_or_owned(&base_dir.join("node_modules/@types/foo-core/index.d.ts"));
    assert_eq!(resolved, Some(expected));
}
