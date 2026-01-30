//! Call Hierarchy implementation for LSP.
//!
//! Provides call hierarchy support that shows incoming and outgoing calls
//! for a given function or method symbol:
//! - `prepare`: identifies the function/method at a cursor position
//! - `incoming_calls`: finds all callers of a function
//! - `outgoing_calls`: finds all callees within a function body

use std::collections::HashMap;

use crate::binder::BinderState;
use crate::lsp::document_symbols::SymbolKind;
use crate::lsp::position::{LineMap, Position, Range};
use crate::lsp::utils::find_node_at_offset;
use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, syntax_kind_ext};
use crate::scanner::SyntaxKind;

/// An item in the call hierarchy (represents a function, method, or constructor).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallHierarchyItem {
    /// The name of the function/method.
    pub name: String,
    /// The kind of this symbol (Function, Method, Constructor, etc.).
    pub kind: SymbolKind,
    /// The URI of the file containing this symbol.
    pub uri: String,
    /// The range enclosing the entire function/method.
    pub range: Range,
    /// The range of the function/method name (selection range).
    pub selection_range: Range,
}

/// An incoming call (a caller of the target function).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallHierarchyIncomingCall {
    /// The calling function/method.
    pub from: CallHierarchyItem,
    /// The ranges within `from` where the target is called.
    pub from_ranges: Vec<Range>,
}

/// An outgoing call (a callee from within the target function).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallHierarchyOutgoingCall {
    /// The called function/method.
    pub to: CallHierarchyItem,
    /// The ranges within the source function where the callee is invoked.
    pub from_ranges: Vec<Range>,
}

/// Provider for call hierarchy operations.
pub struct CallHierarchyProvider<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    file_name: String,
    source_text: &'a str,
}

