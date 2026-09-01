//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/capabilities.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 14d23476cb98c89bc86dcc2b35814626eb6e6407b200dcda619e0bbd36796700 433 test_is_known_node_module_recognizes_subpath_builtins
    #[test]
    fn test_is_known_node_module_recognizes_subpath_builtins() {
        // Base builtins.
        for base in ["fs", "path", "stream", "dns", "util", "readline", "timers"] {
            assert!(
                is_known_node_module(base),
                "expected base builtin {base} to be recognized"
            );
        }
        // Subpath builtins must classify identically to their base module —
        // tsc treats `fs/promises` exactly like `fs`. Previously these fell
        // through to a raw TS2307 because only the suppression list knew them.
        for subpath in [
            "assert/strict",
            "dns/promises",
            "fs/promises",
            "inspector/promises",
            "path/posix",
            "path/win32",
            "readline/promises",
            "stream/consumers",
            "stream/promises",
            "stream/web",
            "timers/promises",
            "util/types",
        ] {
            assert!(
                is_known_node_module(subpath),
                "expected subpath builtin {subpath} to be recognized"
            );
            // The `node:` scheme names the same builtin.
            assert!(
                is_known_node_module(&format!("node:{subpath}")),
                "expected node:{subpath} to be recognized"
            );
        }
        // Non-builtins (real user packages, incl. lookalikes) stay unrecognized
        // so they still take the user-package TS2307 path.
        for other in [
            "express",
            "fs-extra",
            "lodash",
            "fsx",
            "node:does-not-exist",
        ] {
            assert!(
                !is_known_node_module(other),
                "expected non-builtin {other} to NOT be recognized"
            );
        }
    }
// TSZ_INLINE_TEST_END 14d23476cb98c89bc86dcc2b35814626eb6e6407b200dcda619e0bbd36796700

// TSZ_INLINE_TEST_BEGIN 87ddaddcd65e2878a4930a21a36de14d613a9ab542477c3dccdeda54e95ea545 485 test_node_scheme_only_builtins_require_prefix
    #[test]
    fn test_node_scheme_only_builtins_require_prefix() {
        // `node:test`, `node:sqlite`, `node:sea` are builtins reachable only
        // through the `node:` scheme; `@types/node` declares only the prefixed
        // form. They must be recognized when prefixed...
        for scheme_only in ["test", "test/reporters", "sea", "sqlite"] {
            assert!(
                is_known_node_module(&format!("node:{scheme_only}")),
                "expected node:{scheme_only} to be recognized"
            );
            // ...and must NOT be recognized bare: `test`, `sea`, and `sqlite`
            // are real, published npm packages. Treating the bare name as a
            // builtin would steal the user-package TS2307 path.
            assert!(
                !is_known_node_module(scheme_only),
                "expected bare {scheme_only} (an npm package) to NOT be recognized"
            );
        }
    }
// TSZ_INLINE_TEST_END 87ddaddcd65e2878a4930a21a36de14d613a9ab542477c3dccdeda54e95ea545

// TSZ_INLINE_TEST_BEGIN 8054a3077c192524048981f71055495d667fe3c0cf12efd733e6fe3d9115715f 505 test_import_attributes_feature_gate
    #[test]
    fn test_import_attributes_feature_gate() {
        // ESNext supports import attributes
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::ESNext,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(caps.feature_available(FeatureGate::ImportAttributes));

        // CommonJS does not
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.feature_available(FeatureGate::ImportAttributes));

        // Preserve supports it
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::Preserve,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(caps.feature_available(FeatureGate::ImportAttributes));
    }
// TSZ_INLINE_TEST_END 8054a3077c192524048981f71055495d667fe3c0cf12efd733e6fe3d9115715f

// TSZ_INLINE_TEST_BEGIN aee3ca7a42071707c4947551b7b3dd2bfa776cb0780c5ce08f203ec664dce3b6 532 test_top_level_await_using_gate
    #[test]
    fn test_top_level_await_using_gate() {
        // ES2022 module + ESNext target → supported
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::ES2022,
            target: ScriptTarget::ESNext,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(caps.feature_available(FeatureGate::TopLevelAwaitUsing));

        // CommonJS + ESNext target → not supported (wrong module)
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ESNext,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.feature_available(FeatureGate::TopLevelAwaitUsing));

        // ESNext module + ES5 target → not supported (wrong target)
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ES5,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.feature_available(FeatureGate::TopLevelAwaitUsing));
    }
// TSZ_INLINE_TEST_END aee3ca7a42071707c4947551b7b3dd2bfa776cb0780c5ce08f203ec664dce3b6

// TSZ_INLINE_TEST_BEGIN 22807ffb6f4ebe18d4d675b28c66313bcd2336396214937be0e777d1291a8124 562 test_resolve_json_module_compatibility
    #[test]
    fn test_resolve_json_module_compatibility() {
        // CommonJS is compatible
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(caps.resolve_json_module_compatible);

        // None is incompatible
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::None,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.resolve_json_module_compatible);

        // System is incompatible
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::System,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.resolve_json_module_compatible);

        // UMD is incompatible
        let opts = tsz_common::CheckerOptions {
            module: ModuleKind::UMD,
            ..Default::default()
        };
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.resolve_json_module_compatible);
    }
// TSZ_INLINE_TEST_END 22807ffb6f4ebe18d4d675b28c66313bcd2336396214937be0e777d1291a8124

