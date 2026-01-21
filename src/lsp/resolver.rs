//! Symbol resolution for LSP operations.
//!
//! The Binder maps declaration nodes to symbols, but LSP needs to resolve
//! identifier *usages* to symbols as well. This module provides a lightweight
//! scope walker that reconstructs scope chains on demand.

use crate::binder::BinderState;
use crate::binder::{SymbolId, SymbolTable};
use crate::parser::node::{Node, NodeAccess, NodeArena};
use crate::parser::{NodeIndex, node_flags, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;

#[derive(Debug, Default, Clone)]
pub struct ScopeCacheStats {
    pub hits: u32,
    pub misses: u32,
}

impl ScopeCacheStats {
    fn record_hit(&mut self) {
        self.hits = self.hits.saturating_add(1);
    }

    fn record_miss(&mut self) {
        self.misses = self.misses.saturating_add(1);
    }
}

pub type ScopeCache = FxHashMap<u32, Vec<SymbolTable>>;

/// A lightweight scope chain reconstructed on demand.
///
/// This mimics the binder's scope logic but focuses on resolving identifiers
/// to symbols, rather than creating new symbols.
pub struct ScopeWalker<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    /// Stack of active scopes (maps name -> SymbolId)
    scope_stack: Vec<SymbolTable>,
    /// Indices of function-scoped entries within scope_stack
    function_scope_indices: Vec<usize>,
}

