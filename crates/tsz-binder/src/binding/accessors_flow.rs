//! Binder symbol accessors and flow helpers.

use crate::state::BinderState;
use crate::{FlowNodeId, Symbol, SymbolArena, SymbolId, flow_flags};
use std::sync::Arc;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl BinderState {
    // Public accessors

    /// Check if lib symbols have been merged into this binder's local arena.
    pub const fn lib_symbols_are_merged(&self) -> bool {
        self.lib_symbols_merged
    }

    /// Set the `lib_symbols_merged` flag.
    ///
    /// This should be called when a binder is reconstructed from a `MergedProgram`
    /// where all lib symbols have already been remapped to unique global IDs.
    pub const fn set_lib_symbols_merged(&mut self, merged: bool) {
        self.lib_symbols_merged = merged;
    }

    pub fn get_symbol(&self, id: SymbolId) -> Option<&Symbol> {
        // Fast path: If lib symbols are merged, all symbols are in the local arena
        // with unique IDs - no need to check lib_binders.
        if self.lib_symbols_merged {
            return self.symbols.get(id);
        }

        // Prefer local symbols first so source-file declarations can shadow
        // lib symbols even when SymbolId values collide.
        if let Some(sym) = self.symbols.get(id) {
            return Some(sym);
        }

        // Legacy path (for backward compatibility when lib_symbols_merged is false):
        // If this is a lib symbol ID, check lib binders first to avoid
        // collision with local symbols at the same index
        if self.lib_symbol_ids.contains(&id) {
            for lib_binder in self.lib_binders.iter() {
                if let Some(sym) = lib_binder.symbols.get(id) {
                    return Some(sym);
                }
            }
        }

        // Finally try lib binders for any remaining cases
        for lib_binder in self.lib_binders.iter() {
            if let Some(sym) = lib_binder.symbols.get(id) {
                return Some(sym);
            }
        }
        None
    }

    /// Get a symbol, checking lib binders if not found locally.
    /// This is used by the checker to resolve symbols that come from lib.d.ts.
    pub fn get_symbol_with_libs<'a>(
        &'a self,
        id: SymbolId,
        lib_binders: &'a [Arc<Self>],
    ) -> Option<&'a Symbol> {
        // Fast path: If lib symbols are merged, all symbols are in the local arena
        // with unique IDs - no need to check lib_binders.
        if self.lib_symbols_merged {
            return self.symbols.get(id);
        }

        // Prefer local symbols first so source-file declarations can shadow
        // lib symbols even when SymbolId values collide.
        if let Some(sym) = self.symbols.get(id) {
            return Some(sym);
        }

        // Legacy path (for backward compatibility when lib_symbols_merged is false):
        // Prefer lib binders when the ID is known to originate from libs
        if self.lib_symbol_ids.contains(&id) {
            for lib_binder in lib_binders {
                if let Some(sym) = lib_binder.symbols.get(id) {
                    return Some(sym);
                }
            }
        }

        // Then try lib binders
        for lib_binder in lib_binders {
            if let Some(sym) = lib_binder.symbols.get(id) {
                return Some(sym);
            }
        }

        None
    }

    /// Look up a global type by name from `file_locals` and lib binders.
    ///
    /// This method is used by the checker to find built-in types like Array, Object,
    /// Function, Promise, etc. It checks:
    /// 1. Local `file_locals` (for user-defined globals or merged lib symbols)
    /// 2. Lib binders (only when `lib_symbols_merged` is false)
    ///
    /// Resolve a name against the program-wide LIB globals carried by
    /// cross-file lookup binders (`MergedProgram::lib_globals`).
    ///
    /// This is a dedicated accessor, NOT folded into `get_global_type*`:
    /// many checker paths depend on those returning `None` on a cross-file
    /// binder (e.g. JSX element-type resolution treats an unresolved global
    /// as "use the namespace declared by the program's files"); a blanket
    /// fallback there regresses multi-file JSX programs. Callers that
    /// specifically resolve declaration heritage bases (`extends Request`)
    /// through cross-arena delegation chain this explicitly so heritage does
    /// not depend on root-file order.
    pub fn program_global_type(&self, name: &str) -> Option<SymbolId> {
        self.program_globals.get(name)
    }

    /// Returns the `SymbolId` if found, None otherwise.
    pub fn get_global_type(&self, name: &str) -> Option<SymbolId> {
        // First check file_locals (includes merged lib symbols when lib_symbols_merged is true)
        if let Some(sym_id) = self.file_locals.get(name) {
            return Some(sym_id);
        }

        // Fast path: If lib symbols are merged, they're all in file_locals already
        if self.lib_symbols_merged {
            return None;
        }

        // Legacy path: check lib binders directly (for backward compatibility)
        for lib_binder in self.lib_binders.iter() {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        None
    }

    /// Look up a global type by name, using provided lib binders.
    ///
    /// This variant is used when the checker has its own lib contexts and needs
    /// to search them explicitly.
    pub fn get_global_type_with_libs(
        &self,
        name: &str,
        lib_binders: &[Arc<Self>],
    ) -> Option<SymbolId> {
        // First check file_locals (includes merged lib symbols when lib_symbols_merged is true)
        if let Some(sym_id) = self.file_locals.get(name) {
            return Some(sym_id);
        }

        // Fast path: If lib symbols are merged, they're all in file_locals already
        if self.lib_symbols_merged {
            return None;
        }

        // Legacy path: check provided lib binders (for backward compatibility)
        for lib_binder in lib_binders {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        // Finally check our own lib binders
        for lib_binder in self.lib_binders.iter() {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        None
    }

    /// Check if a global type exists (in `file_locals` or lib binders).
    ///
    /// This is a convenience method for checking type availability without
    /// actually retrieving the symbol.
    pub fn has_global_type(&self, name: &str) -> bool {
        self.get_global_type(name).is_some()
    }

    pub fn get_node_symbol(&self, node: NodeIndex) -> Option<SymbolId> {
        self.node_symbols.get(&node.0).copied()
    }

    pub const fn get_symbols(&self) -> &SymbolArena {
        &self.symbols
    }

    /// Check if the current source file is an external module (has top-level import/export).
    /// This is used by the checker to determine if ES module semantics apply.
    pub const fn is_external_module(&self) -> bool {
        self.is_external_module
    }

    /// Check if a module specifier likely refers to an existing module that can be augmented.
    /// Rule #44: Module augmentation vs ambient module declaration detection.
    ///
    /// Returns true if:
    /// - The module specifier refers to an already declared module
    /// - The specifier looks like an external package (not a relative path)
    pub(crate) fn is_potential_module_augmentation(&self, module_specifier: &str) -> bool {
        // In external modules, relative `declare module "./x"` is always an augmentation target.
        if module_specifier.starts_with("./")
            || module_specifier.starts_with("../")
            || module_specifier == "."
            || module_specifier == ".."
        {
            return true;
        }

        // Check if we've already declared this module
        if self.declared_modules.contains(module_specifier) {
            return true;
        }

        // Check if we have exports from this module (meaning it was resolved)
        if self.module_exports.contains_key(module_specifier) {
            return true;
        }

        // External packages (not relative paths) are assumed to exist and can be augmented
        // This handles cases like `declare module 'express' { ... }`
        !module_specifier.starts_with('.') && !module_specifier.starts_with('/')
    }

    /// Get the flow node that was active at a given AST node.
    /// Used by the checker for control flow analysis.
    pub fn get_node_flow(&self, node: NodeIndex) -> Option<FlowNodeId> {
        self.node_flow.get(&node.0).copied()
    }

    /// Get the containing switch statement for a case/default clause.
    pub fn get_switch_for_clause(&self, clause: NodeIndex) -> Option<NodeIndex> {
        self.switch_clause_to_switch.get(&clause.0).copied()
    }

    /// Record the current flow node for an AST node.
    /// Called during binding to track flow position for identifiers and other expressions.
    pub(crate) fn record_flow(&mut self, node: NodeIndex) {
        if self.current_flow.is_some() {
            use tracing::trace;
            if let Some(flow_node) = self.flow_nodes.get(self.current_flow) {
                trace!(
                    node_idx = node.0,
                    flow_id = self.current_flow.0,
                    flow_flags = flow_node.flags,
                    "record_flow: associating node with flow"
                );
            }
            Arc::make_mut(&mut self.node_flow).insert(node.0, self.current_flow);
        }
    }

    pub(crate) fn with_fresh_flow<F>(&mut self, bind_body: F)
    where
        F: FnOnce(&mut Self),
    {
        self.with_fresh_flow_inner(bind_body, false);
    }

    /// Create a fresh flow for a function body, optionally capturing the enclosing flow for closures.
    /// If `capture_enclosing` is true, the START node will point to the enclosing flow, allowing
    /// const/let variables to preserve narrowing from the outer scope.
    pub(crate) fn with_fresh_flow_inner<F>(&mut self, bind_body: F, capture_enclosing: bool)
    where
        F: FnOnce(&mut Self),
    {
        let prev_flow = self.current_flow;
        let start_flow = {
            let flow_nodes = std::sync::Arc::make_mut(&mut self.flow_nodes);
            let start_flow = flow_nodes.alloc(flow_flags::START);

            // For closures (arrow functions and function expressions), capture the enclosing flow
            // so that const/let variables can preserve narrowing from the outer scope
            if capture_enclosing
                && prev_flow.is_some()
                && let Some(start_node) = flow_nodes.get_mut(start_flow)
            {
                start_node.antecedent.push(prev_flow);
            }
            start_flow
        };

        // Save and clear return_targets so that return statements inside
        // non-IIFE functions don't redirect to an enclosing IIFE's return target.
        let prev_return_targets = std::mem::take(&mut self.return_targets);

        self.current_flow = start_flow;
        bind_body(self);
        self.current_flow = prev_flow;
        self.return_targets = prev_return_targets;
    }

    // =========================================================================
    // Expression binding for flow analysis
    // =========================================================================

    pub(crate) const fn is_assignment_operator(operator: u16) -> bool {
        matches!(
            operator,
            k if k == SyntaxKind::EqualsToken as u16
                || k == SyntaxKind::PlusEqualsToken as u16
                || k == SyntaxKind::MinusEqualsToken as u16
                || k == SyntaxKind::AsteriskEqualsToken as u16
                || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                || k == SyntaxKind::SlashEqualsToken as u16
                || k == SyntaxKind::PercentEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
        )
    }

    pub(crate) fn is_array_mutation_call(arena: &NodeArena, call_idx: NodeIndex) -> bool {
        let Some(call) = arena.get_call_expr_at(call_idx) else {
            return false;
        };
        let Some(access) = arena.get_access_expr_at(call.expression) else {
            return false;
        };
        if access.question_dot_token {
            return false;
        }
        let Some(name_node) = arena.get(access.name_or_argument) else {
            return false;
        };
        let name = if let Some(ident) = arena.get_identifier(name_node) {
            ident.escaped_text.as_str()
        } else if let Some(literal) = arena.get_literal(name_node) {
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                literal.text.as_str()
            } else {
                return false;
            }
        } else {
            return false;
        };

        matches!(
            name,
            "copyWithin"
                | "fill"
                | "pop"
                | "push"
                | "reverse"
                | "shift"
                | "sort"
                | "splice"
                | "unshift"
        )
    }

    pub(crate) fn is_optional_chain_access(arena: &NodeArena, idx: NodeIndex) -> bool {
        let idx = arena.skip_parenthesized_and_assertions(idx);
        let Some(node) = arena.get(idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = arena.get_access_expr(node) {
                    access.question_dot_token
                        || Self::is_optional_chain_access(arena, access.expression)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if node.is_optional_chain() {
                    return true;
                }
                if let Some(call) = arena.get_call_expr(node) {
                    Self::is_optional_chain_access(arena, call.expression)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub(crate) fn continues_optional_chain(&self, arena: &NodeArena, idx: NodeIndex) -> bool {
        let Some(ext) = arena.get_extended(idx) else {
            return false;
        };
        let parent = ext.parent;
        if parent.is_none() {
            return false;
        }
        let Some(parent_node) = arena.get(parent) else {
            return false;
        };
        match parent_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                arena.get_access_expr(parent_node).is_some_and(|access| {
                    access.expression == idx && Self::is_optional_chain_access(arena, parent)
                })
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                arena.get_call_expr(parent_node).is_some_and(|call| {
                    call.expression == idx && Self::is_optional_chain_access(arena, parent)
                })
            }
            _ => false,
        }
    }

    pub(crate) fn optional_chain_branch_base(&self) -> FlowNodeId {
        let current = self.current_flow;
        let Some(flow) = self.flow_nodes.get(current) else {
            return current;
        };
        if (flow.flags & flow_flags::TRUE_CONDITION) != 0
            && let Some(&antecedent) = flow.antecedent.first()
            && antecedent.is_some()
        {
            return antecedent;
        }
        current
    }
}