impl<'a> CallHierarchyProvider<'a> {
    /// Create a new call hierarchy provider.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        file_name: String,
        source_text: &'a str,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            file_name,
            source_text,
        }
    }

    /// Prepare a call hierarchy item at the given position.
    ///
    /// Finds the function, method, or constructor at the cursor and returns
    /// a `CallHierarchyItem` describing it. Returns `None` if the cursor
    /// is not on a callable symbol.
    pub fn prepare(&self, _root: NodeIndex, position: Position) -> Option<CallHierarchyItem> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        // Walk up from the found node to find the enclosing function-like node.
        // If the cursor is on an identifier that is the name of a function, use
        // the function itself; otherwise walk parents.
        let func_idx = self.find_function_at_or_around(node_idx)?;
        self.make_call_hierarchy_item(func_idx)
    }

    /// Find all incoming calls (callers) for the function at the given position.
    ///
    /// Scans the AST for identifier nodes whose `escaped_text` matches the
    /// target function name and that appear inside `CallExpression` nodes,
    /// then groups them by their containing function to produce caller items.
    pub fn incoming_calls(
        &self,
        _root: NodeIndex,
        position: Position,
    ) -> Vec<CallHierarchyIncomingCall> {
        let mut results = Vec::new();

        let offset = match self.line_map.position_to_offset(position, self.source_text) {
            Some(o) => o,
            None => return results,
        };

        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return results;
        }

        // Find the function-like node at this position
        let func_idx = match self.find_function_at_or_around(node_idx) {
            Some(idx) => idx,
            None => return results,
        };

        // Get the name of this function via escaped_text (avoids interner dependency)
        let target_name = match self.get_function_name_idx(func_idx) {
            Some(name_idx) => match self.get_identifier_text(name_idx) {
                Some(name) => name,
                None => return results,
            },
            None => return results,
        };

        let name_idx = self.get_function_name_idx(func_idx);

        // Scan all identifier nodes in the arena that match the target name
        // and appear inside a CallExpression, grouping by containing function.
        let mut callers: HashMap<u32, Vec<Range>> = HashMap::new();

        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let idx = NodeIndex(i as u32);

            // Skip the declaration name itself
            if name_idx == Some(idx) {
                continue;
            }

            // Check if this identifier's escaped_text matches
            let ident_data = match self.arena.get_identifier(node) {
                Some(d) => d,
                None => continue,
            };
            if ident_data.escaped_text != target_name {
                continue;
            }

            // Check if this identifier is inside a CallExpression
            if !self.is_inside_call_expression(idx) {
                continue;
            }

            // Walk up to find the containing function
            if let Some(containing_func) = self.find_containing_function(idx) {
                // Don't list the function as calling itself from its own declaration
                if containing_func == func_idx {
                    continue;
                }
                let range = self.get_range(idx);
                callers.entry(containing_func.0).or_default().push(range);
            }
        }

        // Convert grouped callers into CallHierarchyIncomingCall items
        for (caller_idx_raw, ranges) in callers {
            let caller_idx = NodeIndex(caller_idx_raw);
            if let Some(item) = self.make_call_hierarchy_item(caller_idx) {
                results.push(CallHierarchyIncomingCall {
                    from: item,
                    from_ranges: ranges,
                });
            }
        }

        results
    }

    /// Find all outgoing calls from the function at the given position.
    ///
    /// Walks the function's body AST looking for `CallExpression` nodes,
    /// then resolves each callee to build outgoing call items.
    pub fn outgoing_calls(
        &self,
        _root: NodeIndex,
        position: Position,
    ) -> Vec<CallHierarchyOutgoingCall> {
        let mut results = Vec::new();

        let offset = match self.line_map.position_to_offset(position, self.source_text) {
            Some(o) => o,
            None => return results,
        };

        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return results;
        }

        // Find the function-like node
        let func_idx = match self.find_function_at_or_around(node_idx) {
            Some(idx) => idx,
            None => return results,
        };

        // Get the function body
        let body_idx = match self.get_function_body(func_idx) {
            Some(idx) => idx,
            None => return results,
        };

        // Collect all CallExpression nodes within the body
        let mut call_nodes = Vec::new();
        self.collect_call_expressions(body_idx, &mut call_nodes);

        // Group calls by the resolved callee.
        // Key: NodeIndex of the callee's declaration, Value: list of call-site ranges.
        let mut callees: HashMap<u32, (Option<CallHierarchyItem>, Vec<Range>)> = HashMap::new();

        for call_idx in call_nodes {
            let call_node = match self.arena.get(call_idx) {
                Some(n) => n,
                None => continue,
            };

            let call_data = match self.arena.get_call_expr(call_node) {
                Some(d) => d,
                None => continue,
            };

            // Get the callee expression (the thing being called)
            let callee_expr = call_data.expression;
            if callee_expr.is_none() {
                continue;
            }

            // Determine the identifier to resolve
            let callee_ident = self.get_callee_identifier(callee_expr);
            if callee_ident.is_none() {
                continue;
            }

            // Build the call-site range (the range of the callee expression in the source)
            let call_range = self.get_range(callee_ident);

            // Resolve the callee to its declaration using escaped_text and the
            // binder's symbol tables. This avoids depending on
            // ScopeWalker::resolve_node which requires the arena interner to be
            // populated (not the case when the arena is borrowed).
            let resolved = self.resolve_callee_symbol(callee_ident);

            if let Some((symbol_id, decl_idx, name)) = resolved {
                let _ = symbol_id; // used implicitly via decl_idx
                let entry = callees.entry(decl_idx.0).or_insert_with(|| {
                    let item = self.make_call_hierarchy_item_for_declaration(decl_idx, &name);
                    (item, Vec::new())
                });
                entry.1.push(call_range);
            } else if let Some(ident_node) = self.arena.get(callee_ident) {
                // Last-resort fallback: no symbol found at all
                if let Some(ident_data) = self.arena.get_identifier(ident_node) {
                    let name = ident_data.escaped_text.clone();
                    let ident_range = self.get_range(callee_ident);
                    let entry = callees.entry(callee_ident.0).or_insert_with(|| {
                        let item = CallHierarchyItem {
                            name,
                            kind: SymbolKind::Function,
                            uri: self.file_name.clone(),
                            range: ident_range,
                            selection_range: ident_range,
                        };
                        (Some(item), Vec::new())
                    });
                    entry.1.push(call_range);
                }
            }
        }

        // Convert grouped callees into CallHierarchyOutgoingCall items
        for (_decl_idx, (item_opt, ranges)) in callees {
            if let Some(item) = item_opt {
                results.push(CallHierarchyOutgoingCall {
                    to: item,
                    from_ranges: ranges,
                });
            }
        }

        results
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Resolve a callee identifier to its symbol using `escaped_text` and
    /// the binder's symbol tables directly. This avoids the `ScopeWalker`
    /// path which depends on `arena.get_identifier_text()` (atom-based).
    ///
    /// Returns `(SymbolId, declaration NodeIndex, name)` on success.
    fn resolve_callee_symbol(
        &self,
        ident_idx: NodeIndex,
    ) -> Option<(crate::binder::SymbolId, NodeIndex, String)> {
        // 1. Check if the node itself is a declaration in node_symbols
        if let Some(&sym_id) = self.binder.node_symbols.get(&ident_idx.0) {
            if let Some(symbol) = self.binder.symbols.get(sym_id) {
                if let Some(&decl) = symbol.declarations.first() {
                    return Some((sym_id, decl, symbol.escaped_name.clone()));
                }
            }
        }

        // 2. Get the identifier's escaped_text and look it up in file_locals
        let node = self.arena.get(ident_idx)?;
        let ident_data = self.arena.get_identifier(node)?;
        let name = &ident_data.escaped_text;

        if let Some(sym_id) = self.binder.file_locals.get(name) {
            if let Some(symbol) = self.binder.symbols.get(sym_id) {
                if let Some(&decl) = symbol.declarations.first() {
                    return Some((sym_id, decl, name.clone()));
                }
            }
        }

        None
    }

    /// Check whether a node kind is a function-like declaration.
    fn is_function_like(&self, kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || kind == syntax_kind_ext::ARROW_FUNCTION
            || kind == syntax_kind_ext::METHOD_DECLARATION
            || kind == syntax_kind_ext::CONSTRUCTOR
            || kind == syntax_kind_ext::GET_ACCESSOR
            || kind == syntax_kind_ext::SET_ACCESSOR
    }

    /// Find the function-like node at or containing the given node.
    ///
    /// First checks whether `node_idx` itself is a function-like node or the
    /// name of one. Then walks up through parents.
    fn find_function_at_or_around(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        if node_idx.is_none() {
            return None;
        }

        let node = self.arena.get(node_idx)?;

        // If we are directly on a function-like node, return it.
        if self.is_function_like(node.kind) {
            return Some(node_idx);
        }

        // If the node is an identifier, check if its parent is a function-like
        // declaration (i.e., we are on the function name).
        if node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ext) = self.arena.get_extended(node_idx) {
                let parent = ext.parent;
                if !parent.is_none() {
                    if let Some(parent_node) = self.arena.get(parent) {
                        if self.is_function_like(parent_node.kind) {
                            return Some(parent);
                        }
                    }
                }
            }
        }

        // Walk up through parents to find an enclosing function-like node.
        self.find_containing_function(node_idx)
    }

    /// Walk up the parent chain to find the nearest containing function-like node.
    fn find_containing_function(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        loop {
            let ext = self.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;
            if self.is_function_like(parent_node.kind) {
                return Some(parent);
            }
            current = parent;
        }
    }

    /// Check if a node is inside a CallExpression (i.e., used as the callee or argument).
    fn is_inside_call_expression(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        // Walk up a few levels to see if we hit a CallExpression
        for _ in 0..5 {
            if let Some(ext) = self.arena.get_extended(current) {
                let parent = ext.parent;
                if parent.is_none() {
                    return false;
                }
                if let Some(parent_node) = self.arena.get(parent) {
                    if parent_node.kind == syntax_kind_ext::CALL_EXPRESSION {
                        return true;
                    }
                    // Also count NewExpression as a "call"
                    if parent_node.kind == syntax_kind_ext::NEW_EXPRESSION {
                        return true;
                    }
                }
                current = parent;
            } else {
                return false;
            }
        }
        false
    }

    /// Get the name NodeIndex of a function-like node.
    fn get_function_name_idx(&self, func_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(func_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                let func = self.arena.get_function(node)?;
                if func.name.is_none() {
                    None
                } else {
                    Some(func.name)
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                if method.name.is_none() {
                    None
                } else {
                    Some(method.name)
                }
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                // Constructors don't have a name node, return the function node itself
                None
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.arena.get_accessor(node)?;
                if accessor.name.is_none() {
                    None
                } else {
                    Some(accessor.name)
                }
            }
            _ => None,
        }
    }

    /// Get the body NodeIndex of a function-like node.
    fn get_function_body(&self, func_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(func_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                let func = self.arena.get_function(node)?;
                if func.body.is_none() {
                    None
                } else {
                    Some(func.body)
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                if method.body.is_none() {
                    None
                } else {
                    Some(method.body)
                }
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                let ctor = self.arena.get_constructor(node)?;
                if ctor.body.is_none() {
                    None
                } else {
                    Some(ctor.body)
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.arena.get_accessor(node)?;
                if accessor.body.is_none() {
                    None
                } else {
                    Some(accessor.body)
                }
            }
            _ => None,
        }
    }

    /// Recursively collect all CallExpression nodes within a subtree.
    ///
    /// Uses a simple offset-range scan: any node in the arena whose kind is
    /// `CALL_EXPRESSION` and whose [pos, end) is within the body range is
    /// collected. This avoids the need for a full recursive visitor.
    fn collect_call_expressions(&self, body_idx: NodeIndex, out: &mut Vec<NodeIndex>) {
        let body_node = match self.arena.get(body_idx) {
            Some(n) => n,
            None => return,
        };
        let body_start = body_node.pos;
        let body_end = body_node.end;

        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                && node.pos >= body_start
                && node.end <= body_end
            {
                out.push(NodeIndex(i as u32));
            }
        }
    }

    /// Extract the callee identifier from a call expression's `expression` field.
    ///
    /// Handles:
    /// - Simple identifiers: `foo()` -> `foo`
    /// - Property access: `obj.method()` -> `method`
    fn get_callee_identifier(&self, expr_idx: NodeIndex) -> NodeIndex {
        if expr_idx.is_none() {
            return NodeIndex::NONE;
        }
        let expr_node = match self.arena.get(expr_idx) {
            Some(n) => n,
            None => return NodeIndex::NONE,
        };

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => expr_idx,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // For property access (a.b), the identifier is name_or_argument
                if let Some(access) = self.arena.get_access_expr(expr_node) {
                    if !access.name_or_argument.is_none() {
                        access.name_or_argument
                    } else {
                        NodeIndex::NONE
                    }
                } else {
                    NodeIndex::NONE
                }
            }
            _ => NodeIndex::NONE,
        }
    }

    /// Get the name text of a function-like node.
    fn get_function_name(&self, func_idx: NodeIndex) -> String {
        let node = match self.arena.get(func_idx) {
            Some(n) => n,
            None => return "<anonymous>".to_string(),
        };

        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                if let Some(func) = self.arena.get_function(node) {
                    self.get_identifier_text(func.name)
                        .unwrap_or_else(|| "<anonymous>".to_string())
                } else {
                    "<anonymous>".to_string()
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    self.get_identifier_text(method.name)
                        .unwrap_or_else(|| "<method>".to_string())
                } else {
                    "<method>".to_string()
                }
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => "constructor".to_string(),
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let name = self
                        .get_identifier_text(accessor.name)
                        .unwrap_or_else(|| "<accessor>".to_string());
                    format!("get {}", name)
                } else {
                    "get <accessor>".to_string()
                }
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    let name = self
                        .get_identifier_text(accessor.name)
                        .unwrap_or_else(|| "<accessor>".to_string());
                    format!("set {}", name)
                } else {
                    "set <accessor>".to_string()
                }
            }
            _ => "<unknown>".to_string(),
        }
    }

    /// Get the SymbolKind for a function-like node.
    fn get_function_symbol_kind(&self, func_idx: NodeIndex) -> SymbolKind {
        let node = match self.arena.get(func_idx) {
            Some(n) => n,
            None => return SymbolKind::Function,
        };
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                SymbolKind::Function
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => SymbolKind::Method,
            k if k == syntax_kind_ext::CONSTRUCTOR => SymbolKind::Constructor,
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                SymbolKind::Property
            }
            _ => SymbolKind::Function,
        }
    }

    /// Build a `CallHierarchyItem` for a function-like node.
    fn make_call_hierarchy_item(&self, func_idx: NodeIndex) -> Option<CallHierarchyItem> {
        let node = self.arena.get(func_idx)?;
        if !self.is_function_like(node.kind) {
            return None;
        }

        let name = self.get_function_name(func_idx);
        let kind = self.get_function_symbol_kind(func_idx);
        let range = self.get_range(func_idx);

        // Selection range is the name identifier range, or the keyword range
        let selection_range = if let Some(name_idx) = self.get_function_name_idx(func_idx) {
            self.get_range(name_idx)
        } else {
            // For constructors or anonymous functions, use a small range at the start
            let start = self.line_map.offset_to_position(node.pos, self.source_text);
            let end = self
                .line_map
                .offset_to_position(node.pos.saturating_add(11), self.source_text); // "constructor" or similar
            Range::new(start, end)
        };

        Some(CallHierarchyItem {
            name,
            kind,
            uri: self.file_name.clone(),
            range,
            selection_range,
        })
    }

    /// Build a `CallHierarchyItem` for a declaration node that may be
    /// a function or may be a variable holding a function expression.
    fn make_call_hierarchy_item_for_declaration(
        &self,
        decl_idx: NodeIndex,
        symbol_name: &str,
    ) -> Option<CallHierarchyItem> {
        let node = self.arena.get(decl_idx)?;

        // If the declaration itself is function-like, use make_call_hierarchy_item
        if self.is_function_like(node.kind) {
            return self.make_call_hierarchy_item(decl_idx);
        }

        // Otherwise (e.g. variable declaration), build an item from the symbol info
        let range = self.get_range(decl_idx);
        Some(CallHierarchyItem {
            name: symbol_name.to_string(),
            kind: SymbolKind::Function,
            uri: self.file_name.clone(),
            range,
            selection_range: range,
        })
    }

    /// Get the text of an identifier node.
    fn get_identifier_text(&self, node_idx: NodeIndex) -> Option<String> {
        if node_idx.is_none() {
            return None;
        }
        let node = self.arena.get(node_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            self.arena
                .get_identifier(node)
                .map(|id| id.escaped_text.clone())
        } else {
            None
        }
    }

    /// Convert a node to an LSP Range.
    fn get_range(&self, node_idx: NodeIndex) -> Range {
        if let Some(node) = self.arena.get(node_idx) {
            let start = self.line_map.offset_to_position(node.pos, self.source_text);
            let end = self.line_map.offset_to_position(node.end, self.source_text);
            Range::new(start, end)
        } else {
            Range::new(Position::new(0, 0), Position::new(0, 0))
        }
    }
}

