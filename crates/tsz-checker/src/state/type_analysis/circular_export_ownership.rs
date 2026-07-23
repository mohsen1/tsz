//! Current-file ownership recovery for plain value variables.
//!
//! Raw `SymbolId`s are binder-local. A current file's `const`/`let`/`var` can
//! therefore share its numeric id with a foreign symbol selected by an owner
//! index or export overlay. An identifier lookup that proves current-binder
//! provenance can therefore resolve that variable through the local type path.
//!
//! [`super::core`] also consults
//! [`value_variable_owned_by_current_file_not_foreign`] for the narrower
//! wildcard re-export-cycle recovery that predates the provenance-aware path.
//!
//! [`value_variable_owned_by_current_file_not_foreign`]:
//! CheckerState::value_variable_owned_by_current_file_not_foreign

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    fn binder_declares_variable_symbol(
        binder: &tsz_binder::BinderState,
        arena: &tsz_parser::parser::node::NodeArena,
        symbol: &tsz_binder::Symbol,
        sym_id: SymbolId,
    ) -> bool {
        symbol
            .declarations
            .iter()
            .chain(std::iter::once(&symbol.value_declaration))
            .any(|&decl| {
                !decl.is_none()
                    && binder.get_node_symbol(decl) == Some(sym_id)
                    && arena
                        .get(decl)
                        .is_some_and(|node| node.kind == syntax_kind_ext::VARIABLE_DECLARATION)
            })
    }

    /// Prove that a raw symbol id denotes a plain variable in the current
    /// binder. Callers with explicit current-binder lookup provenance can use
    /// this before selecting the local symbol-type path.
    pub(crate) fn current_file_owns_plain_value_variable(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let disqualifying = symbol_flags::ALIAS
            | symbol_flags::TYPE_ALIAS
            | symbol_flags::INTERFACE
            | symbol_flags::CLASS
            | symbol_flags::FUNCTION
            | symbol_flags::ENUM
            | symbol_flags::NAMESPACE_MODULE
            | symbol_flags::VALUE_MODULE;
        symbol.flags & symbol_flags::VARIABLE != 0
            && symbol.flags & disqualifying == 0
            && Self::binder_declares_variable_symbol(
                self.ctx.binder,
                self.ctx.arena,
                symbol,
                sym_id,
            )
    }

    /// `true` when `sym_id` is a plain value variable (`const`/`let`/`var`)
    /// genuinely declared in the file currently being checked, while an owner
    /// index claims a foreign binder-local symbol with the same raw id.
    ///
    /// Scoped narrowly on purpose — only value variables with a real
    /// `VariableDeclaration` node that the current binder maps back to this exact
    /// `SymbolId` (the `get_node_symbol` round-trip rejects cross-binder
    /// `SymbolId` collisions and import/alias bindings). It deliberately does
    /// not touch interfaces, classes, type aliases, functions, enums,
    /// namespaces, or import aliases, whose cross-file ownership the resolution
    /// pipeline relies on.
    pub(super) fn value_variable_owned_by_current_file_not_foreign(
        &self,
        sym_id: SymbolId,
        foreign_owner_idx: usize,
    ) -> bool {
        if !self.current_file_owns_plain_value_variable(sym_id)
            || !self.foreign_file_wildcard_reexports_current_file(foreign_owner_idx)
        {
            return false;
        }
        self.ctx
            .all_binders
            .as_ref()
            .and_then(|binders| binders.get(foreign_owner_idx).cloned())
            .zip(
                self.ctx
                    .all_arenas
                    .as_ref()
                    .and_then(|arenas| arenas.get(foreign_owner_idx).cloned()),
            )
            .is_none_or(|(binder, arena)| {
                binder.get_symbol(sym_id).is_none_or(|symbol| {
                    !Self::binder_declares_variable_symbol(&binder, &arena, symbol, sym_id)
                })
            })
    }

    fn foreign_file_wildcard_reexports_current_file(&self, foreign_idx: usize) -> bool {
        let Some(foreign_binder) = self.ctx.get_binder_for_file(foreign_idx) else {
            return false;
        };
        let Some(foreign_file_name) = self
            .ctx
            .get_arena_for_file(foreign_idx as u32)
            .source_files
            .first()
            .map(|source_file| source_file.file_name.clone())
        else {
            return false;
        };
        let Some(wildcards) = self
            .ctx
            .wildcard_reexports_for_file(foreign_binder, &foreign_file_name)
        else {
            return false;
        };
        wildcards.iter().any(|(module, _)| {
            self.ctx
                .resolve_import_target_from_file(foreign_idx, module)
                == Some(self.ctx.current_file_idx)
        })
    }
}
