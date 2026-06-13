//! Scope entry/exit helpers for `BinderState`.

use super::super::state::BinderState;
use crate::{ContainerKind, SymbolTable, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_scanner::SyntaxKind;

impl BinderState {
    pub(crate) fn enter_scope(&mut self, kind: ContainerKind, node: NodeIndex) {
        self.enter_scope_with_capacity(kind, node, 0);
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
        if capacity > 0 {
            // Take the current scope, push it, and create a pre-sized one
            let old_scope = std::mem::take(&mut self.current_scope);
            self.scope_stack.push(old_scope);
            self.current_scope = SymbolTable::with_capacity(capacity);
        } else {
            self.push_scope();
        }

        // Persistent scope management (for stateless checking)
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

                        // Filter exports: only include symbols with is_exported = true or EXPORT_VALUE flag
                        let mut exports = SymbolTable::new();
                        for (name, &child_id) in self.current_scope.iter() {
                            if let Some(child) = self.symbols.get(child_id) {
                                // Check explicit export flag OR if it's an EXPORT_VALUE (from export {})
                                if export_all
                                    || child.is_exported
                                    || (child.flags & symbol_flags::EXPORT_VALUE) != 0
                                {
                                    exports.set(name.clone(), child_id);
                                }
                            }
                        }

                        // Persist filtered exports
                        if let Some(symbol) = self.symbols.get_mut(*sym_id) {
                            if let Some(ref mut existing) = symbol.exports {
                                for (name, &child_id) in exports.iter() {
                                    existing.set(name.clone(), child_id);
                                }
                            } else {
                                symbol.exports = Some(Box::new(exports));
                            }
                        }
                    }
                }
                ContainerKind::Class => {
                    // Find the symbol for this class
                    if let Some(sym_id) = self.node_symbols.get(&container_node.0) {
                        // Persist the current scope as the class's members
                        if let Some(symbol) = self.symbols.get_mut(*sym_id) {
                            symbol.members = Some(Box::new(self.current_scope.clone()));
                        }
                    }
                }
                _ => {}
            }
        }

        // Copy current scope to persistent scope before popping
        self.sync_current_scope_to_persistent();

        self.pop_scope();

        // Exit persistent scope
        self.exit_persistent_scope();
    }

    pub(crate) fn push_scope(&mut self) {
        let old_scope = std::mem::take(&mut self.current_scope);
        self.scope_stack.push(old_scope);
        self.current_scope = SymbolTable::new();
    }

    pub(crate) fn pop_scope(&mut self) {
        if let Some(parent_scope) = self.scope_stack.pop() {
            self.current_scope = parent_scope;
        }
    }
}
