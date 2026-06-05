impl<'a> CheckerState<'a> {
    pub(super) fn source_file_symbol_has_local_type_declaration(
        arena: &NodeArena,
        binder: &BinderState,
        sym_id: SymbolId,
        symbol: &Symbol,
    ) -> bool {
        symbol.declarations.iter().copied().any(|decl_idx| {
            let is_type_decl = |decl_arena: &NodeArena| {
                decl_arena.get(decl_idx).is_some_and(|decl| {
                    matches!(
                        decl.kind,
                        k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                            || k == syntax_kind_ext::INTERFACE_DECLARATION
                            || k == syntax_kind_ext::CLASS_DECLARATION
                            || k == syntax_kind_ext::ENUM_DECLARATION
                    )
                })
            };
            if let Some(arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                return arenas.iter().any(|decl_arena| {
                    std::ptr::eq(decl_arena.as_ref(), arena) && is_type_decl(decl_arena.as_ref())
                });
            }
            is_type_decl(arena)
        })
    }
}
