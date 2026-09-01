//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/environment.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN d8d5abb105a66b16e3946cac703697650ee79b3c2aa5e2b05a609da6b7d40d9a 290 test_import_attributes_gate_unsupported
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
// TSZ_INLINE_TEST_END d8d5abb105a66b16e3946cac703697650ee79b3c2aa5e2b05a609da6b7d40d9a

// TSZ_INLINE_TEST_BEGIN 13707ac2686cb670164d117fdce73c72e68319ec97638b2eedabe8338fd4ffae 308 test_import_attributes_gate_supported
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
// TSZ_INLINE_TEST_END 13707ac2686cb670164d117fdce73c72e68319ec97638b2eedabe8338fd4ffae

// TSZ_INLINE_TEST_BEGIN 14c47852fef853705f44af1adbda50d17211d45c45da3f004a601914d78b2ac0 330 test_top_level_await_using_gate_unsupported
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
// TSZ_INLINE_TEST_END 14c47852fef853705f44af1adbda50d17211d45c45da3f004a601914d78b2ac0

// TSZ_INLINE_TEST_BEGIN 057293438725c715a62cc8a3ae787195dbc92d919eb6a1c50c48c4760a87637e 362 test_top_level_await_using_gate_supported
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
// TSZ_INLINE_TEST_END 057293438725c715a62cc8a3ae787195dbc92d919eb6a1c50c48c4760a87637e

// TSZ_INLINE_TEST_BEGIN 830e83176c9bf6d58190c8fa01352d5e6da6c3c9b8f00d0374c3a8c9988f5a3d 376 test_resolve_json_module_gate
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
// TSZ_INLINE_TEST_END 830e83176c9bf6d58190c8fa01352d5e6da6c3c9b8f00d0374c3a8c9988f5a3d

// TSZ_INLINE_TEST_BEGIN eaa68c9352ffaae3db56949a71926ea9354a642490880dcdf0cb4e105a0351fe 407 test_using_requires_disposable_no_lib
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
// TSZ_INLINE_TEST_END eaa68c9352ffaae3db56949a71926ea9354a642490880dcdf0cb4e105a0351fe

// TSZ_INLINE_TEST_BEGIN b1aa5b06c4f267d4fe5052ede8cc980c75dc8d553b68b3fc297126f3ab666e85 421 test_await_using_requires_async_disposable_no_lib
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
// TSZ_INLINE_TEST_END b1aa5b06c4f267d4fe5052ede8cc980c75dc8d553b68b3fc297126f3ab666e85

// TSZ_INLINE_TEST_BEGIN 9c562023a882d64c226b957baa164306fe8abbc81e6b3f653c4c6928e7ef37c4 435 test_using_no_diagnostic_with_lib
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
// TSZ_INLINE_TEST_END 9c562023a882d64c226b957baa164306fe8abbc81e6b3f653c4c6928e7ef37c4

// TSZ_INLINE_TEST_BEGIN 5d18364377c9dc6e672d33c6f1e4f75aba8d928cbbfee7fb51bc80f66298feca 450 test_diagnose_missing_node_global
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
// TSZ_INLINE_TEST_END 5d18364377c9dc6e672d33c6f1e4f75aba8d928cbbfee7fb51bc80f66298feca

// TSZ_INLINE_TEST_BEGIN 3902d56b864e4df6d8f3f5f53802a876e876b5cc26024bae9449e0707d54134b 467 test_diagnose_missing_dom_global
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
// TSZ_INLINE_TEST_END 3902d56b864e4df6d8f3f5f53802a876e876b5cc26024bae9449e0707d54134b

// TSZ_INLINE_TEST_BEGIN 3c637a04ad4741efd1811be227a44862af6c8fe8b6810cff4c29232d76c84311 483 test_diagnose_missing_es2015_type
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
// TSZ_INLINE_TEST_END 3c637a04ad4741efd1811be227a44862af6c8fe8b6810cff4c29232d76c84311

// TSZ_INLINE_TEST_BEGIN 5234ce4da20afdc5544cf30497aba5a8bc4bbdbdb37902288a6c26a89bf37d4f 498 test_diagnose_missing_unknown_name
    #[test]
    fn test_diagnose_missing_unknown_name() {
        let caps = EnvironmentCapabilities::from_options(&CheckerOptions::default(), true);
        assert_eq!(caps.diagnose_missing_name("myCustomVar"), None);
    }
// TSZ_INLINE_TEST_END 5234ce4da20afdc5544cf30497aba5a8bc4bbdbdb37902288a6c26a89bf37d4f

// TSZ_INLINE_TEST_BEGIN 7c923a487132c8d6580c323fb12923d73d5754e2f9518e3a0e5dd772b64f03fb 508 test_config_compatibility_resolve_json_module_incompatible
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
// TSZ_INLINE_TEST_END 7c923a487132c8d6580c323fb12923d73d5754e2f9518e3a0e5dd772b64f03fb

// TSZ_INLINE_TEST_BEGIN d7bbc85103222cc541524484adc49ded113a6f4131d7b402ab4c695babd13b69 525 test_config_compatibility_resolve_json_module_compatible
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
// TSZ_INLINE_TEST_END d7bbc85103222cc541524484adc49ded113a6f4131d7b402ab4c695babd13b69

// TSZ_INLINE_TEST_BEGIN 7d5e4e17df603e342a4e80ac13d0575ded74d1583fc73f0c7c2b4dba31dfef21 537 test_config_compatibility_resolve_json_module_not_set
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
// TSZ_INLINE_TEST_END 7d5e4e17df603e342a4e80ac13d0575ded74d1583fc73f0c7c2b4dba31dfef21
