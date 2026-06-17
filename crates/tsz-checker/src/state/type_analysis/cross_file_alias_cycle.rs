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
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::def::DefId;

/// An entry on the cross-arena alias-resolution stack: either a type-alias
/// `DefId` actively being resolved, or a class/interface instance-type
/// resolution boundary.
///
/// The boundary matters for cycle classification. A genuine TS2456 cross-file
/// alias cycle (`type A = B` in one module, `type B = A` in another) re-enters
/// an alias `DefId` directly through alias-body resolution. By contrast, a
/// generic alias such as `type Result<T,E> = Ok<T,E> | Err<T,E>` whose union
/// members are classes that (transitively, cross-arena) reference the alias in
/// their method signatures is NOT circular per tsc: the class instance types
/// defer, breaking the cycle. Those re-entries reach the alias `DefId` only
/// *through* a class instance-type resolution. Recording the class boundary lets
/// [`CheckerState::mark_cross_arena_alias_cycle`] distinguish the two: a re-entry
/// whose path crosses a class boundary is structurally valid recursion, not an
/// alias cycle, so it is not marked circular (which would otherwise collapse the
/// alias to `ERROR` at cross-file references; see
/// `type_reference_alias_collapsed_to_error`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CrossArenaAliasEntry {
    Alias(DefId),
    ClassBoundary,
}

