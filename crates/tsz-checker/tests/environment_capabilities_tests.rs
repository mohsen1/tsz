//! Environment capabilities boundary regression tests.
//!
//! These tests verify that the EnvironmentCapabilities model correctly
//! routes diagnostics for:
//! - TS2318: Missing global types (lib availability)
//! - TS2591: Node.js globals (known-global classification)
//! - TS2583: ES2015+ type suggestions (known-global classification)
//! - TS2584: DOM globals (known-global classification)
//! - TS2823: Import attributes module option check (feature gate)
//! - Feature gate queries (import attributes, using, etc.)

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper: check source without lib files and with given options.
fn check_with_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<crate::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

/// Helper: check source without lib files.
fn check_no_lib(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_with_options(source, CheckerOptions::default())
}

// =============================================================================
// TS2591: Node.js globals routed through capabilities
// =============================================================================

#[test]
fn test_node_global_require_emits_ts2591() {
    let diags = check_no_lib("const x = require('fs');");
    let ts2591: Vec<_> = diags.iter().filter(|d| d.code == 2591).collect();
    assert!(
        !ts2591.is_empty(),
        "Expected TS2591 for 'require' (Node global), got: {diags:?}"
    );
}

#[test]
fn test_node_global_process_classified_correctly() {
    // Verify the capability boundary classifies 'process' as a Node global.
    // Full checker integration (TS2591 emission) depends on the identifier reaching
    // the name resolution error path, which requires the identifier to be used
    // in a value expression context that doesn't short-circuit.
    use crate::query_boundaries::capabilities::{EnvironmentCapabilities, MissingGlobalKind};
    let opts = CheckerOptions::default();
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(
        caps.classify_missing_global("process"),
        Some(MissingGlobalKind::NodeGlobal)
    );
    assert_eq!(
        caps.classify_missing_global("Buffer"),
        Some(MissingGlobalKind::NodeGlobal)
    );
    assert_eq!(
        caps.classify_missing_global("__filename"),
        Some(MissingGlobalKind::NodeGlobal)
    );
    assert_eq!(
        caps.classify_missing_global("__dirname"),
        Some(MissingGlobalKind::NodeGlobal)
    );
    assert_eq!(
        caps.classify_missing_global("exports"),
        Some(MissingGlobalKind::NodeGlobal)
    );
}

// =============================================================================
// TS2583: ES2015+ types routed through capabilities
// =============================================================================

#[test]
fn test_es2015_promise_emits_ts2583_via_capabilities() {
    let diags = check_no_lib("const p = new Promise<void>();");
    let ts2583: Vec<_> = diags.iter().filter(|d| d.code == 2583).collect();
    assert!(
        !ts2583.is_empty(),
        "Expected TS2583 for 'Promise' (ES2015+ type) via capabilities, got: {diags:?}"
    );
}

#[test]
fn test_es2015_map_emits_ts2583_via_capabilities() {
    let diags = check_no_lib("const m = new Map<string, number>();");
    let ts2583: Vec<_> = diags.iter().filter(|d| d.code == 2583).collect();
    assert!(
        !ts2583.is_empty(),
        "Expected TS2583 for 'Map' (ES2015+ type) via capabilities, got: {diags:?}"
    );
}

// =============================================================================
// TS2823: Import attributes module option (feature gate)
// =============================================================================

