//! Environment capabilities boundary regression tests.
//!
//! These tests verify that the `EnvironmentCapabilities` model correctly
//! routes diagnostics for:
//! - TS2318: Missing global types (lib availability)
//! - TS2591: Node.js globals (known-global classification)
//! - TS2583: ES2015+ type suggestions (known-global classification)
//! - TS2584: DOM globals (known-global classification)
//! - TS2823: Import attributes module option check (feature gate)
//! - Feature gate queries (import attributes, using, etc.)

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper: check source without lib files and with given options.
fn check_with_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<tsz_checker::diagnostics::Diagnostic> {
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
fn check_no_lib(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
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
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, MissingGlobalKind};
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
    // With explicit --noLib, tsc still requires CallableFunction/NewableFunction
    // even if the user manually declares Function and the other core globals.
    let diags_with_function = check_with_options(
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
    let ts2318_with_fn: Vec<_> = diags_with_function
        .iter()
        .filter(|d| d.code == 2318)
        .collect();
    assert!(
        ts2318_with_fn
            .iter()
            .any(|d| d.message_text.contains("CallableFunction")),
        "Expected TS2318 for missing CallableFunction with --noLib, got: {diags_with_function:?}"
    );
    assert!(
        ts2318_with_fn
            .iter()
            .any(|d| d.message_text.contains("NewableFunction")),
        "Expected TS2318 for missing NewableFunction with --noLib, got: {diags_with_function:?}"
    );

    // When Function itself is missing (true --noLib with nothing defined),
    // tsc also emits the broader TS2318 set including the auxiliary types.
    let diags_no_types = check_with_options(
        "declare function foo(): void;",
        CheckerOptions {
            no_lib: true,
            ..CheckerOptions::default()
        },
    );
    let ts2318_no_types: Vec<_> = diags_no_types.iter().filter(|d| d.code == 2318).collect();
    assert!(
        !ts2318_no_types.is_empty(),
        "Expected TS2318 for all missing core types with --noLib and no declarations, got: {diags_no_types:?}"
    );
}

#[test]
fn test_nolib_respects_explicit_function_aux_declarations() {
    let diags = check_with_options(
        r#"
interface Array<T> {}
interface Boolean {}
interface CallableFunction extends Function {}
interface Function {}
interface IArguments {}
interface NewableFunction extends Function {}
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
    let ts2318_for_aux: Vec<_> = diags
        .iter()
        .filter(|d| {
            d.code == 2318
                && (d.message_text.contains("CallableFunction")
                    || d.message_text.contains("NewableFunction"))
        })
        .collect();
    assert!(
        ts2318_for_aux.is_empty(),
        "Did not expect TS2318 for explicitly declared function aux types, got: {diags:?}"
    );
}

// =============================================================================
// Capability matrix unit tests (EnvironmentCapabilities struct)
// =============================================================================

#[test]
fn test_capabilities_matrix_esnext_module() {
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};

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
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};

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
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;

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
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, MissingGlobalKind};

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
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;

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
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

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
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

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
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

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
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

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
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

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
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

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
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

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
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

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
    assert_eq!(
        CapabilityDiagnostic::DeprecatedOption {
            name: "baseUrl".to_string()
        }
        .code(),
        5101
    );
    assert_eq!(
        CapabilityDiagnostic::DeprecatedOptionValue {
            name: "target".to_string(),
            value: "ES5".to_string(),
        }
        .code(),
        5107
    );
}

// =============================================================================
// TS2854: Top-level await using prerequisites (integration tests)
// =============================================================================

#[test]
fn test_top_level_await_using_emits_ts2854_with_commonjs() {
    let diags = check_with_options(
        "await using x = getResource();",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );
    let ts2854: Vec<_> = diags.iter().filter(|d| d.code == 2854).collect();
    assert!(
        !ts2854.is_empty(),
        "Expected TS2854 for top-level await using with CommonJS module, got: {diags:?}"
    );
}

