//! Resolution Failure tests for `module_resolver`.
//!
//! Tests for `ResolutionFailure` data: `is_not_found`, accessors,
//! and the `to_diagnostic` mapping to TS2307/TS2792/TS2835.

use super::super::*;

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