#[test]
fn test_import_attributes_emits_ts2823_with_commonjs() {
    let diags = check_with_options(
        r#"import data from './data.json' with { type: "json" };"#,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );
    let ts2823: Vec<_> = diags.iter().filter(|d| d.code == 2823).collect();
    assert!(
        !ts2823.is_empty(),
        "Expected TS2823 for import attributes with CommonJS module, got: {diags:?}"
    );
}

#[test]
fn test_import_attributes_no_ts2823_with_esnext() {
    let diags = check_with_options(
        r#"import data from './data.json' with { type: "json" };"#,
        CheckerOptions {
            module: ModuleKind::ESNext,
            ..CheckerOptions::default()
        },
    );
    let ts2823: Vec<_> = diags.iter().filter(|d| d.code == 2823).collect();
    assert!(
        ts2823.is_empty(),
        "Expected NO TS2823 for import attributes with ESNext module, got: {ts2823:?}"
    );
}

#[test]
fn test_import_attributes_no_ts2823_with_nodenext() {
    let diags = check_with_options(
        r#"import data from './data.json' with { type: "json" };"#,
        CheckerOptions {
            module: ModuleKind::NodeNext,
            ..CheckerOptions::default()
        },
    );
    let ts2823: Vec<_> = diags.iter().filter(|d| d.code == 2823).collect();
    assert!(
        ts2823.is_empty(),
        "Expected NO TS2823 for import attributes with NodeNext module, got: {ts2823:?}"
    );
}

#[test]
fn test_import_attributes_no_ts2823_with_preserve() {
    let diags = check_with_options(
        r#"import data from './data.json' with { type: "json" };"#,
        CheckerOptions {
            module: ModuleKind::Preserve,
            ..CheckerOptions::default()
        },
    );
    let ts2823: Vec<_> = diags.iter().filter(|d| d.code == 2823).collect();
    assert!(
        ts2823.is_empty(),
        "Expected NO TS2823 for import attributes with Preserve module, got: {ts2823:?}"
    );
}

// =============================================================================
// TS2318: Missing global types (capabilities.has_lib / no_lib)
// =============================================================================

#[test]
fn test_nolib_emits_ts2318_via_capabilities() {
    let diags = check_with_options(
        r#"
interface Array<T> {}
interface Boolean {}
interface Function {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}
declare function foo(): void;
"#,
        CheckerOptions {
            no_lib: true,
            ..CheckerOptions::default()
        },
    );
    let ts2318: Vec<_> = diags.iter().filter(|d| d.code == 2318).collect();
    assert!(
        !ts2318.is_empty(),
        "Expected TS2318 for missing CallableFunction/NewableFunction with --noLib, got: {diags:?}"
    );
}

// =============================================================================
// Capability matrix unit tests (EnvironmentCapabilities struct)
// =============================================================================

#[test]
fn test_capabilities_matrix_esnext_module() {
    use crate::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};

    let opts = CheckerOptions {
        module: ModuleKind::ESNext,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);

    assert!(
        caps.import_attributes_supported,
        "ESNext should support import attributes"
    );
    assert!(caps.feature_available(FeatureGate::ImportAttributes));
    assert!(
        caps.resolve_json_module_compatible,
        "ESNext should be compatible with resolveJsonModule"
    );
}

#[test]
fn test_capabilities_matrix_commonjs_module() {
    use crate::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};

    let opts = CheckerOptions {
        module: ModuleKind::CommonJS,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);

    assert!(
        !caps.import_attributes_supported,
        "CommonJS should NOT support import attributes"
    );
    assert!(!caps.feature_available(FeatureGate::ImportAttributes));
    assert!(
        caps.resolve_json_module_compatible,
        "CommonJS should be compatible with resolveJsonModule"
    );
}

#[test]
fn test_capabilities_matrix_none_module_no_json() {
    use crate::query_boundaries::capabilities::EnvironmentCapabilities;

    let opts = CheckerOptions {
        module: ModuleKind::None,
        resolve_json_module: true,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);

    assert!(
        !caps.resolve_json_module_compatible,
        "module=None should be incompatible with resolveJsonModule"
    );
}

#[test]
fn test_capabilities_classify_global_names() {
    use crate::query_boundaries::capabilities::{EnvironmentCapabilities, MissingGlobalKind};

    let opts = CheckerOptions::default();
    let caps = EnvironmentCapabilities::from_options(&opts, true);

    // Node globals
    assert_eq!(
        caps.classify_missing_global("require"),
        Some(MissingGlobalKind::NodeGlobal)
    );
    assert_eq!(
        caps.classify_missing_global("__dirname"),
        Some(MissingGlobalKind::NodeGlobal)
    );

    // DOM globals
    assert_eq!(
        caps.classify_missing_global("document"),
        Some(MissingGlobalKind::DomGlobal)
    );
    assert_eq!(
        caps.classify_missing_global("window"),
        Some(MissingGlobalKind::DomGlobal)
    );

    // ES2015+ types
    assert_eq!(
        caps.classify_missing_global("Promise"),
        Some(MissingGlobalKind::Es2015PlusType)
    );
    assert_eq!(
        caps.classify_missing_global("Map"),
        Some(MissingGlobalKind::Es2015PlusType)
    );
    assert_eq!(
        caps.classify_missing_global("WeakRef"),
        Some(MissingGlobalKind::Es2015PlusType)
    );

    // Unknown names
    assert_eq!(caps.classify_missing_global("myVar"), None);
    assert_eq!(caps.classify_missing_global("customFunc"), None);
}

#[test]
fn test_capabilities_has_lib_updates() {
    use crate::query_boundaries::capabilities::EnvironmentCapabilities;

    let opts = CheckerOptions::default();
    let mut caps = EnvironmentCapabilities::from_options(&opts, false);
    assert!(!caps.has_lib, "Initially should have no lib");

    caps.has_lib = true;
    assert!(caps.has_lib, "After setting has_lib, should be true");
}