// TSZ_INLINE_TEST_BEGIN 6db2c4676150da6a00170a68b61fcff2a5d4fe8f632559f3b3f1207b7fe8237b 597 test_classify_missing_global
    #[test]
    fn test_classify_missing_global() {
        let opts = tsz_common::checker_options::CheckerOptions::default();
        let caps = EnvironmentCapabilities::from_options(&opts, true);

        assert_eq!(
            caps.classify_missing_global("require"),
            Some(MissingGlobalKind::NodeGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("process"),
            Some(MissingGlobalKind::NodeGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("console"),
            Some(MissingGlobalKind::DomGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("document"),
            Some(MissingGlobalKind::DomGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("crypto"),
            Some(MissingGlobalKind::PlainGlobalValue)
        );
        assert_eq!(
            caps.classify_missing_global("$"),
            Some(MissingGlobalKind::JQueryGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("describe"),
            Some(MissingGlobalKind::TestRunnerGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("Bun"),
            Some(MissingGlobalKind::BunGlobal)
        );
        assert_eq!(
            caps.classify_missing_global("Promise"),
            Some(MissingGlobalKind::Es2015PlusType)
        );
        assert_eq!(caps.classify_missing_global("myCustomVar"), None);
    }
// TSZ_INLINE_TEST_END 6db2c4676150da6a00170a68b61fcff2a5d4fe8f632559f3b3f1207b7fe8237b

// TSZ_INLINE_TEST_BEGIN 63a04a552a7cefad215f02826a4728597fcb9f96728a98b07d170ae6fb4e94e6 641 test_required_global_type_for_feature
    #[test]
    fn test_required_global_type_for_feature() {
        assert_eq!(
            EnvironmentCapabilities::required_global_type(FeatureGate::UsingDeclaration),
            Some("Disposable")
        );
        assert_eq!(
            EnvironmentCapabilities::required_global_type(FeatureGate::AwaitUsingDeclaration),
            Some("AsyncDisposable")
        );
        assert_eq!(
            EnvironmentCapabilities::required_global_type(FeatureGate::Generators),
            Some("IterableIterator")
        );
        assert_eq!(
            EnvironmentCapabilities::required_global_type(FeatureGate::ImportAttributes),
            None
        );
        assert_eq!(
            EnvironmentCapabilities::required_global_type(FeatureGate::AsyncFunction),
            Some("Promise")
        );
    }
// TSZ_INLINE_TEST_END 63a04a552a7cefad215f02826a4728597fcb9f96728a98b07d170ae6fb4e94e6

// TSZ_INLINE_TEST_BEGIN 68f29dbe0de06391c9992e41c779c850d324eb3d3e22241e0c77e232b4dc5d56 665 test_gate_for_required_type_reverse_lookup
    #[test]
    fn test_gate_for_required_type_reverse_lookup() {
        // Forward and reverse mappings must be consistent
        let gates = [
            FeatureGate::UsingDeclaration,
            FeatureGate::AwaitUsingDeclaration,
            FeatureGate::Generators,
            FeatureGate::AsyncGenerators,
            FeatureGate::ExperimentalDecorators,
            FeatureGate::AsyncFunction,
        ];
        for gate in gates {
            if let Some(type_name) = EnvironmentCapabilities::required_global_type(gate) {
                assert_eq!(
                    EnvironmentCapabilities::gate_for_required_type(type_name),
                    Some(gate),
                    "Reverse lookup for type '{type_name}' (gate {gate:?}) should match"
                );
            }
        }
    }
// TSZ_INLINE_TEST_END 68f29dbe0de06391c9992e41c779c850d324eb3d3e22241e0c77e232b4dc5d56

// TSZ_INLINE_TEST_BEGIN 158cf093d3caf2c87df737172d0d7c956396022e41818abc7da8e32c98280c1c 687 test_async_function_gate_no_lib
    #[test]
    fn test_async_function_gate_no_lib() {
        let opts = tsz_common::checker_options::CheckerOptions::default();
        let caps = EnvironmentCapabilities::from_options(&opts, false);
        assert!(!caps.feature_available(FeatureGate::AsyncFunction));
    }
// TSZ_INLINE_TEST_END 158cf093d3caf2c87df737172d0d7c956396022e41818abc7da8e32c98280c1c

// TSZ_INLINE_TEST_BEGIN 46cb90197072a1429ded081deb1a2c6cc6c83c16e35e71141eb1890fae63acb3 694 test_async_function_gate_with_lib
    #[test]
    fn test_async_function_gate_with_lib() {
        let opts = tsz_common::checker_options::CheckerOptions::default();
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(caps.feature_available(FeatureGate::AsyncFunction));
    }
// TSZ_INLINE_TEST_END 46cb90197072a1429ded081deb1a2c6cc6c83c16e35e71141eb1890fae63acb3

// TSZ_INLINE_TEST_BEGIN 7e3d30eb9c5a97a31527c0fa9c9bf7e2d836c9a7aab25b39b899244519a76d1b 701 test_deprecation_diagnostics_default_false
    #[test]
    fn test_deprecation_diagnostics_default_false() {
        let opts = tsz_common::checker_options::CheckerOptions::default();
        let caps = EnvironmentCapabilities::from_options(&opts, true);
        assert!(!caps.has_deprecation_diagnostics);
    }
// TSZ_INLINE_TEST_END 7e3d30eb9c5a97a31527c0fa9c9bf7e2d836c9a7aab25b39b899244519a76d1b
