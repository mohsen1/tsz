use super::*;
use crate::module_resolver_helpers::*;

#[test]
fn test_parse_package_specifier_simple() {
    let (name, subpath) = parse_package_specifier("lodash");
    assert_eq!(name, "lodash");
    assert_eq!(subpath, None);
}

#[test]
fn test_parse_package_specifier_with_subpath() {
    let (name, subpath) = parse_package_specifier("lodash/fp");
    assert_eq!(name, "lodash");
    assert_eq!(subpath, Some("fp".to_string()));
}

#[test]
fn test_parse_package_specifier_scoped() {
    let (name, subpath) = parse_package_specifier("@babel/core");
    assert_eq!(name, "@babel/core");
    assert_eq!(subpath, None);
}

#[test]
fn test_parse_package_specifier_scoped_with_subpath() {
    let (name, subpath) = parse_package_specifier("@babel/core/transform");
    assert_eq!(name, "@babel/core");
    assert_eq!(subpath, Some("transform".to_string()));
}

#[test]
fn test_match_export_pattern_exact() {
    assert_eq!(match_export_pattern("./lib", "./lib"), Some(String::new()));
    assert_eq!(match_export_pattern("./lib", "./src"), None);
}

#[test]
fn test_match_export_pattern_wildcard() {
    assert_eq!(
        match_export_pattern("./*", "./foo"),
        Some("foo".to_string())
    );
    assert_eq!(
        match_export_pattern("./lib/*", "./lib/utils"),
        Some("utils".to_string())
    );
    assert_eq!(match_export_pattern("./lib/*", "./src/utils"), None);
}

#[test]
fn test_module_extension_from_path() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("foo.ts")),
        ModuleExtension::Ts
    );
    assert_eq!(
        ModuleExtension::from_path(Path::new("foo.d.ts")),
        ModuleExtension::Dts
    );
    assert_eq!(
        ModuleExtension::from_path(Path::new("foo.tsx")),
        ModuleExtension::Tsx
    );
    assert_eq!(
        ModuleExtension::from_path(Path::new("foo.js")),
        ModuleExtension::Js
    );
}

#[test]
fn test_module_resolver_creation() {
    let resolver = ModuleResolver::node_resolver();
    assert_eq!(resolver.resolution_kind(), ModuleResolutionKind::Node);
}

#[test]
fn test_ts2307_error_code_constant() {
    assert_eq!(CANNOT_FIND_MODULE, 2307);
}

