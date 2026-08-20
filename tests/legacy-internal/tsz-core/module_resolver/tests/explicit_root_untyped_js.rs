//! `require()` of an untyped `node_modules` JS module, when that JS file is
//! itself an explicit program root, is not `isExternalLibraryImport` in tsc's
//! model — the file already has a `SourceFile` from being processed as a
//! root, so no TS7016 fires. Oracle-verified (`typescript@7.0.2`) against the
//! real conformance-corpus shapes `compiler/importNonExportedMember12.ts` and
//! `conformance/salsa/namespaceAssignmentToRequireAlias.ts`: both are `.js`
//! fixtures under `node_modules/` that the TypeScript test harness (and
//! tsz's own conformance harness — see `needs_explicit_root_files` in
//! `crates/conformance/src/tsz_wrapper.rs`) turns into explicit `files`
//! roots, and both are clean of TS7016 only because of that, independent of
//! `maxNodeModuleJsDepth` (unset in both).

use super::super::*;
use super::fixtures::TempFixture;
use rustc_hash::FxHashSet;

fn setup_untyped_package(fixture: &TempFixture) -> std::path::PathBuf {
    fixture.write(
        "node_modules/untyped/package.json",
        r#"{"name":"untyped","version":"1.0.0","main":"index.js"}"#,
    );
    fixture.write("node_modules/untyped/index.js", "module.exports = 1;")
}

/// `ModuleResolver::node_resolver()` hardcodes `allow_js: false`, which
/// routes every `.js`-only resolution through the `probe_js_file` fallback
/// (a different code path from the one under test here). Real projects that
/// hit this false positive set `allowJs: true` (both named conformance
/// fixtures do), so tests need a resolver that actually takes the primary
/// `Ok(resolved_module)` branch for a `.js` file.
fn allow_js_node_resolver() -> ModuleResolver {
    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        allow_js: true,
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    ModuleResolver::new(&options)
}

fn require_request<'a>(
    specifier: &'a str,
    containing_file: &'a std::path::Path,
) -> ModuleLookupRequest<'a> {
    ModuleLookupRequest {
        specifier,
        containing_file,
        specifier_span: Span::new(0, 0),
        import_kind: ImportKind::CjsRequire,
        resolution_mode_override: None,
        no_implicit_any: true,
        implied_classic_resolution: false,
    }
}

#[test]
fn explicit_root_untyped_js_suppresses_ts7016() {
    let fixture = TempFixture::new();
    let resolved_js = setup_untyped_package(&fixture);
    let containing_file = fixture.write("a.ts", "import u = require(\"untyped\");");

    let mut resolver = allow_js_node_resolver();
    let request = require_request("untyped", &containing_file);

    let mut known_files = FxHashSet::default();
    known_files.insert(resolved_js.clone());

    let result = resolver.lookup(&request, |_, _| None, |_| false, Some(&known_files));

    assert_eq!(
        result.resolved_path.as_deref(),
        Some(resolved_js.as_path()),
        "should still resolve to the real JS file"
    );
    assert!(
        result.error.is_none(),
        "an explicit-root untyped JS module must not report TS7016: {:?}",
        result.error
    );
}

#[test]
fn non_root_untyped_js_still_reports_ts7016() {
    let fixture = TempFixture::new();
    let resolved_js = setup_untyped_package(&fixture);
    let containing_file = fixture.write("a.ts", "import u = require(\"untyped\");");

    let mut resolver = allow_js_node_resolver();
    let request = require_request("untyped", &containing_file);

    // No `known_files` at all: the genuine "no root, no depth allowance"
    // default shape most real projects hit.
    let result_no_known_files = resolver.lookup(&request, |_, _| None, |_| false, None);
    let error = result_no_known_files
        .error
        .expect("a genuinely external, non-rooted require() of untyped JS must report TS7016");
    assert_eq!(error.code, COULD_NOT_FIND_DECLARATION_FILE);

    resolver.clear_cache();

    // `known_files` present but not containing this resolution: must not
    // accidentally treat every program file as root-explicit.
    let mut unrelated_known_files = FxHashSet::default();
    unrelated_known_files.insert(containing_file.clone());
    let result_unrelated = resolver.lookup(
        &request,
        |_, _| None,
        |_| false,
        Some(&unrelated_known_files),
    );
    let error = result_unrelated
        .error
        .expect("an unrelated known_files set must not suppress TS7016 for this resolution");
    assert_eq!(error.code, COULD_NOT_FIND_DECLARATION_FILE);
    assert!(
        error.message.contains(&resolved_js.display().to_string()),
        "error should still reference the resolved JS path: {}",
        error.message
    );
}

#[test]
fn explicit_root_esm_import_of_untyped_js_is_unaffected() {
    // The explicit-root bypass is scoped to the external-CJS-require branch
    // (`is_external_cjs_require`); a plain ESM `import` of the same untyped
    // JS file is a different tsc rule (issue #3050, gated on `allowJs`
    // alone) and must keep its existing behavior regardless of root status.
    let fixture = TempFixture::new();
    let resolved_js = setup_untyped_package(&fixture);
    let containing_file = fixture.write("a.ts", "import u from \"untyped\";");

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        allow_js: false,
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let request = ModuleLookupRequest {
        specifier: "untyped",
        containing_file: &containing_file,
        specifier_span: Span::new(0, 0),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: true,
        implied_classic_resolution: false,
    };

    let mut known_files = FxHashSet::default();
    known_files.insert(resolved_js);

    let result = resolver.lookup(&request, |_, _| None, |_| false, Some(&known_files));
    assert!(
        result.error.is_some(),
        "ESM import of untyped JS without allowJs still reports TS7016 even when rooted"
    );
}
