//! Class Inheritance Cycle Detection
//!
//! This module provides cycle detection for class inheritance using the InheritanceGraph.
//! It detects circular inheritance BEFORE type resolution to prevent stack overflow.

use crate::types::diagnostics::{
    Diagnostic, DiagnosticCategory, diagnostic_codes, diagnostic_messages, format_message,
};
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;

pub struct ClassInheritanceChecker<'a, 'ctx> {
    pub ctx: &'a mut crate::CheckerContext<'ctx>,
}

impl<'a, 'ctx> ClassInheritanceChecker<'a, 'ctx> {
    pub fn new(ctx: &'a mut crate::CheckerContext<'ctx>) -> Self {
        Self { ctx }
    }

    /// Check for circular inheritance in a class declaration
    ///
    /// Returns Ok(()) if no cycle is detected, Err(()) if a cycle is found.
    /// Emits appropriate diagnostic error when cycle is detected.
    #[allow(clippy::result_unit_err)]
    pub fn check_class_inheritance_cycle(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Result<(), ()> {
        use tsz_scanner::SyntaxKind;

        let current_sym = match self.ctx.binder.get_node_symbol(class_idx) {
            Some(sym) => sym,
            None => return Ok(()), // No symbol = no inheritance possible
        };

        // Collect parent symbols from heritage clauses
        let mut parent_symbols = Vec::new();
        if let Some(heritage_clauses) = &class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                let Some(&type_idx) = heritage.types.nodes.first() else {
                    continue;
                };
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };
                let expr_idx = self
                    .ctx
                    .arena
                    .get_expr_type_args(type_node)
                    .map(|e| e.expression)
                    .unwrap_or(type_idx);

                if let Some(parent_sym) = self.resolve_heritage_symbol(expr_idx) {
                    // Check for direct self-reference
                    if parent_sym == current_sym {
                        self.error_circular_class_inheritance(expr_idx, class_idx);
                        return Err(()); // Signal to skip type checking
                    }
                    parent_symbols.push(parent_sym);
                }
            }
        }

        // Check for cycles using simple DFS traversal on the InheritanceGraph
        // This is more reliable than using transitive closure, which can be incomplete
        if self.detects_cycle_dfs(current_sym, &parent_symbols) {
            self.error_circular_class_inheritance(class_idx, class_idx);
            return Err(());
        }

        // DEBUG: Log when we successfully register inheritance
        tracing::debug!(
            "Registered inheritance: {:?} extends {:?}",
            current_sym,
            parent_symbols
        );

        // No cycles - register with InheritanceGraph
        self.ctx
            .inheritance_graph
            .add_inheritance(current_sym, &parent_symbols);
        Ok(())
    }

    /// Detect cycles using DFS traversal on the InheritanceGraph
    ///
    /// This checks if adding current->parents would create a cycle by traversing
    /// the graph starting from each parent and seeing if we can reach current.
    fn detects_cycle_dfs(&self, current: SymbolId, parents: &[SymbolId]) -> bool {
        let mut visited = FxHashSet::default();

        for &parent in parents {
            if self.would_create_cycle(parent, current, &mut visited) {
                return true;
            }
            visited.clear();
        }

        false
    }

    /// Check if adding edge child->current would create a cycle
    ///
    /// This does a DFS from child to see if we can reach current.
    fn would_create_cycle(
        &self,
        child: SymbolId,
        target: SymbolId,
        visited: &mut FxHashSet<SymbolId>,
    ) -> bool {
        if child == target {
            return true; // Found a path back to target - cycle!
        }

        if !visited.insert(child) {
            return false; // Already visited this node in current traversal
        }

        // Get child's parents from InheritanceGraph
        let parents = self.ctx.inheritance_graph.get_parents(child);
        for &parent in &parents {
            if self.would_create_cycle(parent, target, visited) {
                return true;
            }
        }

        visited.remove(&child);
        false
    }

    /// Resolve the symbol referenced by an extends clause expression
    ///
    /// This is a helper to resolve heritage symbols without triggering type resolution.
    fn resolve_heritage_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = self.ctx.arena.get(expr_idx)?;
        let sym = if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            // FIX: Use resolve_identifier instead of get_node_symbol
            // get_node_symbol only works for declaration nodes, not references
            self.ctx.binder.resolve_identifier(self.ctx.arena, expr_idx)
        } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            self.resolve_qualified_symbol(expr_idx)
        } else if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            self.resolve_heritage_symbol_access(expr_idx)
        } else {
            None
        };
        tracing::debug!(
            "resolve_heritage_symbol: expr_idx={:?}, sym={:?}",
            expr_idx,
            sym
        );
        sym
    }

    /// Resolve qualified name like 'Namespace.Class'
    fn resolve_qualified_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(expr_idx)?;
        let access = self.ctx.arena.get_access_expr(node)?;
        let left_sym = self.resolve_heritage_symbol(access.expression)?;
        let name = self
            .ctx
            .arena
            .get(access.name_or_argument)
            .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone())?;

        let left_symbol = self.ctx.binder.get_symbol(left_sym)?;
        let exports = left_symbol.exports.as_ref()?;
        exports.get(&name)
    }

    /// Resolve property access like 'Namespace.Class'
    fn resolve_heritage_symbol_access(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(expr_idx)?;
        let access = self.ctx.arena.get_access_expr(node)?;
        let left_sym = self.resolve_heritage_symbol(access.expression)?;
        let name = self
            .ctx
            .arena
            .get(access.name_or_argument)
            .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone())?;

        let left_symbol = self.ctx.binder.get_symbol(left_sym)?;
        let exports = left_symbol.exports.as_ref()?;
        exports.get(&name)
    }

    /// Emit TS2449: Circular inheritance error
    fn error_circular_class_inheritance(
        &mut self,
        error_node_idx: NodeIndex,
        class_idx: NodeIndex,
    ) {
        let class_name = self
            .ctx
            .arena
            .get(class_idx)
            .and_then(|node| self.ctx.arena.get_class(node))
            .and_then(|class| self.ctx.arena.get(class.name))
            .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.clone())
            .unwrap_or_else(|| String::from("<class>"));

        if let Some((start, end)) = self.ctx.get_node_span(error_node_idx) {
            let length = end.saturating_sub(start);
            let message =
                format_message(diagnostic_messages::CIRCULAR_BASE_REFERENCE, &[&class_name]);
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::CIRCULAR_BASE_REFERENCE,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start,
                length,
                related_information: Vec::new(),
            });
        }
    }
}