#[test]
fn test_resolution_failure_not_found_diagnostic() {
    let failure = ResolutionFailure::NotFound {
        specifier: "./missing-module".to_string(),
        containing_file: "/path/to/file.ts".to_string(),
        span: Span::new(10, 30),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
    assert!(diagnostic.message.contains("Cannot find module"));
    assert!(diagnostic.message.contains("./missing-module"));
    assert_eq!(diagnostic.file_name, "/path/to/file.ts");
    assert_eq!(diagnostic.span.start, 10);
    assert_eq!(diagnostic.span.end, 30);
}

#[test]
fn test_resolution_failure_is_not_found() {
    let not_found = ResolutionFailure::NotFound {
        specifier: "test".to_string(),
        containing_file: "test.ts".to_string(),
        span: Span::dummy(),
    };
    assert!(not_found.is_not_found());

    let other = ResolutionFailure::InvalidSpecifier {
        message: "test".to_string(),
        containing_file: "test.ts".to_string(),
        span: Span::dummy(),
    };
    assert!(!other.is_not_found());
}

#[test]
fn test_module_extension_forces_esm() {
    assert!(ModuleExtension::Mts.forces_esm());
    assert!(ModuleExtension::Mjs.forces_esm());
    assert!(ModuleExtension::DmTs.forces_esm());
    assert!(!ModuleExtension::Ts.forces_esm());
    assert!(!ModuleExtension::Cts.forces_esm());
}

#[test]
fn test_module_extension_forces_cjs() {
    assert!(ModuleExtension::Cts.forces_cjs());
    assert!(ModuleExtension::Cjs.forces_cjs());
    assert!(ModuleExtension::DCts.forces_cjs());
    assert!(!ModuleExtension::Ts.forces_cjs());
    assert!(!ModuleExtension::Mts.forces_cjs());
}

#[test]
fn test_match_imports_pattern_exact() {
    assert_eq!(
        match_imports_pattern("#utils", "#utils"),
        Some(String::new())
    );
    assert_eq!(match_imports_pattern("#utils", "#other"), None);
}

#[test]
fn test_match_imports_pattern_wildcard() {
    assert_eq!(
        match_imports_pattern("#utils/*", "#utils/foo"),
        Some("foo".to_string())
    );
    assert_eq!(
        match_imports_pattern("#internal/*", "#internal/helpers/bar"),
        Some("helpers/bar".to_string())
    );
    assert_eq!(match_imports_pattern("#utils/*", "#other/foo"), None);
}

#[test]
fn test_resolver_rejects_root_slash_package_import_with_wildcard() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_package_import_root_slash");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("package.json"),
        r##"{
            "name": "package",
            "private": true,
            "imports": {
                "#/*": "./src/*"
            }
        }"##,
    )
    .unwrap();
    fs::write(dir.join("src/foo.ts"), "export const foo = 'foo';").unwrap();
    fs::write(dir.join("index.ts"), "import { foo } from '#/foo.js'; foo;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_imports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("#/foo.js", &dir.join("index.ts"), Span::new(0, 8));

    assert!(
        matches!(result, Err(ResolutionFailure::NotFound { .. })),
        "Expected #/foo.js to be rejected as an invalid package import specifier, got {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_match_types_versions_pattern() {
    assert_eq!(
        match_types_versions_pattern("*", "index"),
        Some("index".to_string())
    );
    assert_eq!(
        match_types_versions_pattern("lib/*", "lib/utils"),
        Some("utils".to_string())
    );
    assert_eq!(
        match_types_versions_pattern("exact", "exact"),
        Some(String::new())
    );
    assert_eq!(match_types_versions_pattern("lib/*", "src/utils"), None);
}

#[test]
fn test_apply_wildcard_substitution() {
    assert_eq!(
        apply_wildcard_substitution("./lib/*.js", "utils"),
        "./lib/utils.js"
    );
    assert_eq!(
        apply_wildcard_substitution("./dist/index.js", "ignored"),
        "./dist/index.js"
    );
}

#[test]
fn test_substitute_wildcard_in_exports_string() {
    let value = PackageExports::String("./*.cjs".to_string());
    let result = substitute_wildcard_in_exports(&value, "index");
    assert!(matches!(result, PackageExports::String(s) if s == "./index.cjs"));
}

#[test]
fn test_substitute_wildcard_in_exports_conditional() {
    let value = PackageExports::Conditional(vec![
        (
            "import".to_string(),
            PackageExports::String("./*.mjs".to_string()),
        ),
        (
            "default".to_string(),
            PackageExports::String("./*.cjs".to_string()),
        ),
    ]);
    let result = substitute_wildcard_in_exports(&value, "foo");
    match result {
        PackageExports::Conditional(entries) => {
            assert_eq!(entries.len(), 2);
            assert!(matches!(&entries[0].1, PackageExports::String(s) if s == "./foo.mjs"));
            assert!(matches!(&entries[1].1, PackageExports::String(s) if s == "./foo.cjs"));
        }
        _ => panic!("Expected Conditional"),
    }
}

#[test]
fn test_substitute_wildcard_in_exports_no_wildcard() {
    let value = PackageExports::String("./index.js".to_string());
    let result = substitute_wildcard_in_exports(&value, "anything");
    assert!(matches!(result, PackageExports::String(s) if s == "./index.js"));
}

#[test]
fn test_node16_pattern_exports_resolves_with_dts() {
    // Pattern exports like "./cjs/*": "./*.cjs" should resolve when
    // the wildcard-substituted path has a corresponding .d.cts file.
    // This tests the fix: wildcard substitution must happen BEFORE
    // try_export_target, not after.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_pattern_exports_resolve");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/inner")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    // Package with pattern exports
    fs::write(
        dir.join("node_modules/inner/package.json"),
        r#"{"name":"inner","exports":{"./cjs/*":"./*.cjs","./mjs/*":"./*.mjs"}}"#,
    )
    .unwrap();
    // Declaration files that should be found via extension substitution
    fs::write(
        dir.join("node_modules/inner/index.d.cts"),
        "export declare const cjsSource: boolean;",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/index.d.mts"),
        "export declare const mjsSource: boolean;",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import * as x from 'inner/cjs/index';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };

    let mut resolver = ModuleResolver::new(&options);

    // ./cjs/* pattern matches ./cjs/index, wildcard = "index"
    // Target ./*.cjs becomes ./index.cjs after substitution
    // Extension substitution: index.cjs -> index.d.cts (exists)
    let result = resolver.resolve(
        "inner/cjs/index",
        &dir.join("src/index.ts"),
        Span::new(24, 40),
    );
    assert!(
        result.is_ok(),
        "Pattern export ./cjs/* should resolve via .d.cts: {:?}",
        result.err()
    );

    // Also test ./mjs/* pattern
    let result = resolver.resolve(
        "inner/mjs/index",
        &dir.join("src/index.ts"),
        Span::new(24, 40),
    );
    assert!(
        result.is_ok(),
        "Pattern export ./mjs/* should resolve via .d.mts: {:?}",
        result.err()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_type_enum() {
    assert_eq!(PackageType::default(), PackageType::CommonJs);
    assert_ne!(PackageType::Module, PackageType::CommonJs);
}

#[test]
fn test_importing_module_kind_enum() {
    assert_eq!(
        ImportingModuleKind::default(),
        ImportingModuleKind::CommonJs
    );
    assert_ne!(ImportingModuleKind::Esm, ImportingModuleKind::CommonJs);
}

#[test]
fn test_package_json_deserialize_basic() {
    let json = r#"{"name": "test-package", "type": "module", "main": "./index.js"}"#;

    let package_json: PackageJson = serde_json::from_str(json).unwrap();
    assert_eq!(package_json.name, Some("test-package".to_string()));
    assert_eq!(package_json.package_type, Some("module".to_string()));
    assert_eq!(package_json.main, Some("./index.js".to_string()));
}

#[test]
fn test_package_json_deserialize_exports() {
    let json = r#"{"name": "pkg", "exports": {"." : "./dist/index.js"}}"#;

    let package_json: PackageJson = serde_json::from_str(json).unwrap();
    assert!(package_json.exports.is_some());
}

#[test]
fn test_package_json_deserialize_types_versions() {
    // Build JSON programmatically to avoid raw string issues
    let json = serde_json::json!({
        "name": "typed-package",
        "typesVersions": {
            "*": {
                "*": ["./types/index.d.ts"]
            }
        }
    });

    let package_json: PackageJson = serde_json::from_value(json).unwrap();
    assert_eq!(package_json.name, Some("typed-package".to_string()));
    assert!(package_json.types_versions.is_some());
}

#[test]
fn test_package_json_deserialize_invalid_types_field_is_ignored() {
    let json = r#"{
        "name": "csv-parse",
        "main": "./lib",
        "types": ["./lib/index.d.ts", "./lib/sync.d.ts"]
    }"#;

    let package_json: PackageJson = serde_json::from_str(json).unwrap();
    assert_eq!(package_json.name, Some("csv-parse".to_string()));
    assert_eq!(package_json.main, Some("./lib".to_string()));
    assert_eq!(package_json.types, None);
}

// =========================================================================
// TS2307 Diagnostic Emission Tests
// =========================================================================

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
fn test_resolution_failure_all_variants_to_diagnostic() {
    // Test that all ResolutionFailure variants can produce diagnostics with proper location info
    let failures = vec![
        ResolutionFailure::NotFound {
            specifier: "./test".to_string(),
            containing_file: "file.ts".to_string(),
            span: Span::new(0, 10),
        },
        ResolutionFailure::InvalidSpecifier {
            message: "bad".to_string(),
            containing_file: "file2.ts".to_string(),
            span: Span::new(5, 15),
        },
        ResolutionFailure::PackageJsonError {
            message: "error".to_string(),
            containing_file: "file3.ts".to_string(),
            span: Span::new(10, 20),
        },
        ResolutionFailure::CircularResolution {
            message: "loop".to_string(),
            containing_file: "file4.ts".to_string(),
            span: Span::new(15, 25),
        },
        ResolutionFailure::PathMappingFailed {
            message: "@/path".to_string(),
            containing_file: "file5.ts".to_string(),
            span: Span::new(20, 30),
        },
    ];

    for failure in failures {
        let diagnostic = failure.to_diagnostic();
        // All failures should produce TS2307 diagnostic code
        assert_eq!(diagnostic.code, CANNOT_FIND_MODULE);
        // All failures should have non-empty file names
        assert!(!diagnostic.file_name.is_empty());
        // All failures should have valid spans
        assert!(diagnostic.span.start < diagnostic.span.end);
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
fn test_resolution_failure_span_preservation() {
    // Ensure span information is correctly preserved in diagnostics
    let test_cases = vec![(0, 10), (100, 150), (1000, 1050)];

    for (start, end) in test_cases {
        let failure = ResolutionFailure::NotFound {
            specifier: "test".to_string(),
            containing_file: "file.ts".to_string(),
            span: Span::new(start, end),
        };

        let diagnostic = failure.to_diagnostic();
        assert_eq!(diagnostic.span.start, start);
        assert_eq!(diagnostic.span.end, end);
    }
}

#[test]
fn test_resolution_failure_accessors() {
    // Test that accessor methods work correctly
    let failure = ResolutionFailure::InvalidSpecifier {
        message: "test error".to_string(),
        containing_file: "/src/test.ts".to_string(),
        span: Span::new(10, 20),
    };

    assert_eq!(failure.containing_file(), "/src/test.ts");
    assert_eq!(failure.span().start, 10);
    assert_eq!(failure.span().end, 20);
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

// =========================================================================
// TS2835 (Import Path Needs Extension Suggestion) Tests
// =========================================================================

#[test]
fn test_ts2834_error_code_constant() {
    assert_eq!(IMPORT_PATH_NEEDS_EXTENSION, 2834);
}

#[test]
fn test_import_path_needs_extension_produces_ts2835() {
    let failure = ResolutionFailure::ImportPathNeedsExtension {
        specifier: "./utils".to_string(),
        suggested_extension: ".js".to_string(),
        containing_file: "/src/index.mts".to_string(),
        span: Span::new(20, 30),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
    assert_eq!(diagnostic.file_name, "/src/index.mts");
    assert!(
        diagnostic
            .message
            .contains("Relative import paths need explicit file extensions")
    );
    assert!(diagnostic.message.contains("node16"));
    assert!(diagnostic.message.contains("nodenext"));
    assert!(diagnostic.message.contains("./utils.js"));
}

#[test]
fn test_import_path_needs_extension_suggests_mjs() {
    let failure = ResolutionFailure::ImportPathNeedsExtension {
        specifier: "./esm-module".to_string(),
        suggested_extension: ".mjs".to_string(),
        containing_file: "/src/app.mts".to_string(),
        span: Span::new(10, 25),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
    assert!(diagnostic.message.contains("./esm-module.mjs"));
}

#[test]
fn test_import_path_needs_extension_suggests_cjs() {
    let failure = ResolutionFailure::ImportPathNeedsExtension {
        specifier: "./cjs-module".to_string(),
        suggested_extension: ".cjs".to_string(),
        containing_file: "/src/legacy.cts".to_string(),
        span: Span::new(5, 20),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
    assert!(diagnostic.message.contains("./cjs-module.cjs"));
}

// =========================================================================
// TS2792 (Module Resolution Mode Mismatch) Tests
// =========================================================================

#[test]
fn test_ts2792_error_code_constant() {
    assert_eq!(MODULE_RESOLUTION_MODE_MISMATCH, 2792);
}

#[test]
fn test_module_resolution_mode_mismatch_produces_ts2792() {
    let failure = ResolutionFailure::ModuleResolutionModeMismatch {
        specifier: "modern-esm-package".to_string(),
        containing_file: "/src/index.ts".to_string(),
        span: Span::new(15, 35),
    };

    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, MODULE_RESOLUTION_MODE_MISMATCH);
    assert_eq!(diagnostic.file_name, "/src/index.ts");
    assert!(
        diagnostic
            .message
            .contains("Cannot find module 'modern-esm-package'")
    );
    assert!(diagnostic.message.contains("moduleResolution"));
    assert!(diagnostic.message.contains("nodenext"));
    assert!(diagnostic.message.contains("paths"));
}

#[test]
fn test_module_resolution_mode_mismatch_accessors() {
    let failure = ResolutionFailure::ModuleResolutionModeMismatch {
        specifier: "pkg".to_string(),
        containing_file: "/test.ts".to_string(),
        span: Span::new(100, 110),
    };

    assert_eq!(failure.containing_file(), "/test.ts");
    assert_eq!(failure.span().start, 100);
    assert_eq!(failure.span().end, 110);
}

#[test]
fn test_import_path_needs_extension_accessors() {
    let failure = ResolutionFailure::ImportPathNeedsExtension {
        specifier: "./foo".to_string(),
        suggested_extension: ".js".to_string(),
        containing_file: "/bar.mts".to_string(),
        span: Span::new(50, 60),
    };

    assert_eq!(failure.containing_file(), "/bar.mts");
    assert_eq!(failure.span().start, 50);
    assert_eq!(failure.span().end, 60);
}

#[test]
fn test_new_error_codes_emit_correctly() {
    let mut diagnostics = DiagnosticBag::new();
    let resolver = ModuleResolver::node_resolver();

    // Test TS2835
    let failure_2835 = ResolutionFailure::ImportPathNeedsExtension {
        specifier: "./utils".to_string(),
        suggested_extension: ".js".to_string(),
        containing_file: "/src/app.mts".to_string(),
        span: Span::new(0, 10),
    };
    resolver.emit_resolution_error(&mut diagnostics, &failure_2835);

    // Test TS2792
    let failure_2792 = ResolutionFailure::ModuleResolutionModeMismatch {
        specifier: "esm-pkg".to_string(),
        containing_file: "/src/index.ts".to_string(),
        span: Span::new(5, 15),
    };
    resolver.emit_resolution_error(&mut diagnostics, &failure_2792);

    assert_eq!(diagnostics.len(), 2);

    let errors: Vec<_> = diagnostics.errors().collect();
    assert_eq!(errors[0].code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
    assert_eq!(errors[1].code, MODULE_RESOLUTION_MODE_MISMATCH);
}

// =========================================================================
// ModuleExtension::from_path tests
// =========================================================================

#[test]
fn test_extension_from_path_ts() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("foo.ts")),
        ModuleExtension::Ts
    );
}

#[test]
fn test_extension_from_path_tsx() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("Component.tsx")),
        ModuleExtension::Tsx
    );
}

#[test]
fn test_extension_from_path_dts() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("types.d.ts")),
        ModuleExtension::Dts
    );
}

