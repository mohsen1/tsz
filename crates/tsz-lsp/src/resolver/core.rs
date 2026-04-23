//! Symbol resolution for LSP operations.
//!
//! The Binder maps declaration nodes to symbols, but LSP needs to resolve
//! identifier *usages* to symbols as well. This module provides a lightweight
//! scope walker that reconstructs scope chains on demand.

use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;
use tsz_binder::BinderState;
use tsz_binder::{SymbolId, SymbolTable};
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
use tsz_parser::{NodeIndex, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

#[derive(Debug, Default, Clone)]
pub struct ScopeCacheStats {
    pub hits: u32,
    pub misses: u32,
}

impl ScopeCacheStats {
    const fn record_hit(&mut self) {
        self.hits = self.hits.saturating_add(1);
    }

    const fn record_miss(&mut self) {
        self.misses = self.misses.saturating_add(1);
    }
}

pub type ScopeCache = FxHashMap<u32, Vec<SymbolTable>>;

/// A lightweight scope chain reconstructed on demand.
///
/// This mimics the binder's scope logic but focuses on resolving identifiers
/// to symbols, rather than creating new symbols.
pub struct ScopeWalker<'a> {
    pub(crate) arena: &'a NodeArena,
    binder: &'a BinderState,
    /// Stack of active scopes (maps name -> `SymbolId`)
    scope_stack: Vec<SymbolTable>,
    /// Indices of function-scoped entries within `scope_stack`
    function_scope_indices: Vec<usize>,
}

impl<'a> ScopeWalker<'a> {
    fn resolve_module_namespace_string_symbol(&self, target: NodeIndex) -> Option<SymbolId> {
        let node = self.arena.get(target)?;
        if node.kind != SyntaxKind::StringLiteral as u16 {
            return None;
        }

        let ext = self.arena.get_extended(target)?;
        let parent = ext.parent;
        let parent_node = self.arena.get(parent)?;
        if parent_node.kind != syntax_kind_ext::IMPORT_SPECIFIER
            && parent_node.kind != syntax_kind_ext::EXPORT_SPECIFIER
        {
            return None;
        }

        self.binder
            .node_symbols
            .get(&parent.0)
            .copied()
            .or_else(|| {
                let resolve_specifier_symbol = |node_idx: NodeIndex| {
                    self.binder
                        .node_symbols
                        .get(&node_idx.0)
                        .copied()
                        .or_else(|| {
                            let node = self.arena.get(node_idx)?;
                            if node.kind == SyntaxKind::Identifier as u16
                                || node.kind == SyntaxKind::PrivateIdentifier as u16
                            {
                                self.binder.resolve_identifier(self.arena, node_idx)
                            } else {
                                None
                            }
                        })
                };
                let spec = self.arena.get_specifier(parent_node)?;
                let from_name = if spec.name.is_some() {
                    resolve_specifier_symbol(spec.name)
                } else {
                    None
                };
                from_name.or_else(|| {
                    if spec.property_name.is_some() {
                        resolve_specifier_symbol(spec.property_name)
                    } else {
                        None
                    }
                })
            })
    }

    /// Create a new scope walker.
    pub fn new(arena: &'a NodeArena, binder: &'a BinderState) -> Self {
        Self {
            arena,
            binder,
            // Start with file-level scope
            scope_stack: vec![binder.file_locals.clone()],
            function_scope_indices: vec![0],
        }
    }

    /// Push a new scope onto the stack.
    fn push_scope(&mut self, is_function_scope: bool) {
        let next_index = self.scope_stack.len();
        self.scope_stack.push(SymbolTable::new());
        if is_function_scope {
            self.function_scope_indices.push(next_index);
        }
    }

    /// Pop the current scope from the stack.
    fn pop_scope(&mut self) {
        let index = self.scope_stack.len().saturating_sub(1);
        self.scope_stack.pop();
        if self.function_scope_indices.last() == Some(&index) {
            self.function_scope_indices.pop();
        }
    }

