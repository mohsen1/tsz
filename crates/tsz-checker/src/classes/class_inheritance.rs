//! Class Inheritance Cycle Detection
//!
//! This module provides cycle detection for class inheritance using the `InheritanceGraph`.
//! It detects circular inheritance BEFORE type resolution to prevent stack overflow.

use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, diagnostic_codes, diagnostic_messages, format_message,
};
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;

pub struct ClassInheritanceChecker<'a, 'ctx> {
    pub ctx: &'a mut crate::CheckerContext<'ctx>,
}

impl<'a, 'ctx> ClassInheritanceChecker<'a, 'ctx> {
    pub const fn new(ctx: &'a mut crate::CheckerContext<'ctx>) -> Self {
        Self { ctx }
    }

    /// Check for circular inheritance in a class declaration
    ///
    /// Returns `true` if a cycle is found and diagnostics were emitted.
    pub fn check_class_inheritance_cycle(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

        let current_sym = match self.ctx.binder.get_node_symbol(class_idx) {
            Some(sym) => sym,
            None => return false, // No symbol = no inheritance possible
        };

        // Collect parent symbols from heritage clauses
        let mut parent_symbols = Vec::new();
        if let Some(heritage_clauses) = &class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(heritage) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                let Some(&type_idx) = heritage.types.nodes.first() else {
                    continue;
                };
                let expr_idx = self
                    .ctx
                    .arena
                    .get_expr_type_args_at(type_idx)
                    .map_or(type_idx, |e| e.expression);

                if let Some(parent_sym) = self.resolve_heritage_symbol(expr_idx) {
                    // Check for direct self-reference
                    if parent_sym == current_sym {
                        self.error_circular_class_inheritance_for_symbol(current_sym);
                        return true; // Signal to skip type checking
                    }
                    parent_symbols.push(parent_sym);
                }
            }
        }

        self.detect_and_report_cycle(current_sym, &parent_symbols)
    }

    /// Check for circular inheritance in an interface declaration.
    ///
    /// Returns `true` if a cycle is found and diagnostics were emitted.
    pub fn check_interface_inheritance_cycle(
        &mut self,
        iface_idx: NodeIndex,
        iface: &tsz_parser::parser::node::InterfaceData,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

        let current_sym = match self.ctx.binder.get_node_symbol(iface_idx) {
            Some(sym) => sym,
            None => return false,
        };

        let mut parent_symbols = Vec::new();
        if let Some(heritage_clauses) = &iface.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(heritage) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                    continue;
                };
                // Interfaces only use ExtendsKeyword
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                // Interfaces can extend multiple types
                for &type_idx in &heritage.types.nodes {
                    let expr_idx = self
                        .ctx
                        .arena
                        .get_expr_type_args_at(type_idx)
                        .map_or(type_idx, |e| e.expression);

                    if let Some(parent_sym) = self.resolve_heritage_symbol(expr_idx) {
                        if parent_sym == current_sym {
                            self.error_circular_class_inheritance_for_symbol(current_sym);
                            return true;
                        }
                        parent_symbols.push(parent_sym);
                    }
                }
            }
        }

        self.detect_and_report_cycle(current_sym, &parent_symbols)
    }

    fn detect_and_report_cycle(
        &mut self,
        current_sym: SymbolId,
        parent_symbols: &[SymbolId],
    ) -> bool {
        // Check for cycles using simple DFS traversal on the InheritanceGraph
        if self.detects_cycle_dfs(current_sym, parent_symbols) {
            let cycle_symbols = self.collect_cycle_symbols(current_sym, parent_symbols);
            for sym in cycle_symbols {
                self.error_circular_class_inheritance_for_symbol(sym);
            }
            return true;
        }

        // DEBUG: Log when we successfully register inheritance
        // tracing::debug!("Registered inheritance: {:?} extends {:?}", current_sym, parent_symbols);

        // No cycles - register with InheritanceGraph
        self.ctx
            .inheritance_graph
            .add_inheritance(current_sym, parent_symbols);
        false
    }

    /// Detect cycles using DFS traversal on the `InheritanceGraph`
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

    /// Collect all symbols participating in the detected cycle.
    ///
    /// The cycle is formed when adding `current -> parents` and at least one parent can
    /// already reach `current` through existing inheritance edges.
    fn collect_cycle_symbols(
        &self,
        current: SymbolId,
        parents: &[SymbolId],
    ) -> FxHashSet<SymbolId> {
        let mut cycle = FxHashSet::default();
        cycle.insert(current);

        for &parent in parents {
            let mut visited = FxHashSet::default();
            let mut path = Vec::new();
            if self.find_path_to_target(parent, current, &mut visited, &mut path) {
                for sym in path {
                    cycle.insert(sym);
                }
            }
        }

        cycle
    }

    /// Find a path from `node` to `target` in the existing inheritance graph.
    /// Appends symbols on the successful path into `path`.
    fn find_path_to_target(
        &self,
        node: SymbolId,
        target: SymbolId,
        visited: &mut FxHashSet<SymbolId>,
        path: &mut Vec<SymbolId>,
    ) -> bool {
        if node == target {
            path.push(node);
            return true;
        }

        if !visited.insert(node) {
            return false;
        }

        let parents = self.ctx.inheritance_graph.get_parents(node);
        for &parent in &parents {
            if self.find_path_to_target(parent, target, visited, path) {
                path.push(node);
                return true;
            }
        }

        false
    }

    /// Resolve the symbol referenced by an extends clause expression
    ///
    /// This is a helper to resolve heritage symbols without triggering type resolution.
    fn resolve_heritage_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = self.ctx.arena.get(expr_idx)?;

        // println!("resolve_heritage_symbol: expr_idx={:?}, sym={:?}", expr_idx, sym);
        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            // FIX: Use resolve_identifier instead of get_node_symbol
            // get_node_symbol only works for declaration nodes, not references
            self.ctx.binder.resolve_identifier(self.ctx.arena, expr_idx)
        } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            self.resolve_qualified_symbol(expr_idx)
        } else if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            self.resolve_heritage_symbol_access(expr_idx)
        } else {
            None
        }
    }

    /// Resolve qualified name like 'Namespace.Class'
    fn resolve_qualified_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let access = self.ctx.arena.get_access_expr_at(expr_idx)?;
        let left_sym = self.resolve_heritage_symbol(access.expression)?;
        let name = self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.clone())?;

        let left_symbol = self.ctx.binder.get_symbol(left_sym)?;
        let exports = left_symbol.exports.as_ref()?;
        exports.get(&name)
    }

    /// Resolve property access like 'Namespace.Class'
    fn resolve_heritage_symbol_access(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let access = self.ctx.arena.get_access_expr_at(expr_idx)?;
        let left_sym = self.resolve_heritage_symbol(access.expression)?;
        let name = self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.clone())?;

        let left_symbol = self.ctx.binder.get_symbol(left_sym)?;
        let exports = left_symbol.exports.as_ref()?;
        exports.get(&name)
    }

    /// Emit TS2506 for a class symbol, using the class name node as error span when available.
    fn error_circular_class_inheritance_for_symbol(&mut self, sym_id: SymbolId) {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };

        let decl_info = symbol.declarations.iter().find_map(|&decl_idx| {
            let node = self.ctx.arena.get(decl_idx)?;
            match node.kind {
                tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION => {
                    Some((decl_idx, true)) // is_class = true
                }
                tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION => {
                    Some((decl_idx, false)) // is_class = false
                }
                _ => None,
            }
        });

        let Some((decl_idx, is_class)) = decl_info else {
            return;
        };

        let error_node_idx = if is_class {
            self.ctx
                .arena
                .get(decl_idx)
                .and_then(|node| self.ctx.arena.get_class(node))
                .map(|class| class.name)
                .filter(|name| name.is_some())
                .unwrap_or(decl_idx)
        } else {
            self.ctx
                .arena
                .get(decl_idx)
                .and_then(|node| self.ctx.arena.get_interface(node))
                .map(|iface| iface.name)
                .filter(|name| name.is_some())
                .unwrap_or(decl_idx)
        };

        let Some((start, end)) = self.ctx.get_node_span(error_node_idx) else {
            return;
        };

        let (code, message_template) = if is_class {
            (
                diagnostic_codes::IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_BASE_EXPRESSION,
                diagnostic_messages::IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_BASE_EXPRESSION,
            )
        } else {
            (
                diagnostic_codes::TYPE_RECURSIVELY_REFERENCES_ITSELF_AS_A_BASE_TYPE,
                diagnostic_messages::TYPE_RECURSIVELY_REFERENCES_ITSELF_AS_A_BASE_TYPE,
            )
        };

        // Avoid duplicate
        if self
            .ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == code && diag.start == start)
        {
            return;
        }

        let name = symbol.escaped_name.clone();
        let message = format_message(message_template, &[&name]);

        let length = end.saturating_sub(start);
        self.ctx.diagnostics.push(Diagnostic {
            code,
            category: DiagnosticCategory::Error,
            message_text: message,
            file: self.ctx.file_name.clone(),
            start,
            length,
            related_information: Vec::new(),
        });
    }
}
