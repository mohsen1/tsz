//! Coverage for `.json` specifiers resolving through their `.d.json.ts`
//! declaration companion.
//!
//! Structural rule, verified directly against the pinned `typescript@7.0.2`
//! oracle (not just the `declarationFileForJsonImport.ts` conformance
//! fixture, which only exercises the `allowArbitraryExtensions: true` case):
//! tsc's `tryAddingExtensions` `Extension.Json` case tries the Declaration
//! extension (`<base>.d.json.ts`) before the Json extension (`<base>.json`)
//! **unconditionally** — independent of both `resolveJsonModule` and
//! `allowArbitraryExtensions`. The two flags change only the DIAGNOSTIC, not
//! whether resolution lands on the declaration:
//!
//! | `allowArbitraryExtensions` | `resolveJsonModule` | resolves to | diagnostic |
//! | --- | --- | --- | --- |
//! | true | true or false | `.d.json.ts` | none |
//! | false | true or false | `.d.json.ts` | TS6263 (path preserved) |
//!
//! Confirmed with the oracle directly: `resolveJsonModule` has zero effect on
//! either outcome once a `.d.json.ts` sibling exists — it only matters for a
//! literal `.json` file with no companion (covered elsewhere by
//! `test_json_import_without_resolve_json_module`).

use super::super::*;
use super::fixtures::TempFixture;

fn lookup_json(
    dir: &std::path::Path,
    specifier: &str,
    resolve_json_module: bool,
    allow_arbitrary_extensions: bool,
) -> ModuleLookupResult {
    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module,
        allow_arbitrary_extensions,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let request = ModuleLookupRequest {
        specifier,
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(0, specifier.len() as u32),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    resolver.lookup(&request, |_, _| None, |_| false, None)
}

#[test]
fn decl_companion_wins_silently_when_flag_is_on_resolve_json_module_true() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("index.ts", "import data from './data.json';");
    fixture.write("data.json", "{}");
    fixture.write(
        "data.d.json.ts",
        "declare var val: string;\nexport default val;\n",
    );

    let outcome = lookup_json(dir, "./data.json", true, true).classify();
    assert!(outcome.is_resolved);
    assert!(
        outcome.error.is_none(),
        "expected no diagnostic: {:?}",
        outcome.error
    );
    let resolved = outcome.resolved_path.expect("resolved path");
    assert!(resolved.ends_with("data.d.json.ts"));
}

#[test]
fn decl_companion_wins_silently_when_flag_is_on_resolve_json_module_false() {
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("index.ts", "import data from './data.json';");
    fixture.write("data.json", "{}");
    fixture.write(
        "data.d.json.ts",
        "declare var val: string;\nexport default val;\n",
    );

    let outcome = lookup_json(dir, "./data.json", false, true).classify();
    assert!(outcome.is_resolved);
    assert!(
        outcome.error.is_none(),
        "declaration companion resolves silently regardless of resolveJsonModule: {:?}",
        outcome.error
    );
    let resolved = outcome.resolved_path.expect("resolved path");
    assert!(resolved.ends_with("data.d.json.ts"));
}

#[test]
fn decl_companion_still_wins_when_flag_is_off_but_reports_ts6263() {
    // The adjacent case both #17018 and #17019 missed: without
    // `allowArbitraryExtensions`, tsc does NOT fall back to the literal
    // `.json` file — it still resolves to `.d.json.ts` and reports TS6263
    // ("was resolved to '...', but '--allowArbitraryExtensions' is not
    // set"), with the resolved path preserved (verified against pinned
    // typescript@7.0.2: identical for both resolveJsonModule settings).
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("index.ts", "import data from './data.json';");
    fixture.write("data.json", "{}");
    fixture.write(
        "data.d.json.ts",
        "declare var val: string;\nexport default val;\n",
    );

    for resolve_json_module in [true, false] {
        let outcome = lookup_json(dir, "./data.json", resolve_json_module, false).classify();
        let resolved = outcome.resolved_path.as_ref().unwrap_or_else(|| {
            panic!("resolved path preserved on TS6263 (resolveJsonModule={resolve_json_module})")
        });
        assert!(
            resolved.ends_with("data.d.json.ts"),
            "resolveJsonModule={resolve_json_module}: expected resolution to the declaration \
             companion even with the flag off, got {}",
            resolved.display()
        );
        let error = outcome
            .error
            .as_ref()
            .unwrap_or_else(|| panic!("expected TS6263 (resolveJsonModule={resolve_json_module})"));
        assert_eq!(
            error.code, 6263,
            "resolveJsonModule={resolve_json_module}: expected TS6263, got {error:?}"
        );
    }
}

#[test]
fn decl_companion_renamed_specifier_and_binder() {
    // §25 structural-over-identifier gate: same shape, different names.
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("index.ts", "import cfg from './settings.json';");
    fixture.write("settings.json", "{}");
    fixture.write(
        "settings.d.json.ts",
        "declare var payload: number;\nexport default payload;\n",
    );

    let outcome = lookup_json(dir, "./settings.json", true, true).classify();
    assert!(outcome.is_resolved);
    assert!(outcome.error.is_none());
    let resolved = outcome.resolved_path.expect("resolved path");
    assert!(resolved.ends_with("settings.d.json.ts"));
}

#[test]
fn json_literal_wins_when_no_decl_companion_exists() {
    // Negative control: no `.d.json.ts` sibling — the literal `.json` file
    // resolves as before, regardless of `allowArbitraryExtensions`.
    let fixture = TempFixture::new();
    let dir = fixture.path();
    fixture.write("index.ts", "import data from './plain.json';");
    fixture.write("plain.json", "{}");

    for allow_arbitrary_extensions in [true, false] {
        let outcome = lookup_json(dir, "./plain.json", true, allow_arbitrary_extensions).classify();
        assert!(outcome.is_resolved);
        assert!(
            outcome.error.is_none(),
            "unexpected error: {:?}",
            outcome.error
        );
        let resolved = outcome.resolved_path.expect("resolved path");
        assert!(
            resolved.ends_with("plain.json") && !resolved.to_string_lossy().contains("d.json.ts"),
            "expected the literal JSON file, got {}",
            resolved.display()
        );
    }
}
