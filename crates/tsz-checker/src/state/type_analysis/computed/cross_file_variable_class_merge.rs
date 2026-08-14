//! Cross-file `<var> vs class` merged-value-type resolution.
//!
//! A `var`/`let`/`const` never declaration-merges with a class (only a
//! `function` declaration does — `FUNCTION_EXCLUDES` omits `CLASS`), so a
//! cross-file name collision between a block/function-scoped variable and a
//! class reports `TS2451`/`TS2300` (see `duplicate_identifiers.rs`). tsc
//! still resolves *every* value-position reference to that name across the
//! whole program from whichever declaration bound first into the shared
//! script-global symbol table (`mergeSymbol` in `checker.ts`: on a flag
//! conflict the pre-existing target symbol is left untouched and the
//! conflicting source's declarations are used only for the diagnostic).
//! File processing order decides the winner; declaration order within a
//! file does not. Verified against the pinned `typescript@7.0.2` oracle in
//! both TS-only and JS+`.d.ts` combinations, and in both file orders:
//! `declare class A {}` (a.d.ts) + `const A = {}` (b.js) resolves `A` as
//! `typeof A` (the class) everywhere; reversing the file order (variable's
//! file processed first) resolves `A` as the variable's own type instead.
//!
//! Scope: this covers ordinary (non-`checkJs`) property-type resolution.
//! `checkJs` JS files additionally run assignment-target expando-write
//! machinery (`property_access_helpers/expando.rs`) ahead of the generic
//! property-type path for a direct `A.d = {}` write; those eligibility gates
//! consult `variable_is_shadowed_by_earlier_class` (below) so a `.js` file's
//! `A.d = {}` following a conflicting earlier-file `declare class A {}`
//! reports `TS2339` too, matching the order-dependent rule above — but stays
//! a legitimate expando write when the variable's own file is processed
//! first.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Scan every file processed before `self.ctx.current_file_idx` for a
    /// `class` declaration sharing `sym_id`'s name, returning the winning
    /// class's `(file_idx, SymbolId)`. `&self`-only (no type computation or
    /// mutation) so callers that just need order-aware shadowing knowledge —
    /// like the checkJs expando eligibility gates in
    /// `property_access_helpers/expando.rs` — don't need `&mut self`.
    fn earlier_conflicting_class(&self, sym_id: SymbolId) -> Option<(usize, SymbolId)> {
        // Script-global merging only: an external module's top-level
        // variable is module-scoped and cannot collide with a class in a
        // different file at all.
        if self.ctx.binder.is_external_module() {
            return None;
        }
        let name = self.ctx.binder.get_symbol(sym_id)?.escaped_name.clone();

        let all_arenas = self.ctx.all_arenas.as_ref()?;
        let all_binders = self.ctx.all_binders.as_ref()?;
        let current_file_idx = self.ctx.current_file_idx;

        for (file_idx, binder) in all_binders.iter().enumerate() {
            if file_idx >= current_file_idx || binder.is_external_module() {
                continue;
            }
            let arena = all_arenas.get(file_idx)?;
            let Some(source_file) = arena.source_files.first() else {
                continue;
            };

            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                    continue;
                }
                let Some(class_decl) = arena.get_class(stmt_node) else {
                    continue;
                };
                let Some(ident) = arena.get_identifier_at(class_decl.name) else {
                    continue;
                };
                if ident.escaped_text != name {
                    continue;
                }
                let Some(class_sym_id) = binder.get_node_symbol(stmt_idx) else {
                    continue;
                };
                return Some((file_idx, class_sym_id));
            }
        }

        None
    }

    /// When `sym_id` is a script-scope variable whose name collides with a
    /// class declared in an earlier-processed file, return that class's own
    /// value type (`typeof <class>`) — the type every reference to the name
    /// resolves to on `main`, per tsc's first-bound-wins global merge. Returns
    /// `None` when there is no such earlier-processed remote class, letting
    /// the caller fall through to the variable's own initializer-derived type.
    pub(crate) fn cross_file_class_type_for_conflicting_variable(
        &mut self,
        sym_id: SymbolId,
        flags: u32,
    ) -> Option<TypeId> {
        if flags & symbol_flags::VARIABLE == 0 || flags & symbol_flags::CLASS != 0 {
            return None;
        }
        let (file_idx, class_sym_id) = self.earlier_conflicting_class(sym_id)?;
        self.ctx.register_symbol_file_index(class_sym_id, file_idx);
        Some(self.get_type_of_symbol(class_sym_id))
    }

    /// `&self`-safe existence check: does an earlier-processed file's `class`
    /// declaration shadow this (`CLASS`+`VARIABLE`-flagged merged) symbol's
    /// name? Per tsc's first-bound-wins global merge, the class only wins the
    /// value-position resolution — and only then does an expando write/read
    /// against the variable's own shape become illegitimate — when its file
    /// was processed before `sym_id`'s. See `cross_file_class_type_for_conflicting_variable`
    /// for the type-computing counterpart used on the pure-TS property-read path.
    pub(crate) fn variable_is_shadowed_by_earlier_class(&self, sym_id: SymbolId) -> bool {
        self.earlier_conflicting_class(sym_id).is_some()
    }
}
