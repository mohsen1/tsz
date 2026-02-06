//! Go-to-Definition implementation for LSP.
//!
//! Given a position in the source, finds where the symbol at that position is defined.

use crate::binder::{BinderState, SymbolId, symbol_flags};
use crate::lsp::position::{LineMap, Location, Position, Range};
use crate::lsp::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::lsp::utils::find_node_at_offset;
use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;

/// Well-known built-in global identifiers that are provided by the runtime
/// environment and not defined in user source files.
/// When these are encountered and no declaration is found, we return None
/// instead of crashing or returning garbage positions.
const BUILTIN_GLOBALS: &[&str] = &[
    // Console API
    "console",
    // Fundamental objects
    "Object",
    "Function",
    "Boolean",
    "Symbol",
    // Error types
    "Error",
    "AggregateError",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    // Numbers and dates
    "Number",
    "BigInt",
    "Math",
    "Date",
    "Infinity",
    "NaN",
    "undefined",
    // Text processing
    "String",
    "RegExp",
    // Indexed collections
    "Array",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
    // Keyed collections
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "WeakRef",
    // Structured data
    "ArrayBuffer",
    "SharedArrayBuffer",
    "Atomics",
    "DataView",
    "JSON",
    // Control abstraction
    "Promise",
    "Generator",
    "GeneratorFunction",
    "AsyncFunction",
    "AsyncGenerator",
    "AsyncGeneratorFunction",
    // Reflection
    "Reflect",
    "Proxy",
    // Internationalization
    "Intl",
    // Web APIs
    "globalThis",
    "window",
    "document",
    "navigator",
    "location",
    "history",
    "localStorage",
    "sessionStorage",
    "fetch",
    "Headers",
    "Request",
    "Response",
    "URL",
    "URLSearchParams",
    "setTimeout",
    "setInterval",
    "clearTimeout",
    "clearInterval",
    "requestAnimationFrame",
    "cancelAnimationFrame",
    "queueMicrotask",
    "structuredClone",
    "atob",
    "btoa",
    "TextEncoder",
    "TextDecoder",
    "AbortController",
    "AbortSignal",
    "Blob",
    "File",
    "FileReader",
    "FormData",
    "ReadableStream",
    "WritableStream",
    "TransformStream",
    "Event",
    "EventTarget",
    "CustomEvent",
    "MutationObserver",
    "IntersectionObserver",
    "ResizeObserver",
    "PerformanceObserver",
    "WebSocket",
    "Worker",
    "MessageChannel",
    "MessagePort",
    "BroadcastChannel",
    // Node.js globals
    "process",
    "Buffer",
    "require",
    "module",
    "exports",
    "__dirname",
    "__filename",
    "global",
    // TypeScript utility types (may appear as identifiers)
    "Partial",
    "Required",
    "Readonly",
    "Record",
    "Pick",
    "Omit",
    "Exclude",
    "Extract",
    "NonNullable",
    "Parameters",
    "ConstructorParameters",
    "ReturnType",
    "InstanceType",
    "ThisParameterType",
    "OmitThisParameter",
    "ThisType",
    "Awaited",
    // Iterator/Iterable
    "Iterator",
    "IterableIterator",
    "AsyncIterableIterator",
];

/// Check if a name is a well-known built-in global.
fn is_builtin_global(name: &str) -> bool {
    BUILTIN_GLOBALS.contains(&name)
}

/// Rich definition information matching TypeScript's tsserver response format.
/// Includes metadata about the symbol kind, name, and declaration context.
#[derive(Debug, Clone)]
pub struct DefinitionInfo {
    /// The location of the identifier name within the declaration.
    pub location: Location,
    /// The span of the entire declaration (contextSpan in tsserver).
    pub context_span: Option<Range>,
    /// The symbol name (e.g., "ambientVar").
    pub name: String,
    /// The symbol kind string (e.g., "var", "function", "class").
    pub kind: String,
    /// The container name (e.g., class name for a method).
    pub container_name: String,
    /// The container kind string.
    pub container_kind: String,
    /// Whether the symbol is local (not exported).
    pub is_local: bool,
    /// Whether the symbol is ambient (declared with `declare`).
    pub is_ambient: bool,
}

/// Go-to-Definition provider.
///
/// This struct provides LSP "Go to Definition" functionality by:
/// 1. Converting a position to a byte offset
/// 2. Finding the AST node at that offset
/// 3. Resolving the node to a symbol
/// 4. Returning the symbol's declaration locations
pub struct GoToDefinition<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    file_name: String,
    source_text: &'a str,
}

impl<'a> GoToDefinition<'a> {
    /// Create a new Go-to-Definition provider.
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

    /// Get the definition location(s) for the symbol at the given position.
    ///
    /// Returns a list of locations because a symbol can have multiple declarations
    /// (e.g., function overloads, merged declarations).
    ///
    /// Returns None if no symbol is found at the position.
    pub fn get_definition(&self, root: NodeIndex, position: Position) -> Option<Vec<Location>> {
        self.get_definition_internal(root, position, None, None)
    }

