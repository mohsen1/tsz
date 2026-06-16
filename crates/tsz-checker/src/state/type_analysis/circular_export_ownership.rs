//! Current-file ownership recovery for `export *` re-export cycles.
//!
//! When a file's own `export const`/`let`/`var` symbols are re-exported back to
//! it through an `export *` cycle (e.g. `internal.ts` does
//! `export * from "./common"` while `common.ts` imports from `./internal`), the
//! namespace-export stamp can register the current file's value symbols against
//! the re-exporting file. Delegating to that foreign arena — which has no
//! concrete declaration for the const — would collapse its value type to `any`
//! (false `TS7053` on `obj[K]`, masking a real `TS2322`). `tsc` resolves the
//! const to its declared literal everywhere.
//!
//! [`super::core`] consults
//! [`value_variable_owned_by_current_file_not_foreign`] before honoring a
//! cross-file owner so these mis-attributed value symbols resolve locally.
//!
//! [`value_variable_owned_by_current_file_not_foreign`]:
//! CheckerState::value_variable_owned_by_current_file_not_foreign

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// `true` when `sym_id` is a plain value variable (`const`/`let`/`var`)
    /// genuinely declared in the file currently being checked, while the dynamic
    /// cross-file overlay claims a *foreign* owner that does not actually declare
    /// it (a re-export passthrough).
    ///
    /// Scoped narrowly on purpose — only value variables with a real
    /// `VariableDeclaration` node that the current binder maps back to this exact
    /// `SymbolId` (the `get_node_symbol` round-trip rejects cross-binder
    /// `SymbolId` collisions and import/alias bindings). This is the single shape
    /// that an `export *` re-export cycle mis-attributes; it deliberately does
    /// not touch interfaces, classes, type aliases, functions, enums, namespaces,
    /// or import aliases, whose cross-file ownership the resolution pipeline
    /// relies on.
    pub(super) fn value_variable_owned_by_current_file_not_foreign(
        &self,
        sym_id: SymbolId,
        foreign_owner_idx: usize,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        // Only plain value variables; reject anything that legitimately resolves
        // cross-file (aliases, types, callables, namespaces).
        let disqualifying = symbol_flags::ALIAS
            | symbol_flags::TYPE_ALIAS
            | symbol_flags::INTERFACE
            | symbol_flags::CLASS
            | symbol_flags::FUNCTION
            | symbol_flags::ENUM
            | symbol_flags::NAMESPACE_MODULE
            | symbol_flags::VALUE_MODULE;
        if symbol.flags & symbol_flags::VARIABLE == 0 || symbol.flags & disqualifying != 0 {
            return false;
        }
        let declares_variable_in =
            |binder: &tsz_binder::BinderState,
             arena: &tsz_parser::parser::node::NodeArena,
             symbol: &tsz_binder::Symbol| {
                symbol
                    .declarations
                    .iter()
                    .chain(std::iter::once(&symbol.value_declaration))
                    .any(|&decl| {
                        !decl.is_none()
                            && binder.get_node_symbol(decl) == Some(sym_id)
                            && arena.get(decl).is_some_and(|node| {
                                node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                            })
                    })
            };
        // The symbol must have a real variable declaration in the current arena
        // that the current binder maps back to exactly this `SymbolId`.
        if !declares_variable_in(self.ctx.binder, self.ctx.arena, symbol) {
            return false;
        }
        // Only fire under a genuine `export *` cycle: the foreign owner must
        // wildcard-re-export a module that resolves back to the current file.
        // That is precisely the shape that mis-stamps the current file's own
        // `export const` symbols onto the re-exporting file; it excludes ordinary
        // cross-file value imports and CommonJS namespace publication, whose
        // cross-file ownership must be preserved.
        if !self.foreign_file_wildcard_reexports_current_file(foreign_owner_idx) {
            return false;
        }
        // The claimed foreign owner must NOT actually declare the symbol — it only
        // re-exports it. If the foreign binder genuinely owns a same-named
        // variable declaration, keep the cross-file path (genuine cross-file
        // const import).
        let foreign_declares = self
            .ctx
            .all_binders
            .as_ref()
            .and_then(|binders| binders.get(foreign_owner_idx).cloned())
            .zip(
                self.ctx
                    .all_arenas
                    .as_ref()
                    .and_then(|arenas| arenas.get(foreign_owner_idx).cloned()),
            )
            .is_some_and(|(binder, arena)| {
                binder
                    .get_symbol(sym_id)
                    .is_some_and(|s| declares_variable_in(&binder, &arena, s))
            });
        !foreign_declares
    }

    /// `true` when `foreign_idx` declares a wildcard re-export
    /// (`export * from "..."`) whose module specifier resolves to the file
    /// currently being checked — i.e. the back-edge of an `export *` import
    /// cycle through the current file.
    fn foreign_file_wildcard_reexports_current_file(&self, foreign_idx: usize) -> bool {
        let Some(foreign_binder) = self.ctx.get_binder_for_file(foreign_idx) else {
            return false;
        };
        let Some(foreign_file_name) = self
            .ctx
            .get_arena_for_file(foreign_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return false;
        };
        let Some(wildcards) = self
            .ctx
            .wildcard_reexports_for_file(foreign_binder, &foreign_file_name)
        else {
            return false;
        };
        wildcards.iter().any(|(source_module, _is_type_only)| {
            self.ctx
                .resolve_import_target_from_file(foreign_idx, source_module)
                == Some(self.ctx.current_file_idx)
        })
    }
}
