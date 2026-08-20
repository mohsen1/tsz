//! JSON-`null` `exports`/`imports` target blocking.
//!
//! Node.js `PACKAGE_TARGET_RESOLVE` (which `tsc` reimplements) treats a `null`
//! target reached through a *matching* condition, array element, or exact
//! subpath key as a **block**: the whole `exports`/`imports` resolution stops
//! and the specifier is reported as not exported (`TS2307`). The block must NOT
//! fall through to a sibling condition, a later array element, the enclosing
//! conditional, or pattern matching.
//!
//! Each case below is verified byte-for-byte against bundled `tsc` 6.0.2
//! (`module: node16`). Package and file names are varied across cases so the
//! behavior is keyed on structure, not on any identifier.

use super::super::*;
use super::fixtures::TempFixture;

/// Resolve `specifier` against a single `node_modules/<pkg>` package whose
/// `package.json` is `package_json`, importing from a `.cts` file (CommonJS, so
/// the active conditions are `types`, `node`, `require`, `default`). `targets`
/// are written as empty `.d.ts` files inside the package.
fn resolve_cjs(
    pkg: &str,
    package_json: &str,
    targets: &[&str],
    specifier: &str,
) -> Result<ResolvedModule, ResolutionFailure> {
    let fixture = TempFixture::new();
    fixture.write(format!("node_modules/{pkg}/package.json"), package_json);
    for target in targets {
        fixture.write(
            format!("node_modules/{pkg}/{target}"),
            "export declare const value: number;",
        );
    }
    let importer = fixture.write("main.cts", "export {};");

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    resolver.resolve(specifier, &importer, Span::new(0, specifier.len() as u32))
}

#[test]
fn nested_matching_null_blocks_and_does_not_fall_through_to_sibling() {
    // `require` (matched) maps to null INSIDE the matched `node` branch. tsc:
    // node -> require -> null -> blocked; neither the inner `default` nor the
    // outer `default` is reached. This is the canary reduction that motivated
    // the fix — a *nested* null whose block previously leaked to the outer
    // `default`.
    let result = resolve_cjs(
        "alpha-lib",
        r#"{"name":"alpha-lib","exports":{".":{"node":{"require":null,"default":"./inner.js"},"default":"./outer.js"}}}"#,
        &["inner.d.ts", "outer.d.ts"],
        "alpha-lib",
    );
    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "nested matching null must block the whole resolution, got {result:?}"
    );
}

#[test]
fn top_level_matching_null_blocks_sibling_default() {
    // `node` (matched) maps directly to null; the sibling `default` must not be
    // tried. tsc: TS2307.
    let result = resolve_cjs(
        "bravo-kit",
        r#"{"name":"bravo-kit","exports":{".":{"node":null,"default":"./fallback.js"}}}"#,
        &["fallback.d.ts"],
        "bravo-kit",
    );
    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "top-level matching null must block the sibling default, got {result:?}"
    );
}

#[test]
fn null_on_non_matching_condition_does_not_block() {
    // `import` does NOT match a CommonJS importer, so its null is never reached
    // and `default` resolves normally. tsc: resolves `./present.d.ts`.
    let result = resolve_cjs(
        "charlie-pkg",
        r#"{"name":"charlie-pkg","exports":{".":{"import":null,"default":"./present.js"}}}"#,
        &["present.d.ts"],
        "charlie-pkg",
    )
    .expect("a null on an unmatched condition must not block the matched default");
    assert!(
        result.resolved_path.ends_with("present.d.ts"),
        "expected ./present.d.ts, got {}",
        result.resolved_path.display()
    );
}

#[test]
fn null_array_element_blocks_remaining_elements() {
    // A null FIRST element of a fallback array blocks the array — the later
    // `./real.js` is never tried. tsc: TS2307.
    let result = resolve_cjs(
        "delta-mod",
        r#"{"name":"delta-mod","exports":{".":[null,"./real.js"]}}"#,
        &["real.d.ts"],
        "delta-mod",
    );
    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "a null array element must block the remaining fallbacks, got {result:?}"
    );
}