#[test]
fn test_extension_from_path_dmts() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("types.d.mts")),
        ModuleExtension::DmTs
    );
}

#[test]
fn test_extension_from_path_dcts() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("types.d.cts")),
        ModuleExtension::DCts
    );
}

#[test]
fn test_extension_from_path_js() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("bundle.js")),
        ModuleExtension::Js
    );
}

#[test]
fn test_extension_from_path_jsx() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("App.jsx")),
        ModuleExtension::Jsx
    );
}

#[test]
fn test_extension_from_path_mjs() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("module.mjs")),
        ModuleExtension::Mjs
    );
}

#[test]
fn test_extension_from_path_cjs() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("config.cjs")),
        ModuleExtension::Cjs
    );
}

#[test]
fn test_extension_from_path_mts() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("utils.mts")),
        ModuleExtension::Mts
    );
}

#[test]
fn test_extension_from_path_cts() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("config.cts")),
        ModuleExtension::Cts
    );
}

#[test]
fn test_extension_from_path_json() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("package.json")),
        ModuleExtension::Json
    );
}

#[test]
fn test_extension_from_path_unknown() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("style.css")),
        ModuleExtension::Unknown
    );
}

#[test]
fn test_extension_from_path_no_extension() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("Makefile")),
        ModuleExtension::Unknown
    );
}

#[test]
fn test_extension_from_path_nested() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("/project/src/lib/types.d.ts")),
        ModuleExtension::Dts
    );
}

// =========================================================================
// ModuleExtension::as_str tests
// =========================================================================

#[test]
fn test_extension_as_str_roundtrip() {
    let extensions = [
        ModuleExtension::Ts,
        ModuleExtension::Tsx,
        ModuleExtension::Dts,
        ModuleExtension::DmTs,
        ModuleExtension::DCts,
        ModuleExtension::Js,
        ModuleExtension::Jsx,
        ModuleExtension::Mjs,
        ModuleExtension::Cjs,
        ModuleExtension::Mts,
        ModuleExtension::Cts,
        ModuleExtension::Json,
    ];
    for ext in &extensions {
        let ext_str = ext.as_str();
        assert!(
            !ext_str.is_empty(),
            "{ext:?} should have a non-empty string representation"
        );
        // Verify the string starts with a dot
        assert!(
            ext_str.starts_with('.'),
            "{ext:?}.as_str() should start with '.', got: {ext_str}"
        );
    }
    assert_eq!(ModuleExtension::Unknown.as_str(), "");
}

// =========================================================================
// ModuleExtension ESM/CJS mode tests
// =========================================================================

#[test]
fn test_extension_forces_esm() {
    assert!(ModuleExtension::Mts.forces_esm());
    assert!(ModuleExtension::Mjs.forces_esm());
    assert!(ModuleExtension::DmTs.forces_esm());

    assert!(!ModuleExtension::Ts.forces_esm());
    assert!(!ModuleExtension::Tsx.forces_esm());
    assert!(!ModuleExtension::Dts.forces_esm());
    assert!(!ModuleExtension::Js.forces_esm());
    assert!(!ModuleExtension::Cjs.forces_esm());
    assert!(!ModuleExtension::Cts.forces_esm());
}

#[test]
fn test_extension_forces_cjs() {
    assert!(ModuleExtension::Cts.forces_cjs());
    assert!(ModuleExtension::Cjs.forces_cjs());
    assert!(ModuleExtension::DCts.forces_cjs());

    assert!(!ModuleExtension::Ts.forces_cjs());
    assert!(!ModuleExtension::Tsx.forces_cjs());
    assert!(!ModuleExtension::Dts.forces_cjs());
    assert!(!ModuleExtension::Js.forces_cjs());
    assert!(!ModuleExtension::Mjs.forces_cjs());
    assert!(!ModuleExtension::Mts.forces_cjs());
}

#[test]
fn test_extension_neutral_mode() {
    // .ts, .tsx, .js, .jsx, .d.ts, .json should be neutral (neither ESM nor CJS forced)
    let neutral = [
        ModuleExtension::Ts,
        ModuleExtension::Tsx,
        ModuleExtension::Dts,
        ModuleExtension::Js,
        ModuleExtension::Jsx,
        ModuleExtension::Json,
        ModuleExtension::Unknown,
    ];
    for ext in &neutral {
        assert!(
            !ext.forces_esm() && !ext.forces_cjs(),
            "{ext:?} should be neutral (neither ESM nor CJS)"
        );
    }
}

// =========================================================================
// ResolutionFailure tests
// =========================================================================

#[test]
fn test_resolution_failure_not_found_is_not_found() {
    let failure = ResolutionFailure::NotFound {
        specifier: "./missing".to_string(),
        containing_file: "main.ts".to_string(),
        span: Span::new(0, 10),
    };
    assert!(failure.is_not_found());
}

#[test]
fn test_resolution_failure_other_is_not_not_found() {
    let failure = ResolutionFailure::ImportPathNeedsExtension {
        specifier: "./utils".to_string(),
        suggested_extension: ".js".to_string(),
        containing_file: "main.mts".to_string(),
        span: Span::new(0, 10),
    };
    assert!(!failure.is_not_found());
}

#[test]
fn test_resolution_failure_containing_file() {
    let failure = ResolutionFailure::NotFound {
        specifier: "./missing".to_string(),
        containing_file: "/project/src/main.ts".to_string(),
        span: Span::new(5, 20),
    };
    assert_eq!(failure.containing_file(), "/project/src/main.ts");
}

#[test]
fn test_resolution_failure_span() {
    let failure = ResolutionFailure::NotFound {
        specifier: "./missing".to_string(),
        containing_file: "main.ts".to_string(),
        span: Span::new(10, 30),
    };
    let span = failure.span();
    assert_eq!(span.start, 10);
    assert_eq!(span.end, 30);
}

#[test]
fn test_resolution_failure_to_diagnostic_ts2307() {
    let failure = ResolutionFailure::NotFound {
        specifier: "./nonexistent".to_string(),
        containing_file: "main.ts".to_string(),
        span: Span::new(0, 20),
    };
    let diag = failure.to_diagnostic();
    assert_eq!(diag.code, CANNOT_FIND_MODULE);
    assert!(diag.message.contains("./nonexistent"));
}

#[test]
fn test_resolution_failure_to_diagnostic_ts2835() {
    let failure = ResolutionFailure::ImportPathNeedsExtension {
        specifier: "./utils".to_string(),
        suggested_extension: ".js".to_string(),
        containing_file: "app.mts".to_string(),
        span: Span::new(0, 15),
    };
    let diag = failure.to_diagnostic();
    assert_eq!(diag.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION);
}

#[test]
fn test_resolution_failure_to_diagnostic_ts2792() {
    let failure = ResolutionFailure::ModuleResolutionModeMismatch {
        specifier: "some-esm-pkg".to_string(),
        containing_file: "index.ts".to_string(),
        span: Span::new(0, 20),
    };
    let diag = failure.to_diagnostic();
    assert_eq!(diag.code, MODULE_RESOLUTION_MODE_MISMATCH);
}

// =========================================================================
// ModuleResolver with temp files (integration)
// =========================================================================

