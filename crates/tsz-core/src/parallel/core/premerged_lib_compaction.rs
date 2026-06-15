fn for_each_densify_source_symbol(
    symbols: &SymbolArena,
    lib_symbol_ids: &FxHashSet<SymbolId>,
    retained_lib_symbols: &FxHashSet<SymbolId>,
    mut visit: impl FnMut(&crate::binder::Symbol),
) {
    if symbols.lib_prefix_is_pristine(lib_symbol_ids.len()) {
        let mut retained_prefix_ids: Vec<_> = retained_lib_symbols.iter().copied().collect();
        retained_prefix_ids.sort_unstable();
        for sym_id in retained_prefix_ids {
            if lib_symbol_ids.contains(&sym_id)
                && let Some(sym) = symbols.get(sym_id)
            {
                visit(sym);
            }
        }
        for sym in symbols.iter_private_symbols() {
            visit(sym);
        }
        return;
    }

    for sym in symbols
        .iter()
        .filter(|sym| !lib_symbol_ids.contains(&sym.id) || retained_lib_symbols.contains(&sym.id))
    {
        visit(sym);
    }
}

#[cfg(test)]
mod premerged_lib_compaction_tests {
    use super::*;

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
}