thread_local! {
    /// Stack of cross-arena alias-resolution entries (aliases and class
    /// boundaries) currently active through cross-arena delegation, in entry
    /// order. Thread-local because each file is checked on a single worker
    /// thread and the delegation recursion stays on that thread.
    static CROSS_ARENA_ALIAS_STACK: std::cell::RefCell<Vec<CrossArenaAliasEntry>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Clear the cross-arena alias-resolution stack.
///
/// `CrossArenaAliasGuard` already pops on scope exit (including panic unwind),
/// so this stack is normally empty between compilations. It is reset anyway at
/// batch row boundaries to make row isolation total: any leftover entries pin
/// arena-scoped `DefId`s, so dropping them releases per-row memory and
/// guarantees a clean DFS even if a future non-unwinding bail-out path is added.
pub(crate) fn reset_cross_arena_alias_stack() {
    CROSS_ARENA_ALIAS_STACK.with(|stack| stack.borrow_mut().clear());
}

#[cfg(test)]
pub(crate) fn push_cross_arena_alias_for_test(def_id: DefId) {
    CROSS_ARENA_ALIAS_STACK
        .with(|stack| stack.borrow_mut().push(CrossArenaAliasEntry::Alias(def_id)));
}

#[cfg(test)]
pub(crate) fn cross_arena_alias_stack_len_for_test() -> usize {
    CROSS_ARENA_ALIAS_STACK.with(|stack| stack.borrow().len())
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
        CROSS_ARENA_ALIAS_STACK
            .with(|stack| stack.borrow_mut().push(CrossArenaAliasEntry::Alias(def_id)));
        CrossArenaAliasGuard
    }

    /// Push a class/interface instance-type resolution boundary onto the
    /// cross-arena alias-resolution stack for the duration of a cross-file class
    /// instance-type delegation; the returned guard pops it on drop. A re-entry
    /// of an alias `DefId` reached through this boundary defers like tsc and is
    /// not a TS2456 cycle (see [`CrossArenaAliasEntry`]).
    pub(crate) fn enter_cross_arena_class_boundary() -> CrossArenaAliasGuard {
        CROSS_ARENA_ALIAS_STACK
            .with(|stack| stack.borrow_mut().push(CrossArenaAliasEntry::ClassBoundary));
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

    /// If `def_id` is already on the active cross-arena alias stack AND no class
    /// instance-type boundary intervenes between that entry and the top of the
    /// stack, the alias chain is a genuine TS2456 cycle: mark every alias member
    /// of the cycle circular in the shared `DefinitionStore`. A no-op when there
    /// is no cycle, or when the re-entry path crossed a class boundary (the class
    /// instance type defers like tsc, so the recursion is structurally valid —
    /// e.g. `type Result<T,E> = Ok<T,E> | Err<T,E>` whose members reference the
    /// alias only through their class method signatures). Marking only enables
    /// the per-file TS2456 post-pass; it does not change type resolution, so a
    /// legitimately recursive (non-cyclic per tsc) alias is unaffected.
    pub(crate) fn mark_cross_arena_alias_cycle(&mut self, def_id: DefId) {
        let cycle_aliases = CROSS_ARENA_ALIAS_STACK.with(|stack| {
            let stack = stack.borrow();
            let start = stack
                .iter()
                .position(|&e| e == CrossArenaAliasEntry::Alias(def_id))?;
            // A class instance-type resolution between the matched alias and the
            // current re-entry breaks the alias cycle (tsc defers class types).
            // Only an uninterrupted alias-to-alias chain is a TS2456 cycle.
            if stack[start..].contains(&CrossArenaAliasEntry::ClassBoundary) {
                return None;
            }
            Some(
                stack[start..]
                    .iter()
                    .filter_map(|&e| match e {
                        CrossArenaAliasEntry::Alias(d) => Some(d),
                        CrossArenaAliasEntry::ClassBoundary => None,
                    })
                    .collect::<Vec<_>>(),
            )
        });
        if let Some(members) = cycle_aliases {
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
        if let Some(owner_file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && owner_file_idx != self.ctx.current_file_idx
            && let Some(owner_arena) = self
                .ctx
                .all_arenas
                .as_ref()
                .and_then(|arenas| arenas.get(owner_file_idx))
            && symbol.declarations.iter().any(|&decl| {
                owner_arena
                    .get(decl)
                    .and_then(|node| {
                        (node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION)
                            .then(|| owner_arena.get_type_alias(node))
                            .flatten()
                    })
                    .and_then(|type_alias| owner_arena.get_identifier_text(type_alias.name))
                    .is_some_and(|name| name == symbol.escaped_name)
            })
        {
            return false;
        }

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

    /// True when a cross-file type alias `sym_id` (resolved in another arena)
    /// has a declaration whose body is a conditional type whose `extends`
    /// operand is a bare named type reference (no type arguments).
    ///
    /// This is the structural signature of the #13618 family: an exported
    /// generic conditional alias `A<T> = T extends Ref ? X : Y` whose `extends`
    /// operand (`Ref`) is a provider-private (non-exported) named type. Such a
    /// body only binds correctly when lowered in its declaring arena, because
    /// `Ref` is invisible in the consumer scope; it must be delegated
    /// cross-arena rather than re-lowered in the consumer.
    ///
    /// The gate is deliberately tight, because the merged change widened
    /// delegation for *all* imported aliases (#13618) and that regressed cross-
    /// arena conditional helper aliases whose conditional depends on the
    /// caller's type argument rather than a provider-private name — e.g. React's
    /// `type PropsWithRef<P> = 'ref' extends keyof P ? ...` and
    /// `type ElementType<P> = ... P extends JSX.IntrinsicElements[K] ? ...`,
    /// which must keep instantiating `P` in the consumer arena. Those extends
    /// operands are `keyof P`, an indexed access, a type literal, a keyword, or a
    /// *parameterized* reference (`Validator<infer T>`) — never a bare named
    /// reference — so requiring a bare `TYPE_REFERENCE` with no type arguments
    /// admits the #13618 family and excludes the library JSX helpers. The lookup
    /// mirrors `symbol_has_local_type_alias_declaration`: it reads the alias
    /// declaration in its owning arena and inspects node kinds only.
    pub(crate) fn cross_file_alias_body_is_private_extends_conditional(
        &self,
        sym_id: SymbolId,
    ) -> bool {
        let Some(owner_file_idx) = self.ctx.resolve_symbol_file_index(sym_id) else {
            return false;
        };
        let Some(owner_arena) = self
            .ctx
            .all_arenas
            .as_ref()
            .and_then(|arenas| arenas.get(owner_file_idx))
        else {
            return false;
        };
        let Some(owner_binder) = self.ctx.get_binder_for_file(owner_file_idx) else {
            return false;
        };
        let Some(symbol) = owner_binder.get_symbol(sym_id) else {
            return false;
        };
        symbol.declarations.iter().any(|&decl| {
            let Some(type_alias) = owner_arena
                .get(decl)
                .filter(|node| node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION)
                .and_then(|node| owner_arena.get_type_alias(node))
            else {
                return false;
            };
            let Some(body) = owner_arena.get(type_alias.type_node) else {
                return false;
            };
            if body.kind != syntax_kind_ext::CONDITIONAL_TYPE {
                return false;
            }
            let Some(conditional) = owner_arena.get_conditional_type(body) else {
                return false;
            };
            owner_arena
                .get(conditional.extends_type)
                .filter(|extends| extends.kind == syntax_kind_ext::TYPE_REFERENCE)
                .and_then(|extends| owner_arena.get_type_ref(extends))
                .is_some_and(|type_ref| type_ref.type_arguments.is_none())
        })
    }
}
