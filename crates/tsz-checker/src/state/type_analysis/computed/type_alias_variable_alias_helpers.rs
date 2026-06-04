impl<'a> CheckerState<'a> {
    /// Resolve the type of a synthetic `export default interface A {}` /
    /// `export default type A = ...` alias.
    ///
    /// The binder models the inline default export of a pure type as an
    /// `ALIAS`-only `"default"` symbol whose declaration is the
    /// `INTERFACE_DECLARATION` / `TYPE_ALIAS_DECLARATION` node, but it carries no
    /// `value_declaration` (it is type-only). The same node also declares a local
    /// type symbol (`A`) in its file scope. We resolve the alias to that local
    /// symbol's type so a cross-file default import of the type
    /// (`import A from './a'; let x: A`) sees the interface/alias type rather than
    /// the generic alias-`any` fallback.
    ///
    /// The match is purely structural (binder symbol flags + declaration node
    /// kind); no identifier/file-name string is used as a decision predicate.
    fn inline_default_export_type_only_target_type(
        &mut self,
        alias_sym_id: tsz_binder::SymbolId,
        declarations: &[NodeIndex],
    ) -> Option<TypeId> {
        // Resolve the local interface/type-alias symbol first, holding only
        // immutable borrows of the declaring file's arena/binder. Drop those
        // borrows before calling `get_type_of_symbol` (which needs `&mut self`).
        let local_sym_id = {
            // The alias's declarations live in its declaring file's arena/binder,
            // not necessarily the current one (cross-file default import).
            let file_idx = self
                .ctx
                .resolve_symbol_declaring_file_index(alias_sym_id)
                .or_else(|| self.ctx.resolve_symbol_file_index(alias_sym_id));
            let arena = match file_idx {
                Some(idx) => self.ctx.get_arena_for_file(idx as u32),
                None => self.ctx.arena,
            };
            let binder = file_idx
                .and_then(|idx| self.ctx.get_binder_for_file(idx))
                .unwrap_or(self.ctx.binder);

            let mut found = None;
            for &decl_idx in declarations {
                let Some(decl_node) = arena.get(decl_idx) else {
                    continue;
                };
                // Only the synthetic default export carries an inline
                // interface/type declaration node as one of its declarations.
                let name_idx = if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                    arena.get_interface(decl_node).map(|iface| iface.name)
                } else if decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                    arena.get_type_alias(decl_node).map(|alias| alias.name)
                } else {
                    None
                };
                let Some(name_idx) = name_idx else {
                    continue;
                };
                let Some(name) = arena
                    .get(name_idx)
                    .and_then(|n| arena.get_identifier(n))
                    .map(|ident| ident.escaped_text.as_str())
                else {
                    continue;
                };

                // Find the local TYPE symbol the binder declared for this inline
                // declaration's name. It must be a real interface/type-alias
                // symbol, distinct from the `ALIAS`-only default-export symbol.
                let Some(candidate) = binder.file_locals.get(name) else {
                    continue;
                };
                if candidate == alias_sym_id {
                    continue;
                }
                if binder.get_symbol(candidate).is_some_and(|local| {
                    local.has_any_flags(symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS)
                }) {
                    found = Some(candidate);
                    break;
                }
            }
            found?
        };

        let target_type = self.get_type_of_symbol(local_sym_id);
        (target_type != TypeId::ERROR && target_type != TypeId::ANY).then_some(target_type)
    }

    fn type_node_contains_kind(&self, root: NodeIndex, kind: u16) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            if self
                .ctx
                .arena
                .get(idx)
                .is_some_and(|node| node.kind == kind)
            {
                return true;
            }
            stack.extend(self.ctx.arena.get_children(idx));
        }
        false
    }
}
