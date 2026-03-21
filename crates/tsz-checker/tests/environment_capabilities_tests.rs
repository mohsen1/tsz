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
use tsz_common::common::{ModuleKind, ScriptTarget};
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

// =============================================================================
// CapabilityDiagnostic boundary tests (environment.rs)
// =============================================================================

#[test]
fn test_capability_diagnostic_disposable_prerequisite() {
    use crate::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use crate::query_boundaries::environment::CapabilityDiagnostic;

    // Without lib, using requires Disposable
    let opts = CheckerOptions::default();
    let caps = EnvironmentCapabilities::from_options(&opts, false);
    let diag = caps.check_feature_gate(FeatureGate::UsingDeclaration);
    assert_eq!(
        diag,
        Some(CapabilityDiagnostic::FeatureRequiresGlobalType {
            gate: FeatureGate::UsingDeclaration,
            required_type: "Disposable",
        }),
        "using without lib should require Disposable"
    );

    // With lib, no diagnostic
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(
        caps.check_feature_gate(FeatureGate::UsingDeclaration),
        None,
        "using with lib should not produce a diagnostic"
    );
}

#[test]
fn test_capability_diagnostic_async_disposable_prerequisite() {
    use crate::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use crate::query_boundaries::environment::CapabilityDiagnostic;

    // Without lib, await using requires AsyncDisposable
    let opts = CheckerOptions::default();
    let caps = EnvironmentCapabilities::from_options(&opts, false);
    let diag = caps.check_feature_gate(FeatureGate::AwaitUsingDeclaration);
    assert_eq!(
        diag,
        Some(CapabilityDiagnostic::FeatureRequiresGlobalType {
            gate: FeatureGate::AwaitUsingDeclaration,
            required_type: "AsyncDisposable",
        }),
        "await using without lib should require AsyncDisposable"
    );
}

#[test]
fn test_capability_diagnostic_node_global_availability() {
    use crate::query_boundaries::capabilities::EnvironmentCapabilities;
    use crate::query_boundaries::environment::CapabilityDiagnostic;

    let caps = EnvironmentCapabilities::from_options(&CheckerOptions::default(), true);

    // Node globals produce TS2591 via diagnose_missing_name
    for name in &["require", "process", "Buffer", "__filename", "__dirname"] {
        let diag = caps.diagnose_missing_name(name);
        assert!(
            matches!(diag, Some(CapabilityDiagnostic::MissingNodeGlobal { .. })),
            "Expected MissingNodeGlobal for '{name}', got: {diag:?}"
        );
        assert_eq!(diag.unwrap().code(), 2591);
    }
}