#[cfg(test)]
mod call_hierarchy_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;

    /// Helper: parse, bind, and create a provider.
    fn setup(source: &str) -> (NodeIndex, CallHierarchyProvider<'static>) {
        // We leak the parser/binder to get 'static references for test convenience.
        let source_owned = source.to_string();
        let mut parser = ParserState::new("test.ts".to_string(), source_owned.clone());
        let root = parser.parse_source_file();

        // Leak the parser to extend the arena lifetime
        let parser_box = Box::new(parser);
        let parser_ref: &'static mut ParserState = Box::leak(parser_box);
        let arena: &'static NodeArena = parser_ref.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);
        let binder_ref: &'static BinderState = Box::leak(Box::new(binder));

        let source_leaked: &'static str = Box::leak(source_owned.into_boxed_str());
        let line_map = LineMap::build(source_leaked);
        let line_map_ref: &'static LineMap = Box::leak(Box::new(line_map));

        let provider = CallHierarchyProvider::new(
            arena,
            binder_ref,
            line_map_ref,
            "test.ts".to_string(),
            source_leaked,
        );

        (root, provider)
    }

    #[test]
    fn test_prepare_on_function_declaration() {
        let source = "function foo() {\n  return 1;\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at "foo" (line 0, col 9)
        let pos = Position::new(0, 9);
        let item = provider.prepare(root, pos);

        assert!(item.is_some(), "Should find call hierarchy item for 'foo'");
        let item = item.unwrap();
        assert_eq!(item.name, "foo");
        assert_eq!(item.kind, SymbolKind::Function);
    }

    #[test]
    fn test_prepare_on_method_declaration() {
        let source = "class Foo {\n  bar() {\n    return 1;\n  }\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at "bar" (line 1, col 2)
        let pos = Position::new(1, 2);
        let item = provider.prepare(root, pos);

        assert!(item.is_some(), "Should find call hierarchy item for 'bar'");
        let item = item.unwrap();
        assert_eq!(item.name, "bar");
        assert_eq!(item.kind, SymbolKind::Method);
    }

    #[test]
    fn test_prepare_not_on_function() {
        let source = "const x = 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at "x" (line 0, col 6) - a variable, not a function
        let pos = Position::new(0, 6);
        let item = provider.prepare(root, pos);

        assert!(
            item.is_none(),
            "Should not find call hierarchy item for variable"
        );
    }

    #[test]
    fn test_outgoing_calls_simple() {
        let source = "function greet() {}\nfunction main() {\n  greet();\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position inside "main" function name (line 1, col 9)
        let pos = Position::new(1, 9);
        let calls = provider.outgoing_calls(root, pos);

        assert!(!calls.is_empty(), "main should have outgoing calls");
        // Should find the call to greet
        let greet_call = calls.iter().find(|c| c.to.name == "greet");
        assert!(greet_call.is_some(), "Should find outgoing call to 'greet'");
        assert!(
            !greet_call.unwrap().from_ranges.is_empty(),
            "Should have at least one call range"
        );
    }

    #[test]
    fn test_outgoing_calls_multiple() {
        let source =
            "function a() {}\nfunction b() {}\nfunction c() {\n  a();\n  b();\n  a();\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at "c" function name (line 2, col 9)
        let pos = Position::new(2, 9);
        let calls = provider.outgoing_calls(root, pos);

        // Should find calls to a and b
        assert!(calls.len() >= 2, "Should find at least 2 outgoing targets");

        let a_call = calls.iter().find(|c| c.to.name == "a");
        assert!(a_call.is_some(), "Should find outgoing call to 'a'");
        // 'a' is called twice
        assert_eq!(
            a_call.unwrap().from_ranges.len(),
            2,
            "'a' should be called twice"
        );

        let b_call = calls.iter().find(|c| c.to.name == "b");
        assert!(b_call.is_some(), "Should find outgoing call to 'b'");
    }

    #[test]
    fn test_outgoing_calls_no_calls() {
        let source = "function empty() {\n  const x = 1;\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at "empty" (line 0, col 9)
        let pos = Position::new(0, 9);
        let calls = provider.outgoing_calls(root, pos);

        assert!(
            calls.is_empty(),
            "Function with no calls should have no outgoing calls"
        );
    }

    #[test]
    fn test_incoming_calls_simple() {
        let source = "function target() {}\nfunction caller() {\n  target();\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let provider =
            CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at "target" (line 0, col 9)
        let pos = Position::new(0, 9);
        let calls = provider.incoming_calls(root, pos);

        assert!(!calls.is_empty(), "target should have incoming calls");
        let caller_item = calls.iter().find(|c| c.from.name == "caller");
        assert!(
            caller_item.is_some(),
            "Should find incoming call from 'caller'"
        );
    }

    #[test]
    fn test_incoming_calls_no_callers() {
        let source = "function unused() {}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider =
            CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // Position at "unused" (line 0, col 9)
        let pos = Position::new(0, 9);
        let calls = provider.incoming_calls(root, pos);

        assert!(
            calls.is_empty(),
            "Uncalled function should have no incoming calls"
        );
    }

    #[test]
    fn test_call_hierarchy_item_serialization() {
        let item = CallHierarchyItem {
            name: "test".to_string(),
            kind: SymbolKind::Function,
            uri: "file:///test.ts".to_string(),
            range: Range::new(Position::new(0, 0), Position::new(1, 0)),
            selection_range: Range::new(Position::new(0, 9), Position::new(0, 13)),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        // SymbolKind::Function serializes as "Function" (serde default for enums)
        assert!(
            json.contains("\"kind\":\"Function\"") || json.contains("\"kind\":12"),
            "kind should serialize correctly, got: {}",
            json
        );

        let deserialized: CallHierarchyItem = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.kind, SymbolKind::Function);
    }
}