#[test]
fn test_top_level_await_using_no_ts2854_with_esnext() {
    let diags = check_with_options(
        "await using x = getResource();",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );
    let ts2854: Vec<_> = diags.iter().filter(|d| d.code == 2854).collect();
    assert!(
        ts2854.is_empty(),
        "Expected NO TS2854 for top-level await using with ESNext, got: {ts2854:?}"
    );
}

#[test]
fn test_top_level_await_using_emits_ts2854_with_es5_target() {
    let diags = check_with_options(
        "await using x = getResource();",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ES5,
            ..CheckerOptions::default()
        },
    );
    let ts2854: Vec<_> = diags.iter().filter(|d| d.code == 2854).collect();
    assert!(
        !ts2854.is_empty(),
        "Expected TS2854 for top-level await using with ES5 target, got: {diags:?}"
    );
}

// =============================================================================
// TS5071: resolveJsonModule incompatibility (boundary tests)
// =============================================================================

#[test]
fn test_resolve_json_module_incompatible_with_none() {
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

    let opts = CheckerOptions {
        module: ModuleKind::None,
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
}

#[test]
fn test_resolve_json_module_compatible_with_esnext() {
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;

    let opts = CheckerOptions {
        module: ModuleKind::ESNext,
        resolve_json_module: true,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert!(caps.check_config_compatibility().is_empty());
}

#[test]
fn test_resolve_json_module_no_diag_when_not_set() {
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;

    let opts = CheckerOptions {
        module: ModuleKind::None,
        resolve_json_module: false,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert!(caps.check_config_compatibility().is_empty());
}

// =============================================================================
// TS5101/TS5107: Deprecation diagnostic awareness
// =============================================================================

#[test]
fn test_deprecation_state_skip_lib_resolution() {
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;

    let opts = CheckerOptions::default();
    let mut caps = EnvironmentCapabilities::from_options(&opts, true);

    // Default: no deprecation diagnostics
    assert!(!caps.should_skip_lib_type_resolution());

    // After setting deprecation state
    caps.has_deprecation_diagnostics = true;
    assert!(caps.should_skip_lib_type_resolution());
}

// =============================================================================
// Feature gate → required type reverse mapping
// =============================================================================

#[test]
fn test_gate_for_required_type_mapping() {
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};

    assert_eq!(
        EnvironmentCapabilities::gate_for_required_type("Disposable"),
        Some(FeatureGate::UsingDeclaration)
    );
    assert_eq!(
        EnvironmentCapabilities::gate_for_required_type("AsyncDisposable"),
        Some(FeatureGate::AwaitUsingDeclaration)
    );
    assert_eq!(
        EnvironmentCapabilities::gate_for_required_type("IterableIterator"),
        Some(FeatureGate::Generators)
    );
    assert_eq!(
        EnvironmentCapabilities::gate_for_required_type("AsyncIterableIterator"),
        Some(FeatureGate::AsyncGenerators)
    );
    assert_eq!(
        EnvironmentCapabilities::gate_for_required_type("TypedPropertyDescriptor"),
        Some(FeatureGate::ExperimentalDecorators)
    );
    assert_eq!(
        EnvironmentCapabilities::gate_for_required_type("Promise"),
        Some(FeatureGate::AsyncFunction)
    );
    assert_eq!(
        EnvironmentCapabilities::gate_for_required_type("Awaited"),
        Some(FeatureGate::AsyncFunction)
    );
    assert_eq!(
        EnvironmentCapabilities::gate_for_required_type("SomeUnknownType"),
        None
    );
}

// =============================================================================
// AsyncFunction feature gate
// =============================================================================

#[test]
fn test_async_function_requires_promise_no_lib() {
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

    let opts = CheckerOptions::default();
    let caps = EnvironmentCapabilities::from_options(&opts, false);
    let diag = caps.check_feature_gate(FeatureGate::AsyncFunction);
    assert_eq!(
        diag,
        Some(CapabilityDiagnostic::FeatureRequiresGlobalType {
            gate: FeatureGate::AsyncFunction,
            required_type: "Promise",
        })
    );
}

#[test]
fn test_async_function_no_diagnostic_with_lib() {
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};

    let opts = CheckerOptions::default();
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(caps.check_feature_gate(FeatureGate::AsyncFunction), None);
}

// =============================================================================
// Phase 2: Top-level await boundary routing (TS1378 via FeatureGate::TopLevelAwait)
// =============================================================================

#[test]
fn test_top_level_await_gate_unsupported_module() {
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

    // CommonJS + ESNext target → TS1378
    let opts = CheckerOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ESNext,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    let diag = caps.check_feature_gate(FeatureGate::TopLevelAwait);
    assert_eq!(
        diag,
        Some(CapabilityDiagnostic::TopLevelAwaitUnsupported),
        "TopLevelAwait should fire with CommonJS"
    );
    assert_eq!(diag.unwrap().code(), 1378);
}

#[test]
fn test_top_level_await_gate_unsupported_target() {
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

    // ESNext module + ES5 target → TS1378
    let opts = CheckerOptions {
        module: ModuleKind::ESNext,
        target: ScriptTarget::ES5,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(
        caps.check_feature_gate(FeatureGate::TopLevelAwait),
        Some(CapabilityDiagnostic::TopLevelAwaitUnsupported),
    );
}

#[test]
fn test_top_level_await_gate_supported() {
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};

    for module in [
        ModuleKind::ES2022,
        ModuleKind::ESNext,
        ModuleKind::System,
        ModuleKind::Node16,
        ModuleKind::Node18,
        ModuleKind::Node20,
        ModuleKind::NodeNext,
        ModuleKind::Preserve,
    ] {
        let opts = CheckerOptions {
            module,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert_eq!(
            caps.check_feature_gate(FeatureGate::TopLevelAwait),
            None,
            "TopLevelAwait should be supported with {module:?}"
        );
    }
}

// =============================================================================
// Phase 2: Import assert deprecation boundary (TS2880)
// =============================================================================

#[test]
fn test_import_assert_deprecated_fires_by_default() {
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

    let opts = CheckerOptions::default();
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    let diag = caps.check_import_assert_deprecated();
    assert_eq!(
        diag,
        Some(CapabilityDiagnostic::ImportAssertDeprecated),
        "TS2880 should fire when ignore_deprecations is false"
    );
    assert_eq!(diag.unwrap().code(), 2880);
}

#[test]
fn test_import_assert_deprecated_suppressed_by_ignore_deprecations() {
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;

    let opts = CheckerOptions {
        ignore_deprecations: true,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert_eq!(
        caps.check_import_assert_deprecated(),
        None,
        "TS2880 should NOT fire when ignore_deprecations is true"
    );
}

// =============================================================================
// Phase 2: Import assert deprecated integration test (checker-level)
// =============================================================================

#[test]
fn test_import_assert_deprecated_checker_integration_ts2880() {
    let diags = check_with_options(
        r#"import data from './data.json' assert { type: "json" };"#,
        CheckerOptions {
            module: ModuleKind::ESNext,
            ..CheckerOptions::default()
        },
    );
    let ts2880: Vec<_> = diags.iter().filter(|d| d.code == 2880).collect();
    assert!(
        !ts2880.is_empty(),
        "Expected TS2880 for 'assert' keyword, got: {diags:?}"
    );
}

#[test]
fn test_import_assert_deprecated_suppressed_checker_integration() {
    let diags = check_with_options(
        r#"import data from './data.json' assert { type: "json" };"#,
        CheckerOptions {
            module: ModuleKind::ESNext,
            ignore_deprecations: true,
            ..CheckerOptions::default()
        },
    );
    let ts2880: Vec<_> = diags.iter().filter(|d| d.code == 2880).collect();
    assert!(
        ts2880.is_empty(),
        "Expected NO TS2880 with ignore_deprecations=true, got: {ts2880:?}"
    );
}

// =============================================================================
// Phase 2: resolveJsonModule incompatibility extended (TS5071)
// =============================================================================

#[test]
fn test_resolve_json_module_incompatible_umd() {
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

    let opts = CheckerOptions {
        module: ModuleKind::UMD,
        resolve_json_module: true,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    let diags = caps.check_config_compatibility();
    assert_eq!(diags.len(), 1);
    assert_eq!(
        diags[0],
        CapabilityDiagnostic::ResolveJsonModuleIncompatible,
        "resolveJsonModule + UMD should produce TS5071"
    );
}

#[test]
fn test_resolve_json_module_compatible_commonjs() {
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;

    let opts = CheckerOptions {
        module: ModuleKind::CommonJS,
        resolve_json_module: true,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert!(
        caps.check_config_compatibility().is_empty(),
        "resolveJsonModule + CommonJS should be compatible"
    );
}

#[test]
fn test_resolve_json_module_compatible_preserve() {
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;

    let opts = CheckerOptions {
        module: ModuleKind::Preserve,
        resolve_json_module: true,
        ..CheckerOptions::default()
    };
    let caps = EnvironmentCapabilities::from_options(&opts, true);
    assert!(
        caps.check_config_compatibility().is_empty(),
        "resolveJsonModule + Preserve should be compatible"
    );
}

// =============================================================================
// Phase 2: Deprecation state and skip lib resolution (TS5101/TS5107)
// =============================================================================

#[test]
fn test_deprecation_state_propagates_to_skip_lib() {
    use tsz_checker::query_boundaries::capabilities::EnvironmentCapabilities;

    let opts = CheckerOptions::default();
    let mut caps = EnvironmentCapabilities::from_options(&opts, true);

    // Default: no deprecation
    assert!(!caps.has_deprecation_diagnostics);
    assert!(!caps.should_skip_lib_type_resolution());

    // Set deprecation: should skip lib
    caps.has_deprecation_diagnostics = true;
    assert!(caps.should_skip_lib_type_resolution());
}

#[test]
fn test_deprecation_diagnostic_code_mapping() {
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

    assert_eq!(
        CapabilityDiagnostic::DeprecatedOption {
            name: "charset".to_string()
        }
        .code(),
        5101,
        "DeprecatedOption should map to TS5101"
    );
    assert_eq!(
        CapabilityDiagnostic::DeprecatedOptionValue {
            name: "target".to_string(),
            value: "ES3".to_string(),
        }
        .code(),
        5107,
        "DeprecatedOptionValue should map to TS5107"
    );
}

// =============================================================================
// Phase 2: CapabilityDiagnostic code mapping completeness
// =============================================================================

#[test]
fn test_capability_diagnostic_new_variants_code_mapping() {
    use tsz_checker::query_boundaries::environment::CapabilityDiagnostic;

    // New variants added in phase 2
    assert_eq!(
        CapabilityDiagnostic::TopLevelAwaitUnsupported.code(),
        1378,
        "TopLevelAwaitUnsupported should map to TS1378"
    );
    assert_eq!(
        CapabilityDiagnostic::ImportAssertDeprecated.code(),
        2880,
        "ImportAssertDeprecated should map to TS2880"
    );
}

// =============================================================================
// Phase 2: Top-level await Node18/Node20 coverage (fixed by boundary routing)
// =============================================================================

#[test]
fn test_top_level_await_using_node18_node20_supported() {
    use tsz_checker::query_boundaries::capabilities::{EnvironmentCapabilities, FeatureGate};

    // Node18 and Node20 were previously missing from the manual match in
    // core_statement_checks.rs. Routing through the capability boundary fixed this.
    for module in [ModuleKind::Node18, ModuleKind::Node20] {
        let opts = CheckerOptions {
            module,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(
            caps.top_level_await_using_supported,
            "top_level_await_using_supported should be true for {module:?}"
        );
        assert_eq!(
            caps.check_feature_gate(FeatureGate::TopLevelAwaitUsing),
            None,
            "TopLevelAwaitUsing gate should not fire for {module:?}"
        );
        assert_eq!(
            caps.check_feature_gate(FeatureGate::TopLevelAwait),
            None,
            "TopLevelAwait gate should not fire for {module:?}"
        );
    }
}