#[test]
fn test_capability_diagnostic_import_attributes_check() {
    use crate::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use crate::query_boundaries::environment::CapabilityDiagnostic;

    // CommonJS → TS2823
    let opts = CheckerOptions {
        module: ModuleKind::CommonJS,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(
        caps.check_feature_gate(FeatureGate::ImportAttributes),
        Some(CapabilityDiagnostic::ImportAttributesUnsupported),
    );

    // ES2015 → TS2823
    let opts = CheckerOptions {
        module: ModuleKind::ES2015,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(
        caps.check_feature_gate(FeatureGate::ImportAttributes),
        Some(CapabilityDiagnostic::ImportAttributesUnsupported),
    );

    // ESNext → no diagnostic
    let opts = CheckerOptions {
        module: ModuleKind::ESNext,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(caps.check_feature_gate(FeatureGate::ImportAttributes), None);

    // Node20 → no diagnostic
    let opts = CheckerOptions {
        module: ModuleKind::Node20,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(caps.check_feature_gate(FeatureGate::ImportAttributes), None);
}

#[test]
fn test_capability_diagnostic_top_level_await_using() {
    use crate::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use crate::query_boundaries::environment::CapabilityDiagnostic;

    // CommonJS + ESNext target → TS2854 (wrong module)
    let opts = CheckerOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ESNext,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(
        caps.check_feature_gate(FeatureGate::TopLevelAwaitUsing),
        Some(CapabilityDiagnostic::TopLevelAwaitUsingUnsupported),
    );

    // ESNext + ES5 target → TS2854 (wrong target)
    let opts = CheckerOptions {
        module: ModuleKind::ESNext,
        target: ScriptTarget::ES5,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(
        caps.check_feature_gate(FeatureGate::TopLevelAwaitUsing),
        Some(CapabilityDiagnostic::TopLevelAwaitUsingUnsupported),
    );

    // ES2022 + ES2017 target → supported
    let opts = CheckerOptions {
        module: ModuleKind::ES2022,
        target: ScriptTarget::ES2017,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(
        caps.check_feature_gate(FeatureGate::TopLevelAwaitUsing),
        None
    );
}

#[test]
fn test_capability_diagnostic_config_compatibility() {
    use crate::query_boundaries::capabilities::EnvironmentCapabilities;
    use crate::query_boundaries::environment::CapabilityDiagnostic;

    // resolveJsonModule + System → TS5071
    let opts = CheckerOptions {
        module: ModuleKind::System,
        resolve_json_module: true,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    let diags = caps.check_config_compatibility();
    assert_eq!(diags.len(), 1);
    assert_eq!(
        diags[0],
        CapabilityDiagnostic::ResolveJsonModuleIncompatible
    );
    assert_eq!(diags[0].code(), 5071);

    // resolveJsonModule + UMD → TS5071
    let opts = CheckerOptions {
        module: ModuleKind::UMD,
        resolve_json_module: true,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(caps.check_config_compatibility().len(), 1);

    // resolveJsonModule + ESNext → compatible
    let opts = CheckerOptions {
        module: ModuleKind::ESNext,
        resolve_json_module: true,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert!(caps.check_config_compatibility().is_empty());
}

#[test]
fn test_capability_diagnostic_generator_prerequisite() {
    use crate::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use crate::query_boundaries::environment::CapabilityDiagnostic;

    // Without lib, generators require IterableIterator
    let opts = CheckerOptions::default();
    let caps = EnvironmentCapabilities::from_options(&opts, false);
    let diag = caps.check_feature_gate(FeatureGate::Generators);
    assert_eq!(
        diag,
        Some(CapabilityDiagnostic::FeatureRequiresGlobalType {
            gate: FeatureGate::Generators,
            required_type: "IterableIterator",
        })
    );

    // Without lib, async generators require AsyncIterableIterator
    let diag = caps.check_feature_gate(FeatureGate::AsyncGenerators);
    assert_eq!(
        diag,
        Some(CapabilityDiagnostic::FeatureRequiresGlobalType {
            gate: FeatureGate::AsyncGenerators,
            required_type: "AsyncIterableIterator",
        })
    );
}

#[test]
fn test_capability_diagnostic_code_mapping() {
    use crate::query_boundaries::environment::CapabilityDiagnostic;

    // Verify code() returns the correct diagnostic code for each variant
    assert_eq!(
        CapabilityDiagnostic::MissingGlobalType {
            name: "Array".to_string()
        }
        .code(),
        2318
    );
    assert_eq!(
        CapabilityDiagnostic::MissingEs2015Type {
            name: "Promise".to_string(),
            suggested_lib: "es2015".to_string(),
        }
        .code(),
        2583
    );
    assert_eq!(
        CapabilityDiagnostic::MissingDomGlobal {
            name: "document".to_string()
        }
        .code(),
        2584
    );
    assert_eq!(
        CapabilityDiagnostic::MissingNodeGlobal {
            name: "require".to_string()
        }
        .code(),
        2591
    );
    assert_eq!(
        CapabilityDiagnostic::MissingJQueryGlobal {
            name: "$".to_string()
        }
        .code(),
        2592
    );
    assert_eq!(
        CapabilityDiagnostic::MissingTestRunnerGlobal {
            name: "describe".to_string()
        }
        .code(),
        2593
    );
    assert_eq!(
        CapabilityDiagnostic::MissingBunGlobal {
            name: "Bun".to_string()
        }
        .code(),
        2868
    );
    assert_eq!(
        CapabilityDiagnostic::ImportAttributesUnsupported.code(),
        2823
    );
    assert_eq!(
        CapabilityDiagnostic::TopLevelAwaitUsingUnsupported.code(),
        2854
    );
    assert_eq!(
        CapabilityDiagnostic::ResolveJsonModuleIncompatible.code(),
        5071
    );
}
