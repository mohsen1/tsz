//! Environment diagnostic boundary for feature-gate / lib / config checks.
//!
//! This module provides `CapabilityDiagnostic`, a structured representation
//! of environment-related diagnostic decisions. Instead of each call site
//! independently checking `capabilities.import_attributes_supported` and
//! then constructing a diagnostic, the call site asks this boundary
//! "what diagnostic (if any) should be emitted?" and gets a value back.
//!
//! The boundary keeps diagnostic *production* separate from diagnostic
//! *emission* — the checker still decides *where* to attach the diagnostic
//! (position, node, span), but the *what* comes from here.
//!
//! Diagnostic families routed through this boundary:
//! - TS2318: Cannot find global type (missing lib types)
//! - TS2591: Cannot find name (Node.js globals)
//! - TS2583: Cannot find name (ES2015+ lib suggestion)
//! - TS2584: Cannot find name (DOM lib suggestion)
//! - TS2592: Cannot find name (jQuery globals)
//! - TS2593: Cannot find name (test runner globals)
//! - TS2868: Cannot find name (Bun globals)
//! - TS2823: Import attributes require specific module option
//! - TS2854: Top-level await using requires specific module/target
//! - TS5071: resolveJsonModule incompatible with module option
//! - TS5101: Deprecated compiler option (no-value)
//! - TS5107: Deprecated compiler option value

use super::capabilities::{EnvironmentCapabilities, FeatureGate, MissingGlobalKind};

/// A diagnostic decision produced by the environment capability boundary.
///
/// Each variant maps to a specific diagnostic code family and contains
/// enough context for the caller to produce the full diagnostic.
/// The caller (checker error reporter) decides *where* (position/node)
/// and *how* (error_at_node vs error_at_position) to emit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CapabilityDiagnostic {
    /// TS2318: Cannot find global type '{name}'.
    /// Emitted when a core global type is missing (e.g., `Array`, `String`
    /// when --noLib is set).
    MissingGlobalType { name: String },

    /// TS2583: Cannot find name '{name}'. Change target library to '{lib}'.
    /// Emitted when an ES2015+ type (Promise, Map, Set, etc.) is used
    /// but the current lib doesn't include it.
    MissingEs2015Type { name: String, suggested_lib: String },

    /// TS2584: Cannot find name '{name}'. Change target library to include 'dom'.
    MissingDomGlobal { name: String },

    /// TS2591: Cannot find name '{name}'. Need @types/node.
    MissingNodeGlobal { name: String },

    /// TS2592: Cannot find name '{name}'. Need @types/jquery.
    MissingJQueryGlobal { name: String },

    /// TS2593: Cannot find name '{name}'. Need test runner types.
    MissingTestRunnerGlobal { name: String },

    /// TS2868: Cannot find name '{name}'. Need @types/bun.
    MissingBunGlobal { name: String },

    /// TS2823: Import attributes not supported with current module option.
    ImportAttributesUnsupported,

    /// TS2854: Top-level 'await using' not supported with current module/target.
    TopLevelAwaitUsingUnsupported,

    /// TS5071: resolveJsonModule incompatible with current module option.
    ResolveJsonModuleIncompatible,

    /// Feature requires a global type that is not available.
    /// E.g., `using` requires `Disposable`, `await using` requires `AsyncDisposable`.
    FeatureRequiresGlobalType {
        gate: FeatureGate,
        required_type: &'static str,
    },

    /// TS5101: Option '{name}' is deprecated and will stop functioning.
    /// Emitted for deprecated compiler options without value context (e.g., `baseUrl`).
    DeprecatedOption { name: String },

    /// TS5107: Option '{name}={value}' is deprecated and will stop functioning.
    /// Emitted for deprecated compiler option values (e.g., `target=ES5`).
    DeprecatedOptionValue { name: String, value: String },
}