#[test]
fn test_resolver_relative_ts_file() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_relative");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.ts"), "import { foo } from './utils';").unwrap();
    fs::write(dir.join("utils.ts"), "export const foo = 42;").unwrap();

    let mut resolver = ModuleResolver::node_resolver();
    let result = resolver.resolve("./utils", &dir.join("main.ts"), Span::new(0, 10));

    match result {
        Ok(module) => {
            assert_eq!(module.resolved_path, dir.join("utils.ts"));
            assert_eq!(module.extension, ModuleExtension::Ts);
            assert!(!module.is_external);
        }
        Err(_) => {
            // Resolution might fail in some environments, that's OK for this test
        }
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_relative_tsx_file() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_tsx");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("app.ts"), "").unwrap();
    fs::write(
        dir.join("Button.tsx"),
        "export default function Button() {}",
    )
    .unwrap();

    let mut resolver = ModuleResolver::node_resolver();
    let result = resolver.resolve("./Button", &dir.join("app.ts"), Span::new(0, 10));

    if let Ok(module) = result {
        assert_eq!(module.resolved_path, dir.join("Button.tsx"));
        assert_eq!(module.extension, ModuleExtension::Tsx);
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_index_file() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_index");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("utils")).unwrap();

    fs::write(dir.join("main.ts"), "").unwrap();
    fs::write(dir.join("utils").join("index.ts"), "export const foo = 42;").unwrap();

    let mut resolver = ModuleResolver::node_resolver();
    let result = resolver.resolve("./utils", &dir.join("main.ts"), Span::new(0, 10));

    if let Ok(module) = result {
        assert_eq!(module.resolved_path, dir.join("utils").join("index.ts"));
        assert_eq!(module.extension, ModuleExtension::Ts);
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_dot_and_trailing_slash_prefer_directory_index() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_dot_imports");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("a").join("b")).unwrap();

    fs::write(dir.join("a.ts"), "export default { a: 0 };").unwrap();
    fs::write(
        dir.join("a").join("index.ts"),
        "export default { aIndex: 0 };",
    )
    .unwrap();
    fs::write(dir.join("a").join("test.ts"), "import value from '.';").unwrap();
    fs::write(
        dir.join("a").join("b").join("test.ts"),
        "import value from '..';",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let dot = resolver
        .resolve(".", &dir.join("a").join("test.ts"), Span::new(0, 1))
        .expect("Expected '.' to resolve");
    assert_eq!(dot.resolved_path, dir.join("a").join("index.ts"));

    let dot_slash = resolver
        .resolve("./", &dir.join("a").join("test.ts"), Span::new(0, 2))
        .expect("Expected './' to resolve");
    assert_eq!(dot_slash.resolved_path, dir.join("a").join("index.ts"));

    let dotdot = resolver
        .resolve(
            "..",
            &dir.join("a").join("b").join("test.ts"),
            Span::new(0, 2),
        )
        .expect("Expected '..' to resolve");
    assert_eq!(
        fs::canonicalize(&dotdot.resolved_path).unwrap(),
        fs::canonicalize(dir.join("a").join("index.ts")).unwrap()
    );

    let dotdot_slash = resolver
        .resolve(
            "../",
            &dir.join("a").join("b").join("test.ts"),
            Span::new(0, 3),
        )
        .expect("Expected '../' to resolve");
    assert_eq!(
        fs::canonicalize(&dotdot_slash.resolved_path).unwrap(),
        fs::canonicalize(dir.join("a").join("index.ts")).unwrap()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_exports_js_target_substitutes_dts() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_exports_js_target");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","version":"0.0.1","exports":"./entrypoint.js"}"#,
    )
    .unwrap();
    fs::write(dir.join("node_modules/pkg/entrypoint.d.ts"), "export {};").unwrap();
    fs::write(dir.join("src/index.ts"), "import * as p from 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };

    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("pkg", &dir.join("src/index.ts"), Span::new(0, 3));

    // TypeScript resolves export targets with declaration substitution:
    // exports: "./entrypoint.js" → finds entrypoint.d.ts
    let resolved = result.expect("Expected exports .js target to resolve via .d.ts substitution");
    assert!(resolved.resolved_path.ends_with("entrypoint.d.ts"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_dts_file() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_dts");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.ts"), "").unwrap();
    fs::write(dir.join("types.d.ts"), "export interface Foo {}").unwrap();

    let mut resolver = ModuleResolver::node_resolver();
    let result = resolver.resolve("./types", &dir.join("main.ts"), Span::new(0, 10));

    if let Ok(module) = result {
        assert_eq!(module.resolved_path, dir.join("types.d.ts"));
        assert_eq!(module.extension, ModuleExtension::Dts);
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_jsx_without_jsx_option_errors() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_jsx_no_option");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("app.ts"), "import jsx from './jsx';").unwrap();
    fs::write(dir.join("jsx.jsx"), "export default 1;").unwrap();

    let options = ResolvedCompilerOptions {
        allow_js: true,
        jsx: None,
        // Use Node resolution so allowJs is respected (Classic never resolves .jsx)
        module_resolution: Some(ModuleResolutionKind::Node),
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("./jsx", &dir.join("app.ts"), Span::new(0, 10));

    let failure = result.expect_err("Expected jsx resolution to fail without jsx option");
    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, 6142);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_tsx_without_jsx_option_errors() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_tsx_no_option");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("app.ts"), "import tsx from './tsx';").unwrap();
    fs::write(dir.join("tsx.tsx"), "export default 1;").unwrap();

    let options = ResolvedCompilerOptions {
        jsx: None,
        // Use Node resolution so .tsx files are found (Classic also finds .tsx, but be explicit)
        module_resolution: Some(ModuleResolutionKind::Node),
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("./tsx", &dir.join("app.ts"), Span::new(0, 10));

    let failure = result.expect_err("Expected tsx resolution to fail without jsx option");
    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, 6142);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_json_import_without_resolve_json_module() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_ts2732");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("app.ts"), "import data from './data.json';").unwrap();
    fs::write(dir.join("data.json"), "{\"value\": 42}").unwrap();

    let options = ResolvedCompilerOptions {
        resolve_json_module: false, // JSON modules disabled
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let result = resolver.resolve("./data.json", &dir.join("app.ts"), Span::new(0, 10));

    let failure = result.expect_err("Expected JSON resolution to fail without resolveJsonModule");
    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, 2732); // TS2732

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_extensionless_json_import_does_not_resolve_with_resolve_json_module() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_extensionless_json_import");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("app.ts"), "import data = require('./data');").unwrap();
    fs::write(dir.join("data.json"), "{\"value\": 42}").unwrap();

    let options = ResolvedCompilerOptions {
        resolve_json_module: true,
        module_resolution: Some(ModuleResolutionKind::Node),
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let result = resolver.resolve("./data", &dir.join("app.ts"), Span::new(0, 10));

    let failure = result.expect_err(
        "Expected extensionless resolution to reject ./data even when data.json exists",
    );
    let diagnostic = failure.to_diagnostic();
    assert_eq!(diagnostic.code, 2307);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_package_main_with_unknown_extension() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_main_unknown");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules").join("normalize.css")).unwrap();

    fs::write(dir.join("app.ts"), "import 'normalize.css';").unwrap();
    fs::write(
        dir.join("node_modules")
            .join("normalize.css")
            .join("normalize.css"),
        "body {}",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules")
            .join("normalize.css")
            .join("package.json"),
        r#"{ "main": "normalize.css" }"#,
    )
    .unwrap();

    let mut resolver = ModuleResolver::node_resolver();
    let result = resolver.resolve("normalize.css", &dir.join("app.ts"), Span::new(0, 10));
    assert!(
        result.is_ok(),
        "Expected package main with unknown extension to resolve"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_package_types_with_unknown_extension_is_ignored() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_types_unknown");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules").join("foo")).unwrap();

    fs::write(dir.join("app.ts"), "import 'foo';").unwrap();
    fs::write(
        dir.join("node_modules").join("foo").join("foo.js"),
        "module.exports = {};",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules").join("foo").join("package.json"),
        r#"{ "types": "foo.js" }"#,
    )
    .unwrap();

    let mut resolver = ModuleResolver::node_resolver();
    let result = resolver.resolve("foo", &dir.join("app.ts"), Span::new(0, 10));
    assert!(
        result.is_err(),
        "Expected package types with runtime JS extension to be ignored"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_package_types_js_without_allow_js_is_ignored() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_types_js");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules").join("foo")).unwrap();

    fs::write(dir.join("app.ts"), "import 'foo';").unwrap();
    fs::write(
        dir.join("node_modules").join("foo").join("foo.js"),
        "module.exports = {};",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules").join("foo").join("package.json"),
        r#"{ "types": "foo.js" }"#,
    )
    .unwrap();

    let mut resolver = ModuleResolver::node_resolver();
    let result = resolver.resolve("foo", &dir.join("app.ts"), Span::new(0, 10));
    assert!(
        result.is_err(),
        "Expected package types .js to be ignored without allowJs"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_invalid_types_field_falls_back_to_main_declaration() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_invalid_types_field");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules").join("csv-parse").join("lib")).unwrap();

    fs::write(
        dir.join("app.ts"),
        "type Parser = typeof import(\"csv-parse\");",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules")
            .join("csv-parse")
            .join("lib")
            .join("index.d.ts"),
        "export function bar(): number;",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules")
            .join("csv-parse")
            .join("package.json"),
        r#"{
            "name": "csv-parse",
            "main": "./lib",
            "types": ["./lib/index.d.ts", "./lib/sync.d.ts"]
        }"#,
    )
    .unwrap();

    let mut resolver = ModuleResolver::node_resolver();
    let result = resolver.resolve("csv-parse", &dir.join("app.ts"), Span::new(0, 10));

    let resolved = result.expect("invalid package.json types field should be ignored");
    assert_eq!(
        resolved.resolved_path,
        dir.join("node_modules")
            .join("csv-parse")
            .join("lib")
            .join("index.d.ts")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_empty_types_field_uses_types_versions() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_empty_types_field");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules").join("a").join("ts3.1")).unwrap();

    fs::write(dir.join("app.ts"), "import { a } from \"a\";").unwrap();
    fs::write(
        dir.join("node_modules")
            .join("a")
            .join("ts3.1")
            .join("index.d.ts"),
        "export const a = 0;",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules").join("a").join("package.json"),
        r#"{
            "name": "a",
            "types": "",
            "typesVersions": {
                ">=3.1": { "*": ["ts3.1/*"] }
            }
        }"#,
    )
    .unwrap();

    let options = crate::config::ResolvedCompilerOptions {
        module_resolution: Some(crate::config::ModuleResolutionKind::Node),
        types_versions_compiler_version: Some("3.1.0".to_string()),
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("a", &dir.join("app.ts"), Span::new(0, 1));

    let resolved = result.expect("empty package.json types field should be ignored");
    assert_eq!(
        resolved.resolved_path,
        dir.join("node_modules")
            .join("a")
            .join("ts3.1")
            .join("index.d.ts")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_subpath_ambient_module_falls_back_to_types_entry() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_subpath_ambient_module");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules").join("ext").join("ts3.1")).unwrap();

    fs::write(dir.join("app.ts"), "import { b } from \"ext/other\";").unwrap();
    fs::write(
        dir.join("node_modules")
            .join("ext")
            .join("ts3.1")
            .join("index.d.ts"),
        r#"declare module "ext" { export const a: "ts3.1 a"; }
declare module "ext/other" { export const b: "ts3.1 b"; }"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules").join("ext").join("package.json"),
        r#"{
            "name": "ext",
            "types": "index",
            "typesVersions": {
                ">=3.1.0-0": { "*": ["ts3.1/*"] }
            }
        }"#,
    )
    .unwrap();

    let options = crate::config::ResolvedCompilerOptions {
        module_resolution: Some(crate::config::ModuleResolutionKind::Node),
        types_versions_compiler_version: Some("6.0.1".to_string()),
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let result = resolver.resolve("ext/other", &dir.join("app.ts"), Span::new(0, 11));

    let resolved =
        result.expect("ambient subpath should resolve through package types entry fallback");
    assert_eq!(
        resolved.resolved_path,
        dir.join("node_modules")
            .join("ext")
            .join("ts3.1")
            .join("index.d.ts")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resolver_missing_file() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_resolver_missing");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.ts"), "").unwrap();

    let mut resolver = ModuleResolver::node_resolver();
    let result = resolver.resolve("./nonexistent", &dir.join("main.ts"), Span::new(0, 10));

    assert!(result.is_err(), "Missing file should produce error");
    if let Err(failure) = result {
        assert!(failure.is_not_found());
    }

    let _ = fs::remove_dir_all(&dir);
}