#[test]
fn missing_string_target_in_array_falls_through_to_next() {
    // A *missing file* (not a null) is a miss, not a block: the array falls
    // through to the next element. tsc: resolves `./real.d.ts`.
    let result = resolve_cjs(
        "echo-tools",
        r#"{"name":"echo-tools","exports":{".":["./absent.js","./real.js"]}}"#,
        &["real.d.ts"],
        "echo-tools",
    )
    .expect("a missing string target must fall through to the next array element");
    assert!(
        result.resolved_path.ends_with("real.d.ts"),
        "expected ./real.d.ts, got {}",
        result.resolved_path.display()
    );
}

#[test]
fn exact_subpath_null_blocks_without_pattern_fallthrough() {
    // `"./blocked": null` blocks the `blocked` subpath even though a `"./*"`
    // wildcard would otherwise match it; the exact key is authoritative. A
    // different subpath still resolves through the wildcard, proving the block
    // is scoped to the exact key, not the whole map.
    let package_json =
        r#"{"name":"foxtrot-suite","exports":{"./blocked":null,"./*":"./impl/*.js"}}"#;

    let blocked = resolve_cjs(
        "foxtrot-suite",
        package_json,
        &["impl/blocked.d.ts", "impl/allowed.d.ts"],
        "foxtrot-suite/blocked",
    );
    assert!(
        matches!(blocked, Err(ResolutionFailure::NotFound { .. })),
        "an exact null subpath key must block without pattern fallthrough, got {blocked:?}"
    );

    let allowed = resolve_cjs(
        "foxtrot-suite",
        package_json,
        &["impl/blocked.d.ts", "impl/allowed.d.ts"],
        "foxtrot-suite/allowed",
    )
    .expect("a non-blocked subpath must still resolve through the wildcard");
    assert!(
        allowed.resolved_path.ends_with("impl/allowed.d.ts"),
        "expected impl/allowed.d.ts, got {}",
        allowed.resolved_path.display()
    );
}

/// Resolve a `#`-prefixed import against the importer's own package scope.
fn resolve_imports_cjs(
    package_json: &str,
    targets: &[&str],
    specifier: &str,
) -> Result<ResolvedModule, ResolutionFailure> {
    let fixture = TempFixture::new();
    fixture.write("package.json", package_json);
    for target in targets {
        fixture.write(target, "export declare const value: number;");
    }
    let importer = fixture.write("main.cts", "export {};");

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_imports: true,
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    resolver.resolve(specifier, &importer, Span::new(0, specifier.len() as u32))
}

#[test]
fn imports_nested_matching_null_blocks_outer_default() {
    // The `#imports` twin of `nested_matching_null_...`: a nested matching null
    // must block the outer `default`, not fall through to it. tsc: TS2307.
    let result = resolve_imports_cjs(
        r##"{"name":"golf-app","imports":{"#feature":{"node":{"require":null,"default":"./inner.js"},"default":"./outer.js"}}}"##,
        &["inner.d.ts", "outer.d.ts"],
        "#feature",
    );
    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "nested matching null must block the #imports resolution, got {result:?}"
    );
}

#[test]
fn imports_null_on_non_matching_condition_resolves_default() {
    // `import` does not match a CommonJS importer, so its null is unreached and
    // `default` resolves. tsc: resolves `./present.d.ts`.
    let result = resolve_imports_cjs(
        r##"{"name":"hotel-app","imports":{"#feature":{"import":null,"default":"./present.js"}}}"##,
        &["present.d.ts"],
        "#feature",
    )
    .expect("a null on an unmatched condition must not block the matched default");
    assert!(
        result.resolved_path.ends_with("present.d.ts"),
        "expected ./present.d.ts, got {}",
        result.resolved_path.display()
    );
}