impl CapabilityDiagnostic {
    /// The diagnostic code associated with this capability diagnostic.
    pub fn code(&self) -> u32 {
        match self {
            Self::MissingGlobalType { .. } => 2318,
            Self::MissingEs2015Type { .. } => 2583,
            Self::MissingDomGlobal { .. } => 2584,
            Self::MissingNodeGlobal { .. } => 2591,
            Self::MissingJQueryGlobal { .. } => 2592,
            Self::MissingTestRunnerGlobal { .. } => 2593,
            Self::MissingBunGlobal { .. } => 2868,
            Self::ImportAttributesUnsupported => 2823,
            Self::TopLevelAwaitUsingUnsupported => 2854,
            Self::ResolveJsonModuleIncompatible => 5071,
            Self::FeatureRequiresGlobalType { .. } => 2318,
            Self::DeprecatedOption { .. } => 5101,
            Self::DeprecatedOptionValue { .. } => 5107,
        }
    }
}

impl EnvironmentCapabilities {
    /// Check a feature gate and return the diagnostic to emit if unsatisfied.
    ///
    /// This is the single decision point for "should a feature-gate diagnostic
    /// be emitted?" — replacing per-call-site `if !capabilities.X { emit }` patterns.
    pub(crate) fn check_feature_gate(&self, gate: FeatureGate) -> Option<CapabilityDiagnostic> {
        match gate {
            FeatureGate::ImportAttributes => {
                if !self.import_attributes_supported {
                    Some(CapabilityDiagnostic::ImportAttributesUnsupported)
                } else {
                    None
                }
            }
            FeatureGate::TopLevelAwaitUsing => {
                if !self.top_level_await_using_supported {
                    Some(CapabilityDiagnostic::TopLevelAwaitUsingUnsupported)
                } else {
                    None
                }
            }
            FeatureGate::ResolveJsonModule => {
                if !self.resolve_json_module_compatible {
                    Some(CapabilityDiagnostic::ResolveJsonModuleIncompatible)
                } else {
                    None
                }
            }
            // For type-dependent gates, check if the gate requires a global type
            // that the environment should provide. The caller must separately verify
            // whether the type actually exists in the program.
            FeatureGate::UsingDeclaration
            | FeatureGate::AwaitUsingDeclaration
            | FeatureGate::Generators
            | FeatureGate::AsyncGenerators
            | FeatureGate::ExperimentalDecorators
            | FeatureGate::AsyncFunction
            | FeatureGate::AsyncFunctionEs5 => {
                if !self.has_lib {
                    if let Some(required_type) = EnvironmentCapabilities::required_global_type(gate)
                    {
                        Some(CapabilityDiagnostic::FeatureRequiresGlobalType {
                            gate,
                            required_type,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    }

    /// Diagnose a missing global name — returns the appropriate diagnostic
    /// based on the capability classifier.
    ///
    /// This wraps `classify_missing_global` and resolves the classification
    /// into a concrete `CapabilityDiagnostic` with all context needed for
    /// error emission.
    pub(crate) fn diagnose_missing_name(&self, name: &str) -> Option<CapabilityDiagnostic> {
        let kind = self.classify_missing_global(name)?;
        match kind {
            MissingGlobalKind::CoreGlobalType => Some(CapabilityDiagnostic::MissingGlobalType {
                name: name.to_string(),
            }),
            MissingGlobalKind::FeatureGlobalType => Some(CapabilityDiagnostic::MissingGlobalType {
                name: name.to_string(),
            }),
            MissingGlobalKind::Es2015PlusType => {
                let suggested_lib =
                    tsz_binder::lib_loader::get_suggested_lib_for_type(name).to_string();
                Some(CapabilityDiagnostic::MissingEs2015Type {
                    name: name.to_string(),
                    suggested_lib,
                })
            }
            MissingGlobalKind::DomGlobal => Some(CapabilityDiagnostic::MissingDomGlobal {
                name: name.to_string(),
            }),
            MissingGlobalKind::NodeGlobal => Some(CapabilityDiagnostic::MissingNodeGlobal {
                name: name.to_string(),
            }),
            MissingGlobalKind::JQueryGlobal => Some(CapabilityDiagnostic::MissingJQueryGlobal {
                name: name.to_string(),
            }),
            MissingGlobalKind::TestRunnerGlobal => {
                Some(CapabilityDiagnostic::MissingTestRunnerGlobal {
                    name: name.to_string(),
                })
            }
            MissingGlobalKind::BunGlobal => Some(CapabilityDiagnostic::MissingBunGlobal {
                name: name.to_string(),
            }),
        }
    }

    /// Whether lib type resolution should be skipped.
    ///
    /// When TS5107/TS5101 deprecation diagnostics are present, tsc stops compilation
    /// early and never resolves lib types. This centralizes the decision that was
    /// previously passed as `skip_lib_type_resolution` from the driver.
    pub fn should_skip_lib_type_resolution(&self) -> bool {
        self.has_deprecation_diagnostics
    }

    /// Check config compatibility and return any diagnostics.
    ///
    /// Currently checks:
    /// - TS5071: resolveJsonModule incompatible with module kind
    pub(crate) fn check_config_compatibility(&self) -> Vec<CapabilityDiagnostic> {
        let mut diags = Vec::new();

        // TS5071: resolveJsonModule with incompatible module kind
        if self.resolve_json_module && !self.resolve_json_module_compatible {
            diags.push(CapabilityDiagnostic::ResolveJsonModuleIncompatible);
        }

        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_common::checker_options::CheckerOptions;
    use tsz_common::common::{ModuleKind, ScriptTarget};

    // =========================================================================
    // Feature gate diagnostics
    // =========================================================================

    #[test]
    fn test_import_attributes_gate_unsupported() {
        let opts = CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        let diag = caps.check_feature_gate(FeatureGate::ImportAttributes);
        assert_eq!(
            diag,
            Some(CapabilityDiagnostic::ImportAttributesUnsupported)
        );
        assert_eq!(
            diag.expect("import attributes gate should fire").code(),
            2823
        );
    }

    #[test]
    fn test_import_attributes_gate_supported() {
        for module in [
            ModuleKind::ESNext,
            ModuleKind::Node18,
            ModuleKind::Node20,
            ModuleKind::NodeNext,
            ModuleKind::Preserve,
        ] {
            let opts = CheckerOptions {
                module,
                ..CheckerOptions::default()
            };
            let caps = EnvironmentCapabilities::from_options(&opts, true);
            assert_eq!(
                caps.check_feature_gate(FeatureGate::ImportAttributes),
                None,
                "ImportAttributes should be supported with {module:?}"
            );
        }
    }

    #[test]
    fn test_top_level_await_using_gate_unsupported() {
        // Wrong module
        let opts = CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        let diag = caps.check_feature_gate(FeatureGate::TopLevelAwaitUsing);
        assert_eq!(
            diag,
            Some(CapabilityDiagnostic::TopLevelAwaitUsingUnsupported)
        );
        assert_eq!(
            diag.expect("top-level await/using gate should fire").code(),
            2854
        );

        // Wrong target
        let opts = CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ES5,
            ..CheckerOptions::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert_eq!(
            caps.check_feature_gate(FeatureGate::TopLevelAwaitUsing),
            Some(CapabilityDiagnostic::TopLevelAwaitUsingUnsupported)
        );
    }

    #[test]
    fn test_top_level_await_using_gate_supported() {
        let opts = CheckerOptions {
            module: ModuleKind::ES2022,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert_eq!(
            caps.check_feature_gate(FeatureGate::TopLevelAwaitUsing),
            None
        );
    }

    #[test]
    fn test_resolve_json_module_gate() {
        for module in [ModuleKind::None, ModuleKind::System, ModuleKind::UMD] {
            let opts = CheckerOptions {
                module,
                ..CheckerOptions::default()
            };
            let caps = EnvironmentCapabilities::from_options(&opts, true);
            assert_eq!(
                caps.check_feature_gate(FeatureGate::ResolveJsonModule),
                Some(CapabilityDiagnostic::ResolveJsonModuleIncompatible),
                "ResolveJsonModule should be incompatible with {module:?}"
            );
        }

        // Compatible
        let opts = CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert_eq!(
            caps.check_feature_gate(FeatureGate::ResolveJsonModule),
            None
        );
    }

    // =========================================================================
    // Using/await using prerequisite diagnostics
    // =========================================================================

    #[test]
    fn test_using_requires_disposable_no_lib() {
        let opts = CheckerOptions::default();
        let caps = EnvironmentCapabilities::from_options(&opts, false); // no lib loaded
        let diag = caps.check_feature_gate(FeatureGate::UsingDeclaration);
        assert_eq!(
            diag,
            Some(CapabilityDiagnostic::FeatureRequiresGlobalType {
                gate: FeatureGate::UsingDeclaration,
                required_type: "Disposable",
            })
        );
    }

    #[test]
    fn test_await_using_requires_async_disposable_no_lib() {
        let opts = CheckerOptions::default();
        let caps = EnvironmentCapabilities::from_options(&opts, false);
        let diag = caps.check_feature_gate(FeatureGate::AwaitUsingDeclaration);
        assert_eq!(
            diag,
            Some(CapabilityDiagnostic::FeatureRequiresGlobalType {
                gate: FeatureGate::AwaitUsingDeclaration,
                required_type: "AsyncDisposable",
            })
        );
    }

    #[test]
    fn test_using_no_diagnostic_with_lib() {
        let opts = CheckerOptions::default();
        let caps = EnvironmentCapabilities::from_options(&opts, true); // lib loaded
        assert_eq!(caps.check_feature_gate(FeatureGate::UsingDeclaration), None);
        assert_eq!(
            caps.check_feature_gate(FeatureGate::AwaitUsingDeclaration),
            None
        );
    }

    // =========================================================================
    // Missing global name diagnostics
    // =========================================================================

    #[test]
    fn test_diagnose_missing_node_global() {
        let caps = EnvironmentCapabilities::from_options(&CheckerOptions::default(), true);
        let diag = caps.diagnose_missing_name("require");
        assert_eq!(
            diag,
            Some(CapabilityDiagnostic::MissingNodeGlobal {
                name: "require".to_string(),
            })
        );
        assert_eq!(
            diag.expect("missing node global diagnostic expected")
                .code(),
            2591
        );
    }

    #[test]
    fn test_diagnose_missing_dom_global() {
        let caps = EnvironmentCapabilities::from_options(&CheckerOptions::default(), true);
        let diag = caps.diagnose_missing_name("document");
        assert_eq!(
            diag,
            Some(CapabilityDiagnostic::MissingDomGlobal {
                name: "document".to_string(),
            })
        );
        assert_eq!(
            diag.expect("missing DOM global diagnostic expected").code(),
            2584
        );
    }

    #[test]
    fn test_diagnose_missing_es2015_type() {
        let caps = EnvironmentCapabilities::from_options(&CheckerOptions::default(), true);
        let diag = caps.diagnose_missing_name("Promise");
        assert!(matches!(
            diag,
            Some(CapabilityDiagnostic::MissingEs2015Type { .. })
        ));
        assert_eq!(
            diag.expect("missing ES2015 type diagnostic expected")
                .code(),
            2583
        );
    }

    #[test]
    fn test_diagnose_missing_unknown_name() {
        let caps = EnvironmentCapabilities::from_options(&CheckerOptions::default(), true);
        assert_eq!(caps.diagnose_missing_name("myCustomVar"), None);
    }

    // =========================================================================
    // Config compatibility diagnostics
    // =========================================================================

    #[test]
    fn test_config_compatibility_resolve_json_module_incompatible() {
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
    }

    #[test]
    fn test_config_compatibility_resolve_json_module_compatible() {
        let opts = CheckerOptions {
            module: ModuleKind::CommonJS,
            resolve_json_module: true,
            ..CheckerOptions::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        let diags = caps.check_config_compatibility();
        assert!(diags.is_empty());
    }

    #[test]
    fn test_config_compatibility_resolve_json_module_not_set() {
        let opts = CheckerOptions {
            module: ModuleKind::System,
            resolve_json_module: false,
            ..CheckerOptions::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        let diags = caps.check_config_compatibility();
        assert!(diags.is_empty());
    }
}
