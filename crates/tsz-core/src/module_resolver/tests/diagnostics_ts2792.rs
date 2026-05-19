//! Diagnostics Ts2792 tests for `module_resolver`.
//!
//! Tests for **TS2792 (Module resolution mode mismatch)** and the
//! shared error-code helpers used across the diagnostic family.

use super::super::*;

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
