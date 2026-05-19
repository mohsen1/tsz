//! Diagnostics Ts2307 tests for `module_resolver`.
//!
//! Tests that the resolver emits **TS2307 (Cannot find module)**
//! through every failure variant (`NotFound`, bare specifier failures,
//! scoped packages, `#imports`, path mappings, package.json errors,
//! circular resolution, batched diagnostics, Node16 exports failures).

use super::super::*;

#[test]
fn test_ts2307_error_code_constant() {
    assert_eq!(CANNOT_FIND_MODULE, 2307);
}

#[test]
fn test_emit_resolution_error_for_not_found() {
    let mut diagnostics = DiagnosticBag::new();
    let resolver = ModuleResolver::node_resolver();

    let failure = ResolutionFailure::NotFound {
        specifier: "./missing-module".to_string(),
        containing_file: "/src/file.ts".to_string(),
        span: Span::new(10, 30),
    };

    resolver.emit_resolution_error(&mut diagnostics, &failure);

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics.has_errors());
    let errors: Vec<_> = diagnostics.errors().collect();
    assert_eq!(errors[0].code, CANNOT_FIND_MODULE);
    assert!(errors[0].message.contains("Cannot find module"));
    assert!(errors[0].message.contains("./missing-module"));
}

#[test]
fn test_emit_resolution_error_all_variants_emit_ts2307() {
    let mut diagnostics = DiagnosticBag::new();
    let resolver = ModuleResolver::node_resolver();

    // All resolution failure variants should emit TS2307 diagnostics
    let failure = ResolutionFailure::InvalidSpecifier {
        message: "bad specifier".to_string(),
        containing_file: "/src/a.ts".to_string(),
        span: Span::new(0, 10),
    };
    resolver.emit_resolution_error(&mut diagnostics, &failure);
    assert_eq!(diagnostics.len(), 1);

    let failure = ResolutionFailure::PackageJsonError {
        message: "parse error".to_string(),
        containing_file: "/src/b.ts".to_string(),
        span: Span::new(5, 15),
    };
    resolver.emit_resolution_error(&mut diagnostics, &failure);
    assert_eq!(diagnostics.len(), 2);

    let failure = ResolutionFailure::CircularResolution {
        message: "a -> b -> a".to_string(),
        containing_file: "/src/c.ts".to_string(),
        span: Span::new(10, 20),
    };
    resolver.emit_resolution_error(&mut diagnostics, &failure);
    assert_eq!(diagnostics.len(), 3);

    let failure = ResolutionFailure::PathMappingFailed {
        message: "@/ pattern".to_string(),
        containing_file: "/src/d.ts".to_string(),
        span: Span::new(15, 25),
    };
    resolver.emit_resolution_error(&mut diagnostics, &failure);
    assert_eq!(diagnostics.len(), 4);

    // Verify all have TS2307 code
    for diag in diagnostics.errors() {
        assert_eq!(diag.code, CANNOT_FIND_MODULE);
    }
}