    /// Register a declaration in the current scope.
    fn declare_local(&mut self, name: String, sym_id: SymbolId) {
        if let Some(scope) = self.scope_stack.last_mut() {
            scope.set(name, sym_id);
        }
    }

    /// Register a declaration in the nearest function scope.
    fn declare_function_scoped(&mut self, name: String, sym_id: SymbolId) {
        if let Some(&index) = self.function_scope_indices.last() {
            if let Some(scope) = self.scope_stack.get_mut(index) {
                scope.set(name, sym_id);
            }
        } else {
            self.declare_local(name, sym_id);
        }
    }

    /// Resolve a name by walking up the scope stack.
    fn resolve_name(&self, name: &str) -> Option<SymbolId> {
        for scope in self.scope_stack.iter().rev() {
            if let Some(id) = scope.get(name) {
                return Some(id);
            }
        }
        None
    }

    fn resolve_name_in_scopes(scopes: &[SymbolTable], name: &str) -> Option<SymbolId> {
        for scope in scopes.iter().rev() {
            if let Some(id) = scope.get(name) {
                return Some(id);
            }
        }
        None
    }

    /// Check if a node creates a new scope.
    fn node_creates_scope(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        matches!(
            node.kind,
            k if k == syntax_kind_ext::BLOCK
                || k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::METHOD_DECLARATION
                || k == syntax_kind_ext::CONSTRUCTOR
                || k == syntax_kind_ext::GET_ACCESSOR
                || k == syntax_kind_ext::SET_ACCESSOR
                || k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
                || k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT
                || k == syntax_kind_ext::CATCH_CLAUSE
                || k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::MODULE_DECLARATION
                || k == syntax_kind_ext::MODULE_BLOCK
        )
    }

    /// Check if a node creates a function scope.
    fn node_creates_function_scope(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        matches!(
            node.kind,
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::METHOD_DECLARATION
                || k == syntax_kind_ext::CONSTRUCTOR
                || k == syntax_kind_ext::GET_ACCESSOR
                || k == syntax_kind_ext::SET_ACCESSOR
        )
    }

    /// Resolve an identifier node to its symbol.
    ///
    /// This walks the AST from the root to the target node, maintaining
    /// scope state along the way, then resolves the identifier in the
    /// current scope.
    pub fn resolve_node(&mut self, root: NodeIndex, target: NodeIndex) -> Option<SymbolId> {
        // First check if this is a declaration node
        if let Some(&sym_id) = self.binder.node_symbols.get(&target.0) {
            return Some(sym_id);
        }
        if let Some(sym_id) = self.resolve_module_namespace_string_symbol(target) {
            return Some(sym_id);
        }

        // Otherwise, we need to walk the tree to build scope context
        self.walk_to_node(root, target, &mut Vec::new())
    }

    pub fn resolve_node_cached(
        &mut self,
        root: NodeIndex,
        target: NodeIndex,
        cache: &mut ScopeCache,
        mut stats: Option<&mut ScopeCacheStats>,
    ) -> Option<SymbolId> {
        if let Some(&sym_id) = self.binder.node_symbols.get(&target.0) {
            return Some(sym_id);
        }
        if let Some(sym_id) = self.resolve_module_namespace_string_symbol(target) {
            return Some(sym_id);
        }

        let node = self.arena.get(target)?;
        if node.kind == SyntaxKind::PrivateIdentifier as u16 {
            return self.binder.resolve_identifier(self.arena, target);
        }
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let name = self.arena.get_identifier_text(target)?;
        if let Some(scopes) = cache.get(&target.0) {
            if let Some(stats) = stats.as_deref_mut() {
                stats.record_hit();
            }
            return Self::resolve_name_in_scopes(scopes, name)
                .or_else(|| self.binder.resolve_identifier(self.arena, target));
        }

        let scopes = self.get_scope_chain(root, target);
        let symbol_id = Self::resolve_name_in_scopes(&scopes, name)
            .or_else(|| self.binder.resolve_identifier(self.arena, target));
        cache.insert(target.0, scopes);
        if let Some(stats) = stats {
            stats.record_miss();
        }
        symbol_id
    }

