//! Canonical resolved-path identity for package entry points.
//!
//! A `package.json` `main`/`types`/`typings` value is almost always written
//! with a leading `./` (`"./dist/index.js"`). `package_dir.join(field)` keeps
//! that segment verbatim, and the `main`-field probes that return the path
//! without going through `try_file`/`try_types_entry`
//! (`resolve_explicit_unknown_extension`, `declaration_substitution_for_main`)
//! used to surface it as a stray `/./` in the resolved path
//! (`node_modules/pkg/./dist/index.d.ts`). Because module identity in tsz is
//! textual, that is a distinct spelling from the same file reached through a
//! relative import or an `exports` target (both of which are normalized), so it
//! risks minting duplicate module identities. These tests pin that the resolver
//! emits a segment-canonical path for every entry-point field, regardless of
//! how the field is spelled or the package is named.

use super::super::*;
use super::fixtures::TempFixture;

fn node16_opts() -> ResolvedCompilerOptions {
    ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    }
}

/// Resolve `specifier` and assert the resolved path is exactly `expected`
/// (textually canonical — no `.`/`..` segments).
#[track_caller]
fn assert_resolves_to(
    opts: &ResolvedCompilerOptions,
    specifier: &str,
    fixture: &TempFixture,
    expected_rel: &str,
) {
    let mut resolver = ModuleResolver::new(opts);
    let resolved = resolver
        .resolve(specifier, &fixture.join("main.ts"), Span::new(0, 10))
        .unwrap_or_else(|e| panic!("`{specifier}` should resolve, got {e:?}"));
    let expected = fixture.join(expected_rel);
    assert_eq!(
        resolved.resolved_path, expected,
        "`{specifier}` should resolve to a canonical path"
    );
    // Guard the actual invariant directly: no `.`/`..` segment survives.
    assert!(
        !resolved.resolved_path.components().any(|c| matches!(
            c,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )),
        "resolved path must be segment-canonical, got {}",
        resolved.resolved_path.display()
    );
    // And the textual spelling must not carry a stray `/./` (which `components`
    // hides but the file-graph identity does not).
    assert!(
        !resolved.resolved_path.to_string_lossy().contains("/./"),
        "resolved path must not contain a stray `/./`, got {}",
        resolved.resolved_path.display()
    );
}

#[test]
fn types_field_with_dot_slash_resolves_canonically() {
    let f = TempFixture::new();
    f.write(
        "node_modules/alpha/package.json",
        r#"{"name":"alpha","types":"./dist/index.d.ts","main":"./dist/index.js"}"#,
    );
    f.write(
        "node_modules/alpha/dist/index.d.ts",
        "export const a: number;",
    );
    f.write("node_modules/alpha/dist/index.js", "");
    f.write("main.ts", "");
    assert_resolves_to(
        &node16_opts(),
        "alpha",
        &f,
        "node_modules/alpha/dist/index.d.ts",
    );
}

#[test]
fn typings_legacy_field_with_dot_slash_resolves_canonically() {
    // Vary the binder: legacy `typings` field, different package + path names.
    let f = TempFixture::new();
    f.write(
        "node_modules/beta-pkg/package.json",
        r#"{"name":"beta-pkg","typings":"./out/types/entry.d.ts"}"#,
    );
    f.write(
        "node_modules/beta-pkg/out/types/entry.d.ts",
        "export const b: number;",
    );
    f.write("main.ts", "");
    assert_resolves_to(
        &node16_opts(),
        "beta-pkg",
        &f,
        "node_modules/beta-pkg/out/types/entry.d.ts",
    );
}

#[test]
fn main_field_declaration_sibling_with_dot_slash_resolves_canonically() {
    // `declaration_substitution_for_main`: `"./dist/index.js"` -> sibling
    // `index.d.ts`. This is the branch that bypassed `try_file` normalization.
    let f = TempFixture::new();
    f.write(
        "node_modules/gamma/package.json",
        r#"{"name":"gamma","main":"./lib/main.js"}"#,
    );
    f.write(
        "node_modules/gamma/lib/main.d.ts",
        "export const g: number;",
    );
    f.write("node_modules/gamma/lib/main.js", "");
    f.write("main.ts", "");
    assert_resolves_to(
        &node16_opts(),
        "gamma",
        &f,
        "node_modules/gamma/lib/main.d.ts",
    );
}

#[test]
fn main_field_with_interior_dot_segment_resolves_canonically() {
    // An explicit interior `.` segment (`./lib/./main.js`) must collapse too.
    let f = TempFixture::new();
    f.write(
        "node_modules/delta/package.json",
        r#"{"name":"delta","main":"./lib/./main.js"}"#,
    );
    f.write(
        "node_modules/delta/lib/main.d.ts",
        "export const d: number;",
    );
    f.write("node_modules/delta/lib/main.js", "");
    f.write("main.ts", "");
    assert_resolves_to(
        &node16_opts(),
        "delta",
        &f,
        "node_modules/delta/lib/main.d.ts",
    );
}

#[test]
fn main_field_runtime_js_with_dot_slash_resolves_canonically() {
    // `try_file` / `resolve_explicit_unknown_extension` runtime-JS branch under
    // allowJs: the resolved `.js` path must also be canonical.
    let f = TempFixture::new();
    let mut opts = node16_opts();
    opts.allow_js = true;
    f.write(
        "node_modules/epsilon/package.json",
        r#"{"name":"epsilon","main":"./bundle/entry.js"}"#,
    );
    f.write("node_modules/epsilon/bundle/entry.js", "");
    f.write("main.ts", "");
    assert_resolves_to(&opts, "epsilon", &f, "node_modules/epsilon/bundle/entry.js");
}
