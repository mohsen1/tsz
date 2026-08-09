//! TS6263 for a `.json` specifier resolved through its `.d.json.ts`
//! declaration companion without `--allowArbitraryExtensions` (#17020).
//!
//! Structural rule: `#17019` made a `.json` specifier's sibling
//! `<base>.d.json.ts` declaration companion take resolution priority over the
//! literal `.json` file, unconditionally — matching `tsc`'s
//! `tryAddingExtensions` `Extension.Json` case. But `tsc` still gates that
//! resolution behind `--allowArbitraryExtensions`: when the flag is unset,
//! `getIsDeclarationFileName`'s arbitrary-extension diagnostic (TS6263) fires
//! at the import specifier, identically for both `resolveJsonModule` settings
//! (verified against the pinned `typescript@7.0.2` oracle). tsz's
//! `is_arbitrary_extension_declaration` (`mod.rs`) previously excluded
//! `.json` from the arbitrary-extension check outright, so the resolution
//! happened silently instead.

use super::super::*;
use super::fixtures::TempFixture;

fn json_companion_fixture() -> (TempFixture, std::path::PathBuf) {
    let fixture = TempFixture::new();
    let dir = fixture.path().to_path_buf();
    fixture.write("app.ts", "import data from './data.json';");
    fixture.write("data.json", "{}");
    fixture.write(
        "data.d.json.ts",
        "declare var val: string; export default val;",
    );
    (fixture, dir)
}

fn json_lookup_request<'a>(containing_file: &'a std::path::Path) -> ModuleLookupRequest<'a> {
    ModuleLookupRequest {
        specifier: "./data.json",
        containing_file,
        specifier_span: Span::new(19, 32),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    }
}

/// `resolveJsonModule: true`, no `allowArbitraryExtensions`: `tsc` still
/// resolves through the `.d.json.ts` companion and reports TS6263.
#[test]
fn ts6263_fires_for_json_companion_without_allow_arbitrary_extensions_resolve_json_true() {
    let (_fixture, dir) = json_companion_fixture();
    let containing = dir.join("app.ts");
    let mut resolver = ModuleResolver::new(&ResolvedCompilerOptions {
        resolve_json_module: true,
        allow_arbitrary_extensions: false,
        ..Default::default()
    });
    let request = json_lookup_request(&containing);
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    assert_eq!(
        result.resolved_path,
        Some(dir.join("data.d.json.ts")),
        "resolved path should still be the declaration companion"
    );
    let error = result
        .error
        .expect("expected TS6263 when allowArbitraryExtensions is unset");
    assert_eq!(
        error.code, MODULE_WAS_RESOLVED_TO_BUT_ALLOW_ARBITRARY_EXTENSIONS_IS_NOT_SET,
        "expected TS6263, got {error:?}"
    );
}

/// Same fixture with `resolveJsonModule: false` — `tsc` reports TS6263
/// identically regardless of `resolveJsonModule`, since the `.d.json.ts`
/// companion resolution does not consult that flag at all.
#[test]
fn ts6263_fires_for_json_companion_without_allow_arbitrary_extensions_resolve_json_false() {
    let (_fixture, dir) = json_companion_fixture();
    let containing = dir.join("app.ts");
    let mut resolver = ModuleResolver::new(&ResolvedCompilerOptions {
        resolve_json_module: false,
        allow_arbitrary_extensions: false,
        ..Default::default()
    });
    let request = json_lookup_request(&containing);
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    let error = result
        .error
        .expect("expected TS6263 when allowArbitraryExtensions is unset");
    assert_eq!(
        error.code,
        MODULE_WAS_RESOLVED_TO_BUT_ALLOW_ARBITRARY_EXTENSIONS_IS_NOT_SET
    );
}

/// Control from #17020: with `allowArbitraryExtensions: true`, resolution is
/// clean — no TS6263.
#[test]
fn no_ts6263_for_json_companion_with_allow_arbitrary_extensions() {
    let (_fixture, dir) = json_companion_fixture();
    let containing = dir.join("app.ts");
    let mut resolver = ModuleResolver::new(&ResolvedCompilerOptions {
        resolve_json_module: true,
        allow_arbitrary_extensions: true,
        ..Default::default()
    });
    let request = json_lookup_request(&containing);
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    assert_eq!(result.resolved_path, Some(dir.join("data.d.json.ts")));
    assert!(
        result.error.is_none(),
        "expected clean resolution with allowArbitraryExtensions set, got {:?}",
        result.error
    );
}

/// Control from #17020: without a `.d.json.ts` companion at all, both
/// compilers agree exactly regardless of `allowArbitraryExtensions` — the
/// literal `.json` file resolves and TS6263 must not fire.
#[test]
fn no_ts6263_for_plain_json_without_companion() {
    let fixture = TempFixture::new();
    let dir = fixture.path().to_path_buf();
    fixture.write("app.ts", "import data from './data.json';");
    fixture.write("data.json", "{}");
    let containing = dir.join("app.ts");

    let mut resolver = ModuleResolver::new(&ResolvedCompilerOptions {
        resolve_json_module: true,
        allow_arbitrary_extensions: false,
        ..Default::default()
    });
    let request = json_lookup_request(&containing);
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    assert_eq!(result.resolved_path, Some(dir.join("data.json")));
    assert!(
        result.error.is_none(),
        "plain .json resolution (no companion) must not report TS6263, got {:?}",
        result.error
    );
}

/// Renamed-binder variant: the gate is keyed on the `.d.json.ts` shape, not
/// on `data`/`val` specifically.
#[test]
fn ts6263_fires_for_renamed_json_companion() {
    let fixture = TempFixture::new();
    let dir = fixture.path().to_path_buf();
    fixture.write("app.ts", "import cfg from './settings.json';");
    fixture.write("settings.json", "{}");
    fixture.write(
        "settings.d.json.ts",
        "declare var payload: number; export default payload;",
    );
    let containing = dir.join("app.ts");

    let mut resolver = ModuleResolver::new(&ResolvedCompilerOptions {
        resolve_json_module: true,
        allow_arbitrary_extensions: false,
        ..Default::default()
    });
    let request = ModuleLookupRequest {
        specifier: "./settings.json",
        containing_file: &containing,
        specifier_span: Span::new(18, 34),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    let error = result.error.expect("expected TS6263 for renamed companion");
    assert_eq!(
        error.code,
        MODULE_WAS_RESOLVED_TO_BUT_ALLOW_ARBITRARY_EXTENSIONS_IS_NOT_SET
    );
}
