//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/config/option_fanout.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 91c9e1699be12576774252ade0a0dfc8779a099461685699de046f3684013215 147 composite_implies_declaration_and_incremental
    #[test]
    fn composite_implies_declaration_and_incremental() {
        let mut resolved = ResolvedCompilerOptions {
            composite: true,
            ..Default::default()
        };
        apply_non_strict_fanout(&mut resolved);

        assert!(resolved.emit_declarations);
        assert!(resolved.checker.emit_declarations);
        assert!(resolved.incremental);
    }
// TSZ_INLINE_TEST_END 91c9e1699be12576774252ade0a0dfc8779a099461685699de046f3684013215

// TSZ_INLINE_TEST_BEGIN de32aaaaa3af97911757bf9d7f62b0fbf23d21533ac28ff3415f29368eeb30af 160 isolated_modules_preserves_const_enums_on_printer
    #[test]
    fn isolated_modules_preserves_const_enums_on_printer() {
        let mut resolved = ResolvedCompilerOptions::default();
        resolved.checker.isolated_modules = true;
        apply_non_strict_fanout(&mut resolved);

        assert!(
            resolved.printer.preserve_const_enums,
            "isolatedModules must preserve const enums in emit (tsc \
             shouldPreserveConstEnums)"
        );
        assert!(resolved.printer.no_const_enum_inlining);
    }
// TSZ_INLINE_TEST_END de32aaaaa3af97911757bf9d7f62b0fbf23d21533ac28ff3415f29368eeb30af

// TSZ_INLINE_TEST_BEGIN b543594eda329c75eb0779ef783692c2ecac620226953f4513eaae752649ce1a 174 verbatim_module_syntax_implies_isolated_modules_and_const_enums
    #[test]
    fn verbatim_module_syntax_implies_isolated_modules_and_const_enums() {
        let mut resolved = ResolvedCompilerOptions::default();
        resolved.checker.verbatim_module_syntax = true;
        apply_non_strict_fanout(&mut resolved);

        assert!(
            resolved.checker.isolated_modules,
            "verbatimModuleSyntax implies isolatedModules (tsc computedOptions)"
        );
        assert!(resolved.printer.verbatim_module_syntax);
        assert!(resolved.printer.preserve_const_enums);
        assert!(resolved.printer.no_const_enum_inlining);
    }
// TSZ_INLINE_TEST_END b543594eda329c75eb0779ef783692c2ecac620226953f4513eaae752649ce1a

// TSZ_INLINE_TEST_BEGIN b94c2dfc98648b47e7ac29de24ff78217295db1ca59fbd6396a7648754dc873e 189 no_const_enum_implication_when_neither_set
    #[test]
    fn no_const_enum_implication_when_neither_set() {
        let mut resolved = ResolvedCompilerOptions::default();
        apply_non_strict_fanout(&mut resolved);

        assert!(!resolved.printer.preserve_const_enums);
        assert!(!resolved.printer.no_const_enum_inlining);
        assert!(!resolved.checker.isolated_modules);
    }
// TSZ_INLINE_TEST_END b94c2dfc98648b47e7ac29de24ff78217295db1ca59fbd6396a7648754dc873e

// TSZ_INLINE_TEST_BEGIN f1af5d8288fefb6969a8f7510d7cd2bcce4a37e2530f297c5631363547c92678 199 import_helpers_suppresses_inline_helpers
    #[test]
    fn import_helpers_suppresses_inline_helpers() {
        let mut resolved = ResolvedCompilerOptions {
            import_helpers: true,
            ..Default::default()
        };
        apply_non_strict_fanout(&mut resolved);

        assert!(resolved.printer.import_helpers);
        assert!(resolved.printer.no_emit_helpers);
    }
// TSZ_INLINE_TEST_END f1af5d8288fefb6969a8f7510d7cd2bcce4a37e2530f297c5631363547c92678

// TSZ_INLINE_TEST_BEGIN 7193a0b05b0242f5ca9b537a49690502a82fb02b37c5aeaaf339c92ddcfc13a7 211 es_module_interop_implies_synthetic_default_imports
    #[test]
    fn es_module_interop_implies_synthetic_default_imports() {
        // Both engines (CLI/tsconfig) populate the top-level and the
        // `checker` copies of `esModuleInterop` together, so mirror that here:
        // the checker-semantic half of the implication is owned by
        // `apply_checker_fanout` (driven by `checker.es_module_interop`) and
        // the top-level mirror by `apply_es_module_interop_synthetic_defaults`.
        let mut resolved = ResolvedCompilerOptions {
            es_module_interop: true,
            ..Default::default()
        };
        resolved.checker.es_module_interop = true;
        apply_non_strict_fanout(&mut resolved);

        assert!(resolved.allow_synthetic_default_imports);
        assert!(resolved.checker.allow_synthetic_default_imports);
    }
// TSZ_INLINE_TEST_END 7193a0b05b0242f5ca9b537a49690502a82fb02b37c5aeaaf339c92ddcfc13a7

// TSZ_INLINE_TEST_BEGIN 3d7f9d97efa551bfd08419d4a32ead0a915390810676bc55684c3dff0ca7cc27 229 no_implications_fire_for_default_options
    #[test]
    fn no_implications_fire_for_default_options() {
        let mut resolved = ResolvedCompilerOptions::default();
        apply_non_strict_fanout(&mut resolved);

        assert!(!resolved.emit_declarations);
        assert!(!resolved.incremental);
        assert!(!resolved.allow_synthetic_default_imports);
        assert!(!resolved.printer.no_emit_helpers);
    }
// TSZ_INLINE_TEST_END 3d7f9d97efa551bfd08419d4a32ead0a915390810676bc55684c3dff0ca7cc27

// TSZ_INLINE_TEST_BEGIN 411cf8643479673e9b3b2f9f6e37fb4fe3c92a55505de3f1b04e1e14c55e6666 240 idempotent_second_call_is_a_no_op
    #[test]
    fn idempotent_second_call_is_a_no_op() {
        let mut resolved = ResolvedCompilerOptions {
            composite: true,
            import_helpers: true,
            es_module_interop: true,
            ..Default::default()
        };
        resolved.checker.verbatim_module_syntax = true;
        apply_non_strict_fanout(&mut resolved);
        let once = resolved.clone();
        apply_non_strict_fanout(&mut resolved);

        assert_eq!(once.emit_declarations, resolved.emit_declarations);
        assert_eq!(once.incremental, resolved.incremental);
        assert_eq!(
            once.checker.isolated_modules,
            resolved.checker.isolated_modules
        );
        assert_eq!(
            once.printer.preserve_const_enums,
            resolved.printer.preserve_const_enums
        );
        assert_eq!(
            once.allow_synthetic_default_imports,
            resolved.allow_synthetic_default_imports
        );
    }
// TSZ_INLINE_TEST_END 411cf8643479673e9b3b2f9f6e37fb4fe3c92a55505de3f1b04e1e14c55e6666
