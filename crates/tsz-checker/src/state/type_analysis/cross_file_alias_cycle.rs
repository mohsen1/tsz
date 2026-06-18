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
use rustc_hash::FxHashSet;
use std::sync::Arc;
use tsz_binder::{BinderState, Symbol, SymbolId, SymbolTable, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
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

    /// True when a cross-file type alias `sym_id` (resolved in another arena)
    /// has a declaration whose body references a *provider-private* named type:
    /// a `type`/`interface`/`class`/`enum` declared in the alias's own module and
    /// **not exported** from it.
    ///
    /// This is the general structural signature of the #13618 family, of which
    /// [`Self::cross_file_alias_body_is_private_extends_conditional`] is the
    /// narrow top-level-`extends` special case. A provider-private name is
    /// invisible in the consumer scope, so re-lowering the imported body there
    /// leaves the reference an `UnresolvedTypeName`: a conditional degrades to
    /// the wrong branch / a both-branches union, and a mapped type's key filter
    /// (`as K extends Priv ? …`) or value (`Priv[K]`) fails to settle, producing
    /// spurious `TS2322`/`TS2741`. Lowering the body in its declaring arena binds
    /// the private reference, exactly as `tsc` resolves a type reference in the
    /// module where it textually appears.
    ///
    /// This predicate is purely *additive* to the narrow conditional gate (the
    /// caller delegates when **either** is true): every name it newly admits is a
    /// **non-exported** provider-local declaration, which can never be a library's
    /// public conditional/mapped helper (those are exported) nor a global
    /// (globals do not resolve in the provider module's own scope, so
    /// `resolve_name_with_filter` returns `None` for them). Library helpers such
    /// as React's `PropsWithRef`/`ElementType` reference only the caller's type
    /// parameter, globals (`JSX`), or *exported* react types, so this never fires
    /// for them — preserving the consumer-arena instantiation they require. The
    /// alias's own type parameters are excluded by name. The scan reads node kinds
    /// and the provider binder's own symbol tables only; no identifier, file-name,
    /// or rendered-type string drives the decision.
    pub(crate) fn cross_file_alias_body_references_provider_private_type(
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
        let Ok(owner_file_idx_u32) = u32::try_from(owner_file_idx) else {
            return false;
        };
        let Some(owner_binder) = self.ctx.get_binder_for_file(owner_file_idx) else {
            return false;
        };
        let Some(symbol) = owner_binder.get_symbol(sym_id) else {
            return false;
        };
        // Names exported by the alias's own module are consumer-visible (the
        // consumer can import them), so a reference to one binds in the consumer
        // arena and does not require delegation. Read the export table from the
        // alias's enclosing module symbol.
        let module_exports = owner_binder
            .get_symbol(symbol.parent)
            .and_then(|module| module.exports.as_deref());
        let lib_binders = self.ctx.lib_binders_cached.clone();
        symbol.declarations.iter().any(|&decl| {
            let Some(type_alias) = owner_arena
                .get(decl)
                .filter(|node| node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION)
                .and_then(|node| owner_arena.get_type_alias(node))
            else {
                return false;
            };
            let mut tp_names: FxHashSet<String> = FxHashSet::default();
            if let Some(type_params) = type_alias.type_parameters.as_ref() {
                for &tp_idx in &type_params.nodes {
                    if let Some(tp) = owner_arena
                        .get(tp_idx)
                        .and_then(|node| owner_arena.get_type_parameter(node))
                        && let Some(name_node) = owner_arena.get(tp.name)
                        && let Some(ident) = owner_arena.get_identifier(name_node)
                    {
                        tp_names.insert(ident.escaped_text.clone());
                    }
                }
            }
            let scan = ProviderPrivateTypeScan {
                arena: owner_arena,
                binder: owner_binder,
                lib_binders: &lib_binders,
                owner_file_idx: owner_file_idx_u32,
                module_exports,
                tp_names: &tp_names,
            };
            scan.body_has_private_extends_operand(type_alias.type_node, 0)
        })
    }
}