    /// Walk the AST to find a target node, building scope context.
    ///
    /// Returns the symbol ID if the target is an identifier that can be resolved.
    fn walk_to_node(
        &mut self,
        current: NodeIndex,
        target: NodeIndex,
        path: &mut Vec<NodeIndex>,
    ) -> Option<SymbolId> {
        if current == target {
            // Found the target! Try to resolve it
            if let Some(node) = self.arena.get(current) {
                if node.kind == SyntaxKind::Identifier as u16 {
                    if let Some(text) = self.arena.get_identifier_text(current) {
                        return self
                            .resolve_name(text)
                            .or_else(|| self.binder.resolve_identifier(self.arena, current));
                    }
                } else if node.kind == SyntaxKind::PrivateIdentifier as u16 {
                    return self.binder.resolve_identifier(self.arena, current);
                }
            }
            return None;
        }

        let node = self.arena.get(current)?;

        // Optimization: Don't descend if target is not within current node's range
        if let Some(target_node) = self.arena.get(target)
            && (target_node.pos < node.pos || target_node.pos >= node.end)
        {
            return None;
        }

        // Check if this node creates a new scope
        let creates_scope = self.node_creates_scope(current);

        if creates_scope {
            let is_function_scope = self.node_creates_function_scope(current);
            self.push_scope(is_function_scope);
            if is_function_scope {
                self.register_hoisted_var_declarations(current);
            }
            // Register declarations visible in this scope
            self.register_local_declarations(current);
        }

        path.push(current);

        // Recurse into children using for_each_child
        let result = self.for_each_child(current, |walker, child_idx| {
            walker.walk_to_node(child_idx, target, path)
        });

        path.pop();

        if creates_scope {
            self.pop_scope();
        }

        result
    }

    const fn is_var_declaration_list(&self, node: &Node) -> bool {
        !node_flags::is_let_or_const(node.flags as u32)
    }