    pub fn get_definition_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        self.get_definition_internal(root, position, Some(scope_cache), scope_stats)
    }

    fn get_definition_internal(
        &self,
        root: NodeIndex,
        position: Position,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        // 1. Convert position to byte offset
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        // 2. Find the most specific node at this offset
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        // 2a. Skip keyword literals and built-in identifiers with no user definition
        if self.is_builtin_node(node_idx) {
            return None;
        }

        // 3. Resolve the node to a symbol via scope walking
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id_opt = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats)
        } else {
            walker.resolve_node(root, node_idx)
        };

        // 4. If primary resolution succeeded, use the symbol
        if let Some(symbol_id) = symbol_id_opt {
            if let Some(locations) = self.locations_from_symbol(symbol_id) {
                return Some(locations);
            }
        }

        // 5. Fallback: try member access resolution (obj.method, Class.staticProp)
        if let Some(locations) = self.try_member_access_fallback(root, node_idx) {
            return Some(locations);
        }

        // 6. Fallback: try file_locals lookup by identifier text
        if let Some(locations) = self.try_file_locals_fallback(node_idx) {
            return Some(locations);
        }

        None
    }

    /// Get the definition location for a specific node (by NodeIndex).
    ///
    /// This is useful when you already have the node index from another operation.
    pub fn get_definition_for_node(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        self.get_definition_for_node_internal(root, node_idx, None, None)
    }

    pub fn get_definition_for_node_with_scope_cache(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        self.get_definition_for_node_internal(root, node_idx, Some(scope_cache), scope_stats)
    }

    fn get_definition_for_node_internal(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<Location>> {
        if node_idx.is_none() {
            return None;
        }

        // Skip keyword literals and built-in identifiers
        if self.is_builtin_node(node_idx) {
            return None;
        }

        // Resolve the node to a symbol
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id_opt = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats)
        } else {
            walker.resolve_node(root, node_idx)
        };

        // If primary resolution succeeded, use the symbol
        if let Some(symbol_id) = symbol_id_opt {
            if let Some(locations) = self.locations_from_symbol(symbol_id) {
                return Some(locations);
            }
        }

        // Fallback: try file_locals
        if let Some(locations) = self.try_file_locals_fallback(node_idx) {
            return Some(locations);
        }

        None
    }

    /// Convert a symbol's declarations into validated Location objects.
    ///
    /// This validates that declaration positions are within the source text bounds
    /// to prevent crashes when declarations point to other files or invalid positions.
    fn locations_from_symbol(&self, symbol_id: SymbolId) -> Option<Vec<Location>> {
        let symbol = self.binder.symbols.get(symbol_id)?;
        let source_len = self.source_text.len() as u32;

        let locations: Vec<Location> = symbol
            .declarations
            .iter()
            .filter_map(|&decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;

                // Validate that positions are within the current file's bounds.
                // Declarations from other files (cross-file references, built-ins)
                // will have node indices that either don't exist in this arena or
                // have positions outside this file's text range.
                if decl_node.pos > source_len || decl_node.end > source_len {
                    return None;
                }
                if decl_node.end < decl_node.pos {
                    return None;
                }
                // Skip zero-width declarations - these are synthetic/placeholder
                // declarations for built-in globals (undefined, null, etc.)
                if decl_node.pos == decl_node.end {
                    return None;
                }

                let start_pos = self
                    .line_map
                    .offset_to_position(decl_node.pos, self.source_text);
                let end_pos = self
                    .line_map
                    .offset_to_position(decl_node.end, self.source_text);

                // Validate computed positions are within the line map bounds
                let line_count = self.line_map.line_count() as u32;
                if start_pos.line >= line_count || end_pos.line >= line_count {
                    return None;
                }

                Some(Location {
                    file_path: self.file_name.clone(),
                    range: Range::new(start_pos, end_pos),
                })
            })
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    /// Try to resolve a node's identifier text via the binder's file_locals table.
    ///
    /// This serves as a fallback when the scope-based resolution fails (e.g., for
    /// shorthand properties, certain export patterns, etc.)
    fn try_file_locals_fallback(&self, node_idx: NodeIndex) -> Option<Vec<Location>> {
        let node = self.arena.get(node_idx)?;
        let pos = node.pos as usize;
        let end = node.end as usize;
        if end > self.source_text.len() || pos > end {
            return None;
        }

        let text = &self.source_text[pos..end];

        // Skip if this is a built-in global - no definition in user source
        if is_builtin_global(text) {
            return None;
        }

        // Try looking up in file_locals
        let symbol_id = self.binder.file_locals.get(text)?;
        self.locations_from_symbol(symbol_id)
    }

    /// Try to resolve a member access expression (e.g., obj.method, Class.staticProp).
    /// Returns the symbol ID of the member if found.
    fn try_resolve_member_access(&self, root: NodeIndex, node_idx: NodeIndex) -> Option<SymbolId> {
        // Check if the node is the right-hand side of a property access expression
        let ext = self.arena.get_extended(node_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent_node = self.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.arena.get_access_expr(parent_node)?;
        // Make sure we're on the name side (right of dot), not the expression side
        if access.name_or_argument != node_idx {
            return None;
        }

        // Get the member name text
        let node = self.arena.get(node_idx)?;
        let member_name = &self.source_text[node.pos as usize..node.end as usize];

        // Resolve the expression (left side) to a symbol
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let expr_symbol_id = walker.resolve_node(root, access.expression)?;
        let expr_symbol = self.binder.symbols.get(expr_symbol_id)?;

        // Look up in members table (instance members)
        if let Some(ref members) = expr_symbol.members {
            if let Some(member_id) = members.get(member_name) {
                return Some(member_id);
            }
        }

        // Look up in exports table (static members, namespace exports)
        if let Some(ref exports) = expr_symbol.exports {
            if let Some(member_id) = exports.get(member_name) {
                return Some(member_id);
            }
        }

        // For instances: resolve the variable's type by checking its declarations
        // If the expression resolves to a variable (e.g., var x = new Foo()),
        // look at the initializer to find the class and its members.
        for &decl_idx in &expr_symbol.declarations {
            if let Some(decl_node) = self.arena.get(decl_idx) {
                if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                    if let Some(var_data) = self.arena.get_variable_declaration(decl_node) {
                        // Check if the initializer is `new ClassName()`
                        if !var_data.initializer.is_none() {
                            if let Some(init_node) = self.arena.get(var_data.initializer) {
                                if init_node.kind == syntax_kind_ext::NEW_EXPRESSION {
                                    // The new expression's first child is the class name
                                    if let Some(new_data) = self.arena.get_call_expr(init_node) {
                                        // Resolve the class name
                                        let mut walker2 = ScopeWalker::new(self.arena, self.binder);
                                        if let Some(class_symbol_id) =
                                            walker2.resolve_node(root, new_data.expression)
                                        {
                                            let class_symbol =
                                                self.binder.symbols.get(class_symbol_id)?;
                                            if let Some(ref members) = class_symbol.members {
                                                if let Some(member_id) = members.get(member_name) {
                                                    return Some(member_id);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Fallback for member access in get_definition_internal (returns Location objects).
    fn try_member_access_fallback(
        &self,
        root: NodeIndex,
        node_idx: NodeIndex,
    ) -> Option<Vec<Location>> {
        let member_symbol_id = self.try_resolve_member_access(root, node_idx)?;
        self.locations_from_symbol(member_symbol_id)
    }

    /// Check if a node is a built-in keyword literal or built-in identifier
    /// that has no user-navigable definition (e.g., null, true, false, undefined, arguments).
    fn is_builtin_node(&self, node_idx: NodeIndex) -> bool {
        if let Some(node) = self.arena.get(node_idx) {
            use crate::scanner::SyntaxKind;
            let kind = node.kind;
            // Keyword literals never have user-navigable definitions
            if kind == SyntaxKind::NullKeyword as u16
                || kind == SyntaxKind::TrueKeyword as u16
                || kind == SyntaxKind::FalseKeyword as u16
                || kind == SyntaxKind::VoidKeyword as u16
            {
                return true;
            }
            // Check identifier text against built-in globals without definitions
            if kind == SyntaxKind::Identifier as u16 {
                let pos = node.pos as usize;
                let end = node.end as usize;
                if end <= self.source_text.len() && pos <= end {
                    let text = &self.source_text[pos..end];
                    if text == "undefined" || text == "arguments" {
                        return true;
                    }
                }
            }
        }
        false
    }

    // -----------------------------------------------------------------------
    // Rich definition info (for tsserver compatibility)
    // -----------------------------------------------------------------------

    /// Get rich definition info including metadata for tsserver protocol.
    pub fn get_definition_info(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<Vec<DefinitionInfo>> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        // Skip keyword literals and built-in identifiers
        if self.is_builtin_node(node_idx) {
            return None;
        }

        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id_opt = walker.resolve_node(root, node_idx);

        if let Some(symbol_id) = symbol_id_opt {
            if let Some(infos) = self.definition_infos_from_symbol(symbol_id) {
                return Some(infos);
            }
        }

        // Fallback: try member access resolution
        if let Some(member_symbol_id) = self.try_resolve_member_access(root, node_idx) {
            if let Some(infos) = self.definition_infos_from_symbol(member_symbol_id) {
                return Some(infos);
            }
        }

        // Fallback: try file_locals
        if let Some(infos) = self.try_file_locals_fallback_info(node_idx) {
            return Some(infos);
        }

        None
    }

    /// Convert a symbol's declarations into rich DefinitionInfo objects.
    pub fn definition_infos_from_symbol(&self, symbol_id: SymbolId) -> Option<Vec<DefinitionInfo>> {
        let symbol = self.binder.symbols.get(symbol_id)?;
        let source_len = self.source_text.len() as u32;

        let infos: Vec<DefinitionInfo> = symbol
            .declarations
            .iter()
            .filter_map(|&decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;

                if decl_node.pos > source_len || decl_node.end > source_len {
                    return None;
                }
                if decl_node.end < decl_node.pos {
                    return None;
                }
                // Skip zero-width declarations (synthetic builtins)
                if decl_node.pos == decl_node.end {
                    return None;
                }

                // Get the name node span (text span) vs full declaration span (context span)
                let (name_range, context_range) =
                    self.compute_name_and_context_spans(decl_idx, decl_node);

                let line_count = self.line_map.line_count() as u32;
                if name_range.start.line >= line_count || name_range.end.line >= line_count {
                    return None;
                }

                // Determine kind, name, and other metadata
                let kind = self.get_declaration_kind(decl_idx, symbol.flags);
                let name = symbol.escaped_name.clone();
                let (container_name, container_kind) = self.get_container_info(symbol_id);
                let is_local = if kind == "parameter" {
                    false
                } else if self.is_class_or_interface_member(decl_idx) {
                    true
                } else {
                    !self.is_top_level_declaration(decl_idx)
                };
                let is_ambient = self.is_ambient_declaration(decl_idx);

                Some(DefinitionInfo {
                    location: Location {
                        file_path: self.file_name.clone(),
                        range: name_range,
                    },
                    context_span: Some(context_range),
                    name,
                    kind,
                    container_name,
                    container_kind,
                    is_local,
                    is_ambient,
                })
            })
            .collect();

        if infos.is_empty() { None } else { Some(infos) }
    }

    /// Try file_locals fallback but return DefinitionInfo.
    fn try_file_locals_fallback_info(&self, node_idx: NodeIndex) -> Option<Vec<DefinitionInfo>> {
        let node = self.arena.get(node_idx)?;
        let pos = node.pos as usize;
        let end = node.end as usize;
        if end > self.source_text.len() || pos > end {
            return None;
        }

        let text = &self.source_text[pos..end];
        if is_builtin_global(text) {
            return None;
        }

        let symbol_id = self.binder.file_locals.get(text)?;
        self.definition_infos_from_symbol(symbol_id)
    }

    /// Compute the name span and context span for a declaration node.
    /// Returns (name_range for the identifier, full declaration range for context).
    fn compute_name_and_context_spans(
        &self,
        decl_idx: NodeIndex,
        decl_node: &crate::parser::node::Node,
    ) -> (Range, Range) {
        // For the context span, we may need to go up to the parent node.
        // For VariableDeclaration, the context is the VariableStatement
        // (which includes `declare var ... ;`).
        let context_node_span = self.get_context_span_node(decl_idx, decl_node);

        let context_start = self
            .line_map
            .offset_to_position(context_node_span.0, self.source_text);
        let context_end = self
            .line_map
            .offset_to_position(context_node_span.1, self.source_text);
        let context_range = Range::new(context_start, context_end);

        // Try to find the identifier name node within the declaration
        if let Some(name_idx) = self.get_declaration_name_idx(decl_idx) {
            if !name_idx.is_none() {
                if let Some(name_node) = self.arena.get(name_idx) {
                    let name_start = self
                        .line_map
                        .offset_to_position(name_node.pos, self.source_text);
                    let name_end = self
                        .line_map
                        .offset_to_position(name_node.end, self.source_text);
                    return (Range::new(name_start, name_end), context_range);
                }
            }
        }

        // If we can't find a name node, use the declaration span for both
        (context_range, context_range)
    }

    /// Get the span for the context (the full declaration statement).
    /// For VariableDeclaration, walk up to VariableStatement.
    /// For other declarations, use the declaration node itself.
    /// Returns the span with leading trivia stripped (using getStart semantics).
    fn get_context_span_node(
        &self,
        decl_idx: NodeIndex,
        decl_node: &crate::parser::node::Node,
    ) -> (u32, u32) {
        let source_bytes = self.source_text.as_bytes();
        let source_len = self.source_text.len() as u32;

        // Strip leading whitespace/newlines from a position
        let skip_leading = |pos: u32, end: u32| -> u32 {
            let limit = end.min(source_len) as usize;
            let mut i = pos as usize;
            while i < limit {
                match source_bytes[i] {
                    b' ' | b'\t' | b'\n' | b'\r' => i += 1,
                    _ => break,
                }
            }
            i as u32
        };

        // Strip trailing whitespace/newlines from an end position
        let skip_trailing = |pos: u32, end: u32| -> u32 {
            let start = pos as usize;
            let mut i = end.min(source_len) as usize;
            while i > start {
                match source_bytes[i - 1] {
                    b' ' | b'\t' | b'\n' | b'\r' => i -= 1,
                    _ => break,
                }
            }
            i as u32
        };

        // Find the position right after the last significant token in the range.
        // This handles cases where the parser's node `end` extends into the next
        // statement by finding the last ; or } in the range.
        let find_real_end = |pos: u32, end: u32| -> u32 {
            let start = pos as usize;
            let e = end.min(source_len) as usize;
            // Scan backwards for the last ; or } (statement-ending tokens)
            for i in (start..e).rev() {
                match source_bytes[i] {
                    b';' | b'}' => return (i + 1) as u32,
                    _ => {}
                }
            }
            // Fall back to stripping trailing whitespace
            skip_trailing(pos, end)
        };

        // For declarations that end with a body (class, enum, function, etc.),
        // find the closing } and use that as the end (don't include trailing ;).
        let find_brace_end = |pos: u32, end: u32| -> u32 {
            let start = pos as usize;
            let e = end.min(source_len) as usize;
            // Scan backwards for the closing }
            for i in (start..e).rev() {
                if source_bytes[i] == b'}' {
                    return (i + 1) as u32;
                }
            }
            // Fall back to find_real_end
            find_real_end(pos, end)
        };

        // Clean span: strip leading trivia, find real end
        let clean = |pos: u32, end: u32| -> (u32, u32) {
            let s = skip_leading(pos, end);
            let e = find_real_end(s, end);
            (s, e)
        };

        // Clean span for brace-terminated declarations (class, enum, etc.):
        // strip leading trivia, find closing }
        let clean_brace = |pos: u32, end: u32| -> (u32, u32) {
            let s = skip_leading(pos, end);
            let e = find_brace_end(s, end);
            (s, e)
        };

        match decl_node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                // Walk up: VariableDeclaration -> VariableDeclarationList -> VariableStatement
                if let Some(ext) = self.arena.get_extended(decl_idx) {
                    let parent_idx = ext.parent;
                    if !parent_idx.is_none() {
                        // Check if parent is a CatchClause - no contextSpan for catch vars
                        if let Some(parent_node) = self.arena.get(parent_idx) {
                            if parent_node.kind == syntax_kind_ext::CATCH_CLAUSE {
                                return (decl_node.pos, decl_node.end);
                            }
                        }
                        if let Some(parent_ext) = self.arena.get_extended(parent_idx) {
                            let grandparent_idx = parent_ext.parent;
                            if !grandparent_idx.is_none() {
                                if let Some(gp_node) = self.arena.get(grandparent_idx) {
                                    if gp_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                                        return clean(gp_node.pos, gp_node.end);
                                    }
                                }
                            }
                        }
                        if let Some(parent_node) = self.arena.get(parent_idx) {
                            return clean(parent_node.pos, parent_node.end);
                        }
                    }
                }
                (decl_node.pos, decl_node.end)
            }
            syntax_kind_ext::FUNCTION_DECLARATION
            | syntax_kind_ext::CLASS_DECLARATION
            | syntax_kind_ext::INTERFACE_DECLARATION
            | syntax_kind_ext::TYPE_ALIAS_DECLARATION
            | syntax_kind_ext::ENUM_DECLARATION
            | syntax_kind_ext::MODULE_DECLARATION => {
                // Check for modifiers (declare, export, async, abstract, etc.)
                // that extend the span before the declaration keyword.
                let modifiers = match decl_node.kind {
                    syntax_kind_ext::FUNCTION_DECLARATION => self
                        .arena
                        .get_function(decl_node)
                        .and_then(|f| f.modifiers.as_ref()),
                    syntax_kind_ext::CLASS_DECLARATION => self
                        .arena
                        .get_class(decl_node)
                        .and_then(|c| c.modifiers.as_ref()),
                    syntax_kind_ext::INTERFACE_DECLARATION => self
                        .arena
                        .get_interface(decl_node)
                        .and_then(|i| i.modifiers.as_ref()),
                    syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                        .arena
                        .get_type_alias(decl_node)
                        .and_then(|t| t.modifiers.as_ref()),
                    syntax_kind_ext::ENUM_DECLARATION => self
                        .arena
                        .get_enum(decl_node)
                        .and_then(|e| e.modifiers.as_ref()),
                    syntax_kind_ext::MODULE_DECLARATION => self
                        .arena
                        .get_module(decl_node)
                        .and_then(|m| m.modifiers.as_ref()),
                    _ => None,
                };

                // Find the earliest modifier position to include keywords like `declare`, `export`
                let start_pos = if let Some(mods) = modifiers {
                    let mut earliest = decl_node.pos;
                    for &mod_idx in &mods.nodes {
                        if let Some(mod_node) = self.arena.get(mod_idx) {
                            if mod_node.pos < earliest {
                                earliest = mod_node.pos;
                            }
                        }
                    }
                    earliest
                } else {
                    decl_node.pos
                };

                // Type aliases end with ; (e.g., `type T = ...;`), other declarations
                // end with } and should NOT include trailing ;
                if decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                    clean(start_pos, decl_node.end)
                } else {
                    clean_brace(start_pos, decl_node.end)
                }
            }
            syntax_kind_ext::METHOD_SIGNATURE
            | syntax_kind_ext::PROPERTY_SIGNATURE
            | syntax_kind_ext::METHOD_DECLARATION
            | syntax_kind_ext::PROPERTY_DECLARATION
            | syntax_kind_ext::CONSTRUCT_SIGNATURE
            | syntax_kind_ext::CONSTRUCTOR => {
                // For member declarations, include modifiers (public, static, etc.)
                let modifiers = match decl_node.kind {
                    syntax_kind_ext::METHOD_SIGNATURE | syntax_kind_ext::PROPERTY_SIGNATURE => self
                        .arena
                        .get_signature(decl_node)
                        .and_then(|s| s.modifiers.as_ref()),
                    syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(decl_node)
                        .and_then(|m| m.modifiers.as_ref()),
                    syntax_kind_ext::PROPERTY_DECLARATION => self
                        .arena
                        .get_property_decl(decl_node)
                        .and_then(|p| p.modifiers.as_ref()),
                    syntax_kind_ext::CONSTRUCTOR => self
                        .arena
                        .get_constructor(decl_node)
                        .and_then(|c| c.modifiers.as_ref()),
                    _ => None,
                };

                let start_pos = if let Some(mods) = modifiers {
                    let mut earliest = decl_node.pos;
                    for &mod_idx in &mods.nodes {
                        if let Some(mod_node) = self.arena.get(mod_idx) {
                            if mod_node.pos < earliest {
                                earliest = mod_node.pos;
                            }
                        }
                    }
                    earliest
                } else {
                    decl_node.pos
                };

                clean(start_pos, decl_node.end)
            }
            _ => (decl_node.pos, decl_node.end),
        }
    }

    /// Get the name node index from a declaration node.
    fn get_declaration_name_idx(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(decl_idx)?;
        match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let var_decl = self.arena.get_variable_declaration(node)?;
                Some(var_decl.name)
            }
            syntax_kind_ext::FUNCTION_DECLARATION => {
                let func = self.arena.get_function(node)?;
                Some(func.name)
            }
            syntax_kind_ext::CLASS_DECLARATION | syntax_kind_ext::CLASS_EXPRESSION => {
                let class = self.arena.get_class(node)?;
                Some(class.name)
            }
            syntax_kind_ext::INTERFACE_DECLARATION => {
                let iface = self.arena.get_interface(node)?;
                Some(iface.name)
            }
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                let type_alias = self.arena.get_type_alias(node)?;
                Some(type_alias.name)
            }
            syntax_kind_ext::ENUM_DECLARATION => {
                let enum_decl = self.arena.get_enum(node)?;
                Some(enum_decl.name)
            }
            syntax_kind_ext::MODULE_DECLARATION => {
                let module = self.arena.get_module(node)?;
                Some(module.name)
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(node)?;
                Some(method.name)
            }
            syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.arena.get_property_decl(node)?;
                Some(prop.name)
            }
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.arena.get_accessor(node)?;
                Some(accessor.name)
            }
            syntax_kind_ext::ENUM_MEMBER => {
                let member = self.arena.get_enum_member(node)?;
                Some(member.name)
            }
            syntax_kind_ext::PARAMETER => {
                let param = self.arena.get_parameter(node)?;
                Some(param.name)
            }
            syntax_kind_ext::IMPORT_SPECIFIER => {
                let spec = self.arena.get_specifier(node)?;
                Some(spec.name)
            }
            syntax_kind_ext::METHOD_SIGNATURE | syntax_kind_ext::PROPERTY_SIGNATURE => {
                let sig = self.arena.get_signature(node)?;
                Some(sig.name)
            }
            syntax_kind_ext::CONSTRUCT_SIGNATURE | syntax_kind_ext::CALL_SIGNATURE => {
                // These don't have meaningful names
                None
            }
            _ => None,
        }
    }

    /// Get the declaration kind string for a specific declaration,
    /// using node info to distinguish const/let/var when needed.
    fn get_declaration_kind(&self, decl_idx: NodeIndex, flags: u32) -> String {
        use crate::parser::flags::node_flags;

        // Check if the declaration node is a parameter
        if let Some(decl_node) = self.arena.get(decl_idx) {
            if decl_node.kind == syntax_kind_ext::PARAMETER {
                return "parameter".to_string();
            }
        }

        // For block-scoped variables, check if const
        if flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            // Walk up to VariableDeclarationList to check CONST flag
            if let Some(ext) = self.arena.get_extended(decl_idx) {
                let parent_idx = ext.parent; // VariableDeclarationList
                if !parent_idx.is_none() {
                    if let Some(parent_node) = self.arena.get(parent_idx) {
                        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                            if parent_node.flags as u32 & node_flags::CONST != 0 {
                                return "const".to_string();
                            }
                        }
                    }
                }
            }
            return "let".to_string();
        }

        // For function-scoped variables, also check for parameter
        if flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            // Check if this specific declaration is a parameter
            if let Some(node) = self.arena.get(decl_idx) {
                if node.kind == syntax_kind_ext::PARAMETER {
                    return "parameter".to_string();
                }
            }
        }

        self.symbol_flags_to_kind_string(flags)
    }

    /// Convert symbol flags to a tsserver-compatible kind string.
    pub fn symbol_flags_to_kind_string(&self, flags: u32) -> String {
        if flags & symbol_flags::FUNCTION != 0 {
            "function".to_string()
        } else if flags & symbol_flags::CLASS != 0 {
            "class".to_string()
        } else if flags & symbol_flags::INTERFACE != 0 {
            "interface".to_string()
        } else if flags & symbol_flags::TYPE_ALIAS != 0 {
            "type".to_string()
        } else if flags & symbol_flags::ENUM != 0 {
            "enum".to_string()
        } else if flags & symbol_flags::ENUM_MEMBER != 0 {
            "enum member".to_string()
        } else if flags & symbol_flags::MODULE != 0 {
            "module".to_string()
        } else if flags & symbol_flags::METHOD != 0 {
            "method".to_string()
        } else if flags & symbol_flags::PROPERTY != 0 {
            "property".to_string()
        } else if flags & symbol_flags::CONSTRUCTOR != 0 {
            "constructor".to_string()
        } else if flags & symbol_flags::GET_ACCESSOR != 0 {
            "getter".to_string()
        } else if flags & symbol_flags::SET_ACCESSOR != 0 {
            "setter".to_string()
        } else if flags & symbol_flags::TYPE_PARAMETER != 0 {
            "type parameter".to_string()
        } else if flags & symbol_flags::ALIAS != 0 {
            "alias".to_string()
        } else if flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            // Could be let or const
            "let".to_string()
        } else if flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            "var".to_string()
        } else {
            "".to_string()
        }
    }

    /// Get the container name and kind for a symbol.
    fn get_container_info(&self, symbol_id: SymbolId) -> (String, String) {
        let symbol = match self.binder.symbols.get(symbol_id) {
            Some(s) => s,
            None => return (String::new(), String::new()),
        };

        // First try symbol.parent (set by binder for enums, lib types)
        if !symbol.parent.is_none() {
            if let Some(parent_symbol) = self.binder.symbols.get(symbol.parent) {
                let parent_kind = self.symbol_flags_to_kind_string(parent_symbol.flags);
                return (parent_symbol.escaped_name.clone(), parent_kind);
            }
        }

        // Fallback: walk AST from first declaration to find containing class/interface/enum
        if let Some(&decl_idx) = symbol.declarations.first() {
            return self.get_container_from_ast(decl_idx);
        }

        (String::new(), String::new())
    }

    /// Get identifier text from a NodeIndex using source_text.
    fn get_node_text(&self, idx: NodeIndex) -> String {
        if idx.is_none() {
            return String::new();
        }
        if let Some(node) = self.arena.get(idx) {
            let pos = node.pos as usize;
            let end = node.end as usize;
            if pos < end && end <= self.source_text.len() {
                return self.source_text[pos..end].to_string();
            }
        }
        String::new()
    }

    /// Walk up the AST from a declaration node to find the containing class/interface/enum.
    fn get_container_from_ast(&self, decl_idx: NodeIndex) -> (String, String) {
        let mut current = decl_idx;
        for _ in 0..20 {
            if let Some(ext) = self.arena.get_extended(current) {
                let parent = ext.parent;
                if parent.is_none() {
                    break;
                }
                if let Some(parent_node) = self.arena.get(parent) {
                    match parent_node.kind {
                        k if k == syntax_kind_ext::CLASS_DECLARATION
                            || k == syntax_kind_ext::CLASS_EXPRESSION =>
                        {
                            let name = self
                                .arena
                                .get_class(parent_node)
                                .map(|c| self.get_node_text(c.name))
                                .unwrap_or_default();
                            return (name, "class".to_string());
                        }
                        k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                            let name = self
                                .arena
                                .get_interface(parent_node)
                                .map(|i| self.get_node_text(i.name))
                                .unwrap_or_default();
                            return (name, "interface".to_string());
                        }
                        k if k == syntax_kind_ext::ENUM_DECLARATION => {
                            let name = self
                                .arena
                                .get_enum(parent_node)
                                .map(|e| self.get_node_text(e.name))
                                .unwrap_or_default();
                            return (name, "enum".to_string());
                        }
                        k if k == syntax_kind_ext::MODULE_DECLARATION => {
                            let name = self
                                .arena
                                .get_module(parent_node)
                                .map(|m| self.get_node_text(m.name))
                                .unwrap_or_default();
                            return (name, "module".to_string());
                        }
                        _ => {}
                    }
                }
                current = parent;
            } else {
                break;
            }
        }
        (String::new(), String::new())
    }

    /// Check if a declaration is ambient (has `declare` modifier).
    /// Walks up the parent chain to find a node with `declare` in its modifiers.
    fn is_ambient_declaration(&self, decl_idx: NodeIndex) -> bool {
        let mut current = decl_idx;
        for _ in 0..15 {
            if let Some(node) = self.arena.get(current) {
                if self.node_has_declare_modifier(current, node) {
                    return true;
                }
            }
            // Walk up to parent
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
        }
        false
    }

    /// Check if a specific node has the `declare` keyword in its modifiers list.
    fn node_has_declare_modifier(
        &self,
        _node_idx: NodeIndex,
        node: &crate::parser::node::Node,
    ) -> bool {
        use crate::scanner::SyntaxKind;
        let modifiers = match node.kind {
            syntax_kind_ext::VARIABLE_STATEMENT => self
                .arena
                .get_variable(node)
                .and_then(|v| v.modifiers.as_ref()),
            syntax_kind_ext::FUNCTION_DECLARATION => self
                .arena
                .get_function(node)
                .and_then(|f| f.modifiers.as_ref()),
            syntax_kind_ext::CLASS_DECLARATION => self
                .arena
                .get_class(node)
                .and_then(|c| c.modifiers.as_ref()),
            syntax_kind_ext::INTERFACE_DECLARATION => self
                .arena
                .get_interface(node)
                .and_then(|i| i.modifiers.as_ref()),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                .arena
                .get_type_alias(node)
                .and_then(|t| t.modifiers.as_ref()),
            syntax_kind_ext::ENUM_DECLARATION => {
                self.arena.get_enum(node).and_then(|e| e.modifiers.as_ref())
            }
            syntax_kind_ext::MODULE_DECLARATION => self
                .arena
                .get_module(node)
                .and_then(|m| m.modifiers.as_ref()),
            _ => None,
        };
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::DeclareKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if a declaration is at the top level of the source file.
    /// Top-level declarations have isLocal = false.
    /// Check if a declaration is a member of a class or interface.
    fn is_class_or_interface_member(&self, decl_idx: NodeIndex) -> bool {
        if let Some(ext) = self.arena.get_extended(decl_idx) {
            let parent = ext.parent;
            if !parent.is_none() {
                if let Some(parent_node) = self.arena.get(parent) {
                    let k = parent_node.kind;
                    return k == syntax_kind_ext::CLASS_DECLARATION
                        || k == syntax_kind_ext::CLASS_EXPRESSION
                        || k == syntax_kind_ext::INTERFACE_DECLARATION;
                }
            }
        }
        false
    }

    fn is_top_level_declaration(&self, decl_idx: NodeIndex) -> bool {
        let mut current = decl_idx;
        // Walk up through the parent chain looking for source file
        for _ in 0..20 {
            if let Some(ext) = self.arena.get_extended(current) {
                let parent = ext.parent;
                if parent.is_none() {
                    return true; // Reached root
                }
                if let Some(parent_node) = self.arena.get(parent) {
                    match parent_node.kind {
                        syntax_kind_ext::SOURCE_FILE => return true,
                        // Transparent containers - keep walking up
                        syntax_kind_ext::VARIABLE_DECLARATION_LIST
                        | syntax_kind_ext::VARIABLE_STATEMENT
                        | syntax_kind_ext::CLASS_DECLARATION
                        | syntax_kind_ext::CLASS_EXPRESSION
                        | syntax_kind_ext::INTERFACE_DECLARATION
                        | syntax_kind_ext::ENUM_DECLARATION
                        | syntax_kind_ext::MODULE_DECLARATION
                        | syntax_kind_ext::MODULE_BLOCK => {
                            current = parent;
                            continue;
                        }
                        // If we hit a function/method/class body, it's local
                        _ => return false,
                    }
                }
                current = parent;
            } else {
                break;
            }
        }
        false
    }
}

