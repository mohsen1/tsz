//! Coverage for `.json` specifiers resolving through their arbitrary-extension
//! companion declaration file (`<base>.json` -> `<base>.d.json.ts`).
//!
//! Structural rule (verified against pinned `typescript@7.0.2`): when
//! `allowArbitraryExtensions` is set, the companion declaration takes
//! priority over the literal `.json` file regardless of `resolveJsonModule`,
//! so `resolveJsonModule: false` must NOT upgrade to TS2732
//! (`JsonModuleWithoutResolveJsonModule`) when the companion declaration
//! exists. Fixes the `declarationFileForJsonImport.ts` conformance false
//! positive (extra TS2322 from typing the import against the literal JSON
//! content instead of the declared type).

use super::super::*;

#[test]
fn test_json_arbitrary_ext_decl_wins_over_resolve_json_module_gate() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_json_arbitrary_ext_decl");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import data from './data.json';").unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(
        dir.join("data.d.json.ts"),
        "declare var val: string;\nexport default val;\n",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: false,
        allow_arbitrary_extensions: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./data.json",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(18, 31),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(
        outcome.is_resolved,
        "Expected the companion .d.json.ts declaration to resolve, got error: {:?}",
        outcome.error
    );
    let resolved_path = outcome.resolved_path.expect("Expected a resolved path");
    assert!(
        resolved_path.ends_with("data.d.json.ts"),
        "Expected resolution to the companion declaration file, got {}",
        resolved_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_json_arbitrary_ext_decl_wins_with_resolve_json_module_enabled() {
    // Same companion-declaration preference, renamed binder, with
    // resolveJsonModule ENABLED — the companion still wins (verified against
    // pinned typescript@7.0.2: both resolveJsonModule true and false prefer
    // the declaration when allowArbitraryExtensions is set and it exists).
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_json_arbitrary_ext_decl_rjm_true");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import config from './config.json';").unwrap();
    fs::write(dir.join("config.json"), "{}").unwrap();
    fs::write(
        dir.join("config.d.json.ts"),
        "declare var setting: number;\nexport default setting;\n",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: true,
        allow_arbitrary_extensions: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./config.json",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(20, 35),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(
        outcome.is_resolved,
        "Expected the companion .d.json.ts declaration to resolve, got error: {:?}",
        outcome.error
    );
    let resolved_path = outcome.resolved_path.expect("Expected a resolved path");
    assert!(
        resolved_path.ends_with("config.d.json.ts"),
        "Expected resolution to the companion declaration file, got {}",
        resolved_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_json_literal_wins_when_no_arbitrary_ext_decl_exists() {
    // Negative control: without a companion `.d.json.ts` file, the literal
    // `.json` file resolves as before (no over-suppression from the new
    // priority check) even with allowArbitraryExtensions set.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_json_no_arbitrary_ext_decl");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import data from './plain.json';").unwrap();
    fs::write(dir.join("plain.json"), "{}").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: true,
        allow_arbitrary_extensions: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./plain.json",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(18, 32),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(outcome.is_resolved, "Expected the literal .json to resolve");
    let resolved_path = outcome.resolved_path.expect("Expected a resolved path");
    assert!(
        resolved_path.ends_with("plain.json"),
        "Expected resolution to the literal JSON file, got {}",
        resolved_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_json_arbitrary_ext_decl_ignored_without_allow_arbitrary_extensions() {
    // Negative control: the companion-declaration priority is gated on
    // `allowArbitraryExtensions`. Without the flag, a `.json` file that
    // exists on disk still resolves to the literal file even when a
    // `.d.json.ts` companion is present (unaffected by this change).
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_json_arbitrary_ext_decl_flag_off");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import data from './data.json';").unwrap();
    fs::write(dir.join("data.json"), "{}").unwrap();
    fs::write(
        dir.join("data.d.json.ts"),
        "declare var val: string;\nexport default val;\n",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: true,
        allow_arbitrary_extensions: false,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./data.json",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(18, 31),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(outcome.is_resolved, "Expected the literal .json to resolve");
    let resolved_path = outcome.resolved_path.expect("Expected a resolved path");
    assert!(
        resolved_path.ends_with("data.json") && !resolved_path.ends_with("d.json.ts"),
        "Expected resolution to the literal JSON file (flag off), got {}",
        resolved_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}