    fn is_var_declaration(&self, decl_idx: NodeIndex) -> bool {
        let Some(ext) = self.arena.get_extended(decl_idx) else {
            return false;
        };
        let Some(parent) = self.arena.get(ext.parent) else {
            return false;
        };
        if parent.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return false;
        }
        self.is_var_declaration_list(parent)
    }

    fn register_binding_declarations_in_function_scope(&mut self, name_idx: NodeIndex) {
        self.register_binding_declarations_with_scope(name_idx, true);
    }

    fn register_binding_declarations_with_scope(
        &mut self,
        name_idx: NodeIndex,
        function_scope: bool,
    ) {
        if name_idx.is_none() {
            return;
        }

        if let Some(&sym_id) = self.binder.node_symbols.get(&name_idx.0) {
            if let Some(symbol) = self.binder.symbols.get(sym_id) {
                if function_scope {
                    self.declare_function_scoped(symbol.escaped_name.clone(), sym_id);
                } else {
                    self.declare_local(symbol.escaped_name.clone(), sym_id);
                }
            }
            return;
        }

        let Some(node) = self.arena.get(name_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(binding) = self.arena.get_binding_element(node) {
                    self.register_binding_declarations_with_scope(binding.name, function_scope);
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &elem in &pattern.elements.nodes {
                        if elem.is_none() {
                            continue;
                        }
                        self.register_binding_declarations_with_scope(elem, function_scope);
                    }
                }
            }
            _ => {}
        }
    }

    fn register_hoisted_var_declarations(&mut self, container: NodeIndex) {
        let Some(node) = self.arena.get(container) else {
            return;
        };

        let body = match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                self.arena.get_function(node).map(|func| func.body)
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.arena.get_method_decl(node).map(|method| method.body)
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.arena.get_constructor(node).map(|ctor| ctor.body)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.arena.get_accessor(node).map(|accessor| accessor.body)
            }
            _ => None,
        };

        let Some(body_idx) = body else {
            return;
        };
        if body_idx.is_none() {
            return;
        }

        let mut var_lists = Vec::new();
        self.collect_var_declaration_lists(body_idx, &mut var_lists);
        for list_idx in var_lists {
            let Some(list_node) = self.arena.get(list_idx) else {
                continue;
            };
            let Some(list) = self.arena.get_variable(list_node) else {
                continue;
            };
            for &decl_idx in &list.declarations.nodes {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    self.register_binding_declarations_in_function_scope(decl.name);
                }
            }
        }
    }

    fn collect_var_declaration_lists(&mut self, current: NodeIndex, lists: &mut Vec<NodeIndex>) {
        if current.is_none() {
            return;
        }

        let Some(node) = self.arena.get(current) else {
            return;
        };

        if self.node_creates_function_scope(current) {
            return;
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && self.is_var_declaration_list(node)
        {
            lists.push(current);
        }

        self.for_each_child(current, |walker, child_idx| {
            walker.collect_var_declaration_lists(child_idx, lists);
            None::<()>
        });
    }

    /// Register local declarations from a container node.
    fn register_local_declarations(&mut self, container: NodeIndex) {
        let mut skip_name = None;
        if let Some(node) = self.arena.get(container) {
            match node.kind {
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    if let Some(&sym_id) = self.binder.node_symbols.get(&container.0)
                        && let Some(symbol) = self.binder.symbols.get(sym_id)
                    {
                        self.declare_local(symbol.escaped_name.clone(), sym_id);
                    }
                    // Class members are not lexically scoped identifiers.
                    return;
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.arena.get_method_decl(node) {
                        skip_name = Some(method.name);
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.arena.get_accessor(node) {
                        skip_name = Some(accessor.name);
                    }
                }
                _ => {}
            }
        }

        // Iterate over direct children to find declarations
        self.for_each_child(container, |walker, child_idx| {
            if let Some(skip_idx) = skip_name
                && child_idx == skip_idx
            {
                return None::<()>;
            }

            if let Some(node) = walker.arena.get(child_idx) {
                if node.kind == syntax_kind_ext::PARAMETER
                    && let Some(param) = walker.arena.get_parameter(node)
                {
                    walker.register_binding_declarations(param.name);
                }
                if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                    if let Some(decl) = walker.arena.get_variable_declaration(node) {
                        if walker.is_var_declaration(child_idx) {
                            walker.register_binding_declarations_in_function_scope(decl.name);
                        } else {
                            walker.register_binding_declarations(decl.name);
                        }
                    }
                    return None::<()>;
                }
                if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                    walker.for_each_child(child_idx, |w, list_idx| {
                        // Inside VariableStatement is VariableDeclarationList
                        if let Some(list_node) = w.arena.get(list_idx)
                            && list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                        {
                            let is_var = w.is_var_declaration_list(list_node);
                            w.for_each_child(list_idx, |w2, decl_idx| {
                                // Inside List is VariableDeclaration - this has the symbol!
                                if let Some(&sym_id) = w2.binder.node_symbols.get(&decl_idx.0)
                                    && let Some(symbol) = w2.binder.symbols.get(sym_id)
                                {
                                    if is_var {
                                        w2.declare_function_scoped(
                                            symbol.escaped_name.clone(),
                                            sym_id,
                                        );
                                    } else {
                                        w2.declare_local(symbol.escaped_name.clone(), sym_id);
                                    }
                                }
                                if let Some(decl_node) = w2.arena.get(decl_idx)
                                    && let Some(decl) = w2.arena.get_variable_declaration(decl_node)
                                {
                                    if is_var {
                                        w2.register_binding_declarations_in_function_scope(
                                            decl.name,
                                        );
                                    } else {
                                        w2.register_binding_declarations(decl.name);
                                    }
                                }
                                None::<()>
                            });
                        }
                        None::<()>
                    });
                    return None::<()>;
                }
                // For ExportDeclaration, unwrap to find the inner declaration
                else if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                    if let Some(export) = walker.arena.get_export_decl(node)
                        && export.export_clause.is_some()
                    {
                        // Check if export_clause is a declaration (e.g., export const x = 1)
                        if let Some(_export_clause_node) = walker.arena.get(export.export_clause) {
                            // Recurse into the export_clause to find the actual declaration
                            walker.for_each_child(export.export_clause, |w, inner_idx| {
                                if let Some(&sym_id) = w.binder.node_symbols.get(&inner_idx.0)
                                    && let Some(symbol) = w.binder.symbols.get(sym_id)
                                {
                                    w.declare_local(symbol.escaped_name.clone(), sym_id);
                                }
                                None::<()>
                            });
                        }
                    }
                }
                // For VariableDeclarationList (direct child), we need to go one level deeper
                else if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                    let is_var = walker.is_var_declaration_list(node);
                    walker.for_each_child(child_idx, |w, decl_idx| {
                        if let Some(&sym_id) = w.binder.node_symbols.get(&decl_idx.0)
                            && let Some(symbol) = w.binder.symbols.get(sym_id)
                        {
                            if is_var {
                                w.declare_function_scoped(symbol.escaped_name.clone(), sym_id);
                            } else {
                                w.declare_local(symbol.escaped_name.clone(), sym_id);
                            }
                        }
                        if let Some(decl_node) = w.arena.get(decl_idx)
                            && let Some(decl) = w.arena.get_variable_declaration(decl_node)
                        {
                            if is_var {
                                w.register_binding_declarations_in_function_scope(decl.name);
                            } else {
                                w.register_binding_declarations(decl.name);
                            }
                        }
                        None::<()> // Continue iteration
                    });
                    return None::<()>;
                }
            }

            // Check if this child has a symbol associated in the binder
            if let Some(&sym_id) = walker.binder.node_symbols.get(&child_idx.0)
                && let Some(symbol) = walker.binder.symbols.get(sym_id)
            {
                walker.declare_local(symbol.escaped_name.clone(), sym_id);
            }

            None::<()> // Continue iteration
        });
    }

    fn register_binding_declarations(&mut self, name_idx: NodeIndex) {
        self.register_binding_declarations_with_scope(name_idx, false);
    }

    /// Get the scope chain (symbol tables) active at the target node.
    ///
    /// This is used by the completions feature to suggest identifiers
    /// that are visible at a given cursor position.
    ///
    /// Returns a vector of symbol tables, ordered from innermost to outermost scope.
    pub fn get_scope_chain(&mut self, root: NodeIndex, target: NodeIndex) -> Vec<SymbolTable> {
        let mut found_stack = None;
        self.walk_for_scope(root, target, &mut found_stack);
        found_stack.unwrap_or_else(|| self.scope_stack.clone())
    }

    pub fn get_scope_chain_cached<'b>(
        &'b mut self,
        root: NodeIndex,
        target: NodeIndex,
        cache: &'b mut ScopeCache,
        mut stats: Option<&mut ScopeCacheStats>,
    ) -> &'b [SymbolTable] {
        match cache.entry(target.0) {
            Entry::Occupied(entry) => {
                if let Some(stats) = stats.as_deref_mut() {
                    stats.record_hit();
                }
                entry.into_mut().as_slice()
            }
            Entry::Vacant(entry) => {
                let scopes = self.get_scope_chain(root, target);
                if let Some(stats) = stats {
                    stats.record_miss();
                }
                entry.insert(scopes).as_slice()
            }
        }
    }

    /// Walk the AST to find a target node and capture its scope stack.
    fn walk_for_scope(
        &mut self,
        current: NodeIndex,
        target: NodeIndex,
        result: &mut Option<Vec<SymbolTable>>,
    ) -> bool {
        if current == target {
            // Found the target! Capture the current scope stack
            *result = Some(self.scope_stack.clone());
            return true;
        }

        let Some(node) = self.arena.get(current) else {
            return false;
        };

        // Optimization: Don't descend if target is not within current node's range
        if let Some(target_node) = self.arena.get(target)
            && (target_node.pos < node.pos || target_node.pos >= node.end)
        {
            return false;
        }

        // Check if this node creates a new scope
        let creates_scope = self.node_creates_scope(current);

        if creates_scope {
            let is_function_scope = self.node_creates_function_scope(current);
            self.push_scope(is_function_scope);
            if is_function_scope {
                self.register_hoisted_var_declarations(current);
            }
            self.register_local_declarations(current);
        }

        // Recurse into children
        let found = self
            .for_each_child(current, |walker, child_idx| {
                walker
                    .walk_for_scope(child_idx, target, result)
                    .then_some(true)
            })
            .is_some();

        if creates_scope {
            self.pop_scope();
        }

        found
    }

    /// Find all references to a symbol in the AST.
    pub fn find_references(&mut self, root: NodeIndex, target_symbol: SymbolId) -> Vec<NodeIndex> {
        let mut refs = Vec::new();

        // Get the target symbol's name
        let target_name = self
            .binder
            .symbols
            .get(target_symbol)
            .map(|s| s.escaped_name.clone());

        if let Some(name) = target_name {
            self.collect_references(root, &name, target_symbol, &mut refs);
        }

        refs
    }

    /// Recursively collect references to a symbol.
    fn collect_references(
        &mut self,
        current: NodeIndex,
        target_name: &str,
        target_symbol: SymbolId,
        refs: &mut Vec<NodeIndex>,
    ) {
        let Some(node) = self.arena.get(current) else {
            return;
        };

        // 1. Manage Scope (same logic as walk_to_node)
        let creates_scope = self.node_creates_scope(current);

        if creates_scope {
            let is_function_scope = self.node_creates_function_scope(current);
            self.push_scope(is_function_scope);
            if is_function_scope {
                self.register_hoisted_var_declarations(current);
            }
            self.register_local_declarations(current);
        }

        // 2. Check if this is an identifier with matching text
        if (node.kind == SyntaxKind::Identifier as u16
            || node.kind == SyntaxKind::PrivateIdentifier as u16)
            && let Some(text) = self.arena.get_identifier_text(current)
            && text == target_name
        {
            // Check if this is a declaration
            if let Some(&sym_id) = self.binder.node_symbols.get(&current.0) {
                // This is a declaration, check if it's the right symbol
                if sym_id == target_symbol {
                    refs.push(current);
                }
            } else {
                // It's a usage - resolve using CURRENT scope stack (O(1))
                let resolved_sym = if node.kind == SyntaxKind::PrivateIdentifier as u16 {
                    self.binder.resolve_identifier(self.arena, current)
                } else {
                    self.resolve_name(text)
                };
                if let Some(resolved_sym) = resolved_sym
                    && resolved_sym == target_symbol
                {
                    refs.push(current);
                }
            }
        }
        // 2b. Support arbitrary module namespace identifiers represented as
        // string literals in import/export specifiers.
        if node.kind == SyntaxKind::StringLiteral as u16
            && let Some(text) = self.arena.get_literal_text(current)
            && text == target_name
            && let Some(resolved_sym) = self.resolve_module_namespace_string_symbol(current)
            && resolved_sym == target_symbol
        {
            refs.push(current);
        }

        // 3. Recurse into children
        self.for_each_child(current, |walker, child_idx| {
            walker.collect_references(child_idx, target_name, target_symbol, refs);
            None::<()> // Continue iteration
        });

        // 4. Pop Scope
        if creates_scope {
            self.pop_scope();
        }
    }
}
