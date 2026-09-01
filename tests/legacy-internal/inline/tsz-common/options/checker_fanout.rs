//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/options/checker_fanout.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 67a8c9a32654bb122d0a326ad4f12d5b9d46247d6d6b699e3f33c942c43592cd 54 verbatim_module_syntax_implies_isolated_modules
    #[test]
    fn verbatim_module_syntax_implies_isolated_modules() {
        let mut options = CheckerOptions {
            verbatim_module_syntax: true,
            isolated_modules: false,
            ..Default::default()
        };
        apply_checker_fanout(&mut options);
        assert!(options.isolated_modules);
    }
// TSZ_INLINE_TEST_END 67a8c9a32654bb122d0a326ad4f12d5b9d46247d6d6b699e3f33c942c43592cd

// TSZ_INLINE_TEST_BEGIN 0f3dfc49d898574daa69c8f4c23c961a085ece5c552a5baf07bf521ee6647ac8 65 es_module_interop_implies_synthetic_default_imports
    #[test]
    fn es_module_interop_implies_synthetic_default_imports() {
        let mut options = CheckerOptions {
            es_module_interop: true,
            allow_synthetic_default_imports: false,
            ..Default::default()
        };
        apply_checker_fanout(&mut options);
        assert!(options.allow_synthetic_default_imports);
    }
// TSZ_INLINE_TEST_END 0f3dfc49d898574daa69c8f4c23c961a085ece5c552a5baf07bf521ee6647ac8

// TSZ_INLINE_TEST_BEGIN 301bd75a458cf39c3fb5cb6f96739aac6281d4a257efd35dccbaa8ff93029714 76 no_implication_when_sources_unset
    #[test]
    fn no_implication_when_sources_unset() {
        let mut options = CheckerOptions {
            verbatim_module_syntax: false,
            es_module_interop: false,
            isolated_modules: false,
            allow_synthetic_default_imports: false,
            ..Default::default()
        };
        apply_checker_fanout(&mut options);
        assert!(!options.isolated_modules);
        assert!(!options.allow_synthetic_default_imports);
    }
// TSZ_INLINE_TEST_END 301bd75a458cf39c3fb5cb6f96739aac6281d4a257efd35dccbaa8ff93029714