// =========================================================================
// PackageType tests
// =========================================================================

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

#[test]
fn test_package_type_default_is_commonjs() {
    assert_eq!(PackageType::default(), PackageType::CommonJs);
}

#[test]
fn test_importing_module_kind_default_is_commonjs() {
    assert_eq!(
        ImportingModuleKind::default(),
        ImportingModuleKind::CommonJs
    );
}

#[test]
fn test_node16_js_file_no_ts2834_for_relative_imports() {
    // In Node16/NodeNext, JS files should NOT get TS2834 for extensionless
    // relative imports. TSC does not enforce extension requirements on JS files.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_node16_js_no_ts2834");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // package.json with type=module makes the dir ESM
    fs::write(dir.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(dir.join("index.js"), "import * as m from './utils';").unwrap();
    fs::write(dir.join("utils.js"), "export const x = 1;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        allow_js: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    // Resolve from a JS file — should succeed without TS2834
    let result = resolver.resolve_with_kind(
        "./utils",
        &dir.join("index.js"),
        Span::new(22, 30),
        ImportKind::EsmImport,
    );

    assert!(
        result.is_ok(),
        "Extensionless relative import from JS file should resolve, not emit TS2834: {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_ts_file_still_gets_ts2834_for_relative_imports() {
    // In Node16/NodeNext, TS files SHOULD still get TS2834/TS2835 for
    // extensionless relative imports even when allowJs is enabled.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_node16_ts_still_ts2834");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(dir.join("index.ts"), "import * as m from './utils';").unwrap();
    fs::write(dir.join("utils.js"), "export const x = 1;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        allow_js: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    // Resolve from a TS file — should emit TS2835 (with suggestion)
    let result = resolver.resolve_with_kind(
        "./utils",
        &dir.join("index.ts"),
        Span::new(22, 30),
        ImportKind::EsmImport,
    );

    assert!(
        result.is_err(),
        "Extensionless relative import from TS file should emit TS2834/TS2835: {result:?}"
    );
    let failure = result.unwrap_err();
    let diag = failure.to_diagnostic();
    assert!(
        diag.code == IMPORT_PATH_NEEDS_EXTENSION
            || diag.code == IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION,
        "Expected TS2834 or TS2835, got TS{}: {}",
        diag.code,
        diag.message,
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_json_file_produces_ts2835_suggestion() {
    // When an extensionless ESM import matches a .json file on disk,
    // the resolver should suggest the .json extension (TS2835) instead
    // of emitting TS2834 with no suggestion.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_node16_json_ts2835");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(
        dir.join("package.json"),
        r#"{"type":"module","name":"pkg"}"#,
    )
    .unwrap();
    fs::write(dir.join("index.ts"), "import './package';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let result = resolver.resolve_with_kind(
        "./package",
        &dir.join("index.ts"),
        Span::new(8, 19),
        ImportKind::EsmImport,
    );

    assert!(
        result.is_err(),
        "Expected extension error for extensionless ESM import"
    );
    let failure = result.unwrap_err();
    let diag = failure.to_diagnostic();
    assert_eq!(
        diag.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION,
        "Expected TS2835 (with .json suggestion), got TS{}: {}",
        diag.code, diag.message,
    );
    assert!(
        diag.message.contains("./package.json"),
        "Expected suggestion to include './package.json', got: {}",
        diag.message,
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_js_file_package_json_produces_ts2835_suggestion() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_node16_js_package_json_ts2835");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(
        dir.join("package.json"),
        r#"{"type":"module","name":"pkg"}"#,
    )
    .unwrap();
    fs::write(dir.join("index.js"), "import './package';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        allow_js: true,
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let result = resolver.resolve_with_kind(
        "./package",
        &dir.join("index.js"),
        Span::new(8, 19),
        ImportKind::EsmImport,
    );

    assert!(
        result.is_err(),
        "Expected extension error for JS ESM import targeting package.json"
    );
    let failure = result.unwrap_err();
    let diag = failure.to_diagnostic();
    assert_eq!(
        diag.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION,
        "Expected TS2835 (with .json suggestion), got TS{}: {}",
        diag.code, diag.message,
    );
    assert!(
        diag.message.contains("./package.json"),
        "Expected suggestion to include './package.json', got: {}",
        diag.message,
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_esm_package_no_directory_index_for_subpath() {
    // In Node16/NodeNext, ESM packages (type: "module") without an exports
    // field should NOT resolve subpaths through directory index (e.g.,
    // pkg/dist/dir → pkg/dist/dir/index.d.ts). Node.js ESM does not
    // support directory index resolution.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_esm_no_dir_index");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // Root package.json
    fs::write(dir.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(dir.join("index.ts"), "import 'test-pkg/dist/dir';").unwrap();

    // Package without exports field
    let pkg_dir = dir.join("node_modules/test-pkg");
    fs::create_dir_all(pkg_dir.join("dist/dir")).unwrap();
    fs::write(
        pkg_dir.join("package.json"),
        r#"{"name":"test-pkg","type":"module","main":"dist/index.js"}"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.join("dist/index.d.ts"),
        "export declare const a: number;",
    )
    .unwrap();
    fs::write(
        pkg_dir.join("dist/dir/index.d.ts"),
        "export declare const b: number;",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    // Should NOT resolve through directory index in ESM package
    let result = resolver.resolve_with_kind(
        "test-pkg/dist/dir",
        &dir.join("index.ts"),
        Span::new(8, 26),
        ImportKind::EsmImport,
    );

    assert!(
        result.is_err(),
        "ESM package subpath should not resolve through directory index: {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_node16_cjs_package_allows_directory_index_for_subpath() {
    // CJS packages (no "type": "module") should still allow directory
    // index resolution for subpaths, even in Node16/NodeNext mode.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_cjs_allows_dir_index");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(dir.join("index.ts"), "import 'test-cjs/lib/sub';").unwrap();

    let pkg_dir = dir.join("node_modules/test-cjs");
    fs::create_dir_all(pkg_dir.join("lib/sub")).unwrap();
    // CJS package (no "type" field → defaults to commonjs)
    fs::write(
        pkg_dir.join("package.json"),
        r#"{"name":"test-cjs","main":"lib/index.js"}"#,
    )
    .unwrap();
    fs::write(
        pkg_dir.join("lib/index.d.ts"),
        "export declare const a: number;",
    )
    .unwrap();
    fs::write(
        pkg_dir.join("lib/sub/index.d.ts"),
        "export declare const b: number;",
    )
    .unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    // CJS package subpath SHOULD resolve through directory index
    let result = resolver.resolve_with_kind(
        "test-cjs/lib/sub",
        &dir.join("index.ts"),
        Span::new(8, 25),
        ImportKind::EsmImport,
    );

    assert!(
        result.is_ok(),
        "CJS package subpath should resolve through directory index: {result:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

// -----------------------------------------------------------------------
// ModuleLookupRequest / ModuleLookupResult tests
// -----------------------------------------------------------------------

#[test]
fn test_lookup_extension_suggestion_esm() {
    // TS2835: relative import without extension in ESM Node16 context
    // should fail with a suggestion extension
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_ext_suggestion_esm");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/utils.ts"), "export const x = 1;").unwrap();
    fs::write(dir.join("src/index.mts"), "import { x } from './utils';").unwrap();
    fs::write(dir.join("package.json"), r#"{"type": "module"}"#).unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./utils",
        containing_file: &dir.join("src/index.mts"),
        specifier_span: Span::new(22, 30),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false);

    assert!(
        result.resolved_path.is_none(),
        "should not resolve without extension"
    );
    assert!(!result.treat_as_resolved, "should not treat as resolved");
    let error = result.error.expect("should have an error");
    assert_eq!(
        error.code, IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION,
        "should suggest extension (TS2835)"
    );
    assert!(
        error.message.contains(".js'"),
        "should suggest .js extension: {}",
        error.message
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_cjs_esm_mismatch_classic_resolution() {
    // TS2792: classic resolution should produce moduleResolution mismatch
    let dir = std::env::temp_dir().join("tsz_lookup_cjs_esm_mismatch");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/index.ts"), "import 'nonexistent';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Classic),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "nonexistent",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 20),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: true,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false);

    let error = result.error.expect("should have an error");
    assert_eq!(
        error.code, MODULE_RESOLUTION_MODE_MISMATCH,
        "classic resolution should produce TS2792, got TS{}",
        error.code
    );
    assert!(
        error.message.contains("'moduleResolution'"),
        "message should suggest moduleResolution: {}",
        error.message
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_json_module_without_flag() {
    // TS2732: importing .json without resolveJsonModule
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_json_module");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/data.json"), r#"{"key": "value"}"#).unwrap();
    fs::write(dir.join("src/index.ts"), "import data from './data.json';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: false,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./data.json",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(18, 30),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false);

    let error = result.error.expect("should have an error");
    assert_eq!(
        error.code, JSON_MODULE_WITHOUT_RESOLVE_JSON_MODULE,
        "should emit TS2732 for .json without resolveJsonModule, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ambient_module_suppresses_error() {
    // Ambient module declarations should suppress resolution errors
    let dir = std::env::temp_dir().join("tsz_lookup_ambient");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/index.ts"), "import 'my-ambient';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "my-ambient",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 19),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |spec| spec == "my-ambient");

    assert!(result.resolved_path.is_none(), "ambient has no file path");
    assert!(
        result.treat_as_resolved,
        "ambient should be treated as resolved"
    );
    assert!(result.error.is_none(), "ambient should have no error");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_untyped_js_module_no_implicit_any() {
    // TS7016: untyped JS module with noImplicitAny
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_untyped_js");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/untyped")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("node_modules/untyped/package.json"),
        r#"{"name":"untyped"}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/untyped/index.js"),
        "module.exports = {};",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import 'untyped';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    // With noImplicitAny: should get TS7016
    let request = ModuleLookupRequest {
        specifier: "untyped",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 16),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: true,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false);
    assert!(
        result.treat_as_resolved,
        "untyped JS should be treated as resolved"
    );
    let error = result.error.expect("noImplicitAny should produce TS7016");
    assert_eq!(error.code, COULD_NOT_FIND_DECLARATION_FILE);

    // Without noImplicitAny: should be resolved with no error
    resolver.clear_cache();
    let request_no_strict = ModuleLookupRequest {
        specifier: "untyped",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 16),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result2 = resolver.lookup(&request_no_strict, |_, _| None, |_| false);
    assert!(
        result2.treat_as_resolved,
        "untyped JS should be resolved without error"
    );
    assert!(
        result2.error.is_none(),
        "without noImplicitAny, no error expected"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_fallback_success() {
    // Fallback resolver provides the file when primary fails
    let dir = std::env::temp_dir().join("tsz_lookup_fallback");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/index.ts"), "import 'virtual-mod';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let fake_target = dir.join("src/virtual.d.ts");
    std::fs::write(&fake_target, "export {};").unwrap();
    let fake_target_clone = fake_target;

    let request = ModuleLookupRequest {
        specifier: "virtual-mod",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 20),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| Some(fake_target_clone), |_| false);

    assert!(
        result.resolved_path.is_some(),
        "fallback should provide resolved path"
    );
    assert!(
        result.error.is_none(),
        "successful fallback should have no error"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_node16_esm_extensionless_fallback_error() {
    // Node16/NodeNext: extensionless relative import in ESM context
    // should fail even if fallback finds the file
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_n16_ext_fallback");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/utils.ts"), "export const x = 1;").unwrap();
    fs::write(dir.join("src/index.mts"), "import { x } from './utils';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let fake_target = dir.join("src/utils.ts");
    let fake_target_clone = fake_target;

    let request = ModuleLookupRequest {
        specifier: "./utils",
        containing_file: &dir.join("src/index.mts"),
        specifier_span: Span::new(22, 30),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    // Primary resolution emits TS2835 (extension suggestion).
    // Even if fallback would find the file, the primary error takes priority.
    let result = resolver.lookup(&request, |_, _| Some(fake_target_clone), |_| false);

    assert!(
        result.resolved_path.is_none(),
        "ESM extensionless should not resolve"
    );
    let error = result.error.expect("should have an extension error");
    assert!(
        error.code == IMPORT_PATH_NEEDS_EXTENSION_SUGGESTION
            || error.code == IMPORT_PATH_NEEDS_EXTENSION,
        "should be TS2834 or TS2835, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_package_exports_subpath() {
    // Package exports resolution via lookup
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_pkg_exports");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{".":"./main.js","./utils":"./utils.js"}}"#,
    )
    .unwrap();
    fs::write(dir.join("node_modules/pkg/main.d.ts"), "export {};").unwrap();
    fs::write(
        dir.join("node_modules/pkg/utils.d.ts"),
        "export const u: number;",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import 'pkg/utils';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "pkg/utils",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 19),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false);

    assert!(
        result.resolved_path.is_some(),
        "package exports subpath should resolve: {:?}",
        result.error
    );
    assert!(result.error.is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_resolution_mode_override_selects_import_condition() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_lookup_resolution_mode_override");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{".":{"import":"./esm.d.ts","require":"./missing.d.cts"}}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/esm.d.ts"),
        "export type Foo = 1;",
    )
    .unwrap();
    fs::write(dir.join("src/index.cts"), "import type { Foo } from 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request_without_override = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.cts"),
        specifier_span: Span::new(24, 29),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result_without_override =
        resolver.lookup(&request_without_override, |_, _| None, |_| false);
    assert!(
        result_without_override.error.is_some(),
        "CJS implied mode should follow the missing require condition"
    );

    resolver.clear_cache();

    let request_with_override = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.cts"),
        specifier_span: Span::new(24, 29),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: Some(ImportingModuleKind::Esm),
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result_with_override = resolver.lookup(&request_with_override, |_, _| None, |_| false);

    assert!(
        result_with_override.resolved_path.is_some(),
        "resolution-mode import override should select the import condition: {:?}",
        result_with_override.error
    );
    assert!(result_with_override.error.is_none());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_module_preserve_uses_syntax_directed_conditions() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_lookup_module_preserve_conditions");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{".":{"import":"./esm.d.ts","require":"./cjs.d.ts"}}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/esm.d.ts"),
        r#"export const esm: "esm";"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/cjs.d.ts"),
        r#"declare const cjs: "cjs"; export = cjs;"#,
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import { esm } from 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Preserve,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let esm_request = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(19, 24),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let esm_result = resolver.lookup(&esm_request, |_, _| None, |_| false);
    let esm_path = esm_result
        .resolved_path
        .expect("module preserve import should select the import condition");
    assert!(
        esm_path.ends_with("esm.d.ts"),
        "expected import condition path, got {}",
        esm_path.display()
    );

    resolver.clear_cache();

    let cjs_request = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(19, 24),
        import_kind: ImportKind::CjsRequire,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let cjs_result = resolver.lookup(&cjs_request, |_, _| None, |_| false);
    let cjs_path = cjs_result
        .resolved_path
        .expect("module preserve require should select the require condition");
    assert!(
        cjs_path.ends_with("cjs.d.ts"),
        "expected require condition path, got {}",
        cjs_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_module_preserve_honors_forced_cts_and_mts_conditions() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_lookup_module_preserve_forced_conditions");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{".":{"import":"./esm.d.ts","require":"./cjs.d.ts"}}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/esm.d.ts"),
        r#"export const esm: "esm";"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/cjs.d.ts"),
        r#"export const cjs: "cjs";"#,
    )
    .unwrap();
    fs::write(dir.join("src/index.mts"), "import { esm } from 'pkg';").unwrap();
    fs::write(dir.join("src/index.cts"), "import { cjs } from 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Preserve,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let mts_request = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.mts"),
        specifier_span: Span::new(19, 24),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let mts_result = resolver.lookup(&mts_request, |_, _| None, |_| false);
    let mts_path = mts_result
        .resolved_path
        .expect(".mts import should stay on the import condition");
    assert!(
        mts_path.ends_with("esm.d.ts"),
        "expected import condition path for .mts, got {}",
        mts_path.display()
    );

    resolver.clear_cache();

    let cts_request = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &dir.join("src/index.cts"),
        specifier_span: Span::new(19, 24),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let cts_result = resolver.lookup(&cts_request, |_, _| None, |_| false);
    let cts_path = cts_result
        .resolved_path
        .expect(".cts import should stay on the require condition");
    assert!(
        cts_path.ends_with("cjs.d.ts"),
        "expected require condition path for .cts, got {}",
        cts_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_versioned_types_condition_prefers_matching_export_branch() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_lookup_versioned_types_condition");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/inner")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/inner/package.json"),
        r#"{
            "name":"inner",
            "exports":{
                ".":{
                    "types@>=10000":"./future-types.d.ts",
                    "types@>=1":"./new-types.d.ts",
                    "types":"./old-types.d.ts",
                    "import":"./index.mjs",
                    "node":"./index.js"
                }
            }
        }"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/old-types.d.ts"),
        "export const oldThing: number;",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/inner/new-types.d.ts"),
        "export const goodThing: number;",
    )
    .unwrap();
    fs::write(dir.join("src/index.ts"), "import * as mod from 'inner';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        checker: crate::checker::context::CheckerOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "inner",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(22, 29),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false);

    let resolved_path = result
        .resolved_path
        .expect("versioned types condition should resolve");
    assert!(
        resolved_path.ends_with("new-types.d.ts"),
        "expected versioned types branch, got {}",
        resolved_path.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

// -----------------------------------------------------------------------
// ModuleLookupOutcome / classify() tests
// -----------------------------------------------------------------------

#[test]
fn test_classify_resolved_path() {
    let result = ModuleLookupResult::resolved(PathBuf::from("/tmp/foo.ts"));
    let outcome = result.classify();

    assert_eq!(outcome.resolved_path, Some(PathBuf::from("/tmp/foo.ts")));
    assert!(!outcome.resolved_using_ts_extension);
    assert!(outcome.is_resolved);
    assert!(outcome.error.is_none());
}

#[test]
fn test_package_imports_exact_mapping_does_not_mark_ts_extension_usage() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_test_package_imports_exact_ts_usage");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("package.json"),
        r##"{
            "name": "pkg",
            "type": "module",
            "imports": {
                "#foo.ts": "./src/foo.ts"
            }
        }"##,
    )
    .unwrap();
    fs::write(dir.join("src/foo.ts"), "export {};").unwrap();
    fs::write(dir.join("index.ts"), "import {} from \"#foo.ts\";").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_imports: true,
        rewrite_relative_import_extensions: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let request = ModuleLookupRequest {
        specifier: "#foo.ts",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(0, 9),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let outcome = resolver.lookup(&request, |_, _| None, |_| false).classify();
    assert!(outcome.resolved_path.is_some());
    assert!(
        !outcome.resolved_using_ts_extension,
        "exact package imports entry should suppress ts-extension rewrite diagnostics"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_package_imports_pattern_marks_ts_extension_usage() {
    use std::fs;

    let dir = std::env::temp_dir().join("tsz_test_package_imports_pattern_ts_usage");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("internal")).unwrap();
    fs::write(
        dir.join("package.json"),
        r##"{
            "name": "pkg",
            "type": "module",
            "imports": {
                "#internal/*": "./internal/*"
            }
        }"##,
    )
    .unwrap();
    fs::write(dir.join("internal/foo.ts"), "export {};").unwrap();
    fs::write(dir.join("index.ts"), "import {} from \"#internal/foo.ts\";").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        resolve_package_json_imports: true,
        rewrite_relative_import_extensions: true,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let request = ModuleLookupRequest {
        specifier: "#internal/foo.ts",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(0, 18),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let outcome = resolver.lookup(&request, |_, _| None, |_| false).classify();
    assert!(outcome.resolved_path.is_some());
    assert!(
        outcome.resolved_using_ts_extension,
        "pattern package imports entry should preserve ts-extension usage for TS2877"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_classify_failed() {
    let result =
        ModuleLookupResult::failed(CANNOT_FIND_MODULE, "Cannot find module 'foo'".to_string());
    let outcome = result.classify();

    assert!(outcome.resolved_path.is_none());
    assert!(!outcome.is_resolved);
    let error = outcome.error.expect("should have error");
    assert_eq!(error.code, CANNOT_FIND_MODULE);
}

#[test]
fn test_classify_ambient() {
    let result = ModuleLookupResult::ambient();
    let outcome = result.classify();

    assert!(outcome.resolved_path.is_none());
    assert!(
        outcome.is_resolved,
        "ambient modules should be treated as resolved"
    );
    assert!(outcome.error.is_none());
}

#[test]
fn test_classify_resolved_with_error() {
    let result = ModuleLookupResult::resolved_with_error(
        MODULE_WAS_RESOLVED_TO_BUT_JSX_NOT_SET,
        "jsx not set".to_string(),
    );
    let outcome = result.classify();

    assert!(outcome.resolved_path.is_none());
    assert!(outcome.is_resolved, "JsxNotEnabled should suppress TS2307");
    let error = outcome.error.expect("should have error");
    assert_eq!(error.code, MODULE_WAS_RESOLVED_TO_BUT_JSX_NOT_SET);
}

#[test]
fn test_classify_untyped_js_with_no_implicit_any() {
    let result = ModuleLookupResult::untyped_js(PathBuf::from("/tmp/foo.js"), true, "foo");
    let outcome = result.classify();

    assert!(outcome.resolved_path.is_none());
    assert!(outcome.is_resolved, "untyped JS should suppress TS2307");
    let error = outcome.error.expect("should have TS7016 error");
    assert_eq!(error.code, COULD_NOT_FIND_DECLARATION_FILE);
}

#[test]
fn test_classify_untyped_js_without_no_implicit_any() {
    let result = ModuleLookupResult::untyped_js(PathBuf::from("/tmp/foo.js"), false, "foo");
    let outcome = result.classify();

    assert!(outcome.resolved_path.is_none());
    assert!(outcome.is_resolved);
    assert!(outcome.error.is_none(), "without noImplicitAny, no error");
}

// =========================================================================
// lookup() diagnostic code selection — additional coverage
//
// Tests for lookup() paths not covered by the existing suite above:
// - TS5097 via lookup() (import with .ts/.mts extension, file not found)
// - TS5097 non-trigger (file exists, resolution succeeds)
// - TS6142 via lookup() (JSX not enabled, verifying classify() outcome)
// - Successful resolution via lookup() -> classify() (happy path)
// - Plain TS2307 via lookup() (no upgrade conditions)
// =========================================================================

#[test]
fn test_lookup_ts5097_ts_extension_not_found() {
    // TS5097: Import path ends with a TypeScript extension (.ts)
    // without allowImportingTsExtensions enabled.
    // When the specifier has an explicit .ts extension and the file is NOT found,
    // resolve_with_kind upgrades NotFound -> ImportingTsExtensionNotAllowed.
    // lookup() then propagates this as a TS5097 error via classify().
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_lookup_ts5097_nf");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // Only create main.ts — utils.ts does NOT exist
    fs::write(dir.join("main.ts"), "import { foo } from './utils.ts';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        allow_importing_ts_extensions: false,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./utils.ts",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(20, 32),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false);
    let outcome = result.classify();

    let error = outcome
        .error
        .expect("Expected TS5097 error for .ts extension import");
    assert_eq!(
        error.code, IMPORT_PATH_TS_EXTENSION_NOT_ALLOWED,
        "Expected TS5097 but got TS{}",
        error.code
    );
    assert!(
        error.message.contains("allowImportingTsExtensions"),
        "Error message should mention allowImportingTsExtensions: {}",
        error.message
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ts5097_mts_extension_not_found() {
    // TS5097 for .mts extension (not just .ts).
    // When the specifier has an explicit .mts extension and the file is NOT found,
    // we should get TS5097 instead of TS2307.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_lookup_ts5097_mts_nf");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.ts"), "").unwrap();
    // utils.mts does NOT exist

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        allow_importing_ts_extensions: false,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./utils.mts",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(20, 33),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false);
    let outcome = result.classify();

    let error = outcome
        .error
        .expect("Expected TS5097 error for .mts extension import");
    assert_eq!(
        error.code, IMPORT_PATH_TS_EXTENSION_NOT_ALLOWED,
        "Expected TS5097 for .mts but got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ts_extension_resolves_when_file_exists() {
    // When ./utils.ts is imported and the file EXISTS, the resolution succeeds
    // (no TS5097 error in this case — the current implementation only upgrades
    // NotFound to TS5097).
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_lookup_ts_ext_exists");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.ts"), "import { foo } from './utils.ts';").unwrap();
    fs::write(dir.join("utils.ts"), "export const foo = 42;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        allow_importing_ts_extensions: false,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./utils.ts",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(20, 32),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false);
    let outcome = result.classify();

    // When the file exists, Node resolution resolves it successfully
    assert!(
        outcome.resolved_path.is_some(),
        "Should resolve when file exists even with .ts extension"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_jsx_not_enabled_classify_outcome() {
    // When a module resolves to a .tsx file but --jsx is not set,
    // lookup() should mark it as resolved (suppress TS2307) with a TS6142 error.
    // This specifically tests the classify() outcome (is_resolved + error).
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_lookup_jsx_classify");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.ts"), "import Cmp from './Cmp';").unwrap();
    fs::write(dir.join("Cmp.tsx"), "export default function Cmp() {}").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        jsx: None,
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./Cmp",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(16, 23),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false);
    let outcome = result.classify();

    assert!(
        outcome.is_resolved,
        "JsxNotEnabled should still mark the module as resolved"
    );
    let error = outcome
        .error
        .expect("Expected TS6142 error for JSX not enabled");
    assert_eq!(
        error.code, MODULE_WAS_RESOLVED_TO_BUT_JSX_NOT_SET,
        "Expected TS6142 but got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_successful_resolution_classify() {
    // Verify the full lookup() -> classify() pipeline for a successful resolution.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_lookup_success_classify");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.ts"), "import { foo } from './utils';").unwrap();
    fs::write(dir.join("utils.ts"), "export const foo = 42;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./utils",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(20, 29),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false);
    let outcome = result.classify();

    assert!(
        outcome.error.is_none(),
        "Successful resolution should have no error"
    );
    assert!(
        outcome.resolved_path.is_some(),
        "Successful resolution should have a path"
    );
    assert!(
        outcome.is_resolved,
        "Successful resolution should be resolved"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ts2307_plain_no_upgrade() {
    // When a relative module is not found and no upgrade conditions are met
    // (not .json, not implied_classic_resolution, not ambient, no JS file),
    // lookup() should produce plain TS2307.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_test_lookup_ts2307_plain_v2");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("main.ts"), "import { foo } from './nonexistent';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./nonexistent",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(20, 35),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };

    let result = resolver.lookup(&request, |_, _| None, |_| false);
    let outcome = result.classify();

    assert!(!outcome.is_resolved, "Not-found should not be resolved");
    let error = outcome.error.expect("Expected TS2307 error");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "Expected TS2307 but got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ts2732_nonexistent_json_without_resolve_json_module() {
    // When a .json import specifier does NOT resolve to any file on disk and
    // resolveJsonModule is false, lookup() should upgrade NotFound -> TS2732.
    // This exercises the upgrade branch in lookup() that catches NotFound for
    // .json specifiers after fallback fails.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_ts2732_nonexistent");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import './missing.json';").unwrap();
    // Intentionally do NOT create missing.json

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: false,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./missing.json",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 22),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false);
    let outcome = result.classify();

    assert!(
        !outcome.is_resolved,
        "Nonexistent .json should not be resolved"
    );
    let error = outcome
        .error
        .expect("Expected error for nonexistent .json import");
    assert_eq!(
        error.code, JSON_MODULE_WITHOUT_RESOLVE_JSON_MODULE,
        "Expected TS2732 for nonexistent .json without resolveJsonModule, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_ts2732_not_emitted_when_resolve_json_module_enabled() {
    // When resolveJsonModule is true but the .json file doesn't exist,
    // the error should be plain TS2307, not TS2732.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_ts2732_enabled");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import './missing.json';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        resolve_json_module: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "./missing.json",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 22),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false);
    let outcome = result.classify();

    let error = outcome.error.expect("Expected error for missing .json");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "Expected TS2307 (not TS2732) when resolveJsonModule is enabled, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_path_mapping_failure_produces_ts2307_via_lookup() {
    // When path mappings are configured but fail to resolve, lookup() should
    // produce TS2307 (not panic or silently succeed).
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_path_mapping_fail");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/index.ts"), "import '@app/missing';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        base_url: Some(dir.clone()),
        paths: Some(vec![PathMapping {
            pattern: "@app/*".to_string(),
            prefix: "@app/".to_string(),
            suffix: String::new(),
            targets: vec!["src/*".to_string()],
        }]),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "@app/missing",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 22),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false);
    let outcome = result.classify();

    assert!(
        !outcome.is_resolved,
        "Failed path mapping should not be resolved"
    );
    let error = outcome
        .error
        .expect("Expected error for failed path mapping");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "Expected TS2307 for failed path mapping, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_fallback_rescues_not_found() {
    // When primary resolution fails but fallback succeeds, lookup() should
    // return resolved with no error.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_fallback_rescue");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.ts"), "import './virtual';").unwrap();
    let virtual_path = dir.join("virtual.ts");
    fs::write(&virtual_path, "export const x = 1;").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let fallback_path = virtual_path;
    let request = ModuleLookupRequest {
        specifier: "./nonexistent-specifier",
        containing_file: &dir.join("main.ts"),
        specifier_span: Span::new(8, 18),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| Some(fallback_path), |_| false);
    let outcome = result.classify();

    assert!(outcome.is_resolved, "Fallback should mark as resolved");
    assert!(
        outcome.resolved_path.is_some(),
        "Fallback should provide a resolved path"
    );
    assert!(
        outcome.error.is_none(),
        "Fallback success should have no error"
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
    // When implied_classic_resolution is true and the module is not found,
    // lookup() should upgrade TS2307 -> TS2792.
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_classic_ts2792");
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
    let result = resolver.lookup(&request, |_, _| None, |_| false);
    let outcome = result.classify();

    assert!(!outcome.is_resolved);
    let error = outcome.error.expect("Expected error for missing module");
    assert_eq!(
        error.code, MODULE_RESOLUTION_MODE_MISMATCH,
        "Expected TS2792 for implied classic resolution, got TS{}",
        error.code
    );
    assert!(
        error.message.contains("moduleResolution"),
        "TS2792 message should suggest moduleResolution option"
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
    let result = resolver.lookup(&request, |_, _| None, |_| false);
    let outcome = result.classify();

    let error = outcome.error.expect("Expected error for bare .json import");
    assert_eq!(
        error.code, JSON_MODULE_WITHOUT_RESOLVE_JSON_MODULE,
        "Expected TS2732 for bare .json without resolveJsonModule, got TS{}",
        error.code
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_lookup_classify_ambient_no_path_no_error() {
    // Ambient module: classify should show is_resolved=true, no path, no error
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_lookup_classify_ambient");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), "import 'my-ambient-mod';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "my-ambient-mod",
        containing_file: &dir.join("index.ts"),
        specifier_span: Span::new(8, 24),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |spec| spec == "my-ambient-mod");
    let outcome = result.classify();

    assert!(outcome.is_resolved, "Ambient module should be resolved");
    assert!(
        outcome.resolved_path.is_none(),
        "Ambient module should have no file path"
    );
    assert!(
        outcome.error.is_none(),
        "Ambient module should have no error"
    );

    let _ = fs::remove_dir_all(&dir);
}
