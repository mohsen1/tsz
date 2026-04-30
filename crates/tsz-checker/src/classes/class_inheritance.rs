//! Class Inheritance Cycle Detection
//!
//! This module provides cycle detection for class inheritance using the `InheritanceGraph`.
//! It detects circular inheritance BEFORE type resolution to prevent stack overflow.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;

pub(crate) struct ClassInheritanceChecker<'a, 'ctx> {
    pub(crate) ctx: &'a mut crate::CheckerContext<'ctx>,
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
        let mut seen_extends = false;
        if let Some(heritage_clauses) = &class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(heritage) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                if seen_extends {
                    continue;
                }
                seen_extends = true;
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
            self.error_circular_class_inheritance_for_symbol(current_sym);
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

        // Prefer registered inheritance graph edges; fall back to declared heritage
        // parents so cycle detection still works when checking files independently.
        let parents = self.get_parents_for_cycle_search(child);
        for &parent in &parents {
            if self.would_create_cycle(parent, target, visited) {
                return true;
            }
        }

        visited.remove(&child);
        false
    }

    fn get_parents_for_cycle_search(&self, symbol_id: SymbolId) -> Vec<SymbolId> {
        let parents = self.ctx.inheritance_graph.get_parents(symbol_id);
        if !parents.is_empty() {
            return parents;
        }
        self.resolve_declared_parent_symbols(symbol_id)
    }

    fn resolve_declared_parent_symbols(&self, symbol_id: SymbolId) -> Vec<SymbolId> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) else {
            return Vec::new();
        };

        let arena = self.ctx.get_arena_for_file(symbol.decl_file_idx);
        let binder = self
            .ctx
            .get_binder_for_file(symbol.decl_file_idx as usize)
            .unwrap_or(self.ctx.binder);

        let mut out = Vec::new();

        for &decl_idx in &symbol.declarations {
            let Some(node) = arena.get(decl_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::CLASS_DECLARATION {
                let Some(class) = arena.get_class(node) else {
                    continue;
                };
                let Some(heritage_clauses) = &class.heritage_clauses else {
                    continue;
                };
                for &clause_idx in &heritage_clauses.nodes {
                    let Some(heritage) = arena.get_heritage_clause_at(clause_idx) else {
                        continue;
                    };
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }
                    let Some(&type_idx) = heritage.types.nodes.first() else {
                        continue;
                    };
                    let expr_idx = arena
                        .get_expr_type_args_at(type_idx)
                        .map_or(type_idx, |e| e.expression);
                    if let Some(parent_sym) =
                        self.resolve_heritage_symbol_with(arena, binder, expr_idx)
                        && parent_sym != symbol_id
                    {
                        out.push(parent_sym);
                    }
                }
                continue;
            }

            if node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                let Some(iface) = arena.get_interface(node) else {
                    continue;
                };
                let Some(heritage_clauses) = &iface.heritage_clauses else {
                    continue;
                };
                for &clause_idx in &heritage_clauses.nodes {
                    let Some(heritage) = arena.get_heritage_clause_at(clause_idx) else {
                        continue;
                    };
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }
                    for &type_idx in &heritage.types.nodes {
                        let expr_idx = arena
                            .get_expr_type_args_at(type_idx)
                            .map_or(type_idx, |e| e.expression);
                        if let Some(parent_sym) =
                            self.resolve_heritage_symbol_with(arena, binder, expr_idx)
                            && parent_sym != symbol_id
                        {
                            out.push(parent_sym);
                        }
                    }
                }
            }
        }

        out.sort_unstable_by_key(|sym| sym.0);
        out.dedup();
        out
    }

    /// Resolve the symbol referenced by an extends clause expression
    ///
    /// This is a helper to resolve heritage symbols without triggering type resolution.
    fn resolve_heritage_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        self.resolve_heritage_symbol_with(self.ctx.arena, self.ctx.binder, expr_idx)
    }

    fn resolve_heritage_symbol_with(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        binder: &tsz_binder::BinderState,
        expr_idx: NodeIndex,
    ) -> Option<SymbolId> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = arena.get(expr_idx)?;

        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            // FIX: Use resolve_identifier instead of get_node_symbol
            // get_node_symbol only works for declaration nodes, not references.
            // Then prefer a namespace-local symbol when the raw binder lookup
            // fell back to a same-name file-level global.
            let resolved = binder.resolve_identifier(arena, expr_idx);
            if let Some(ident) = arena.get_identifier(node)
                && let Some(found_sym) = resolved
                && binder.file_locals.get(ident.escaped_text.as_str()) == Some(found_sym)
                && let Some(namespace_sym) = self.resolve_unqualified_name_in_enclosing_namespace(
                    arena,
                    binder,
                    expr_idx,
                    ident.escaped_text.as_str(),
                )
                && namespace_sym != found_sym
            {
                return Some(namespace_sym);
            }
            // Cross-file fallback: when the local binder can't resolve the
            // heritage identifier, try the other files' binders. In script
            // (non-module) projects, classes declared across separate files
            // share a single global namespace at compile time. Without this
            // fallback, a class `E extends D` in file3 cannot see the class
            // `D` defined in file2 during cycle detection — leaving 3-way
            // inheritance loops (TS2506) silently undetected.
            // Conformance: `classExtendsItselfIndirectly3.ts`.
            if resolved.is_none()
                && let Some(ident) = arena.get_identifier(node)
            {
                let name = ident.escaped_text.as_str();
                if let Some(all_binders) = self.ctx.all_binders.as_ref() {
                    for other_binder in all_binders.iter() {
                        if std::ptr::eq(other_binder.as_ref() as *const _, binder as *const _) {
                            continue;
                        }
                        if other_binder.is_external_module() {
                            continue;
                        }
                        if let Some(sym) = other_binder.file_locals.get(name) {
                            return Some(sym);
                        }
                    }
                }
            }
            resolved
        } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            self.resolve_qualified_symbol_with(arena, binder, expr_idx)
        } else if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            self.resolve_heritage_symbol_access_with(arena, binder, expr_idx)
        } else {
            None
        }
    }

    fn resolve_unqualified_name_in_enclosing_namespace(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        binder: &tsz_binder::BinderState,
        node_idx: NodeIndex,
        name: &str,
    ) -> Option<SymbolId> {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        if binder.is_external_module() {
            return None;
        }

        let mut current = node_idx;
        for _ in 0..100 {
            let ext = arena.get_extended(current)?;
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let parent_node = arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module_data) = arena.get_module(parent_node)
                && let Some(ns_name_ident) = arena.get_identifier_at(module_data.name)
            {
                if module_data.body.is_some()
                    && let Some(&scope_id) = binder.node_scope_ids.get(&module_data.body.0)
                    && let Some(scope) = binder.scopes.get(scope_id.0 as usize)
                    && let Some(member_id) = scope.table.get(name)
                {
                    let is_enum_member = binder
                        .get_symbol(member_id)
                        .is_some_and(|s| s.has_any_flags(symbol_flags::ENUM_MEMBER));
                    if !is_enum_member {
                        return Some(member_id);
                    }
                }

                let ns_name = ns_name_ident.escaped_text.as_str();
                if let Some(ns_sym_id) = binder.file_locals.get(ns_name)
                    && let Some(ns_sym) = binder.get_symbol(ns_sym_id)
                    && let Some(exports) = ns_sym.exports.as_ref()
                    && let Some(member_id) = exports.get(name)
                {
                    let is_enum_member = binder
                        .get_symbol(member_id)
                        .is_some_and(|s| s.has_any_flags(symbol_flags::ENUM_MEMBER));
                    if !is_enum_member {
                        return Some(member_id);
                    }
                }
            }
            current = parent_idx;
        }
        None
    }

    /// Resolve qualified name like 'Namespace.Class'
    fn resolve_qualified_symbol_with(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        binder: &tsz_binder::BinderState,
        expr_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let access = arena.get_access_expr_at(expr_idx)?;
        let left_sym = self.resolve_heritage_symbol_with(arena, binder, access.expression)?;
        let name = arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.clone())?;

        let left_symbol = binder.get_symbol(left_sym)?;
        let exports = left_symbol.exports.as_ref()?;
        exports.get(&name)
    }

    /// Resolve property access like 'Namespace.Class'
    fn resolve_heritage_symbol_access_with(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        binder: &tsz_binder::BinderState,
        expr_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let access = arena.get_access_expr_at(expr_idx)?;
        let left_sym = self.resolve_heritage_symbol_with(arena, binder, access.expression)?;
        let name = arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.clone())?;

        let left_symbol = binder.get_symbol(left_sym)?;
        let exports = left_symbol.exports.as_ref()?;
        exports.get(&name)
    }

    /// Emit TS2506/TS2310 for a symbol, using the declaration name as error span when available.
    fn error_circular_class_inheritance_for_symbol(&mut self, sym_id: SymbolId) {
        self.error_base_cycle_for_symbol(sym_id, true);
    }

    /// Emit TS2310 when a class or interface recursively appears through an
    /// instantiated base type rather than a direct extends-graph cycle.
    pub(crate) fn error_recursive_base_type_for_symbol(&mut self, sym_id: SymbolId) {
        self.error_base_cycle_for_symbol(sym_id, false);
    }

    fn error_base_cycle_for_symbol(&mut self, sym_id: SymbolId, direct_class_cycle: bool) {
        // Track circular symbols so `new C` can return `C<unknown>` instead of `C<T>`.
        self.ctx.circular_class_symbols.insert(sym_id);

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

        let (code, message_template) = if is_class && direct_class_cycle {
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

        let mut name = symbol.escaped_name.clone();

        // TS2310 includes type parameters in the display name. TS2506 uses the
        // bare class name even for generic declarations.
        if code == diagnostic_codes::TYPE_RECURSIVELY_REFERENCES_ITSELF_AS_A_BASE_TYPE {
            let type_parameters = if is_class {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|node| self.ctx.arena.get_class(node))
                    .and_then(|class| class.type_parameters.clone())
            } else {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|node| self.ctx.arena.get_interface(node))
                    .and_then(|iface| iface.type_parameters.clone())
            };
            if let Some(list) = &type_parameters {
                let mut param_names = Vec::new();
                for &param_idx in &list.nodes {
                    if let Some(node) = self.ctx.arena.get(param_idx)
                        && let Some(data) = self.ctx.arena.get_type_parameter(node)
                        && let Some(name_node) = self.ctx.arena.get(data.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        param_names.push(ident.escaped_text.as_str());
                    }
                }
                if !param_names.is_empty() {
                    name.push('<');
                    name.push_str(&param_names.join(", "));
                    name.push('>');
                }
            }
        }

        let message = format_message(message_template, &[&name]);

        let length = end.saturating_sub(start);
        self.ctx.error(start, length, message, code);
    }
}

#[cfg(test)]
mod tests {
    use super::ClassInheritanceChecker;
    use crate::query_boundaries::type_construction::TypeInterner;
    use crate::state::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    #[test]
    fn declared_parent_fallback_detects_cycle_without_registered_graph_edges() {
        let source = r#"
class C extends E {}
class D extends C {}
class E extends D {}
        "#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            Default::default(),
        );

        let c_sym = checker
            .ctx
            .binder
            .file_locals
            .get("C")
            .expect("class C symbol should exist");
        let cycle_checker = ClassInheritanceChecker::new(&mut checker.ctx);
        let parents = cycle_checker.get_parents_for_cycle_search(c_sym);

        assert_eq!(
            parents.len(),
            1,
            "C should have exactly one declared parent"
        );
        let parent_name = cycle_checker
            .ctx
            .binder
            .get_symbol(parents[0])
            .map(|s| s.escaped_name.clone())
            .unwrap_or_default();
        assert_eq!(parent_name, "E");
        assert!(
            cycle_checker.detects_cycle_dfs(c_sym, &parents),
            "fallback parent traversal should detect C -> E -> D -> C cycle without pre-registered edges"
        );
    }
}