impl<'a> ScopeWalker<'a> {
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
            return Self::resolve_name_in_scopes(scopes, name);
        }

        let scopes = self.get_scope_chain(root, target);
        let symbol_id = Self::resolve_name_in_scopes(&scopes, name);
        cache.insert(target.0, scopes);
        if let Some(stats) = stats {
            stats.record_miss();
        }
        symbol_id
    }

    /// Iterate over direct children of a node using proper typed accessors.
    ///
    /// The callback `f` receives the walker and the child node index.
    /// It should return `Some(T)` to stop iteration with a result, or `None` to continue.
    fn for_each_child<T, F>(&mut self, node_idx: NodeIndex, mut f: F) -> Option<T>
    where
        F: FnMut(&mut Self, NodeIndex) -> Option<T>,
    {
        let node = self.arena.get(node_idx)?;

        match node.kind {
            // --- Source File & Blocks ---
            k if k == syntax_kind_ext::SOURCE_FILE => {
                if let Some(sf) = self.arena.get_source_file(node) {
                    for &stmt in &sf.statements.nodes {
                        if let Some(res) = f(self, stmt) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::BLOCK
                || k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
                || k == syntax_kind_ext::CASE_BLOCK =>
            {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        if let Some(res) = f(self, stmt) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::MODULE_BLOCK => {
                if let Some(mod_block) = self.arena.get_module_block(node)
                    && let Some(ref stmts) = mod_block.statements
                {
                    for &stmt in &stmts.nodes {
                        if let Some(res) = f(self, stmt) {
                            return Some(res);
                        }
                    }
                }
            }

            // --- Declarations ---
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                if let Some(func) = self.arena.get_function(node) {
                    if let Some(ref modifiers) = func.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if !func.name.is_none()
                        && let Some(res) = f(self, func.name)
                    {
                        return Some(res);
                    }
                    if let Some(ref type_params) = func.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    for &param in &func.parameters.nodes {
                        if let Some(res) = f(self, param) {
                            return Some(res);
                        }
                    }
                    if !func.type_annotation.is_none()
                        && let Some(res) = f(self, func.type_annotation)
                    {
                        return Some(res);
                    }
                    if !func.body.is_none()
                        && let Some(res) = f(self, func.body)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    if let Some(ref modifiers) = method.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if !method.name.is_none()
                        && let Some(res) = f(self, method.name)
                    {
                        return Some(res);
                    }
                    if let Some(ref type_params) = method.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    for &param in &method.parameters.nodes {
                        if let Some(res) = f(self, param) {
                            return Some(res);
                        }
                    }
                    if !method.type_annotation.is_none()
                        && let Some(res) = f(self, method.type_annotation)
                    {
                        return Some(res);
                    }
                    if !method.body.is_none()
                        && let Some(res) = f(self, method.body)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                if let Some(ctor) = self.arena.get_constructor(node) {
                    if let Some(ref modifiers) = ctor.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(ref type_params) = ctor.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    for &param in &ctor.parameters.nodes {
                        if let Some(res) = f(self, param) {
                            return Some(res);
                        }
                    }
                    if !ctor.body.is_none()
                        && let Some(res) = f(self, ctor.body)
                    {
                        return Some(res);
                    }
                }
            }

            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                if let Some(class) = self.arena.get_class(node) {
                    if let Some(ref modifiers) = class.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if !class.name.is_none()
                        && let Some(res) = f(self, class.name)
                    {
                        return Some(res);
                    }
                    if let Some(ref type_params) = class.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(ref heritage) = class.heritage_clauses {
                        for &clause in &heritage.nodes {
                            if let Some(res) = f(self, clause) {
                                return Some(res);
                            }
                        }
                    }
                    for &member in &class.members.nodes {
                        if let Some(res) = f(self, member) {
                            return Some(res);
                        }
                    }
                }
            }

            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var) = self.arena.get_variable(node) {
                    for &decl_list in &var.declarations.nodes {
                        if let Some(res) = f(self, decl_list) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                if let Some(list) = self.arena.get_variable(node) {
                    for &decl in &list.declarations.nodes {
                        if let Some(res) = f(self, decl) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(node) {
                    if let Some(res) = f(self, decl.name) {
                        return Some(res);
                    }
                    if !decl.type_annotation.is_none()
                        && let Some(res) = f(self, decl.type_annotation)
                    {
                        return Some(res);
                    }
                    if !decl.initializer.is_none()
                        && let Some(res) = f(self, decl.initializer)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::PARAMETER => {
                if let Some(param) = self.arena.get_parameter(node) {
                    if let Some(ref modifiers) = param.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(res) = f(self, param.name) {
                        return Some(res);
                    }
                    if !param.type_annotation.is_none()
                        && let Some(res) = f(self, param.type_annotation)
                    {
                        return Some(res);
                    }
                    if !param.initializer.is_none()
                        && let Some(res) = f(self, param.initializer)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.arena.get_property_decl(node) {
                    if let Some(ref modifiers) = prop.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(res) = f(self, prop.name) {
                        return Some(res);
                    }
                    if !prop.type_annotation.is_none()
                        && let Some(res) = f(self, prop.type_annotation)
                    {
                        return Some(res);
                    }
                    if !prop.initializer.is_none()
                        && let Some(res) = f(self, prop.initializer)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::DECORATOR => {
                if let Some(decorator) = self.arena.get_decorator(node)
                    && let Some(res) = f(self, decorator.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    if let Some(ref modifiers) = accessor.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(res) = f(self, accessor.name) {
                        return Some(res);
                    }
                    if let Some(ref type_params) = accessor.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    for &param in &accessor.parameters.nodes {
                        if let Some(res) = f(self, param) {
                            return Some(res);
                        }
                    }
                    if !accessor.type_annotation.is_none()
                        && let Some(res) = f(self, accessor.type_annotation)
                    {
                        return Some(res);
                    }
                    if !accessor.body.is_none()
                        && let Some(res) = f(self, accessor.body)
                    {
                        return Some(res);
                    }
                }
            }

            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(iface) = self.arena.get_interface(node) {
                    if !iface.name.is_none()
                        && let Some(res) = f(self, iface.name)
                    {
                        return Some(res);
                    }
                    if let Some(ref type_params) = iface.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(ref heritage) = iface.heritage_clauses {
                        for &clause in &heritage.nodes {
                            if let Some(res) = f(self, clause) {
                                return Some(res);
                            }
                        }
                    }
                    for &member in &iface.members.nodes {
                        if let Some(res) = f(self, member) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(alias) = self.arena.get_type_alias(node) {
                    if !alias.name.is_none()
                        && let Some(res) = f(self, alias.name)
                    {
                        return Some(res);
                    }
                    if let Some(ref type_params) = alias.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    if !alias.type_node.is_none()
                        && let Some(res) = f(self, alias.type_node)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node) {
                    if !enum_decl.name.is_none()
                        && let Some(res) = f(self, enum_decl.name)
                    {
                        return Some(res);
                    }
                    for &member in &enum_decl.members.nodes {
                        if let Some(res) = f(self, member) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = self.arena.get_module(node) {
                    if !module.name.is_none()
                        && let Some(res) = f(self, module.name)
                    {
                        return Some(res);
                    }
                    if !module.body.is_none()
                        && let Some(res) = f(self, module.body)
                    {
                        return Some(res);
                    }
                }
            }

            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                if let Some(import) = self.arena.get_import_decl(node)
                    && !import.import_clause.is_none()
                    && let Some(res) = f(self, import.import_clause)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export) = self.arena.get_export_decl(node)
                    && !export.export_clause.is_none()
                    && let Some(res) = f(self, export.export_clause)
                {
                    return Some(res);
                }
            }

            // --- Statements ---
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(stmt) = self.arena.get_if_statement(node) {
                    if let Some(res) = f(self, stmt.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, stmt.then_statement) {
                        return Some(res);
                    }
                    if !stmt.else_statement.is_none()
                        && let Some(res) = f(self, stmt.else_statement)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(node)
                    && !ret.expression.is_none()
                    && let Some(res) = f(self, ret.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr) = self.arena.get_expression_statement(node)
                    && let Some(res) = f(self, expr.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    if !loop_data.initializer.is_none()
                        && let Some(res) = f(self, loop_data.initializer)
                    {
                        return Some(res);
                    }
                    if !loop_data.condition.is_none()
                        && let Some(res) = f(self, loop_data.condition)
                    {
                        return Some(res);
                    }
                    if !loop_data.incrementor.is_none()
                        && let Some(res) = f(self, loop_data.incrementor)
                    {
                        return Some(res);
                    }
                    if let Some(res) = f(self, loop_data.statement) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                if let Some(for_in_of) = self.arena.get_for_in_of(node) {
                    if let Some(res) = f(self, for_in_of.initializer) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, for_in_of.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, for_in_of.statement) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT || k == syntax_kind_ext::DO_STATEMENT => {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    if !loop_data.condition.is_none()
                        && let Some(res) = f(self, loop_data.condition)
                    {
                        return Some(res);
                    }
                    if let Some(res) = f(self, loop_data.statement) {
                        return Some(res);
                    }
                }
            }

            // --- Expressions ---
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    if let Some(res) = f(self, bin.left) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, bin.right) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    if let Some(res) = f(self, call.expression) {
                        return Some(res);
                    }
                    if let Some(ref args) = call.arguments {
                        for &arg in &args.nodes {
                            if let Some(res) = f(self, arg) {
                                return Some(res);
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                if node.has_data()
                    && let Some(tagged) = self.arena.tagged_templates.get(node.data_index as usize)
                {
                    if let Some(res) = f(self, tagged.tag) {
                        return Some(res);
                    }
                    if let Some(ref type_args) = tagged.type_arguments {
                        for &arg in &type_args.nodes {
                            if let Some(res) = f(self, arg) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(res) = f(self, tagged.template) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.arena.get_access_expr(node) {
                    if let Some(res) = f(self, access.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, access.name_or_argument) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node)
                    && let Some(res) = f(self, paren.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if node.has_data()
                    && let Some(assertion) =
                        self.arena.type_assertions.get(node.data_index as usize)
                {
                    if let Some(res) = f(self, assertion.expression) {
                        return Some(res);
                    }
                    if !assertion.type_node.is_none()
                        && let Some(res) = f(self, assertion.type_node)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                if let Some(lit) = self.arena.get_literal_expr(node) {
                    for &elem in &lit.elements.nodes {
                        if let Some(res) = f(self, elem) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.arena.get_property_assignment(node) {
                    if let Some(res) = f(self, prop.name) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, prop.initializer) {
                        return Some(res);
                    }
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
                        if let Some(res) = f(self, elem) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(binding) = self.arena.get_binding_element(node) {
                    if !binding.property_name.is_none()
                        && let Some(prop_node) = self.arena.get(binding.property_name)
                        && prop_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(res) = f(self, binding.property_name)
                    {
                        return Some(res);
                    }
                    if let Some(res) = f(self, binding.name) {
                        return Some(res);
                    }
                    if !binding.initializer.is_none()
                        && let Some(res) = f(self, binding.initializer)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = self.arena.get_computed_property(node)
                    && let Some(res) = f(self, computed.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    if let Some(res) = f(self, cond.condition) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, cond.when_true) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, cond.when_false) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if let Some(template) = self.arena.get_template_expr(node) {
                    if let Some(res) = f(self, template.head) {
                        return Some(res);
                    }
                    for &span in &template.template_spans.nodes {
                        if let Some(res) = f(self, span) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_SPAN => {
                if let Some(span) = self.arena.get_template_span(node) {
                    if let Some(res) = f(self, span.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, span.literal) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                if let Some(element) = self.arena.get_jsx_element(node) {
                    if let Some(res) = f(self, element.opening_element) {
                        return Some(res);
                    }
                    for &child in &element.children.nodes {
                        if let Some(res) = f(self, child) {
                            return Some(res);
                        }
                    }
                    if let Some(res) = f(self, element.closing_element) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                || k == syntax_kind_ext::JSX_OPENING_ELEMENT =>
            {
                if let Some(opening) = self.arena.get_jsx_opening(node) {
                    if let Some(res) = f(self, opening.tag_name) {
                        return Some(res);
                    }
                    if let Some(ref type_args) = opening.type_arguments {
                        for &arg in &type_args.nodes {
                            if let Some(res) = f(self, arg) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(res) = f(self, opening.attributes) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_CLOSING_ELEMENT => {
                if let Some(closing) = self.arena.get_jsx_closing(node)
                    && let Some(res) = f(self, closing.tag_name)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::JSX_FRAGMENT => {
                if let Some(fragment) = self.arena.get_jsx_fragment(node) {
                    if let Some(res) = f(self, fragment.opening_fragment) {
                        return Some(res);
                    }
                    for &child in &fragment.children.nodes {
                        if let Some(res) = f(self, child) {
                            return Some(res);
                        }
                    }
                    if let Some(res) = f(self, fragment.closing_fragment) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTES => {
                if let Some(attrs) = self.arena.get_jsx_attributes(node) {
                    for &prop in &attrs.properties.nodes {
                        if let Some(res) = f(self, prop) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTE => {
                if let Some(attr) = self.arena.get_jsx_attribute(node) {
                    if let Some(res) = f(self, attr.name) {
                        return Some(res);
                    }
                    if !attr.initializer.is_none()
                        && let Some(res) = f(self, attr.initializer)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE => {
                if let Some(spread) = self.arena.get_jsx_spread_attribute(node)
                    && let Some(res) = f(self, spread.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::JSX_EXPRESSION => {
                if let Some(expr) = self.arena.get_jsx_expression(node)
                    && !expr.expression.is_none()
                    && let Some(res) = f(self, expr.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::JSX_NAMESPACED_NAME => {
                if let Some(ns) = self.arena.get_jsx_namespaced_name(node) {
                    if let Some(res) = f(self, ns.namespace) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, ns.name) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = self.arena.get_unary_expr(node)
                    && let Some(res) = f(self, unary.operand)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION =>
            {
                if node.has_data()
                    && let Some(unary) = self.arena.unary_exprs_ex.get(node.data_index as usize)
                    && let Some(res) = f(self, unary.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.arena.get_shorthand_property(node) {
                    if let Some(res) = f(self, prop.name) {
                        return Some(res);
                    }
                    if !prop.object_assignment_initializer.is_none()
                        && let Some(res) = f(self, prop.object_assignment_initializer)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT
                || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
            {
                if let Some(spread) = self.arena.get_spread(node)
                    && let Some(res) = f(self, spread.expression)
                {
                    return Some(res);
                }
            }

            // --- Types ---
            k if k == syntax_kind_ext::HERITAGE_CLAUSE => {
                if let Some(heritage) = self.arena.get_heritage_clause(node) {
                    for &ty in &heritage.types.nodes {
                        if let Some(res) = f(self, ty) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(expr) = self.arena.get_expr_type_args(node) {
                    if let Some(res) = f(self, expr.expression) {
                        return Some(res);
                    }
                    if let Some(ref type_args) = expr.type_arguments {
                        for &arg in &type_args.nodes {
                            if let Some(res) = f(self, arg) {
                                return Some(res);
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.arena.get_type_ref(node) {
                    if let Some(res) = f(self, type_ref.type_name) {
                        return Some(res);
                    }
                    if let Some(ref type_args) = type_ref.type_arguments {
                        for &arg in &type_args.nodes {
                            if let Some(res) = f(self, arg) {
                                return Some(res);
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                if let Some(qualified) = self.arena.get_qualified_name(node) {
                    if let Some(res) = f(self, qualified.left) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, qualified.right) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                if let Some(query) = self.arena.get_type_query(node) {
                    if let Some(res) = f(self, query.expr_name) {
                        return Some(res);
                    }
                    if let Some(ref type_args) = query.type_arguments {
                        for &arg in &type_args.nodes {
                            if let Some(res) = f(self, arg) {
                                return Some(res);
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.arena.get_type_operator(node)
                    && let Some(res) = f(self, op.type_node)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(pred) = self.arena.get_type_predicate(node) {
                    if let Some(res) = f(self, pred.parameter_name) {
                        return Some(res);
                    }
                    if !pred.type_node.is_none()
                        && let Some(res) = f(self, pred.type_node)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                if let Some(param) = self.arena.get_type_parameter(node) {
                    if let Some(res) = f(self, param.name) {
                        return Some(res);
                    }
                    if !param.constraint.is_none()
                        && let Some(res) = f(self, param.constraint)
                    {
                        return Some(res);
                    }
                    if !param.default.is_none()
                        && let Some(res) = f(self, param.default)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.arena.get_function_type(node) {
                    if let Some(ref type_params) = func_type.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    for &param in &func_type.parameters.nodes {
                        if let Some(res) = f(self, param) {
                            return Some(res);
                        }
                    }
                    if !func_type.type_annotation.is_none()
                        && let Some(res) = f(self, func_type.type_annotation)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(literal) = self.arena.get_type_literal(node) {
                    for &member in &literal.members.nodes {
                        if let Some(res) = f(self, member) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE
                || k == syntax_kind_ext::METHOD_SIGNATURE
                || k == syntax_kind_ext::CALL_SIGNATURE
                || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
            {
                if let Some(sig) = self.arena.get_signature(node) {
                    if !sig.name.is_none()
                        && let Some(res) = f(self, sig.name)
                    {
                        return Some(res);
                    }
                    if let Some(ref type_params) = sig.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(ref params) = sig.parameters {
                        for &param in &params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    if !sig.type_annotation.is_none()
                        && let Some(res) = f(self, sig.type_annotation)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                if let Some(sig) = self.arena.get_index_signature(node) {
                    for &param in &sig.parameters.nodes {
                        if let Some(res) = f(self, param) {
                            return Some(res);
                        }
                    }
                    if !sig.type_annotation.is_none()
                        && let Some(res) = f(self, sig.type_annotation)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(array) = self.arena.get_array_type(node)
                    && let Some(res) = f(self, array.element_type)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.arena.get_tuple_type(node) {
                    for &elem in &tuple.elements.nodes {
                        if let Some(res) = f(self, elem) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                if let Some(member) = self.arena.get_named_tuple_member(node) {
                    if let Some(res) = f(self, member.name) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, member.type_node) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(comp) = self.arena.get_composite_type(node) {
                    for &ty in &comp.types.nodes {
                        if let Some(res) = f(self, ty) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.arena.get_conditional_type(node) {
                    if let Some(res) = f(self, cond.check_type) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, cond.extends_type) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, cond.true_type) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, cond.false_type) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                if let Some(wrapped) = self.arena.get_wrapped_type(node)
                    && let Some(res) = f(self, wrapped.type_node)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.arena.get_infer_type(node)
                    && let Some(res) = f(self, infer.type_parameter)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.arena.get_indexed_access_type(node) {
                    if let Some(res) = f(self, indexed.object_type) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, indexed.index_type) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.arena.get_mapped_type(node) {
                    if let Some(res) = f(self, mapped.type_parameter) {
                        return Some(res);
                    }
                    if !mapped.name_type.is_none()
                        && let Some(res) = f(self, mapped.name_type)
                    {
                        return Some(res);
                    }
                    if !mapped.type_node.is_none()
                        && let Some(res) = f(self, mapped.type_node)
                    {
                        return Some(res);
                    }
                    if let Some(ref members) = mapped.members {
                        for &member in &members.nodes {
                            if let Some(res) = f(self, member) {
                                return Some(res);
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                if let Some(lit) = self.arena.get_literal_type(node)
                    && let Some(res) = f(self, lit.literal)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.arena.get_template_literal_type(node) {
                    if let Some(res) = f(self, template.head) {
                        return Some(res);
                    }
                    for &span in &template.template_spans.nodes {
                        if let Some(res) = f(self, span) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN => {
                if let Some(span) = self.arena.get_template_span(node) {
                    if let Some(res) = f(self, span.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, span.literal) {
                        return Some(res);
                    }
                }
            }

            // --- Control Flow ---
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.arena.get_try(node) {
                    if let Some(res) = f(self, try_stmt.try_block) {
                        return Some(res);
                    }
                    if !try_stmt.catch_clause.is_none()
                        && let Some(res) = f(self, try_stmt.catch_clause)
                    {
                        return Some(res);
                    }
                    if !try_stmt.finally_block.is_none()
                        && let Some(res) = f(self, try_stmt.finally_block)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch) = self.arena.get_catch_clause(node) {
                    if !catch.variable_declaration.is_none()
                        && let Some(res) = f(self, catch.variable_declaration)
                    {
                        return Some(res);
                    }
                    if let Some(res) = f(self, catch.block) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch) = self.arena.get_switch(node) {
                    if let Some(res) = f(self, switch.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, switch.case_block) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                if let Some(assign) = self.arena.get_export_assignment(node)
                    && let Some(res) = f(self, assign.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.arena.get_labeled_statement(node) {
                    if let Some(res) = f(self, labeled.label) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, labeled.statement) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_stmt) = self.arena.get_with_statement(node) {
                    if let Some(res) = f(self, with_stmt.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, with_stmt.then_statement) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(case) = self.arena.get_case_clause(node) {
                    if !case.expression.is_none()
                        && let Some(res) = f(self, case.expression)
                    {
                        return Some(res);
                    }
                    for &stmt in &case.statements.nodes {
                        if let Some(res) = f(self, stmt) {
                            return Some(res);
                        }
                    }
                }
            }

            // --- Default: no children or not yet implemented ---
            _ => {}
        }

        None
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
                        return self.resolve_name(text);
                    }
                } else if node.kind == SyntaxKind::PrivateIdentifier as u16 {
                    return self.binder.resolve_identifier(self.arena, current);
                }
            }
            return None;
        }

        let Some(node) = self.arena.get(current) else {
            return None;
        };

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

    fn is_var_declaration_list(&self, node: &Node) -> bool {
        (node.flags as u32 & (node_flags::LET | node_flags::CONST)) == 0
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
                        && !export.export_clause.is_none()
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
                if walker.walk_for_scope(child_idx, target, result) {
                    Some(true)
                } else {
                    None
                }
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

#[cfg(test)]
mod resolver_tests {

    use crate::binder::BinderState;
    use crate::parser::ParserState;

    #[test]
    fn test_resolve_simple_variable() {
        // const x = 1; x + 1;
        let source = "const x = 1; x + 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        // Should have a symbol for 'x'
        assert!(binder.file_locals.get("x").is_some());
    }
}
