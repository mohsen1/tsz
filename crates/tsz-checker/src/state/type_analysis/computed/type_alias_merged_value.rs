//! Merged type-alias/value cache helpers.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn compute_value_type_for_merged_alias(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut decl = symbol.value_declaration;

        if let Some(decl_node) = self.ctx.arena.get(decl)
            && decl_node.kind == SyntaxKind::Identifier as u16
        {
            decl = self.ctx.arena.get_extended(decl)?.parent;
        }

        let decl_node = self.ctx.arena.get(decl)?;
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;

        if var_decl.type_annotation.is_some() {
            let ann_type = self.get_type_from_type_node(var_decl.type_annotation);
            // A `const X: unique symbol` declaration carries a `unique symbol` value
            // identity (`typeof X`), even when `X` is name-merged with a same-named
            // type alias. Apply the same upgrade `get_type_of_variable_declaration`
            // uses so the merged value type keeps the symbol's own identity instead
            // of degrading to the general `symbol` type.
            let ann_type =
                self.const_unique_symbol_value_type(decl, var_decl.type_annotation, ann_type);
            if ann_type != TypeId::ERROR && ann_type != TypeId::ANY {
                return Some(ann_type);
            }
        }

        // `const X = Symbol()` / `const X = Symbol.for(...)` name-merged with a
        // same-named `type X = typeof X` alias: keep the `unique symbol` value
        // identity instead of the wide `symbol` the factory call returns, so the
        // merged value cache (and `typeof X`) agrees with the symbol's own
        // identity.
        if let Some(unique) = self.const_symbol_factory_unique_value_type(decl) {
            return Some(unique);
        }

        if var_decl.initializer.is_some() {
            let init_type = self.get_type_of_node(var_decl.initializer);
            if init_type != TypeId::ERROR && init_type != TypeId::UNKNOWN {
                return Some(init_type);
            }
        }

        None
    }

    /// Resolve the VALUE type of a named import whose export resolves (possibly
    /// through intermediate re-export/import aliases) to a symbol that merges a
    /// `const`/`function`/`var` value with a same-named `type X = ...` alias.
    ///
    /// On a `TYPE_ALIAS`+`VALUE` merge with a computable value type, caches the alias
    /// body under `alias_sym_id` for type-position consumers, applies module
    /// augmentations, and returns the value type. Returns `None` otherwise, so
    /// the caller falls through to its existing alias/interface/value handling.
    ///
    /// The value type is computed in the declaration's own arena so it survives
    /// cross-file re-export hops, where `get_type_of_symbol` would instead return
    /// the type-alias body (`typeof X`) — which collapses to `error` across the
    /// extra hop. Mirrors the `import X = NS.Foo` `TYPE_ALIAS`+`VALUE` branch.
    pub(crate) fn imported_merged_type_alias_value_type(
        &mut self,
        export_sym_id: SymbolId,
        alias_sym_id: SymbolId,
        module_name: &str,
        export_name: &str,
    ) -> Option<TypeId> {
        use crate::symbols_domain::alias_cycle::AliasCycleTracker;

        // Follow re-export / import alias hops to the ultimate declaration so a
        // chain like `pattern.ts -> patterns.ts (re-export) -> symbols.ts (const +
        // type)` lands on the merged declaration rather than an intermediate alias.
        let mut visited = AliasCycleTracker::new();
        let ultimate = self
            .resolve_alias_symbol(export_sym_id, &mut visited)
            .unwrap_or(export_sym_id);

        let symbol = self.get_symbol_globally(ultimate)?;
        let has_type_alias = symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS);
        let has_value = symbol.has_any_flags(
            tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE
                | tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE
                | tsz_binder::symbol_flags::FUNCTION,
        );
        let value_decl = symbol.value_declaration;
        if !has_type_alias || !has_value || value_decl.is_none() {
            return None;
        }

        // Compute the value declaration's type in the arena that owns it. The
        // declaration `NodeIndex` is arena-relative, so a same-file symbol must
        // use the current arena while a cross-file symbol must spawn a child
        // checker on the owning file (a raw `NodeIndex` reused against the wrong
        // arena would resolve an unrelated node).
        let owning_file = self.ctx.resolve_symbol_file_index(ultimate);
        let value_type = if owning_file == Some(self.ctx.current_file_idx) {
            self.type_of_value_declaration_for_symbol(ultimate, value_decl)
        } else {
            self.cross_file_value_declaration_type(ultimate, value_decl)
                .unwrap_or(TypeId::ERROR)
        };
        if matches!(value_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        // The alias body is the type-position meaning; cache it best-effort so
        // type-position uses of the import keep resolving the alias. Skip caching
        // when it can't be computed cleanly (it must never leak into value space).
        let ta = self.get_type_of_symbol(ultimate);
        if ta != TypeId::ERROR && ta != TypeId::UNKNOWN {
            self.ctx.import_type_alias_types.insert(alias_sym_id, ta);
        }

        Some(self.apply_module_augmentations(module_name, export_name, value_type))
    }

    pub(crate) fn merged_alias_value_decl_refs_type_alias(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let mut decl = symbol.value_declaration;

        if let Some(decl_node) = self.ctx.arena.get(decl)
            && decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(decl)
        {
            decl = ext.parent;
        }

        let Some(decl_node) = self.ctx.arena.get(decl) else {
            return false;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };

        (var_decl.type_annotation.is_some()
            && self.type_position_subtree_refs_symbol(var_decl.type_annotation, sym_id))
            || (var_decl.initializer.is_some()
                && self.type_position_subtree_refs_symbol(var_decl.initializer, sym_id))
    }

    fn type_position_subtree_refs_symbol(&self, root: NodeIndex, sym_id: SymbolId) -> bool {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };

            let lookup_target = match node.kind {
                k if k == syntax_kind_ext::TYPE_REFERENCE => {
                    self.ctx.arena.get_type_ref(node).map(|tr| tr.type_name)
                }
                k if k == syntax_kind_ext::TYPE_QUERY => {
                    self.ctx.arena.get_type_query(node).map(|tq| tq.expr_name)
                }
                _ => None,
            };
            if let Some(target) = lookup_target
                && self.resolve_type_symbol_for_lowering(target) == Some(sym_id.0)
            {
                return true;
            }

            stack.extend(self.ctx.arena.get_children(idx));
        }
        false
    }
}
