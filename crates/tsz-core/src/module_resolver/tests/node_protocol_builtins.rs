//! `node:` protocol builtin resolution for `module_resolver`.
//!
//! `tsc` resolves a `node:`-scheme specifier (e.g. `node:fs`,
//! `node:stream/promises`) by stripping the scheme and resolving the remainder
//! against the Node typings (`@types/node`), which declares each builtin as an
//! ambient module / package subpath. tsz previously treated `node:foo` as an
//! ordinary bare specifier, walked `node_modules` for a package literally named
//! `node:foo`, and emitted a false TS2307. These tests pin the strip-and-resolve
//! behavior (issue #13826, sub-root #1) without over-resolving non-builtins.

use super::super::*;

fn node_resolver() -> ModuleResolver {
    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    ModuleResolver::new(&options)
}

fn request_for<'a>(
    specifier: &'a str,
    containing_file: &'a std::path::Path,
) -> ModuleLookupRequest<'a> {
    ModuleLookupRequest {
        specifier,
        containing_file,
        specifier_span: Span::new(0, specifier.len() as u32),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    }
}

/// `node:stream/promises` resolves when the Node typings declare the
/// scheme-less ambient module `stream/promises` (the common `@types/node`
/// shape). The scheme is stripped before the ambient lookup.
#[test]
fn node_scheme_resolves_via_scheme_less_ambient() {
    let mut resolver = node_resolver();
    let containing = std::path::Path::new("/proj/src/index.ts");
    let request = request_for("node:stream/promises", containing);

    let result = resolver.lookup(
        &request,
        |_, _| None,
        |spec| spec == "stream/promises",
        None,
    );

    assert!(
        result.treat_as_resolved,
        "`node:stream/promises` must resolve through the scheme-less ambient module"
    );
    assert!(result.error.is_none(), "resolution should not emit TS2307");
}

/// A `node:`-prefixed ambient (the shape newer `@types/node` also ships,
/// `declare module "node:fs"`) still resolves — the raw specifier is tried
/// before the stripped form.
#[test]
fn node_scheme_resolves_via_scheme_prefixed_ambient() {
    let mut resolver = node_resolver();
    let containing = std::path::Path::new("/proj/src/index.ts");
    let request = request_for("node:fs", containing);

    let result = resolver.lookup(&request, |_, _| None, |spec| spec == "node:fs", None);

    assert!(
        result.treat_as_resolved,
        "`node:fs` must resolve through a `node:`-prefixed ambient module"
    );
    assert!(result.error.is_none(), "resolution should not emit TS2307");
}

/// The bare builtin name `assert/strict` and its `node:` form resolve the same
/// way — the fix is name-agnostic across builtins and subpaths.
#[test]
fn node_scheme_is_name_agnostic_across_builtins() {
    for (specifier, ambient) in [
        ("node:assert/strict", "assert/strict"),
        ("node:test", "test"),
        ("node:worker_threads", "worker_threads"),
    ] {
        let mut resolver = node_resolver();
        let containing = std::path::Path::new("/proj/src/index.ts");
        let request = request_for(specifier, containing);
        let result = resolver.lookup(&request, |_, _| None, |spec| spec == ambient, None);
        assert!(
            result.treat_as_resolved,
            "`{specifier}` should resolve via the scheme-less ambient `{ambient}`"
        );
        assert!(
            result.error.is_none(),
            "`{specifier}` should not emit TS2307"
        );
    }
}

/// When the scheme-less name is an installed package on disk (e.g. a builtin
/// polyfill or the `@types/node`-provided typings that ship a resolvable
/// package), the `node:` specifier resolves to that file via the strip-retry,
/// without any ambient declaration.
#[test]
fn node_scheme_resolves_via_installed_package_on_disk() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_node_proto_installed_pkg");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/events")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("node_modules/events/package.json"),
        r#"{"name":"events","version":"1.0.0","types":"index.d.ts"}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/events/index.d.ts"),
        "export class EventEmitter {}\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import { EventEmitter } from 'node:events';",
    )
    .unwrap();

    let mut resolver = node_resolver();
    let containing = dir.join("src/index.ts");
    let request = request_for("node:events", &containing);
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    assert!(
        result
            .resolved_path
            .as_ref()
            .is_some_and(|p| p.ends_with("index.d.ts")),
        "`node:events` should resolve to the installed `events` package; got {:?}",
        result.resolved_path
    );
    assert!(result.error.is_none(), "resolution should not emit TS2307");

    let _ = fs::remove_dir_all(&dir);
}

/// A `node:` specifier whose scheme-less form is not a known module still
/// fails — the fix only adds the strip-retry, it does not invent modules. The
/// emitted TS2307 keeps the original `node:`-scheme spelling.
#[test]
fn unknown_node_scheme_specifier_still_reports_ts2307() {
    let mut resolver = node_resolver();
    let containing = std::path::Path::new("/proj/src/index.ts");
    let request = request_for("node:totally-not-a-builtin", containing);

    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    assert!(
        !result.treat_as_resolved,
        "an unknown `node:` specifier must not be treated as resolved"
    );
    let error = result.error.expect("should emit a resolution error");
    assert_eq!(error.code, CANNOT_FIND_MODULE);
    assert!(
        error.message.contains("node:totally-not-a-builtin"),
        "the diagnostic should keep the original `node:` spelling; got: {}",
        error.message
    );
}