#[test]
fn test_relative_import_failure_produces_ts2307() {
    let failure = ResolutionFailure::NotFound {
        specifier: "./components/Button".to_string(),
        containing_file: "/src/App.tsx".to_string(),
        span: Span::new(20, 45),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
    assert_eq!(diagnostic.file_name, "/src/App.tsx");
    assert!(diagnostic.message.contains("./components/Button"));
    assert_eq!(diagnostic.span.start, 20);
    assert_eq!(diagnostic.span.end, 45);
}

#[test]
fn test_bare_specifier_failure_produces_ts2307() {
    let failure = ResolutionFailure::NotFound {
        specifier: "nonexistent-package".to_string(),
        containing_file: "/project/src/index.ts".to_string(),
        span: Span::new(7, 28),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
    assert!(diagnostic.message.contains("nonexistent-package"));
}

#[test]
fn test_scoped_package_failure_produces_ts2307() {
    let failure = ResolutionFailure::NotFound {
        specifier: "@org/missing-lib".to_string(),
        containing_file: "/app/main.ts".to_string(),
        span: Span::new(15, 35),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
    assert!(diagnostic.message.contains("@org/missing-lib"));
}

#[test]
fn test_hash_import_failure_produces_ts2307() {
    // Package.json subpath import failure
    let failure = ResolutionFailure::NotFound {
        specifier: "#utils/helpers".to_string(),
        containing_file: "/pkg/src/index.ts".to_string(),
        span: Span::new(8, 25),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
    assert!(diagnostic.message.contains("#utils/helpers"));
}

#[test]
fn test_path_mapping_failure_produces_ts2307() {
    let failure = ResolutionFailure::PathMappingFailed {
        message: "path mapping '@/utils/*' did not resolve to any file".to_string(),
        containing_file: "/project/src/index.ts".to_string(),
        span: Span::new(8, 30),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
    assert_eq!(diagnostic.file_name, "/project/src/index.ts");
    assert!(diagnostic.message.contains("Cannot find module"));
    assert!(diagnostic.message.contains("path mapping"));
}

#[test]
fn test_package_json_error_produces_ts2307() {
    let failure = ResolutionFailure::PackageJsonError {
        message: "invalid exports field in package.json".to_string(),
        containing_file: "/project/src/app.ts".to_string(),
        span: Span::new(15, 45),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
    assert_eq!(diagnostic.file_name, "/project/src/app.ts");
    assert!(diagnostic.message.contains("Cannot find module"));
}

#[test]
fn test_circular_resolution_produces_ts2307() {
    let failure = ResolutionFailure::CircularResolution {
        message: "circular dependency: a.ts -> b.ts -> a.ts".to_string(),
        containing_file: "/project/src/a.ts".to_string(),
        span: Span::new(20, 50),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
    assert_eq!(diagnostic.file_name, "/project/src/a.ts");
    assert!(diagnostic.message.contains("Cannot find module"));
    assert!(diagnostic.message.contains("circular"));
}

#[test]
fn test_diagnostic_bag_collects_multiple_resolution_errors() {
    let mut diagnostics = DiagnosticBag::new();
    let resolver = ModuleResolver::node_resolver();

    let failures = vec![
        ResolutionFailure::NotFound {
            specifier: "./module1".to_string(),
            containing_file: "a.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::NotFound {
            specifier: "./module2".to_string(),
            containing_file: "b.ts".to_string(),
            span: Span::new(5, 15),
        },
        ResolutionFailure::NotFound {
            specifier: "external-pkg".to_string(),
            containing_file: "c.ts".to_string(),
            span: Span::new(10, 25),
        },
    ];

    for failure in &failures {
        resolver.emit_resolution_error(&mut diagnostics, failure);
    }

    assert_eq!(diagnostics.len(), 3);
    assert_eq!(diagnostics.error_count(), 3);

    // Verify all have TS2307 code
    let codes: Vec<_> = diagnostics.errors().map(|d| d.code).collect();
    assert!(codes.iter().all(|&c| c == CANNOT_FIND_MODULE));
}

#[test]
fn test_node16_exports_failure_produces_ts2307_not_ts2792() {
    // When moduleResolution is Node16/NodeNext and a package has an exports
    // field but the subpath can't be resolved, the error should be TS2307
    // (NotFound), NOT TS2792 (ModuleResolutionModeMismatch).
    // TS2792 means "set moduleResolution to nodenext" which is nonsensical
    // when you're already on Node16/NodeNext.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_node16_exports_ts2307");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/inner")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    // Package with exports field that won't match our subpath
    fs::write(
        dir.join("node_modules/inner/package.json"),
        r#"{"name":"inner","exports":{"./cjs/*":"./*.cjs","./mjs/*":"./*.mjs"}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import * as x from 'inner/cjs/foo';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };

    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve(
        "inner/cjs/foo",
        &dir.join("src/index.ts"),
        Span::new(24, 38),
    );

    // Should be NotFound (TS2307), not ModuleResolutionModeMismatch (TS2792)
    let err = result.expect_err("Expected resolution to fail");
    let diagnostic = err.to_diagnostic();
    assert_eq!(
        diagnostic.code, CANNOT_FIND_MODULE,
        "Node16 exports failure should produce TS2307, not TS2792"
    );
    assert!(
        !matches!(err, ResolutionFailure::ModuleResolutionModeMismatch { .. }),
        "Node16 exports failure should NOT be ModuleResolutionModeMismatch"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_nodenext_entry_exports_failure_produces_ts2307() {
    // Same test but for entry point (no subpath) with NodeNext resolution.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_nodenext_entry_ts2307");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    // Package with exports "." pointing to a non-existent file
    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{".":"./nonexistent.js"}}"#,
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import * as p from 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_exports: true,
        ..Default::default()
    };

    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("pkg", &dir.join("src/index.ts"), Span::new(24, 29));

    let err = result.expect_err("Expected resolution to fail");
    let diagnostic = err.to_diagnostic();
    assert_eq!(
        diagnostic.code, CANNOT_FIND_MODULE,
        "NodeNext entry exports failure should produce TS2307, not TS2792"
    );

    let _ = fs::remove_dir_all(&dir);
}