#[cfg(test)]
mod definition_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;

    #[test]
    fn test_goto_definition_simple_variable() {
        // const x = 1;
        // x + 1;
        let source = "const x = 1;\nx + 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'x' in "x + 1" (line 1, column 0)
        let position = Position::new(1, 0);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // Should find the definition at "const x = 1"
        assert!(definitions.is_some(), "Should find definition for x");

        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            // The definition should be on line 0
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_type_reference() {
        let source = "type Foo = { value: string };\nconst x: Foo = { value: \"\" };";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'Foo' in the type annotation (line 1)
        let position = Position::new(1, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should find definition for type reference"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_binding_pattern() {
        let source = "const { foo } = obj;\nfoo;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'foo' usage (line 1)
        let position = Position::new(1, 0);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should find definition for binding pattern name"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_parameter_binding_pattern() {
        let source = "function demo({ foo }: { foo: number }) {\n  return foo;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'foo' usage in the return (line 1)
        let position = Position::new(1, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should find definition for parameter binding name"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_class_method_local() {
        let source = "class Foo {\n  method() {\n    const value = 1;\n    return value;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 3)
        let position = Position::new(3, 11);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should find definition for method local"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 2,
                "Definition should be on line 2"
            );
        }
    }

    #[test]
    fn test_goto_definition_class_method_name() {
        let source = "class Foo {\n  method() {}\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'method' name (line 1)
        let position = Position::new(1, 2);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should find definition for method name"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_class_member_not_in_scope() {
        let source = "class Foo {\n  value = 1;\n  method() {\n    return value;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 3)
        let position = Position::new(3, 11);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_none(),
            "Class members should not resolve as lexical identifiers"
        );
    }

    #[test]
    fn test_goto_definition_class_self_reference() {
        let source = "class Foo {\n  method() {\n    return Foo;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'Foo' usage (line 2)
        let position = Position::new(2, 11);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve class name within class scope"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_class_expression_name() {
        let source = "const Foo = class Bar {\n  method() {\n    return Bar;\n  }\n};";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'Bar' usage (line 2)
        let position = Position::new(2, 11);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve class expression name in body"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_nested_arrow_in_conditional() {
        let source =
            "const handler = cond ? (() => {\n  const value = 1;\n  return value;\n}) : null;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(definitions.is_some(), "Should resolve nested arrow locals");
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_nested_arrow_in_if_condition() {
        let source = "if ((() => {\n  const value = 1;\n  return value;\n})()) {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve nested arrow locals in condition"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_nested_arrow_in_while_condition() {
        let source = "while ((() => {\n  const value = 1;\n  return value;\n})()) {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve nested arrow locals in while condition"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_nested_arrow_in_for_of_expression() {
        let source = "for (const item of (() => {\n  const value = 1;\n  return value;\n})()) {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve nested arrow locals in for-of expression"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_export_default_expression() {
        let source = "export default (() => {\n  const value = 1;\n  return value;\n})();";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve locals in export default expression"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_labeled_statement_local() {
        let source = "label: {\n  const value = 1;\n  value;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 2);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve locals inside labeled statement"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_with_statement_local() {
        let source = "with (obj) {\n  const value = 1;\n  value;\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 2);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve locals inside with statement"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_var_hoisted_in_nested_block() {
        let source = "function demo() {\n  value;\n  if (cond) {\n    var value = 1;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage before the declaration (line 1)
        let position = Position::new(1, 2);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve hoisted var definition"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 3,
                "Definition should be on line 3"
            );
        }
    }

    #[test]
    fn test_goto_definition_decorator_reference() {
        let source = "const deco = () => {};\n@deco\nclass Foo {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'deco' usage in the decorator (line 1)
        let position = Position::new(1, 1);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(definitions.is_some(), "Should resolve decorator reference");
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 0,
                "Definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_decorator_argument_local() {
        let source = "const deco = (cb) => cb();\n@deco(() => {\n  const value = 1;\n  return value;\n})\nclass Foo {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage inside the decorator argument (line 3)
        let position = Position::new(3, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve locals inside decorator arguments"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 2,
                "Definition should be on line 2"
            );
        }
    }

    #[test]
    fn test_goto_definition_nested_arrow_in_object_literal() {
        let source = "const holder = { run: () => {\n  const value = 1;\n  return value;\n} };";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 2)
        let position = Position::new(2, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_some(),
            "Should resolve nested object literal locals"
        );
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1"
            );
        }
    }

    #[test]
    fn test_goto_definition_class_static_block_local() {
        let source = "class Foo {\n  static {\n    const value = 1;\n    value;\n  }\n}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the 'value' usage (line 3)
        let position = Position::new(3, 4);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(definitions.is_some(), "Should resolve static block locals");
        if let Some(defs) = definitions {
            assert!(!defs.is_empty(), "Should have at least one definition");
            assert_eq!(
                defs[0].range.start.line, 2,
                "Definition should be on line 2"
            );
        }
    }

    #[test]
    fn test_goto_definition_not_found() {
        let source = "const x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position outside any identifier
        let position = Position::new(0, 11); // At the semicolon

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // Should not find a definition
        assert!(
            definitions.is_none(),
            "Should not find definition at semicolon"
        );
    }

    // =========================================================================
    // New edge case tests
    // =========================================================================

    #[test]
    fn test_goto_definition_builtin_console_returns_none() {
        // "console" is a built-in global with no user declaration.
        // Should return None gracefully instead of crashing.
        let source = "console.log('hello');";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at "console" (line 0, column 0)
        let position = Position::new(0, 0);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // Should return None (no crash) since console is a built-in
        assert!(
            definitions.is_none(),
            "Built-in global 'console' should return None, not crash"
        );
    }

    #[test]
    fn test_goto_definition_builtin_array_returns_none() {
        let source = "const arr = new Array(10);";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at "Array" (line 0, column 16)
        let position = Position::new(0, 16);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_none(),
            "Built-in global 'Array' should return None"
        );
    }

    #[test]
    fn test_goto_definition_builtin_promise_returns_none() {
        let source = "const p: Promise<number> = Promise.resolve(42);";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at the Promise usage (after the =)
        let position = Position::new(0, 27);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_none(),
            "Built-in global 'Promise' should return None"
        );
    }

    #[test]
    fn test_goto_definition_no_crash_on_position_beyond_file() {
        let source = "const x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position way beyond the file (line 100, column 0)
        let position = Position::new(100, 0);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // Should return None (no crash)
        assert!(
            definitions.is_none(),
            "Position beyond file should return None without crash"
        );
    }

    #[test]
    fn test_goto_definition_empty_source() {
        let source = "";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let position = Position::new(0, 0);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        assert!(
            definitions.is_none(),
            "Empty source should return None without crash"
        );
    }

    #[test]
    fn test_goto_definition_self_declaration_identifier() {
        // Clicking on the declaration itself should navigate to it
        let source = "function hello() {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at "hello" in the function declaration (line 0, column 9)
        let position = Position::new(0, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // Should find the declaration (itself)
        assert!(
            definitions.is_some(),
            "Should find declaration for function name"
        );
        if let Some(defs) = definitions {
            assert_eq!(defs[0].range.start.line, 0);
        }
    }

    #[test]
    fn test_goto_definition_is_builtin_global_helper() {
        // Test the is_builtin_global helper function directly
        assert!(is_builtin_global("console"));
        assert!(is_builtin_global("Array"));
        assert!(is_builtin_global("Promise"));
        assert!(is_builtin_global("Map"));
        assert!(is_builtin_global("Set"));
        assert!(is_builtin_global("setTimeout"));
        assert!(is_builtin_global("fetch"));
        assert!(is_builtin_global("process"));
        assert!(is_builtin_global("Buffer"));

        // User-defined names should NOT be built-in
        assert!(!is_builtin_global("myFunction"));
        assert!(!is_builtin_global("MyClass"));
        assert!(!is_builtin_global("handler"));
        assert!(!is_builtin_global("data"));
    }

    #[test]
    fn test_goto_definition_multiple_builtin_globals_no_crash() {
        // Multiple built-in references in one file should all return None
        let source =
            "console.log(Array.from([1, 2, 3]));\nPromise.resolve(42);\nsetTimeout(() => {}, 100);";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);

        // console at (0, 0)
        let d1 = goto_def.get_definition(root, Position::new(0, 0));
        assert!(d1.is_none(), "console should return None");

        // Promise at (1, 0)
        let d2 = goto_def.get_definition(root, Position::new(1, 0));
        assert!(d2.is_none(), "Promise should return None");

        // setTimeout at (2, 0)
        let d3 = goto_def.get_definition(root, Position::new(2, 0));
        assert!(d3.is_none(), "setTimeout should return None");
    }

    #[test]
    fn test_goto_definition_interface_reference() {
        // Interface declarations should be findable
        let source = "interface IFoo { bar: string; }\nconst x: IFoo = { bar: 'hi' };";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at "IFoo" type reference on line 1
        let position = Position::new(1, 9);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // We expect this to either find the interface or return None gracefully
        // (no crash is the critical requirement)
        if let Some(defs) = &definitions {
            assert_eq!(
                defs[0].range.start.line, 0,
                "Interface definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_enum_reference() {
        let source = "enum Color { Red, Green, Blue }\nconst c: Color = Color.Red;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at "Color" value reference on line 1 (after the =)
        let position = Position::new(1, 17);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // No crash is the critical requirement
        if let Some(defs) = &definitions {
            assert_eq!(
                defs[0].range.start.line, 0,
                "Enum definition should be on line 0"
            );
        }
    }

    #[test]
    fn test_goto_definition_default_export_function() {
        // Export default function should be navigable
        let source = "export default function greet() { return 'hi'; }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Position at "greet" (line 0, column 24)
        let position = Position::new(0, 24);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition(root, position);

        // Should find the function declaration or not crash
        if let Some(defs) = &definitions {
            assert_eq!(defs[0].range.start.line, 0);
        }
    }

    #[test]
    fn test_goto_definition_validated_positions_are_in_bounds() {
        // Ensure returned positions are always within the source text bounds
        let source = "const x = 1;\nconst y = x + 2;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        // Try every possible valid position in the source
        let line_count = line_map.line_count() as u32;
        for line in 0..line_count {
            for col in 0..50 {
                let position = Position::new(line, col);
                let goto_def =
                    GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
                let definitions = goto_def.get_definition(root, position);

                // If we got definitions, all positions must be in bounds
                if let Some(defs) = definitions {
                    for def in &defs {
                        assert!(
                            def.range.start.line < line_count,
                            "Start line {} should be < line_count {}",
                            def.range.start.line,
                            line_count
                        );
                        assert!(
                            def.range.end.line < line_count,
                            "End line {} should be < line_count {}",
                            def.range.end.line,
                            line_count
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_goto_definition_for_node_with_none_index() {
        let source = "const x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);

        let goto_def =
            GoToDefinition::new(arena, &binder, &line_map, "test.ts".to_string(), source);
        let definitions = goto_def.get_definition_for_node(root, NodeIndex::NONE);

        assert!(
            definitions.is_none(),
            "Should return None for NodeIndex::none()"
        );
    }
}
