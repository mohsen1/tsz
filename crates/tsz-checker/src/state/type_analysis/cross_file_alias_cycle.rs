//! Cross-file circular type-alias detection (TS2456).
//!
//! Cross-file alias bodies are resolved by delegating to a per-file child
//! checker (see [`super::cross_file`]). A cycle such as `type A = B` in one
//! module and `type B = A` in another would otherwise ping-pong between arenas
//! until the cross-arena depth guard collapses the type to `ERROR`, leaving no
//! alias flagged. This module tracks the aliases currently being resolved on a
//! delegation path (keyed by canonical `DefId`, since raw `SymbolId`s are
//! file-local) and, on re-entry, marks every alias in the cycle circular in the
//! shared `DefinitionStore`. Each file's `check_cross_file_circular_type_aliases`
//! post-pass then emits the TS2456 for its own member, applying the same
//! deferral/suppression rules tsc uses for same-file cycles.

use crate::state::CheckerState;
use tsz_binder::{Symbol, SymbolId, symbol_flags};
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::def::DefId;

thread_local! {
    /// Stack of type-alias `DefId`s currently being resolved through cross-arena
    /// delegation, in entry order. Thread-local because each file is checked on
    /// a single worker thread and the delegation recursion stays on that thread.
    static CROSS_ARENA_ALIAS_STACK: std::cell::RefCell<Vec<DefId>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// RAII guard returned by [`CheckerState::enter_cross_arena_alias`]. Pops the
/// pushed alias `DefId` on drop — including on panic unwind — so a stale entry
/// cannot poison later resolutions on a reused worker thread.
pub(crate) struct CrossArenaAliasGuard;

impl Drop for CrossArenaAliasGuard {
    fn drop(&mut self) {
        CROSS_ARENA_ALIAS_STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

impl<'a> CheckerState<'a> {
    /// Push a type-alias `DefId` onto the cross-arena alias-resolution stack for
    /// the duration of a cross-file delegation; the returned guard pops it on
    /// drop.
    pub(crate) fn enter_cross_arena_alias(def_id: DefId) -> CrossArenaAliasGuard {
        CROSS_ARENA_ALIAS_STACK.with(|stack| stack.borrow_mut().push(def_id));
        CrossArenaAliasGuard
    }

    /// Canonical `DefId` for `sym_id` when it is a delegatable cross-file type
    /// alias (not a class/interface, whose recursion is structurally valid), or
    /// `None` otherwise. The `DefId` is stable per declaring file + symbol, so
    /// it identifies the alias across child checker contexts.
    pub(crate) fn cross_arena_alias_def_id(&self, sym_id: SymbolId) -> Option<DefId> {
        self.get_cross_file_symbol(sym_id)
            .filter(|symbol| {
                symbol.has_any_flags(symbol_flags::TYPE_ALIAS)
                    && !symbol.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE)
            })
            .map(|_| self.ctx.get_or_create_def_id(sym_id))
            .filter(|def_id| *def_id != DefId::INVALID)
    }

    /// If `def_id` is already on the active cross-arena alias stack, the alias
    /// chain is circular: mark every member of the cycle (from that entry to the
    /// top of the stack) circular in the shared `DefinitionStore`. A no-op when
    /// there is no cycle. Marking only enables the per-file TS2456 post-pass; it
    /// does not change type resolution, so a legitimately recursive (non-cyclic
    /// per tsc) alias is unaffected — its own file never owns a circular member.
    pub(crate) fn mark_cross_arena_alias_cycle(&mut self, def_id: DefId) {
        let cycle_members = CROSS_ARENA_ALIAS_STACK.with(|stack| {
            let stack = stack.borrow();
            stack
                .iter()
                .position(|&d| d == def_id)
                .map(|idx| stack[idx..].to_vec())
        });
        if let Some(members) = cycle_members {
            for member in members {
                self.ctx.definition_store.mark_circular_def(member);
            }
        }
    }

    /// True when one of `symbol`'s declarations is a type-alias declaration that
    /// genuinely belongs to `sym_id` in the current file. Raw `SymbolId`s and
    /// `NodeIndex`es are file-local, so a cross-file alias can collide with an
    /// identically-positioned, identically-named alias in this arena (e.g. two
    /// modules with the same `export type T` shape). Ownership is confirmed
    /// through the current binder's node->symbol map; a name match alone is not
    /// enough to claim the declaration as local — that would suppress the
    /// cross-arena delegation a cross-file alias requires.
    pub(crate) fn symbol_has_local_type_alias_declaration(
        &self,
        symbol: &Symbol,
        sym_id: SymbolId,
    ) -> bool {
        symbol.declarations.iter().any(|&decl| {
            if self.ctx.binder.get_node_symbol(decl) != Some(sym_id) {
                return false;
            }
            self.ctx
                .arena
                .get(decl)
                .and_then(|node| {
                    if node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                        let type_alias = self.ctx.arena.get_type_alias(node)?;
                        let name_node = self.ctx.arena.get(type_alias.name)?;
                        let ident = self.ctx.arena.get_identifier(name_node)?;
                        let name = self.ctx.arena.resolve_identifier_text(ident);
                        Some(name == symbol.escaped_name.as_str())
                    } else {
                        Some(false)
                    }
                })
                .unwrap_or(false)
        })
    }
}
