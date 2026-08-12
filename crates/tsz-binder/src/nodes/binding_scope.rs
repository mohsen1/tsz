//! Scope entry/exit helpers for `BinderState`.

use super::super::state::BinderState;
use crate::{ContainerKind, SymbolId, SymbolTable, symbol_flags};
use rustc_hash::FxHashSet;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_scanner::SyntaxKind;

impl BinderState {
    pub(crate) fn enter_scope(&mut self, kind: ContainerKind, node: NodeIndex) {
        self.enter_scope_with_capacity(kind, node, 0);
    }

    /// A same-named `TYPE_PARAMETER` symbol in the enclosing function scope,
    /// when the scope currently being bound is that function's own top-level
    /// body block. tsc's binder gives that block no container of its own, so a
    /// type parameter and a top-level local type share one declaration space
    /// (`TS2300`); tsz keeps them in two persistent scopes, so `declare_symbol`
    /// needs this cross-scope lookup to see the collision. A genuinely nested
    /// block (`if`/`for`/further-nested `{ }`) is excluded via
    /// `is_function_body_block`.
    pub(crate) fn function_type_parameter_collision(
        &self,
        arena: &NodeArena,
        name: &str,
    ) -> Option<SymbolId> {
        let scope = self.current_persistent_scope()?;
        if scope.kind != ContainerKind::Block
            || !self.is_function_body_block(arena, scope.container_node)
        {
            return None;
        }
        let parent_scope = self.scopes.get(scope.parent.0 as usize)?;
        let candidate = parent_scope.table.get(name)?;
        self.symbols
            .get(candidate)
            .is_some_and(|sym| (sym.flags & symbol_flags::TYPE_PARAMETER) != 0)
            .then_some(candidate)
    }

    /// Enter a new scope with pre-allocated capacity for the symbol table.
    /// This avoids repeated hash map resizing for scopes where the approximate
    /// member count is known (e.g., class bodies with many members).
    pub(crate) fn enter_scope_with_capacity(
        &mut self,
        kind: ContainerKind,
        node: NodeIndex,
        capacity: usize,
    ) {
        // The persistent scope arena owns scope identity AND contents: pushing a
        // child scope (pre-sized) makes `scopes[current_scope_id].table` the new
        // live declaration table. The parent is reachable via the scope's parent
        // link, so no separate save/restore stack is needed.
        self.enter_persistent_scope_with_capacity(kind, node, capacity);
    }

    pub(crate) fn exit_scope(&mut self, arena: &NodeArena) {
        // Capture exports before popping if this is a module/namespace.
        // Copy the scope identity out of the persistent arena first so the
        // mutable symbol-table writes below don't conflict with the borrow.
        let current_scope_info = self
            .current_persistent_scope()
            .map(|scope| (scope.kind, scope.container_node));
        if let Some((container_kind, container_node)) = current_scope_info {
            match container_kind {
                ContainerKind::Module => {
                    // Find the symbol for this module/namespace
                    if let Some(sym_id) = self.node_symbols.get(&container_node.0) {
                        let export_all = self.in_global_augmentation
                            || arena
                                .get(container_node)
                                .and_then(|node| arena.get_module(node))
                                .is_some_and(|module| {
                                    let is_external =
                                        arena.get(module.name).is_some_and(|name_node| {
                                            name_node.kind == SyntaxKind::StringLiteral as u16
                                                || name_node.kind
                                                    == SyntaxKind::NoSubstitutionTemplateLiteral
                                                        as u16
                                        });
                                    let is_ambient = arena.has_modifier_ref(
                                        module.modifiers.as_ref(),
                                        SyntaxKind::DeclareKeyword,
                                    ) || is_external;
                                    // Implicit export only applies while the ambient body
                                    // is still an export context (no explicit export
                                    // declaration/assignment); see the helper for the rule.
                                    is_ambient
                                        && !Self::ambient_module_body_disables_export_context(
                                            arena,
                                            module.body,
                                        )
                                });

                        // Filter exports: only include symbols with is_exported = true or EXPORT_VALUE flag.
                        // Snapshot which child SymbolIds pass the export filter
                        // first so the `self.symbols` borrow drops before the
                        // scope-table copy and the mutable `self.symbols` access
                        // below. The set is keyed by SymbolId, so the copy below
                        // preserves the source scope's atom side-index.
                        let exported_children: FxHashSet<crate::SymbolId> = self
                            .current_scope()
                            .iter()
                            .map(|(_, &child_id)| child_id)
                            .filter(|&child_id| {
                                self.symbols.get(child_id).is_some_and(|child| {
                                    // Check explicit export flag OR if it's an
                                    // EXPORT_VALUE (from export {}).
                                    export_all
                                        || child.is_exported
                                        || (child.flags & symbol_flags::EXPORT_VALUE) != 0
                                })
                            })
                            .collect();
                        // Copy retained entries from the live scope table,
                        // preserving both name keys and same-arena atom keys.
                        let source = self.current_scope().clone();
                        let mut exports = SymbolTable::new();
                        exports.merge_filtered_from(&source, |child_id| {
                            exported_children.contains(&child_id)
                        });

                        // Persist filtered exports
                        if let Some(symbol) = self.symbols.get_mut(*sym_id) {
                            if let Some(ref mut existing) = symbol.exports {
                                existing.merge_filtered_from(&exports, |_| true);
                            } else {
                                symbol.exports = Some(Box::new(exports));
                            }
                        }
                    }
                }
                ContainerKind::Class => {
                    // Find the symbol for this class
                    if let Some(sym_id) = self.node_symbols.get(&container_node.0).copied() {
                        // Persist the current scope as the class's members.
                        // Clone the table before the mutable `self.symbols` borrow.
                        let members = self.current_scope().clone();
                        if let Some(symbol) = self.symbols.get_mut(sym_id) {
                            symbol.members = Some(Box::new(members));
                        }
                    }
                }
                _ => {}
            }
        }

        // Pop back to the parent scope via the persistent arena's parent link.
        self.exit_persistent_scope();
    }
}
