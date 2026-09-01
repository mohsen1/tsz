//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/parallel/core/premerged_lib_compaction.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN d133251705e2a1aba60ebc3ddec0e31690eeec9b9ee18b6c0fbcc24720fee336 35 densify_source_symbols_skip_unretained_shared_lib_prefix
    #[test]
    fn densify_source_symbols_skip_unretained_shared_lib_prefix() {
        let mut binder = BinderState::new();
        let dropped_lib = binder.symbols.alloc(0, "Array".to_owned());
        let retained_lib = binder.symbols.alloc(0, "Promise".to_owned());
        binder.symbols.share_current_symbols_for_append();
        let local = binder.symbols.alloc(0, "Local".to_owned());
        binder.symbols.get_mut(local).expect("local symbol").parent = retained_lib;

        let lib_symbol_ids = FxHashSet::from_iter([dropped_lib, retained_lib]);
        let retained_lib_symbols = FxHashSet::from_iter([retained_lib]);

        let mut source_ids = Vec::new();
        for_each_densify_source_symbol(
            &binder.symbols,
            &lib_symbol_ids,
            &retained_lib_symbols,
            |sym| source_ids.push(sym.id),
        );

        assert_eq!(source_ids, vec![retained_lib, local]);
        assert!(!source_ids.contains(&dropped_lib));
    }
// TSZ_INLINE_TEST_END d133251705e2a1aba60ebc3ddec0e31690eeec9b9ee18b6c0fbcc24720fee336

// TSZ_INLINE_TEST_BEGIN d25b4f2bc26db8766f1dfc2c7d196d139370fe96bce4465f1ad41744ad18fc85 59 densify_source_symbols_mixed_prefix_uses_full_filter_order
    #[test]
    fn densify_source_symbols_mixed_prefix_uses_full_filter_order() {
        let mut binder = BinderState::new();
        let shared_lib = binder.symbols.alloc(0, "Array".to_owned());
        let shared_non_lib = binder.symbols.alloc(0, "SharedUser".to_owned());
        binder.symbols.share_current_symbols_for_append();
        let local = binder.symbols.alloc(0, "Local".to_owned());

        let lib_symbol_ids = FxHashSet::from_iter([shared_lib]);
        let retained_lib_symbols = FxHashSet::default();

        let mut source_ids = Vec::new();
        for_each_densify_source_symbol(
            &binder.symbols,
            &lib_symbol_ids,
            &retained_lib_symbols,
            |sym| source_ids.push(sym.id),
        );

        assert_eq!(source_ids, vec![shared_non_lib, local]);
        assert!(!source_ids.contains(&shared_lib));
    }
// TSZ_INLINE_TEST_END d25b4f2bc26db8766f1dfc2c7d196d139370fe96bce4465f1ad41744ad18fc85
