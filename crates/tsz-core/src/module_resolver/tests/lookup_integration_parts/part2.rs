#[test]
fn test_lookup_skips_fallback_for_nodenext_literal_star_specifier_not_found() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_skip_fallback_literal_star");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("node_modules/double-asterisk")).unwrap();

    fs::write(
        dir.join("node_modules/double-asterisk/package.json"),
        r#"{
            "name":"double-asterisk",
            "exports":{"./a/*/b/*/c/*":"./example.js"}
        }"#,
    )
    .unwrap();
    let fallback_target = dir.join("node_modules/double-asterisk/example.d.ts");
    fs::write(&fallback_target, "export {};").unwrap();
    fs::write(
        dir.join("src/index.mts"),
        "import {} from 'double-asterisk/a/*/b/*/c/*';\n",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "double-asterisk/a/*/b/*/c/*",
        containing_file: &dir.join("src/index.mts"),
        specifier_span: Span::new(16, 44),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(
        &request,
        |_, _| Some(fallback_target.clone()),
        |_| false,
        None,
    );
    let outcome = result.classify();

    assert!(
        !outcome.is_resolved,
        "Fallback must be skipped for literal '*' package specifier in NodeNext"
    );
    let error = outcome
        .error
        .expect("Expected TS2307 after skipping fallback for literal '*'");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "Expected TS2307 for literal '*' package specifier, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_should_try_fallback_not_for_hard_failures() {
    // Hard failures like ImportPathNeedsExtension should NOT trigger fallback.
    // Verify the should_try_fallback contract on failure variants.
    let hard_failures = vec![
        ResolutionFailure::ImportPathNeedsExtension {
            specifier: "./utils".to_string(),
            suggested_extension: ".js".to_string(),
            containing_file: "/app/index.mts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::ImportingTsExtensionNotAllowed {
            extension: ".ts".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::JsxNotEnabled {
            specifier: "./comp".to_string(),
            resolved_path: PathBuf::from("/app/comp.tsx"),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::CircularResolution {
            message: "circular".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::InvalidSpecifier {
            message: "bad".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
    ];

    for failure in &hard_failures {
        assert!(
            !failure.should_try_fallback(),
            "Expected should_try_fallback=false for {:?}",
            std::mem::discriminant(failure)
        );
    }

    // Soft failures SHOULD trigger fallback
    let soft_failures = vec![
        ResolutionFailure::NotFound {
            specifier: "foo".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::PackageJsonError {
            message: "bad pkg".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::PathMappingFailed {
            message: "no match".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::ModuleResolutionModeMismatch {
            specifier: "pkg".to_string(),
            containing_file: "/app/index.ts".to_string(),
            span: Span::new(0, 10),
        },
    ];

    for failure in &soft_failures {
        assert!(
            failure.should_try_fallback(),
            "Expected should_try_fallback=true for {:?}",
            std::mem::discriminant(failure)
        );
    }
}

#[test]
fn test_lookup_classic_implied_resolution_upgrades_to_ts2792() {
    // Under classic-style resolution (module: amd|system|umd|none or
    // explicit moduleResolution: classic — issue #3077), bare specifiers
    // that fail to resolve always upgrade TS2307 → TS2792 to surface the
    // "Did you mean to set the 'moduleResolution' option to 'nodenext'..."
    // hint. The presence of an ancestor `node_modules/<pkg>/` directory is
    // not required: tsc emits TS2792 for missing packages regardless.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_classic_ts2792");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(dir.join("node_modules").join("some-pkg")).unwrap();
    fs::write(dir.join("index.ts"), "import 'some-pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "some-pkg",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 18),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: true,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(!outcome.is_resolved);
    let error = outcome.error.expect("Expected error for missing module");
    assert_eq!(
        error.code, MODULE_RESOLUTION_MODE_MISMATCH,
        "Expected TS2792 for implied classic resolution with matching node_modules/<pkg>, got TS{}",
        error.code
    );
    assert!(
        error.message.contains("moduleResolution"),
        "TS2792 message should suggest moduleResolution option"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_classic_implied_resolution_without_node_modules_upgrades_to_ts2792() {
    // Even without a matching `node_modules/<pkg>/` ancestor, classic-style
    // resolution still upgrades TS2307 → TS2792 (issue #3077). Earlier
    // versions of this resolver gated the upgrade on node-style lookahead;
    // tsc 6.0.3 emits TS2792 unconditionally for bare specifiers under
    // classic resolution, so we no longer probe.
    //
    // Relative specifiers stay on plain TS2307 — see
    // `test_lookup_classic_implied_resolution_relative_keeps_ts2307` below.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_classic_ts2792_no_node_modules");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import 'some-pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "some-pkg",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 18),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: true,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(!outcome.is_resolved);
    let error = outcome.error.expect("Expected error for missing module");
    assert_eq!(
        error.code, MODULE_RESOLUTION_MODE_MISMATCH,
        "Expected TS2792 for bare specifier under classic resolution even without a node_modules entry, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_classic_implied_resolution_relative_keeps_ts2307() {
    // Relative specifiers stay on plain TS2307 — switching to a different
    // `moduleResolution` would not help them, so the TS2792 hint is
    // suppressed for the relative-import case (issue #3077).
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_classic_relative_ts2307");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import './missing';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./missing",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 19),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: true,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(!outcome.is_resolved);
    let error = outcome.error.expect("Expected error for missing module");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "Relative specifier under classic resolution should stay on TS2307, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_bare_json_specifier_nonexistent_upgrades_to_ts2732() {
    // Even for bare (non-relative) .json specifiers that don't exist,
    // lookup() should upgrade NotFound -> TS2732 when resolveJsonModule is false.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_bare_json_ts2732");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import 'config.json';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: false,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "config.json",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 21),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    let error = outcome.error.expect("Expected error for bare .json import");
    assert_eq!(
        error.code, JSON_MODULE_WITHOUT_RESOLVE_JSON_MODULE,
        "Expected TS2732 for bare .json without resolveJsonModule, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}