/// Maximum type-node nesting the provider-private reference scan descends
/// before giving up. A genuine alias body is only a handful of levels deep;
/// the cap bounds work on a pathological/recursive body without affecting the
/// decision (an undetected deep reference simply keeps the prior, non-delegated
/// behavior — the scan never produces a false positive from bailing out).
const PROVIDER_PRIVATE_SCAN_MAX_DEPTH: u32 = 64;

/// Read-only scan over a cross-file alias body, in its *declaring* arena, that
/// reports whether the body references a provider-private named type. Bundling
/// the borrowed context keeps the recursive walk to two parameters.
struct ProviderPrivateTypeScan<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    lib_binders: &'a [Arc<BinderState>],
    owner_file_idx: u32,
    module_exports: Option<&'a SymbolTable>,
    tp_names: &'a FxHashSet<String>,
}

impl ProviderPrivateTypeScan<'_> {
    /// True when `name`, resolved in the provider module's own scope, binds to a
    /// non-exported `type`/`interface`/`class`/`enum` declared in that module.
    fn name_is_provider_private(&self, name: &str, name_node: NodeIndex) -> bool {
        if self.tp_names.contains(name) {
            return false;
        }
        if self.module_exports.is_some_and(|exports| exports.has(name)) {
            return false;
        }
        self.binder
            .resolve_name_with_filter(name, self.arena, name_node, self.lib_binders, |sym| {
                self.binder.symbols.get(sym).is_some_and(|resolved| {
                    resolved.has_any_flags(
                        symbol_flags::TYPE_ALIAS
                            | symbol_flags::INTERFACE
                            | symbol_flags::CLASS
                            | symbol_flags::ENUM,
                    ) && !resolved.has_any_flags(symbol_flags::TYPE_PARAMETER)
                        && (resolved.decl_file_idx == self.owner_file_idx
                            || resolved
                                .stable_declarations
                                .iter()
                                .any(|stable| stable.file_idx == self.owner_file_idx))
                })
            })
            .is_some()
    }

    /// Search `node_idx` for conditional types whose `extends` operand
    /// references a provider-private type. Provider-private references in
    /// branch/result positions are not enough to delegate: library helpers such
    /// as React's `LibraryManagedAttributes` compose private branch helpers but
    /// still require consumer-arena instantiation of their caller-provided type
    /// parameter. The unsafe-to-lower-in-consumer shape is the conditional test
    /// itself depending on a provider-private type.
    fn body_has_private_extends_operand(&self, node_idx: NodeIndex, depth: u32) -> bool {
        if node_idx == NodeIndex::NONE || depth > PROVIDER_PRIVATE_SCAN_MAX_DEPTH {
            return false;
        }
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        let next = depth + 1;
        match node.kind {
            syntax_kind_ext::CONDITIONAL_TYPE => {
                self.arena.get_conditional_type(node).is_some_and(|cond| {
                    self.body_references_private(cond.extends_type, next)
                        || self.body_has_private_extends_operand(cond.check_type, next)
                        || self.body_has_private_extends_operand(cond.true_type, next)
                        || self.body_has_private_extends_operand(cond.false_type, next)
                })
            }
            syntax_kind_ext::TYPE_REFERENCE => {
                self.arena.get_type_ref(node).is_some_and(|type_ref| {
                    type_ref.type_arguments.as_ref().is_some_and(|args| {
                        args.nodes
                            .iter()
                            .any(|&arg| self.body_has_private_extends_operand(arg, next))
                    })
                })
            }
            syntax_kind_ext::UNION_TYPE | syntax_kind_ext::INTERSECTION_TYPE => self
                .arena
                .get_composite_type(node)
                .is_some_and(|composite| {
                    composite
                        .types
                        .nodes
                        .iter()
                        .any(|&member| self.body_has_private_extends_operand(member, next))
                }),
            syntax_kind_ext::ARRAY_TYPE => self.arena.get_array_type(node).is_some_and(|array| {
                self.body_has_private_extends_operand(array.element_type, next)
            }),
            syntax_kind_ext::TUPLE_TYPE => self.arena.get_tuple_type(node).is_some_and(|tuple| {
                tuple
                    .elements
                    .nodes
                    .iter()
                    .any(|&elem| self.body_has_private_extends_operand(elem, next))
            }),
            syntax_kind_ext::PARENTHESIZED_TYPE
            | syntax_kind_ext::OPTIONAL_TYPE
            | syntax_kind_ext::REST_TYPE => {
                self.arena.get_wrapped_type(node).is_some_and(|wrapped| {
                    self.body_has_private_extends_operand(wrapped.type_node, next)
                })
            }
            syntax_kind_ext::NAMED_TUPLE_MEMBER => self
                .arena
                .get_named_tuple_member(node)
                .is_some_and(|member| {
                    self.body_has_private_extends_operand(member.type_node, next)
                }),
            syntax_kind_ext::INFER_TYPE => self.arena.get_infer_type(node).is_some_and(|infer| {
                self.type_parameter_has_private_extends_operand(infer.type_parameter, next)
            }),
            syntax_kind_ext::TYPE_OPERATOR => self
                .arena
                .get_type_operator(node)
                .is_some_and(|op| self.body_has_private_extends_operand(op.type_node, next)),
            syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                self.arena.get_indexed_access_type(node).is_some_and(|ia| {
                    self.body_has_private_extends_operand(ia.object_type, next)
                        || self.body_has_private_extends_operand(ia.index_type, next)
                })
            }
            syntax_kind_ext::MAPPED_TYPE => {
                self.arena.get_mapped_type(node).is_some_and(|mapped| {
                    self.type_parameter_has_private_extends_operand(mapped.type_parameter, next)
                        || self.body_has_private_extends_operand(mapped.name_type, next)
                        || self.body_has_private_extends_operand(mapped.type_node, next)
                })
            }
            syntax_kind_ext::TYPE_LITERAL => self.arena.get_type_literal(node).is_some_and(|lit| {
                lit.members
                    .nodes
                    .iter()
                    .any(|&member| self.member_has_private_extends_operand(member, next))
            }),
            syntax_kind_ext::FUNCTION_TYPE | syntax_kind_ext::CONSTRUCTOR_TYPE => {
                self.arena.get_function_type(node).is_some_and(|func| {
                    self.body_has_private_extends_operand(func.type_annotation, next)
                        || self.params_have_private_extends_operand(&func.parameters.nodes, next)
                })
            }
            syntax_kind_ext::TEMPLATE_LITERAL_TYPE => self
                .arena
                .get_template_literal_type(node)
                .is_some_and(|template| {
                    template.template_spans.nodes.iter().any(|&span_idx| {
                        self.arena
                            .get(span_idx)
                            .and_then(|span_node| self.arena.get_template_span(span_node))
                            .is_some_and(|span| {
                                self.body_has_private_extends_operand(span.expression, next)
                            })
                    })
                }),
            syntax_kind_ext::TYPE_PREDICATE => {
                self.arena
                    .get_type_predicate(node)
                    .is_some_and(|predicate| {
                        self.body_has_private_extends_operand(predicate.type_node, next)
                    })
            }
            _ => false,
        }
    }

    /// Recurse into every nested type slot of `node_idx`, returning `true` as
    /// soon as a provider-private named reference is found. Walks the type-node
    /// shapes that can hold a reference. This is used only after entering a
    /// conditional `extends` operand; references found elsewhere do not by
    /// themselves trigger cross-arena delegation.
    fn body_references_private(&self, node_idx: NodeIndex, depth: u32) -> bool {
        if node_idx == NodeIndex::NONE || depth > PROVIDER_PRIVATE_SCAN_MAX_DEPTH {
            return false;
        }
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        let next = depth + 1;
        match node.kind {
            syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = self.arena.get_type_ref(node) else {
                    return false;
                };
                if let Some(name_node) = self.arena.get(type_ref.type_name)
                    && let Some(ident) = self.arena.get_identifier(name_node)
                    && self.name_is_provider_private(&ident.escaped_text, type_ref.type_name)
                {
                    return true;
                }
                type_ref.type_arguments.as_ref().is_some_and(|args| {
                    args.nodes
                        .iter()
                        .any(|&arg| self.body_references_private(arg, next))
                })
            }
            syntax_kind_ext::UNION_TYPE | syntax_kind_ext::INTERSECTION_TYPE => self
                .arena
                .get_composite_type(node)
                .is_some_and(|composite| {
                    composite
                        .types
                        .nodes
                        .iter()
                        .any(|&member| self.body_references_private(member, next))
                }),
            syntax_kind_ext::ARRAY_TYPE => self
                .arena
                .get_array_type(node)
                .is_some_and(|array| self.body_references_private(array.element_type, next)),
            syntax_kind_ext::TUPLE_TYPE => self.arena.get_tuple_type(node).is_some_and(|tuple| {
                tuple
                    .elements
                    .nodes
                    .iter()
                    .any(|&elem| self.body_references_private(elem, next))
            }),
            syntax_kind_ext::PARENTHESIZED_TYPE
            | syntax_kind_ext::OPTIONAL_TYPE
            | syntax_kind_ext::REST_TYPE => self
                .arena
                .get_wrapped_type(node)
                .is_some_and(|wrapped| self.body_references_private(wrapped.type_node, next)),
            syntax_kind_ext::NAMED_TUPLE_MEMBER => self
                .arena
                .get_named_tuple_member(node)
                .is_some_and(|member| self.body_references_private(member.type_node, next)),
            syntax_kind_ext::CONDITIONAL_TYPE => {
                self.arena.get_conditional_type(node).is_some_and(|cond| {
                    self.body_references_private(cond.check_type, next)
                        || self.body_references_private(cond.extends_type, next)
                        || self.body_references_private(cond.true_type, next)
                        || self.body_references_private(cond.false_type, next)
                })
            }
            syntax_kind_ext::INFER_TYPE => self.arena.get_infer_type(node).is_some_and(|infer| {
                self.type_parameter_references_private(infer.type_parameter, next)
            }),
            syntax_kind_ext::TYPE_OPERATOR => self
                .arena
                .get_type_operator(node)
                .is_some_and(|op| self.body_references_private(op.type_node, next)),
            syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                self.arena.get_indexed_access_type(node).is_some_and(|ia| {
                    self.body_references_private(ia.object_type, next)
                        || self.body_references_private(ia.index_type, next)
                })
            }
            syntax_kind_ext::MAPPED_TYPE => {
                self.arena.get_mapped_type(node).is_some_and(|mapped| {
                    self.type_parameter_references_private(mapped.type_parameter, next)
                        || self.body_references_private(mapped.name_type, next)
                        || self.body_references_private(mapped.type_node, next)
                })
            }
            syntax_kind_ext::TYPE_LITERAL => self.arena.get_type_literal(node).is_some_and(|lit| {
                lit.members
                    .nodes
                    .iter()
                    .any(|&member| self.member_references_private(member, next))
            }),
            syntax_kind_ext::FUNCTION_TYPE | syntax_kind_ext::CONSTRUCTOR_TYPE => {
                self.arena.get_function_type(node).is_some_and(|func| {
                    self.body_references_private(func.type_annotation, next)
                        || self.params_reference_private(&func.parameters.nodes, next)
                })
            }
            syntax_kind_ext::TEMPLATE_LITERAL_TYPE => self
                .arena
                .get_template_literal_type(node)
                .is_some_and(|template| {
                    template.template_spans.nodes.iter().any(|&span_idx| {
                        self.arena
                            .get(span_idx)
                            .and_then(|span_node| self.arena.get_template_span(span_node))
                            .is_some_and(|span| self.body_references_private(span.expression, next))
                    })
                }),
            syntax_kind_ext::TYPE_PREDICATE => self
                .arena
                .get_type_predicate(node)
                .is_some_and(|predicate| self.body_references_private(predicate.type_node, next)),
            _ => false,
        }
    }

    /// Recurse into a type parameter's `constraint`/`default` slots (used for
    /// `infer T extends …` and a mapped type's `K in …` clause).
    fn type_parameter_references_private(&self, tp_idx: NodeIndex, depth: u32) -> bool {
        self.arena
            .get(tp_idx)
            .and_then(|node| self.arena.get_type_parameter(node))
            .is_some_and(|tp| {
                self.body_references_private(tp.constraint, depth)
                    || self.body_references_private(tp.default, depth)
            })
    }

    /// Recurse into a type-literal member's type slots (property/method/index
    /// signatures and call/construct signatures).
    fn member_references_private(&self, member_idx: NodeIndex, depth: u32) -> bool {
        let Some(member) = self.arena.get(member_idx) else {
            return false;
        };
        match member.kind {
            syntax_kind_ext::INDEX_SIGNATURE => {
                self.arena.get_index_signature(member).is_some_and(|index| {
                    self.body_references_private(index.type_annotation, depth)
                        || self.params_reference_private(&index.parameters.nodes, depth)
                })
            }
            _ => self.arena.get_signature(member).is_some_and(|sig| {
                self.body_references_private(sig.type_annotation, depth)
                    || sig
                        .parameters
                        .as_ref()
                        .is_some_and(|params| self.params_reference_private(&params.nodes, depth))
            }),
        }
    }

    /// True when any parameter in `params` carries a type annotation that
    /// references a provider-private type. Shared by function/constructor types
    /// and type-literal index/call/construct/method signatures.
    fn params_reference_private(&self, params: &[NodeIndex], depth: u32) -> bool {
        params.iter().any(|&param| {
            self.arena
                .get(param)
                .and_then(|node| self.arena.get_parameter(node))
                .is_some_and(|parameter| {
                    self.body_references_private(parameter.type_annotation, depth)
                })
        })
    }

    fn type_parameter_has_private_extends_operand(&self, tp_idx: NodeIndex, depth: u32) -> bool {
        self.arena
            .get(tp_idx)
            .and_then(|node| self.arena.get_type_parameter(node))
            .is_some_and(|tp| {
                self.body_has_private_extends_operand(tp.constraint, depth)
                    || self.body_has_private_extends_operand(tp.default, depth)
            })
    }

    fn member_has_private_extends_operand(&self, member_idx: NodeIndex, depth: u32) -> bool {
        let Some(member) = self.arena.get(member_idx) else {
            return false;
        };
        match member.kind {
            syntax_kind_ext::INDEX_SIGNATURE => {
                self.arena.get_index_signature(member).is_some_and(|index| {
                    self.body_has_private_extends_operand(index.type_annotation, depth)
                        || self.params_have_private_extends_operand(&index.parameters.nodes, depth)
                })
            }
            _ => self.arena.get_signature(member).is_some_and(|sig| {
                self.body_has_private_extends_operand(sig.type_annotation, depth)
                    || sig.parameters.as_ref().is_some_and(|params| {
                        self.params_have_private_extends_operand(&params.nodes, depth)
                    })
            }),
        }
    }

    fn params_have_private_extends_operand(&self, params: &[NodeIndex], depth: u32) -> bool {
        params.iter().any(|&param| {
            self.arena
                .get(param)
                .and_then(|node| self.arena.get_parameter(node))
                .is_some_and(|parameter| {
                    self.body_has_private_extends_operand(parameter.type_annotation, depth)
                })
        })
    }
}
